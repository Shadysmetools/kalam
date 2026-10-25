//! `D5.judge_score`: the Becker-rubric LLM judge (FOUNDER-QUEUE Q20).
//!
//! The harness never calls a model. It only:
//!
//! 1. **`kalam-eval judge-sample`** writes `eval/judge/sample.json`: a pinned, deterministic
//!    sample of rendered diagnostics. One item per golden code (`eval/goldens/<CODE>/`); the
//!    skin of each item is chosen by `sha256(seed + code)`, so every code is judged once and
//!    the three message languages rotate. Each item carries the source and the committed
//!    `.expected` text (the reviewed rendering). `sample_sha256` hashes the items.
//! 2. **`kalam-eval judge-ingest --in FILE`** validates scores produced outside (by the
//!    orchestrator's judge agent with the pinned rubric `eval/judge/RUBRIC.md`) and writes
//!    `eval/judge/report.json`:
//!    `{schema, model, rubric_version, rubric_sha256, sample_sha256, n, mean, scores: [{id, score, rationale?}]}`.
//!    Every sample id must be scored exactly once, every score must be a number in 0..=5,
//!    `model` must be named, and `rubric_version` must be the pinned one. The mean is
//!    recomputed here; an input `mean` that disagrees is rejected.
//! 3. The scorecard run reads the report. The value is its `mean`, but only when the report
//!    matches the current sample and rubric **and** the calibration that scorecard.json
//!    requires holds: `eval/judge/calibration.json` `{schema, grader, scores: [{id, score}]}`
//!    with >= 50 human grades of sample items whose mean absolute error against the judge is
//!    <= 0.5. Without it the metric is `not_measurable` (the mean is still shown).

use crate::goldens::GoldenFile;
use kalam_base::Skin;
use kalam_syntax::sha256;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const JUDGE_DIR: &str = "eval/judge";
pub const SAMPLE_PATH: &str = "eval/judge/sample.json";
pub const REPORT_PATH: &str = "eval/judge/report.json";
pub const RUBRIC_PATH: &str = "eval/judge/RUBRIC.md";
pub const CALIBRATION_PATH: &str = "eval/judge/calibration.json";
pub const SAMPLE_SCHEMA: &str = "kalam-judge-sample/1";
pub const REPORT_SCHEMA: &str = "kalam-judge-report/1";
pub const CALIBRATION_SCHEMA: &str = "kalam-judge-calibration/1";
pub const SEED: &str = "kalam-judge-sample/1";
pub const CALIBRATION_MIN: usize = 50;
pub const CALIBRATION_MAE: f64 = 0.5;

fn hex(b: &[u8]) -> String {
    sha256::hex(&sha256::digest(b))
}

/// `rubric_version: <v>` from the rubric's text.
pub fn rubric_version(rubric: &str) -> Option<String> {
    rubric.lines().find_map(|l| {
        l.trim()
            .trim_start_matches(['*', '-', ' '])
            .strip_prefix("rubric_version:")
            .map(|v| v.trim().trim_matches('`').to_string())
            .filter(|v| !v.is_empty())
    })
}

/// The sample document for these goldens and this rubric.
pub fn sample(goldens: &[GoldenFile], rubric: &str) -> Result<Value, String> {
    let version = rubric_version(rubric)
        .ok_or_else(|| format!("{RUBRIC_PATH} has no `rubric_version:` line"))?;
    let codes: BTreeSet<&str> = goldens.iter().map(|g| g.code.as_str()).collect();
    let mut items = Vec::new();
    for code in codes {
        let h = sha256::digest(format!("{SEED}{code}").as_bytes());
        let skin = Skin::ALL[h[0] as usize % Skin::ALL.len()];
        let Some(g) = goldens.iter().find(|g| g.code == code && g.skin == skin) else {
            continue;
        };
        let (Some(expected), false) = (&g.expected, g.source.is_empty()) else {
            continue;
        };
        items.push(json!({
            "id": format!("{code}/{}", skin.id()),
            "code": code,
            "skin": skin.id(),
            "message_language": skin.id(),
            "source": String::from_utf8_lossy(&g.source),
            "rendered": expected.strip_suffix('\n').unwrap_or(expected),
        }));
    }
    if items.is_empty() {
        return Err("no golden with a source and an .expected file to sample".into());
    }
    let items = Value::Array(items);
    Ok(json!({
        "schema": SAMPLE_SCHEMA,
        "seed": SEED,
        "rule": "one item per eval/goldens code; skin = Skin::ALL[sha256(seed + code)[0] % 3]; rendered = the committed .expected text",
        "rubric": RUBRIC_PATH,
        "rubric_version": version,
        "rubric_sha256": hex(rubric.as_bytes()),
        "n": items.as_array().map_or(0, Vec::len),
        "sample_sha256": hex(items.to_string().as_bytes()),
        "items": items,
    }))
}

/// Validates externally produced scores against `sample` and returns the report.
pub fn ingest(sample: &Value, input: &Value) -> Result<Value, String> {
    let model = input["model"]
        .as_str()
        .filter(|m| !m.trim().is_empty())
        .ok_or("`model` must name the judge model (exact id)")?;
    let version = input["rubric_version"]
        .as_str()
        .ok_or("`rubric_version` is missing")?;
    if Some(version) != sample["rubric_version"].as_str() {
        return Err(format!(
            "`rubric_version` {version:?} is not the pinned {}",
            sample["rubric_version"]
        ));
    }
    if let Some(s) = input.get("sample_sha256").and_then(Value::as_str) {
        if Some(s) != sample["sample_sha256"].as_str() {
            return Err("`sample_sha256` names another sample".into());
        }
    }
    let ids: Vec<&str> = sample["items"]
        .as_array()
        .ok_or("the sample has no items")?
        .iter()
        .filter_map(|i| i["id"].as_str())
        .collect();
    let scores = input["scores"]
        .as_array()
        .ok_or("`scores` must be an array of {id, score}")?;
    let mut seen: BTreeMap<&str, f64> = BTreeMap::new();
    let mut out = Vec::new();
    for s in scores {
        let id = s["id"].as_str().ok_or("every score needs a string `id`")?;
        if !ids.contains(&id) {
            return Err(format!("score for {id:?}, which is not a sample item"));
        }
        let v = s["score"]
            .as_f64()
            .filter(|v| (0.0..=5.0).contains(v))
            .ok_or_else(|| format!("{id}: `score` must be a number in 0..=5"))?;
        if seen.insert(id, v).is_some() {
            return Err(format!("{id} is scored more than once"));
        }
        let mut row = json!({"id": id, "score": v});
        if let Some(r) = s.get("rationale").filter(|r| r.is_string()) {
            row["rationale"] = r.clone();
        }
        out.push(row);
    }
    let missing: Vec<&&str> = ids.iter().filter(|i| !seen.contains_key(*i)).collect();
    if !missing.is_empty() {
        return Err(format!(
            "{} sample item(s) are not scored, e.g. {:?}",
            missing.len(),
            missing.iter().take(3).collect::<Vec<_>>()
        ));
    }
    let mean = seen.values().sum::<f64>() / seen.len() as f64;
    if let Some(m) = input.get("mean").and_then(Value::as_f64) {
        if (m - mean).abs() > 0.005 {
            return Err(format!(
                "the input mean {m} disagrees with the scores (mean {mean:.4})"
            ));
        }
    }
    Ok(json!({
        "schema": REPORT_SCHEMA,
        "model": model,
        "rubric_version": version,
        "rubric_sha256": sample["rubric_sha256"],
        "sample_sha256": sample["sample_sha256"],
        "n": out.len(),
        "mean": (mean * 10000.0).round() / 10000.0,
        "scores": out,
    }))
}

/// The `D5.judge_score` value: `Ok((mean, evidence))`, or `Err(why not measurable)`.
pub fn measure(
    current_sample: Result<Value, String>,
    committed_sample: Option<&str>,
    report: Option<&str>,
    calibration: Option<&str>,
) -> Result<(f64, String), String> {
    let current = current_sample?;
    let committed: Value = committed_sample
        .ok_or_else(|| {
            format!("{SAMPLE_PATH} does not exist (write it with `kalam-eval judge-sample`)")
        })
        .and_then(|t| serde_json::from_str(t).map_err(|e| format!("{SAMPLE_PATH}: {e}")))?;
    if committed["sample_sha256"] != current["sample_sha256"]
        || committed["rubric_sha256"] != current["rubric_sha256"]
    {
        return Err(format!(
            "{SAMPLE_PATH} is stale: the goldens or the rubric changed since it was written (re-run \
             `kalam-eval judge-sample`, then the judge)"
        ));
    }
    let report: Value = report
        .ok_or_else(|| format!("{REPORT_PATH} does not exist: the orchestrator has not run the judge on the pinned sample (FOUNDER-QUEUE Q20)"))
        .and_then(|t| serde_json::from_str(t).map_err(|e| format!("{REPORT_PATH}: {e}")))?;
    if report["schema"] != REPORT_SCHEMA
        || report["sample_sha256"] != current["sample_sha256"]
        || report["rubric_sha256"] != current["rubric_sha256"]
    {
        return Err(format!(
            "{REPORT_PATH} does not score the current sample and rubric (stale or hand-written); re-run the judge and `kalam-eval judge-ingest`"
        ));
    }
    let mean = report["mean"]
        .as_f64()
        .ok_or_else(|| format!("{REPORT_PATH} has no mean"))?;
    let judge: BTreeMap<String, f64> = report["scores"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| Some((s["id"].as_str()?.to_string(), s["score"].as_f64()?)))
        .collect();
    let ev = format!(
        "{REPORT_PATH}: model {}, rubric {}, {} items, mean {mean:.3}",
        report["model"], report["rubric_version"], report["n"]
    );
    let cal: Value = calibration
        .ok_or_else(|| {
            format!(
                "{ev}; but scorecard.json makes the judge unavailable until it is calibrated on >= {CALIBRATION_MIN} \
                 human-graded diagnostics with mean absolute error <= {CALIBRATION_MAE}, and {CALIBRATION_PATH} does not exist"
            )
        })
        .and_then(|t| serde_json::from_str(t).map_err(|e| format!("{ev}; {CALIBRATION_PATH}: {e}")))?;
    if cal["schema"] != CALIBRATION_SCHEMA || cal["grader"].as_str().is_none_or_empty() {
        return Err(format!(
            "{ev}; {CALIBRATION_PATH} needs schema {CALIBRATION_SCHEMA} and a named human `grader`"
        ));
    }
    let pairs: Vec<(f64, f64)> = cal["scores"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let h = s["score"].as_f64().filter(|v| (0.0..=5.0).contains(v))?;
            Some((h, *judge.get(s["id"].as_str()?)?))
        })
        .collect();
    if pairs.len() < CALIBRATION_MIN {
        return Err(format!(
            "{ev}; calibration has {} human grades of sample items, needs >= {CALIBRATION_MIN}",
            pairs.len()
        ));
    }
    let mae = pairs.iter().map(|(h, j)| (h - j).abs()).sum::<f64>() / pairs.len() as f64;
    if mae > CALIBRATION_MAE {
        return Err(format!(
            "{ev}; calibration mean absolute error {mae:.3} on {} items exceeds {CALIBRATION_MAE}, so the judge is not trusted",
            pairs.len()
        ));
    }
    Ok((
        mean,
        format!(
            "{ev}; calibrated: MAE {mae:.3} on {} human grades by {}",
            pairs.len(),
            cal["grader"]
        ),
    ))
}

trait NoneOrEmpty {
    fn is_none_or_empty(&self) -> bool;
}

impl NoneOrEmpty for Option<&str> {
    fn is_none_or_empty(&self) -> bool {
        self.map_or(true, |s| s.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUBRIC: &str = "# Rubric\n\nrubric_version: `becker-test.1`\n";

    fn goldens() -> Vec<GoldenFile> {
        let mut v = Vec::new();
        for code in ["K0901", "K0902", "K0903"] {
            for skin in Skin::ALL {
                v.push(GoldenFile {
                    code: code.into(),
                    skin,
                    source: b"x\n".to_vec(),
                    expected: Some(format!("-- {code} {}\n", skin.id())),
                });
            }
        }
        v
    }

    fn scores(s: &Value, v: f64) -> Value {
        json!({
            "model": "claude-test", "rubric_version": "becker-test.1",
            "scores": s["items"].as_array().unwrap().iter().map(|i| json!({"id": i["id"], "score": v})).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn the_sample_is_deterministic_one_item_per_code() {
        let a = sample(&goldens(), RUBRIC).unwrap();
        let b = sample(&goldens(), RUBRIC).unwrap();
        assert_eq!(a, b);
        assert_eq!(a["n"], 3);
        assert_eq!(a["rubric_version"], "becker-test.1");
        let item = &a["items"][0];
        assert!(item["rendered"].as_str().unwrap().starts_with("-- K0901"));
        assert!(!item["rendered"].as_str().unwrap().ends_with('\n'));
        assert!(sample(&goldens(), "no version").is_err());
        let mut g = goldens();
        g[0].expected = Some("changed".into());
        g[1].expected = Some("changed".into());
        g[2].expected = Some("changed".into());
        assert_ne!(
            sample(&g, RUBRIC).unwrap()["sample_sha256"],
            a["sample_sha256"]
        );
    }

    #[test]
    fn ingest_validates_everything() {
        let s = sample(&goldens(), RUBRIC).unwrap();
        let ok = ingest(&s, &scores(&s, 4.0)).unwrap();
        assert_eq!(ok["mean"], 4.0);
        assert_eq!(ok["n"], 3);
        let mut bad = scores(&s, 4.0);
        bad["model"] = json!("");
        assert!(ingest(&s, &bad).is_err());
        let mut bad = scores(&s, 4.0);
        bad["rubric_version"] = json!("other");
        assert!(ingest(&s, &bad).is_err());
        let mut bad = scores(&s, 6.0);
        assert!(ingest(&s, &bad).unwrap_err().contains("0..=5"));
        bad = scores(&s, 4.0);
        bad["scores"].as_array_mut().unwrap().pop();
        assert!(ingest(&s, &bad).unwrap_err().contains("not scored"));
        bad = scores(&s, 4.0);
        let first = bad["scores"][0].clone();
        bad["scores"].as_array_mut().unwrap().push(first);
        assert!(ingest(&s, &bad).unwrap_err().contains("more than once"));
        bad = scores(&s, 4.0);
        bad["mean"] = json!(4.5);
        assert!(ingest(&s, &bad).unwrap_err().contains("disagrees"));
    }

    #[test]
    fn the_score_needs_a_fresh_report_and_a_calibration() {
        let s = sample(&goldens(), RUBRIC).unwrap();
        let rep = ingest(&s, &scores(&s, 3.5)).unwrap().to_string();
        let st = s.to_string();
        let cur = || sample(&goldens(), RUBRIC);
        assert!(measure(cur(), None, Some(&rep), None)
            .unwrap_err()
            .contains("does not exist"));
        assert!(measure(cur(), Some(&st), None, None)
            .unwrap_err()
            .contains("report.json does not exist"));
        let err = measure(cur(), Some(&st), Some(&rep), None).unwrap_err();
        assert!(
            err.contains("mean 3.500") && err.contains("calibrat"),
            "{err}"
        );
        // stale: the goldens changed after the sample was pinned
        let mut g = goldens();
        g[0].expected = Some("new".into());
        g[1].expected = Some("new".into());
        g[2].expected = Some("new".into());
        assert!(measure(sample(&g, RUBRIC), Some(&st), Some(&rep), None)
            .unwrap_err()
            .contains("stale"));
        // calibration with too few grades
        let cal = json!({"schema": CALIBRATION_SCHEMA, "grader": "founder", "scores": [{"id": s["items"][0]["id"], "score": 3.0}]}).to_string();
        assert!(measure(cur(), Some(&st), Some(&rep), Some(&cal))
            .unwrap_err()
            .contains("needs >= 50"));
    }

    #[test]
    fn calibration_mae_is_computed_against_the_judge() {
        // a 60-code sample so that 50 human grades fit
        let mut g = Vec::new();
        for n in 0..60 {
            for skin in Skin::ALL {
                g.push(GoldenFile {
                    code: format!("K{n:04}"),
                    skin,
                    source: b"x\n".to_vec(),
                    expected: Some(format!("r {n}\n")),
                });
            }
        }
        let s = sample(&g, RUBRIC).unwrap();
        let rep = ingest(&s, &scores(&s, 4.0)).unwrap().to_string();
        let human = |v: f64| {
            json!({"schema": CALIBRATION_SCHEMA, "grader": "founder",
                   "scores": s["items"].as_array().unwrap().iter().take(50).map(|i| json!({"id": i["id"], "score": v})).collect::<Vec<_>>()})
            .to_string()
        };
        let (mean, ev) = measure(
            sample(&g, RUBRIC),
            Some(&s.to_string()),
            Some(&rep),
            Some(&human(3.6)),
        )
        .unwrap();
        assert_eq!(mean, 4.0);
        assert!(ev.contains("MAE 0.400"), "{ev}");
        assert!(measure(
            sample(&g, RUBRIC),
            Some(&s.to_string()),
            Some(&rep),
            Some(&human(3.0))
        )
        .unwrap_err()
        .contains("exceeds"));
    }

    #[test]
    fn reads_the_rubric_version() {
        assert_eq!(
            rubric_version("x\n- rubric_version: `b.1`\n").as_deref(),
            Some("b.1")
        );
        assert_eq!(rubric_version("nothing"), None);
    }
}
