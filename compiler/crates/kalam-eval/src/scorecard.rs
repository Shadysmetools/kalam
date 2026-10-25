//! `eval/scorecard.json`: metric definitions, normalization to 0–100 and dimension scores
//! (SCORECARD.md §2). Thresholds are always read from the JSON file, never hardcoded.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Milestone {
    Mvp,
    Beta,
    One,
}

impl Milestone {
    pub const ALL: [Milestone; 3] = [Milestone::Mvp, Milestone::Beta, Milestone::One];

    pub fn key(self) -> &'static str {
        match self {
            Milestone::Mvp => "mvp",
            Milestone::Beta => "beta",
            Milestone::One => "1.0",
        }
    }

    pub fn parse(s: &str) -> Option<Milestone> {
        Milestone::ALL.into_iter().find(|m| m.key() == s)
    }

    fn index(self) -> usize {
        match self {
            Milestone::Mvp => 0,
            Milestone::Beta => 1,
            Milestone::One => 2,
        }
    }
}

/// One milestone threshold cell.
#[derive(Clone, Debug, PartialEq)]
pub enum Threshold {
    Num(f64),
    Bool(bool),
    NotApplicable,
    ReportOnly,
}

impl Threshold {
    fn from_json(v: Option<&Value>) -> Threshold {
        match v {
            Some(Value::Number(n)) => Threshold::Num(n.as_f64().unwrap_or(0.0)),
            Some(Value::Bool(b)) => Threshold::Bool(*b),
            Some(Value::String(s)) if s == "report_only" => Threshold::ReportOnly,
            _ => Threshold::NotApplicable,
        }
    }

    fn num(&self) -> Option<f64> {
        match self {
            Threshold::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Threshold::Num(n) => serde_json::json!(n),
            Threshold::Bool(b) => Value::Bool(*b),
            Threshold::NotApplicable => Value::String("n/a".into()),
            Threshold::ReportOnly => Value::String("report_only".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Higher,
    Lower,
    Binary,
    Ladder,
}

#[derive(Clone, Debug)]
pub struct MetricDef {
    pub id: String,
    pub dimension: String,
    pub name: String,
    pub direction: Direction,
    pub unit: String,
    pub floor: f64,
    /// `[mvp, beta, 1.0]`
    pub thresholds: [Threshold; 3],
    pub stretch: Option<f64>,
    pub stage: String,
    pub gate: Option<String>,
    pub levels: usize,
}

#[derive(Clone, Debug)]
pub struct Dimension {
    pub id: String,
    pub name: String,
    pub weight: f64,
}

#[derive(Clone, Debug)]
pub struct Scorecard {
    pub eval_version: String,
    pub dimensions: Vec<Dimension>,
    pub metrics: Vec<MetricDef>,
    /// `(stage id, stage name)`
    pub stages: Vec<(String, String)>,
}

impl Scorecard {
    pub fn parse(json: &str) -> Result<Scorecard, String> {
        let root: Value = serde_json::from_str(json).map_err(|e| format!("scorecard.json: {e}"))?;
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let arr = |k: &str| -> Result<&Vec<Value>, String> {
            root.get(k)
                .and_then(Value::as_array)
                .ok_or_else(|| format!("scorecard.json: missing array `{k}`"))
        };

        let dimensions = arr("dimensions")?
            .iter()
            .map(|d| Dimension {
                id: s(d, "id"),
                name: s(d, "name"),
                weight: d.get("weight").and_then(Value::as_f64).unwrap_or(0.0),
            })
            .collect();

        let mut metrics = Vec::new();
        for m in arr("metrics")? {
            let direction = match m.get("direction").and_then(Value::as_str) {
                Some("higher") => Direction::Higher,
                Some("lower") => Direction::Lower,
                Some("binary") => Direction::Binary,
                Some("ladder") => Direction::Ladder,
                other => {
                    return Err(format!(
                        "metric {}: unknown direction {other:?}",
                        s(m, "id")
                    ))
                }
            };
            let th = m.get("thresholds");
            let cell = |k: &str| Threshold::from_json(th.and_then(|t| t.get(k)));
            metrics.push(MetricDef {
                id: s(m, "id"),
                dimension: s(m, "dimension"),
                name: s(m, "name"),
                direction,
                unit: s(m, "unit"),
                floor: m.get("floor").and_then(Value::as_f64).unwrap_or(0.0),
                thresholds: [cell("mvp"), cell("beta"), cell("1.0")],
                stretch: m.get("stretch").and_then(Value::as_f64),
                stage: s(m, "stage"),
                gate: m.get("gate").and_then(Value::as_str).map(str::to_string),
                levels: m
                    .get("levels")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            });
        }

        let stages = arr("stages")?
            .iter()
            .map(|st| (s(st, "id"), s(st, "name")))
            .collect();

        Ok(Scorecard {
            eval_version: s(&root, "eval_version"),
            dimensions,
            metrics,
            stages,
        })
    }

    pub fn metric(&self, id: &str) -> Option<&MetricDef> {
        self.metrics.iter().find(|m| m.id == id)
    }

    pub fn stage_name(&self, id: &str) -> &str {
        self.stages
            .iter()
            .find(|(s, _)| s == id)
            .map_or("", |(_, n)| n.as_str())
    }
}

/// A measured value.
#[derive(Clone, Debug, PartialEq)]
pub enum MetricValue {
    Num(f64),
    Bool(bool),
}

impl MetricValue {
    pub fn to_json(&self) -> Value {
        match self {
            MetricValue::Num(n) => serde_json::json!(n),
            MetricValue::Bool(b) => Value::Bool(*b),
        }
    }
}

/// How a metric enters the dimension mean for the target milestone (SCORECARD.md §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inclusion {
    /// Counted in the dimension mean.
    Counted,
    /// `n/a` for the target milestone: left out of the mean.
    NotApplicable,
    /// `report_only`: recorded, weight 0.
    ReportOnly,
}

pub fn inclusion(def: &MetricDef, target: Milestone) -> Inclusion {
    match def.thresholds[target.index()] {
        Threshold::NotApplicable => Inclusion::NotApplicable,
        Threshold::ReportOnly => Inclusion::ReportOnly,
        _ => Inclusion::Counted,
    }
}

/// True when the 1.0 threshold is the metric's natural bound (100%, a full count,
/// 0 failures), so meeting it scores 100 (scorecard.json `normalization.stretch_default`).
fn natural_bound(def: &MetricDef, one: f64) -> bool {
    let unit = def.unit.trim();
    if unit == "%" && one == 100.0 {
        return true;
    }
    if def.direction == Direction::Lower && one == 0.0 {
        return true;
    }
    // "of 8", "apps of 10": the count is out of N.
    if let Some(n) = unit.rsplit("of ").next().filter(|_| unit.contains("of ")) {
        if n.trim().parse::<f64>().ok() == Some(one) {
            return true;
        }
    }
    // `tasks`: the 1.0 threshold is the whole corpus (INDEX.md: 50 tasks).
    unit == "tasks"
}

/// Normalizes a value to 0–100 (SCORECARD.md §2). Returns `None` when the value's kind
/// does not match the metric's direction.
pub fn normalize(def: &MetricDef, value: &MetricValue) -> Option<f64> {
    match (def.direction, value) {
        (Direction::Binary, MetricValue::Bool(b)) => Some(if *b { 100.0 } else { 0.0 }),
        (Direction::Higher | Direction::Lower | Direction::Ladder, MetricValue::Num(v)) => {
            Some(piecewise(&anchors(def), signed(def, *v)))
        }
        _ => None,
    }
}

/// Lower-is-better metrics are scored on the negated axis, so one routine serves both.
fn signed(def: &MetricDef, x: f64) -> f64 {
    if def.direction == Direction::Lower {
        -x
    } else {
        x
    }
}

/// `(x on the signed axis, score)` pairs in ascending score order.
fn anchors(def: &MetricDef) -> Vec<(f64, f64)> {
    let mut out = vec![(signed(def, def.floor), 0.0)];
    let ladder = def.direction == Direction::Ladder;
    let scores = if ladder {
        [60.0, 75.0, 100.0]
    } else {
        [60.0, 75.0, 90.0]
    };
    for (t, score) in def.thresholds.iter().zip(scores) {
        if let Some(x) = t.num() {
            out.push((signed(def, x), score));
        }
    }
    if ladder {
        if def.levels > 0 {
            out.push(((def.levels - 1) as f64, 100.0));
        }
        return out;
    }
    let beta = def.thresholds[1].num();
    let one = def.thresholds[2].num();
    match (def.stretch, one) {
        (Some(s), _) => out.push((signed(def, s), 100.0)),
        (None, Some(one)) if natural_bound(def, one) => out.push((signed(def, one), 100.0)),
        (None, Some(one)) => {
            if let Some(beta) = beta {
                let (one, beta) = (signed(def, one), signed(def, beta));
                out.push((one + (one - beta), 100.0));
            }
        }
        _ => {}
    }
    out
}

/// Piecewise-linear through the anchors; a value meeting an anchor gets at least that
/// anchor's score, and tied anchors give the higher score.
fn piecewise(anchors: &[(f64, f64)], v: f64) -> f64 {
    let Some(i) = anchors.iter().rposition(|(x, _)| v >= *x) else {
        return 0.0;
    };
    let (xi, si) = anchors[i];
    let score = match anchors.get(i + 1) {
        Some(&(xj, sj)) if xj > xi => si + (v - xi) / (xj - xi) * (sj - si),
        _ => si,
    };
    score.clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(
        direction: Direction,
        unit: &str,
        floor: f64,
        t: [Threshold; 3],
        stretch: Option<f64>,
    ) -> MetricDef {
        MetricDef {
            id: "X.test".into(),
            dimension: "X".into(),
            name: "test".into(),
            direction,
            unit: unit.into(),
            floor,
            thresholds: t,
            stretch,
            stage: "S0".into(),
            gate: None,
            levels: 0,
        }
    }

    use Threshold::Num;

    fn approx(a: Option<f64>, b: f64) {
        let a = a.expect("normalized");
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn worked_example_tokens_vs_python_scores_68() {
        // SCORECARD.md §2 worked example.
        let d = def(
            Direction::Lower,
            "ratio",
            3.9,
            [Num(1.3), Num(1.15), Num(1.1)],
            None,
        );
        approx(normalize(&d, &MetricValue::Num(1.22)), 68.0);
    }

    #[test]
    fn anchors_score_exactly() {
        let d = def(
            Direction::Higher,
            "%",
            0.0,
            [Num(60.0), Num(80.0), Num(90.0)],
            None,
        );
        approx(normalize(&d, &MetricValue::Num(0.0)), 0.0);
        approx(normalize(&d, &MetricValue::Num(60.0)), 60.0);
        approx(normalize(&d, &MetricValue::Num(80.0)), 75.0);
        approx(normalize(&d, &MetricValue::Num(90.0)), 90.0);
        // stretch default: 90 + (90 - 80) = 100 on the axis
        approx(normalize(&d, &MetricValue::Num(95.0)), 95.0);
        approx(normalize(&d, &MetricValue::Num(100.0)), 100.0);
        approx(normalize(&d, &MetricValue::Num(30.0)), 30.0);
    }

    #[test]
    fn clamps_below_floor_and_above_stretch() {
        let d = def(
            Direction::Lower,
            "count",
            120.0,
            [Num(40.0), Num(35.0), Num(35.0)],
            None,
        );
        approx(normalize(&d, &MetricValue::Num(500.0)), 0.0);
        approx(normalize(&d, &MetricValue::Num(1.0)), 100.0);
    }

    #[test]
    fn tied_anchors_give_the_higher_score() {
        // D4.keywords_per_skin: beta = 1.0 = 35, stretch default also 35.
        let d = def(
            Direction::Lower,
            "count",
            120.0,
            [Num(40.0), Num(35.0), Num(35.0)],
            None,
        );
        approx(normalize(&d, &MetricValue::Num(35.0)), 100.0);
        approx(normalize(&d, &MetricValue::Num(40.0)), 60.0);
        // between floor 120 (0) and mvp 40 (60)
        approx(normalize(&d, &MetricValue::Num(80.0)), 30.0);
    }

    #[test]
    fn natural_bound_meeting_one_point_oh_scores_100() {
        let d = def(
            Direction::Higher,
            "%",
            0.0,
            [Num(60.0), Num(90.0), Num(100.0)],
            None,
        );
        approx(normalize(&d, &MetricValue::Num(100.0)), 100.0);
        let d = def(
            Direction::Higher,
            "of 8",
            0.0,
            [Num(3.0), Num(6.0), Num(8.0)],
            None,
        );
        approx(normalize(&d, &MetricValue::Num(8.0)), 100.0);
    }

    #[test]
    fn explicit_stretch_is_used() {
        let d = def(
            Direction::Higher,
            "ratio",
            0.0,
            [Num(0.6), Num(0.8), Num(0.9)],
            Some(1.0),
        );
        approx(normalize(&d, &MetricValue::Num(0.95)), 95.0);
    }

    #[test]
    fn binary_and_kind_mismatch() {
        let t = [
            Threshold::Bool(true),
            Threshold::Bool(true),
            Threshold::Bool(true),
        ];
        let d = def(Direction::Binary, "bool", 0.0, t, None);
        approx(normalize(&d, &MetricValue::Bool(true)), 100.0);
        approx(normalize(&d, &MetricValue::Bool(false)), 0.0);
        assert_eq!(normalize(&d, &MetricValue::Num(1.0)), None);
    }

    #[test]
    fn ladder_levels() {
        let mut d = def(
            Direction::Ladder,
            "level",
            0.0,
            [Num(1.0), Num(2.0), Num(3.0)],
            None,
        );
        d.levels = 4;
        approx(normalize(&d, &MetricValue::Num(0.0)), 0.0);
        approx(normalize(&d, &MetricValue::Num(1.0)), 60.0);
        approx(normalize(&d, &MetricValue::Num(2.0)), 75.0);
        approx(normalize(&d, &MetricValue::Num(3.0)), 100.0);
    }

    #[test]
    fn inclusion_follows_target_milestone_cell() {
        let t = [Threshold::NotApplicable, Threshold::ReportOnly, Num(1.0)];
        let d = def(Direction::Higher, "x", 0.0, t, None);
        assert_eq!(inclusion(&d, Milestone::Mvp), Inclusion::NotApplicable);
        assert_eq!(inclusion(&d, Milestone::Beta), Inclusion::ReportOnly);
        assert_eq!(inclusion(&d, Milestone::One), Inclusion::Counted);
    }

    #[test]
    fn parses_the_repo_scorecard() {
        let json = include_str!("../../../../eval/scorecard.json");
        let sc = Scorecard::parse(json).expect("parses");
        assert_eq!(sc.dimensions.len(), 8);
        assert!(sc.metrics.len() >= 50);
        let kw = sc.metric("D4.keywords_per_skin").expect("metric exists");
        assert_eq!(kw.direction, Direction::Lower);
        assert_eq!(kw.thresholds[0], Num(40.0));
        let tp = sc.metric("D4.tokens_vs_python").unwrap();
        approx(normalize(tp, &MetricValue::Num(1.22)), 68.0);
    }
}
