//! `--append`: one line per run in `eval/history.jsonl` (schema: eval/HISTORY-SCHEMA.md).
//! The file is append-only: existing bytes are never rewritten.

use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub struct AppendArgs<'a> {
    pub timestamp: &'a str,
    pub git_sha: Option<String>,
    pub outcome: Option<&'a str>,
    pub notes: Option<&'a str>,
}

/// The next iteration number: last line's `iteration` + 1, or 0 for an empty file.
pub fn next_iteration(existing: &str) -> Result<u64, String> {
    let Some(last) = existing.lines().rev().find(|l| !l.trim().is_empty()) else {
        return Ok(0);
    };
    let v: Value = serde_json::from_str(last)
        .map_err(|e| format!("history.jsonl: last line is not JSON: {e}"))?;
    v.get("iteration")
        .and_then(Value::as_u64)
        .map(|n| n + 1)
        .ok_or_else(|| "history.jsonl: last line has no integer `iteration`".to_string())
}

/// Builds the history record from a scorecard run (`report::run` JSON).
pub fn record(run: &Value, iteration: u64, args: &AppendArgs<'_>) -> Value {
    let eval_version = run["eval_version"].clone();
    let commit = args.git_sha.clone();
    let metrics: Vec<Value> = run["metrics"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .map(|m| {
                    json!({
                        "dimension": m["dimension"], "metric": m["metric"], "value": m["value"], "ci95": Value::Null,
                        "threshold_mvp": m["threshold_mvp"], "threshold_beta": m["threshold_beta"],
                        "threshold_1_0": m["threshold_1_0"], "normalized": m["normalized"], "gate": m["gate"],
                        "eval_version": eval_version, "commit": commit, "status": m["status"],
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let gates: serde_json::Map<String, Value> = run["gates"]
        .as_object()
        .map(|g| {
            g.iter()
                .map(|(k, v)| {
                    let mut entry = json!({"state": v["state"]});
                    if let Some(c) = v.get("checks") {
                        entry["checks"] = c.clone();
                    }
                    (k.clone(), entry)
                })
                .collect()
        })
        .unwrap_or_default();
    let c = &run["checks"];
    json!({
        "schema": "kalam-history/1",
        "iteration": iteration,
        "ts": args.timestamp,
        "git_sha": commit,
        "commit": commit,
        "git_dirty": run.get("git_dirty").cloned().unwrap_or(Value::Null),
        "eval_version": eval_version,
        "target_milestone": run["target_milestone"],
        "outcome": args.outcome,
        "dimensions": run["dimensions"],
        "composite": run["composite"],
        "gates": gates,
        "metrics": metrics,
        "checks": {
            "corpus_lex": {"passed": c["corpus_lex"]["passed"], "total": c["corpus_lex"]["case_skin_pairs"]},
            "security_lex": {"passed": c["security_lex"]["passed"], "total": c["security_lex"]["case_skin_pairs"]},
            "lex_roundtrip": {"failed": c["lex_roundtrip"]["failed"], "total": c["lex_roundtrip"]["checks"]},
            "keyword_table_load": c["keyword_table_load"]["pass"],
            "cargo_test": {
                "status": c["cargo_test"]["status"], "passed": c["cargo_test"]["passed"],
                "failed": c["cargo_test"]["failed"],
            },
        },
        "summary": run["summary"],
        "notes": args.notes,
    })
}

/// Appends exactly one line. Adds a separating newline first if the file lacks one.
pub fn append(path: &Path, run: &Value, args: &AppendArgs<'_>) -> Result<Value, String> {
    let existing = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let iteration = next_iteration(&existing)?;
    let rec = record(run, iteration, args);
    let mut line = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        line.push('\n');
    }
    line.push_str(&serde_json::to_string(&rec).map_err(|e| e.to_string())?);
    line.push('\n');
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(line.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_run() -> Value {
        json!({
            "eval_version": "eval-v0.1-draft", "target_milestone": "mvp", "git_dirty": true,
            "dimensions": {"D1": 0.0, "D4": 50.0}, "composite": 6.0,
            "gates": {"G-SEC": {"state": "pending", "checks": {"trojan_source_suite": "pending"}}, "G-INT": {"state": "pending", "reason": "x"}},
            "metrics": [{"dimension": "D4", "metric": "D4.synonym_count", "status": "measured", "value": true,
                         "normalized": 100.0, "threshold_mvp": true, "threshold_beta": true, "threshold_1_0": true, "gate": null}],
            "checks": {"corpus_lex": {"passed": 1, "case_skin_pairs": 2}, "security_lex": {"passed": 0, "case_skin_pairs": 0},
                       "lex_roundtrip": {"failed": 0, "checks": 3}, "keyword_table_load": {"pass": true},
                       "cargo_test": {"status": "skipped"}},
            "summary": {"measured": 1},
        })
    }

    #[test]
    fn iteration_numbering() {
        assert_eq!(next_iteration("").unwrap(), 0);
        assert_eq!(next_iteration("\n").unwrap(), 0);
        assert_eq!(
            next_iteration("{\"iteration\":0}\n{\"iteration\":4}\n").unwrap(),
            5
        );
        assert!(next_iteration("not json\n").is_err());
        assert!(next_iteration("{\"x\":1}\n").is_err());
    }

    #[test]
    fn append_is_append_only_and_one_line_per_call() {
        let dir = std::env::temp_dir().join(format!("kalam-eval-history-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.jsonl");
        let args = AppendArgs {
            timestamp: "2026-10-25T00:00:00Z",
            git_sha: None,
            outcome: None,
            notes: Some("first"),
        };

        let r0 = append(&path, &fake_run(), &args).unwrap();
        assert_eq!(r0["iteration"], 0);
        let after_first = fs::read_to_string(&path).unwrap();
        assert_eq!(after_first.lines().count(), 1);

        let args2 = AppendArgs {
            git_sha: Some("abc123".into()),
            notes: Some("second"),
            ..args
        };
        let r1 = append(&path, &fake_run(), &args2).unwrap();
        let after_second = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_dir_all(&dir);

        assert!(
            after_second.starts_with(&after_first),
            "previous bytes must be unchanged"
        );
        assert_eq!(after_second.lines().count(), 2);
        assert_eq!(r1["iteration"], 1);
        assert_eq!(r1["git_sha"], "abc123");
        assert_eq!(r1["git_dirty"], true);
        assert_eq!(r1["ts"], "2026-10-25T00:00:00Z");
        assert_eq!(r1["notes"], "second");
        assert_eq!(r1["gates"]["G-SEC"]["state"], "pending");
        assert_eq!(r1["metrics"][0]["status"], "measured");
        assert_eq!(r1["dimensions"]["D4"], 50.0);
        let parsed: Value = serde_json::from_str(after_second.lines().nth(1).unwrap()).unwrap();
        assert_eq!(parsed["schema"], "kalam-history/1");
    }

    #[test]
    fn append_repairs_a_missing_trailing_newline_without_rewriting() {
        let dir =
            std::env::temp_dir().join(format!("kalam-eval-history-nl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("h.jsonl");
        fs::write(&path, "{\"iteration\":7}").unwrap();
        let args = AppendArgs {
            timestamp: "t",
            git_sha: None,
            outcome: Some("baseline"),
            notes: None,
        };
        let r = append(&path, &fake_run(), &args).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(r["iteration"], 8);
        assert!(text.starts_with("{\"iteration\":7}\n"));
        assert_eq!(text.lines().count(), 2);
    }
}
