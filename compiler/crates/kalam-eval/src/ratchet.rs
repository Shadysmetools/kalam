//! G-RATCHET (LOOP.md §3 step 8, A8; SCORECARD.md §10): a tier check that was met at the
//! baseline stays met.
//!
//! **Baseline** = the last `eval/history.jsonl` line with `outcome: "accepted"`, for the same
//! `eval_version` and `target_milestone`. If there is none, it is the first line of that
//! eval version and tier that carries `tier_checks` (the first measured run). Lines with any
//! other outcome (`rejected`, `integrity_fail`, the pre-schema `kept`, …) are never a
//! baseline unless they are that first measured run.
//!
//! **Rule:** every baseline tier check with status `pass` (G-RATCHET itself excluded) must
//! still be `pass` in this run, matched by `(id, tier)`. A check that is now `fail` or
//! `not_measurable` breaks the ratchet: an unmeasured check scores 0 (LOOP.md A2). Without a
//! baseline the ratchet is `pending`, never a pass.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Ratchet {
    /// No baseline line for this eval version and tier.
    NoBaseline(String),
    Held {
        baseline_iteration: Option<u64>,
        how: &'static str,
        kept: usize,
    },
    Broken {
        baseline_iteration: Option<u64>,
        how: &'static str,
        regressions: Vec<String>,
    },
}

impl Ratchet {
    pub fn state(&self) -> &'static str {
        match self {
            Ratchet::NoBaseline(_) => "pending",
            Ratchet::Held { .. } => "pass",
            Ratchet::Broken { .. } => "fail",
        }
    }

    pub fn reason(&self) -> String {
        match self {
            Ratchet::NoBaseline(why) => why.clone(),
            Ratchet::Held {
                baseline_iteration,
                how,
                kept,
            } => format!(
                "baseline = iteration {baseline_iteration:?} ({how}); all {kept} checks met there are still met"
            ),
            Ratchet::Broken {
                baseline_iteration,
                how,
                regressions,
            } => format!(
                "baseline = iteration {baseline_iteration:?} ({how}); met there, not met now: {}",
                regressions.join("; ")
            ),
        }
    }
}

/// Picks the baseline line (see the module docs). Malformed lines are skipped.
pub fn baseline(history: &str, eval_version: &str, tier: &str) -> Option<(Value, &'static str)> {
    let lines: Vec<Value> = history
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["eval_version"] == eval_version && v["target_milestone"] == tier)
        .collect();
    if let Some(v) = lines.iter().rev().find(|v| v["outcome"] == "accepted") {
        return Some((v.clone(), "last accepted line"));
    }
    lines
        .into_iter()
        .find(|v| v["tier_checks"].as_array().is_some_and(|a| !a.is_empty()))
        .map(|v| (v, "no accepted line yet: first measured run"))
}

/// Compares this run's tier checks (`[{id, tier, status}, ...]`) with the baseline's.
pub fn evaluate(history: &str, eval_version: &str, tier: &str, current: &[Value]) -> Ratchet {
    let Some((base, how)) = baseline(history, eval_version, tier) else {
        return Ratchet::NoBaseline(format!(
            "no eval/history.jsonl line for {eval_version} / {tier} with tier checks: nothing to ratchet against"
        ));
    };
    let iteration = base["iteration"].as_u64();
    let mut kept = 0;
    let mut regressions = Vec::new();
    for b in base["tier_checks"].as_array().into_iter().flatten() {
        if b["status"] != "pass" || b["id"] == "G-RATCHET" {
            continue;
        }
        let now = current
            .iter()
            .find(|c| c["id"] == b["id"] && c["tier"] == b["tier"])
            .map(|c| c["status"].as_str().unwrap_or("missing").to_string())
            .unwrap_or_else(|| "missing".into());
        if now == "pass" {
            kept += 1;
        } else {
            regressions.push(format!(
                "{} ({}): {now}",
                b["id"].as_str().unwrap_or("?"),
                b["tier"].as_str().unwrap_or("?")
            ));
        }
    }
    if regressions.is_empty() {
        Ratchet::Held {
            baseline_iteration: iteration,
            how,
            kept,
        }
    } else {
        Ratchet::Broken {
            baseline_iteration: iteration,
            how,
            regressions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(it: u64, outcome: &str, checks: Value) -> String {
        json!({"iteration": it, "eval_version": "v", "target_milestone": "M1", "outcome": outcome,
               "tier_checks": checks})
        .to_string()
    }

    fn c(id: &str, status: &str) -> Value {
        json!({"id": id, "tier": "M1", "status": status})
    }

    #[test]
    fn baseline_is_last_accepted_else_first_measured() {
        let h = [
            json!({"iteration": 0, "eval_version": "v", "target_milestone": "M1", "outcome": "kept"}).to_string(),
            line(1, "kept", json!([c("A", "pass")])),
            line(2, "accepted", json!([c("B", "pass")])),
            line(3, "accepted", json!([c("C", "pass")])),
            line(4, "rejected", json!([c("D", "pass")])),
            line(5, "accepted", json!([c("E", "pass")])).replace("\"v\"", "\"other\""),
        ]
        .join("\n");
        let (b, how) = baseline(&h, "v", "M1").unwrap();
        assert_eq!(b["iteration"], 3);
        assert_eq!(how, "last accepted line");
        let no_accepted = [
            json!({"iteration": 0, "eval_version": "v", "target_milestone": "M1"}).to_string(),
            line(1, "kept", json!([c("A", "pass")])),
        ]
        .join("\n");
        assert_eq!(baseline(&no_accepted, "v", "M1").unwrap().0["iteration"], 1);
        assert!(baseline("", "v", "M1").is_none());
        assert!(baseline(&no_accepted, "v", "M2").is_none());
    }

    #[test]
    fn met_checks_must_stay_met() {
        let h = line(
            0,
            "accepted",
            json!([c("A", "pass"), c("B", "fail"), c("G-RATCHET", "pass")]),
        );
        let held = evaluate(&h, "v", "M1", &[c("A", "pass"), c("B", "not_measurable")]);
        assert_eq!(held.state(), "pass", "{held:?}");
        let broken = evaluate(&h, "v", "M1", &[c("A", "not_measurable")]);
        assert_eq!(broken.state(), "fail");
        assert!(broken.reason().contains("A (M1): not_measurable"));
        let gone = evaluate(&h, "v", "M1", &[]);
        assert_eq!(gone.state(), "fail");
        assert_eq!(evaluate("", "v", "M1", &[]).state(), "pending");
    }
}
