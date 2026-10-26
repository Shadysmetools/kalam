//! The M2 roadmap checkpoints (scorecard.json `milestone_tiers` M2 `checkpoints`).
//!
//! Checkpoints are reported in the milestone summary and judged by the founder; they are
//! never tier checks, never in the tier score and never ratcheted (`milestone_tiers.modes`).
//! Each one runs the user-facing `kalam` binary ([`crate::cli`]) on the dev corpus:
//!
//! - `roadmap.M2.typecheck`: every `run` case of T01–T15, all three skins, passes `kalam
//!   check` in directory mode against the stdlib signature stubs (`kalam check --json --std
//!   <root>/stdlib PROJECT`, the file staged as `PROJECT/src/main<ext>`) with no error in the
//!   file or in the stdlib, **and** that mode runs the type checker (the K0602 probe). An
//!   error is a `fail` whether or not the type checker runs (a file that does not resolve
//!   does not type-check); without the type checker a clean file proves nothing, so the
//!   checkpoint is then `not_measurable`, with the name-resolution counts as evidence;
//! - `roadmap.M2.interpreter`: T01, T02, T05, T11–T14 run on `kalam run --interp` with the
//!   expected stdout and exit code (stdin from `stdin.txt` when `expected.txt` says so). T08
//!   is listed as conditional ("if it needs no float formatting") and reported separately;
//! - `roadmap.M2.negative`: every `compile-error` case of T48–T50, all three skins, gives
//!   exactly the expected error codes (`others: none`) on an expected line, through the same
//!   directory mode against the stdlib (a stdlib error is an unexpected code).
//!
//! T05, T10 and T13 are held out (LOOP.md §6.2): the harness never reads them. The sealed
//! held-out runner reports counts only, through the same route as dev:
//!
//! - for `roadmap.M2.typecheck`, [`held_typecheck`] (files checked, passed and failed, tasks
//!   seen, whether the type checker ran);
//! - for `roadmap.M2.interpreter`, [`held_interpreter`] (the held-out T05 and T13 `run`
//!   files on `kalam run --interp`: files run, passed and failed, files that could not be
//!   run, tasks seen, whether the interpreter is available).
//!
//! Each of those checkpoints passes only when the dev files and every held-out file pass,
//! with every held-out task in its scope seen; a held-out failure fails it. `roadmap.M2.negative`
//! has no held-out task.

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

/// Errors that belong to a stdlib file rather than to the staged project.
fn stdlib_errors(errors: &[crate::cli::CliDiag]) -> usize {
    errors
        .iter()
        .filter(|d| {
            !d.file.is_empty() && !d.file.starts_with("./src/") && !d.file.starts_with("src/")
        })
        .count()
}

/// What directory-mode `kalam check` made of the `run` cases of T01-T15 (dev or held-out).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypecheckCounts {
    pub passed: usize,
    pub total: usize,
    /// `task/case skin: ...` for each file with an error (dev evidence only: these name
    /// tasks and cases, so the held-out aggregate keeps only their number).
    pub failed: Vec<String>,
    /// Files the binary could not check at all (an infrastructure error, not a verdict).
    pub unchecked: Vec<String>,
    pub from_std: usize,
    pub tasks: BTreeSet<String>,
}

/// The tasks of `roadmap.M2.typecheck`.
fn typecheck_tasks() -> Vec<u32> {
    (1..=15).collect()
}

/// Checks every `run` case of T01-T15 in `cases`, every skin, in directory mode against
/// the stdlib at `std`.
pub fn typecheck_counts(cli: &KalamCli, cases: &[Case], std: &Path) -> TypecheckCounts {
    let tasks = typecheck_tasks();
    let mut n = TypecheckCounts::default();
    for c in cases.iter().filter(|c| in_tasks(c, &tasks)) {
        if c.expect.kind.as_deref() != Some("run") {
            continue;
        }
        n.tasks.insert(c.task.clone());
        for f in &c.files {
            n.total += 1;
            let r = read(c, f).and_then(|b| cli.check_in_project(f.skin, &b, std));
            match r {
                Ok(r) if r.errors.is_empty() && r.exit == Some(0) => n.passed += 1,
                Ok(r) => {
                    n.from_std += stdlib_errors(&r.errors);
                    n.failed.push(format!(
                        "{}/{} {}: exit {:?} {:?}",
                        c.task,
                        c.case,
                        f.skin.id(),
                        r.exit,
                        r.errors
                            .iter()
                            .map(|d| format!("{}@{}:{}", d.code, d.file, d.line))
                            .collect::<Vec<_>>()
                    ))
                }
                Err(e) => n
                    .unchecked
                    .push(format!("{}/{} {}: {e}", c.task, c.case, f.skin.id())),
            }
        }
    }
    n
}

/// The held-out `typecheck` aggregate (runs inside the sealed child): counts and booleans
/// only, never a task, case, file, code or message.
pub fn held_typecheck(cli: &Result<KalamCli, String>, cases: &[Case], std: &Path) -> Value {
    let Ok(cli) = cli else {
        return json!({"status": "not_measurable", "files_passed": 0, "files": 0,
                      "files_failed": 0, "unchecked": 0, "tasks": 0, "type_checker": false});
    };
    let typeck = cli.typeck_available_in_project(std).is_ok();
    let n = typecheck_counts(cli, cases, std);
    json!({
        "status": "measured",
        "files_passed": n.passed,
        "files": n.total,
        "files_failed": n.failed.len(),
        "unchecked": n.unchecked.len(),
        "tasks": n.tasks.len(),
        "type_checker": typeck,
    })
}

/// The held-out `typecheck` aggregate as the checkpoint reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldTypecheck {
    pub files_passed: u64,
    pub files: u64,
    pub files_failed: u64,
    pub unchecked: u64,
    pub tasks: u64,
    pub type_checker: bool,
}

impl HeldTypecheck {
    /// `None` unless the aggregate was measured.
    pub fn from_json(v: &Value) -> Option<HeldTypecheck> {
        if v["status"] != "measured" {
            return None;
        }
        let n = |k: &str| v[k].as_u64();
        Some(HeldTypecheck {
            files_passed: n("files_passed")?,
            files: n("files")?,
            files_failed: n("files_failed")?,
            unchecked: n("unchecked")?,
            tasks: n("tasks")?,
            type_checker: v["type_checker"].as_bool()?,
        })
    }
}

/// `held`: the held-out aggregate's `typecheck` value, `None` when the held-out runner did
/// not run or reported none.
pub fn typecheck(
    cli: &Result<KalamCli, String>,
    cases: &[Case],
    std: &Path,
    held_agg: Option<&Value>,
) -> Checkpoint {
    let tasks = typecheck_tasks();
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
    let typeck = cli.typeck_available_in_project(std);
    let n = typecheck_counts(cli, cases, std);
    let held = held_out_in(&tasks);
    let held_run = held_agg.and_then(HeldTypecheck::from_json);
    let status = typecheck_status(
        &n.failed,
        &n.unchecked,
        typeck.is_ok(),
        n.passed,
        n.total,
        &held,
        held_run,
    );
    let (passed, total, from_std) = (n.passed, n.total, n.from_std);
    Checkpoint {
        id: "roadmap.M2.typecheck",
        target,
        status,
        value: json!({"dev_files_passed": passed, "dev_files": total, "dev_tasks": n.tasks.len(),
                      "type_checker": typeck.is_ok(), "stdlib_errors": from_std,
                      "held_out": held_agg.cloned().unwrap_or(Value::Null)}),
        evidence: format!(
            "`{} check --json --std {} PROJECT` per skin (file staged as src/main<ext>): dev {passed}/{total} files of \
             {} tasks have no error{}; errors in stdlib files {from_std}; failures {:?}; could not be checked {} {:?}; \
             held-out tasks {held:?}: {}",
            cli.exe.display(),
            std.display(),
            n.tasks.len(),
            match &typeck {
                Ok(()) => " (type checker on)".to_string(),
                Err(w) => format!(" under name resolution only, so a clean file proves nothing yet: {w}"),
            },
            n.failed.iter().take(5).collect::<Vec<_>>(),
            n.unchecked.len(),
            n.unchecked.iter().take(2).collect::<Vec<_>>(),
            match held_run {
                None => "the held-out runner reported no measured type-check aggregate".to_string(),
                Some(h) => format!(
                    "{}/{} files of {} tasks have no error, {} failed, {} could not be checked (type checker {})",
                    h.files_passed,
                    h.files,
                    h.tasks,
                    h.files_failed,
                    h.unchecked,
                    if h.type_checker { "on" } else { "off" }
                ),
            }
        ),
    }
}

/// `roadmap.M2.typecheck`: an error in a checked file (dev or held-out) fails it; anything
/// unchecked, or no type checker, leaves it not measurable; a scope with held-out tasks
/// passes only on a measured held-out aggregate in which every file passed and every
/// held-out task was seen.
fn typecheck_status(
    failed: &[String],
    unchecked: &[String],
    typeck: bool,
    passed: usize,
    total: usize,
    held: &[String],
    held_run: Option<HeldTypecheck>,
) -> &'static str {
    if !failed.is_empty() || held_run.is_some_and(|h| h.files_failed > 0) {
        return "fail";
    }
    if !unchecked.is_empty() || !typeck {
        return "not_measurable";
    }
    if held.is_empty() {
        return verdict(failed, passed, total, held);
    }
    match held_run {
        Some(h)
            if h.type_checker
                && h.unchecked == 0
                && h.files > 0
                && h.files_passed == h.files
                && h.tasks >= held.len() as u64 =>
        {
            verdict(failed, passed, total, &[])
        }
        _ => "not_measurable",
    }
}

/// The tasks of `roadmap.M2.interpreter`, and the conditional T08.
const INTERP_TASKS: &[u32] = &[1, 2, 5, 11, 12, 13, 14];
const INTERP_CONDITIONAL: &[u32] = &[8];

/// What `kalam run --interp` made of the `run` cases of `tasks` (dev or held-out).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InterpCounts {
    pub passed: usize,
    pub total: usize,
    /// `task/case skin: ...` per file with the wrong stdout or exit, or a case without a
    /// `--- stdout` section (dev evidence only: the held-out aggregate keeps the number).
    pub failed: Vec<String>,
    /// Files the binary could not run at all (an infrastructure error, not a verdict).
    pub unchecked: Vec<String>,
    pub tasks: BTreeSet<String>,
}

/// Runs every `run` case of `tasks` in `cases`, every skin, on `kalam run --interp` and
/// compares stdout and the exit code with `expected.txt` (stdin from its `stdin: file`).
pub fn interp_counts(cli: &KalamCli, cases: &[Case], tasks: &[u32]) -> InterpCounts {
    let mut n = InterpCounts::default();
    for c in cases
        .iter()
        .filter(|c| in_tasks(c, tasks) && c.expect.kind.as_deref() == Some("run"))
    {
        n.tasks.insert(c.task.clone());
        let text = fs::read_to_string(c.dir.join("expected.txt")).unwrap_or_default();
        let want = parse_run_expect(&text);
        let stdin = want
            .stdin_file
            .as_ref()
            .map(|f| fs::read(c.dir.join(f)).unwrap_or_default());
        for f in &c.files {
            n.total += 1;
            let label = format!("{}/{} {}", c.task, c.case, f.skin.id());
            let Some(out) = &want.stdout else {
                n.failed
                    .push(format!("{label}: expected.txt has no `--- stdout` section"));
                continue;
            };
            match read(c, f).and_then(|b| cli.run_interp(f.skin, &b, stdin.as_deref())) {
                Ok(r) if r.exit == Some(want.exit.unwrap_or(0)) && &r.stdout == out => {
                    n.passed += 1
                }
                Ok(r) => n.failed.push(format!(
                    "{label}: exit {:?}, stdout matches: {}",
                    r.exit,
                    &r.stdout == out
                )),
                Err(e) => n.unchecked.push(format!("{label}: {e}")),
            }
        }
    }
    n
}

/// The held-out `interpreter` aggregate (runs inside the sealed child): counts and booleans
/// only, never a task, case, file, stdout or message.
pub fn held_interpreter(cli: &Result<KalamCli, String>, cases: &[Case]) -> Value {
    let none = |interp: bool| {
        json!({"status": "not_measurable", "files_passed": 0, "files": 0, "files_failed": 0,
               "unchecked": 0, "tasks": 0, "interpreter": interp})
    };
    let Ok(cli) = cli else {
        return none(false);
    };
    if cli.interp_available().is_err() {
        return none(false);
    }
    let n = interp_counts(cli, cases, INTERP_TASKS);
    json!({
        "status": "measured",
        "files_passed": n.passed,
        "files": n.total,
        "files_failed": n.failed.len(),
        "unchecked": n.unchecked.len(),
        "tasks": n.tasks.len(),
        "interpreter": true,
    })
}

/// The held-out `interpreter` aggregate as the checkpoint reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldInterp {
    pub files_passed: u64,
    pub files: u64,
    pub files_failed: u64,
    pub unchecked: u64,
    pub tasks: u64,
}

impl HeldInterp {
    /// `None` unless the aggregate was measured with the interpreter available.
    pub fn from_json(v: &Value) -> Option<HeldInterp> {
        if v["status"] != "measured" || v["interpreter"] != true {
            return None;
        }
        let n = |k: &str| v[k].as_u64();
        Some(HeldInterp {
            files_passed: n("files_passed")?,
            files: n("files")?,
            files_failed: n("files_failed")?,
            unchecked: n("unchecked")?,
            tasks: n("tasks")?,
        })
    }
}

/// `roadmap.M2.interpreter`: a wrong output (dev or held-out) fails it; a file that could
/// not be run leaves it not measurable; with held-out tasks in scope it passes only on a
/// measured held-out aggregate in which every file passed and every held-out task was seen.
fn interpreter_status(
    dev: &InterpCounts,
    held: &[String],
    held_run: Option<HeldInterp>,
) -> &'static str {
    if !dev.failed.is_empty() || held_run.is_some_and(|h| h.files_failed > 0) {
        return "fail";
    }
    if !dev.unchecked.is_empty() {
        return "not_measurable";
    }
    if held.is_empty() {
        return verdict(&[], dev.passed, dev.total, &[]);
    }
    match held_run {
        Some(h)
            if h.unchecked == 0
                && h.files > 0
                && h.files_passed == h.files
                && h.tasks >= held.len() as u64 =>
        {
            verdict(&[], dev.passed, dev.total, &[])
        }
        _ => "not_measurable",
    }
}

/// `held_agg`: the held-out aggregate's `interpreter` value, `None` when the held-out runner
/// did not run or reported none.
pub fn interpreter(
    cli: &Result<KalamCli, String>,
    cases: &[Case],
    held_agg: Option<&Value>,
) -> Checkpoint {
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
    let dev = interp_counts(cli, cases, INTERP_TASKS);
    let cond = interp_counts(cli, cases, INTERP_CONDITIONAL);
    let held = held_out_in(INTERP_TASKS);
    let held_run = held_agg.and_then(HeldInterp::from_json);
    let (passed, total) = (dev.passed, dev.total);
    let (cp, ct) = (cond.passed, cond.total);
    Checkpoint {
        id: "roadmap.M2.interpreter",
        target,
        status: interpreter_status(&dev, &held, held_run),
        value: json!({"dev_files_passed": passed, "dev_files": total, "dev_tasks": dev.tasks.len(),
                      "conditional_T08": {"passed": cp, "files": ct},
                      "held_out": held_agg.cloned().unwrap_or(Value::Null)}),
        evidence: format!(
            "`{} run --interp` per skin: dev {passed}/{total} files of {} tasks give the expected stdout and exit; \
             failures {:?}; could not be run {} {:?}; T08 (conditional, not required) {cp}/{ct}{}; held-out tasks {held:?}: {}",
            cli.exe.display(),
            dev.tasks.len(),
            dev.failed.iter().take(5).collect::<Vec<_>>(),
            dev.unchecked.len(),
            dev.unchecked.iter().take(2).collect::<Vec<_>>(),
            cond.failed
                .iter()
                .chain(&cond.unchecked)
                .next()
                .map(|f| format!(" (first: {f})"))
                .unwrap_or_default(),
            match held_run {
                None => "the held-out runner reported no measured interpreter aggregate".to_string(),
                Some(h) => format!(
                    "{}/{} files of {} tasks give the expected output, {} wrong, {} could not be run",
                    h.files_passed, h.files, h.tasks, h.files_failed, h.unchecked
                ),
            }
        ),
    }
}

pub fn negative(cli: &Result<KalamCli, String>, cases: &[Case], std: &Path) -> Checkpoint {
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
    let mut unchecked: Vec<String> = Vec::new();
    for c in cases
        .iter()
        .filter(|c| in_tasks(c, &tasks) && c.expect.kind.as_deref() == Some("compile-error"))
    {
        for f in &c.files {
            total += 1;
            let label = format!("{}/{} {}", c.task, c.case, f.skin.id());
            let r = match read(c, f).and_then(|b| cli.check_in_project(f.skin, &b, std)) {
                Ok(r) => r,
                Err(e) => {
                    unchecked.push(format!("{label}: {e}"));
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
        status: typecheck_status(&failed, &unchecked, true, passed, total, &[], None),
        value: json!({"dev_files_passed": passed, "dev_files": total, "unchecked": unchecked.len()}),
        evidence: format!(
            "`{} check --json --std {} PROJECT` per skin on every compile-error case: {passed}/{total} files give \
             exactly the expected codes on an expected line; failures {:?}; could not be checked {} {:?}",
            cli.exe.display(),
            std.display(),
            failed.iter().take(6).collect::<Vec<_>>(),
            unchecked.len(),
            unchecked.iter().take(2).collect::<Vec<_>>()
        ),
    }
}

/// All M2 checkpoints over the dev corpus at `corpus_dir`, against the stdlib at `std`;
/// `held` is the held-out runner's aggregate (its `typecheck` and `interpreter` parts), if
/// it ran.
pub fn m2(
    cli: &Result<KalamCli, String>,
    corpus_dir: &Path,
    std: &Path,
    held: Option<&Value>,
) -> Vec<Checkpoint> {
    let part = |k: &str| held.map(|a| &a[k]).filter(|v| !v.is_null());
    let (held_typecheck, held_interp) = (part("typecheck"), part("interpreter"));
    let cases = match corpus::discover(corpus_dir) {
        Ok(c) => c,
        Err(e) => {
            let why = format!("{}: {e}", corpus_dir.display());
            let cli_err: Result<KalamCli, String> = Err(why);
            return vec![
                typecheck(&cli_err, &[], std, held_typecheck),
                interpreter(&cli_err, &[], held_interp),
                negative(&cli_err, &[], std),
            ];
        }
    };
    vec![
        typecheck(cli, &cases, std, held_typecheck),
        interpreter(cli, &cases, held_interp),
        negative(cli, &cases, std),
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
    fn a_binary_that_cannot_check_is_not_a_failure() {
        let x = vec!["x".to_string()];
        assert_eq!(
            typecheck_status(&[], &x, true, 0, 1, &[], None),
            "not_measurable"
        );
        assert_eq!(typecheck_status(&x, &x, true, 0, 2, &[], None), "fail");
        // an error is a failure even under name resolution only
        assert_eq!(typecheck_status(&x, &[], false, 0, 1, &[], None), "fail");
        // a clean run without the type checker proves nothing
        assert_eq!(
            typecheck_status(&[], &[], false, 3, 3, &[], None),
            "not_measurable"
        );
        assert_eq!(typecheck_status(&[], &[], true, 3, 3, &[], None), "pass");
        assert_eq!(
            typecheck_status(&[], &[], true, 3, 3, &["T05".into()], None),
            "not_measurable"
        );
    }

    fn held(passed: u64, files: u64, failed: u64, tasks: u64) -> HeldTypecheck {
        HeldTypecheck {
            files_passed: passed,
            files,
            files_failed: failed,
            unchecked: files - passed - failed,
            tasks,
            type_checker: true,
        }
    }

    #[test]
    fn the_held_out_aggregate_decides_a_held_out_scope() {
        let h3 = ["T05".to_string(), "T10".into(), "T13".into()];
        let st = |h: Option<HeldTypecheck>| typecheck_status(&[], &[], true, 36, 36, &h3, h);
        // every held-out file of every held-out task passes: pass
        assert_eq!(st(Some(held(9, 9, 0, 3))), "pass");
        // a held-out failure fails it, even with dev clean
        assert_eq!(st(Some(held(8, 9, 1, 3))), "fail");
        // an unchecked held-out file, a missing task, no type checker or no aggregate: unknown
        assert_eq!(st(Some(held(8, 9, 0, 3))), "not_measurable");
        assert_eq!(st(Some(held(6, 6, 0, 2))), "not_measurable");
        assert_eq!(st(Some(held(0, 0, 0, 0))), "not_measurable");
        let off = HeldTypecheck {
            type_checker: false,
            ..held(9, 9, 0, 3)
        };
        assert_eq!(st(Some(off)), "not_measurable");
        assert_eq!(st(None), "not_measurable");
        // a dev failure still fails
        assert_eq!(
            typecheck_status(
                &["x".into()],
                &[],
                true,
                35,
                36,
                &h3,
                Some(held(9, 9, 0, 3))
            ),
            "fail"
        );
    }

    #[test]
    fn the_held_out_aggregate_round_trips_and_names_nothing() {
        let v = held_typecheck(&Err("no kalam binary".into()), &[], Path::new("std"));
        assert_eq!(v["status"], "not_measurable");
        assert!(HeldTypecheck::from_json(&v).is_none());
        let measured = json!({"status": "measured", "files_passed": 9, "files": 9,
                              "files_failed": 0, "unchecked": 0, "tasks": 3, "type_checker": true});
        assert_eq!(HeldTypecheck::from_json(&measured), Some(held(9, 9, 0, 3)));
        // counts and booleans only
        for (_, x) in measured.as_object().unwrap() {
            assert!(x.is_u64() || x.is_boolean() || x == "measured");
        }
    }

    fn dev(passed: usize, total: usize, failed: usize, unchecked: usize) -> InterpCounts {
        InterpCounts {
            passed,
            total,
            failed: vec!["x".into(); failed],
            unchecked: vec!["x".into(); unchecked],
            tasks: BTreeSet::new(),
        }
    }

    fn hi(passed: u64, files: u64, failed: u64, tasks: u64) -> HeldInterp {
        HeldInterp {
            files_passed: passed,
            files,
            files_failed: failed,
            unchecked: files - passed - failed,
            tasks,
        }
    }

    #[test]
    fn the_held_out_aggregate_decides_the_interpreter_checkpoint() {
        // T05 and T13 are the held-out tasks in this scope (T10 is not)
        let held = held_out_in(INTERP_TASKS);
        assert_eq!(held, vec!["T05", "T13"]);
        let st = |d: InterpCounts, h: Option<HeldInterp>| interpreter_status(&d, &held, h);
        assert_eq!(st(dev(15, 15, 0, 0), Some(hi(6, 6, 0, 2))), "pass");
        // a wrong held-out output fails it, even with dev clean
        assert_eq!(st(dev(15, 15, 0, 0), Some(hi(5, 6, 1, 2))), "fail");
        // a dev failure fails it whatever the held-out side says
        assert_eq!(st(dev(14, 15, 1, 0), None), "fail");
        // unknowns: a held-out file not run, a held-out task missing, no files, no aggregate
        assert_eq!(
            st(dev(15, 15, 0, 0), Some(hi(5, 6, 0, 2))),
            "not_measurable"
        );
        assert_eq!(
            st(dev(15, 15, 0, 0), Some(hi(3, 3, 0, 1))),
            "not_measurable"
        );
        assert_eq!(
            st(dev(15, 15, 0, 0), Some(hi(0, 0, 0, 0))),
            "not_measurable"
        );
        assert_eq!(st(dev(15, 15, 0, 0), None), "not_measurable");
        // a dev file that could not be run is not a verdict
        assert_eq!(
            st(dev(14, 15, 0, 1), Some(hi(6, 6, 0, 2))),
            "not_measurable"
        );
        // a scope without held-out tasks is judged on dev alone
        assert_eq!(interpreter_status(&dev(3, 3, 0, 0), &[], None), "pass");
        assert_eq!(
            interpreter_status(&dev(0, 0, 0, 0), &[], None),
            "not_measurable"
        );
    }

    #[test]
    fn the_held_out_interpreter_aggregate_round_trips_and_names_nothing() {
        let v = held_interpreter(&Err("no kalam binary".into()), &[]);
        assert_eq!(v["status"], "not_measurable");
        assert!(HeldInterp::from_json(&v).is_none());
        let measured = json!({"status": "measured", "files_passed": 6, "files": 6,
                              "files_failed": 0, "unchecked": 0, "tasks": 2, "interpreter": true});
        assert_eq!(HeldInterp::from_json(&measured), Some(hi(6, 6, 0, 2)));
        let mut off = measured.clone();
        off["interpreter"] = json!(false);
        assert!(HeldInterp::from_json(&off).is_none());
        for (_, x) in measured.as_object().unwrap() {
            assert!(x.is_u64() || x.is_boolean() || x == "measured");
        }
    }

    #[test]
    fn without_a_binary_every_checkpoint_is_not_measurable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../eval/corpus");
        let cps = m2(
            &Err("no kalam binary".into()),
            &root,
            &root.join("../../stdlib"),
            None,
        );
        assert_eq!(cps.len(), 3);
        assert!(cps.iter().all(|c| c.status == "not_measurable"), "{cps:?}");
    }
}
