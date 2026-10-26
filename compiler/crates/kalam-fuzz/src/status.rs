//! `eval/fuzz/status.json`: the fuzz clock that feeds `D1.fuzz_crash_free_hours`.
//!
//! Rules:
//! - `version` is the SHA-256 of the fuzz target binary. Crash-free time accumulates across
//!   runs only while it is unchanged; a new binary starts the clock at zero (and re-checks
//!   every saved crash input first).
//! - The clock runs only while **no crash is open** on this binary: a crash found (or a saved
//!   crash that still reproduces) sets `crash_free_hours` to 0, and it stays 0 until a binary
//!   without that crash is fuzzed. Re-finding a known crash on the same binary is not a "new"
//!   crash, but the binary is still not crash-free, so the clock does not run.
//! - Oracle violations (wrong output, not a crash) are counted separately and do not stop
//!   the clock.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

#[derive(Clone, Debug, Default)]
pub struct CrashRecord {
    pub signature: String,
    pub kind: String,
    pub message: String,
    /// Crash input, relative to the fuzz output directory.
    pub file: String,
    pub skin: String,
    pub first_seen: String,
    pub count: u64,
    pub reproducible: Option<bool>,
    /// Bytes before and after minimization.
    pub original_len: usize,
    pub minimized_len: usize,
}

#[derive(Clone, Debug, Default)]
pub struct OracleRecord {
    pub count: u64,
    pub examples: Vec<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub version: String,
    pub previous_version: Option<String>,
    pub crash_free_secs: f64,
    /// Fuzzing time on this version (crash-free or not).
    pub version_fuzz_secs: f64,
    pub version_execs: u64,
    /// Lifetime executions, across versions.
    pub total_execs: u64,
    pub runs: u64,
    pub crashes: Vec<CrashRecord>,
    pub oracle: BTreeMap<String, OracleRecord>,
    pub last_run: String,
    pub last_run_secs: f64,
    pub last_run_execs: u64,
    pub jobs: usize,
    pub corpus_seeds: usize,
    pub corpus_size: usize,
    pub features: usize,
    /// Saved crash inputs re-run at the start of the last run, and how many still crash.
    pub replay_checked: usize,
    pub replay_open: usize,
}

fn f64_of(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

fn u64_of(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl Status {
    pub fn load(path: &Path) -> Option<Status> {
        let text = std::fs::read_to_string(path).ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        let crashes = v
            .get("crash_list")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|c| CrashRecord {
                        signature: str_of(c, "signature"),
                        kind: str_of(c, "kind"),
                        message: str_of(c, "message"),
                        file: str_of(c, "file"),
                        skin: str_of(c, "skin"),
                        first_seen: str_of(c, "first_seen"),
                        count: u64_of(c, "count"),
                        reproducible: c.get("reproducible").and_then(Value::as_bool),
                        original_len: u64_of(c, "original_len") as usize,
                        minimized_len: u64_of(c, "minimized_len") as usize,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let oracle = v
            .get("oracle_violations")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .map(|(k, r)| {
                        (
                            k.clone(),
                            OracleRecord {
                                count: u64_of(r, "count"),
                                detail: str_of(r, "detail"),
                                examples: r
                                    .get("examples")
                                    .and_then(Value::as_array)
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|x| x.as_str().map(String::from))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Status {
            version: str_of(&v, "version"),
            previous_version: v
                .get("previous_version")
                .and_then(Value::as_str)
                .map(String::from),
            crash_free_secs: v
                .get("crash_free_secs")
                .and_then(Value::as_f64)
                .unwrap_or_else(|| f64_of(&v, "crash_free_hours") * 3600.0),
            version_fuzz_secs: v
                .get("version_fuzz_secs")
                .and_then(Value::as_f64)
                .unwrap_or_else(|| f64_of(&v, "version_fuzz_hours") * 3600.0),
            version_execs: u64_of(&v, "version_execs"),
            total_execs: u64_of(&v, "total_execs"),
            runs: u64_of(&v, "runs"),
            crashes,
            oracle,
            last_run: str_of(&v, "last_run"),
            last_run_secs: f64_of(&v, "last_run_secs"),
            last_run_execs: u64_of(&v, "last_run_execs"),
            jobs: u64_of(&v, "jobs") as usize,
            corpus_seeds: u64_of(&v, "corpus_seeds") as usize,
            corpus_size: u64_of(&v, "corpus_size") as usize,
            features: u64_of(&v, "features") as usize,
            replay_checked: u64_of(&v, "replay_checked") as usize,
            replay_open: u64_of(&v, "replay_open") as usize,
        })
    }

    /// Starts a run on `version`: keeps the clock if the binary is unchanged, resets it
    /// (and the version-scoped counters and crash list) otherwise.
    pub fn begin(previous: Option<Status>, version: &str) -> Status {
        match previous {
            Some(s) if s.version == version => s,
            Some(s) => Status {
                version: version.to_string(),
                previous_version: Some(s.version),
                total_execs: s.total_execs,
                runs: s.runs,
                ..Status::default()
            },
            None => Status {
                version: version.to_string(),
                ..Status::default()
            },
        }
    }

    pub fn to_json(&self) -> Value {
        let execs_per_sec = if self.last_run_secs > 0.0 {
            self.last_run_execs as f64 / self.last_run_secs
        } else {
            0.0
        };
        let round = |x: f64, d: i32| {
            let m = 10f64.powi(d);
            (x * m).round() / m
        };
        json!({
            "crash_free_hours": round(self.crash_free_secs / 3600.0, 4),
            "total_execs": self.total_execs,
            "crashes": self.crashes.len(),
            "last_run": self.last_run,
            "version": self.version,
            "previous_version": self.previous_version,
            "metric": "D1.fuzz_crash_free_hours",
            "targets": crate::target::TARGETS,
            "crash_free_secs": round(self.crash_free_secs, 3),
            "version_fuzz_hours": round(self.version_fuzz_secs / 3600.0, 4),
            "version_fuzz_secs": round(self.version_fuzz_secs, 3),
            "version_execs": self.version_execs,
            "runs": self.runs,
            "last_run_secs": round(self.last_run_secs, 1),
            "last_run_execs": self.last_run_execs,
            "execs_per_sec": round(execs_per_sec, 1),
            "jobs": self.jobs,
            "corpus_seeds": self.corpus_seeds,
            "corpus_size": self.corpus_size,
            "features": self.features,
            "replay_checked": self.replay_checked,
            "replay_open": self.replay_open,
            "crash_list": self.crashes.iter().map(|c| json!({
                "signature": c.signature,
                "kind": c.kind,
                "message": c.message,
                "file": c.file,
                "skin": c.skin,
                "first_seen": c.first_seen,
                "count": c.count,
                "reproducible": c.reproducible,
                "original_len": c.original_len,
                "minimized_len": c.minimized_len,
            })).collect::<Vec<_>>(),
            "oracle_violations": self.oracle.iter().map(|(k, r)| (k.clone(), json!({
                "count": r.count,
                "detail": r.detail,
                "examples": r.examples,
            }))).collect::<serde_json::Map<String, Value>>(),
            "clock_rule": "crash_free_hours accumulates across runs only while `version` (SHA-256 of the fuzz target binary) is unchanged and no crash is open on it; it resets to 0 on a new binary or a crash",
        })
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&self.to_json()).unwrap_or_default() + "\n";
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_version_keeps_the_clock_new_version_resets_it() {
        let s = Status {
            version: "aaa".into(),
            crash_free_secs: 7200.0,
            total_execs: 100,
            version_execs: 100,
            ..Status::default()
        };
        let kept = Status::begin(Some(s.clone()), "aaa");
        assert_eq!(kept.crash_free_secs, 7200.0);
        let reset = Status::begin(Some(s), "bbb");
        assert_eq!(reset.crash_free_secs, 0.0);
        assert_eq!(reset.version_execs, 0);
        assert_eq!(reset.total_execs, 100);
        assert_eq!(reset.previous_version.as_deref(), Some("aaa"));
    }

    #[test]
    fn json_round_trip() {
        let s = Status {
            version: "v".into(),
            crash_free_secs: 3600.0,
            total_execs: 5,
            crashes: vec![CrashRecord {
                signature: "panic@a.rs:1".into(),
                count: 2,
                reproducible: Some(true),
                ..CrashRecord::default()
            }],
            ..Status::default()
        };
        let dir = std::env::temp_dir().join(format!("kalam-fuzz-status-{}", std::process::id()));
        let p = dir.join("status.json");
        s.save(&p).unwrap();
        let back = Status::load(&p).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["targets"], json!(crate::target::TARGETS));
        assert!(v["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t.as_str().unwrap().contains("typeck")));
        assert_eq!(back.crash_free_secs, 3600.0);
        assert_eq!(back.crashes.len(), 1);
        assert_eq!(back.crashes[0].reproducible, Some(true));
        let _ = std::fs::remove_dir_all(dir);
    }
}
