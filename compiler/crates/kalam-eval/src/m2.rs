//! The M2 runners wired into a scorecard run: spec clause coverage, conformance, goldens,
//! machine-fix rate, the judge score, the M2-scope versions of four M1 checks, and the M2
//! roadmap checkpoints. Each one reports `not_measurable` with its reason unless its whole
//! definition was measured (LOOP.md A2).
//!
//! The four checks M2 repeats with a wider scope (scorecard.json `milestone_tiers` M2):
//!
//! - `D5.diag_completeness`: no scope limit, so the denominator is every spec §27 code (all
//!   families, retired dropped) plus every catalog code; unimplemented spec codes count as
//!   incomplete (the metric's own definition);
//! - `D2.lexing_hazard_suite` "adds the SEC0104 post-fold collision cases": the suite is
//!   judged with its `[M2]` cases and with SEC0104 in scope;
//! - `D1.fuzz_crash_free_hours` "adds the type-checker target": as at M1, plus the running
//!   fuzz binary must list a type-checker target in `eval/fuzz/status.json` `targets` (a
//!   name containing `typeck` or `typecheck`, ignoring case, `-`, `_` and spaces). The
//!   status file is the only source: a target that exists in the repository but is not in
//!   the running binary's list does not count, and a status file without `targets` is not
//!   measurable. Until the target is listed the check is not measurable;
//! - `G-PAR.P8` "K0201 and K0301": the K0201 part is the M1 observation (eval/suites/
//!   mixed-skin). The K0301 part runs [`K0301_CASES`] through the user-facing `kalam check
//!   --json --std <root>/stdlib PROJECT` (directory mode, FOUNDER-QUEUE Q17): each case is
//!   one project, judged as a whole, because K0301 is reported on the second file of the
//!   module only. P8 passes only when the K0201 part passes and every K0301 case (must-reject
//!   and control) passes; any failure fails it; anything unmeasured leaves it not measurable.
//!
//! The K0301 cases are built into the harness from spec §4.1 ("If `users.kl` and
//! `users.ar.kl` both exist in one directory, that is K0301") because the shared suite
//! format judges each file alone and has no project case.

use crate::checkpoints::{self, Checkpoint};
use crate::clauses::{self, Unit};
use crate::cli::{CliCheck, KalamCli};
use crate::conformance::{self, ConfRun, Coverage, Stages};
use crate::diagcat::{self, CatalogReport};
use crate::external::{FuzzStatus, FUZZ_STATUS};
use crate::goldens::{self, FixRun, GoldenFile, GoldenRun};
use crate::judge;
use crate::parity::ParseHook;
use crate::report::Measure;
use crate::scorecard::MetricValue;
use crate::suites::{self, RejectResult, RejectRules, SuiteCase};
use crate::tiers::Observation;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// One directory-mode project for the K0301 part of `G-PAR.P8` (module docs).
#[derive(Clone, Copy, Debug)]
pub struct ProjectCase {
    pub id: &'static str,
    /// Paths relative to the project's `src/`, and their text.
    pub files: &'static [(&'static str, &'static str)],
    /// `true`: K0301 must be reported; `false` (a control): no error at all.
    pub k0301: bool,
}

const EN: &str = "fn main():\n    print(\"x\")\n";
const AR: &str = "دالة رئيسي():\n    اطبع(\"x\")\n";
const FRANCO: &str = "dala ra2isi():\n    ektb(\"x\")\n";

/// Spec §4.1: two skin files of one module are K0301, for every skin pair, in a nested
/// directory too; files of different modules (different names, or one name in different
/// directories) are not.
pub const K0301_CASES: &[ProjectCase] = &[
    ProjectCase {
        id: "en-ar",
        files: &[("users.kl", EN), ("users.ar.kl", AR)],
        k0301: true,
    },
    ProjectCase {
        id: "en-franco",
        files: &[("users.kl", EN), ("users.franco.kl", FRANCO)],
        k0301: true,
    },
    ProjectCase {
        id: "ar-franco",
        files: &[("users.ar.kl", AR), ("users.franco.kl", FRANCO)],
        k0301: true,
    },
    ProjectCase {
        id: "nested-three-skins",
        files: &[
            ("web/views.kl", EN),
            ("web/views.ar.kl", AR),
            ("web/views.franco.kl", FRANCO),
        ],
        k0301: true,
    },
    ProjectCase {
        id: "control-distinct-modules",
        files: &[("a.kl", EN), ("b.ar.kl", AR), ("c.franco.kl", FRANCO)],
        k0301: false,
    },
    ProjectCase {
        id: "control-same-name-other-directory",
        files: &[("a/users.kl", EN), ("b/users.ar.kl", AR)],
        k0301: false,
    },
];

/// Judges one K0301 project result (module docs).
pub fn judge_project(case: &ProjectCase, r: &CliCheck) -> Result<(), String> {
    let codes: Vec<String> = r
        .errors
        .iter()
        .map(|d| format!("{}@{}", d.code, d.file))
        .collect();
    let ok = if case.k0301 {
        r.exit == Some(1) && r.errors.iter().any(|d| d.code == "K0301")
    } else {
        r.exit == Some(0) && r.errors.is_empty()
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{} ({}): exit {:?}, errors {codes:?}",
            case.id,
            if case.k0301 {
                "must give K0301"
            } else {
                "control: must give no error"
            },
            r.exit
        ))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectRun {
    pub must_reject: usize,
    pub controls: usize,
    pub passed: usize,
    pub failures: Vec<String>,
}

impl ProjectRun {
    pub fn total(&self) -> usize {
        self.must_reject + self.controls
    }

    pub fn evidence(&self) -> String {
        format!(
            "K0301 part: {}/{} directory-mode projects pass ({} must give K0301, {} controls must give no error; \
             spec §4.1 cases built into the harness, each project judged as a whole){}",
            self.passed,
            self.total(),
            self.must_reject,
            self.controls,
            self.failures.first().map(|f| format!("; first failure: {f}")).unwrap_or_default()
        )
    }
}

/// Runs [`K0301_CASES`] through `kalam check --json --std STD PROJECT`. `Err` when no
/// binary is available or a project could not be checked at all.
pub fn run_projects(
    cli: &Result<KalamCli, String>,
    cases: &[ProjectCase],
    std: &Path,
) -> Result<ProjectRun, String> {
    let cli = cli.as_ref().map_err(|w| format!("K0301 part: {w}"))?;
    let mut run = ProjectRun::default();
    for case in cases {
        let files: Vec<(&str, &[u8])> =
            case.files.iter().map(|(p, t)| (*p, t.as_bytes())).collect();
        let r = cli
            .check_project(&files, std)
            .map_err(|e| format!("K0301 part: project {} could not be checked: {e}", case.id))?;
        if case.k0301 {
            run.must_reject += 1;
        } else {
            run.controls += 1;
        }
        match judge_project(case, &r) {
            Ok(()) => run.passed += 1,
            Err(f) => run.failures.push(f),
        }
    }
    Ok(run)
}

/// A fuzz target that fuzzes the type checker (module docs).
pub fn is_typeck_target(name: &str) -> bool {
    let n: String = name
        .to_lowercase()
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect();
    n.contains("typeck") || n.contains("typecheck")
}

pub struct M2Inputs {
    /// The committed index checked against the spec (`Err`: missing or stale).
    pub clauses: Result<Vec<Unit>, String>,
    pub conformance: Result<(ConfRun, Coverage), String>,
    pub stages: Stages,
    pub goldens: Result<(Vec<GoldenFile>, GoldenRun), String>,
    pub catalog_codes: Vec<String>,
    pub fix: Result<FixRun, String>,
    pub judge: Result<(f64, String), String>,
    pub lexhaz: Result<Option<RejectResult>, String>,
    pub catalog_full: CatalogReport,
    pub checkpoints: Vec<Checkpoint>,
    pub cli: Result<KalamCli, String>,
    /// The K0301 part of `G-PAR.P8` (directory mode).
    pub projects: Result<ProjectRun, String>,
}

fn read(root: &Path, rel: &str) -> Option<String> {
    fs::read_to_string(root.join(rel)).ok()
}

/// Runs every M2 runner. `checkpoint_ids` are the target tier's checkpoint ids.
pub fn compute(
    root: &Path,
    hook: &dyn ParseHook,
    spec_text: &str,
    kalam: Option<&Path>,
    seeded: &[SuiteCase],
    checkpoint_ids: &[String],
    held_typecheck: Option<&Value>,
) -> M2Inputs {
    let spec_units = clauses::index(spec_text);
    let clauses_checked =
        clauses::load_checked(read(root, clauses::CLAUSES_PATH).as_deref(), spec_text);
    let cli = KalamCli::locate(kalam);
    let stages = conformance::probe_stages(hook, &cli);
    let conformance = conformance::load(&root.join(conformance::CONFORMANCE_DIR))
        .and_then(|c| c.ok_or_else(|| format!("{} does not exist", conformance::CONFORMANCE_DIR)))
        .map(|cases| {
            let run = conformance::run(hook, &cli, &stages, &cases, &spec_units);
            let cov = conformance::coverage(&spec_units, &run);
            (run, cov)
        });
    let golden_files = goldens::load(&root.join(goldens::GOLDENS_DIR))
        .and_then(|g| g.ok_or_else(|| format!("{} does not exist", goldens::GOLDENS_DIR)));
    let goldens = golden_files.clone().map(|files| {
        let run = goldens::run(hook, &files);
        (files, run)
    });
    let catalog_codes: Vec<String> = diagcat::builtin_entries()
        .into_iter()
        .map(|e| e.code)
        .collect();
    let fix = golden_files
        .as_ref()
        .map(|g| goldens::fix_rate(hook, &goldens::fix_corpus(g, seeded)));
    let fix = fix.map_err(|e| e.clone());
    let judge = judge::measure(
        golden_files.clone().and_then(|g| {
            let rubric = read(root, judge::RUBRIC_PATH)
                .ok_or_else(|| format!("{} does not exist", judge::RUBRIC_PATH))?;
            judge::sample(&g, &rubric)
        }),
        read(root, judge::SAMPLE_PATH).as_deref(),
        read(root, judge::REPORT_PATH).as_deref(),
        read(root, judge::CALIBRATION_PATH).as_deref(),
    );
    let lexhaz = suites::load(&root.join(suites::SUITES_DIR).join(suites::LEXHAZ)).map(|c| {
        c.map(|cases| {
            suites::run_reject(
                hook,
                &cases,
                &RejectRules {
                    include_m2: true,
                    ..RejectRules::default()
                },
            )
        })
    });
    let catalog_full = diagcat::measure_in(spec_text, &diagcat::builtin_entries(), None);
    let std_dir = root.join(STDLIB_DIR);
    let checkpoints = if checkpoint_ids.iter().any(|c| c.starts_with("roadmap.M2.")) {
        checkpoints::m2(&cli, &root.join("eval/corpus"), &std_dir, held_typecheck)
    } else {
        Vec::new()
    };
    let projects = run_projects(&cli, K0301_CASES, &std_dir);
    M2Inputs {
        clauses: clauses_checked,
        conformance,
        stages,
        goldens,
        catalog_codes,
        fix,
        judge,
        lexhaz,
        catalog_full,
        checkpoints,
        cli,
        projects,
    }
}

/// The stdlib signature stubs the directory-mode checks resolve against.
pub const STDLIB_DIR: &str = "stdlib";

fn measured(value: f64, evidence: String) -> Measure {
    Measure::Measured {
        value: MetricValue::Num(value),
        note: Some(evidence),
    }
}

impl M2Inputs {
    pub fn spec_clause_coverage(&self) -> Measure {
        let units = match &self.clauses {
            Err(why) => return Measure::NotMeasurable(why.clone()),
            Ok(u) => u,
        };
        match &self.conformance {
            Err(why) => Measure::NotMeasurable(why.clone()),
            Ok((run, _)) if !run.defects.is_empty() => Measure::NotMeasurable(format!(
                "{} has defects, so coverage cannot be judged: {:?}",
                conformance::CONFORMANCE_DIR,
                run.defects.iter().take(3).collect::<Vec<_>>()
            )),
            Ok((run, _)) => {
                let cov = conformance::coverage(units, run);
                measured(
                    cov.pct(),
                    format!(
                        "{}: {} units (### subsections of spec §4-§22, plus §N.0 for text directly under a section \
                         heading), {} clauses; a unit is covered when it has >= 1 must-reject and >= 1 accepted case \
                         under {}/ (pending/ excluded): {}/{} covered {:?}; one polarity only: {:?}",
                        clauses::CLAUSES_PATH,
                        units.len(),
                        units.iter().map(|u| u.clauses.len()).sum::<usize>(),
                        conformance::CONFORMANCE_DIR,
                        cov.covered.len(),
                        cov.units,
                        cov.covered,
                        cov.one_sided
                    ),
                )
            }
        }
    }

    pub fn conformance_pass_rate(&self) -> Measure {
        match &self.conformance {
            Err(why) => Measure::NotMeasurable(why.clone()),
            Ok((run, _)) => match run.pass_rate() {
                Err(why) => Measure::NotMeasurable(why),
                Ok(v) => measured(
                    v,
                    format!("{}; stages: {}", run.evidence(), self.stages.to_json()),
                ),
            },
        }
    }

    pub fn golden_coverage(&self) -> Measure {
        match &self.goldens {
            Err(why) => Measure::NotMeasurable(why.clone()),
            Ok(_) if self.catalog_codes.is_empty() => {
                Measure::NotMeasurable("`kalam explain --catalog --json` lists no code".into())
            }
            Ok((_, run)) => {
                let (pct, ev) = goldens::coverage(run, &self.catalog_codes);
                // the same goldens against the diag_completeness denominator, for the founder
                let full: Vec<String> = self.catalog_full.codes.keys().cloned().collect();
                let complete = run.complete();
                let n = full.iter().filter(|c| complete.contains(*c)).count();
                measured(
                    pct,
                    format!(
                        "{ev}. Against spec §27 ∪ catalog (the D5.diag_completeness denominator, {} codes) the same \
                         goldens give {n}/{} = {:.2}%",
                        full.len(),
                        full.len(),
                        if full.is_empty() { 0.0 } else { 100.0 * n as f64 / full.len() as f64 }
                    ),
                )
            }
        }
    }

    pub fn machine_fix_rate(&self) -> Measure {
        match &self.fix {
            Err(why) => Measure::NotMeasurable(why.clone()),
            Ok(r) => match r.pct() {
                None => Measure::NotMeasurable(format!("{}; no error was emitted", r.evidence())),
                Some(_) if !r.unchecked.is_empty() => Measure::NotMeasurable(format!(
                    "{}; {} file(s) could not be checked, so the rate is partial: {:?}",
                    r.evidence(),
                    r.unchecked.len(),
                    r.unchecked.iter().take(3).collect::<Vec<_>>()
                )),
                Some(v) => measured(v, r.evidence()),
            },
        }
    }

    pub fn judge_score(&self) -> Measure {
        match &self.judge {
            Err(why) => Measure::NotMeasurable(why.clone()),
            Ok((mean, ev)) => measured(*mean, ev.clone()),
        }
    }

    /// The M2-scope observation of a check M2 repeats with a wider scope (module docs).
    pub fn observe_wider(&self, id: &str, m1: Option<&Observation>, root: &Path) -> Observation {
        match id {
            "D5.diag_completeness" => Observation::Measured {
                value: MetricValue::Num(self.catalog_full.completeness_pct()),
                evidence: self.catalog_full.evidence(),
                dev_only: false,
                definitive_fail: false,
            },
            "D2.lexing_hazard_suite" => match &self.lexhaz {
                Err(e) => Observation::NotMeasurable(e.clone()),
                Ok(None) => Observation::NotMeasurable(format!(
                    "{}/{} does not exist",
                    suites::SUITES_DIR,
                    suites::LEXHAZ
                )),
                Ok(Some(r)) => {
                    let ev = format!(
                        "{}/{} at M2 scope ([M2] cases and SEC0104 included): bad rejected {}/{}, good accepted {}/{}, \
                         defects {}, not measurable {}{}",
                        suites::SUITES_DIR,
                        suites::LEXHAZ,
                        r.bad_passed,
                        r.bad_total,
                        r.good_passed,
                        r.good_total,
                        r.defects.len(),
                        r.not_measurable.len(),
                        r.failures.first().map(|f| format!("; first failure: {f}")).unwrap_or_default()
                    );
                    if !r.defects.is_empty() || r.total() == 0 {
                        Observation::NotMeasurable(format!(
                            "{ev}; the suite needs defect-free cases"
                        ))
                    } else if !r.not_measurable.is_empty() && r.all_pass() {
                        Observation::NotMeasurable(format!("{ev}; some cases could not be checked"))
                    } else {
                        Observation::Measured {
                            value: MetricValue::Bool(r.all_pass()),
                            evidence: ev,
                            dev_only: false,
                            definitive_fail: !r.all_pass(),
                        }
                    }
                }
            },
            "D1.fuzz_crash_free_hours" => {
                let targets = match fs::read_to_string(root.join(FUZZ_STATUS))
                    .map_err(|_| format!("{FUZZ_STATUS} does not exist (the fuzz runner writes it)"))
                    .and_then(|t| FuzzStatus::parse(&t).map_err(|e| format!("{FUZZ_STATUS}: {e}")))
                {
                    Err(e) => return Observation::NotMeasurable(e),
                    Ok(s) => match s.targets {
                        None => {
                            return Observation::NotMeasurable(format!(
                                "{FUZZ_STATUS} lists no `targets`, so the M2 scope (the type-checker target in the \
                                 running fuzz binary) cannot be confirmed"
                            ))
                        }
                        Some(t) => t,
                    },
                };
                let typeck: Vec<&String> = targets.iter().filter(|t| is_typeck_target(t)).collect();
                match m1 {
                    _ if typeck.is_empty() => Observation::NotMeasurable(format!(
                        "the M2 scope adds the type-checker fuzz target, and {FUZZ_STATUS} lists none (targets: \
                         {targets:?}; a type-checker target's name contains `typeck` or `typecheck`); the 24 h clock \
                         counts from when that target joins the run"
                    )),
                    Some(Observation::Measured { value, evidence, dev_only, definitive_fail }) => Observation::Measured {
                        value: value.clone(),
                        evidence: format!("{evidence}; type-checker target(s) {typeck:?} (M2 scope)"),
                        dev_only: *dev_only,
                        definitive_fail: *definitive_fail,
                    },
                    Some(o) => o.clone(),
                    None => Observation::NotMeasurable("no M1-scope fuzz observation".into()),
                }
            }
            "G-PAR.P8" => p8(m1, &self.projects),
            _ => Observation::NotMeasurable(format!("no M2-scope runner for `{id}`")),
        }
    }

    /// The K0301 part as JSON (supporting checks).
    pub fn projects_json(&self) -> Value {
        match &self.projects {
            Ok(r) => {
                json!({"must_reject": r.must_reject, "controls": r.controls, "passed": r.passed, "failures": r.failures})
            }
            Err(e) => json!({"status": "not_measurable", "reason": e}),
        }
    }

    /// The target tier's checkpoints (`checkpoint_defs` from scorecard.json).
    pub fn checkpoints_json(&self, checkpoint_defs: &[Value]) -> Value {
        let mut out = Vec::new();
        for def in checkpoint_defs {
            let id = def["id"].as_str().unwrap_or("");
            let target = def["target"].as_str().unwrap_or("").to_string();
            if let Some(c) = self.checkpoints.iter().find(|c| c.id == id) {
                out.push(c.to_json());
                continue;
            }
            if id == "D1.spec_clause_coverage" {
                let want = target
                    .split(">=")
                    .nth(1)
                    .and_then(|r| r.trim().trim_start_matches(' ').split('%').next())
                    .and_then(|n| n.trim().parse::<f64>().ok());
                let (status, value, evidence) = match (self.spec_clause_coverage(), want) {
                    (
                        Measure::Measured {
                            value: MetricValue::Num(v),
                            note,
                        },
                        Some(w),
                    ) => (
                        if v >= w { "pass" } else { "fail" },
                        json!(crate::report::round2(v)),
                        note.unwrap_or_default(),
                    ),
                    (Measure::NotMeasurable(why), _) => ("not_measurable", Value::Null, why),
                    _ => (
                        "not_measurable",
                        Value::Null,
                        "the checkpoint target has no `>= N%`".into(),
                    ),
                };
                out.push(json!({"id": id, "target": target, "status": status, "value": value, "evidence": evidence}));
                continue;
            }
            out.push(
                json!({"id": id, "target": target, "status": "not_measurable", "value": null,
                            "evidence": format!("no checkpoint runner for `{id}`")}),
            );
        }
        Value::Array(out)
    }

    pub fn checks_json(&self) -> Value {
        json!({
            "kalam_binary": match &self.cli { Ok(c) => json!(c.exe.display().to_string()), Err(e) => json!({"status": "missing", "reason": e}) },
            "stages": self.stages.to_json(),
            "clauses": match &self.clauses { Ok(u) => json!({"units": u.len(), "clauses": u.iter().map(|u| u.clauses.len()).sum::<usize>()}), Err(e) => json!({"status": "not_measurable", "reason": e}) },
            "conformance": match &self.conformance { Ok((r, cov)) => json!({"run": r.to_json(), "covered_units": cov.covered, "one_sided_units": cov.one_sided}), Err(e) => json!({"status": "missing", "reason": e}) },
            "goldens": match &self.goldens { Ok((_, r)) => json!({"complete": r.complete(), "failures": r.failures}), Err(e) => json!({"status": "missing", "reason": e}) },
            "fix_rate": match &self.fix { Ok(r) => goldens::fix_json(r), Err(e) => json!({"status": "missing", "reason": e}) },
            "p8_k0301_projects": self.projects_json(),
            "judge": match &self.judge { Ok((m, ev)) => json!({"mean": m, "evidence": ev}), Err(e) => json!({"status": "not_measurable", "reason": e}) },
        })
    }
}

/// `G-PAR.P8` at M2 scope: the K0201 part (the M1 observation) and the K0301 part.
pub fn p8(k0201: Option<&Observation>, k0301: &Result<ProjectRun, String>) -> Observation {
    let k0201_ev = match k0201 {
        Some(Observation::Measured { evidence, .. })
        | Some(Observation::NotMeasurable(evidence))
        | Some(Observation::Passed(evidence))
        | Some(Observation::Failed(evidence)) => evidence.clone(),
        None => "not observed".into(),
    };
    let k0301_ev = match k0301 {
        Ok(r) => r.evidence(),
        Err(e) => e.clone(),
    };
    let evidence = format!("K0201 part: {k0201_ev}; {k0301_ev}");
    let k0201_failed = matches!(
        k0201,
        Some(Observation::Measured {
            definitive_fail: true,
            ..
        }) | Some(Observation::Failed(_))
    );
    let k0301_failed = matches!(k0301, Ok(r) if !r.failures.is_empty());
    if k0201_failed || k0301_failed {
        return Observation::Measured {
            value: MetricValue::Bool(false),
            evidence,
            dev_only: false,
            definitive_fail: true,
        };
    }
    let k0201_passed = matches!(
        k0201,
        Some(Observation::Measured {
            value: MetricValue::Bool(true),
            dev_only: false,
            ..
        }) | Some(Observation::Passed(_))
    );
    let k0301_passed =
        matches!(k0301, Ok(r) if r.must_reject > 0 && r.controls > 0 && r.passed == r.total());
    if k0201_passed && k0301_passed {
        Observation::Measured {
            value: MetricValue::Bool(true),
            evidence,
            dev_only: false,
            definitive_fail: false,
        }
    } else {
        Observation::NotMeasurable(format!(
            "{evidence}; both parts must be measured in full to pass"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::CliDiag;

    fn diag(code: &str, file: &str) -> CliDiag {
        CliDiag {
            code: code.into(),
            line: 1,
            file: file.into(),
        }
    }

    #[test]
    fn k0301_projects_are_judged_as_a_whole() {
        let must = &K0301_CASES[0];
        let control = K0301_CASES.iter().find(|c| !c.k0301).unwrap();
        let k0301 = CliCheck {
            exit: Some(1),
            errors: vec![diag("K0301", "./src/users.kl")],
        };
        let clean = CliCheck {
            exit: Some(0),
            errors: vec![],
        };
        let other = CliCheck {
            exit: Some(1),
            errors: vec![diag("K0401", "./src/users.ar.kl")],
        };
        assert!(judge_project(must, &k0301).is_ok());
        assert!(judge_project(must, &clean).is_err());
        assert!(judge_project(must, &other).is_err());
        assert!(judge_project(control, &clean).is_ok());
        assert!(judge_project(control, &other).is_err());
        assert!(judge_project(control, &k0301).is_err());
        // every skin pair is covered, and there are controls
        for pair in [("", ".ar"), ("", ".franco"), (".ar", ".franco")] {
            assert!(K0301_CASES.iter().any(|c| c.k0301
                && c.files
                    .iter()
                    .any(|(p, _)| p.ends_with(&format!("users{}.kl", pair.0)))
                && c.files
                    .iter()
                    .any(|(p, _)| p.ends_with(&format!("users{}.kl", pair.1)))));
        }
        assert!(K0301_CASES.iter().filter(|c| !c.k0301).count() >= 2);
    }

    #[test]
    fn without_a_binary_the_k0301_part_is_not_measurable() {
        let r = run_projects(
            &Err("no kalam binary".into()),
            K0301_CASES,
            Path::new("stdlib"),
        );
        assert!(r.unwrap_err().contains("no kalam binary"));
    }

    #[test]
    fn p8_needs_both_parts() {
        let pass = Observation::Measured {
            value: MetricValue::Bool(true),
            evidence: "k0201 ok".into(),
            dev_only: false,
            definitive_fail: false,
        };
        let fail = Observation::Measured {
            value: MetricValue::Bool(false),
            evidence: "k0201 bad".into(),
            dev_only: false,
            definitive_fail: true,
        };
        let good = Ok(ProjectRun {
            must_reject: 4,
            controls: 2,
            passed: 6,
            failures: vec![],
        });
        let bad = Ok(ProjectRun {
            must_reject: 4,
            controls: 2,
            passed: 5,
            failures: vec!["en-ar".into()],
        });
        let none: Result<ProjectRun, String> = Err("no kalam binary".into());
        let is = |o: Observation| match o {
            Observation::Measured {
                value: MetricValue::Bool(b),
                ..
            } => Some(b),
            _ => None,
        };
        assert_eq!(is(p8(Some(&pass), &good)), Some(true));
        assert_eq!(is(p8(Some(&pass), &bad)), Some(false));
        assert_eq!(is(p8(Some(&fail), &good)), Some(false));
        assert_eq!(is(p8(Some(&fail), &none)), Some(false));
        assert_eq!(is(p8(Some(&pass), &none)), None);
        assert_eq!(is(p8(None, &good)), None);
        let dev_only = Observation::Measured {
            value: MetricValue::Bool(true),
            evidence: "dev".into(),
            dev_only: true,
            definitive_fail: false,
        };
        assert_eq!(is(p8(Some(&dev_only), &good)), None);
    }

    #[test]
    fn typeck_fuzz_targets_are_named_for_the_type_checker() {
        for t in [
            "typeck",
            "Typeck",
            "type_check",
            "type-checker",
            "kalam_typecheck",
        ] {
            assert!(is_typeck_target(t), "{t}");
        }
        for t in [
            "lexer",
            "parser",
            "fmt",
            "convert",
            "kcf",
            "prototype",
            "type_alias_parse",
        ] {
            assert!(!is_typeck_target(t), "{t}");
        }
    }

    fn fuzz_root(name: &str, status: Option<&str>) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("kalam-eval-m2-fuzz-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("eval/fuzz")).unwrap();
        if let Some(s) = status {
            fs::write(d.join(FUZZ_STATUS), s).unwrap();
        }
        d
    }

    fn inputs_without_runs() -> M2Inputs {
        M2Inputs {
            clauses: Err("x".into()),
            conformance: Err("x".into()),
            stages: Stages {
                parse: Err("x".into()),
                resolve: Err("x".into()),
                typeck: Err("x".into()),
                interp: Err("x".into()),
            },
            goldens: Err("x".into()),
            catalog_codes: vec![],
            fix: Err("x".into()),
            judge: Err("x".into()),
            lexhaz: Err("x".into()),
            catalog_full: diagcat::measure_in("", &[], None),
            checkpoints: vec![],
            cli: Err("x".into()),
            projects: Err("x".into()),
        }
    }

    #[test]
    fn fuzz_at_m2_needs_a_listed_type_checker_target() {
        let m1 = Observation::Measured {
            value: MetricValue::Num(30.0),
            evidence: "m1".into(),
            dev_only: false,
            definitive_fail: false,
        };
        let i = inputs_without_runs();
        let obs = |status: Option<&str>, name: &str| {
            let d = fuzz_root(name, status);
            // a type-checker target in the repository does not count: only the status list
            fs::create_dir_all(d.join("compiler/fuzz/fuzz_targets")).unwrap();
            fs::write(d.join("compiler/fuzz/fuzz_targets/typeck.rs"), "").unwrap();
            let o = i.observe_wider("D1.fuzz_crash_free_hours", Some(&m1), &d);
            let _ = fs::remove_dir_all(&d);
            o
        };
        assert!(matches!(
            obs(None, "missing"),
            Observation::NotMeasurable(_)
        ));
        assert!(matches!(
            obs(Some(r#"{"crash_free_hours": 30}"#), "untargeted"),
            Observation::NotMeasurable(ref w) if w.contains("no `targets`")
        ));
        assert!(matches!(
            obs(Some(r#"{"crash_free_hours": 30, "targets": ["lexer", "parser", "fmt"]}"#), "m1only"),
            Observation::NotMeasurable(ref w) if w.contains("lists none")
        ));
        match obs(
            Some(r#"{"crash_free_hours": 30, "targets": ["lexer", "parser", "fmt", "typeck"]}"#),
            "typeck",
        ) {
            Observation::Measured {
                value, evidence, ..
            } => {
                assert_eq!(value, MetricValue::Num(30.0));
                assert!(evidence.contains("typeck"));
            }
            o => panic!("{o:?}"),
        }
    }
}
