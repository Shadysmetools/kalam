//! The M2 roadmap checkpoints (scorecard.json `milestone_tiers` M2 `checkpoints`).
//!
//! Checkpoints are reported in the milestone summary and judged by the founder; they are
//! never tier checks, never in the tier score and never ratcheted (`milestone_tiers.modes`).
//! Each one runs the user-facing `kalam` binary ([`crate::cli`]) on the dev corpus:
//!
//! - `roadmap.M2.typecheck`: every `run` case of T01–T15, all three skins, passes `kalam
//!   check` with no error, **and** `kalam check` runs the type checker (the K0602 probe;
//!   without it an accepted file proves nothing, so the checkpoint is `not_measurable`);
//! - `roadmap.M2.interpreter`: T01, T02, T05, T11–T14 run on `kalam run --interp` with the
//!   expected stdout and exit code (stdin from `stdin.txt` when `expected.txt` says so). T08
//!   is listed as conditional ("if it needs no float formatting") and reported separately;
//! - `roadmap.M2.negative`: every `compile-error` case of T48–T50, all three skins, gives
//!   exactly the expected error codes (`others: none`) on an expected line.
//!
//! T05, T10 and T13 are held out (LOOP.md §6.2): the harness never reads them, and the
//! held-out runner reports no checkpoint aggregate. A checkpoint whose scope includes a
//! held-out task can therefore be `fail` (a dev task fails) or `not_measurable`, never `pass`.

use crate::cli::KalamCli;
use crate::corpus::{self, Case};
use crate::parity::task_number;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Held-out MVP tasks (eval/corpus/INDEX.md §5): their numbers, never their contents.
pub const HELD_OUT_TASKS: &[u32] = &[5, 10, 13];

#[derive(Clone, Debug, PartialEq)]
pub struct Checkpoint {
    pub id: &'static str,
    pub target: String,
    /// `pass`, `fail` or `not_measurable`.
    pub status: &'static str,
    pub value: Value,
    pub evidence: String,
}

impl Checkpoint {
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "target": self.target, "status": self.status, "value": self.value, "evidence": self.evidence})
    }
}

/// The `--- stdout` section of an `expected.txt`, its `stdin:` and `exit:` lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunExpect {
    pub stdout: Option<String>,
    /// `stdin: file X` → `Some(X)`.
    pub stdin_file: Option<String>,
    pub exit: Option<i32>,
}

pub fn parse_run_expect(text: &str) -> RunExpect {
    let mut r = RunExpect::default();
    let mut lines = text.split_inclusive('\n');
    for line in lines.by_ref() {
        let l = line.trim_end_matches(['\n', '\r']);
        if l == "--- stdout" {
            r.stdout = Some(String::new());
            break;
        }
        if let Some(v) = l.strip_prefix("stdin:") {
            r.stdin_file = v.trim().strip_prefix("file ").map(|f| f.trim().to_string());
        } else if let Some(v) = l.strip_prefix("exit:") {
            r.exit = v.trim().parse().ok();
        }
    }
    if let Some(out) = r.stdout.as_mut() {
        for line in lines {
            if line.starts_with("--- ") {
                break;
            }
            out.push_str(line);
        }
    }
    r
}

fn in_tasks(c: &Case, tasks: &[u32]) -> bool {
    task_number(&c.task).is_some_and(|n| tasks.contains(&n))
}

fn held_out_in(tasks: &[u32]) -> Vec<String> {
    tasks
        .iter()
        .filter(|t| HELD_OUT_TASKS.contains(t))
        .map(|t| format!("T{t:02}"))
        .collect()
}

/// Judges dev results against a scope that may include held-out tasks.
fn verdict(failed: &[String], passed: usize, total: usize, held: &[String]) -> &'static str {
    if !failed.is_empty() {
        "fail"
    } else if total == 0 || !held.is_empty() {
        "not_measurable"
    } else {
        debug_assert_eq!(passed, total);
        "pass"
    }
}

fn read(c: &Case, f: &corpus::SkinFile) -> Result<Vec<u8>, String> {
    fs::read(&f.path).map_err(|e| format!("{}/{}: {e}", c.task, c.case))
}

pub fn typecheck(cli: &Result<KalamCli, String>, cases: &[Case]) -> Checkpoint {
    let tasks: Vec<u32> = (1..=15).collect();
    let target =
        "MVP-tier tasks T01-T15 type-check in all three skins against the stdlib signature stubs"
            .to_string();
    let nm = |why: String| Checkpoint {
        id: "roadmap.M2.typecheck",
        target: target.clone(),
        status: "not_measurable",
        value: Value::Null,
        evidence: why,
    };
    let cli = match cli {
        Err(w) => return nm(w.clone()),
        Ok(c) => c,
    };
    if let Err(w) = cli.typeck_available() {
        return nm(w);
    }
    let (mut passed, mut total, mut failed) = (0, 0, Vec::new());
    let mut seen = BTreeSet::new();
    for c in cases.iter().filter(|c| in_tasks(c, &tasks)) {
        if c.expect.kind.as_deref() != Some("run") {
            continue;
        }
        seen.insert(c.task.clone());
        for f in &c.files {
            total += 1;
            let r = read(c, f).and_then(|b| cli.check(f.skin, &b));
            match r {
                Ok(r) if r.errors.is_empty() && r.exit == Some(0) => passed += 1,
                Ok(r) => failed.push(format!(
                    "{}/{} {}: {:?}",
                    c.task,
                    c.case,
                    f.skin.id(),
                    r.errors
                        .iter()
                        .map(|d| format!("{}@{}", d.code, d.line))
                        .collect::<Vec<_>>()
                )),
                Err(e) => failed.push(format!("{}/{} {}: {e}", c.task, c.case, f.skin.id())),
            }
        }
    }
    let held = held_out_in(&tasks);
    Checkpoint {
        id: "roadmap.M2.typecheck",
        target,
        status: verdict(&failed, passed, total, &held),
        value: json!({"dev_files_passed": passed, "dev_files": total, "dev_tasks": seen.len()}),
        evidence: format!(
            "`{} check --json` per skin: dev {passed}/{total} files of {} tasks type-check; failures {:?}; held-out \
             tasks {held:?} are not run (the held-out runner has no type-check aggregate)",
            cli.exe.display(),
            seen.len(),
            failed.iter().take(5).collect::<Vec<_>>()
        ),
    }
}

pub fn interpreter(cli: &Result<KalamCli, String>, cases: &[Case]) -> Checkpoint {
    let tasks: Vec<u32> = vec![1, 2, 5, 11, 12, 13, 14];
    let conditional: Vec<u32> = vec![8];
    let target = "T01, T02, T05, T11-T14 (and T08 if it needs no float formatting) run on the reference interpreter with the expected output".to_string();
    let nm = |why: String| Checkpoint {
        id: "roadmap.M2.interpreter",
        target: target.clone(),
        status: "not_measurable",
        value: Value::Null,
        evidence: why,
    };
    let cli = match cli {
        Err(w) => return nm(w.clone()),
        Ok(c) => c,
    };
    if let Err(w) = cli.interp_available() {
        return nm(w);
    }
    let run_case = |c: &Case, failed: &mut Vec<String>| -> (usize, usize) {
        let text = fs::read_to_string(c.dir.join("expected.txt")).unwrap_or_default();
        let want = parse_run_expect(&text);
        let stdin = want
            .stdin_file
            .as_ref()
            .map(|f| fs::read(c.dir.join(f)).unwrap_or_default());
        let (mut p, mut t) = (0, 0);
        for f in &c.files {
            t += 1;
            let label = format!("{}/{} {}", c.task, c.case, f.skin.id());
            let Some(out) = &want.stdout else {
                failed.push(format!("{label}: expected.txt has no `--- stdout` section"));
                continue;
            };
            match read(c, f).and_then(|b| cli.run_interp(f.skin, &b, stdin.as_deref())) {
                Ok(r) if r.exit == Some(want.exit.unwrap_or(0)) && &r.stdout == out => p += 1,
                Ok(r) => failed.push(format!(
                    "{label}: exit {:?}, stdout matches: {}",
                    r.exit,
                    &r.stdout == out
                )),
                Err(e) => failed.push(format!("{label}: {e}")),
            }
        }
        (p, t)
    };
    let (mut passed, mut total, mut failed) = (0, 0, Vec::new());
    for c in cases
        .iter()
        .filter(|c| in_tasks(c, &tasks) && c.expect.kind.as_deref() == Some("run"))
    {
        let (p, t) = run_case(c, &mut failed);
        passed += p;
        total += t;
    }
    let (mut cp, mut ct, mut cfail) = (0, 0, Vec::new());
    for c in cases
        .iter()
        .filter(|c| in_tasks(c, &conditional) && c.expect.kind.as_deref() == Some("run"))
    {
        let (p, t) = run_case(c, &mut cfail);
        cp += p;
        ct += t;
    }
    let held = held_out_in(&tasks);
    Checkpoint {
        id: "roadmap.M2.interpreter",
        target,
        status: verdict(&failed, passed, total, &held),
        value: json!({"dev_files_passed": passed, "dev_files": total, "conditional_T08": {"passed": cp, "files": ct}}),
        evidence: format!(
            "`{} run --interp` per skin: dev {passed}/{total} files give the expected stdout and exit; failures {:?}; \
             T08 (conditional, not required) {cp}/{ct}{}; held-out tasks {held:?} are not run",
            cli.exe.display(),
            failed.iter().take(5).collect::<Vec<_>>(),
            cfail.first().map(|f| format!(" (first: {f})")).unwrap_or_default()
        ),
    }
}

pub fn negative(cli: &Result<KalamCli, String>, cases: &[Case]) -> Checkpoint {
    let tasks: Vec<u32> = vec![48, 49, 50];
    let target = "T48-T50 fail with exactly the expected codes".to_string();
    let cli = match cli {
        Err(w) => {
            return Checkpoint {
                id: "roadmap.M2.negative",
                target,
                status: "not_measurable",
                value: Value::Null,
                evidence: w.clone(),
            }
        }
        Ok(c) => c,
    };
    let (mut passed, mut total, mut failed) = (0, 0, Vec::new());
    for c in cases
        .iter()
        .filter(|c| in_tasks(c, &tasks) && c.expect.kind.as_deref() == Some("compile-error"))
    {
        for f in &c.files {
            total += 1;
            let label = format!("{}/{} {}", c.task, c.case, f.skin.id());
            let r = match read(c, f).and_then(|b| cli.check(f.skin, &b)) {
                Ok(r) => r,
                Err(e) => {
                    failed.push(format!("{label}: {e}"));
                    continue;
                }
            };
            let mut got: Vec<(String, usize)> =
                r.errors.iter().map(|d| (d.code.clone(), d.line)).collect();
            let mut problems = Vec::new();
            for want in &c.expect.diagnostics {
                match got.iter().position(|(code, line)| {
                    *code == want.code && (want.lines.is_empty() || want.lines.contains(line))
                }) {
                    Some(i) => {
                        got.remove(i);
                    }
                    None => problems.push(format!("missing {}@{:?}", want.code, want.lines)),
                }
            }
            if c.expect.others_none && !got.is_empty() {
                problems.push(format!("unexpected {got:?}"));
            }
            if c.expect.diagnostics.is_empty() {
                problems.push("expected.txt lists no diagnostic".into());
            }
            if problems.is_empty() {
                passed += 1;
            } else {
                failed.push(format!("{label}: {}", problems.join("; ")));
            }
        }
    }
    Checkpoint {
        id: "roadmap.M2.negative",
        target,
        status: verdict(&failed, passed, total, &[]),
        value: json!({"dev_files_passed": passed, "dev_files": total}),
        evidence: format!(
            "`{} check --json` per skin on every compile-error case: {passed}/{total} files give exactly the expected \
             codes on an expected line; failures {:?}",
            cli.exe.display(),
            failed.iter().take(6).collect::<Vec<_>>()
        ),
    }
}

/// All M2 checkpoints over the dev corpus at `corpus_dir`.
pub fn m2(cli: &Result<KalamCli, String>, corpus_dir: &Path) -> Vec<Checkpoint> {
    let cases = match corpus::discover(corpus_dir) {
        Ok(c) => c,
        Err(e) => {
            let why = format!("{}: {e}", corpus_dir.display());
            let cli_err: Result<KalamCli, String> = Err(why);
            return vec![
                typecheck(&cli_err, &[]),
                interpreter(&cli_err, &[]),
                negative(&cli_err, &[]),
            ];
        }
    };
    vec![
        typecheck(cli, &cases),
        interpreter(cli, &cases),
        negative(cli, &cases),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_run_expectations() {
        let t = "# kalam-expect/1\nkind: run\nstdin: file stdin.txt\nexit: 0\nstderr: ignore\n--- stdout\nHello\nمرحبا\n";
        let r = parse_run_expect(t);
        assert_eq!(r.stdout.as_deref(), Some("Hello\nمرحبا\n"));
        assert_eq!(r.stdin_file.as_deref(), Some("stdin.txt"));
        assert_eq!(r.exit, Some(0));
        let e = parse_run_expect("kind: run\nstdin: empty\n");
        assert_eq!(
            e,
            RunExpect {
                stdout: None,
                stdin_file: None,
                exit: None
            }
        );
    }

    #[test]
    fn held_out_scope_never_passes() {
        assert_eq!(verdict(&[], 3, 3, &["T05".into()]), "not_measurable");
        assert_eq!(verdict(&["x".into()], 2, 3, &["T05".into()]), "fail");
        assert_eq!(verdict(&[], 3, 3, &[]), "pass");
        assert_eq!(verdict(&[], 0, 0, &[]), "not_measurable");
        assert_eq!(held_out_in(&[1, 5, 13]), vec!["T05", "T13"]);
    }

    #[test]
    fn without_a_binary_every_checkpoint_is_not_measurable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../eval/corpus");
        let cps = m2(&Err("no kalam binary".into()), &root);
        assert_eq!(cps.len(), 3);
        assert!(cps.iter().all(|c| c.status == "not_measurable"), "{cps:?}");
    }
}
