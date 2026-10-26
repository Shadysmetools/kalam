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
//!    Each row records the rendered text it judged (`rendered_sha256`). With `--merge`
//!    ([`ingest_merge`]) the input scores only re-judged items and the fresh rows are kept.
//! 3. The scorecard run compares the report with the current sample **item by item**
//!    (FOUNDER-QUEUE Q43, [`staleness`]): an item whose rendered golden or the rubric changed
//!    since judging is stale, a sampled item the report does not score is unjudged, and either
//!    makes the metric `not_measurable` (`kalam-eval judge-stale` lists them). Otherwise the
//!    value is the mean judge score over the current sample, but only when the calibration
//!    that scorecard.json requires holds: `eval/judge/calibration.json`
//!    `{schema, grader, scores: [{id, score, rendered_sha256?}]}` with >= 50 human grades of
//!    current items whose mean absolute error against the judge is <= 0.5. Without it the
//!    metric is `not_measurable` (the mean is still shown).
//! 4. **`kalam-eval judge-calibration-kit`** writes `eval/judge/calibration-kit.csv`
//!    ([`kit_csv`]): 60 fresh items for a human to grade blind; **`kalam-eval
//!    judge-calibrate --grader NAME CSV`** reads the graded kit ([`read_grades`]) and writes
//!    the calibration ([`calibration_doc`]). The harness never writes a grade itself.

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
pub const KIT_PATH: &str = "eval/judge/calibration-kit.csv";
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
    let hashes = item_hashes(sample);
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
        let mut row = json!({"id": id, "score": v, "rendered_sha256": hashes.get(id)});
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

/// `sha256:<hex>` of a rendered diagnostic, as the judge saw it.
pub fn rendered_sha256(rendered: &str) -> String {
    format!("sha256:{}", hex(rendered.as_bytes()))
}

/// `id -> rendered_sha256` for every item of a sample document.
fn item_hashes(sample: &Value) -> BTreeMap<String, String> {
    sample["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| {
            Some((
                i["id"].as_str()?.to_string(),
                rendered_sha256(i["rendered"].as_str()?),
            ))
        })
        .collect()
}

/// `true` when `sample`'s items rehash to `sha` (the `sample_sha256` a report names).
fn sample_is(sample: &Value, sha: &Value) -> bool {
    sample["items"].is_array()
        && sha.as_str() == Some(hex(sample["items"].to_string().as_bytes()).as_str())
}

/// One judged item that no longer describes the current rendering (FOUNDER-QUEUE Q43).
#[derive(Clone, Debug, PartialEq)]
pub struct StaleItem {
    pub id: String,
    pub reason: String,
}

/// Which report items still score the current sample (FOUNDER-QUEUE Q43).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Staleness {
    /// Judged items whose rendered golden (or the rubric) changed since judging.
    pub stale: Vec<StaleItem>,
    /// Current sample items the report does not score (a new golden code, a new skin pick).
    pub unjudged: Vec<String>,
    /// Report items that are no longer in the sample; they are left out of the mean.
    pub removed: Vec<String>,
    /// `id -> judge score` for every current, non-stale item.
    pub fresh: BTreeMap<String, f64>,
    /// `id -> rendered_sha256` of the current sample.
    pub current: BTreeMap<String, String>,
}

impl Staleness {
    /// No stale and no unjudged item: the report scores every sampled item as it is now.
    pub fn is_clean(&self) -> bool {
        self.stale.is_empty() && self.unjudged.is_empty()
    }

    pub fn stale_ids(&self) -> Vec<&str> {
        self.stale.iter().map(|s| s.id.as_str()).collect()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "sampled_items": self.current.len(),
            "fresh_items": self.fresh.len(),
            "stale": self.stale.iter().map(|s| json!({"id": s.id, "reason": s.reason})).collect::<Vec<_>>(),
            "unjudged": self.unjudged,
            "removed": self.removed,
        })
    }

    fn summary(&self) -> String {
        let ids: Vec<&str> = self
            .stale
            .iter()
            .map(|s| s.id.as_str())
            .chain(self.unjudged.iter().map(String::as_str))
            .take(5)
            .collect();
        format!(
            "{} judged item(s) are stale (their rendered golden or the rubric changed since judging) and {} sampled \
             item(s) are unjudged, e.g. {ids:?}; FOUNDER-QUEUE Q43: list them with `kalam-eval judge-stale`, re-judge \
             them, then `kalam-eval judge-ingest --merge --in FILE`",
            self.stale.len(),
            self.unjudged.len()
        )
    }
}

/// Compares a judge report with the current sample, item by item (FOUNDER-QUEUE Q43).
///
/// The text an item was judged on is its row's `rendered_sha256` (written by [`ingest`]);
/// for a row written before that field existed, it is the committed sample's item, but only
/// when that sample is the one the report scored (its items rehash to the report's
/// `sample_sha256`). Without either, the judged text is unknown and the item is stale. A
/// report made under another rubric is stale throughout.
pub fn staleness(current: &Value, committed: Option<&Value>, report: &Value) -> Staleness {
    let cur = item_hashes(current);
    let legacy = committed
        .filter(|c| sample_is(c, &report["sample_sha256"]))
        .map(item_hashes)
        .unwrap_or_default();
    let rubric_changed = report["rubric_sha256"] != current["rubric_sha256"];
    let mut st = Staleness::default();
    let mut judged = BTreeSet::new();
    for row in report["scores"].as_array().into_iter().flatten() {
        let (Some(id), Some(score)) = (row["id"].as_str(), row["score"].as_f64()) else {
            continue;
        };
        judged.insert(id.to_string());
        let Some(now) = cur.get(id) else {
            st.removed.push(id.to_string());
            continue;
        };
        let then = row["rendered_sha256"]
            .as_str()
            .map(str::to_string)
            .or_else(|| legacy.get(id).cloned());
        let reason = match then {
            _ if rubric_changed => Some("the rubric changed since judging"),
            None => Some(
                "the judged text is unknown (no rendered_sha256 and the scored sample is gone)",
            ),
            Some(t) if &t != now => Some("its rendered golden changed since judging"),
            Some(_) => None,
        };
        match reason {
            Some(reason) => st.stale.push(StaleItem {
                id: id.to_string(),
                reason: reason.to_string(),
            }),
            None => {
                st.fresh.insert(id.to_string(), score);
            }
        }
    }
    st.unjudged = cur
        .keys()
        .filter(|id| !judged.contains(*id))
        .cloned()
        .collect();
    st.current = cur;
    st
}

/// Adds `rendered_sha256` to every row of `report` that lacks one, taken from the committed
/// sample the report scored, so the rows keep their judged text when `judge-sample` rewrites
/// that sample. Scores are untouched. `None` when there is nothing to add or `committed` is
/// not the scored sample.
pub fn pin_hashes(report: &Value, committed: &Value) -> Option<Value> {
    if !sample_is(committed, &report["sample_sha256"]) {
        return None;
    }
    let hashes = item_hashes(committed);
    let mut out = report.clone();
    let mut changed = false;
    for row in out["scores"].as_array_mut()? {
        if row.get("rendered_sha256").is_none() {
            if let Some(h) = row["id"].as_str().and_then(|id| hashes.get(id)) {
                row["rendered_sha256"] = json!(h);
                changed = true;
            }
        }
    }
    changed.then_some(out)
}

/// Parses the report (and the committed sample, for rows without `rendered_sha256`) and
/// compares it with `current`.
pub fn assess(
    current: &Value,
    committed_sample: Option<&str>,
    report: Option<&str>,
) -> Result<(Value, Staleness), String> {
    let report: Value = report
        .ok_or_else(|| format!("{REPORT_PATH} does not exist: the orchestrator has not run the judge on the pinned sample (FOUNDER-QUEUE Q20)"))
        .and_then(|t| serde_json::from_str(t).map_err(|e| format!("{REPORT_PATH}: {e}")))?;
    if report["schema"] != REPORT_SCHEMA {
        return Err(format!(
            "{REPORT_PATH} is not a {REPORT_SCHEMA} report (hand-written?); re-run the judge and `kalam-eval judge-ingest`"
        ));
    }
    let committed: Option<Value> = committed_sample.and_then(|t| serde_json::from_str(t).ok());
    let st = staleness(current, committed.as_ref(), &report);
    Ok((report, st))
}

/// Mean absolute error of human grades against the judge, over the ids both scored:
/// `(pairs, mae)`.
pub fn calibration_mae(judge: &BTreeMap<String, f64>, human: &[(String, f64)]) -> (usize, f64) {
    let pairs: Vec<(f64, f64)> = human
        .iter()
        .filter_map(|(id, h)| Some((*h, *judge.get(id)?)))
        .collect();
    if pairs.is_empty() {
        return (0, 0.0);
    }
    let mae = pairs.iter().map(|(h, j)| (h - j).abs()).sum::<f64>() / pairs.len() as f64;
    (pairs.len(), mae)
}

/// Re-judging after FOUNDER-QUEUE Q43: `input` scores only some items (at least every stale
/// and unjudged one); every other current item keeps its row from `existing`, which must be
/// fresh (`st`, computed against `sample`). The judge model must be the existing report's.
pub fn ingest_merge(
    sample: &Value,
    existing: &Value,
    st: &Staleness,
    input: &Value,
) -> Result<Value, String> {
    if input["model"] != existing["model"] {
        return Err(format!(
            "`model` {} is not the existing report's {}: a merge keeps one judge model (re-judge every item instead)",
            input["model"], existing["model"]
        ));
    }
    let mut scores = input["scores"]
        .as_array()
        .cloned()
        .ok_or("`scores` must be an array of {id, score}")?;
    let given: BTreeSet<String> = scores
        .iter()
        .filter_map(|s| s["id"].as_str().map(str::to_string))
        .collect();
    for row in existing["scores"].as_array().into_iter().flatten() {
        let Some(id) = row["id"].as_str() else {
            continue;
        };
        if st.fresh.contains_key(id) && !given.contains(id) {
            let mut kept = json!({"id": id, "score": row["score"]});
            if let Some(r) = row.get("rationale") {
                kept["rationale"] = r.clone();
            }
            scores.push(kept);
        }
    }
    let mut combined = input.clone();
    combined["scores"] = Value::Array(scores);
    if let Some(o) = combined.as_object_mut() {
        o.remove("mean");
    }
    ingest(sample, &combined)
}

/// The `D5.judge_score` value: `Ok((mean, evidence))`, or `Err(why not measurable)`.
///
/// Every sampled item must be judged on its current rendering (FOUNDER-QUEUE Q43): one stale
/// or unjudged item makes the metric not measurable. The mean is over the current sample.
pub fn measure(
    current_sample: Result<Value, String>,
    committed_sample: Option<&str>,
    report: Option<&str>,
    calibration: Option<&str>,
) -> Result<(f64, String), String> {
    let current = current_sample?;
    let (report, st) = assess(&current, committed_sample, report)?;
    let mean = if st.fresh.is_empty() {
        0.0
    } else {
        st.fresh.values().sum::<f64>() / st.fresh.len() as f64
    };
    let ev = format!(
        "{REPORT_PATH}: model {}, rubric {}, {} of {} sampled items judged on their current rendering, mean {mean:.3}{}",
        report["model"],
        report["rubric_version"],
        st.fresh.len(),
        st.current.len(),
        if st.removed.is_empty() {
            String::new()
        } else {
            format!(
                " ({} judged item(s) no longer sampled, left out)",
                st.removed.len()
            )
        }
    );
    if !st.is_clean() {
        return Err(format!("{ev}; {}", st.summary()));
    }
    let cal: Value = calibration
        .ok_or_else(|| {
            format!(
                "{ev}; but scorecard.json makes the judge unavailable until it is calibrated on >= {CALIBRATION_MIN} \
                 human-graded diagnostics with mean absolute error <= {CALIBRATION_MAE}, and {CALIBRATION_PATH} does not exist \
                 (a human grades {KIT_PATH}, then `kalam-eval judge-calibrate`)"
            )
        })
        .and_then(|t| serde_json::from_str(t).map_err(|e| format!("{ev}; {CALIBRATION_PATH}: {e}")))?;
    if cal["schema"] != CALIBRATION_SCHEMA || cal["grader"].as_str().is_none_or_empty() {
        return Err(format!(
            "{ev}; {CALIBRATION_PATH} needs schema {CALIBRATION_SCHEMA} and a named human `grader`"
        ));
    }
    let human: Vec<(String, f64)> = cal["scores"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let h = s["score"].as_f64().filter(|v| (0.0..=5.0).contains(v))?;
            let id = s["id"].as_str()?;
            // a grade of a rendering that has changed since grading no longer counts
            if let Some(g) = s["rendered_sha256"].as_str() {
                if st.current.get(id).map(String::as_str) != Some(g) {
                    return None;
                }
            }
            Some((id.to_string(), h))
        })
        .collect();
    let (n, mae) = calibration_mae(&st.fresh, &human);
    if n < CALIBRATION_MIN {
        return Err(format!(
            "{ev}; calibration has {n} human grades of sample items, needs >= {CALIBRATION_MIN}"
        ));
    }
    if mae > CALIBRATION_MAE {
        return Err(format!(
            "{ev}; calibration mean absolute error {mae:.3} on {n} items exceeds {CALIBRATION_MAE}, so the judge is not trusted"
        ));
    }
    Ok((
        (mean * 10000.0).round() / 10000.0,
        format!(
            "{ev}; calibrated: MAE {mae:.3} on {n} human grades by {}",
            cal["grader"]
        ),
    ))
}

// --- the calibration kit -------------------------------------------------------------

/// Items in the calibration kit (at least [`CALIBRATION_MIN`] must be graded).
pub const KIT_SIZE: usize = 60;
/// The kit's columns, in order.
pub const KIT_COLUMNS: &[&str] = &[
    "id",
    "code",
    "skin",
    "human_score",
    "notes",
    "rendered",
    "source",
    "rubric",
    "rendered_sha256",
];

/// The kit items: [`KIT_SIZE`] current, judged, non-stale sample items, ordered by
/// `sha256(seed + "/calibration/" + id)` (deterministic, independent of the judge's scores).
pub fn kit_items<'a>(current: &'a Value, st: &Staleness) -> Result<Vec<&'a Value>, String> {
    let mut pool: Vec<(String, &Value)> = current["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["id"].as_str().is_some_and(|id| st.fresh.contains_key(id)))
        .map(|i| {
            let id = i["id"].as_str().unwrap_or_default();
            (hex(format!("{SEED}/calibration/{id}").as_bytes()), i)
        })
        .collect();
    if pool.len() < CALIBRATION_MIN {
        return Err(format!(
            "only {} sampled item(s) are judged on their current rendering; the kit needs >= {CALIBRATION_MIN} \
             (re-judge the stale and unjudged items first: `kalam-eval judge-stale`)",
            pool.len()
        ));
    }
    pool.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pool.into_iter().take(KIT_SIZE).map(|(_, i)| i).collect())
}

/// A CSV field: always quoted (RFC 4180). A free-text field that starts with a character a
/// spreadsheet reads as a formula (`= + - @`, tab, CR) gets one leading space.
fn csv_field(s: &str, text: bool) -> String {
    let s = if text && s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!(" {s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// The calibration kit CSV (UTF-8 with a BOM, CRLF rows): one row per kit item, an empty
/// `human_score` column, never the judge's score or rationale (blind grading).
pub fn kit_csv(current: &Value, st: &Staleness) -> Result<(String, usize), String> {
    let items = kit_items(current, st)?;
    let rubric = format!(
        "{RUBRIC_PATH} ({})",
        current["rubric_version"].as_str().unwrap_or("?")
    );
    let mut out = String::from("\u{feff}");
    out.push_str(
        &KIT_COLUMNS
            .iter()
            .map(|c| csv_field(c, false))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push_str("\r\n");
    for i in &items {
        let s = |k: &str| i[k].as_str().unwrap_or_default().to_string();
        let row = [
            csv_field(&s("id"), false),
            csv_field(&s("code"), false),
            csv_field(&s("skin"), false),
            csv_field("", false),
            csv_field("", false),
            csv_field(&s("rendered"), true),
            csv_field(&s("source"), true),
            csv_field(&rubric, false),
            csv_field(&rendered_sha256(&s("rendered")), false),
        ];
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    Ok((out, items.len()))
}

/// Parses CSV text (RFC 4180 quoting; a leading BOM, CRLF or LF rows; the delimiter is the
/// first of `,` `;` tab found in the header line, as spreadsheets in other locales save it).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let header = text.lines().next().unwrap_or_default();
    let delim = [',', ';', '\t']
        .into_iter()
        .find(|d| header.contains(*d))
        .unwrap_or(',');
    let (mut rows, mut row, mut field) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) if c == delim => row.push(std::mem::take(&mut field)),
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

/// Graded kit rows: `(id, score)` pairs, and the graded rows left out (with the reason).
pub type Grades = (Vec<(String, f64)>, Vec<String>);

/// Reads human grades from a graded kit: `(id, score)` for every row with a `human_score`,
/// and the graded rows left out. Rows join on `id` only (other columns may be mangled by a
/// spreadsheet). A grade is left out, with its reason, when the item is stale or unjudged
/// now or its `rendered_sha256` no longer names the current rendering: the judge score it
/// would be compared with is not for the text the human read. A malformed score, an
/// unknown id or a repeated id rejects the whole file.
pub fn read_grades(csv: &str, st: &Staleness) -> Result<Grades, String> {
    let rows = parse_csv(csv);
    let header = rows.first().ok_or("the CSV is empty")?;
    let col = |name: &str| header.iter().position(|h| h.trim() == name);
    let id_col = col("id").ok_or("the CSV has no `id` column")?;
    let score_col = col("human_score").ok_or("the CSV has no `human_score` column")?;
    let hash_col = col("rendered_sha256");
    let mut seen = BTreeSet::new();
    let (mut grades, mut skipped) = (Vec::new(), Vec::new());
    for (n, row) in rows.iter().enumerate().skip(1) {
        let cell = |c: usize| row.get(c).map(|s| s.trim()).unwrap_or_default();
        let (id, raw) = (cell(id_col), cell(score_col));
        if raw.is_empty() {
            continue;
        }
        let line = n + 1;
        let score: f64 = raw
            .replace(',', ".")
            .parse()
            .ok()
            .filter(|v: &f64| (0.0..=5.0).contains(v))
            .ok_or_else(|| {
                format!("row {line} ({id}): human_score {raw:?} is not a number in 0..=5")
            })?;
        if !st.current.contains_key(id) {
            return Err(format!("row {line}: {id:?} is not a current sample item"));
        }
        if !seen.insert(id.to_string()) {
            return Err(format!("row {line}: {id} is graded more than once"));
        }
        if !st.fresh.contains_key(id) {
            skipped.push(format!(
                "{id}: stale or unjudged, so there is no judge score to compare yet"
            ));
            continue;
        }
        if let Some(h) = hash_col.map(cell).filter(|h| !h.is_empty()) {
            if Some(h) != st.current.get(id).map(String::as_str) {
                skipped.push(format!(
                    "{id}: graded on a rendering that has changed since the kit was written"
                ));
                continue;
            }
        }
        grades.push((id.to_string(), score));
    }
    Ok((grades, skipped))
}

/// The `eval/judge/calibration.json` document for `grades` by `grader`, with the MAE against
/// the judge. `Err` when the grader is unnamed or fewer than [`CALIBRATION_MIN`] items are
/// graded (nothing is written then).
pub fn calibration_doc(
    grader: &str,
    grades: &[(String, f64)],
    st: &Staleness,
    report: &Value,
) -> Result<(Value, f64), String> {
    if grader.trim().is_empty() {
        return Err("--grader must name the human who graded the kit".into());
    }
    let (n, mae) = calibration_mae(&st.fresh, grades);
    if n < CALIBRATION_MIN {
        return Err(format!(
            "{n} item(s) are graded; calibration needs >= {CALIBRATION_MIN} human grades"
        ));
    }
    let doc = json!({
        "schema": CALIBRATION_SCHEMA,
        "grader": grader.trim(),
        "rubric_version": report["rubric_version"],
        "rubric_sha256": report["rubric_sha256"],
        "judge_model": report["model"],
        "n": n,
        "mae": (mae * 10000.0).round() / 10000.0,
        "scores": grades.iter().map(|(id, s)| json!({"id": id, "score": s, "rendered_sha256": st.current.get(id)})).collect::<Vec<_>>(),
    });
    Ok((doc, mae))
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

    /// A `n`-code golden set (every code in all three skins).
    fn many(n: usize) -> Vec<GoldenFile> {
        let mut g = Vec::new();
        for k in 0..n {
            for skin in Skin::ALL {
                g.push(GoldenFile {
                    code: format!("K{k:04}"),
                    skin,
                    source: format!("-- source {k}, \"quoted\"\n").into_bytes(),
                    expected: Some(format!("-- K{k:04} {}\nline two, \"q\"\n", skin.id())),
                });
            }
        }
        g
    }

    /// Changes the rendering of `code` in every skin.
    fn rerender(g: &mut [GoldenFile], code: &str) {
        for f in g.iter_mut().filter(|f| f.code == code) {
            f.expected = Some(format!("-- {code} reworded\n"));
        }
    }

    fn report_for(s: &Value, v: f64) -> Value {
        ingest(s, &scores(s, v)).unwrap()
    }

    #[test]
    fn ingest_records_the_judged_rendering_per_item() {
        let s = sample(&goldens(), RUBRIC).unwrap();
        let rep = report_for(&s, 4.0);
        for (row, item) in rep["scores"]
            .as_array()
            .unwrap()
            .iter()
            .zip(s["items"].as_array().unwrap())
        {
            assert_eq!(row["id"], item["id"]);
            assert_eq!(
                row["rendered_sha256"],
                json!(rendered_sha256(item["rendered"].as_str().unwrap()))
            );
        }
    }

    #[test]
    fn an_item_whose_golden_changed_is_stale_and_blocks_the_metric() {
        let g = goldens();
        let s = sample(&g, RUBRIC).unwrap();
        let rep = report_for(&s, 4.0);
        let fresh = staleness(&s, Some(&s), &rep);
        assert!(fresh.is_clean(), "{fresh:?}");
        assert_eq!(fresh.fresh.len(), 3);
        // only K0902's rendering changes: that item alone is stale
        let mut g2 = g.clone();
        rerender(&mut g2, "K0902");
        let cur = sample(&g2, RUBRIC).unwrap();
        let st = staleness(&cur, Some(&s), &rep);
        assert_eq!(st.stale.len(), 1, "{st:?}");
        assert!(st.stale[0].id.starts_with("K0902/"), "{st:?}");
        assert_eq!(st.stale_ids(), vec![st.stale[0].id.as_str()]);
        assert!(st.stale[0].reason.contains("rendered golden changed"));
        assert_eq!(st.fresh.len(), 2);
        assert!(st.unjudged.is_empty() && st.removed.is_empty());
        let err = measure(Ok(cur), Some(&s.to_string()), Some(&rep.to_string()), None).unwrap_err();
        assert!(
            err.contains("1 judged item(s) are stale") && err.contains("K0902/"),
            "{err}"
        );
        assert!(err.contains("judge-stale"), "{err}");
        // a source-only change does not make the item stale
        let mut g3 = g.clone();
        for f in g3.iter_mut() {
            f.source = b"y\n".to_vec();
        }
        assert!(staleness(&sample(&g3, RUBRIC).unwrap(), None, &rep).is_clean());
    }

    #[test]
    fn a_rubric_change_makes_every_item_stale() {
        let s = sample(&goldens(), RUBRIC).unwrap();
        let rep = report_for(&s, 4.0);
        let cur = sample(&goldens(), "rubric_version: `becker-test.1`\nreworded\n").unwrap();
        let st = staleness(&cur, Some(&s), &rep);
        assert_eq!(st.stale.len(), 3);
        assert!(st.stale.iter().all(|i| i.reason.contains("rubric")));
    }

    #[test]
    fn new_codes_are_unjudged_and_removed_codes_are_left_out() {
        let mut g = goldens();
        let s = sample(&g, RUBRIC).unwrap();
        let rep = report_for(&s, 4.0);
        // a new golden code
        for skin in Skin::ALL {
            g.push(GoldenFile {
                code: "K0999".into(),
                skin,
                source: b"x\n".to_vec(),
                expected: Some("-- K0999\n".into()),
            });
        }
        let st = staleness(&sample(&g, RUBRIC).unwrap(), None, &rep);
        assert_eq!(st.unjudged.len(), 1);
        assert!(!st.is_clean());
        let err = measure(sample(&g, RUBRIC), None, Some(&rep.to_string()), None).unwrap_err();
        assert!(err.contains("1 sampled item(s) are unjudged"), "{err}");
        // a removed golden code: the rest stays measurable, the mean is over the sample
        let g: Vec<GoldenFile> = goldens()
            .into_iter()
            .filter(|f| f.code != "K0901")
            .collect();
        let st = staleness(&sample(&g, RUBRIC).unwrap(), None, &rep);
        assert!(st.is_clean(), "{st:?}");
        assert_eq!(st.removed.len(), 1);
        let err = measure(sample(&g, RUBRIC), None, Some(&rep.to_string()), None).unwrap_err();
        assert!(
            err.contains("2 of 2 sampled items") && err.contains("no longer sampled"),
            "{err}"
        );
        assert!(err.contains("calibrat"), "{err}");
    }

    #[test]
    fn a_report_without_item_hashes_uses_the_sample_it_scored() {
        let g = goldens();
        let s = sample(&g, RUBRIC).unwrap();
        let mut rep = report_for(&s, 4.0);
        for row in rep["scores"].as_array_mut().unwrap() {
            row.as_object_mut().unwrap().remove("rendered_sha256");
        }
        assert!(staleness(&s, Some(&s), &rep).is_clean());
        // without the scored sample the judged text is unknown
        let st = staleness(&s, None, &rep);
        assert_eq!(st.stale.len(), 3);
        assert!(st.stale[0].reason.contains("unknown"));
        // a doctored sample (items no longer hash to the report's sample_sha256) is ignored
        let mut doctored = s.clone();
        doctored["items"][0]["rendered"] = json!("other");
        assert_eq!(staleness(&s, Some(&doctored), &rep).stale.len(), 3);
        // pin_hashes keeps the judged text when the sample is rewritten; scores untouched
        let pinned = pin_hashes(&rep, &s).unwrap();
        assert_eq!(pinned["scores"][0]["score"], rep["scores"][0]["score"]);
        assert!(pin_hashes(&pinned, &s).is_none());
        assert!(pin_hashes(&rep, &doctored).is_none());
        let mut g2 = g.clone();
        rerender(&mut g2, "K0903");
        let st = staleness(&sample(&g2, RUBRIC).unwrap(), None, &pinned);
        assert_eq!(st.stale.len(), 1);
        assert_eq!(st.fresh.len(), 2);
    }

    #[test]
    fn a_merge_re_judges_only_the_stale_items() {
        let g = goldens();
        let s = sample(&g, RUBRIC).unwrap();
        let rep = report_for(&s, 4.0);
        let mut g2 = g.clone();
        rerender(&mut g2, "K0902");
        let cur = sample(&g2, RUBRIC).unwrap();
        let st = staleness(&cur, Some(&s), &rep);
        let id = st.stale[0].id.clone();
        let input = json!({"model": "claude-test", "rubric_version": "becker-test.1",
                           "scores": [{"id": id, "score": 2.0, "rationale": "re-judged"}]});
        let merged = ingest_merge(&cur, &rep, &st, &input).unwrap();
        assert_eq!(merged["n"], 3);
        assert_eq!(merged["mean"], 3.3333);
        assert!(staleness(&cur, None, &merged).is_clean());
        // another model may not merge into this report
        let mut other = input.clone();
        other["model"] = json!("another-model");
        assert!(ingest_merge(&cur, &rep, &st, &other)
            .unwrap_err()
            .contains("one judge model"));
        // a stale item left out is not carried over
        let empty =
            json!({"model": "claude-test", "rubric_version": "becker-test.1", "scores": []});
        assert!(ingest_merge(&cur, &rep, &st, &empty)
            .unwrap_err()
            .contains("not scored"));
    }

    #[test]
    fn the_kit_is_sixty_fresh_items_blind_to_the_judge() {
        let g = many(70);
        let s = sample(&g, RUBRIC).unwrap();
        let mut input = scores(&s, 4.25);
        for row in input["scores"].as_array_mut().unwrap() {
            row["rationale"] = json!("SECRET-RATIONALE");
        }
        let rep = ingest(&s, &input).unwrap();
        let st = staleness(&s, None, &rep);
        let (csv, n) = kit_csv(&s, &st).unwrap();
        assert_eq!(n, KIT_SIZE);
        assert_eq!(kit_csv(&s, &st).unwrap().0, csv, "deterministic");
        assert!(csv.starts_with('\u{feff}'));
        assert!(!csv.contains("SECRET-RATIONALE") && !csv.contains("4.25"));
        let rows = parse_csv(&csv);
        assert_eq!(rows.len(), KIT_SIZE + 1);
        assert_eq!(
            rows[0],
            KIT_COLUMNS
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
        );
        let item = s["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == json!(rows[1][0]))
            .unwrap();
        // multi-line, quoted text survives; a leading `-` is neutralized with one space
        assert_eq!(
            rows[1][5],
            format!(" {}", item["rendered"].as_str().unwrap())
        );
        assert_eq!(rows[1][3], "");
        assert_eq!(rows[1][7], "eval/judge/RUBRIC.md (becker-test.1)");
        assert_eq!(
            rows[1][8],
            rendered_sha256(item["rendered"].as_str().unwrap())
        );
        // stale items never enter the kit; too few fresh items is an error, not padding
        let mut g2 = g.clone();
        for k in 0..25 {
            rerender(&mut g2, &format!("K{k:04}"));
        }
        let cur = sample(&g2, RUBRIC).unwrap();
        let st2 = staleness(&cur, None, &rep);
        assert!(kit_csv(&cur, &st2).unwrap_err().contains("only 45"));
    }

    #[test]
    fn parses_spreadsheet_csv() {
        let rows = parse_csv("\u{feff}id;human_score\r\n\"a\";\"4,5\"\r\n\r\nb;\"x\"\"y\nz\"\n");
        assert_eq!(
            rows,
            vec![
                vec!["id", "human_score"],
                vec!["a", "4,5"],
                vec!["b", "x\"y\nz"]
            ]
        );
        let rows = parse_csv("id,human_score\nc,3");
        assert_eq!(rows[1], vec!["c", "3"]);
    }

    /// The kit with the first `n` rows graded `score`, as a spreadsheet would save it.
    fn graded(kit: &str, n: usize, score: &str) -> String {
        let mut rows = parse_csv(kit);
        for r in rows.iter_mut().skip(1).take(n) {
            r[3] = score.to_string();
        }
        rows.iter()
            .map(|r| {
                r.iter()
                    .map(|f| csv_field(f, false))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn calibration_reads_human_grades_and_computes_the_mae() {
        let g = many(60);
        let s = sample(&g, RUBRIC).unwrap();
        let rep = report_for(&s, 4.0);
        let st = staleness(&s, None, &rep);
        let (kit, _) = kit_csv(&s, &st).unwrap();
        assert!(read_grades(&kit, &st).unwrap().0.is_empty());
        let (few, _) = read_grades(&graded(&kit, 49, "3.5"), &st).unwrap();
        assert!(calibration_doc("founder", &few, &st, &rep)
            .unwrap_err()
            .contains("49"));
        // a decimal comma, as some locales save it
        let (grades, skipped) = read_grades(&graded(&kit, 52, "3,5"), &st).unwrap();
        assert_eq!(grades.len(), 52);
        assert!(skipped.is_empty());
        assert!(calibration_doc(" ", &grades, &st, &rep)
            .unwrap_err()
            .contains("--grader"));
        let (doc, mae) = calibration_doc("founder", &grades, &st, &rep).unwrap();
        assert!((mae - 0.5).abs() < 1e-9);
        assert_eq!(doc["n"], 52);
        assert_eq!(doc["schema"], CALIBRATION_SCHEMA);
        let (mean, ev) = measure(
            Ok(s.clone()),
            None,
            Some(&rep.to_string()),
            Some(&doc.to_string()),
        )
        .unwrap();
        assert_eq!(mean, 4.0);
        assert!(ev.contains("MAE 0.500 on 52"), "{ev}");
        // bad rows are rejected, never skipped
        assert!(read_grades(&graded(&kit, 1, "6"), &st)
            .unwrap_err()
            .contains("0..=5"));
        assert!(read_grades(&graded(&kit, 1, "abc"), &st).is_err());
        let first = parse_csv(&kit)[1].clone();
        let again: Vec<String> = first
            .iter()
            .enumerate()
            .map(|(i, f)| csv_field(if i == 3 { "4" } else { f }, false))
            .collect();
        let dup = format!("{}\n{}", graded(&kit, 1, "4"), again.join(","));
        assert!(read_grades(&dup, &st)
            .unwrap_err()
            .contains("more than once"));
        assert!(read_grades("id,human_score\nK9999/en,4\n", &st)
            .unwrap_err()
            .contains("not a current sample item"));
        assert!(read_grades("human_score\n4\n", &st)
            .unwrap_err()
            .contains("`id`"));
        // a grade of an item that is stale now, or of a rendering that changed since the kit
        // was written, is left out with its reason; the other grades still count
        let mut g2 = g.clone();
        rerender(&mut g2, first[0].split('/').next().unwrap());
        let cur = sample(&g2, RUBRIC).unwrap();
        let stale_now = staleness(&cur, None, &rep);
        let (kept, skipped) = read_grades(&graded(&kit, 3, "4"), &stale_now).unwrap();
        assert_eq!(kept.len(), 2);
        assert!(skipped[0].contains("stale or unjudged"), "{skipped:?}");
        let rep2 = ingest(&cur, &scores(&cur, 4.0)).unwrap();
        let st2 = staleness(&cur, None, &rep2);
        let (kept, skipped) = read_grades(&graded(&kit, 3, "4"), &st2).unwrap();
        assert_eq!(kept.len(), 2);
        assert!(
            skipped[0].contains("has changed since the kit"),
            "{skipped:?}"
        );
        // and an old calibration grade of that item no longer counts
        let (_, ev) = measure(
            Ok(cur),
            None,
            Some(&rep2.to_string()),
            Some(&doc.to_string()),
        )
        .unwrap();
        assert!(ev.contains("on 51 human grades"), "{ev}");
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
