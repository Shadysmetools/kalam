//! The supervisor: owns the corpus, mutates, feeds worker processes, records crashes, keeps
//! `status.json`.
//!
//! Each job is one thread here driving one `kalam-fuzz --worker` child over pipes. The
//! supervisor writes an input, then waits for the reply with a timeout:
//! - a reply is an ordinary result, a caught panic, or an oracle violation;
//! - a closed pipe means the worker died (stack overflow, abort): the input just sent is the
//!   crash, the worker's stderr tail and exit code classify it, and a new worker starts;
//! - no reply in time is a hang: the worker is killed and the input recorded as `timeout`.
//!
//! New crash inputs are minimized (same signature), saved to `crashes/`, and re-run in a
//! fresh `kalam-fuzz --repro` process to confirm they reproduce outside the fuzzer.

use std::collections::{HashSet, VecDeque};
use std::io::{self, BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kalam_base::Skin;
use kalam_props::rng::{splitmix, Rng};

use crate::corpus;
use crate::mutate::{mutate, Input};
use crate::proto::{read_reply, write_request, Reply};
use crate::status::{CrashRecord, OracleRecord, Status};
use crate::target::fnv;
use crate::util::{iso8601, target_version, unix_now};

#[derive(Clone, Debug)]
pub struct Options {
    pub duration: Duration,
    pub jobs: usize,
    pub seed: u64,
    pub out: PathBuf,
    pub corpus_dirs: Vec<PathBuf>,
    pub timeout: Duration,
    pub max_execs: Option<u64>,
    pub replay: bool,
    pub generated: u64,
    pub quiet: bool,
    /// Mutate (false: run the seeds once and stop; used by the self-tests).
    pub mutate: bool,
}

impl Options {
    pub fn defaults() -> Options {
        let root = crate::util::repo_root();
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Options {
            duration: Duration::from_secs(3600),
            jobs: (cores / 4).max(1),
            seed: unix_now(),
            out: root.join("eval/fuzz"),
            corpus_dirs: vec![root.join("eval/corpus"), root.join("eval/suites")],
            timeout: Duration::from_secs(10),
            max_execs: None,
            replay: true,
            generated: 150,
            quiet: false,
            mutate: true,
        }
    }
}

// ----- one worker process ----------------------------------------------------------------

struct WorkerProc {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    rx: Receiver<io::Result<Reply>>,
    stderr: Arc<Mutex<VecDeque<u8>>>,
}

const STDERR_KEEP: usize = 4096;

impl WorkerProc {
    fn spawn(exe: &Path) -> io::Result<WorkerProc> {
        let mut child = Command::new(exe)
            .arg("--worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = BufWriter::new(child.stdin.take().expect("piped stdin"));
        let stdout = child.stdout.take().expect("piped stdout");
        let mut err = child.stderr.take().expect("piped stderr");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            loop {
                let reply = read_reply(&mut r);
                let end = reply.is_err();
                if tx.send(reply).is_err() || end {
                    return;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let keep = Arc::clone(&stderr);
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    return;
                }
                let mut k = keep.lock().unwrap_or_else(|e| e.into_inner());
                k.extend(&buf[..n]);
                while k.len() > STDERR_KEEP {
                    k.pop_front();
                }
            }
        });
        Ok(WorkerProc {
            child,
            stdin,
            rx,
            stderr,
        })
    }

    fn stderr_tail(&self) -> String {
        let k = self.stderr.lock().unwrap_or_else(|e| e.into_inner());
        let bytes: Vec<u8> = k.iter().copied().collect();
        String::from_utf8_lossy(&bytes).trim().to_string()
    }

    /// Waits briefly for the dead (or dying) process and returns its exit status.
    fn reap(&mut self) -> Option<ExitStatus> {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match self.child.try_wait() {
                Ok(Some(st)) => return Some(st),
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
                _ => {
                    let _ = self.child.kill();
                    return self.child.wait().ok();
                }
            }
        }
    }
}

impl Drop for WorkerProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The result of one execution.
#[derive(Clone, Debug)]
pub enum Exec {
    Ok(Vec<u32>),
    Oracle {
        name: String,
        detail: String,
        features: Vec<u32>,
    },
    Crash {
        kind: String,
        signature: String,
        message: String,
    },
}

/// How a dead worker died, from its exit code and the end of its stderr.
pub fn classify_death(code: Option<i32>, stderr: &str) -> (String, String) {
    let hex = code.map(|c| format!("{:#010x}", c as u32));
    let kind = if stderr.contains("overflowed its stack") || code == Some(0xC00000FDu32 as i32) {
        "stack_overflow"
    } else if code == Some(0xC0000409u32 as i32) || code == Some(3) || code == Some(134) {
        "abort"
    } else {
        "crash"
    };
    let tail: String = stderr
        .lines()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    (
        kind.to_string(),
        format!(
            "exit code {}; stderr: {tail}",
            hex.as_deref().unwrap_or("none")
        ),
    )
}

/// Runs inputs on a worker, restarting it after a crash or hang.
pub struct Runner {
    exe: PathBuf,
    proc: Option<WorkerProc>,
    timeout: Duration,
}

impl Runner {
    pub fn new(exe: PathBuf, timeout: Duration) -> Runner {
        Runner {
            exe,
            proc: None,
            timeout,
        }
    }

    fn died(&mut self) -> Exec {
        let Some(mut p) = self.proc.take() else {
            return Exec::Crash {
                kind: "crash".into(),
                signature: "crash".into(),
                message: "worker vanished".into(),
            };
        };
        let st = p.reap();
        // Let the stderr thread drain what the dying worker wrote.
        std::thread::sleep(Duration::from_millis(20));
        let (kind, message) = classify_death(st.and_then(|s| s.code()), &p.stderr_tail());
        Exec::Crash {
            signature: kind.clone(),
            kind,
            message,
        }
    }

    pub fn exec(&mut self, input: &Input) -> Exec {
        if self.proc.is_none() {
            match WorkerProc::spawn(&self.exe) {
                Ok(p) => self.proc = Some(p),
                Err(e) => {
                    return Exec::Crash {
                        kind: "spawn".into(),
                        signature: "spawn".into(),
                        message: format!("cannot start a worker: {e}"),
                    }
                }
            }
        }
        let p = self.proc.as_mut().expect("worker started");
        if write_request(&mut p.stdin, input.skin, &input.text).is_err() {
            return self.died();
        }
        match p.rx.recv_timeout(self.timeout) {
            Ok(Ok(Reply::Ok { features })) => Exec::Ok(features),
            Ok(Ok(Reply::Oracle {
                name,
                detail,
                features,
            })) => Exec::Oracle {
                name,
                detail,
                features,
            },
            Ok(Ok(Reply::Panic { message, location })) => Exec::Crash {
                kind: "panic".into(),
                signature: format!("panic@{location}"),
                message,
            },
            Ok(Err(_)) | Err(RecvTimeoutError::Disconnected) => self.died(),
            Err(RecvTimeoutError::Timeout) => {
                self.proc = None; // Drop kills it.
                Exec::Crash {
                    kind: "timeout".into(),
                    signature: "timeout".into(),
                    message: format!("no result within {} ms", self.timeout.as_millis()),
                }
            }
        }
    }

    fn same_crash(&mut self, input: &Input, signature: &str) -> bool {
        matches!(self.exec(input), Exec::Crash { signature: s, .. } if s == signature)
    }
}

struct Shrink<'a> {
    runner: &'a mut Runner,
    signature: &'a str,
    budget: u32,
    deadline: Instant,
    best: Input,
}

impl Shrink<'_> {
    fn live(&self) -> bool {
        self.budget > 0 && Instant::now() < self.deadline
    }

    /// Removes `a..b` from the best input if it still crashes the same way.
    fn cut(&mut self, a: usize, b: usize) -> bool {
        if !self.live() || a >= b {
            return false;
        }
        self.budget -= 1;
        let mut text = self.best.text.clone();
        text.replace_range(a..b, "");
        let cand = Input {
            skin: self.best.skin,
            text,
        };
        if self.runner.same_crash(&cand, self.signature) {
            self.best = cand;
            true
        } else {
            false
        }
    }

    fn line_starts(&self) -> Vec<usize> {
        std::iter::once(0)
            .chain(self.best.text.match_indices('\n').map(|(k, _)| k + 1))
            .filter(|&k| k < self.best.text.len())
            .collect()
    }
}

/// Shrinks a crash input while it keeps crashing with the same signature (line chunks,
/// then byte chunks), within an execution and time budget.
pub fn minimize(runner: &mut Runner, input: &Input, signature: &str) -> Input {
    if signature == "timeout" {
        return input.clone();
    }
    let mut s = Shrink {
        runner,
        signature,
        budget: 400,
        deadline: Instant::now() + Duration::from_secs(30),
        best: input.clone(),
    };
    // Lines.
    let mut chunk = (s.line_starts().len() / 2).max(1);
    loop {
        let mut i = 0;
        while s.live() {
            let starts = s.line_starts();
            if i >= starts.len() {
                break;
            }
            let a = starts[i];
            let b = starts.get(i + chunk).copied().unwrap_or(s.best.text.len());
            if !s.cut(a, b) {
                i += chunk;
            }
        }
        if chunk == 1 || !s.live() {
            break;
        }
        chunk /= 2;
    }
    // Bytes (on char boundaries).
    let mut chunk = (s.best.text.len() / 4).max(1);
    loop {
        let mut a = 0;
        while a < s.best.text.len() && s.live() {
            let mut b = (a + chunk).min(s.best.text.len());
            while !s.best.text.is_char_boundary(b) {
                b += 1;
            }
            if !s.cut(a, b) {
                a = b;
            }
        }
        if chunk == 1 || !s.live() {
            break;
        }
        chunk /= 2;
    }
    s.best
}

/// Runs `kalam-fuzz --repro FILE` in a fresh process: does the input crash outside the
/// fuzzer too?
pub fn reproduces(exe: &Path, file: &Path, skin: Skin, timeout: Duration) -> bool {
    let Ok(mut child) = Command::new(exe)
        .arg("--repro")
        .arg(file)
        .arg("--skin")
        .arg(skin.id())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let until = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(st)) => return !st.success(),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return true;
            }
        }
    }
}

// ----- the run ---------------------------------------------------------------------------

const CORPUS_CAP: usize = 4000;
const ORACLE_EXAMPLES: usize = 3;

struct Shared {
    exe: PathBuf,
    opts: Options,
    corpus: Mutex<Vec<Input>>,
    seeds: Vec<Input>,
    features: Mutex<HashSet<u32>>,
    execs: AtomicU64,
    stop: AtomicBool,
    status: Mutex<Status>,
    /// Crash signatures being minimized/saved right now (another job only counts them).
    pending: Mutex<HashSet<String>>,
}

impl Shared {
    fn learn(&self, input: &Input, features: &[u32]) {
        let new = {
            let mut f = self.features.lock().unwrap_or_else(|e| e.into_inner());
            let mut new = false;
            for x in features {
                new |= f.insert(*x);
            }
            new
        };
        if new {
            let mut c = self.corpus.lock().unwrap_or_else(|e| e.into_inner());
            if c.len() < CORPUS_CAP {
                c.push(input.clone());
            } else {
                let lo = self.seeds.len().min(c.len() - 1);
                let i = lo
                    + (splitmix(features.first().copied().unwrap_or(0) as u64) as usize)
                        % (c.len() - lo);
                c[i] = input.clone();
            }
        }
    }

    fn oracle(&self, input: &Input, name: &str, detail: &str) {
        let mut st = self.status.lock().unwrap_or_else(|e| e.into_inner());
        let rec = st
            .oracle
            .entry(name.to_string())
            .or_insert_with(|| OracleRecord {
                detail: detail.to_string(),
                ..OracleRecord::default()
            });
        rec.count += 1;
        if rec.examples.len() < ORACLE_EXAMPLES {
            let rel = format!(
                "oracle/{name}-{}{}",
                rec.examples.len() + 1,
                input.skin.extension()
            );
            let path = self.opts.out.join(&rel);
            if let Some(d) = path.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if std::fs::write(&path, &input.text).is_ok() {
                rec.examples.push(rel);
            }
        }
    }

    /// Counts a crash; a new signature is minimized, saved and re-checked.
    fn crash(&self, runner: &mut Runner, input: &Input, kind: &str, sig: &str, msg: &str) {
        {
            let mut st = self.status.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = st.crashes.iter_mut().find(|c| c.signature == sig) {
                c.count += 1;
                return;
            }
        }
        if !self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(sig.to_string())
        {
            return;
        }
        if !self.opts.quiet {
            eprintln!("[kalam-fuzz] NEW CRASH {sig}: {}", first_line(msg));
        }
        let small = minimize(runner, input, sig);
        let stem = format!("{kind}-{:08x}", fnv(&[sig.as_bytes()]));
        let rel = format!("crashes/{stem}{}", small.skin.extension());
        let dir = self.opts.out.join("crashes");
        let _ = std::fs::create_dir_all(&dir);
        let path = self.opts.out.join(&rel);
        let _ = std::fs::write(&path, &small.text);
        if small.text != input.text {
            let _ = std::fs::write(dir.join(format!("{stem}.orig.txt")), &input.text);
        }
        let repro = reproduces(&self.exe, &path, small.skin, self.opts.timeout);
        let rec = CrashRecord {
            signature: sig.to_string(),
            kind: kind.to_string(),
            message: msg.chars().take(600).collect(),
            file: rel,
            skin: small.skin.id().to_string(),
            first_seen: iso8601(unix_now()),
            count: 1,
            reproducible: Some(repro),
            original_len: input.text.len(),
            minimized_len: small.text.len(),
        };
        let meta = serde_json::json!({
            "signature": rec.signature, "kind": rec.kind, "message": rec.message,
            "skin": rec.skin, "first_seen": rec.first_seen, "reproducible": repro,
            "original_len": rec.original_len, "minimized_len": rec.minimized_len,
            "reproduce": format!("kalam-fuzz --repro {} --skin {}", rec.file, rec.skin),
        });
        let _ = std::fs::write(
            dir.join(format!("{stem}.json")),
            serde_json::to_string_pretty(&meta).unwrap_or_default() + "\n",
        );
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .crashes
            .push(rec);
    }

    fn handle(&self, runner: &mut Runner, input: &Input, r: Exec) {
        self.execs.fetch_add(1, Ordering::Relaxed);
        match r {
            Exec::Ok(f) => self.learn(input, &f),
            Exec::Oracle {
                name,
                detail,
                features,
            } => {
                self.oracle(input, &name, &detail);
                self.learn(input, &features);
            }
            Exec::Crash {
                kind,
                signature,
                message,
            } => self.crash(runner, input, &kind, &signature, &message),
        }
    }

    fn done(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
            || self
                .opts
                .max_execs
                .is_some_and(|m| self.execs.load(Ordering::Relaxed) >= m)
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

fn job(shared: &Shared, j: usize) {
    let mut runner = Runner::new(shared.exe.clone(), shared.opts.timeout);
    let jobs = shared.opts.jobs.max(1);
    // Every seed once, unmutated.
    for seed in shared.seeds.iter().skip(j).step_by(jobs) {
        if shared.done() {
            return;
        }
        let r = runner.exec(seed);
        shared.handle(&mut runner, seed, r);
    }
    if !shared.opts.mutate {
        return;
    }
    let mut rng = Rng::new(splitmix(shared.opts.seed ^ (j as u64 + 1)));
    while !shared.done() {
        let (base, other) = {
            let c = shared.corpus.lock().unwrap_or_else(|e| e.into_inner());
            if c.is_empty() {
                return;
            }
            (c[rng.below(c.len())].clone(), c[rng.below(c.len())].clone())
        };
        let input = mutate(&mut rng, &base, &other);
        let r = runner.exec(&input);
        shared.handle(&mut runner, &input, r);
    }
}

/// Re-runs every saved crash input; the ones that still crash are open crashes on this
/// binary. Returns (checked, still crashing).
fn replay(shared: &Shared) -> (usize, usize) {
    let dir = shared.opts.out.join("crashes");
    let saved = corpus::load(std::slice::from_ref(&dir));
    let mut runner = Runner::new(shared.exe.clone(), shared.opts.timeout);
    let mut open = 0;
    for (path, input) in &saved {
        if let Exec::Crash {
            kind,
            signature,
            message,
        } = runner.exec(input)
        {
            open += 1;
            let rel = format!(
                "crashes/{}",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
            );
            let mut st = shared.status.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = st.crashes.iter_mut().find(|c| c.signature == signature) {
                c.count += 1;
            } else {
                st.crashes.push(CrashRecord {
                    signature,
                    kind,
                    message,
                    file: rel,
                    skin: input.skin.id().to_string(),
                    first_seen: iso8601(unix_now()),
                    count: 1,
                    reproducible: Some(true),
                    original_len: input.text.len(),
                    minimized_len: input.text.len(),
                });
            }
        }
    }
    (saved.len(), open)
}

/// The outcome of a run, for the command line.
pub struct Summary {
    pub status: Status,
    pub run_execs: u64,
    pub run_secs: f64,
}

/// Fuzzes for `opts.duration` (or `opts.max_execs`), then writes `status.json`.
pub fn run(opts: Options) -> io::Result<Summary> {
    let exe = std::env::current_exe()?;
    let version = target_version();
    let status_path = opts.out.join("status.json");
    std::fs::create_dir_all(&opts.out)?;
    let base = Status::begin(Status::load(&status_path), &version);

    let mut seeds: Vec<Input> = corpus::load(&opts.corpus_dirs)
        .into_iter()
        .map(|(_, i)| i)
        .collect();
    seeds.extend(corpus::generated(opts.generated, opts.seed));
    if seeds.is_empty() {
        seeds.push(Input {
            skin: Skin::En,
            text: "fn main():\n    print(\"hello\")\n".into(),
        });
    }

    let shared = Shared {
        exe,
        corpus: Mutex::new(seeds.clone()),
        seeds,
        features: Mutex::new(HashSet::new()),
        execs: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        status: Mutex::new(base.clone()),
        pending: Mutex::new(HashSet::new()),
        opts,
    };

    let (checked, open) = if shared.opts.replay {
        replay(&shared)
    } else {
        (0, 0)
    };
    if !shared.opts.quiet {
        eprintln!(
            "[kalam-fuzz] version {}…, {} seeds, {} job(s), {} saved crash(es) re-checked, {open} still crash; clock {:.3} h",
            &version[..12.min(version.len())],
            shared.seeds.len(),
            shared.opts.jobs,
            checked,
            base.crash_free_secs / 3600.0
        );
    }
    let start = Instant::now();
    let deadline = start + shared.opts.duration;

    let snapshot = |shared: &Shared| -> Status {
        let secs = start.elapsed().as_secs_f64();
        let execs = shared.execs.load(Ordering::Relaxed);
        let mut st = shared
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        st.last_run = iso8601(unix_now());
        st.last_run_secs = secs;
        st.last_run_execs = execs;
        st.version_execs = base.version_execs + execs;
        st.total_execs = base.total_execs + execs;
        st.version_fuzz_secs = base.version_fuzz_secs + secs;
        st.runs = base.runs + 1;
        st.crash_free_secs = if st.crashes.is_empty() {
            base.crash_free_secs + secs
        } else {
            0.0
        };
        st.jobs = shared.opts.jobs;
        st.corpus_seeds = shared.seeds.len();
        st.corpus_size = shared
            .corpus
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        st.features = shared
            .features
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        st.replay_checked = checked;
        st.replay_open = open;
        st
    };

    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for j in 0..shared.opts.jobs.max(1) {
            let shared = &shared;
            handles.push(scope.spawn(move || job(shared, j)));
        }
        let mut last_save = Instant::now();
        let mut last_print = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let finished = handles.iter().all(|h| h.is_finished());
            if Instant::now() >= deadline || finished || shared.done() {
                shared.stop.store(true, Ordering::Relaxed);
                break;
            }
            if last_save.elapsed() >= Duration::from_secs(30) {
                let _ = snapshot(&shared).save(&status_path);
                last_save = Instant::now();
            }
            if !shared.opts.quiet && last_print.elapsed() >= Duration::from_secs(60) {
                let st = snapshot(&shared);
                eprintln!(
                    "[kalam-fuzz] {:>6.0}s execs {} ({:.0}/s) corpus {} features {} crashes {} oracle {}",
                    st.last_run_secs,
                    st.last_run_execs,
                    st.last_run_execs as f64 / st.last_run_secs.max(1e-9),
                    st.corpus_size,
                    st.features,
                    st.crashes.len(),
                    st.oracle.values().map(|o| o.count).sum::<u64>()
                );
                last_print = Instant::now();
            }
        }
    });
    let st = snapshot(&shared);
    st.save(&status_path)?;
    Ok(Summary {
        run_execs: st.last_run_execs,
        run_secs: st.last_run_secs,
        status: st,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deaths_are_classified() {
        let (k, _) = classify_death(Some(0xC00000FDu32 as i32), "");
        assert_eq!(k, "stack_overflow");
        let (k, _) = classify_death(
            Some(0xC0000409u32 as i32),
            "thread 'main' has overflowed its stack\nfatal runtime error: stack overflow",
        );
        assert_eq!(k, "stack_overflow");
        let (k, m) = classify_death(Some(0xC0000409u32 as i32), "");
        assert_eq!(k, "abort");
        assert!(m.contains("0xc0000409"), "{m}");
        assert_eq!(classify_death(Some(7), "").0, "crash");
    }
}
