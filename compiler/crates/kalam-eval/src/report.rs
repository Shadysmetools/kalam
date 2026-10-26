//! Assembles one scorecard run: every scorecard metric (measured or `not_measurable`),
//! dimension scores, composite, gate states, the milestone tier checks and the supporting
//! checks.
//!
//! Honesty rules (LOOP.md A2/A12, SCORECARD.md §1):
//! - a metric with no harness measurer is `not_measurable` and scores 0 in its dimension;
//! - a partial measurement never produces a pass: a metric or gate is `pass` only when its
//!   whole definition was checked, `fail` as soon as any checked part fails, else `pending`;
//! - checks judged on the frozen set held-out included (the "held-out split" table in
//!   eval/HISTORY-SCHEMA.md) pass only when the held-out runner also ran and passed.

use crate::checks::{self, KeywordCounts};
use crate::corpus_run::{self, CorpusRun};
use crate::diagcat::{self, CatalogReport};
use crate::external;
use crate::holdout::{self, Held};
use crate::integrity::{self, IntegrityReport, State};
use crate::m2;
use crate::parity::{self, Outcome, ParseHook, SyntaxOutcome};
use crate::ratchet::{self, Ratchet};
use crate::scorecard::{inclusion, normalize, Inclusion, MetricDef, MetricValue, Scorecard};
use crate::suites::{self, FmtTally, Recovery, RejectResult, RejectRules, SuiteCase};
use crate::tiers::{self, Observation, ScorecardRule, Status, TierCheck};
use kalam_base::Skin;
use kalam_skins::KeywordTable;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Options {
    /// Repository root (holds `eval/`, `spec/`, `compiler/`).
    pub root: PathBuf,
    /// The target milestone tier (`M1`..`M10`, `beta`, `1.0`) or a legacy scorecard name
    /// (`mvp` = M5). Metrics are normalized against the tier's scorecard milestone.
    pub tier: String,
    pub run_cargo_test: bool,
    /// Compiler access ([`parity::default_hook`]).
    pub hook: Box<dyn ParseHook>,
    /// The evaluated commit: external reports (props) must be tied to it.
    pub git_sha: Option<String>,
    /// The held-out runner; `None` = not run (every held-out-split check stays unmeasured).
    pub holdout: Option<holdout::Runner>,
    /// Run `cargo test -p kalam-props --release` and read the report it writes, instead of
    /// reading `eval/props/report.json` (which must then carry the evaluated commit).
    pub run_props: bool,
    /// Read the kalam-props report from this file instead of `eval/props/report.json`
    /// (`--props-report`); its `commit` must still be the evaluated commit.
    pub props_report: Option<PathBuf>,
    /// The `kalam` binary for the M2 checkpoints and interpreter cases (`--kalam`, else
    /// `KALAM_BIN`); never built by the harness.
    pub kalam: Option<PathBuf>,
}

/// One metric's outcome before normalization.
#[derive(Clone, Debug)]
pub enum Measure {
    Measured {
        value: MetricValue,
        note: Option<String>,
    },
    NotMeasurable(String),
}

pub struct Run {
    pub json: Value,
}

/// Every M1 tier check with a runner. `D5.diag_completeness` is in M1 scope: LOOP.md §5.1
/// item 14 and scorecard.json `milestone_tiers` M1 list it at stage S2 (the metric table's
/// S3 is the stage where it reaches its MVP scope).
pub const M1_MEASURED: &[&str] = &[
    "G-PAR.P1",
    "G-PAR.P2",
    "G-PAR.P3",
    "G-PAR.P4",
    "G-PAR.P6",
    "G-PAR.P7",
    "G-PAR.P8",
    "D1.corpus_syntax_pass",
    "D1.property_tests",
    "D1.fuzz_crash_free_hours",
    "D2.trojan_source_suite",
    "D2.lexing_hazard_suite",
    "D5.error_recovery_precision",
    "D5.diag_completeness",
    "D7.formatter_idempotence",
    "D7.skin_convert",
];

/// Spec §7.4 rule 7 (word-choice notes enforced by tests): `(concept, required ar spelling)`
/// and Arabic spellings no reserved word may use.
const RULE7_REQUIRED: &[(&str, &str)] = &[
    ("KW_TRUE", "صح"),
    ("CORE_INT", "عدد"),
    ("CORE_NONE", "لا_شيء"),
    ("KW_FALSE", "خطأ"),
    ("CORE_ERR", "فشل"),
    ("KW_CASE", "عندما"),
];
const RULE7_AVOIDED: &[&str] = &["صحيح", "عدم", "حالة", "عند"];

/// G-PAR P7 at M1 scope: spec §7.4 rules 1-4 (`KeywordTable::load`) and rule 7. Rules 5-6
/// need the confusable table and the 2,000-word list, which do not exist (spec §7.4); the
/// M1 scope counts them "once their data exists".
pub fn p7_conflicts(rows: &[Value], load: &Result<(), String>) -> (usize, Vec<String>) {
    let mut conflicts = Vec::new();
    if let Err(e) = load {
        conflicts.push(format!("rules 1-4: {e}"));
    }
    for (concept, want) in RULE7_REQUIRED {
        let got = rows
            .iter()
            .find(|r| r["concept"] == *concept)
            .and_then(|r| r["ar"].as_str());
        if got != Some(*want) {
            conflicts.push(format!(
                "rule 7: {concept} ar is {got:?}, spec §7.4 requires {want}"
            ));
        }
    }
    for avoided in RULE7_AVOIDED {
        if let Some(r) = rows.iter().find(|r| r["ar"] == *avoided) {
            conflicts.push(format!(
                "rule 7: {} uses the avoided spelling {avoided}",
                r["concept"]
            ));
        }
    }
    (conflicts.len(), conflicts)
}

/// The dev-side inputs of every M1 check.
struct Inputs {
    corpus: CorpusRun,
    trojan: Option<RejectResult>,
    lexhaz: Option<RejectResult>,
    mixed: Option<RejectResult>,
    recovery: Option<Recovery>,
    conformance: Option<FmtTally>,
    held: Held,
    props: Result<external::PropsReport, String>,
    /// The Trojan Source suite is judged with its `[M2]` cases (target tier M2 or later).
    trojan_m2: bool,
    fuzz: Observation,
    catalog: CatalogReport,
    p7: (usize, Vec<String>),
}

fn suite_note(name: &str) -> String {
    format!("{}/{name}", suites::SUITES_DIR)
}

/// The held-out counts at `path` (e.g. `["parity", "P1"]`) as `(passed, total)`.
fn held_counts(
    held: &Held,
    path: &[&str],
    passed: &str,
    total: &str,
) -> Result<(usize, usize), String> {
    let agg = held.aggregate().ok_or_else(|| held.why_not())?;
    let mut v = agg;
    for p in path {
        v = &v[*p];
    }
    if v.is_null() {
        return Err(format!("the held-out store has no {} part", path.join(".")));
    }
    let n = |k: &str| v[k].as_u64().map(|n| n as usize);
    match (n(passed), n(total)) {
        (Some(p), Some(t)) if t > 0 => Ok((p, t)),
        (Some(_), Some(_)) => Err(format!(
            "the held-out store has no case in {} scope",
            path.join(".")
        )),
        _ => Err(format!(
            "the held-out aggregate has no {} counts",
            path.join(".")
        )),
    }
}

/// A count-based check (P1-P3): dev `passed/total` combined with the held-out counts.
fn combine_counts(what: &str, dev: &Outcome, held: Result<(usize, usize), String>) -> Observation {
    let (passed, total, failures) = match dev {
        Outcome::Unavailable(why) => return Observation::NotMeasurable(format!("{what}: {why}")),
        Outcome::Measured {
            passed,
            total,
            failures,
        } => (*passed, *total, failures),
    };
    let first = failures
        .first()
        .map(|f| format!("; first failure: {f}"))
        .unwrap_or_default();
    match held {
        Err(why) => Observation::Measured {
            value: MetricValue::Num(100.0 * passed as f64 / total as f64),
            evidence: format!("{what}: dev {passed}/{total}{first}; held-out: {why}"),
            dev_only: true,
            definitive_fail: passed < total,
        },
        Ok((hp, ht)) => Observation::Measured {
            value: MetricValue::Num(100.0 * (passed + hp) as f64 / (total + ht) as f64),
            evidence: format!("{what}: dev {passed}/{total}, held-out {hp}/{ht}{first}"),
            dev_only: false,
            definitive_fail: passed < total || hp < ht,
        },
    }
}

/// A binary check with a held-out split.
fn combine_bool(
    dev: Result<(bool, String), String>,
    held: Result<(bool, String), String>,
) -> Observation {
    let (ok, ev) = match dev {
        Err(why) => return Observation::NotMeasurable(why),
        Ok(d) => d,
    };
    match held {
        Err(why) => Observation::Measured {
            value: MetricValue::Bool(ok),
            evidence: format!("{ev}; held-out: {why}"),
            dev_only: true,
            definitive_fail: !ok,
        },
        Ok((hok, hev)) => Observation::Measured {
            value: MetricValue::Bool(ok && hok),
            evidence: format!("{ev}; held-out: {hev}"),
            dev_only: false,
            definitive_fail: !(ok && hok),
        },
    }
}

/// A binary check judged on dev data only (no held-out split).
fn whole(ok: bool, evidence: String) -> Observation {
    Observation::Measured {
        value: MetricValue::Bool(ok),
        evidence,
        dev_only: false,
        definitive_fail: !ok,
    }
}

fn reject_summary(r: &RejectResult) -> String {
    format!(
        "diagnostics via {}; bad rejected {}/{}, good accepted {}/{}; deferred to M2 (`[M2]` cases, FOUNDER-QUEUE Q10) {}; \
         out of scope {}; defects {}; not measurable {}; failing cases that expect a code no M1 stage emits \
         (name resolution or later): {} {:?}{}",
        r.route,
        r.bad_passed,
        r.bad_total,
        r.good_passed,
        r.good_total,
        r.deferred_m2,
        r.out_of_scope.len(),
        r.defects.len(),
        r.not_measurable.len(),
        r.later_stage.len(),
        r.later_stage.iter().take(5).collect::<Vec<_>>(),
        r.failures
            .first()
            .or(r.defects.first())
            .map(|f| format!("; first problem: {f}"))
            .unwrap_or_default()
    )
}

/// A rejection-suite observation when some files could not be checked at all (`partial`):
/// a failure stands, anything else is not measurable. A partial suite never passes.
fn unless_partial(obs: Observation, partial: &[String]) -> Observation {
    if partial.is_empty() {
        return obs;
    }
    let note =
        format!(
        "{} case(s) could not be checked by any compiler entry point, so the suite cannot pass: {}",
        partial.len(),
        partial.iter().take(3).cloned().collect::<Vec<_>>().join(" | ")
    );
    match obs {
        Observation::Measured {
            value,
            evidence,
            dev_only,
            definitive_fail: true,
        } => Observation::Measured {
            value,
            evidence: format!("{evidence}; {note}"),
            dev_only,
            definitive_fail: true,
        },
        Observation::Measured { evidence, .. } => {
            Observation::NotMeasurable(format!("{evidence}; {note}"))
        }
        Observation::NotMeasurable(e) | Observation::Passed(e) => {
            Observation::NotMeasurable(format!("{e}; {note}"))
        }
        failed @ Observation::Failed(_) => failed,
    }
}

/// The not-measurable cases of a rejection suite.
fn partial_of(r: &Option<RejectResult>) -> Vec<String> {
    r.as_ref()
        .map(|r| r.not_measurable.clone())
        .unwrap_or_default()
}

/// The Trojan Source cases no entry point could check, dev and held-out (the held-out
/// runner reports a count only).
fn trojan_partial(dev: &Option<RejectResult>, held: &Held) -> Vec<String> {
    let mut partial = partial_of(dev);
    let n = held
        .aggregate()
        .map_or(0, |a| a["trojan"]["not_measurable"].as_u64().unwrap_or(0));
    if n > 0 {
        partial.push(format!("held-out: {n} case(s)"));
    }
    partial
}

/// A rejection suite judged as a whole: present, non-empty, no defects.
fn reject_dev(
    name: &str,
    r: &Option<RejectResult>,
    need_both: bool,
) -> Result<(bool, String), String> {
    let r = r
        .as_ref()
        .ok_or_else(|| format!("{} does not exist", suite_note(name)))?;
    let ev = format!("{}: {}", suite_note(name), reject_summary(r));
    if !r.defects.is_empty() {
        return Err(format!(
            "{ev}; the suite has defects, so it cannot be judged"
        ));
    }
    if r.total() == 0 || (need_both && (r.bad_total == 0 || r.good_total == 0)) {
        return Err(format!("{ev}; the suite needs bad and good cases in scope"));
    }
    Ok((r.all_pass(), ev))
}

fn observe_m1(id: &str, i: &Inputs, c: &TierCheck) -> Observation {
    let par = &i.corpus.parity;
    match id {
        "G-PAR.P1" => combine_counts("P1 canonical identity", &par.p1, held_counts(&i.held, &["parity", "P1"], "passed", "total")),
        "G-PAR.P2" => combine_counts("P2 pairwise and 3-hop round-trip (kalam convert)", &par.p2, held_counts(&i.held, &["parity", "P2"], "passed", "total")),
        "G-PAR.P3" => combine_counts("P3 source fixpoint (kalam convert, 9 ordered pairs)", &par.p3, held_counts(&i.held, &["parity", "P3"], "passed", "total")),
        "G-PAR.P4" => external::p4(&i.props),
        "D1.property_tests" => external::property_tests(&i.props),
        "D1.fuzz_crash_free_hours" => i.fuzz.clone(),
        "G-PAR.P6" | "D5.diag_completeness" => {
            let v = if id == "G-PAR.P6" { i.catalog.p6_pct() } else { i.catalog.completeness_pct() };
            Observation::Measured {
                value: MetricValue::Num(v),
                evidence: format!(
                    "{}. Ruling: in M1 scope (LOOP.md §5.1 items 14-15; scorecard.json M1 checks, stage S2)",
                    i.catalog.evidence()
                ),
                dev_only: false,
                definitive_fail: false,
            }
        }
        "G-PAR.P7" => Observation::Measured {
            value: MetricValue::Num(i.p7.0 as f64),
            evidence: format!(
                "spec/keywords.json: spec §7.4 rules 1-4 (KeywordTable::load) and rule 7 checked: {} conflict(s) {:?}. \
                 Rules 5-6 need the confusable table and the 2,000-word list, which do not exist (spec §7.4), so they are \
                 outside the M1 scope (\"rules 5-7 once their data exists\")",
                i.p7.0, i.p7.1
            ),
            dev_only: false,
            definitive_fail: false,
        },
        "G-PAR.P8" => {
            let r = match &i.mixed {
                None => return Observation::NotMeasurable(format!("{} does not exist", suite_note(suites::MIXED))),
                Some(r) => r,
            };
            let ev = format!(
                "{} (K0201 cases; K0301 deferred to M2): {}; must-reject files hosted in skins {:?}",
                suite_note(suites::MIXED),
                reject_summary(r),
                r.reject_skins.iter().map(|s| s.id()).collect::<Vec<_>>()
            );
            if !r.defects.is_empty() || r.bad_total == 0 {
                return Observation::NotMeasurable(format!("{ev}; needs defect-free K0201 cases"));
            }
            if r.reject_skins.len() < Skin::ALL.len() && r.all_pass() {
                return Observation::NotMeasurable(format!("{ev}; \"in every skin\" needs a K0201 case hosted in each of en, ar, franco"));
            }
            unless_partial(whole(r.all_pass(), ev), &r.not_measurable)
        }
        "D2.trojan_source_suite" => {
            let dev = reject_dev(suites::TROJAN, &i.trojan, true).map(|(ok, ev)| {
                let corpus_ok = i.corpus.sec_pass == i.corpus.sec_pairs;
                (
                    ok && corpus_ok,
                    format!(
                        "{ev}; corpus source-text cases {}/{} at lex level; every expected SEC code must name its code point",
                        i.corpus.sec_pass, i.corpus.sec_pairs
                    ),
                )
            });
            let held = held_counts(&i.held, &["trojan"], "bad_rejected", "bad_total").and_then(|(bp, bt)| {
                let (gp, gt) = held_counts(&i.held, &["trojan"], "good_accepted", "good_total")?;
                let defects = i.held.aggregate().map_or(0, |a| a["trojan"]["defects"].as_u64().unwrap_or(0));
                if defects > 0 {
                    return Err(format!("the held-out Trojan suite has {defects} defect(s)"));
                }
                let deferred = i.held.aggregate().map_or(0, |a| a["trojan"]["deferred_m2"].as_u64().unwrap_or(0));
                if i.trojan_m2 && deferred > 0 {
                    return Err(format!(
                        "the held-out runner deferred {deferred} `[M2]` case(s), which count from M2 (FOUNDER-QUEUE Q10)"
                    ));
                }
                Ok((
                    bp == bt && gp == gt,
                    format!("bad rejected {bp}/{bt}, good accepted {gp}/{gt}, deferred to M2 {deferred}"),
                ))
            });
            unless_partial(combine_bool(dev, held), &trojan_partial(&i.trojan, &i.held))
        }
        "D2.lexing_hazard_suite" => match reject_dev(suites::LEXHAZ, &i.lexhaz, false) {
            Err(why) => Observation::NotMeasurable(why),
            Ok((ok, ev)) => unless_partial(
                whole(ok, format!("{ev} (SEC0104 post-fold collision and K0401 cases are marked `[M2]` and count from M2)")),
                &partial_of(&i.lexhaz),
            ),
        },
        "D5.error_recovery_precision" => recovery_obs(i, c),
        "D7.formatter_idempotence" => {
            let dev = match &i.conformance {
                None => Err(format!("{} does not exist; the check covers every corpus and conformance file", suite_note(suites::CONFORMANCE))),
                Some(conf) => {
                    let mut all = i.corpus.fmt.clone();
                    all.add(conf);
                    let ev = format!(
                        "kalam fmt (kalam_driver::format_source) on corpus {}/{} and conformance {}/{} files (all skins): \
                         fmt(fmt(x)) == fmt(x) and kcf(fmt(x)) == kcf(x); layout kept (re-spelled only) {}; excluded {}; \
                         deferred to M2 (`[M2]` cases) {}{}",
                        i.corpus.fmt.passed, i.corpus.fmt.total, conf.passed, conf.total, all.layout_kept, all.excluded,
                        conf.deferred_m2,
                        all.failures.first().map(|f| format!("; first failure: {f}")).unwrap_or_default()
                    );
                    if !conf.defects.is_empty() {
                        Err(format!("{ev}; conformance suite defects: {:?}", conf.defects))
                    } else if all.total == 0 || conf.total == 0 {
                        Err(format!("{ev}; no file in scope (the formatter or parser is unavailable, or the suite is empty)"))
                    } else {
                        Ok((all.failures.is_empty(), ev))
                    }
                }
            };
            let held = held_counts(&i.held, &["fmt"], "passed", "total")
                .map(|(p, t)| (p == t, format!("corpus files {p}/{t}")));
            combine_bool(dev, held)
        }
        "D7.skin_convert" => {
            let p2 = observe_m1("G-PAR.P2", i, c);
            let p3 = observe_m1("G-PAR.P3", i, c);
            match (&p2, &p3) {
                (
                    Observation::Measured { value: MetricValue::Num(a), evidence: e2, dev_only: d2, .. },
                    Observation::Measured { value: MetricValue::Num(b), evidence: e3, dev_only: d3, .. },
                ) => {
                    let ok = *a == 100.0 && *b == 100.0;
                    Observation::Measured {
                        value: MetricValue::Bool(ok),
                        evidence: format!("through kalam_driver::convert_source (the `kalam convert` entry point): {e2}; {e3}"),
                        dev_only: *d2 || *d3,
                        definitive_fail: !ok,
                    }
                }
                (Observation::NotMeasurable(w), _) | (_, Observation::NotMeasurable(w)) => {
                    Observation::NotMeasurable(w.clone())
                }
                _ => Observation::NotMeasurable("P2/P3 gave no percentage".into()),
            }
        }
        "D1.corpus_syntax_pass" => syntax_obs(&par.syntax, c, &i.held),
        _ => Observation::NotMeasurable(format!("no M1 runner for `{id}`")),
    }
}

fn recovery_obs(i: &Inputs, c: &TierCheck) -> Observation {
    let r = match &i.recovery {
        None => {
            return Observation::NotMeasurable(format!(
                "{} does not exist (the canonical seeded-error suite, FOUNDER-QUEUE Q14)",
                suite_note(suites::SEEDED)
            ))
        }
        Some(r) => r,
    };
    let ev = format!(
        "{} (canonical seeded-error suite, FOUNDER-QUEUE Q14), diagnostics via {}: per file genuine/reported, 0 unless \
         the seeded (code, line) is reported; dev files {} ({} cases in all 3 skins), score sum {:.3}; secondary: first \
         reported error is seeded in {} files, no cascade in {}; files scored 0 because they crashed or could not be \
         checked {}; skipped clean cases {}; deferred to M2 (`[M2]` cases) {}; defects {}",
        suite_note(suites::SEEDED),
        r.route,
        r.files,
        r.full_skin_cases,
        r.score_sum,
        r.first_match,
        r.cascade_free,
        r.unchecked,
        r.skipped_clean,
        r.deferred_m2,
        r.defects.len()
    );
    if !r.defects.is_empty() {
        return Observation::NotMeasurable(format!("{ev}; defects: {:?}", r.defects));
    }
    if r.files == 0 {
        return Observation::NotMeasurable(format!("{ev}; no seeded file"));
    }
    let need_cases = 30;
    let held = i
        .held
        .aggregate()
        .map(|a| &a["diag_recovery"])
        .filter(|v| !v.is_null());
    let (files, sum, full, held_ev, dev_only) = match held {
        Some(h) => {
            let hf = h["files"].as_u64().unwrap_or(0) as usize;
            let hs = h["score_sum"].as_f64().unwrap_or(0.0);
            let hc = h["full_skin_cases"].as_u64().unwrap_or(0) as usize;
            if h["defects"].as_u64().unwrap_or(0) > 0 || hf == 0 {
                (
                    r.files,
                    r.score_sum,
                    r.full_skin_cases,
                    "held-out suite has defects or no file".to_string(),
                    true,
                )
            } else {
                (
                    r.files + hf,
                    r.score_sum + hs,
                    r.full_skin_cases + hc,
                    format!(
                        "held-out files {hf} ({hc} cases in 3 skins), score sum {hs:.3}, deferred to M2 {}",
                        h["deferred_m2"].as_u64().unwrap_or(0)
                    ),
                    false,
                )
            }
        }
        None => (
            r.files,
            r.score_sum,
            r.full_skin_cases,
            format!(
                "held-out: {}",
                if i.held.aggregate().is_some() {
                    "no seeded-errors suite in the store".into()
                } else {
                    i.held.why_not()
                }
            ),
            true,
        ),
    };
    let value = sum / files as f64;
    let ev = format!("{ev}; {held_ev}; value {value:.4}");
    if full < need_cases {
        return Observation::NotMeasurable(format!(
            "{ev}; the scope needs >= {need_cases} cases x 3 skins, found {full} (check scope: {})",
            c.scope
        ));
    }
    Observation::Measured {
        value: MetricValue::Num(value),
        evidence: ev,
        dev_only,
        definitive_fail: false,
    }
}

fn load_suite(root: &Path, name: &str) -> Result<Option<Vec<SuiteCase>>, String> {
    suites::load(&root.join(suites::SUITES_DIR).join(name))
}

fn obs_to_measure(o: &Observation) -> Measure {
    match o {
        Observation::Measured {
            value,
            evidence,
            dev_only,
            ..
        } => Measure::Measured {
            value: value.clone(),
            note: Some(if *dev_only {
                format!("{evidence} [dev part only]")
            } else {
                evidence.clone()
            }),
        },
        Observation::Passed(e) => Measure::Measured {
            value: MetricValue::Bool(true),
            note: Some(e.clone()),
        },
        Observation::Failed(e) => Measure::Measured {
            value: MetricValue::Bool(false),
            note: Some(e.clone()),
        },
        Observation::NotMeasurable(e) => Measure::NotMeasurable(e.clone()),
    }
}

pub fn run(sc: &Scorecard, opts: &Options) -> Result<Run, String> {
    let root = &opts.root;
    let hook = opts.hook.as_ref();
    let tier = sc
        .tiers
        .resolve(&opts.tier)
        .ok_or_else(|| {
            format!(
                "unknown --target `{}`: expected one of {:?} (or mvp)",
                opts.tier, sc.tiers.order
            )
        })?
        .id
        .clone();
    let target = sc.tiers.normalization(&tier);
    let read = |rel: &str| fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}: {e}"));

    // --- data metrics over spec files (stage S0)
    let keywords_text = read("spec/keywords.json")?;
    let rows: Vec<Value> =
        serde_json::from_str(&keywords_text).map_err(|e| format!("spec/keywords.json: {e}"))?;
    let counts: KeywordCounts = checks::keyword_counts(&rows);
    let synonyms = checks::synonym_offenders(&rows);
    let table_load = KeywordTable::load(&keywords_text)
        .map(|_| ())
        .map_err(|e| e.to_string());

    let spec_text = read("spec/KALAM-SPEC.md")?;
    let spec_productions = checks::grammar_productions(&spec_text, false);
    let file_productions = fs::read_to_string(root.join("compiler/grammar/kalam.ebnf"))
        .ok()
        .and_then(|t| checks::grammar_productions(&t, true));

    // --- the M1 runners: dev corpus, shared suites, held-out store, external reports
    let corpus = corpus_run::run(&root.join("eval/corpus"), hook)?;
    // FOUNDER-QUEUE Q10: `[M2]` cases are checked from tier M2 on. The Trojan Source suite is
    // judged with them (dev and held-out) whenever the target tier is M2 or ratchets from it.
    let from_m2 = sc.tiers.chain(&tier).iter().any(|t| t.id == "M2");
    let trojan_rules = RejectRules {
        name_codepoints: true,
        out_of_scope: vec![],
        include_m2: from_m2,
    };
    let reject = |name: &str, rules: &RejectRules| -> Result<Option<RejectResult>, String> {
        Ok(load_suite(root, name)?.map(|c| suites::run_reject(hook, &c, rules)))
    };
    let held = match &opts.holdout {
        None => Held::Missing(
            "the held-out runner was not started (library call without a runner)".into(),
        ),
        Some(r) => holdout::run_child(
            &holdout::Runner {
                include_m2: from_m2,
                kalam: r.kalam.clone().or_else(|| opts.kalam.clone()),
                ..r.clone()
            },
            root,
        ),
    };
    let holdout_store = opts
        .holdout
        .as_ref()
        .map(|r| r.store.clone())
        .unwrap_or_else(|| PathBuf::from(integrity::DEFAULT_HOLDOUT));
    let inputs = Inputs {
        trojan: reject(suites::TROJAN, &trojan_rules)?,
        lexhaz: reject(
            suites::LEXHAZ,
            &RejectRules {
                name_codepoints: false,
                out_of_scope: vec!["SEC0104"],
                include_m2: false,
            },
        )?,
        mixed: reject(
            suites::MIXED,
            &RejectRules {
                name_codepoints: false,
                out_of_scope: vec!["K0301"],
                include_m2: false,
            },
        )?,
        recovery: load_suite(root, suites::SEEDED)?.map(|c| suites::run_recovery(hook, &c)),
        conformance: load_suite(root, suites::CONFORMANCE)?.map(|c| suites::run_fmt(hook, &c)),
        props: match (&opts.props_report, opts.run_props) {
            (Some(path), _) => external::load_props_at(path, opts.git_sha.as_deref()),
            (None, true) => run_props(&root.join("compiler")),
            (None, false) => external::load_props(root, opts.git_sha.as_deref()),
        },
        fuzz: external::fuzz(root),
        catalog: diagcat::measure(&spec_text, &diagcat::builtin_entries()),
        p7: p7_conflicts(&rows, &table_load),
        trojan_m2: from_m2,
        held,
        corpus,
    };
    let integrity_report: IntegrityReport = integrity::run(
        root,
        &integrity::CheckOptions {
            eval_version: &sc.eval_version,
            holdout_store: &holdout_store,
            holdout_reported: inputs
                .held
                .aggregate()
                .and_then(|a| a["manifest_sha256"].as_str()),
        },
    );
    let leak_hits = inputs
        .held
        .aggregate()
        .and_then(|a| a["leak"]["hits"].as_u64())
        .unwrap_or(0);

    // --- M2 runners (conformance, goldens, fix rate, judge, wider scopes, checkpoints)
    let raw_scorecard: Value = serde_json::from_str(&read("eval/scorecard.json")?)
        .map_err(|e| format!("eval/scorecard.json: {e}"))?;
    let checkpoint_defs: Vec<Value> = raw_scorecard["milestone_tiers"]["tiers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["id"] == json!(tier))
        .and_then(|t| t["checkpoints"].as_array().cloned())
        .unwrap_or_default();
    let checkpoint_ids: Vec<String> = checkpoint_defs
        .iter()
        .filter_map(|c| c["id"].as_str().map(str::to_string))
        .collect();
    let seeded = load_suite(root, suites::SEEDED)?.unwrap_or_default();
    let m2 = m2::compute(
        root,
        hook,
        &spec_text,
        opts.kalam.as_deref(),
        &seeded,
        &checkpoint_ids,
        inputs.held.aggregate().map(|a| &a["typecheck"]),
    );
    let m2_def = |id: &str| {
        sc.tiers
            .get("M2")
            .and_then(|t| t.checks.iter().find(|c| c.id == id))
    };

    // --- M1 check observations
    let m1 = sc.tiers.get("M1");
    let m1_def = |id: &str| m1.and_then(|t| t.checks.iter().find(|c| c.id == id));
    let mut obs: BTreeMap<&str, Observation> = BTreeMap::new();
    for id in M1_MEASURED {
        if let Some(c) = m1_def(id) {
            obs.insert(id, observe_m1(id, &inputs, c));
        }
    }

    // --- measurements per metric id
    let mut measures: BTreeMap<&str, Measure> = BTreeMap::new();
    for (id, o) in &obs {
        if sc.metric(id).is_some() {
            measures.insert(id, obs_to_measure(o));
        }
    }
    // From tier M2 on (M2 itself or any tier that ratchets from it), the metric rows of the
    // checks M2 widens are reported at the M2 scope, so a row never claims the narrower M1
    // value for the metric's full definition. An M1-target run is unchanged.
    if sc.tiers.chain(&tier).iter().any(|t| t.id == "M2") {
        for id in [
            "D5.diag_completeness",
            "D2.lexing_hazard_suite",
            "D1.fuzz_crash_free_hours",
        ] {
            let o = m2.observe_wider(id, obs.get(id), root);
            measures.insert(id, obs_to_measure(&o));
        }
    }
    measures.insert("D1.spec_clause_coverage", m2.spec_clause_coverage());
    measures.insert("D1.conformance_pass_rate", m2.conformance_pass_rate());
    measures.insert("D5.golden_coverage", m2.golden_coverage());
    measures.insert("D5.machine_fix_rate", m2.machine_fix_rate());
    measures.insert("D5.judge_score", m2.judge_score());
    measures.insert(
        "D4.keywords_per_skin",
        Measure::Measured {
            value: MetricValue::Num(counts.keywords_max as f64),
            note: Some(format!(
                "per skin (keyword+ascii_fixed, +core_name): {:?}",
                counts.per_skin
            )),
        },
    );
    measures.insert(
        "D4.reserved_spellings_per_skin",
        Measure::Measured {
            value: MetricValue::Num(counts.reserved_max as f64),
            note: None,
        },
    );
    measures.insert(
        "D4.synonym_count",
        Measure::Measured {
            value: MetricValue::Bool(synonyms.is_empty()),
            note: (!synonyms.is_empty()).then(|| format!("concepts with synonyms: {synonyms:?}")),
        },
    );
    measures.insert(
        "D4.grammar_productions",
        match (spec_productions, file_productions) {
            (_, Some(n)) => Measure::Measured {
                value: MetricValue::Num(n as f64),
                note: Some(format!(
                    "compiler/grammar/kalam.ebnf; spec §8.2 block has {spec_productions:?}"
                )),
            },
            (Some(n), None) => Measure::Measured {
                value: MetricValue::Num(n as f64),
                note: Some(
                    "spec §8.2 EBNF block (compiler/grammar/kalam.ebnf does not exist yet)".into(),
                ),
            },
            (None, None) => Measure::NotMeasurable("spec §8.2 EBNF block not found".into()),
        },
    );

    // --- every scorecard metric
    let mut metric_rows = Vec::new();
    let mut dim_scores: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let (mut n_measured, mut n_unmeasurable, mut n_excluded) = (0, 0, 0);
    for def in &sc.metrics {
        let incl = inclusion(def, target);
        let measure = measures
            .get(def.id.as_str())
            .cloned()
            .unwrap_or_else(|| Measure::NotMeasurable(default_reason(sc, def)));
        let (status, value, normalized, note) = match (&measure, incl) {
            (Measure::Measured { value, note }, Inclusion::Counted) => {
                n_measured += 1;
                (
                    "measured",
                    Some(value.clone()),
                    normalize(def, value),
                    note.clone(),
                )
            }
            (Measure::Measured { value, note }, Inclusion::ReportOnly) => {
                n_excluded += 1;
                ("report_only", Some(value.clone()), None, note.clone())
            }
            (Measure::Measured { value, note }, Inclusion::NotApplicable) => {
                n_excluded += 1;
                ("n/a", Some(value.clone()), None, note.clone())
            }
            (Measure::NotMeasurable(why), Inclusion::Counted) => {
                n_unmeasurable += 1;
                ("not_measurable", None, Some(0.0), Some(why.clone()))
            }
            (Measure::NotMeasurable(why), Inclusion::ReportOnly) => {
                n_excluded += 1;
                (
                    "report_only",
                    None,
                    None,
                    Some(format!("not measurable: {why}")),
                )
            }
            (Measure::NotMeasurable(why), Inclusion::NotApplicable) => {
                n_excluded += 1;
                ("n/a", None, None, Some(format!("not measurable: {why}")))
            }
        };
        if incl == Inclusion::Counted {
            dim_scores
                .entry(def.dimension.clone())
                .or_default()
                .push(normalized.unwrap_or(0.0));
        }
        metric_rows.push(json!({
            "dimension": def.dimension,
            "metric": def.id,
            "status": status,
            "value": value.as_ref().map(MetricValue::to_json),
            "normalized": normalized.map(round2),
            "threshold_mvp": def.thresholds[0].to_json(),
            "threshold_beta": def.thresholds[1].to_json(),
            "threshold_1_0": def.thresholds[2].to_json(),
            "gate": def.gate,
            "stage": def.stage,
            "note": note,
        }));
    }

    let mut dimensions = Map::new();
    let mut composite = 0.0;
    for d in &sc.dimensions {
        let scores = dim_scores.get(&d.id).cloned().unwrap_or_default();
        let score = if scores.is_empty() {
            0.0
        } else {
            scores.iter().sum::<f64>() / scores.len() as f64
        };
        composite += d.weight * score / 100.0;
        dimensions.insert(d.id.clone(), json!(round2(score)));
    }

    // --- gate check states, judged by the M1 tier check definitions (pending unless the
    //     whole check was measured)
    let m1_state = |id: &str| -> &'static str {
        let (Some(c), Some(o)) = (m1_def(id), obs.get(id)) else {
            return "pending";
        };
        let th = sc
            .metric(id)
            .and_then(|d| d.thresholds[target.index()].num().map(|t| (d.direction, t)));
        match tiers::evaluate_check(c, o, th).0 {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::NotMeasurable => "pending",
        }
    };
    let g_par_checks: Map<String, Value> =
        ["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9", "P10"]
            .iter()
            .map(|p| (p.to_string(), json!(m1_state(&format!("G-PAR.{p}")))))
            .collect();
    let gate_state = |states: &[&str]| {
        if states.contains(&"fail") {
            "fail"
        } else if states.iter().all(|s| *s == "pass") {
            "pass"
        } else {
            "pending"
        }
    };
    let par_states: Vec<&str> = g_par_checks.values().filter_map(Value::as_str).collect();
    let trojan_state = m1_state("D2.trojan_source_suite");
    let lexhaz_state = m1_state("D2.lexing_hazard_suite");
    // G-FUZZ fails on a crash; too few crash-free hours is the time-accumulated M1 check
    // (`D1.fuzz_crash_free_hours`), not a gate failure.
    let fuzz_crashes = external::fuzz_crashes(root);
    let fuzz_state = if fuzz_crashes.is_some_and(|c| c > 0) {
        "fail"
    } else {
        "pending"
    };
    let int_state = if leak_hits > 0 {
        State::Fail
    } else {
        integrity_report.state()
    };
    let mut gates = json!({
        "G-INT": {
            "state": int_state.as_str(),
            "reason": format!(
                "{}{}",
                integrity_report.reason(),
                if leak_hits > 0 { format!(" | held-out leak: {leak_hits} held-out identifier(s) found in compiler sources") } else { String::new() }
            ),
            "checks": integrity_report.to_json()["checks"],
        },
        "G-PAR": {
            "state": gate_state(&par_states),
            "checks": g_par_checks,
            "note": "Each check's state is its M1 tier check judgment (pending = not fully measured, held-out included). \
                     P5, P9 and P10 have no M1 runner and stay pending, so G-PAR as a whole cannot pass at M1.",
        },
        "G-SEC": {
            "state": gate_state(&[trojan_state, lexhaz_state, "pending"]),
            "checks": {"vulnerable_cases": "pending", "trojan_source_suite": trojan_state, "lexing_hazard_suite": lexhaz_state},
        },
        "G-FUZZ": {
            "state": fuzz_state,
            "reason": format!(
                "fails on any unfixed crash in {} (crashes = {fuzz_crashes:?}); the deep-job check (15 min per target) \
                 has no runner, so it never passes; D1.fuzz_crash_free_hours is judged as its own M1 check",
                external::FUZZ_STATUS
            ),
        },
        "G-RATCHET": {"state": "pending", "reason": "computed after the tier checks"},
    });

    // --- cargo test (opt-in)
    let cargo = if opts.run_cargo_test {
        run_cargo_test(&root.join("compiler"))
    } else {
        json!({"status": "skipped", "reason": "pass --cargo-test to run `cargo test --workspace`"})
    };

    let c = &inputs.corpus;
    let reject_json = |r: &Option<RejectResult>| match r {
        None => json!({"status": "missing"}),
        Some(r) => json!({
            "bad_rejected": r.bad_passed, "bad_total": r.bad_total, "good_accepted": r.good_passed,
            "good_total": r.good_total, "deferred_m2": r.deferred_m2, "out_of_scope": r.out_of_scope, "defects": r.defects,
            "failures": r.failures, "later_stage": r.later_stage,
            "reject_skins": r.reject_skins.iter().map(|s| s.id()).collect::<Vec<_>>(),
        }),
    };
    let fmt_json = |t: &FmtTally| {
        json!({"passed": t.passed, "total": t.total, "layout_kept": t.layout_kept, "excluded": t.excluded,
               "deferred_m2": t.deferred_m2, "failures": t.failures, "defects": t.defects})
    };
    let checks_json = json!({
        "keyword_table_load": {"pass": table_load.is_ok(), "error": table_load.clone().err(), "scope": "spec §7.4 rules 1-4"},
        "corpus_lex": {
            "tasks": c.tasks().len(), "cases": c.cases.len(), "case_skin_pairs": c.lex_pairs, "passed": c.lex_pass,
            "rate_pct": pct(c.lex_pass, c.lex_pairs),
            "rule": "run/http cases and later-stage compile-error cases must lex cleanly; lex-stage expected diagnostics must match code, line and code points",
        },
        "security_lex": {"case_skin_pairs": c.sec_pairs, "passed": c.sec_pass, "rate_pct": pct(c.sec_pass, c.sec_pairs)},
        "lex_roundtrip": {
            "checks": c.roundtrip.checks,
            "failed": c.roundtrip.failures.len(),
            "rate_pct": pct(c.roundtrip.checks - c.roundtrip.failures.len(), c.roundtrip.checks),
            "failures": c.roundtrip.failures,
            "scope": "kalam_fmt::convert: X->Y->X for 6 ordered pairs, 3-hop cycles, fixpoint, en->Y == committed Y (byte level)",
        },
        "cargo_test": cargo,
        "parity": {
            "hook": hook.name(),
            "diagnostics": hook.diag_route(),
            "p1": outcome_json(&c.parity.p1),
            "p2": outcome_json(&c.parity.p2),
            "p3": outcome_json(&c.parity.p3),
            "corpus_syntax": syntax_json(&c.parity.syntax),
            "corpus_fmt": fmt_json(&c.fmt),
            "scope": "G-PAR scope rule: a case rejected in all three skins is unsupported (costs D1), not a parity \
                      failure; lex-stage rejection cases (T50 main) are outside P1-P3. Dev corpus only.",
        },
        "suites": {
            "trojan-source": reject_json(&inputs.trojan),
            "lexing-hazards": reject_json(&inputs.lexhaz),
            "mixed-skin": reject_json(&inputs.mixed),
            "seeded-errors": match &inputs.recovery {
                None => json!({"status": "missing"}),
                Some(r) => json!({"files": r.files, "value": r.value(), "full_skin_cases": r.full_skin_cases,
                                  "first_match": r.first_match, "cascade_free": r.cascade_free,
                                  "deferred_m2": r.deferred_m2, "defects": r.defects}),
            },
            "conformance": inputs.conformance.as_ref().map(fmt_json).unwrap_or_else(|| json!({"status": "missing"})),
        },
        "catalog": {
            "codes": inputs.catalog.total(), "p6_strict_pct": round2(inputs.catalog.p6_pct()),
            "message_and_explain_pct": round2(inputs.catalog.message_and_explain_pct()),
            "evidence": inputs.catalog.evidence(),
        },
        "p7_conflicts": inputs.p7.1,
        "m2": m2.checks_json(),
        "props": match &inputs.props { Ok(p) => json!({"source": p.source, "provenance": format!("{:?}", p.provenance), "properties": p.properties.len()}), Err(e) => json!({"status": "not_measurable", "reason": e}) },
        "cases": c.records,
    });

    // --- milestone tier (SCORECARD.md §10, LOOP.md §5.1, §8), then G-RATCHET over it
    let ratchet_cell: std::cell::RefCell<Option<Ratchet>> = std::cell::RefCell::new(None);
    let gates_cell = std::cell::RefCell::new(gates.clone());
    let gate_obs = |state: &Value, why: &Value| {
        let why = why.as_str().map(str::to_string);
        match state.as_str() {
            Some("pass") => Observation::Passed(why.unwrap_or_else(|| "gate passed".into())),
            Some("fail") => Observation::Failed(why.unwrap_or_else(|| "gate failed".into())),
            other => Observation::NotMeasurable(
                why.unwrap_or_else(|| format!("gate state {}", other.unwrap_or("missing"))),
            ),
        }
    };
    let observe = |c: &TierCheck| -> Observation {
        let id = c.id.as_str();
        // a check M2 repeats with a wider scope than M1 (src/m2.rs)
        if let (Some(d2), Some(d1)) = (m2_def(id), m1_def(id)) {
            if d2.scope == c.scope && d1.scope != c.scope {
                return m2.observe_wider(id, obs.get(id), root);
            }
        }
        if let (Some(o), Some(d)) = (obs.get(id), m1_def(id)) {
            return if d.scope == c.scope {
                o.clone()
            } else {
                Observation::NotMeasurable(format!(
                    "{id} is measured at its M1 scope only ({}); this tier's scope is: {}",
                    d.scope, c.scope
                ))
            };
        }
        if id == "G-RATCHET" {
            return match ratchet_cell.borrow().as_ref() {
                None => Observation::NotMeasurable("computed after the other tier checks".into()),
                Some(r) => gate_obs(&json!(r.state()), &json!(r.reason())),
            };
        }
        let gates = gates_cell.borrow();
        if let Some(p) = id.strip_prefix("G-PAR.") {
            let st = &gates["G-PAR"]["checks"][p];
            return gate_obs(st, &json!(format!("G-PAR {p}: {st}")));
        }
        if id == "G-SEC.cwe_vulnerable" {
            let st = &gates["G-SEC"]["checks"]["vulnerable_cases"];
            return gate_obs(st, &json!(format!("G-SEC vulnerable_cases: {st}")));
        }
        if let Some(g) = gates.get(id) {
            let why = g
                .get("reason")
                .cloned()
                .unwrap_or_else(|| json!(format!("{id}: {}", g["state"])));
            return gate_obs(&g["state"], &why);
        }
        match measures.get(id) {
            Some(Measure::Measured { value, note }) => Observation::Measured {
                value: value.clone(),
                evidence: format!(
                    "{id} = {}{}",
                    value.to_json(),
                    note.as_deref()
                        .map(|n| format!(" ({n})"))
                        .unwrap_or_default()
                ),
                dev_only: false,
                definitive_fail: false,
            },
            Some(Measure::NotMeasurable(why)) => Observation::NotMeasurable(why.clone()),
            None => Observation::NotMeasurable(match sc.metric(id) {
                Some(def) => default_reason(sc, def),
                None => format!("no harness measurer for `{id}`"),
            }),
        }
    };
    let threshold_of = |id: &str| {
        sc.metric(id)
            .and_then(|d| d.thresholds[target.index()].num().map(|t| (d.direction, t)))
    };
    let dims: Vec<(String, f64)> = dimensions
        .iter()
        .map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0)))
        .collect();
    let rule = ScorecardRule {
        milestone: target,
        dimensions: &dims,
        composite: round2(composite),
        dimension_min: sc.dimension_min[target.index()],
        composite_min: sc.composite_min[target.index()],
        human_gates: &sc.human_gates[target.index()],
    };
    let first = tiers::evaluate(&sc.tiers, &tier, &observe, &threshold_of, Some(&rule))?;
    let history = fs::read_to_string(root.join("eval/history.jsonl")).unwrap_or_default();
    let current: Vec<Value> = first.checks.iter().map(|c| c.to_json()).collect();
    let r = ratchet::evaluate(&history, &sc.eval_version, &tier, &current);
    gates["G-RATCHET"] = json!({"state": r.state(), "reason": r.reason()});
    *gates_cell.borrow_mut() = gates.clone();
    *ratchet_cell.borrow_mut() = Some(r);
    let tier_eval = tiers::evaluate(&sc.tiers, &tier, &observe, &threshold_of, Some(&rule))?;

    let json = json!({
        "schema": "kalam-eval-scorecard/1",
        "eval_version": sc.eval_version,
        "target_milestone": tier,
        "normalized_against": target.key(),
        "summary": {
            "metrics_total": sc.metrics.len(), "measured": n_measured,
            "not_measurable": n_unmeasurable, "excluded_n_a_or_report_only": n_excluded,
        },
        "dimensions": dimensions,
        "composite": round2(composite),
        "gates": gates,
        "metrics": metric_rows,
        "holdout": inputs.held.to_json(),
        "checkpoints": m2.checkpoints_json(&checkpoint_defs),
        "checks": checks_json,
    });
    let mut json = json;
    if let (Some(obj), Value::Object(t)) = (json.as_object_mut(), tier_eval.to_json()) {
        obj.extend(t);
    }
    Ok(Run { json })
}

/// `D1.corpus_syntax_pass` against one tier check: dev tasks in the check's scope plus the
/// held-out runner's `m1_syntax` count.
fn syntax_obs(o: &SyntaxOutcome, c: &TierCheck, held: &Held) -> Observation {
    let (passed, failed, undecided, format_unavailable) = match o {
        SyntaxOutcome::Unavailable(why) => return Observation::NotMeasurable(why.clone()),
        SyntaxOutcome::Measured {
            passed,
            failed,
            undecided,
            format_unavailable,
        } => (passed, failed, undecided, format_unavailable),
    };
    let scope = parity::scope_tasks(&c.scope);
    let in_scope =
        |t: &String| scope.is_empty() || parity::task_number(t).is_some_and(|n| scope.contains(&n));
    let passed: Vec<&String> = passed.iter().filter(|t| in_scope(t)).collect();
    let failed: Vec<&(String, String)> = failed.iter().filter(|(t, _)| in_scope(t)).collect();
    let undecided: Vec<&String> = undecided.iter().filter(|t| in_scope(t)).collect();
    let dev_total = passed.len() + failed.len() + undecided.len();
    let evidence = format!(
        "dev corpus, {dev_total} in-scope tasks: passed {passed:?}; undecided {undecided:?}; failed {:?}",
        failed
            .iter()
            .map(|(t, why)| format!("{t}: {why}"))
            .collect::<Vec<_>>()
    );
    if let Some(why) = format_unavailable {
        return Observation::NotMeasurable(format!("{evidence}; {why}"));
    }
    let held_part = held_counts(held, &["m1_syntax"], "tasks_passed", "tasks_total").and_then(|hp| {
        let status = held.aggregate().map(|a| a["m1_syntax"]["status"].clone());
        if status == Some(json!("measured")) {
            Ok(hp)
        } else {
            Err("the held-out runner could not judge its tasks (parser or formatter unavailable)".into())
        }
    });
    match held_part {
        Err(why) => {
            // a failed in-scope dev task is definitive once the rest can no longer reach it
            let definitive = c.value.is_some_and(|t| {
                !failed.is_empty() && !scope.is_empty() && ((scope.len() - failed.len()) as f64) < t
            });
            Observation::Measured {
                value: MetricValue::Num(passed.len() as f64),
                evidence: format!("{evidence}; held-out: {why}"),
                dev_only: true,
                definitive_fail: definitive,
            }
        }
        Ok((hp, ht)) => Observation::Measured {
            value: MetricValue::Num((passed.len() + hp) as f64),
            evidence: format!(
                "{evidence}; held-out {hp}/{ht} in-scope tasks; {} of the {} scope tasks exist (dev + held-out)",
                dev_total + ht,
                scope.len()
            ),
            dev_only: false,
            definitive_fail: false,
        },
    }
}

fn outcome_json(o: &Outcome) -> Value {
    match o {
        Outcome::Unavailable(why) => json!({"status": "not_measurable", "reason": why}),
        Outcome::Measured {
            passed,
            total,
            failures,
        } => json!({
            "status": "measured", "passed": passed, "total": total,
            "rate_pct": pct(*passed, *total), "failures": failures,
        }),
    }
}

fn syntax_json(o: &SyntaxOutcome) -> Value {
    match o {
        SyntaxOutcome::Unavailable(why) => json!({"status": "not_measurable", "reason": why}),
        SyntaxOutcome::Measured {
            passed,
            failed,
            undecided,
            format_unavailable,
        } => json!({
            // without the formatter no task can pass: the parse-level counts are evidence only
            "status": if format_unavailable.is_some() { "not_measurable" } else { "measured" },
            "passed": passed, "undecided": undecided,
            "failed": failed.iter().map(|(t, why)| json!({"task": t, "reason": why})).collect::<Vec<_>>(),
            "format_unavailable": format_unavailable,
        }),
    }
}

fn default_reason(sc: &Scorecard, def: &MetricDef) -> String {
    match def.id.as_str() {
        "D3.context_footprint_tokens" | "D4.tokens_vs_python" => {
            "needs the pinned tiktoken o200k_base tokenizer (eval/harness/requirements.lock); no approximation is substituted".into()
        }
        "D1.compiler_line_coverage" | "D1.compiler_branch_coverage" => {
            "cargo llvm-cov is not wired into the harness yet".into()
        }
        _ => format!("no harness runner yet; needs stage {} ({})", def.stage, sc.stage_name(&def.stage)),
    }
}

/// The props report path and the harness's cargo `--target-dir`, both absolute. Child
/// processes run in other directories (the props test runs in `crates/kalam-props`), so a
/// relative `--root` such as `..` must not leak into a path they read. A relative path is
/// joined to the harness's working directory (not `canonicalize`, which gives verbatim
/// `\\?\` paths on Windows; `std::path::absolute` is above the crate's MSRV 1.75).
fn harness_paths(compiler_dir: &Path) -> (PathBuf, PathBuf) {
    let dir = if compiler_dir.is_absolute() {
        compiler_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(compiler_dir))
            .unwrap_or_else(|_| compiler_dir.to_path_buf())
    };
    (
        dir.join("target")
            .join("kalam-eval-props")
            .join("report.json"),
        dir.join("target").join("kalam-eval-cargo-test"),
    )
}

/// Runs the property suite on this tree and reads the report it writes. The report path
/// is the harness's own (under `target/`), so an old report can never be read as fresh.
fn run_props(compiler_dir: &Path) -> Result<external::PropsReport, String> {
    let (report, target_dir) = harness_paths(compiler_dir);
    let _ = fs::remove_file(&report);
    if let Some(dir) = report.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    Command::new("cargo")
        .args(["test", "-p", "kalam-props", "--release", "--target-dir"])
        .arg(&target_dir)
        .env("KALAM_PROPS_REPORT", &report)
        .current_dir(compiler_dir)
        .output()
        .map_err(|e| format!("could not run cargo test -p kalam-props: {e}"))?;
    // the suite fails when a property fails, and still writes its report
    external::load_fresh(&report)
}

fn run_cargo_test(compiler_dir: &Path) -> Value {
    let (_, target_dir) = harness_paths(compiler_dir);
    let out = Command::new("cargo")
        .args(["test", "--workspace", "--no-fail-fast", "--target-dir"])
        .arg(&target_dir)
        .current_dir(compiler_dir)
        .output();
    match out {
        Err(e) => json!({"status": "error", "error": format!("could not run cargo: {e}")}),
        Ok(o) => {
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            let t = checks::parse_cargo_test(&text);
            let build_failed = !o.status.success() && t.suites == 0;
            let compile_errors: Vec<String> = text
                .lines()
                .filter(|l| l.starts_with("error"))
                .take(10)
                .map(str::to_string)
                .collect();
            json!({
                "status": if build_failed { "build_failed" } else { "ran" },
                "exit_ok": o.status.success(),
                "suites": t.suites, "passed": t.passed, "failed": t.failed, "ignored": t.ignored,
                "pass_rate_pct": pct(t.passed as usize, (t.passed + t.failed) as usize),
                "errors": compile_errors,
            })
        }
    }
}

fn pct(n: usize, d: usize) -> Value {
    if d == 0 {
        Value::Null
    } else {
        json!(round2(n as f64 * 100.0 / d as f64))
    }
}

pub fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap()
    }

    fn run_with(tier: &str, hook: Box<dyn ParseHook>) -> Value {
        let root = repo_root();
        let sc = Scorecard::parse(&fs::read_to_string(root.join("eval/scorecard.json")).unwrap())
            .unwrap();
        run(
            &sc,
            &Options {
                root,
                tier: tier.into(),
                run_cargo_test: false,
                hook,
                git_sha: None,
                holdout: None,
                run_props: false,
                props_report: None,
                kalam: None,
            },
        )
        .unwrap()
        .json
    }

    fn run_repo() -> Value {
        run_with("mvp", crate::parity::default_hook())
    }

    fn tier_check<'a>(v: &'a Value, id: &str, tier: &str) -> &'a Value {
        v["tier_checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id && c["tier"] == tier)
            .unwrap_or_else(|| panic!("no tier check {id} ({tier})"))
    }

    /// A parser that accepts every file and gives every file the same KCF: P1/P2 hold on
    /// the dev corpus, which must still never make a held-out-scoped check pass.
    struct PerfectFake;

    impl ParseHook for PerfectFake {
        fn name(&self) -> String {
            "perfect fake".into()
        }
        fn probe(&self) -> Result<(), String> {
            Ok(())
        }
        fn probe_format(&self) -> Result<(), String> {
            Ok(())
        }
        fn parse(&self, _: &str, _: kalam_base::Skin) -> Result<crate::parity::Parsed, String> {
            Ok(crate::parity::Parsed {
                errors: vec![],
                kcf: b"same".to_vec(),
            })
        }
        fn convert(
            &self,
            t: &str,
            _: kalam_base::Skin,
            _: kalam_base::Skin,
        ) -> Result<String, String> {
            Ok(t.to_string())
        }
        fn format(&self, t: &str, _: kalam_base::Skin) -> Result<crate::parity::FmtOut, String> {
            Ok(crate::parity::FmtOut {
                text: t.to_string(),
                layout_kept: false,
            })
        }
    }

    #[test]
    fn m1_target_reports_every_tier_check_and_is_not_met() {
        let v = run_with("M1", crate::parity::default_hook());
        assert_eq!(v["target_milestone"], "M1");
        // at M1 the `[M2]` Trojan Source cases stay deferred (FOUNDER-QUEUE Q10)
        assert!(
            v["checks"]["suites"]["trojan-source"]["deferred_m2"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert_eq!(v["normalized_against"], "mvp");
        assert_eq!(v["tier_mode"], "checklist");
        assert!(v["tier_score"].is_number());
        assert_eq!(v["milestone_met"], false);
        let checks = v["tier_checks"].as_array().unwrap();
        // 18 checks + 3 ratchet_only metrics
        assert_eq!(checks.len(), 21);
        for c in checks {
            let st = c["status"].as_str().unwrap();
            assert!(["pass", "fail", "not_measurable"].contains(&st), "{c}");
            assert!(c["evidence"].is_string(), "{c}");
        }
        // without a held-out run and a reviewer sign-off these can never pass
        for id in [
            "G-INT",
            "G-PAR.P1",
            "G-PAR.P2",
            "G-PAR.P3",
            "D1.corpus_syntax_pass",
            "D2.trojan_source_suite",
            "D7.skin_convert",
            "D7.formatter_idempotence",
        ] {
            assert_ne!(tier_check(&v, id, "M1")["status"], "pass", "{id}");
        }
        // the S0 data metrics are measured, so their ratchet checks are decided
        let kw = tier_check(&v, "D4.keywords_per_skin", "M1");
        assert_eq!(kw["kind"], "ratchet_only");
        assert_ne!(kw["status"], "not_measurable", "{kw}");
        // P7 is fully measured at its M1 scope
        assert_ne!(tier_check(&v, "G-PAR.P7", "M1")["status"], "not_measurable");
        // P6 / D5.diag_completeness: measured, with the M1-scope ruling in the evidence
        let p6 = tier_check(&v, "G-PAR.P6", "M1");
        assert!(
            p6["evidence"].as_str().unwrap().contains("in M1 scope"),
            "{p6}"
        );
        assert!(v["holdout"]["status"] == "not_run");
    }

    #[test]
    fn a_perfect_dev_parser_measures_parity_but_never_passes_held_out_checks() {
        let v = run_with("M1", Box::new(PerfectFake));
        let p1 = &v["checks"]["parity"]["p1"];
        assert_eq!(p1["status"], "measured", "{p1}");
        assert_eq!(p1["rate_pct"], 100.0);
        assert_eq!(v["gates"]["G-PAR"]["checks"]["P1"], "pending");
        for id in [
            "G-PAR.P1",
            "G-PAR.P2",
            "G-PAR.P3",
            "D1.corpus_syntax_pass",
            "D7.skin_convert",
        ] {
            let c = tier_check(&v, id, "M1");
            assert_eq!(c["status"], "not_measurable", "{c}");
            assert!(c["evidence"].as_str().unwrap().contains("held-out"), "{c}");
        }
        let syn = v["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["metric"] == "D1.corpus_syntax_pass")
            .unwrap();
        assert!(syn["value"].is_number(), "{syn}");
        assert_eq!(v["milestone_met"], false);
    }

    /// An "accept everything" compiler (no resolver, no type checker, no renderer, no
    /// binary) must not pass any M2 check or checkpoint, and the M1 checks M2 ratchets from
    /// must still be listed and judged.
    #[test]
    fn an_accept_everything_compiler_passes_no_m2_check_and_m1_checks_still_run() {
        let v = run_with("M2", Box::new(PerfectFake));
        let checks = v["tier_checks"].as_array().unwrap();
        let m2: Vec<&Value> = checks.iter().filter(|c| c["tier"] == "M2").collect();
        assert_eq!(m2.len(), 8, "{m2:?}");
        // D5.diag_completeness is measured from the real diagnostic catalog, not through the
        // fake compiler, so it may legitimately pass here
        for c in m2.iter().filter(|c| c["id"] != "D5.diag_completeness") {
            assert_ne!(c["status"], "pass", "{c}");
        }
        let root = repo_root();
        let sc = Scorecard::parse(&fs::read_to_string(root.join("eval/scorecard.json")).unwrap())
            .unwrap();
        for c in &sc.tiers.get("M1").unwrap().checks {
            let r = tier_check(&v, &c.id, "M1");
            assert!(r["status"].is_string(), "{r}");
        }
        // the wider-scope M2 checks are judged by the M2 runners, not the M1 ones
        let lex = tier_check(&v, "D2.lexing_hazard_suite", "M2");
        assert!(
            lex["evidence"].as_str().unwrap().contains("M2 scope"),
            "{lex}"
        );
        // the metric rows of widened checks carry the M2-scope value, not the M1 one
        let row = |id: &str| -> Value {
            v["metrics"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["metric"] == id)
                .unwrap()
                .clone()
        };
        let dc = row("D5.diag_completeness");
        assert!(
            dc["note"].as_str().unwrap().contains("Full code set"),
            "{dc}"
        );
        let lx = row("D2.lexing_hazard_suite");
        assert!(lx["note"].as_str().unwrap().contains("M2 scope"), "{lx}");
        let conf = &v["checks"]["m2"]["conformance"]["run"];
        assert!(
            conf["stage_missing"]["resolve"].as_u64().unwrap() > 0,
            "{conf}"
        );
        assert!(
            conf["stage_missing"]["typeck"].as_u64().unwrap() > 0,
            "{conf}"
        );
        let cps = v["checkpoints"].as_array().unwrap();
        assert_eq!(cps.len(), 4, "{cps:?}");
        // spec clause coverage counts conformance cases, not compiler behaviour, so it may
        // pass; every checkpoint that runs the compiler must not
        for c in cps.iter().filter(|c| c["id"] != "D1.spec_clause_coverage") {
            assert_ne!(c["status"], "pass", "{c}");
        }
        // without a binary, the K0301 part of P8 is not measured, so P8 at M2 cannot pass
        let p8 = tier_check(&v, "G-PAR.P8", "M2");
        assert_ne!(p8["status"], "pass", "{p8}");
        assert!(
            p8["evidence"].as_str().unwrap().contains("K0301 part"),
            "{p8}"
        );
        // FOUNDER-QUEUE Q10: from M2 the `[M2]` Trojan Source cases are judged, not deferred
        assert_eq!(
            v["checks"]["suites"]["trojan-source"]["deferred_m2"], 0,
            "{}",
            v["checks"]["suites"]["trojan-source"]
        );
        assert_eq!(v["milestone_met"], false);
    }

    #[test]
    fn mvp_alias_is_the_m5_scorecard_tier() {
        let v = run_repo();
        assert_eq!(v["target_milestone"], "M5");
        assert_eq!(v["tier_mode"], "scorecard");
        assert_eq!(v["tier_score"], Value::Null);
        assert_eq!(v["milestone_met"], false);
        assert!(v["tier_checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "composite_min"));
    }

    #[test]
    fn every_scorecard_metric_is_reported_once_and_unmeasured_never_pass() {
        let v = run_repo();
        let metrics = v["metrics"].as_array().unwrap();
        let ids: std::collections::BTreeSet<_> = metrics
            .iter()
            .map(|m| m["metric"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), metrics.len());
        for m in metrics {
            if m["status"] == "not_measurable" {
                assert_eq!(m["value"], Value::Null, "{m}");
                assert_eq!(m["normalized"], json!(0.0), "{m}");
                assert!(
                    m["note"].is_string(),
                    "unmeasured metric needs a reason: {m}"
                );
            }
        }
        // tokenizer metrics are never approximated
        let ctx = metrics
            .iter()
            .find(|m| m["metric"] == "D3.context_footprint_tokens")
            .unwrap();
        assert_eq!(ctx["status"], "not_measurable");
        // the stale "needs stage S2" wording is gone
        let text = v.to_string();
        assert!(
            !text.contains("needs stage S2 (parser"),
            "stale S2 evidence"
        );
    }

    #[test]
    fn no_gate_passes_while_its_checks_are_partial() {
        let v = run_repo();
        for (id, g) in v["gates"].as_object().unwrap() {
            if id != "G-RATCHET" {
                assert_ne!(g["state"], "pass", "{id} must not pass on partial evidence");
            }
        }
    }

    #[test]
    fn composite_is_the_weighted_dimension_mean() {
        let v = run_repo();
        let dims = v["dimensions"].as_object().unwrap();
        let weights = [
            ("D1", 18.0),
            ("D2", 16.0),
            ("D3", 16.0),
            ("D4", 12.0),
            ("D5", 8.0),
            ("D6", 10.0),
            ("D7", 10.0),
            ("D8", 10.0),
        ];
        let expect: f64 = weights
            .iter()
            .map(|(d, w)| w * dims[*d].as_f64().unwrap() / 100.0)
            .sum();
        assert!((v["composite"].as_f64().unwrap() - expect).abs() < 0.05);
    }

    fn ran(v: Value) -> Held {
        let mut v = v;
        v["status"] = json!("measured");
        Held::Ran(v)
    }

    #[test]
    fn counts_combine_dev_and_held_out() {
        let dev = Outcome::Measured {
            passed: 10,
            total: 10,
            failures: vec![],
        };
        let missing = Held::Missing("no store".into());
        let o = combine_counts(
            "P1",
            &dev,
            held_counts(&missing, &["parity", "P1"], "passed", "total"),
        );
        assert!(
            matches!(o, Observation::Measured { dev_only: true, .. }),
            "{o:?}"
        );

        let h = ran(json!({"parity": {"P1": {"passed": 3, "total": 3}}}));
        match combine_counts(
            "P1",
            &dev,
            held_counts(&h, &["parity", "P1"], "passed", "total"),
        ) {
            Observation::Measured {
                value,
                dev_only,
                definitive_fail,
                ..
            } => {
                assert_eq!(value, MetricValue::Num(100.0));
                assert!(!dev_only && !definitive_fail);
            }
            o => panic!("{o:?}"),
        }
        let bad = ran(json!({"parity": {"P1": {"passed": 2, "total": 3}}}));
        assert!(matches!(
            combine_counts(
                "P1",
                &dev,
                held_counts(&bad, &["parity", "P1"], "passed", "total")
            ),
            Observation::Measured {
                definitive_fail: true,
                ..
            }
        ));
        // an empty held-out scope is not a pass
        let empty = ran(json!({"parity": {"P1": {"passed": 0, "total": 0}}}));
        assert!(held_counts(&empty, &["parity", "P1"], "passed", "total").is_err());
        // a store that is encrypted is not an aggregate
        let enc = Held::Ran(json!({"status": "encrypted"}));
        assert!(held_counts(&enc, &["parity", "P1"], "passed", "total")
            .unwrap_err()
            .contains("encrypted"));
    }

    #[test]
    fn syntax_check_counts_held_out_tasks() {
        let o = SyntaxOutcome::Measured {
            passed: (1..=18).map(|n| format!("t{n:02}-x")).collect(),
            failed: vec![],
            undecided: vec![],
            format_unavailable: None,
        };
        let c = TierCheck {
            id: "D1.corpus_syntax_pass".into(),
            op: tiers::Op::Ge,
            value: Some(21.0),
            unit: "tasks".into(),
            scope: "T01-T15, T31-T33, T48-T50".into(),
        };
        // dev passes 15 in-scope tasks (t16-t18 are out of scope)
        let held =
            ran(json!({"m1_syntax": {"status": "measured", "tasks_passed": 3, "tasks_total": 3}}));
        match syntax_obs(&o, &c, &held) {
            Observation::Measured {
                value, dev_only, ..
            } => {
                assert_eq!(value, MetricValue::Num(18.0));
                assert!(!dev_only);
            }
            other => panic!("{other:?}"),
        }
        let unjudged = ran(
            json!({"m1_syntax": {"status": "not_measurable", "tasks_passed": 0, "tasks_total": 3}}),
        );
        assert!(matches!(
            syntax_obs(&o, &c, &unjudged),
            Observation::Measured { dev_only: true, .. }
        ));
    }

    #[test]
    fn harness_child_paths_are_absolute_for_a_relative_root() {
        // `kalam-eval --root ..`: the props test process runs in crates/kalam-props, so a
        // relative KALAM_PROPS_REPORT or --target-dir would point somewhere else.
        let (report, target) = harness_paths(&Path::new("..").join("compiler"));
        assert!(report.is_absolute(), "{}", report.display());
        assert!(target.is_absolute(), "{}", target.display());
        assert!(report.ends_with("target/kalam-eval-props/report.json"));
        assert!(target.ends_with("target/kalam-eval-cargo-test"));
        assert!(!report.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn a_suite_with_unmeasurable_cases_never_passes() {
        let pass = || whole(true, "ev".into());
        let partial = vec!["sec0100-x/case.kl: not valid UTF-8".to_string()];
        assert_eq!(unless_partial(pass(), &[]), pass());
        match unless_partial(pass(), &partial) {
            Observation::NotMeasurable(e) => {
                assert!(e.contains("cannot pass") && e.contains("sec0100-x"), "{e}")
            }
            o => panic!("{o:?}"),
        }
        assert!(matches!(
            unless_partial(Observation::Passed("p".into()), &partial),
            Observation::NotMeasurable(_)
        ));
        // a definitive failure stands
        match unless_partial(whole(false, "ev".into()), &partial) {
            Observation::Measured {
                value: MetricValue::Bool(false),
                definitive_fail: true,
                ..
            } => {}
            o => panic!("{o:?}"),
        }
        // a dev-only pass (held-out missing) is also not a pass
        let dev_only = Observation::Measured {
            value: MetricValue::Bool(true),
            evidence: "ev".into(),
            dev_only: true,
            definitive_fail: false,
        };
        assert!(matches!(
            unless_partial(dev_only, &partial),
            Observation::NotMeasurable(_)
        ));
    }

    #[test]
    fn trojan_partial_counts_dev_and_held_out_unmeasurable_cases() {
        let dev = Some(RejectResult {
            not_measurable: vec!["a/case.kl: not valid UTF-8".into()],
            ..RejectResult::default()
        });
        let held = ran(json!({"trojan": {"bad_rejected": 1, "bad_total": 1, "not_measurable": 2}}));
        let p = trojan_partial(&dev, &held);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[1].contains("held-out: 2"), "{p:?}");
        // held-out alone is enough to block a pass
        let p = trojan_partial(&Some(RejectResult::default()), &held);
        assert_eq!(p.len(), 1);
        assert!(trojan_partial(&None, &ran(json!({"trojan": {"not_measurable": 0}}))).is_empty());
        assert!(trojan_partial(&None, &Held::Missing("no store".into())).is_empty());
    }

    #[test]
    fn p7_counts_rule_7_conflicts() {
        let rows: Vec<Value> = serde_json::from_str(
            &fs::read_to_string(repo_root().join("spec/keywords.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(p7_conflicts(&rows, &Ok(())).0, 0);
        let mut bad = rows.clone();
        for r in &mut bad {
            if r["concept"] == "KW_TRUE" {
                r["ar"] = json!("صحيح");
            }
        }
        let (n, why) = p7_conflicts(&bad, &Err("rule 1".into()));
        assert_eq!(n, 3, "{why:?}");
    }
}
