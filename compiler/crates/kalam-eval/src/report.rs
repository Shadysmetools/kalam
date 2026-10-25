//! Assembles one scorecard run: every scorecard metric (measured or `not_measurable`),
//! dimension scores, composite, gate states and the supporting checks.
//!
//! Honesty rules (LOOP.md A2/A12, SCORECARD.md §1):
//! - a metric with no harness measurer is `not_measurable` and scores 0 in its dimension;
//! - a partial measurement never produces a pass: a metric or gate is `pass` only when its
//!   whole definition was checked, `fail` as soon as any checked part fails, else `pending`.

use crate::checks::{self, KeywordCounts, LexResult, RoundTrip};
use crate::corpus;
use crate::parity::{self, Outcome, ParityCase, ParseHook, SyntaxOutcome};
use crate::scorecard::{inclusion, normalize, Inclusion, MetricDef, MetricValue, Scorecard};
use crate::tiers::{self, Observation, ScorecardRule, TierCheck};
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
    /// Parser access for G-PAR P1/P2 and `D1.corpus_syntax_pass` ([`parity::default_hook`]).
    pub hook: Box<dyn ParseHook>,
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
    let mut parity_cases = Vec::new();
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
        parity_cases.push(ParityCase {
            task: case.task.clone(),
            case: case.case.clone(),
            sources: sources.clone(),
            // Outside P1-P3 only when it is meant to fail at lex level AND did not lex
            // cleanly in any skin (T50 main); a case that lexes still goes through parity.
            lex_stage_rejection: lexed.iter().any(|l| l.lex_expectations > 0)
                && lexed.iter().all(|l| !l.clean),
            lex_rejection_matched: lexed.len() == 3 && lexed.iter().all(|l| l.pass),
        });
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

    // --- parser-dependent checks (stage S2): P1, P2, D1.corpus_syntax_pass
    let par = parity::measure(opts.hook.as_ref(), &parity_cases);

    // --- measurements per metric id
    let mut measures: BTreeMap<&str, Measure> = BTreeMap::new();
    measures.insert("D1.corpus_syntax_pass", syntax_measure(&par.syntax));
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
    // P1/P2: a dev-corpus failure fails the check; a dev-corpus pass stays pending, because
    // G-PAR is judged on the frozen set held-out included.
    let outcome_state = |o: &Outcome| match o {
        Outcome::Measured { passed, total, .. } if passed < total => "fail",
        _ => "pending",
    };
    let (p1, p2) = (outcome_state(&par.p1), outcome_state(&par.p2));
    let g_par_checks: Map<String, Value> =
        ["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9", "P10"]
            .iter()
            .map(|p| {
                let state = match *p {
                    "P1" => p1,
                    "P2" => p2,
                    "P7" => p7,
                    _ => "pending",
                };
                (p.to_string(), json!(state))
            })
            .collect();
    let g_par_state = if [p1, p2, p7].contains(&"fail") {
        "fail"
    } else {
        "pending"
    };
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
                     P1/P2 come from the parser hook (checks.parity): fail on any dev-corpus failure, otherwise pending \
                     (held-out not run). P3 needs the KCF printer; the lex-level proxy is in checks.lex_roundtrip.",
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
        "parity": {
            "hook": opts.hook.name(),
            "p1": outcome_json(&par.p1),
            "p2": outcome_json(&par.p2),
            "corpus_syntax": syntax_json(&par.syntax),
            "scope": "G-PAR scope rule: a case rejected in all three skins is unsupported (costs D1), not a parity \
                      failure; lex-stage rejection cases (T50 main) are outside P1-P3. Dev corpus only.",
        },
        "cases": records,
    });

    // --- milestone tier (SCORECARD.md §10, LOOP.md §5.1, §8)
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
        if let Some(p) = id.strip_prefix("G-PAR.") {
            return match p {
                "P1" => parity_obs(&par.p1, "P1 canonical identity"),
                "P2" => parity_obs(&par.p2, "P2 pairwise and 3-hop round-trip"),
                _ => gate_obs(
                    &gates["G-PAR"]["checks"][p],
                    &json!(format!("G-PAR {p}: {}", gates["G-PAR"]["checks"][p])),
                ),
            };
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
        if id == "D1.corpus_syntax_pass" {
            return syntax_obs(&par.syntax, c);
        }
        match measures.get(id) {
            Some(Measure::Measured { value, note }) => {
                let dev_only = id == "D2.trojan_source_suite";
                Observation::Measured {
                    value: value.clone(),
                    evidence: format!(
                        "{id} = {}{}",
                        value.to_json(),
                        note.as_deref()
                            .map(|n| format!(" ({n})"))
                            .unwrap_or_default()
                    ),
                    dev_only,
                    // a dev-corpus failure of a 100% suite cannot be undone by held-out cases
                    definitive_fail: dev_only && *value == MetricValue::Bool(false),
                }
            }
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
        "checks": checks_json,
    });
    let mut json = json;
    if let (Some(obj), Value::Object(t)) = (json.as_object_mut(), tier_eval.to_json()) {
        obj.extend(t);
    }
    Ok(Run { json })
}

fn syntax_measure(o: &SyntaxOutcome) -> Measure {
    match o {
        SyntaxOutcome::Unavailable(why) => Measure::NotMeasurable(why.clone()),
        SyntaxOutcome::Measured {
            format_unavailable: Some(why),
            failed,
            undecided,
            ..
        } => Measure::NotMeasurable(format!(
            "needs parse, format and round-trip; {why}. Parse-level: {} task(s) undecided, {} failed",
            undecided.len(),
            failed.len()
        )),
        SyntaxOutcome::Measured { passed, failed, .. } => Measure::Measured {
            value: MetricValue::Num(passed.len() as f64),
            note: Some(format!(
                "dev corpus: passed {passed:?}; failed {:?}",
                failed.iter().map(|(t, _)| t).collect::<Vec<_>>()
            )),
        },
    }
}

/// `D1.corpus_syntax_pass` against one tier check: counts only the tasks the check's scope
/// names (all tasks if it names none). A failed in-scope task is definitive once the
/// remaining tasks can no longer reach the threshold.
fn syntax_obs(o: &SyntaxOutcome, c: &TierCheck) -> Observation {
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
    let evidence = format!(
        "dev corpus, in-scope tasks: passed {passed:?}; undecided {undecided:?}; failed {:?}",
        failed
            .iter()
            .map(|(t, why)| format!("{t}: {why}"))
            .collect::<Vec<_>>()
    );
    if let (false, Some(threshold), false) = (failed.is_empty(), c.value, scope.is_empty()) {
        if (scope.len().saturating_sub(failed.len()) as f64) < threshold {
            return Observation::Failed(evidence);
        }
    }
    if let Some(why) = format_unavailable {
        return Observation::NotMeasurable(format!("{evidence}; {why}"));
    }
    Observation::Measured {
        value: MetricValue::Num(passed.len() as f64),
        evidence,
        dev_only: true,
        definitive_fail: false,
    }
}

fn parity_obs(o: &Outcome, what: &str) -> Observation {
    match o {
        Outcome::Unavailable(why) => Observation::NotMeasurable(format!("{what}: {why}")),
        Outcome::Measured {
            passed,
            total,
            failures,
        } => Observation::Measured {
            value: MetricValue::Num(o.pct().unwrap_or(0.0)),
            evidence: format!(
                "{what}: {passed}/{total} on the dev corpus{}",
                failures
                    .first()
                    .map(|f| format!("; first failure: {f}"))
                    .unwrap_or_default()
            ),
            dev_only: true,
            definitive_fail: passed < total,
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
        fn format(&self, t: &str, _: kalam_base::Skin) -> Result<String, String> {
            Ok(t.to_string())
        }
    }

    #[test]
    fn m1_target_reports_every_tier_check_and_is_not_met() {
        let v = run_with("M1", crate::parity::default_hook());
        assert_eq!(v["target_milestone"], "M1");
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
        for id in [
            "G-INT",
            "G-RATCHET",
            "G-PAR.P1",
            "G-PAR.P2",
            "D1.corpus_syntax_pass",
            "D7.skin_convert",
        ] {
            assert_ne!(tier_check(&v, id, "M1")["status"], "pass", "{id}");
        }
        // the S0 data metrics are measured, so their ratchet checks are decided
        let kw = tier_check(&v, "D4.keywords_per_skin", "M1");
        assert_eq!(kw["kind"], "ratchet_only");
        assert_ne!(kw["status"], "not_measurable", "{kw}");
    }

    #[test]
    fn a_perfect_dev_parser_measures_parity_but_never_passes_held_out_checks() {
        let v = run_with("M1", Box::new(PerfectFake));
        let p1 = &v["checks"]["parity"]["p1"];
        assert_eq!(p1["status"], "measured", "{p1}");
        assert_eq!(p1["rate_pct"], 100.0);
        assert_eq!(v["gates"]["G-PAR"]["checks"]["P1"], "pending");
        for id in ["G-PAR.P1", "G-PAR.P2", "D1.corpus_syntax_pass"] {
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
