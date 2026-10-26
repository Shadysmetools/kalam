//! The sealed held-out runner (LOOP.md §6.2).
//!
//! `kalam-eval holdout --root R --store S` runs in its own process. It reads the held-out
//! store, runs the same runners as for dev, and prints **one line of aggregate JSON and
//! nothing else**: counts and booleans, never a task name, case id, file name, diagnostic or
//! source text. A panic is silenced (a quiet panic hook, so no message reaches stderr), and
//! the parent never forwards the child's stderr.
//!
//! Store layout: held-out corpus tasks (`tNN-slug/`, the dev corpus format) under
//! `S/corpus/` or directly under `S/` (only `tNN-*` directories are tasks); held-out suite
//! splits (the shared suite format, `<case>/` files + `expect.json`) under
//! `S/suites/<suite>/` or `S/<suite>/`: `trojan-source` feeds `trojan`, `seeded-errors`
//! feeds `diag_recovery` (`null` when the split does not exist). Cases marked `[M2]` are
//! counted as `deferred_m2`, never judged (FOUNDER-QUEUE Q10), except Trojan Source cases
//! when the parent passes `--include-m2` (a target tier at or after M2, [`Runner::include_m2`]):
//! those are then judged like every other case. Files ending in `.age` mean the store is encrypted;
//! this runner has no decryption, so it reports `encrypted` and measures nothing.
//!
//! Aggregate (`kalam-holdout-aggregate/1`): the LOOP.md §6.2.4 keys, with pass counts so dev
//! and held-out results can be combined:
//!
//! ```text
//! {"schema", "status": "measured"|"empty"|"encrypted", "manifest_sha256",
//!  "m1_syntax": {"status", "tasks_passed", "tasks_total"},
//!  "parity": {"P1"|"P2"|"P3": {"pass": bool|null, "passed", "total"}},
//!  "fmt": {"passed", "total", "layout_kept", "excluded"},
//!  "trojan": {"bad_rejected", "bad_total", "good_accepted", "good_total", "deferred_m2", "defects",
//!             "not_measurable"} | null,
//!  "diag_recovery": {"value", "files", "score_sum", "full_skin_cases", "first_match", "cascade_free",
//!                    "deferred_m2", "defects", "unchecked"} | null,
//!  "typecheck": {"status": "measured"|"not_measurable", "files_passed", "files", "files_failed",
//!                "unchecked", "tasks", "type_checker"} | null,
//!  "leak": {"hits"}}
//! ```
//!
//! `typecheck` feeds `roadmap.M2.typecheck` ([`crate::checkpoints::held_typecheck`]): every
//! `run` case of the held-out T01-T15 tasks, all skins, through directory-mode `kalam check`
//! against `<root>/stdlib`, the same route as dev. It needs the `kalam` binary (`--kalam
//! PATH` or `KALAM_BIN`); without one it is `not_measurable`, and it is `null` when the
//! corpus could not be read.

use crate::checkpoints;
use crate::cli::KalamCli;
use crate::corpus;
use crate::corpus_run;
use crate::integrity::{self, tree_hash};
use crate::parity::{self, Outcome, ParseHook, SyntaxOutcome};
use crate::suites::{self, RejectRules};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SCHEMA: &str = "kalam-holdout-aggregate/1";

fn counts(o: &Outcome) -> Value {
    match o {
        Outcome::Unavailable(_) => json!({"pass": null, "passed": 0, "total": 0}),
        Outcome::Measured { passed, total, .. } => {
            json!({"pass": passed == total, "passed": passed, "total": total})
        }
    }
}

fn suite_dir(store: &Path, name: &str) -> Option<PathBuf> {
    [store.join("suites").join(name), store.join(name)]
        .into_iter()
        .find(|p| p.is_dir())
}

/// The aggregate for one store (runs inside the child process).
pub fn aggregate(
    store: &Path,
    root: &Path,
    hook: &dyn ParseHook,
    m1_scope: &[u32],
    include_m2: bool,
    kalam: Option<&Path>,
) -> Value {
    let manifest = tree_hash(store);
    let files = integrity::walk(store, &["."]);
    if files.is_empty() {
        return json!({"schema": SCHEMA, "status": "empty", "manifest_sha256": manifest});
    }
    if files.iter().any(|f| f.ends_with(".age")) {
        return json!({"schema": SCHEMA, "status": "encrypted", "manifest_sha256": manifest});
    }

    // corpus
    let corpus_dir = if store.join("corpus").is_dir() {
        store.join("corpus")
    } else {
        store.to_path_buf()
    };
    let (m1, par, fmt, needles) = match corpus_run::run(&corpus_dir, hook) {
        Err(_) => (
            json!({"status": "not_measurable", "tasks_passed": 0, "tasks_total": 0}),
            json!(null),
            json!(null),
            Vec::new(),
        ),
        Ok(run) => {
            let in_scope = |t: &str| parity::task_number(t).is_some_and(|n| m1_scope.contains(&n));
            let total = run.tasks().into_iter().filter(|t| in_scope(t)).count();
            let m1 = match &run.parity.syntax {
                SyntaxOutcome::Unavailable(_) => {
                    json!({"status": "not_measurable", "tasks_passed": 0, "tasks_total": total})
                }
                SyntaxOutcome::Measured {
                    passed,
                    format_unavailable,
                    ..
                } => json!({
                    "status": if format_unavailable.is_some() { "not_measurable" } else { "measured" },
                    "tasks_passed": passed.iter().filter(|t| in_scope(t)).count(),
                    "tasks_total": total,
                }),
            };
            let par = json!({
                "P1": counts(&run.parity.p1),
                "P2": counts(&run.parity.p2),
                "P3": counts(&run.parity.p3),
            });
            let fmt = json!({
                "passed": run.fmt.passed, "total": run.fmt.total,
                "layout_kept": run.fmt.layout_kept, "excluded": run.fmt.excluded,
            });
            let rel = corpus_dir
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            (m1, par, fmt, integrity::corpus_needles(&corpus_dir, &rel))
        }
    };

    let typecheck = corpus::discover(&corpus_dir)
        .map(|cases| {
            let std = root.join(crate::m2::STDLIB_DIR);
            checkpoints::held_typecheck(&KalamCli::locate(kalam), &cases, &std)
        })
        .unwrap_or(Value::Null);

    // suites
    let load = |name: &str| suite_dir(store, name).and_then(|d| suites::load(&d).ok().flatten());
    let trojan = load(suites::TROJAN).map(|cases| {
        let rules = RejectRules {
            name_codepoints: true,
            out_of_scope: vec![],
            include_m2,
        };
        let r = suites::run_reject(hook, &cases, &rules);
        json!({
            "bad_rejected": r.bad_passed, "bad_total": r.bad_total,
            "good_accepted": r.good_passed, "good_total": r.good_total,
            "deferred_m2": r.deferred_m2, "defects": r.defects.len(),
            "not_measurable": r.not_measurable.len(),
        })
    });
    let recovery = load(suites::SEEDED).map(|cases| {
        let r = suites::run_recovery(hook, &cases);
        json!({
            "value": r.value(), "files": r.files, "score_sum": r.score_sum,
            "full_skin_cases": r.full_skin_cases, "first_match": r.first_match,
            "cascade_free": r.cascade_free, "deferred_m2": r.deferred_m2,
            "defects": r.defects.len(), "unchecked": r.unchecked,
        })
    });

    json!({
        "schema": SCHEMA,
        "status": "measured",
        "manifest_sha256": manifest,
        "m1_syntax": m1,
        "parity": par,
        "fmt": fmt,
        "trojan": trojan,
        "diag_recovery": recovery,
        "typecheck": typecheck,
        "leak": {"hits": integrity::scan_sources(root, &needles).len()},
    })
}

/// How the parent starts the held-out runner.
#[derive(Clone, Debug)]
pub struct Runner {
    /// The `kalam-eval` binary (normally `std::env::current_exe()`).
    pub exe: PathBuf,
    /// The store, relative to the repository root or absolute.
    pub store: PathBuf,
    /// Judge the `[M2]` Trojan Source cases (target tier M2 or later, FOUNDER-QUEUE Q10);
    /// passed to the child as `--include-m2`.
    pub include_m2: bool,
    /// The `kalam` binary for the held-out type-check aggregate, passed as `--kalam` (the
    /// child also honours `KALAM_BIN`).
    pub kalam: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Held {
    /// No store (missing or empty directory): nothing held-out can be judged.
    Missing(String),
    /// The runner failed; its output is withheld.
    Failed(String),
    Ran(Value),
}

impl Held {
    pub fn aggregate(&self) -> Option<&Value> {
        match self {
            Held::Ran(v) if v["status"] == "measured" => Some(v),
            _ => None,
        }
    }

    pub fn why_not(&self) -> String {
        match self {
            Held::Missing(w) | Held::Failed(w) => w.clone(),
            Held::Ran(v) => format!("the held-out store is {}", v["status"]),
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Held::Ran(v) => v.clone(),
            other => json!({"status": "not_run", "reason": other.why_not()}),
        }
    }
}

/// Runs the held-out runner as a child process and keeps only its aggregate.
pub fn run_child(runner: &Runner, root: &Path) -> Held {
    let store = root.join(&runner.store);
    if tree_hash(&store).is_none() {
        return Held::Missing(format!(
            "no held-out store at {} (missing or empty)",
            runner.store.display()
        ));
    }
    let mut cmd = Command::new(&runner.exe);
    cmd.arg("holdout")
        .arg("--root")
        .arg(root)
        .arg("--store")
        .arg(&store);
    if runner.include_m2 {
        cmd.arg("--include-m2");
    }
    if let Some(k) = &runner.kalam {
        cmd.arg("--kalam").arg(k);
    }
    let out = cmd.output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return Held::Failed(format!("could not start the held-out runner: {e}")),
    };
    let parsed = serde_json::from_slice::<Value>(&out.stdout)
        .ok()
        .filter(|v| v["schema"] == SCHEMA);
    match (out.status.success(), parsed) {
        (true, Some(v)) => Held::Ran(v),
        (ok, _) => Held::Failed(format!(
            "the held-out runner {} (its output is withheld)",
            if ok {
                "printed no aggregate"
            } else {
                "exited with an error"
            }
        )),
    }
}
