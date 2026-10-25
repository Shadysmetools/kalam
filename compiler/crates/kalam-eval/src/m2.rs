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
//! - `D1.fuzz_crash_free_hours` "adds the type-checker target": as at M1, plus a fuzz
//!   target whose name names the type checker (`typeck`/`type`), else not measurable;
//! - `G-PAR.P8` "K0201 and K0301": a K0201 failure is a failure; otherwise not measurable,
//!   because a K0301 case (two skin files of one module) needs both files checked as one
//!   package, which neither the shared suite format (each file judged alone) nor `kalam
//!   check` supports yet.

use crate::checkpoints::{self, Checkpoint};
use crate::clauses::{self, Unit};
use crate::cli::KalamCli;
use crate::conformance::{self, ConfRun, Coverage, Stages};
use crate::diagcat::{self, CatalogReport};
use crate::external::{self, FuzzStatus, FUZZ_STATUS};
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
    let checkpoints = if checkpoint_ids.iter().any(|c| c.starts_with("roadmap.M2.")) {
        checkpoints::m2(&cli, &root.join("eval/corpus"))
    } else {
        Vec::new()
    };
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
    }
}

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
                        Observation::NotMeasurable(format!("{ev}; the suite needs defect-free cases"))
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
                let targets = fs::read_to_string(root.join(FUZZ_STATUS))
                    .ok()
                    .and_then(|t| FuzzStatus::parse(&t).ok())
                    .and_then(|s| s.targets)
                    .unwrap_or_else(|| external::repo_fuzz_targets(root));
                let has_typeck = targets.iter().any(|t| {
                    let t = t.to_lowercase();
                    t.contains("typeck") || t.contains("type")
                });
                match m1 {
                    Some(o) if has_typeck => o.clone(),
                    Some(Observation::NotMeasurable(e)) => Observation::NotMeasurable(e.clone()),
                    _ => Observation::NotMeasurable(format!(
                        "the M2 scope adds the type-checker fuzz target, and none is listed (targets: {targets:?}); \
                         the 24 h clock counts from when that target joins the run"
                    )),
                }
            }
            "G-PAR.P8" => match m1 {
                Some(o @ Observation::Measured { definitive_fail: true, .. }) | Some(o @ Observation::Failed(_)) => o.clone(),
                other => Observation::NotMeasurable(format!(
                    "K0201 part: {}; K0301 part: no case and no runner. K0301 (two skin files for one module, spec §4.1) \
                     needs both files checked together as one package; the shared suite format judges each file alone \
                     and `kalam check` takes one file, so the M2 scope cannot be measured yet",
                    match other {
                        Some(Observation::Measured { evidence, .. }) | Some(Observation::NotMeasurable(evidence)) | Some(Observation::Passed(evidence)) => evidence.clone(),
                        _ => "not observed".into(),
                    }
                )),
            },
            _ => Observation::NotMeasurable(format!("no M2-scope runner for `{id}`")),
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
            "judge": match &self.judge { Ok((m, ev)) => json!({"mean": m, "evidence": ev}), Err(e) => json!({"status": "not_measurable", "reason": e}) },
        })
    }
}
