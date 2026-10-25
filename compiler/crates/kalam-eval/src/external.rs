//! Results written by other runners, read (never recomputed or edited) by the harness:
//!
//! - `eval/props/report.json`, written by the `kalam-props` property suite
//!   (`D1.property_tests`, `G-PAR.P4`), or a fresh report the harness asks
//!   `cargo test -p kalam-props --release` to write (`kalam-eval --run-props`);
//! - `eval/fuzz/status.json`, written by the fuzz runner (`D1.fuzz_crash_free_hours`).
//!
//! The accepted schemas are documented in eval/HISTORY-SCHEMA.md. A missing, malformed,
//! stale or under-scoped file is `not_measurable`, never a pass.

use crate::scorecard::MetricValue;
use crate::tiers::Observation;
use serde_json::Value;
use std::path::Path;

pub const PROPS_REPORT: &str = "eval/props/report.json";
pub const FUZZ_STATUS: &str = "eval/fuzz/status.json";

/// Minimum generated cases per property (scorecard: ">= 10k cases each").
pub const MIN_CASES: u64 = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub name: String,
    pub cases: u64,
    pub failures: u64,
    /// `false` for a report-only property (`"gating": false`); it never counts.
    pub gating: bool,
    /// The G-PAR checks (`P1`, `P2`, `P3`) this property runs on every generated program,
    /// exactly as its own `parity` list says (none when it has no list: coverage is never
    /// guessed from a name or a description).
    pub parity: Vec<String>,
}

/// Where the report came from: a file whose commit must match, or a run the harness just
/// made on this tree.
#[derive(Clone, Debug, PartialEq)]
pub enum Provenance {
    Commit(String),
    FreshRun,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropsReport {
    pub source: String,
    pub provenance: Option<Provenance>,
    pub properties: Vec<Property>,
}

fn num(v: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| v.get(*k).and_then(Value::as_u64))
}

fn property(name: &str, v: &Value) -> Result<Property, String> {
    let cases = num(v, &["cases", "cases_run", "programs"])
        .ok_or_else(|| format!("property `{name}` has no integer `cases`"))?;
    let failures = num(v, &["failures", "failed"])
        .ok_or_else(|| format!("property `{name}` has no integer `failures`"))?;
    let parity = v
        .get("parity")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(Property {
        name: name.to_string(),
        cases,
        failures,
        gating: v.get("gating").and_then(Value::as_bool).unwrap_or(true),
        parity,
    })
}

impl PropsReport {
    pub fn parse(text: &str) -> Result<PropsReport, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        let mut properties = Vec::new();
        match v.get("properties") {
            Some(Value::Array(a)) => {
                for p in a {
                    let name = p
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or("a property has no `name`")?;
                    properties.push(property(name, p)?);
                }
            }
            Some(Value::Object(m)) => {
                for (name, p) in m {
                    properties.push(property(name, p)?);
                }
            }
            _ => return Err("no `properties` array or object".into()),
        }
        let commit = ["commit", "git_sha"]
            .iter()
            .find_map(|k| v.get(*k).and_then(Value::as_str))
            .map(|c| Provenance::Commit(c.to_string()));
        Ok(PropsReport {
            source: PROPS_REPORT.into(),
            provenance: commit,
            properties,
        })
    }

    /// Gating properties whose name contains one of `needles`.
    fn matching(&self, needles: &[&str]) -> Vec<&Property> {
        self.properties
            .iter()
            .filter(|p| {
                let n = p.name.to_lowercase().replace(['-', ' '], "_");
                p.gating && needles.iter().any(|k| n.contains(k))
            })
            .collect()
    }

    fn origin(&self) -> String {
        match &self.provenance {
            Some(Provenance::Commit(c)) => format!("{} (commit {c})", self.source),
            Some(Provenance::FreshRun) => format!("{} (written by this run)", self.source),
            None => self.source.clone(),
        }
    }
}

/// Loads `eval/props/report.json` and ties it to the evaluated commit.
pub fn load_props(root: &Path, head: Option<&str>) -> Result<PropsReport, String> {
    let text = std::fs::read_to_string(root.join(PROPS_REPORT))
        .map_err(|_| format!("{PROPS_REPORT} does not exist (the kalam-props suite writes it)"))?;
    let r = PropsReport::parse(&text).map_err(|e| format!("{PROPS_REPORT}: {e}"))?;
    match (&r.provenance, head) {
        (None, _) => Err(format!(
            "{PROPS_REPORT} has no `commit`, so it cannot be tied to the evaluated tree \
             (run `kalam-eval --run-props`, or have kalam-props record the commit)"
        )),
        (Some(_), None) => Err("the evaluated commit is unknown (git unavailable)".into()),
        (Some(Provenance::Commit(c)), Some(h)) if !same_commit(c, h) => Err(format!(
            "{PROPS_REPORT} is stale: it was written for commit {c}, the evaluated commit is {h}"
        )),
        _ => Ok(r),
    }
}

/// A report the harness just had `cargo test -p kalam-props` write at `path`.
pub fn load_fresh(path: &Path) -> Result<PropsReport, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| format!("kalam-props wrote no report at {}", path.display()))?;
    let mut r = PropsReport::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    r.source = path.display().to_string();
    r.provenance = Some(Provenance::FreshRun);
    Ok(r)
}

/// Full or abbreviated SHAs (at least 7 hex digits) of the same commit.
fn same_commit(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim().to_lowercase(), b.trim().to_lowercase());
    a.len() >= 7 && b.len() >= 7 && (a.starts_with(&b) || b.starts_with(&a))
}

/// `D1.property_tests` at M1: every gating fmt-idempotence property and every gating
/// round-trip property passes on at least [`MIN_CASES`] cases (at least one of each).
pub fn property_tests(r: &Result<PropsReport, String>) -> Observation {
    let r = match r {
        Err(why) => return Observation::NotMeasurable(why.clone()),
        Ok(r) => r,
    };
    let wanted = [
        ("fmt idempotence", &["idempot"][..]),
        ("round-trip", &["round_trip", "roundtrip"][..]),
    ];
    let mut evidence = Vec::new();
    let mut missing = Vec::new();
    let mut failed = Vec::new();
    let mut short = false;
    for (label, needles) in wanted {
        let props = r.matching(needles);
        if props.is_empty() {
            missing.push(label);
        }
        for p in props {
            evidence.push(format!(
                "{label}: `{}` {} cases, {} failures",
                p.name, p.cases, p.failures
            ));
            if p.failures > 0 {
                failed.push(format!(
                    "`{}` failed {} of {} cases",
                    p.name, p.failures, p.cases
                ));
            }
            short |= p.cases < MIN_CASES;
        }
    }
    let evidence = format!("{}: {}", r.origin(), evidence.join("; "));
    if !failed.is_empty() {
        Observation::Failed(format!("{evidence}; {}", failed.join("; ")))
    } else if !missing.is_empty() {
        Observation::NotMeasurable(format!("{evidence}; no gating {missing:?} property"))
    } else if short {
        Observation::NotMeasurable(format!(
            "{evidence}; fewer than {MIN_CASES} cases per property"
        ))
    } else {
        Observation::Passed(evidence)
    }
}

/// `G-PAR.P4`: failures over at least [`MIN_CASES`] generated programs run through P1, P2
/// and P3 (a property counts only for the checks its own `parity` list names).
pub fn p4(r: &Result<PropsReport, String>) -> Observation {
    let r = match r {
        Err(why) => return Observation::NotMeasurable(why.clone()),
        Ok(r) => r,
    };
    const CHECKS: [&str; 3] = ["P1", "P2", "P3"];
    let covering: Vec<&Property> = r
        .properties
        .iter()
        .filter(|p| p.gating && p.parity.iter().any(|c| CHECKS.contains(&c.as_str())))
        .collect();
    let failures: u64 = covering.iter().map(|p| p.failures).sum();
    let mut cover = Vec::new();
    for check in CHECKS {
        let cases = covering
            .iter()
            .filter(|p| p.parity.iter().any(|c| c == check))
            .map(|p| p.cases)
            .max()
            .unwrap_or(0);
        cover.push(format!("{check}: {cases} cases"));
        // a failure is a failure at any count; a clean run must reach the minimum
        if cases < MIN_CASES && failures == 0 {
            return Observation::NotMeasurable(format!(
                "{}: {check} is run on {cases} generated programs (< {MIN_CASES}); properties: {:?}",
                r.origin(),
                covering.iter().map(|p| (&p.name, &p.parity)).collect::<Vec<_>>()
            ));
        }
    }
    Observation::Measured {
        value: MetricValue::Num(failures as f64),
        evidence: format!(
            "{}: {failures} failures over {:?}; {}",
            r.origin(),
            covering.iter().map(|p| &p.name).collect::<Vec<_>>(),
            cover.join(", ")
        ),
        dev_only: false,
        definitive_fail: false,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuzzStatus {
    pub crash_free_hours: f64,
    pub crashes: Option<u64>,
    pub last_run: Option<String>,
    pub targets: Option<Vec<String>>,
    /// Oracle violations the fuzz runner reports (`oracle_violations.<name>.count`): not
    /// crashes, so they do not reset the clock; shown in the evidence.
    pub oracle_violations: u64,
}

impl FuzzStatus {
    pub fn parse(text: &str) -> Result<FuzzStatus, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        Ok(FuzzStatus {
            crash_free_hours: v
                .get("crash_free_hours")
                .and_then(Value::as_f64)
                .filter(|h| *h >= 0.0)
                .ok_or("no non-negative number `crash_free_hours`")?,
            crashes: v.get("crashes").and_then(Value::as_u64),
            last_run: v
                .get("last_run")
                .and_then(Value::as_str)
                .map(str::to_string),
            targets: v.get("targets").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().or_else(|| t.get("name").and_then(Value::as_str)))
                    .map(str::to_string)
                    .collect()
            }),
            oracle_violations: v
                .get("oracle_violations")
                .and_then(Value::as_object)
                .map(|m| m.values().filter_map(|o| o["count"].as_u64()).sum())
                .unwrap_or(0),
        })
    }
}

/// Unfixed crashes in `eval/fuzz/status.json`, if it says (G-FUZZ fails on any).
pub fn fuzz_crashes(root: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(root.join(FUZZ_STATUS)).ok()?;
    FuzzStatus::parse(&text).ok()?.crashes
}

/// The fuzz targets the M1 scope needs, as name fragments.
const M1_TARGETS: [(&str, &[&str]); 3] = [
    ("lexer", &["lex"]),
    ("parser", &["pars"]),
    ("formatter", &["fmt", "format"]),
];

/// Fuzz target names found in the repository: `fuzz_targets/*.rs` under `compiler/fuzz`
/// or any `compiler/crates/*/fuzz`.
pub fn repo_fuzz_targets(root: &Path) -> Vec<String> {
    let mut dirs = vec![root.join("compiler/fuzz/fuzz_targets")];
    if let Ok(rd) = std::fs::read_dir(root.join("compiler/crates")) {
        for e in rd.flatten() {
            dirs.push(e.path().join("fuzz/fuzz_targets"));
        }
    }
    let mut out = Vec::new();
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if let Some(stem) = n.strip_suffix(".rs") {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// `D1.fuzz_crash_free_hours` at M1 scope (lexer, parser and formatter targets).
pub fn fuzz(root: &Path) -> Observation {
    let text = match std::fs::read_to_string(root.join(FUZZ_STATUS)) {
        Ok(t) => t,
        Err(_) => {
            return Observation::NotMeasurable(format!(
                "{FUZZ_STATUS} does not exist (the fuzz runner writes it)"
            ))
        }
    };
    let s = match FuzzStatus::parse(&text) {
        Ok(s) => s,
        Err(e) => return Observation::NotMeasurable(format!("{FUZZ_STATUS}: {e}")),
    };
    let (targets, source) = match &s.targets {
        Some(t) => (t.clone(), FUZZ_STATUS),
        None => (
            repo_fuzz_targets(root),
            "fuzz_targets/*.rs in the repository",
        ),
    };
    let missing: Vec<&str> = M1_TARGETS
        .iter()
        .filter(|(_, frags)| {
            !targets
                .iter()
                .any(|t| frags.iter().any(|f| t.to_lowercase().contains(f)))
        })
        .map(|(n, _)| *n)
        .collect();
    let evidence = format!(
        "{FUZZ_STATUS}: crash_free_hours = {}, crashes = {:?}, last_run = {:?}, oracle violations = {}; \
         targets ({source}): {targets:?}",
        s.crash_free_hours, s.crashes, s.last_run, s.oracle_violations
    );
    if !missing.is_empty() {
        return Observation::NotMeasurable(format!(
            "{evidence}; cannot confirm the M1 scope: no {missing:?} fuzz target"
        ));
    }
    Observation::Measured {
        value: MetricValue::Num(s.crash_free_hours),
        evidence,
        dev_only: false,
        definitive_fail: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suites::tests::temp_dir;
    use std::fs;

    const GOOD: &str = r#"{"schema": "kalam-props-report/1", "commit": "abc1234def",
        "properties": [
          {"name": "fmt_idempotence", "cases": 10000, "failures": 0, "parity": []},
          {"name": "skin_round_trip", "cases": 12000, "failures": 0, "parity": ["P1", "P2", "P3"]}
        ]}"#;

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    #[test]
    fn props_report_passes_only_at_head_with_enough_cases() {
        let d = temp_dir("props");
        assert!(matches!(
            property_tests(&load_props(&d, Some("abc1234"))),
            Observation::NotMeasurable(_)
        ));
        write(&d, PROPS_REPORT, GOOD);
        let r = load_props(&d, Some("abc1234def0000"));
        assert!(
            matches!(property_tests(&r), Observation::Passed(_)),
            "{r:?}"
        );
        match p4(&r) {
            Observation::Measured { value, .. } => assert_eq!(value, MetricValue::Num(0.0)),
            o => panic!("{o:?}"),
        }
        // stale report
        let stale = load_props(&d, Some("ffffffff"));
        assert!(
            matches!(property_tests(&stale), Observation::NotMeasurable(ref w) if w.contains("stale"))
        );
        // too few cases
        write(&d, PROPS_REPORT, &GOOD.replace("10000", "999"));
        let r = load_props(&d, Some("abc1234def"));
        assert!(matches!(property_tests(&r), Observation::NotMeasurable(_)));
        // a failure is a definitive fail
        write(
            &d,
            PROPS_REPORT,
            &GOOD.replace(
                "\"failures\": 0, \"parity\": []",
                "\"failures\": 2, \"parity\": []",
            ),
        );
        let r = load_props(&d, Some("abc1234def"));
        assert!(matches!(property_tests(&r), Observation::Failed(_)));
        // no commit: cannot be tied to the tree
        write(
            &d,
            PROPS_REPORT,
            &GOOD.replace("\"commit\": \"abc1234def\",", ""),
        );
        assert!(load_props(&d, Some("abc1234def")).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn p4_needs_declared_parity_coverage() {
        let no_cover =
            PropsReport::parse(&GOOD.replace("[\"P1\", \"P2\", \"P3\"]", "[\"P1\"]")).unwrap();
        assert!(matches!(p4(&Ok(no_cover)), Observation::NotMeasurable(ref w) if w.contains("P2")));
        let map_form = PropsReport::parse(
            r#"{"commit": "x", "properties": {"rt": {"cases_run": 20000, "failed": 1, "parity": ["P1","P2","P3"]}}}"#,
        )
        .unwrap();
        match p4(&Ok(map_form)) {
            Observation::Measured { value, .. } => assert_eq!(value, MetricValue::Num(1.0)),
            o => panic!("{o:?}"),
        }
    }

    /// The shape `kalam-props` wrote before iteration 5: no `parity` lists, a `gating` flag,
    /// no commit.
    const KALAM_PROPS: &str = r#"{"suite": "kalam-props", "passed": false, "properties": [
        {"name": "ast_print_parse_roundtrip", "cases": 10000, "failures": 3, "gating": true},
        {"name": "fmt_idempotence", "cases": 10000, "failures": 0, "gating": true},
        {"name": "convert_roundtrip", "cases": 10000, "failures": 0, "gating": true},
        {"name": "kcf_cross_skin_equality", "cases": 10000, "failures": 0, "gating": true},
        {"name": "kcf_fmt_invariance", "cases": 10000, "failures": 9, "gating": false}
    ]}"#;

    #[test]
    fn kalam_props_report_shape() {
        let d = temp_dir("props-shape");
        write(&d, PROPS_REPORT, KALAM_PROPS);
        // no commit: only a run made by the harness itself is tied to the tree
        assert!(load_props(&d, Some("abc1234")).is_err());
        let fresh = load_fresh(&d.join(PROPS_REPORT));
        let _ = fs::remove_dir_all(&d);
        // every gating round-trip property counts: the failing AST round trip fails it
        match property_tests(&fresh) {
            Observation::Failed(w) => assert!(w.contains("ast_print_parse_roundtrip"), "{w}"),
            o => panic!("{o:?}"),
        }
        // no property declares the parity checks it runs: P4 is never guessed
        assert!(matches!(p4(&fresh), Observation::NotMeasurable(_)));
        let declared = load_fresh_text(&KALAM_PROPS.replace(
            "\"name\": \"convert_roundtrip\",",
            "\"name\": \"convert_roundtrip\", \"parity\": [\"P1\", \"P2\", \"P3\"],",
        ));
        match p4(&declared) {
            Observation::Measured {
                value, evidence, ..
            } => {
                assert_eq!(value, MetricValue::Num(0.0));
                // the report-only property is ignored
                assert!(!evidence.contains("kcf_fmt_invariance"), "{evidence}");
            }
            o => panic!("{o:?}"),
        }
        let ok = load_fresh_text(&KALAM_PROPS.replace("\"failures\": 3", "\"failures\": 0"));
        assert!(
            matches!(property_tests(&ok), Observation::Passed(_)),
            "{ok:?}"
        );
    }

    /// The shape `kalam-props` writes now: a commit and a `parity` list per property. A
    /// program accepted in some skins only fails `kcf_cross_skin_equality` (P1), so P4 is
    /// measured even when the dependent properties ran on fewer than 10k programs.
    #[test]
    fn current_kalam_props_shape_makes_p4_measurable() {
        let text = r#"{"suite": "kalam-props", "commit": "abc1234def", "properties": [
            {"name": "ast_print_parse_roundtrip", "gating": true, "parity": [], "cases": 10000, "failures": 3300},
            {"name": "fmt_idempotence", "gating": true, "parity": [], "cases": 6700, "failures": 0},
            {"name": "convert_roundtrip", "gating": true, "parity": ["P2"], "cases": 6700, "failures": 0},
            {"name": "kcf_cross_skin_equality", "gating": true, "parity": ["P1", "P2"], "cases": 10000, "failures": 3300},
            {"name": "kcf_hash_stability", "gating": true, "parity": ["P1"], "cases": 6700, "failures": 0},
            {"name": "source_fixpoint", "gating": true, "parity": ["P3"], "cases": 6700, "failures": 0},
            {"name": "kcf_fmt_invariance", "gating": false, "parity": [], "cases": 6700, "failures": 9}
        ]}"#;
        let d = temp_dir("props-now");
        write(&d, PROPS_REPORT, text);
        let r = load_props(&d, Some("abc1234def"));
        let clean = load_props(&d, Some("abc1234"));
        write(
            &d,
            PROPS_REPORT,
            &text.replace("3300", "0").replace("6700", "10000"),
        );
        let ok = load_props(&d, Some("abc1234def"));
        let _ = fs::remove_dir_all(&d);
        assert!(clean.is_ok());
        match p4(&r) {
            Observation::Measured { value, .. } => assert_eq!(value, MetricValue::Num(3300.0)),
            o => panic!("{o:?}"),
        }
        assert!(matches!(property_tests(&r), Observation::Failed(_)));
        match p4(&ok) {
            Observation::Measured { value, .. } => assert_eq!(value, MetricValue::Num(0.0)),
            o => panic!("{o:?}"),
        }
        assert!(matches!(property_tests(&ok), Observation::Passed(_)));
    }

    fn load_fresh_text(text: &str) -> Result<PropsReport, String> {
        let d = temp_dir("props-fresh");
        write(&d, "r.json", text);
        let r = load_fresh(&d.join("r.json"));
        let _ = fs::remove_dir_all(&d);
        r
    }

    #[test]
    fn fuzz_status_needs_the_m1_targets() {
        let d = temp_dir("fuzz");
        assert!(matches!(fuzz(&d), Observation::NotMeasurable(_)));
        write(
            &d,
            FUZZ_STATUS,
            r#"{"crash_free_hours": 30.5, "crashes": 0, "last_run": "2026-10-25T00:00:00Z"}"#,
        );
        // no targets listed and none in the repository: scope unconfirmed
        assert!(matches!(fuzz(&d), Observation::NotMeasurable(ref w) if w.contains("scope")));
        for t in ["lexer", "parser", "fmt_idempotence"] {
            write(&d, &format!("compiler/fuzz/fuzz_targets/{t}.rs"), "");
        }
        match fuzz(&d) {
            Observation::Measured { value, .. } => assert_eq!(value, MetricValue::Num(30.5)),
            o => panic!("{o:?}"),
        }
        assert_eq!(fuzz_crashes(&d), Some(0));
        write(
            &d,
            FUZZ_STATUS,
            r#"{"crash_free_hours": 3, "targets": ["lexer"], "oracle_violations": {"a": {"count": 2}}}"#,
        );
        assert_eq!(
            FuzzStatus::parse(&fs::read_to_string(d.join(FUZZ_STATUS)).unwrap())
                .unwrap()
                .oracle_violations,
            2
        );
        assert!(matches!(fuzz(&d), Observation::NotMeasurable(_)));
        write(&d, FUZZ_STATUS, r#"{"crashes": 0}"#);
        assert!(matches!(fuzz(&d), Observation::NotMeasurable(_)));
        let _ = fs::remove_dir_all(&d);
    }
}
