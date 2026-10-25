//! Spec conformance: `eval/conformance/<clause-id>/<case>/` (`D1.spec_clause_coverage`,
//! `D1.conformance_pass_rate`).
//!
//! A case is the shared suite format (eval/suites/README.md): source files named with the
//! spec skin extensions, the same program once per skin, plus an `expect.json`:
//!
//! ```text
//! {"must_reject": bool, "codes": ["K0401"], "lines": [3], "notes": "spec sentence",
//!  "stdout": "text the program prints"?, "stage": "lex|parse|resolve|typeck|interp"?}
//! ```
//!
//! - `<clause-id>` is a clause or unit id of `eval/conformance/CLAUSES.json`, with or without
//!   the leading `§` (`13.1.3` or `§13.1.3`). An id the spec does not have is a defect.
//! - `stage` is the compiler stage the case needs. Default: `interp` when `stdout` is set,
//!   else by section: §4–§8 `parse`, §10.1 and §13 `resolve`, everything else `typeck`.
//! - `eval/conformance/pending/` is never counted (LOOP.md §3 step 7: pending tests count
//!   only after the reviewer promotes them).
//!
//! **Honesty rule.** A case whose stage the compiler does not run yet is a **failure**
//! (scorecard.json: "a test that cannot run because a stage is missing counts as failed"),
//! never a vacuous pass: without a type checker every well-typed program is "accepted", so
//! acceptance proves nothing. Stage presence is probed through the same `kalam check` route:
//! name resolution must report K0401 for an undefined name, the type checker K0602 for
//! `1 == 1.0` (spec §9.8), and the interpreter must run a prelude-only `print` program.

use crate::clauses::{self, Unit};
use crate::cli::{self, KalamCli};
use crate::parity::{DiagError, ParseHook};
use crate::suites::{self, judge_file, RejectRules, SuiteCase, SuiteExpect};
use kalam_base::Skin;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const CONFORMANCE_DIR: &str = "eval/conformance";
pub const PENDING: &str = "pending";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Lex,
    Parse,
    Resolve,
    Typeck,
    Interp,
}

impl Stage {
    pub fn parse(s: &str) -> Option<Stage> {
        Some(match s {
            "lex" => Stage::Lex,
            "parse" => Stage::Parse,
            "resolve" => Stage::Resolve,
            "typeck" => Stage::Typeck,
            "interp" => Stage::Interp,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Lex => "lex",
            Stage::Parse => "parse",
            Stage::Resolve => "resolve",
            Stage::Typeck => "typeck",
            Stage::Interp => "interp",
        }
    }

    /// The default stage of a clause (see the module docs).
    pub fn for_clause(clause: &str) -> Stage {
        let mut parts = clause.trim_start_matches('§').split('.');
        let sec: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let sub: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        match (sec, sub) {
            (4..=8, _) => Stage::Parse,
            (10, 1) | (13, _) => Stage::Resolve,
            _ => Stage::Typeck,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfExpect {
    pub base: SuiteExpect,
    pub stdout: Option<String>,
    pub stage: Stage,
}

impl ConfExpect {
    pub fn parse(text: &str, clause: &str) -> Result<ConfExpect, String> {
        let base = SuiteExpect::parse(text)?;
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let stdout = match v.get("stdout") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => return Err("`stdout` must be a string".into()),
        };
        if stdout.is_some() && base.must_reject {
            return Err("`stdout` needs `must_reject: false`".into());
        }
        let stage = match v.get("stage") {
            None | Some(Value::Null) => {
                if stdout.is_some() {
                    Stage::Interp
                } else {
                    Stage::for_clause(clause)
                }
            }
            Some(Value::String(s)) => {
                Stage::parse(s).ok_or_else(|| format!("unknown `stage` {s:?}"))?
            }
            Some(_) => return Err("`stage` must be a string".into()),
        };
        if stdout.is_some() && stage != Stage::Interp {
            return Err("a case with `stdout` has stage `interp`".into());
        }
        Ok(ConfExpect {
            base,
            stdout,
            stage,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ConfCase {
    /// `§13.1.3`.
    pub clause: String,
    /// `13.1.3/glob-import` (relative to `eval/conformance`).
    pub id: String,
    pub case: SuiteCase,
    pub expect: Result<ConfExpect, String>,
}

/// Loads every case under `dir`, skipping `pending/`. `Ok(None)` when `dir` does not exist.
pub fn load(dir: &Path) -> Result<Option<Vec<ConfCase>>, String> {
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for clause_dir in entries {
        let name = clause_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if name == PENDING {
            continue;
        }
        let clause = clauses::normalize_id(&name);
        for case in suites::load(&clause_dir)?.unwrap_or_default() {
            let text = std::fs::read_to_string(clause_dir.join(&case.id).join("expect.json"));
            let expect = match (&case.expect, text) {
                (Err(e), _) => Err(e.clone()),
                (Ok(_), Err(e)) => Err(e.to_string()),
                (Ok(_), Ok(t)) => ConfExpect::parse(&t, &clause),
            };
            out.push(ConfCase {
                id: format!("{name}/{}", case.id),
                clause: clause.clone(),
                case,
                expect,
            });
        }
    }
    Ok(Some(out))
}

/// Which stages `kalam check` (and `kalam run --interp`) run today.
#[derive(Clone, Debug)]
pub struct Stages {
    pub parse: Result<(), String>,
    pub resolve: Result<(), String>,
    pub typeck: Result<(), String>,
    pub interp: Result<(), String>,
}

impl Stages {
    pub fn get(&self, s: Stage) -> &Result<(), String> {
        match s {
            Stage::Lex | Stage::Parse => &self.parse,
            Stage::Resolve => &self.resolve,
            Stage::Typeck => &self.typeck,
            Stage::Interp => &self.interp,
        }
    }

    pub fn to_json(&self) -> Value {
        let s = |r: &Result<(), String>| match r {
            Ok(()) => json!("available"),
            Err(e) => json!(format!("missing: {e}")),
        };
        json!({"parse": s(&self.parse), "resolve": s(&self.resolve), "typeck": s(&self.typeck), "interp": s(&self.interp)})
    }
}

fn reports(hook: &dyn ParseHook, src: &str, code: &str, what: &str) -> Result<(), String> {
    match hook.diagnose(src.as_bytes(), Skin::En) {
        Ok(r) if r.iter().any(|d| d.code == code) => Ok(()),
        Ok(r) => Err(format!(
            "`kalam check` does not run {what} yet: the probe gives {:?}, not {code}",
            r.iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
        )),
        Err(DiagError::Crash(c)) => Err(format!("the {what} probe crashed: {c}")),
        Err(DiagError::NotMeasurable(w)) => Err(w),
    }
}

/// Probes the stages through `hook` (and `cli` for the interpreter).
pub fn probe_stages(hook: &dyn ParseHook, cli: &Result<KalamCli, String>) -> Stages {
    Stages {
        parse: hook.probe(),
        resolve: reports(hook, cli::RESOLVE_PROBE, "K0401", "name resolution"),
        typeck: reports(hook, cli::TYPECK_PROBE, "K0602", "the type checker"),
        interp: match cli {
            Err(why) => Err(why.clone()),
            Ok(c) => c.interp_available(),
        },
    }
}

#[derive(Clone, Debug, Default)]
pub struct CaseOutcome {
    pub id: String,
    pub clause: String,
    pub must_reject: bool,
    pub stage: String,
    /// Skin pairs judged (always 3: en, ar, franco) and passed.
    pub pairs: usize,
    pub passed: usize,
    pub failures: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ConfRun {
    pub cases: Vec<CaseOutcome>,
    pub defects: Vec<String>,
    /// Case-skin pairs that failed only because their stage is missing, per stage.
    pub stage_missing: BTreeMap<String, usize>,
    pub route: String,
}

impl ConfRun {
    pub fn pairs(&self) -> (usize, usize) {
        self.cases
            .iter()
            .fold((0, 0), |(p, t), c| (p + c.passed, t + c.pairs))
    }

    /// `passed / pairs * 100`, or `Err(why)` when it cannot be judged.
    pub fn pass_rate(&self) -> Result<f64, String> {
        if !self.defects.is_empty() {
            return Err(format!(
                "{} defective case(s), so the suite cannot be judged: {}",
                self.defects.len(),
                self.defects
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
        let (p, t) = self.pairs();
        if t == 0 {
            return Err(format!("{CONFORMANCE_DIR} has no case"));
        }
        Ok(100.0 * p as f64 / t as f64)
    }

    pub fn evidence(&self) -> String {
        let (p, t) = self.pairs();
        let clauses: BTreeSet<&str> = self.cases.iter().map(|c| c.clause.as_str()).collect();
        let failing: Vec<String> = self
            .cases
            .iter()
            .filter(|c| c.passed < c.pairs)
            .take(3)
            .map(|c| {
                format!(
                    "{}: {}",
                    c.id,
                    c.failures.first().cloned().unwrap_or_default()
                )
            })
            .collect();
        format!(
            "{CONFORMANCE_DIR} via {}: {} cases over {} clauses, {p}/{t} case-skin pairs pass; pairs failed only because \
             their stage is missing (counted as failed): {:?}; defects {}{}",
            self.route,
            self.cases.len(),
            clauses.len(),
            self.stage_missing,
            self.defects.len(),
            if failing.is_empty() { String::new() } else { format!("; first failures: {}", failing.join(" | ")) }
        )
    }

    pub fn to_json(&self) -> Value {
        let (p, t) = self.pairs();
        json!({
            "pairs_passed": p, "pairs_total": t, "stage_missing": self.stage_missing,
            "defects": self.defects,
            "cases": self.cases.iter().map(|c| json!({
                "id": c.id, "clause": c.clause, "must_reject": c.must_reject, "stage": c.stage,
                "passed": c.passed, "pairs": c.pairs, "failures": c.failures,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Runs every case. `spec_units` (the index derived from the spec) validates clause ids.
pub fn run(
    hook: &dyn ParseHook,
    cli: &Result<KalamCli, String>,
    stages: &Stages,
    cases: &[ConfCase],
    spec_units: &[Unit],
) -> ConfRun {
    let known: BTreeSet<String> = spec_units
        .iter()
        .flat_map(|u| std::iter::once(u.id.clone()).chain(u.clauses.iter().map(|c| c.id.clone())))
        .collect();
    let mut r = ConfRun {
        route: hook.diag_route(),
        ..ConfRun::default()
    };
    for c in cases {
        let expect = match &c.expect {
            Err(e) => {
                r.defects.push(format!("{}: expect.json: {e}", c.id));
                continue;
            }
            Ok(e) => e,
        };
        if !known.contains(&c.clause) {
            r.defects.push(format!(
                "{}: {} is not a clause or unit of the spec index ({})",
                c.id,
                c.clause,
                clauses::CLAUSES_PATH
            ));
            continue;
        }
        if c.case.files.is_empty() {
            r.defects
                .push(format!("{}: no .kl/.ar.kl/.franco.kl file", c.id));
            continue;
        }
        if let Some(code) = expect
            .base
            .codes
            .iter()
            .find(|x| suites::RETIRED.contains(&x.as_str()))
        {
            r.defects
                .push(format!("{}: expects the retired code {code}", c.id));
            continue;
        }
        let mut o = CaseOutcome {
            id: c.id.clone(),
            clause: c.clause.clone(),
            must_reject: expect.base.must_reject,
            stage: expect.stage.as_str().into(),
            ..CaseOutcome::default()
        };
        for skin in Skin::ALL {
            o.pairs += 1;
            let files: Vec<_> = c.case.files.iter().filter(|f| f.skin == skin).collect();
            let verdict = match files.as_slice() {
                [] => Err(Failure::Wrong(format!("no {} file", skin.id()))),
                [f] => judge(hook, cli, stages, expect, f),
                _ => Err(Failure::Wrong(format!("more than one {} file", skin.id()))),
            };
            match verdict {
                Ok(()) => o.passed += 1,
                Err(Failure::StageMissing(why)) => {
                    *r.stage_missing
                        .entry(expect.stage.as_str().to_string())
                        .or_default() += 1;
                    o.failures.push(format!("{}: {why}", skin.id()));
                }
                Err(Failure::Wrong(why)) => o.failures.push(format!("{}: {why}", skin.id())),
            }
        }
        r.cases.push(o);
    }
    r
}

enum Failure {
    StageMissing(String),
    Wrong(String),
}

impl From<String> for Failure {
    fn from(s: String) -> Failure {
        Failure::Wrong(s)
    }
}

fn judge(
    hook: &dyn ParseHook,
    cli: &Result<KalamCli, String>,
    stages: &Stages,
    expect: &ConfExpect,
    file: &suites::SuiteFile,
) -> Result<(), Failure> {
    if let Err(why) = stages.get(expect.stage) {
        return Err(Failure::StageMissing(format!(
            "stage `{}` is missing, so the case cannot run ({why})",
            expect.stage.as_str()
        )));
    }
    let reported = match suites::diagnose(hook, file) {
        Ok(r) => r,
        Err(DiagError::Crash(c)) => return Err(Failure::Wrong(c)),
        Err(DiagError::NotMeasurable(w)) => return Err(Failure::Wrong(w)),
    };
    judge_file(&expect.base, &reported, &RejectRules::default())?;
    if let Some(want) = &expect.stdout {
        let Ok(cli) = cli else {
            return Err(Failure::StageMissing("no kalam binary".into()));
        };
        let run = cli
            .run_interp(file.skin, &file.bytes, None)
            .map_err(Failure::Wrong)?;
        if run.exit != Some(0) {
            return Err(Failure::Wrong(format!(
                "`kalam run --interp` exited {:?}",
                run.exit
            )));
        }
        if &run.stdout != want {
            return Err(Failure::Wrong(format!(
                "stdout {:?}, expected {want:?}",
                run.stdout
            )));
        }
    }
    Ok(())
}

/// `D1.spec_clause_coverage`: a unit is covered when its cases (filed under the unit or any
/// of its clauses, defects excluded) include >= 1 must-reject and >= 1 accepted case.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    pub units: usize,
    pub covered: Vec<String>,
    /// Units with cases of one polarity only.
    pub one_sided: Vec<String>,
}

impl Coverage {
    pub fn pct(&self) -> f64 {
        if self.units == 0 {
            0.0
        } else {
            100.0 * self.covered.len() as f64 / self.units as f64
        }
    }
}

pub fn coverage(units: &[Unit], run: &ConfRun) -> Coverage {
    let mut polarity: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    for c in &run.cases {
        let e = polarity.entry(clauses::unit_of(&c.clause)).or_default();
        if c.must_reject {
            e.0 = true;
        } else {
            e.1 = true;
        }
    }
    let mut cov = Coverage {
        units: units.len(),
        ..Coverage::default()
    };
    for u in units {
        match polarity.get(&u.id) {
            Some((true, true)) => cov.covered.push(u.id.clone()),
            Some(_) => cov.one_sided.push(u.id.clone()),
            None => {}
        }
    }
    cov
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::{FmtOut, Parsed, Reported};
    use crate::suites::tests::{temp_dir, write_case};

    /// Reports `ERR(CODE)` markers as diagnostics on their line; the resolver and type
    /// checker "exist" only when switched on (they answer their probes).
    struct Fake {
        resolve: bool,
        typeck: bool,
    }

    impl ParseHook for Fake {
        fn name(&self) -> String {
            "fake".into()
        }
        fn probe(&self) -> Result<(), String> {
            Ok(())
        }
        fn probe_format(&self) -> Result<(), String> {
            Ok(())
        }
        fn parse(&self, text: &str, _: Skin) -> Result<Parsed, String> {
            let mut errors = Vec::new();
            for (i, l) in text.lines().enumerate() {
                if let Some(rest) = l.split("ERR(").nth(1) {
                    errors.push(Reported::new(rest.split(')').next().unwrap(), i + 1));
                }
                if self.resolve && l.contains("kalam_probe_undefined_name") {
                    errors.push(Reported::new("K0401", i + 1));
                }
                if self.typeck && l.contains("1 == 1.0") {
                    errors.push(Reported::new("K0602", i + 1));
                }
            }
            Ok(Parsed {
                errors,
                kcf: vec![],
            })
        }
        fn convert(&self, t: &str, _: Skin, _: Skin) -> Result<String, String> {
            Ok(t.into())
        }
        fn format(&self, t: &str, _: Skin) -> Result<FmtOut, String> {
            Ok(FmtOut {
                text: t.into(),
                layout_kept: false,
            })
        }
    }

    const SPEC: &str = "## 9. T\n\n### 9.8 Eq\n\n- one\n- two\n\n## 13. M\n\n### 13.1 Mods\n\n- a\n\n### 13.2 Vis\n\n- b\n";

    fn three(dir: &Path, id: &str, expect: &str, body: &str) {
        write_case(
            dir,
            id,
            expect,
            &[("c.kl", body), ("c.ar.kl", body), ("c.franco.kl", body)],
        );
    }

    fn fixture() -> std::path::PathBuf {
        let d = temp_dir("conf");
        three(
            &d,
            "13.1.1/neg",
            r#"{"must_reject": true, "codes": ["K1203"], "lines": [1]}"#,
            "use x.* ERR(K1203)\n",
        );
        three(&d, "13.1.1/pos", r#"{"must_reject": false}"#, "use x.{y}\n");
        three(
            &d,
            "§9.8.2/neg",
            r#"{"must_reject": true, "codes": ["K0602"]}"#,
            "x\nERR(K0602)\n",
        );
        // only one polarity: §13.2 is not covered
        three(&d, "13.2/pos", r#"{"must_reject": false}"#, "ok\n");
        // pending cases never count
        three(
            &d,
            "pending/13.2.1/neg",
            r#"{"must_reject": true, "codes": ["K0303"]}"#,
            "ERR(K0303)\n",
        );
        d
    }

    fn run_with(hook: &Fake, d: &Path) -> (ConfRun, Coverage) {
        let units = clauses::index(SPEC);
        let cases = load(d).unwrap().unwrap();
        let cli = Err("no kalam binary".to_string());
        let stages = probe_stages(hook, &cli);
        let r = run(hook, &cli, &stages, &cases, &units);
        let cov = coverage(&units, &r);
        (r, cov)
    }

    #[test]
    fn missing_stages_fail_every_case_never_pass_vacuously() {
        let d = fixture();
        let (r, cov) = run_with(
            &Fake {
                resolve: false,
                typeck: false,
            },
            &d,
        );
        assert_eq!(r.cases.len(), 4, "pending/ is skipped");
        assert_eq!(r.pairs(), (0, 12), "{:?}", r.cases);
        assert_eq!(r.stage_missing.get("resolve"), Some(&9));
        assert_eq!(r.stage_missing.get("typeck"), Some(&3));
        assert_eq!(r.pass_rate().unwrap(), 0.0);
        // coverage counts tests that exist, not tests that pass
        assert_eq!(cov.covered, vec!["§13.1".to_string()]);
        assert_eq!(cov.one_sided, vec!["§9.8".to_string(), "§13.2".to_string()]);
        assert_eq!(cov.units, 3);
    }

    #[test]
    fn with_the_stages_the_cases_are_judged() {
        let d = fixture();
        let (r, _) = run_with(
            &Fake {
                resolve: true,
                typeck: true,
            },
            &d,
        );
        assert_eq!(r.pairs(), (12, 12), "{:?}", r.cases);
        assert!(r.stage_missing.is_empty());
        // a wrong line fails
        let d2 = temp_dir("conf-line");
        three(
            &d2,
            "13.1.1/neg",
            r#"{"must_reject": true, "codes": ["K1203"], "lines": [2]}"#,
            "use x.* ERR(K1203)\n",
        );
        let (r2, _) = run_with(
            &Fake {
                resolve: true,
                typeck: true,
            },
            &d2,
        );
        assert_eq!(r2.pairs(), (0, 3));
        assert!(
            r2.cases[0].failures[0].contains("line"),
            "{:?}",
            r2.cases[0]
        );
    }

    #[test]
    fn a_missing_skin_file_is_a_failed_pair_and_bad_cases_are_defects() {
        let d = temp_dir("conf-defects");
        write_case(
            &d,
            "13.1.1/en-only",
            r#"{"must_reject": false}"#,
            &[("c.kl", "ok\n")],
        );
        three(&d, "99.1/unknown", r#"{"must_reject": false}"#, "ok\n");
        three(&d, "13.2.1/bad", r#"{"must_reject": true}"#, "ok\n");
        three(
            &d,
            "13.2.1/stdout-reject",
            r#"{"must_reject": true, "codes": ["K1"], "stdout": "x"}"#,
            "ok\n",
        );
        let (r, _) = run_with(
            &Fake {
                resolve: true,
                typeck: true,
            },
            &d,
        );
        assert_eq!(r.pairs(), (1, 3));
        assert_eq!(r.defects.len(), 3, "{:?}", r.defects);
        assert!(r.pass_rate().is_err());
    }

    #[test]
    fn stdout_cases_need_the_interpreter() {
        let d = temp_dir("conf-interp");
        three(
            &d,
            "9.8.1/prints",
            r#"{"must_reject": false, "stdout": "true\n"}"#,
            "ok\n",
        );
        let (r, _) = run_with(
            &Fake {
                resolve: true,
                typeck: true,
            },
            &d,
        );
        assert_eq!(r.pairs(), (0, 3));
        assert_eq!(r.stage_missing.get("interp"), Some(&3));
        assert_eq!(r.cases[0].stage, "interp");
    }

    #[test]
    fn default_stages_follow_the_section() {
        assert_eq!(Stage::for_clause("§5.3.2"), Stage::Parse);
        assert_eq!(Stage::for_clause("§10.1.2"), Stage::Resolve);
        assert_eq!(Stage::for_clause("§10.6.1"), Stage::Typeck);
        assert_eq!(Stage::for_clause("§13.1"), Stage::Resolve);
        assert_eq!(Stage::for_clause("§9.4.1"), Stage::Typeck);
        let e =
            ConfExpect::parse(r#"{"must_reject": false, "stage": "parse"}"#, "§13.1.1").unwrap();
        assert_eq!(e.stage, Stage::Parse);
        assert!(ConfExpect::parse(r#"{"must_reject": false, "stage": "x"}"#, "§13.1").is_err());
    }
}
