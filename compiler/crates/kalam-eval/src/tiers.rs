//! Milestone tiers (`scorecard.json` → `milestone_tiers`, SCORECARD.md §10, LOOP.md §5.1, §8).
//!
//! A tier is either a **checklist** tier (M1–M4, M6–M10: it is met when every listed check
//! holds, plus every check of the tiers it ratchets from) or a **scorecard** tier (M5 = `mvp`,
//! `beta`, `1.0`: the SCORECARD.md §6 rule). This module is pure: it turns observations
//! (what the harness measured for each check id) into per-check statuses, the tier score and
//! `milestone_met`. It never turns an unmeasured or partially measured check into a pass.

use crate::scorecard::{Direction, MetricValue, Milestone};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Pass,
    Eq,
    Ge,
    Le,
}

impl Op {
    fn parse(s: &str) -> Option<Op> {
        match s {
            "pass" => Some(Op::Pass),
            "==" => Some(Op::Eq),
            ">=" => Some(Op::Ge),
            "<=" => Some(Op::Le),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Op::Pass => "pass",
            Op::Eq => "==",
            Op::Ge => ">=",
            Op::Le => "<=",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TierCheck {
    pub id: String,
    pub op: Op,
    pub value: Option<f64>,
    pub unit: String,
    pub scope: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TierMode {
    Checklist,
    Scorecard,
}

impl TierMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TierMode::Checklist => "checklist",
            TierMode::Scorecard => "scorecard",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tier {
    pub id: String,
    pub name: String,
    pub mode: TierMode,
    pub scorecard_tier: Option<Milestone>,
    pub ratchet_from: Option<String>,
    pub stages_in_scope: Vec<String>,
    pub checks: Vec<TierCheck>,
    pub ratchet_only: Vec<String>,
    pub pass_rule: String,
}

#[derive(Clone, Debug, Default)]
pub struct Tiers {
    pub order: Vec<String>,
    pub tiers: Vec<Tier>,
}

impl Tiers {
    /// Parses `milestone_tiers`. A missing section gives an empty set (every `--target` tier
    /// id is then unknown).
    pub fn parse(section: Option<&Value>) -> Result<Tiers, String> {
        let Some(section) = section else {
            return Ok(Tiers::default());
        };
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let strs = |v: &Value, k: &str| -> Vec<String> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let order = strs(section, "order");
        let mut tiers = Vec::new();
        for t in section
            .get("tiers")
            .and_then(Value::as_array)
            .ok_or("scorecard.json: milestone_tiers has no `tiers` array")?
        {
            let id = s(t, "id");
            let mode = match t.get("mode").and_then(Value::as_str) {
                Some("checklist") => TierMode::Checklist,
                Some("scorecard") => TierMode::Scorecard,
                other => return Err(format!("milestone tier {id}: unknown mode {other:?}")),
            };
            let mut checks = Vec::new();
            for c in t
                .get("checks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let op = c
                    .get("op")
                    .and_then(Value::as_str)
                    .and_then(Op::parse)
                    .ok_or_else(|| {
                        format!(
                            "milestone tier {id}: check {} has an unknown op",
                            s(c, "id")
                        )
                    })?;
                checks.push(TierCheck {
                    id: s(c, "id"),
                    op,
                    value: c.get("value").and_then(Value::as_f64),
                    unit: s(c, "unit"),
                    scope: s(c, "scope"),
                });
            }
            tiers.push(Tier {
                mode,
                name: s(t, "name"),
                scorecard_tier: t
                    .get("scorecard_tier")
                    .and_then(Value::as_str)
                    .and_then(Milestone::parse),
                ratchet_from: t
                    .get("ratchet_from")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                stages_in_scope: strs(t, "stages_in_scope"),
                checks,
                ratchet_only: strs(t, "ratchet_only"),
                pass_rule: s(t, "pass_rule"),
                id,
            });
        }
        Ok(Tiers { order, tiers })
    }

    pub fn get(&self, id: &str) -> Option<&Tier> {
        self.tiers.iter().find(|t| t.id == id)
    }

    /// Resolves a `--target` value: a tier id, or the legacy scorecard names `mvp` (= the
    /// tier whose `scorecard_tier` is `mvp`, M5), `beta`, `1.0`.
    pub fn resolve(&self, target: &str) -> Option<&Tier> {
        self.get(target).or_else(|| {
            let m = Milestone::parse(target)?;
            self.tiers.iter().find(|t| t.scorecard_tier == Some(m))
        })
    }

    /// The tier and every tier it ratchets from, transitively (target first). A cycle or a
    /// missing tier ends the walk.
    pub fn chain(&self, id: &str) -> Vec<&Tier> {
        let mut out: Vec<&Tier> = Vec::new();
        let mut cur = self.get(id);
        while let Some(t) = cur {
            if out.iter().any(|o| o.id == t.id) {
                break;
            }
            out.push(t);
            cur = t.ratchet_from.as_deref().and_then(|r| self.get(r));
        }
        out
    }

    /// The scorecard milestone metrics are normalized against for a tier: its own
    /// `scorecard_tier`, else the next scorecard tier in `order` (M1–M4 → `mvp`,
    /// M6–M10 → `beta`), else `mvp`.
    pub fn normalization(&self, id: &str) -> Milestone {
        if let Some(m) = self.get(id).and_then(|t| t.scorecard_tier) {
            return m;
        }
        let start = self.order.iter().position(|o| o == id).unwrap_or(0);
        self.order[start..]
            .iter()
            .filter_map(|o| self.get(o))
            .find_map(|t| t.scorecard_tier)
            .unwrap_or(Milestone::Mvp)
    }
}

/// What the harness knows about one check id.
#[derive(Clone, Debug, PartialEq)]
pub enum Observation {
    /// A value. `dev_only`: computed on the dev corpus only, while the check is judged on
    /// the frozen set held-out included. `definitive_fail`: some measured part already makes
    /// the check impossible to meet (e.g. a dev case fails a 100% check).
    Measured {
        value: MetricValue,
        evidence: String,
        dev_only: bool,
        definitive_fail: bool,
    },
    /// The check (usually a gate) definitively passed on its whole scope.
    Passed(String),
    /// The check definitively failed.
    Failed(String),
    NotMeasurable(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    NotMeasurable,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::NotMeasurable => "not_measurable",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CheckResult {
    pub id: String,
    /// The tier that lists the check (the target or a ratchet ancestor).
    pub tier: String,
    /// `check`, `ratchet_only` or `scorecard_rule`.
    pub kind: &'static str,
    pub op: Op,
    pub threshold: Option<f64>,
    pub unit: String,
    pub scope: String,
    pub status: Status,
    pub score: f64,
    pub evidence: String,
}

impl CheckResult {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id, "tier": self.tier, "kind": self.kind, "op": self.op.as_str(),
            "threshold": self.threshold, "unit": self.unit, "scope": self.scope,
            "status": self.status.as_str(), "score": crate::report::round2(self.score),
            "evidence": self.evidence,
        })
    }
}

/// Context for checks with `op: pass` on a numeric metric (and `ratchet_only` metrics): the
/// metric's direction and its threshold at the normalization milestone.
pub type MetricThreshold = Option<(Direction, f64)>;

/// Judges one check against its observation (tier_score rule of `milestone_tiers`).
pub fn evaluate_check(
    check: &TierCheck,
    obs: &Observation,
    metric_threshold: MetricThreshold,
) -> (Status, f64, String) {
    let (value, evidence, dev_only, definitive_fail) = match obs {
        Observation::Passed(e) => return (Status::Pass, 100.0, e.clone()),
        Observation::Failed(e) => return (Status::Fail, 0.0, e.clone()),
        Observation::NotMeasurable(e) => return (Status::NotMeasurable, 0.0, e.clone()),
        Observation::Measured {
            value,
            evidence,
            dev_only,
            definitive_fail,
        } => (value, evidence, *dev_only, *definitive_fail),
    };
    match judge(check, value, metric_threshold) {
        None => (
            Status::NotMeasurable,
            0.0,
            format!(
                "{evidence}; the measured value {} cannot be judged against `{}`{}",
                value.to_json(),
                check.op.as_str(),
                check.value.map(|v| format!(" {v}")).unwrap_or_default()
            ),
        ),
        Some((true, _)) if dev_only => (
            Status::NotMeasurable,
            0.0,
            format!(
                "{evidence}; the dev set meets the check, but it is judged on the frozen set \
                 held-out included and the held-out runner is not available"
            ),
        ),
        Some((true, score)) => (Status::Pass, score, evidence.clone()),
        Some((false, _)) if dev_only && !definitive_fail => (
            Status::NotMeasurable,
            0.0,
            format!(
                "{evidence}; the dev set alone does not meet the check and held-out cases \
                 are not available, so it is undecided"
            ),
        ),
        Some((false, score)) => (Status::Fail, score, evidence.clone()),
    }
}

/// `(holds, check_score)`, or `None` when the value cannot be judged by this op.
fn judge(
    check: &TierCheck,
    value: &MetricValue,
    metric_threshold: MetricThreshold,
) -> Option<(bool, f64)> {
    let binary = |b: bool| (b, if b { 100.0 } else { 0.0 });
    match (check.op, value) {
        (Op::Pass, MetricValue::Bool(b)) => Some(binary(*b)),
        (Op::Pass, MetricValue::Num(v)) => {
            let (dir, t) = metric_threshold?;
            match dir {
                Direction::Lower => Some(binary(*v <= t)),
                Direction::Higher | Direction::Ladder => Some(binary(*v >= t)),
                Direction::Binary => None,
            }
        }
        (_, MetricValue::Bool(_)) => None,
        (op, MetricValue::Num(v)) => {
            let (v, t) = (*v, check.value?);
            Some(match op {
                Op::Ge => {
                    let holds = v >= t;
                    let score = if t <= 0.0 {
                        binary(holds).1
                    } else {
                        (100.0 * v / t).clamp(0.0, 100.0)
                    };
                    (holds, score)
                }
                Op::Le => {
                    let holds = v <= t;
                    let score = if v <= 0.0 {
                        100.0
                    } else {
                        (100.0 * t / v).clamp(0.0, 100.0)
                    };
                    (holds, score)
                }
                Op::Eq => {
                    let holds = (v - t).abs() < 1e-9;
                    if check.unit.trim() == "%" && t == 100.0 {
                        (holds, v.clamp(0.0, 100.0))
                    } else {
                        binary(holds)
                    }
                }
                Op::Pass => unreachable!("handled above"),
            })
        }
    }
}

/// Inputs for the SCORECARD.md §6 rule of a scorecard tier.
pub struct ScorecardRule<'a> {
    pub milestone: Milestone,
    pub dimensions: &'a [(String, f64)],
    pub composite: f64,
    pub dimension_min: Option<f64>,
    pub composite_min: Option<f64>,
    pub human_gates: &'a [String],
}

pub struct TierEval {
    pub tier: String,
    pub mode: TierMode,
    pub normalized_against: Milestone,
    pub checks: Vec<CheckResult>,
    pub tier_score: Option<f64>,
    pub milestone_met: bool,
}

impl TierEval {
    pub fn to_json(&self) -> Value {
        json!({
            "tier_mode": self.mode.as_str(),
            "tier_score": self.tier_score.map(crate::report::round2),
            "tier_checks": self.checks.iter().map(CheckResult::to_json).collect::<Vec<_>>(),
            "milestone_met": self.milestone_met,
        })
    }
}

/// Evaluates `tier_id` and its ratchet chain. `observe` maps a check (by id and scope) to what the harness
/// measured; `threshold_of` gives a metric's direction and threshold at the normalization
/// milestone. `rule` is required for scorecard tiers.
pub fn evaluate(
    tiers: &Tiers,
    tier_id: &str,
    observe: &dyn Fn(&TierCheck) -> Observation,
    threshold_of: &dyn Fn(&str) -> MetricThreshold,
    rule: Option<&ScorecardRule<'_>>,
) -> Result<TierEval, String> {
    let target = tiers.get(tier_id).ok_or_else(|| {
        format!(
            "unknown milestone tier `{tier_id}` (scorecard.json milestone_tiers.order: {:?})",
            tiers.order
        )
    })?;
    let mut checks = Vec::new();
    let mut push = |tier: &str, kind: &'static str, c: &TierCheck, obs: &Observation| {
        let (status, score, evidence) = evaluate_check(c, obs, threshold_of(&c.id));
        checks.push(CheckResult {
            id: c.id.clone(),
            tier: tier.to_string(),
            kind,
            op: c.op,
            threshold: c.value,
            unit: c.unit.clone(),
            scope: c.scope.clone(),
            status,
            score,
            evidence,
        });
    };
    let rule_check = |id: String, op: Op, value: Option<f64>, scope: String| TierCheck {
        id,
        op,
        value,
        unit: String::new(),
        scope,
    };

    if target.mode == TierMode::Scorecard {
        let rule = rule.ok_or_else(|| {
            format!("tier {tier_id} is a scorecard tier: the SCORECARD.md §6 inputs are required")
        })?;
        let key = rule.milestone.key();
        for gate in ["G-INT", "G-PAR", "G-SEC", "G-FUZZ"] {
            let scope = format!("SCORECARD.md §6 rule 1 at the {key} level, held-out included");
            let c = rule_check(gate.into(), Op::Pass, None, scope);
            push(&target.id, "scorecard_rule", &c, &observe(&c));
        }
        let minimum = |min: Option<f64>, value: f64, what: String| match min {
            Some(_) => Observation::Measured {
                value: MetricValue::Num(value),
                evidence: format!("{what} = {value}"),
                dev_only: false,
                definitive_fail: false,
            },
            None => Observation::NotMeasurable(format!(
                "scorecard.json has no {key} minimum for {what}"
            )),
        };
        for (dim, score) in rule.dimensions {
            let scope = format!("SCORECARD.md §6 rule 2 ({key})");
            let c = rule_check(
                format!("dimension_min.{dim}"),
                Op::Ge,
                rule.dimension_min,
                scope,
            );
            push(
                &target.id,
                "scorecard_rule",
                &c,
                &minimum(rule.dimension_min, *score, dim.clone()),
            );
        }
        let scope = format!("SCORECARD.md §6 rule 3 ({key})");
        let c = rule_check("composite_min".into(), Op::Ge, rule.composite_min, scope);
        let obs = minimum(rule.composite_min, rule.composite, "composite".into());
        push(&target.id, "scorecard_rule", &c, &obs);
        for h in rule.human_gates {
            let scope = format!("SCORECARD.md §6 rule 4 ({key})");
            let c = rule_check(format!("human_gate.{h}"), Op::Pass, None, scope);
            let obs = Observation::NotMeasurable(format!(
                "human gate {h}: completed and recorded by people; the harness has no record of it"
            ));
            push(&target.id, "scorecard_rule", &c, &obs);
        }
    }

    for t in tiers.chain(tier_id) {
        for c in &t.checks {
            push(&t.id, "check", c, &observe(c));
        }
        for id in &t.ratchet_only {
            let scope =
                "ratchet_only: holds at the metric's threshold for the normalization milestone";
            let c = rule_check(id.clone(), Op::Pass, None, scope.into());
            push(&t.id, "ratchet_only", &c, &observe(&c));
        }
    }

    let tier_score = (target.mode == TierMode::Checklist).then(|| {
        if checks.is_empty() {
            0.0
        } else {
            checks.iter().map(|c| c.score).sum::<f64>() / checks.len() as f64
        }
    });
    let milestone_met = !checks.is_empty() && checks.iter().all(|c| c.status == Status::Pass);
    Ok(TierEval {
        tier: target.id.clone(),
        mode: target.mode,
        normalized_against: tiers.normalization(tier_id),
        checks,
        tier_score,
        milestone_met,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(id: &str, op: Op, value: Option<f64>, unit: &str, scope: &str) -> TierCheck {
        TierCheck {
            id: id.into(),
            op,
            value,
            unit: unit.into(),
            scope: scope.into(),
        }
    }

    fn measured(v: MetricValue) -> Observation {
        Observation::Measured {
            value: v,
            evidence: "e".into(),
            dev_only: false,
            definitive_fail: false,
        }
    }

    use MetricValue::{Bool, Num};

    #[test]
    fn op_pass_on_a_bool() {
        let c = check("D4.synonym_count", Op::Pass, None, "", "");
        assert_eq!(
            evaluate_check(&c, &measured(Bool(true)), None).0,
            Status::Pass
        );
        let (st, score, _) = evaluate_check(&c, &measured(Bool(false)), None);
        assert_eq!((st, score), (Status::Fail, 0.0));
    }

    #[test]
    fn op_pass_on_a_number_uses_the_metric_threshold() {
        let c = check("D4.keywords_per_skin", Op::Pass, None, "", "");
        let th = Some((Direction::Lower, 40.0));
        assert_eq!(evaluate_check(&c, &measured(Num(36.0)), th).0, Status::Pass);
        assert_eq!(evaluate_check(&c, &measured(Num(41.0)), th).0, Status::Fail);
        // no threshold to judge by: never a pass
        assert_eq!(
            evaluate_check(&c, &measured(Num(1.0)), None).0,
            Status::NotMeasurable
        );
    }

    #[test]
    fn numeric_ops_and_scores() {
        let ge = check("D5.error_recovery_precision", Op::Ge, Some(0.5), "", "");
        let (st, score, _) = evaluate_check(&ge, &measured(Num(0.25)), None);
        assert_eq!(st, Status::Fail);
        assert!((score - 50.0).abs() < 1e-9);
        let (st, score, _) = evaluate_check(&ge, &measured(Num(0.9)), None);
        assert_eq!((st, score), (Status::Pass, 100.0));

        let le = check("D6.hello_cold_start_ms", Op::Le, Some(200.0), "ms", "");
        let (st, score, _) = evaluate_check(&le, &measured(Num(400.0)), None);
        assert_eq!(st, Status::Fail);
        assert!((score - 50.0).abs() < 1e-9);
        let (_, score, _) = evaluate_check(&le, &measured(Num(0.0)), None);
        assert_eq!(score, 100.0);

        // == 100 %: the score is the value
        let eq100 = check("G-PAR.P1", Op::Eq, Some(100.0), "%", "");
        let (st, score, _) = evaluate_check(&eq100, &measured(Num(80.0)), None);
        assert_eq!((st, score), (Status::Fail, 80.0));
        assert_eq!(
            evaluate_check(&eq100, &measured(Num(100.0)), None).0,
            Status::Pass
        );

        // == 0 failures: 100 at 0, else 0
        let eq0 = check("G-PAR.P4", Op::Eq, Some(0.0), "failures", "");
        assert_eq!(
            evaluate_check(&eq0, &measured(Num(0.0)), None),
            (Status::Pass, 100.0, "e".into())
        );
        assert_eq!(evaluate_check(&eq0, &measured(Num(3.0)), None).1, 0.0);

        // kind mismatch never passes
        assert_eq!(
            evaluate_check(&ge, &measured(Bool(true)), None).0,
            Status::NotMeasurable
        );
    }

    #[test]
    fn gate_style_observations() {
        let c = check("G-INT", Op::Pass, None, "", "");
        assert_eq!(
            evaluate_check(&c, &Observation::Passed("ok".into()), None).0,
            Status::Pass
        );
        assert_eq!(
            evaluate_check(&c, &Observation::Failed("x".into()), None),
            (Status::Fail, 0.0, "x".into())
        );
        assert_eq!(
            evaluate_check(&c, &Observation::NotMeasurable("pending".into()), None),
            (Status::NotMeasurable, 0.0, "pending".into())
        );
    }

    #[test]
    fn dev_only_pass_is_not_measurable_but_a_definitive_fail_fails() {
        let c = check(
            "G-PAR.P1",
            Op::Eq,
            Some(100.0),
            "%",
            "every corpus case that parses",
        );
        let dev_pass = Observation::Measured {
            value: Num(100.0),
            evidence: "dev".into(),
            dev_only: true,
            definitive_fail: false,
        };
        let (st, score, ev) = evaluate_check(&c, &dev_pass, None);
        assert_eq!((st, score), (Status::NotMeasurable, 0.0));
        assert!(ev.contains("held-out"), "{ev}");

        let dev_fail = Observation::Measured {
            value: Num(50.0),
            evidence: "dev".into(),
            dev_only: true,
            definitive_fail: true,
        };
        assert_eq!(evaluate_check(&c, &dev_fail, None).0, Status::Fail);

        // a dev count below the threshold that held-out cases could still make up
        let c21 = check("D1.corpus_syntax_pass", Op::Ge, Some(21.0), "tasks", "");
        let partial = Observation::Measured {
            value: Num(8.0),
            evidence: "dev".into(),
            dev_only: true,
            definitive_fail: false,
        };
        assert_eq!(
            evaluate_check(&c21, &partial, None).0,
            Status::NotMeasurable
        );
    }

    fn tiers_json() -> Value {
        json!({
            "order": ["M1", "M2", "M5", "M6", "beta"],
            "tiers": [
                {"id": "M1", "name": "one", "mode": "checklist", "scorecard_tier": null,
                 "stages_in_scope": ["S0"],
                 "checks": [
                    {"id": "G-INT", "op": "pass"},
                    {"id": "D5.x", "op": ">=", "value": 0.5},
                    {"id": "D1.fuzz", "op": ">=", "value": 24, "unit": "h", "scope": "m1 targets"}
                 ],
                 "ratchet_only": ["D4.kw"]},
                {"id": "M2", "name": "two", "mode": "checklist", "scorecard_tier": null, "ratchet_from": "M1",
                 "checks": [{"id": "D1.fuzz", "op": ">=", "value": 24, "unit": "h", "scope": "adds typeck"}]},
                {"id": "M5", "name": "mvp", "mode": "scorecard", "scorecard_tier": "mvp", "ratchet_from": "M2", "checks": []},
                {"id": "M6", "name": "six", "mode": "checklist", "scorecard_tier": null, "ratchet_from": "M5", "checks": []},
                {"id": "beta", "name": "beta", "mode": "scorecard", "scorecard_tier": "beta", "ratchet_from": "M6", "checks": []}
            ]
        })
    }

    fn all_pass(_: &TierCheck) -> Observation {
        Observation::Passed("ok".into())
    }

    fn no_threshold(_: &str) -> MetricThreshold {
        None
    }

    #[test]
    fn parses_tiers_and_resolves_targets() {
        let t = Tiers::parse(Some(&tiers_json())).unwrap();
        assert_eq!(t.order.len(), 5);
        assert_eq!(t.get("M1").unwrap().checks.len(), 3);
        assert_eq!(t.get("M1").unwrap().checks[1].op, Op::Ge);
        assert_eq!(t.resolve("mvp").unwrap().id, "M5");
        assert_eq!(t.resolve("M2").unwrap().id, "M2");
        assert!(t.resolve("M99").is_none());
        let chain: Vec<_> = t.chain("M6").iter().map(|t| t.id.as_str()).collect();
        assert_eq!(chain, ["M6", "M5", "M2", "M1"]);
        assert_eq!(t.normalization("M1"), Milestone::Mvp);
        assert_eq!(t.normalization("M6"), Milestone::Beta);
        assert_eq!(t.normalization("M5"), Milestone::Mvp);
    }

    #[test]
    fn ratchet_cycles_terminate() {
        let j = json!({"order": ["A", "B"], "tiers": [
            {"id": "A", "mode": "checklist", "ratchet_from": "B", "checks": []},
            {"id": "B", "mode": "checklist", "ratchet_from": "A", "checks": []}
        ]});
        let t = Tiers::parse(Some(&j)).unwrap();
        assert_eq!(t.chain("A").len(), 2);
    }

    #[test]
    fn checklist_tier_met_only_when_every_check_passes() {
        let t = Tiers::parse(Some(&tiers_json())).unwrap();
        let e = evaluate(&t, "M1", &all_pass, &no_threshold, None).unwrap();
        assert_eq!(e.mode, TierMode::Checklist);
        // 3 checks + 1 ratchet_only
        assert_eq!(e.checks.len(), 4);
        assert!(e.milestone_met);
        assert_eq!(e.tier_score, Some(100.0));

        let one_unmeasured = |c: &TierCheck| {
            if c.id == "D5.x" {
                Observation::NotMeasurable("no runner".into())
            } else {
                Observation::Passed("ok".into())
            }
        };
        let e = evaluate(&t, "M1", &one_unmeasured, &no_threshold, None).unwrap();
        assert!(!e.milestone_met, "an unmeasured check blocks the milestone");
        assert_eq!(e.tier_score, Some(75.0));
        let d5 = e.checks.iter().find(|c| c.id == "D5.x").unwrap();
        assert_eq!(d5.status, Status::NotMeasurable);
        assert_eq!(d5.evidence, "no runner");
    }

    #[test]
    fn ratchet_checks_are_included_and_same_ids_stay_distinct() {
        let t = Tiers::parse(Some(&tiers_json())).unwrap();
        let e = evaluate(&t, "M2", &all_pass, &no_threshold, None).unwrap();
        let fuzz: Vec<_> = e.checks.iter().filter(|c| c.id == "D1.fuzz").collect();
        assert_eq!(fuzz.len(), 2);
        assert_eq!(fuzz[0].tier, "M2");
        assert_eq!(fuzz[1].tier, "M1");
        assert_eq!(e.checks.len(), 5);
        let fail_m1_only = |c: &TierCheck| {
            if c.id == "D4.kw" {
                Observation::Failed("regressed".into())
            } else {
                Observation::Passed("ok".into())
            }
        };
        let e = evaluate(&t, "M2", &fail_m1_only, &no_threshold, None).unwrap();
        assert!(!e.milestone_met, "a broken ratchet blocks the milestone");
    }

    #[test]
    fn tier_with_no_checks_is_never_met() {
        let j = json!({"order": ["E"], "tiers": [{"id": "E", "mode": "checklist", "checks": []}]});
        let t = Tiers::parse(Some(&j)).unwrap();
        let e = evaluate(&t, "E", &all_pass, &no_threshold, None).unwrap();
        assert!(e.checks.is_empty());
        assert!(!e.milestone_met);
    }

    #[test]
    fn scorecard_tier_uses_the_section_6_rule_not_an_empty_checklist() {
        let t = Tiers::parse(Some(&tiers_json())).unwrap();
        // without the rule inputs a scorecard tier cannot be judged
        assert!(evaluate(&t, "M5", &all_pass, &no_threshold, None).is_err());

        let dims = vec![("D1".to_string(), 80.0), ("D2".to_string(), 50.0)];
        let rule = ScorecardRule {
            milestone: Milestone::Mvp,
            dimensions: &dims,
            composite: 65.0,
            dimension_min: Some(55.0),
            composite_min: Some(60.0),
            human_gates: &[],
        };
        let e = evaluate(&t, "M5", &all_pass, &no_threshold, Some(&rule)).unwrap();
        assert_eq!(e.mode, TierMode::Scorecard);
        assert_eq!(e.tier_score, None);
        assert!(!e.milestone_met, "D2 is below the dimension minimum");
        let d2 = e
            .checks
            .iter()
            .find(|c| c.id == "dimension_min.D2")
            .unwrap();
        assert_eq!(d2.status, Status::Fail);
        assert!(e
            .checks
            .iter()
            .any(|c| c.id == "G-PAR" && c.kind == "scorecard_rule"));
        // ratchet chain M2 -> M1 is included
        assert!(e.checks.iter().any(|c| c.tier == "M1"));

        let dims_ok = vec![("D1".to_string(), 80.0), ("D2".to_string(), 60.0)];
        let ok = ScorecardRule {
            dimensions: &dims_ok,
            ..rule
        };
        let e = evaluate(&t, "M5", &all_pass, &no_threshold, Some(&ok)).unwrap();
        assert!(e.milestone_met);

        let humans = vec!["H-RCT".to_string()];
        let with_humans = ScorecardRule {
            human_gates: &humans,
            dimensions: &dims_ok,
            ..ok
        };
        let e = evaluate(&t, "M5", &all_pass, &no_threshold, Some(&with_humans)).unwrap();
        let h = e
            .checks
            .iter()
            .find(|c| c.id == "human_gate.H-RCT")
            .unwrap();
        assert_eq!(h.status, Status::NotMeasurable);
        assert!(!e.milestone_met);
    }

    #[test]
    fn unknown_tier_is_an_error() {
        let t = Tiers::parse(Some(&tiers_json())).unwrap();
        assert!(evaluate(&t, "M42", &all_pass, &no_threshold, None).is_err());
    }

    #[test]
    fn parses_the_repo_tiers() {
        let root: Value =
            serde_json::from_str(include_str!("../../../../eval/scorecard.json")).unwrap();
        let t = Tiers::parse(root.get("milestone_tiers")).unwrap();
        for id in [
            "M1", "M2", "M3", "M4", "M5", "M6", "M7", "M8", "M9", "M10", "beta", "1.0",
        ] {
            assert!(t.get(id).is_some(), "{id}");
        }
        let m1 = t.get("M1").unwrap();
        assert_eq!(m1.mode, TierMode::Checklist);
        assert_eq!(m1.checks.len(), 18);
        assert_eq!(m1.ratchet_only.len(), 3);
        assert_eq!(t.get("M5").unwrap().scorecard_tier, Some(Milestone::Mvp));
        assert_eq!(t.normalization("M1"), Milestone::Mvp);
    }
}
