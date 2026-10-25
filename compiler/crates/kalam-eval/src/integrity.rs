//! G-INT (SCORECARD.md §3, LOOP.md §2 step 4, §3 step 1, §6) and `kalam-eval freeze`.
//!
//! `freeze` writes `eval/FREEZE.json` and then `eval/MANIFEST.sha256`. It never reads the
//! clock (the timestamp is passed in) and it keeps the founder's fields of an existing
//! FREEZE.json. It does not sign anything: the reviewer signs the manifest by appending an
//! approve record to `eval/reviews.jsonl` (LOOP.md §2 step 4).
//!
//! `check` runs the five G-INT checks. Each is `pass`, `fail` or `pending`; the gate is
//! `fail` if any check fails, `pass` only if every check passes, else `pending`.
//!
//! 1. **manifest**: every frozen file matches `MANIFEST.sha256`, and re-walking the frozen
//!    roots finds no added or deleted file.
//! 2. **sign-off**: `eval/reviews.jsonl` holds `{"verdict": "approve", "manifest_sha256":
//!    <SHA-256 of MANIFEST.sha256>}`. Missing = `pending` (awaiting the reviewer): a freeze
//!    run by the fixer never makes G-INT pass.
//! 3. **append-only**: each append-only log still starts with the bytes recorded at freeze
//!    time and with its `git HEAD` version; history iterations strictly increase.
//! 4. **corpus leak** (A5): compiler, runtime and stdlib sources name no task id, corpus
//!    task path or expected-output line of 20+ characters (scope in [`scan_sources`]).
//! 5. **version and holdout pin**: FREEZE.json `eval_version` equals scorecard.json's, and the
//!    held-out store's manifest hash equals the pinned one.

use kalam_syntax::sha256;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MANIFEST: &str = "eval/MANIFEST.sha256";
pub const FREEZE: &str = "eval/FREEZE.json";
pub const REVIEWS: &str = "eval/reviews.jsonl";
pub const DEFAULT_HOLDOUT: &str = "eval/holdout";

/// Protected paths frozen per eval version (LOOP.md §1, plus the shared suites, the
/// history schema and the harness code itself: A3 makes harness code reviewer-gated, and
/// here the harness is `compiler/crates/kalam-eval`). The held-out store is pinned by its
/// manifest hash only, never listed.
pub const FROZEN_ROOTS: &[&str] = &[
    "eval/SCORECARD.md",
    "eval/scorecard.json",
    "eval/LOOP.md",
    "eval/HISTORY-SCHEMA.md",
    "eval/FREEZE.json",
    "eval/corpus",
    "eval/suites",
    "eval/harness",
    "eval/diagnostics",
    "eval/seccorpus",
    "compiler/crates/kalam-eval",
];

/// Append-only logs (LOOP.md §1).
pub const APPEND_ONLY: &[&str] = &[
    "eval/history.jsonl",
    "eval/questions.jsonl",
    "eval/reviews.jsonl",
];

pub fn sha_hex(bytes: &[u8]) -> String {
    sha256::hex(&sha256::digest(bytes))
}

/// Every file under `roots` (files or directories, relative to `root`), as sorted
/// `/`-separated relative paths. `target` directories are skipped; missing roots are
/// skipped.
pub fn walk(root: &Path, roots: &[&str]) -> Vec<String> {
    fn rec(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if e.file_name() != "target" {
                    rec(base, &p, out);
                }
            } else if let Ok(rel) = p.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    for r in roots {
        let p = root.join(r);
        if p.is_file() {
            out.push(r.to_string());
        } else if p.is_dir() {
            rec(root, &p, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// SHA-256 over the sorted `"<sha256>  <relative path>\n"` lines of every file under
/// `dir`; `None` when the directory is missing or empty. File contents are hashed, never
/// returned.
pub fn tree_hash(dir: &Path) -> Option<String> {
    let files = walk(dir, &["."]);
    if files.is_empty() {
        return None;
    }
    let mut listing = String::new();
    for f in &files {
        let rel = f.trim_start_matches("./");
        let bytes = fs::read(dir.join(rel)).ok()?;
        listing.push_str(&format!("{}  {rel}\n", sha_hex(&bytes)));
    }
    Some(sha_hex(listing.as_bytes()))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Manifest {
    pub eval_version: Option<String>,
    pub entries: BTreeMap<String, String>,
    /// `(path, byte length, SHA-256 of those bytes)` at freeze time.
    pub append_only: Vec<(String, usize, String)>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let mut m = Manifest::default();
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("# kalam-manifest/1 ") {
                m.eval_version = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("# append-only ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let field = |p: &str, k: &str| p.strip_prefix(k).map(str::to_string);
                match parts.as_slice() {
                    [path, bytes, sha] => {
                        let len = field(bytes, "bytes=")
                            .and_then(|b| b.parse().ok())
                            .ok_or(format!("line {}: bad bytes=", n + 1))?;
                        let sha =
                            field(sha, "sha256=").ok_or(format!("line {}: bad sha256=", n + 1))?;
                        m.append_only.push((path.to_string(), len, sha));
                    }
                    _ => return Err(format!("line {}: bad append-only record", n + 1)),
                }
            } else if line.starts_with('#') {
                continue;
            } else {
                let (sha, path) = line
                    .split_once("  ")
                    .ok_or(format!("line {}: expected `<sha256>  <path>`", n + 1))?;
                if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(format!("line {}: bad SHA-256", n + 1));
                }
                m.entries.insert(path.to_string(), sha.to_lowercase());
            }
        }
        Ok(m)
    }
}

pub struct FreezeOptions<'a> {
    pub eval_version: &'a str,
    pub target_milestone: &'a str,
    pub timestamp: Option<&'a str>,
    /// The held-out store, relative to the root or absolute.
    pub holdout_store: &'a Path,
}

pub struct Frozen {
    pub freeze: Value,
    pub manifest: String,
}

/// Fields of an existing FREEZE.json that belong to the founder and are carried over.
const FOUNDER_FIELDS: &[&str] = &[
    "model_panel",
    "llm_judge",
    "api_cost_cap_usd",
    "wallclock_cap_hours",
];

/// Writes `eval/FREEZE.json`, then `eval/MANIFEST.sha256` (which covers FREEZE.json).
pub fn freeze(root: &Path, o: &FreezeOptions<'_>) -> Result<Frozen, String> {
    let old: Value = fs::read_to_string(root.join(FREEZE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let store = root.join(o.holdout_store);
    let frozen_files: Vec<String> = walk(root, FROZEN_ROOTS)
        .into_iter()
        .filter(|f| f != FREEZE && f != MANIFEST)
        .collect();
    let mut pending = Vec::new();
    let mut carried = serde_json::Map::new();
    for k in FOUNDER_FIELDS {
        let v = old.get(*k).cloned().unwrap_or(Value::Null);
        if v.is_null() {
            pending.push(json!(k));
        }
        carried.insert(k.to_string(), v);
    }
    // The seed and its note are carried over. A seed is still the founder's to set while
    // it is null or its note marks it provisional (FOUNDER-QUEUE Q15).
    let split_seed = old["holdout"]["split_seed"].clone();
    let split_seed_note = old["holdout"]["split_seed_note"].clone();
    let provisional = split_seed_note
        .as_str()
        .is_some_and(|n| n.trim_start().to_lowercase().starts_with("provisional"));
    if split_seed.is_null() || provisional {
        pending.push(json!("holdout.split_seed"));
    }
    let llms = fs::read(root.join("spec/llms.txt"))
        .ok()
        .map(|b| sha_hex(&b));
    let mut freeze = json!({
        "schema": "kalam-freeze/1",
        "eval_version": o.eval_version,
        "target_milestone": o.target_milestone,
        "status": "draft: binding once the reviewer signs this manifest (an approve record in eval/reviews.jsonl \
                   whose manifest_sha256 is the SHA-256 of eval/MANIFEST.sha256) and the founder approves the freeze \
                   (LOOP.md §2 steps 3-4)",
        "frozen_at": o.timestamp,
        "manifest": MANIFEST,
        "frozen_roots": FROZEN_ROOTS,
        "frozen_files": frozen_files,
        "append_only": APPEND_ONLY,
        "holdout": {
            "store": o.holdout_store.to_string_lossy().replace('\\', "/"),
            "manifest_sha256": tree_hash(&store),
            "manifest_rule": "SHA-256 over the sorted lines `<sha256(file)>  <relative path>` of every file in the store",
            "split_seed": split_seed,
            "split_seed_note": split_seed_note,
        },
        "spec_llms_txt_sha256": llms,
        "pending_founder": pending,
    });
    if let Some(obj) = freeze.as_object_mut() {
        obj.extend(carried);
    }
    let text = serde_json::to_string_pretty(&freeze).map_err(|e| e.to_string())? + "\n";
    fs::write(root.join(FREEZE), text).map_err(|e| format!("{FREEZE}: {e}"))?;

    let mut manifest = format!(
        "# kalam-manifest/1 {}\n# SHA-256 of every frozen file (eval/FREEZE.json frozen_roots). Written by `kalam-eval freeze`; never edit by hand.\n",
        o.eval_version
    );
    for f in APPEND_ONLY {
        if let Ok(bytes) = fs::read(root.join(f)) {
            manifest.push_str(&format!(
                "# append-only {f} bytes={} sha256={}\n",
                bytes.len(),
                sha_hex(&bytes)
            ));
        }
    }
    for f in walk(root, FROZEN_ROOTS)
        .into_iter()
        .filter(|f| f != MANIFEST)
    {
        let bytes = fs::read(root.join(&f)).map_err(|e| format!("{f}: {e}"))?;
        manifest.push_str(&format!("{}  {f}\n", sha_hex(&bytes)));
    }
    fs::write(root.join(MANIFEST), &manifest).map_err(|e| format!("{MANIFEST}: {e}"))?;
    Ok(Frozen { freeze, manifest })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Pass,
    Fail,
    Pending,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Pass => "pass",
            State::Fail => "fail",
            State::Pending => "pending",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntCheck {
    pub name: &'static str,
    pub state: State,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntegrityReport {
    pub checks: Vec<IntCheck>,
}

impl IntegrityReport {
    pub fn state(&self) -> State {
        if self.checks.iter().any(|c| c.state == State::Fail) {
            State::Fail
        } else if self.checks.iter().all(|c| c.state == State::Pass) {
            State::Pass
        } else {
            State::Pending
        }
    }

    pub fn reason(&self) -> String {
        self.checks
            .iter()
            .map(|c| format!("{} {}: {}", c.name, c.state.as_str(), c.detail))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    pub fn to_json(&self) -> Value {
        json!({
            "state": self.state().as_str(),
            "checks": self.checks.iter().map(|c| json!({"check": c.name, "state": c.state.as_str(), "detail": c.detail})).collect::<Vec<_>>(),
        })
    }
}

pub struct CheckOptions<'a> {
    /// scorecard.json `eval_version`.
    pub eval_version: &'a str,
    pub holdout_store: &'a Path,
    /// The manifest hash the held-out runner reported, if it ran.
    pub holdout_reported: Option<&'a str>,
}

fn check(name: &'static str, state: State, detail: impl Into<String>) -> IntCheck {
    IntCheck {
        name,
        state,
        detail: detail.into(),
    }
}

pub fn run(root: &Path, o: &CheckOptions<'_>) -> IntegrityReport {
    let freeze: Option<Value> = fs::read_to_string(root.join(FREEZE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let manifest_bytes = fs::read(root.join(MANIFEST)).ok();
    let manifest = manifest_bytes
        .as_ref()
        .map(|b| Manifest::parse(&String::from_utf8_lossy(b)));
    let mut checks = Vec::new();

    // 1. manifest
    checks.push(match &manifest {
        None => check("manifest", State::Pending, format!("{MANIFEST} does not exist: the eval is not frozen (run `kalam-eval freeze`, then the reviewer signs it)")),
        Some(Err(e)) => check("manifest", State::Fail, format!("{MANIFEST} is malformed: {e}")),
        Some(Ok(m)) => {
            let now: Vec<String> = walk(root, FROZEN_ROOTS).into_iter().filter(|f| f != MANIFEST).collect();
            let mut changed = Vec::new();
            for f in &now {
                match m.entries.get(f) {
                    None => changed.push(format!("added {f}")),
                    Some(sha) => match fs::read(root.join(f)) {
                        Ok(b) if &sha_hex(&b) == sha => {}
                        _ => changed.push(format!("changed {f}")),
                    },
                }
            }
            for f in m.entries.keys().filter(|f| !now.contains(f)) {
                changed.push(format!("deleted {f}"));
            }
            if changed.is_empty() {
                check("manifest", State::Pass, format!("{} frozen files match {MANIFEST}", now.len()))
            } else {
                let n = changed.len();
                changed.truncate(20);
                check("manifest", State::Fail, format!("{n} protected file change(s) since the freeze: {}", changed.join(", ")))
            }
        }
    });

    // 2. reviewer sign-off of this manifest
    checks.push(match &manifest_bytes {
        None => check("sign-off", State::Pending, "no manifest to sign"),
        Some(b) => {
            let want = sha_hex(b);
            let signed = fs::read_to_string(root.join(REVIEWS))
                .unwrap_or_default()
                .lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .any(|r| r["verdict"] == "approve" && r["manifest_sha256"] == want.as_str());
            if signed {
                check("sign-off", State::Pass, format!("{REVIEWS} approves manifest {want}"))
            } else {
                check("sign-off", State::Pending, format!("awaiting reviewer sign-off: no approve record in {REVIEWS} for manifest sha256 {want}"))
            }
        }
    });

    // 3. append-only logs
    let recorded: Vec<(String, usize, String)> = match &manifest {
        Some(Ok(m)) => m.append_only.clone(),
        _ => Vec::new(),
    };
    checks.push(append_only(root, &recorded));

    // 4. corpus leak scan
    let needles = corpus_needles(&root.join("eval/corpus"), "eval/corpus");
    let hits = scan_sources(root, &needles);
    checks.push(if hits.is_empty() {
        check("corpus-leak", State::Pass, format!("{} needles (task ids, task paths, expected-output lines >= 20 chars) absent from {}", needles.len(), SCAN_SCOPE))
    } else {
        let shown: Vec<String> = hits.iter().take(10).map(|(f, n)| format!("{f}: {:?}", needles[*n])).collect();
        check("corpus-leak", State::Fail, format!("{} hit(s): {}", hits.len(), shown.join("; ")))
    });

    // 5. eval version and held-out pin
    checks.push(match &freeze {
        None => check(
            "version-pin",
            State::Pending,
            format!("{FREEZE} does not exist or is not JSON"),
        ),
        Some(f) => {
            let mut problems = Vec::new();
            if f["eval_version"] != o.eval_version {
                problems.push(format!(
                    "{FREEZE} eval_version {} != scorecard.json {}",
                    f["eval_version"], o.eval_version
                ));
            }
            if let Some(Ok(m)) = &manifest {
                if m.eval_version.as_deref() != Some(o.eval_version) {
                    problems.push(format!("{MANIFEST} is for {:?}", m.eval_version));
                }
            }
            let pinned = f["holdout"]["manifest_sha256"].as_str();
            let now = tree_hash(&root.join(o.holdout_store));
            if pinned != now.as_deref() {
                problems.push(format!(
                    "held-out store manifest is {now:?}, FREEZE.json pins {pinned:?}"
                ));
            }
            if let Some(rep) = o.holdout_reported {
                if Some(rep) != pinned {
                    problems.push(format!(
                        "the held-out runner reported manifest {rep}, FREEZE.json pins {pinned:?}"
                    ));
                }
            }
            if problems.is_empty() {
                check(
                    "version-pin",
                    State::Pass,
                    format!(
                        "eval_version {} and held-out manifest {pinned:?} match the freeze",
                        o.eval_version
                    ),
                )
            } else {
                check("version-pin", State::Fail, problems.join("; "))
            }
        }
    });
    IntegrityReport { checks }
}

fn git_head_bytes(root: &Path, rel: &str) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .args(["show", &format!("HEAD:{rel}")])
        .current_dir(root)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

fn append_only(root: &Path, recorded: &[(String, usize, String)]) -> IntCheck {
    let mut problems = Vec::new();
    let mut checked = Vec::new();
    for f in APPEND_ONLY {
        let Ok(now) = fs::read(root.join(f)) else {
            if recorded.iter().any(|(p, _, _)| p == f) {
                problems.push(format!("{f} was deleted"));
            }
            continue;
        };
        if let Some((_, len, sha)) = recorded.iter().find(|(p, _, _)| p == f) {
            if now.len() < *len || &sha_hex(&now[..*len]) != sha {
                problems.push(format!(
                    "{f}: the first {len} bytes recorded at freeze time were rewritten"
                ));
            }
        }
        if let Some(head) = git_head_bytes(root, f) {
            if !now.starts_with(&head) {
                problems.push(format!(
                    "{f}: the committed (HEAD) content is not a prefix of the working file"
                ));
            }
        }
        if *f == "eval/history.jsonl" {
            let mut last: Option<u64> = None;
            for (n, l) in String::from_utf8_lossy(&now).lines().enumerate() {
                if l.trim().is_empty() {
                    continue;
                }
                let it = serde_json::from_str::<Value>(l)
                    .ok()
                    .and_then(|v| v["iteration"].as_u64());
                match (it, last) {
                    (None, _) => problems.push(format!("{f} line {}: no integer iteration", n + 1)),
                    (Some(i), Some(p)) if i <= p => {
                        problems.push(format!("{f} line {}: iteration {i} after {p}", n + 1))
                    }
                    (Some(i), _) => last = Some(i),
                }
            }
        }
        checked.push(*f);
    }
    if !problems.is_empty() {
        check("append-only", State::Fail, problems.join("; "))
    } else if recorded.is_empty() {
        check(
            "append-only",
            State::Pending,
            format!(
                "no freeze-time record in {MANIFEST}; checked against git HEAD only: {checked:?}"
            ),
        )
    } else {
        check(
            "append-only",
            State::Pass,
            format!("{checked:?} extend their freeze-time and HEAD bytes"),
        )
    }
}

/// What the leak scan reads (A5): Rust sources under `compiler/crates/*/src` up to the
/// first `#[cfg(test)]` (unit-test fixtures are not compiler code), and every file under
/// `stdlib/` and `compiler/runtime/`. Excluded: the harness crate `kalam-eval` (it must
/// name corpus paths and its tests name task ids), integration tests and `compiler/testdata`.
pub const SCAN_SCOPE: &str = "compiler/crates/*/src/**/*.rs (except kalam-eval; up to #[cfg(test)]), stdlib/**, compiler/runtime/**";

/// Leak needles of a corpus directory: task directory names, task ids (`T07`), task paths
/// (`<prefix>/<task>`) and expected-output lines of at least 20 characters.
pub fn corpus_needles(corpus_dir: &Path, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(corpus_dir) else {
        return out;
    };
    let mut tasks: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    tasks.sort();
    for t in tasks {
        let name = t
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let Some(n) = crate::parity::task_number(&name) else {
            continue;
        };
        out.push(name.clone());
        out.push(format!("T{n:02}"));
        out.push(format!("{prefix}/{name}"));
        for f in walk(&t, &["."]) {
            if !f.ends_with("expected.txt") {
                continue;
            }
            let text = fs::read_to_string(t.join(f.trim_start_matches("./"))).unwrap_or_default();
            for l in text.lines().skip_while(|l| !l.starts_with("--- ")) {
                let l = l.trim();
                if !l.starts_with("--- ") && l.chars().count() >= 20 {
                    out.push(l.to_string());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn word_hit(hay: &str, needle: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    hay.match_indices(needle).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + needle.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

/// `(file, needle index)` for every needle found in the scanned sources.
pub fn scan_sources(root: &Path, needles: &[String]) -> Vec<(String, usize)> {
    let mut files: Vec<(String, String)> = Vec::new();
    if let Ok(rd) = fs::read_dir(root.join("compiler/crates")) {
        let mut crates: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        crates.sort();
        for c in crates {
            if c.file_name().is_some_and(|n| n == "kalam-eval") {
                continue;
            }
            for f in walk(
                root,
                &[&format!(
                    "compiler/crates/{}/src",
                    c.file_name().unwrap_or_default().to_string_lossy()
                )],
            ) {
                if f.ends_with(".rs") {
                    let text = fs::read_to_string(root.join(&f)).unwrap_or_default();
                    let live = text.find("#[cfg(test)]").map_or(&text[..], |i| &text[..i]);
                    files.push((f, live.to_string()));
                }
            }
        }
    }
    for f in walk(root, &["stdlib", "compiler/runtime"]) {
        files.push((
            f.clone(),
            fs::read_to_string(root.join(&f)).unwrap_or_default(),
        ));
    }
    let mut hits = Vec::new();
    for (f, text) in &files {
        for (i, n) in needles.iter().enumerate() {
            if !n.is_empty() && word_hit(text, n) {
                hits.push((f.clone(), i));
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suites::tests::temp_dir;

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn tiny_repo() -> PathBuf {
        let d = temp_dir("integrity");
        write(&d, "eval/scorecard.json", "{\"eval_version\": \"v1\"}");
        write(&d, "eval/LOOP.md", "loop");
        write(&d, "eval/corpus/t07-demo/en.kalam", "fn main():\n");
        write(
            &d,
            "eval/corpus/t07-demo/expected.txt",
            "kind: run\n--- stdout\nthe quick brown fox jumps over\nshort\n",
        );
        write(
            &d,
            "eval/suites/trojan-source/a/expect.json",
            "{\"must_reject\": false}",
        );
        write(&d, "eval/history.jsonl", "{\"iteration\":0}\n");
        write(
            &d,
            "compiler/crates/kalam-x/src/lib.rs",
            "pub fn f() {}\n#[cfg(test)]\nmod t { const X: &str = \"t07-demo\"; }\n",
        );
        d
    }

    fn opts(store: &Path) -> CheckOptions<'_> {
        CheckOptions {
            eval_version: "v1",
            holdout_store: store,
            holdout_reported: None,
        }
    }

    fn state_of(r: &IntegrityReport, name: &str) -> State {
        r.checks.iter().find(|c| c.name == name).unwrap().state
    }

    #[test]
    fn before_freeze_the_gate_is_pending_never_pass() {
        let d = tiny_repo();
        let store = PathBuf::from(DEFAULT_HOLDOUT);
        let r = run(&d, &opts(&store));
        let _ = fs::remove_dir_all(&d);
        assert_eq!(r.state(), State::Pending, "{}", r.reason());
    }

    #[test]
    fn freeze_keeps_a_provisional_split_seed_pending_with_its_note() {
        let d = tiny_repo();
        let store = PathBuf::from(DEFAULT_HOLDOUT);
        let fz = FreezeOptions {
            eval_version: "v1",
            target_milestone: "M1",
            timestamp: None,
            holdout_store: &store,
        };
        let pending = |f: &Frozen| f.freeze["pending_founder"].as_array().unwrap().clone();
        // no seed yet: pending
        let f = freeze(&d, &fz).unwrap();
        assert!(pending(&f).contains(&json!("holdout.split_seed")));
        // a provisional seed (FOUNDER-QUEUE Q15): carried with its note, still pending
        let mut v: Value =
            serde_json::from_str(&fs::read_to_string(d.join(FREEZE)).unwrap()).unwrap();
        v["holdout"]["split_seed"] = json!("d3abd2d");
        v["holdout"]["split_seed_note"] =
            json!("provisional, founder may override (FOUNDER-QUEUE Q15)");
        fs::write(d.join(FREEZE), v.to_string()).unwrap();
        let f = freeze(&d, &fz).unwrap();
        assert_eq!(f.freeze["holdout"]["split_seed"], "d3abd2d");
        assert!(f.freeze["holdout"]["split_seed_note"]
            .as_str()
            .unwrap()
            .starts_with("provisional"));
        assert!(pending(&f).contains(&json!("holdout.split_seed")));
        // the founder confirms (drops the provisional note): no longer pending
        let mut v: Value =
            serde_json::from_str(&fs::read_to_string(d.join(FREEZE)).unwrap()).unwrap();
        v["holdout"]["split_seed_note"] = json!("founder-approved");
        fs::write(d.join(FREEZE), v.to_string()).unwrap();
        let f = freeze(&d, &fz).unwrap();
        let _ = fs::remove_dir_all(&d);
        assert!(!pending(&f).contains(&json!("holdout.split_seed")));
        assert_eq!(f.freeze["holdout"]["split_seed_note"], "founder-approved");
    }

    #[test]
    fn freeze_then_check_passes_only_with_a_reviewer_sign_off() {
        let d = tiny_repo();
        let store = PathBuf::from(DEFAULT_HOLDOUT);
        write(&d, "eval/holdout/corpus/t05-x/en.kalam", "secret\n");
        let fz = FreezeOptions {
            eval_version: "v1",
            target_milestone: "M1",
            timestamp: None,
            holdout_store: &store,
        };
        let f = freeze(&d, &fz).unwrap();
        assert_eq!(f.freeze["eval_version"], "v1");
        assert!(f.freeze["frozen_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "eval/corpus/t07-demo/en.kalam"));
        assert!(f.freeze["holdout"]["manifest_sha256"].is_string());
        assert!(
            !f.manifest.contains("t05-x"),
            "the held-out store is pinned by hash only"
        );
        assert!(f.manifest.contains("  eval/FREEZE.json\n"));
        assert!(f
            .manifest
            .contains("# append-only eval/history.jsonl bytes=16"));
        // deterministic: a second freeze writes the same bytes
        assert_eq!(freeze(&d, &fz).unwrap().manifest, f.manifest);

        let r = run(&d, &opts(&store));
        assert_eq!(state_of(&r, "manifest"), State::Pass, "{}", r.reason());
        assert_eq!(state_of(&r, "sign-off"), State::Pending);
        assert_eq!(state_of(&r, "corpus-leak"), State::Pass, "{}", r.reason());
        assert_eq!(state_of(&r, "version-pin"), State::Pass, "{}", r.reason());
        assert_eq!(r.state(), State::Pending);

        let msha = sha_hex(&fs::read(d.join(MANIFEST)).unwrap());
        write(&d, REVIEWS, &format!("{{\"reviewer\": \"r\", \"verdict\": \"approve\", \"manifest_sha256\": \"{msha}\"}}\n"));
        let r = run(&d, &opts(&store));
        // reviews.jsonl appeared after the freeze: append-only has no record for it, fine
        assert_eq!(r.state(), State::Pass, "{}", r.reason());

        // tampering: a changed, an added and a deleted protected file
        write(&d, "eval/LOOP.md", "loop!");
        write(&d, "eval/suites/trojan-source/b/expect.json", "{}");
        fs::remove_file(d.join("eval/corpus/t07-demo/en.kalam")).unwrap();
        let r = run(&d, &opts(&store));
        let detail = &r
            .checks
            .iter()
            .find(|c| c.name == "manifest")
            .unwrap()
            .detail;
        assert_eq!(r.state(), State::Fail);
        assert!(
            detail.contains("changed eval/LOOP.md")
                && detail.contains("added")
                && detail.contains("deleted"),
            "{detail}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn version_holdout_and_history_rewrites_fail() {
        let d = tiny_repo();
        let store = PathBuf::from(DEFAULT_HOLDOUT);
        let fz = FreezeOptions {
            eval_version: "v1",
            target_milestone: "M1",
            timestamp: Some("2026-10-25T00:00:00Z"),
            holdout_store: &store,
        };
        freeze(&d, &fz).unwrap();
        let wrong_version = CheckOptions {
            eval_version: "v2",
            ..opts(&store)
        };
        assert_eq!(
            state_of(&run(&d, &wrong_version), "version-pin"),
            State::Fail
        );
        // a held-out store appears after the freeze
        write(&d, "eval/holdout/x.kl", "x\n");
        assert_eq!(
            state_of(&run(&d, &opts(&store)), "version-pin"),
            State::Fail
        );
        fs::remove_dir_all(d.join("eval/holdout")).unwrap();
        let reported = CheckOptions {
            holdout_reported: Some("abc"),
            ..opts(&store)
        };
        assert_eq!(state_of(&run(&d, &reported), "version-pin"), State::Fail);
        // appending is fine, rewriting is not
        write(
            &d,
            "eval/history.jsonl",
            "{\"iteration\":0}\n{\"iteration\":1}\n",
        );
        assert_eq!(
            state_of(&run(&d, &opts(&store)), "append-only"),
            State::Pass
        );
        write(
            &d,
            "eval/history.jsonl",
            "{\"iteration\":5}\n{\"iteration\":1}\n",
        );
        assert_eq!(
            state_of(&run(&d, &opts(&store)), "append-only"),
            State::Fail
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn leak_scan_finds_ids_paths_and_output_lines_in_live_code_only() {
        let d = tiny_repo();
        let needles = corpus_needles(&d.join("eval/corpus"), "eval/corpus");
        assert!(needles.contains(&"T07".to_string()));
        assert!(needles.contains(&"eval/corpus/t07-demo".to_string()));
        assert!(needles.contains(&"the quick brown fox jumps over".to_string()));
        assert!(!needles.contains(&"short".to_string()));
        // the id inside #[cfg(test)] is not live code
        assert!(scan_sources(&d, &needles).is_empty());
        write(
            &d,
            "compiler/crates/kalam-x/src/leak.rs",
            "// handles T07 specially\nconst S: &str = \"the quick brown fox jumps over\";\n",
        );
        write(
            &d,
            "compiler/crates/kalam-x/src/fine.rs",
            "const T070: u8 = 0; // T07x and xT07 are other words\n",
        );
        write(
            &d,
            "compiler/crates/kalam-eval/src/lib.rs",
            "// T07 eval/corpus/t07-demo\n",
        );
        let hits = scan_sources(&d, &needles);
        let files: Vec<&str> = hits.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(
            files,
            [
                "compiler/crates/kalam-x/src/leak.rs",
                "compiler/crates/kalam-x/src/leak.rs"
            ],
            "{hits:?}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn manifest_parsing() {
        let m = Manifest::parse(&format!(
            "# kalam-manifest/1 v1\n# note\n# append-only eval/history.jsonl bytes=3 sha256=ab\n{}  eval/LOOP.md\n",
            "a".repeat(64)
        ))
        .unwrap();
        assert_eq!(m.eval_version.as_deref(), Some("v1"));
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.append_only[0].1, 3);
        assert!(Manifest::parse("zz  x\n").is_err());
        assert!(Manifest::parse("nohash\n").is_err());
        assert!(tree_hash(Path::new("/definitely/not/here")).is_none());
    }
}
