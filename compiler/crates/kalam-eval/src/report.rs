//! Assembles one scorecard run: every scorecard metric (measured or `not_measurable`),
//! dimension scores, composite, gate states and the supporting checks.
//!
//! Honesty rules (LOOP.md A2/A12, SCORECARD.md §1):
//! - a metric with no harness measurer is `not_measurable` and scores 0 in its dimension;
//! - a partial measurement never produces a pass: a metric or gate is `pass` only when its
//!   whole definition was checked, `fail` as soon as any checked part fails, else `pending`.

use crate::checks::{self, KeywordCounts, LexResult, RoundTrip};
use crate::corpus;
use crate::scorecard::{
    inclusion, normalize, Inclusion, MetricDef, MetricValue, Milestone, Scorecard,
};
use kalam_skins::KeywordTable;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Options {
    /// Repository root (holds `eval/`, `spec/`, `compiler/`).
    pub root: PathBuf,
    pub target: Milestone,
    pub run_cargo_test: bool,
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

pub fn run(sc: &Scorecard, opts: &Options) -> Result<Run, String> {
    let root = &opts.root;
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
    let ebnf_path = root.join("compiler/grammar/kalam.ebnf");
    let file_productions = fs::read_to_string(&ebnf_path)
        .ok()
        .and_then(|t| checks::grammar_productions(&t, true));

    // --- corpus checks (stage S1 lexer, token converter)
    let cases =
        corpus::discover(&root.join("eval/corpus")).map_err(|e| format!("eval/corpus: {e}"))?;
    let mut records = Vec::new();
    let (mut lex_pairs, mut lex_pass) = (0usize, 0usize);
    let (mut sec_pairs, mut sec_pass) = (0usize, 0usize);
    let mut rt_total = RoundTrip::default();
    for case in &cases {
        let mut sources = BTreeMap::new();
        let mut lexed: Vec<LexResult> = Vec::new();
        for f in &case.files {
            let text =
                fs::read_to_string(&f.path).map_err(|e| format!("{}: {e}", f.path.display()))?;
            let r = checks::lex_case(&text, f.skin, &f.staged, &case.expect);
            lex_pairs += 1;
            lex_pass += r.pass as usize;
            if r.lex_expectations > 0 {
                sec_pairs += 1;
                sec_pass += r.pass as usize;
            }
            lexed.push(r);
            sources.insert(f.skin, text);
        }
        let rt = checks::round_trip(&sources);
        rt_total.checks += rt.checks;
        rt_total.failures.extend(
            rt.failures
                .iter()
                .map(|f| format!("{}/{}: {f}", case.task, case.case)),
        );
        records.push(checks::case_record(case, &lexed, &rt));
    }
    let tasks: std::collections::BTreeSet<&str> = cases.iter().map(|c| c.task.as_str()).collect();

    // --- measurements per metric id
    let mut measures: BTreeMap<&str, Measure> = BTreeMap::new();
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
    // Trojan Source: a failed corpus case is a definitive fail of the binary suite metric;
    // all-pass on the corpus subset is not the ~60-case suite, so it stays unmeasured.
    measures.insert(
        "D2.trojan_source_suite",
        if sec_pass < sec_pairs {
            Measure::Measured {
                value: MetricValue::Bool(false),
                note: Some(format!(
                    "{} of {sec_pairs} corpus source-text cases failed at lex level (see checks.security_lex)",
                    sec_pairs - sec_pass
                )),
            }
        } else {
            Measure::NotMeasurable(format!(
                "the ~60-case Trojan Source suite does not exist yet; the {sec_pairs} corpus source-text case-skin pairs \
                 all pass at lex level (checks.security_lex), which is never scored as a suite pass"
            ))
        },
    );

    // --- every scorecard metric
    let mut metric_rows = Vec::new();
    let mut dim_scores: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let (mut n_measured, mut n_unmeasurable, mut n_excluded) = (0, 0, 0);
    for def in &sc.metrics {
        let incl = inclusion(def, opts.target);
        let measure = measures
            .remove(def.id.as_str())
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

    // --- gates
    let trojan_state = match measures_state(&metric_rows, "D2.trojan_source_suite") {
        Some(false) => "fail",
        _ => "pending",
    };
    let p7 = if table_load.is_err() {
        "fail"
    } else {
        "pending"
    };
    let g_par_checks: Map<String, Value> =
        ["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9", "P10"]
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    json!(if *p == "P7" { p7 } else { "pending" }),
                )
            })
            .collect();
    let g_par_state = if p7 == "fail" { "fail" } else { "pending" };
    let gates = json!({
        "G-INT": {
            "state": "pending",
            "reason": format!(
                "integrity checker not implemented; MANIFEST.sha256 {}, FREEZE.json {}",
                presence(&root.join("eval/MANIFEST.sha256")), presence(&root.join("eval/FREEZE.json"))
            ),
        },
        "G-PAR": {
            "state": g_par_state,
            "checks": g_par_checks,
            "note": "P7: spec/keywords.json loads and passes spec §7.4 rules 1-4 (rules 5-7 unchecked) => pending, not pass. \
                     P1-P3 need the parser/KCF; the lex-level token-converter proxy is in checks.lex_roundtrip.",
            "p7_rules_1_4": match &table_load { Ok(()) => json!("pass"), Err(e) => json!(format!("fail: {e}")) },
        },
        "G-SEC": {
            "state": if trojan_state == "fail" { "fail" } else { "pending" },
            "checks": {"vulnerable_cases": "pending", "trojan_source_suite": trojan_state, "lexing_hazard_suite": "pending"},
        },
        "G-FUZZ": {"state": "pending", "reason": "no fuzz targets (stage S10)"},
        "G-RATCHET": {"state": "pending", "reason": "no accepted baseline in eval/history.jsonl"},
    });

    // --- cargo test (opt-in)
    let cargo = if opts.run_cargo_test {
        run_cargo_test(&root.join("compiler"))
    } else {
        json!({"status": "skipped", "reason": "pass --cargo-test to run `cargo test --workspace`"})
    };

    let checks_json = json!({
        "keyword_table_load": {"pass": table_load.is_ok(), "error": table_load.err(), "scope": "spec §7.4 rules 1-4"},
        "corpus_lex": {
            "tasks": tasks.len(), "cases": cases.len(), "case_skin_pairs": lex_pairs, "passed": lex_pass,
            "rate_pct": pct(lex_pass, lex_pairs),
            "rule": "run/http cases and later-stage compile-error cases must lex cleanly; lex-stage expected diagnostics must match code, line and code points",
        },
        "security_lex": {"case_skin_pairs": sec_pairs, "passed": sec_pass, "rate_pct": pct(sec_pass, sec_pairs)},
        "lex_roundtrip": {
            "checks": rt_total.checks,
            "failed": rt_total.failures.len(),
            "rate_pct": pct(rt_total.checks - rt_total.failures.len(), rt_total.checks),
            "failures": rt_total.failures,
            "scope": "kalam_fmt::convert (token level): X->Y->X for 6 ordered pairs, 3-hop cycles, fixpoint, en->Y == committed Y. Proxy only: P2/P3 are defined on KCF.",
        },
        "cargo_test": cargo,
        "cases": records,
    });

    let json = json!({
        "schema": "kalam-eval-scorecard/1",
        "eval_version": sc.eval_version,
        "target_milestone": opts.target.key(),
        "summary": {
            "metrics_total": sc.metrics.len(), "measured": n_measured,
            "not_measurable": n_unmeasurable, "excluded_n_a_or_report_only": n_excluded,
        },
        "dimensions": dimensions,
        "composite": round2(composite),
        "gates": gates,
        "metrics": metric_rows,
        "checks": checks_json,
    });
    Ok(Run { json })
}

fn measures_state(rows: &[Value], id: &str) -> Option<bool> {
    rows.iter()
        .find(|r| r["metric"] == id && r["status"] == "measured")
        .and_then(|r| r["value"].as_bool())
}

fn default_reason(sc: &Scorecard, def: &MetricDef) -> String {
    match def.id.as_str() {
        "D3.context_footprint_tokens" | "D4.tokens_vs_python" => {
            "needs the pinned tiktoken o200k_base tokenizer (eval/harness/requirements.lock); no approximation is substituted".into()
        }
        "D1.property_tests" => "the kalam-props property suite does not exist yet".into(),
        "D1.compiler_line_coverage" | "D1.compiler_branch_coverage" => {
            "cargo llvm-cov is not wired into the harness yet".into()
        }
        "D7.skin_convert" | "D7.formatter_idempotence" => format!(
            "defined on KCF via the real `kalam convert`/`kalam fmt`; needs stage {} ({}). Lex-level proxy: checks.lex_roundtrip",
            def.stage,
            sc.stage_name(&def.stage)
        ),
        "D2.lexing_hazard_suite" => "the lexing-hazard suite (kalam-eval lexhaz) does not exist yet".into(),
        _ => format!("no harness runner yet; needs stage {} ({})", def.stage, sc.stage_name(&def.stage)),
    }
}

fn run_cargo_test(compiler_dir: &Path) -> Value {
    let target_dir = compiler_dir.join("target").join("kalam-eval-cargo-test");
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

fn presence(p: &Path) -> &'static str {
    if p.exists() {
        "present"
    } else {
        "missing"
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

    fn run_repo() -> Value {
        let root = repo_root();
        let sc = Scorecard::parse(&fs::read_to_string(root.join("eval/scorecard.json")).unwrap())
            .unwrap();
        run(
            &sc,
            &Options {
                root,
                target: Milestone::Mvp,
                run_cargo_test: false,
            },
        )
        .unwrap()
        .json
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
    }

    #[test]
    fn no_gate_passes_while_its_checks_are_partial() {
        let v = run_repo();
        for (id, g) in v["gates"].as_object().unwrap() {
            assert_ne!(g["state"], "pass", "{id} must not pass on partial evidence");
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
}
