//! Pluggable measurement hooks for the parser-dependent checks: G-PAR P1 (canonical
//! identity: equal KCF across en/ar/franco), G-PAR P2 (pairwise and 3-hop round-trip,
//! KCF-equal) and `D1.corpus_syntax_pass` (a task's every case parses, formats and
//! round-trips in all three skins).
//!
//! The harness talks to the parser only through [`ParseHook`]. The real hook
//! ([`KalamParserHook`], cargo feature `parser-hook`, on by default) calls
//! `kalam_parser::parse_source` and compares a skin-free canonical form: the KCF bytes from
//! `kalam_syntax::kcf::serialize` when that serializer works, otherwise the AST shape with
//! trivia from `kalam_ast::shape` (spec §4.3: conversions of each other have byte-identical
//! shapes). The form in use is named in every evidence string ([`ParseHook::name`]). The
//! parser is probed once on an empty file: while it is a stub (`todo!()`), the probe panics,
//! the panic is caught, and every parser-dependent result is `Unavailable`. The CST text and
//! the token-level converter round-trip are never used as the canonical form, and a crash on
//! a corpus case after a successful probe is a failure, never a skip.

use kalam_base::Skin;
use std::collections::BTreeMap;

/// One parsed file: error-diagnostic codes (lex and parse) and its canonical skin-free
/// bytes (KCF, or the AST shape while KCF is a stub; see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub errors: Vec<String>,
    pub kcf: Vec<u8>,
}

impl Parsed {
    fn accepted(&self) -> bool {
        self.errors.is_empty()
    }
}

pub trait ParseHook {
    /// A short description for evidence strings.
    fn name(&self) -> String;
    /// `Ok` when parse + KCF are implemented; `Err(reason)` otherwise.
    fn probe(&self) -> Result<(), String>;
    /// `Ok` when the formatter is implemented; `Err(reason)` otherwise.
    fn probe_format(&self) -> Result<(), String>;
    /// Lex + parse + KCF of one file. `Err` is a crash.
    fn parse(&self, text: &str, skin: Skin) -> Result<Parsed, String>;
    /// The skin printer (X → Y source text).
    fn convert(&self, text: &str, from: Skin, to: Skin) -> Result<String, String>;
    /// `kalam fmt`.
    fn format(&self, text: &str, skin: Skin) -> Result<String, String>;
}

/// One corpus case as the parity runner sees it.
#[derive(Clone, Debug, Default)]
pub struct ParityCase {
    pub task: String,
    pub case: String,
    pub sources: BTreeMap<Skin, String>,
    /// `expected.txt` lists lex-stage diagnostics: the case is meant to be rejected before
    /// parsing (T50 main), so P1–P3 do not apply (G-PAR scope rule).
    pub lex_stage_rejection: bool,
    /// For such a case: the same expected rejection happened in all three skins.
    pub lex_rejection_matched: bool,
}

/// A parser-dependent result.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Unavailable(String),
    /// `passed` of `total` (cases for P1, round-trip checks for P2).
    Measured {
        passed: usize,
        total: usize,
        failures: Vec<String>,
    },
}

impl Outcome {
    pub fn pct(&self) -> Option<f64> {
        match self {
            Outcome::Measured { passed, total, .. } if *total > 0 => {
                Some(*passed as f64 * 100.0 / *total as f64)
            }
            _ => None,
        }
    }
}

/// `D1.corpus_syntax_pass` per task.
#[derive(Clone, Debug, PartialEq)]
pub enum SyntaxOutcome {
    Unavailable(String),
    Measured {
        /// Tasks whose every case passed parse, format and round-trip in all three skins.
        passed: Vec<String>,
        /// `(task, reason)`: a case failed a part that was checked.
        failed: Vec<(String, String)>,
        /// Tasks that passed every checked part while the formatter is unavailable
        /// (`Some(reason)`): these are neither passes nor fails.
        undecided: Vec<String>,
        format_unavailable: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParityReport {
    pub p1: Outcome,
    pub p2: Outcome,
    pub syntax: SyntaxOutcome,
}

#[derive(Default)]
struct TaskState {
    failure: Option<String>,
    undecided: bool,
}

impl TaskState {
    fn fail(&mut self, why: String) {
        self.failure.get_or_insert(why);
    }
}

#[derive(Default)]
struct Tally {
    passed: usize,
    total: usize,
    failures: Vec<String>,
}

impl Tally {
    fn check(&mut self, ok: bool, what: impl FnOnce() -> String) -> Option<String> {
        self.total += 1;
        if ok {
            self.passed += 1;
            None
        } else {
            let w = what();
            self.failures.push(w.clone());
            Some(w)
        }
    }

    fn outcome(self, empty: &str) -> Outcome {
        if self.total == 0 {
            Outcome::Unavailable(empty.to_string())
        } else {
            Outcome::Measured {
                passed: self.passed,
                total: self.total,
                failures: self.failures,
            }
        }
    }
}

const CYCLES: [[Skin; 4]; 2] = [
    [Skin::En, Skin::Ar, Skin::Franco, Skin::En],
    [Skin::En, Skin::Franco, Skin::Ar, Skin::En],
];

/// Runs P1, P2 and the per-task syntax check over `cases` through `hook`.
pub fn measure(hook: &dyn ParseHook, cases: &[ParityCase]) -> ParityReport {
    if let Err(why) = hook.probe() {
        let msg = format!("{}: {why}", hook.name());
        return ParityReport {
            p1: Outcome::Unavailable(msg.clone()),
            p2: Outcome::Unavailable(msg.clone()),
            syntax: SyntaxOutcome::Unavailable(msg),
        };
    }
    let format_unavailable = hook
        .probe_format()
        .err()
        .map(|w| format!("{}: {w}", hook.name()));
    let mut p1 = Tally::default();
    let mut p2 = Tally::default();
    let mut tasks: BTreeMap<String, TaskState> = BTreeMap::new();

    for c in cases {
        let id = format!("{}/{}", c.task, c.case);
        let task = tasks.entry(c.task.clone()).or_default();
        if c.lex_stage_rejection {
            if !c.lex_rejection_matched {
                task.fail(format!(
                    "{id}: the expected lex-stage rejection did not happen identically in all three skins"
                ));
            }
            continue;
        }
        let parsed: BTreeMap<Skin, Result<Parsed, String>> = c
            .sources
            .iter()
            .map(|(s, t)| (*s, hook.parse(t, *s)))
            .collect();
        let missing: Vec<&str> = Skin::ALL
            .iter()
            .filter(|s| !c.sources.contains_key(s))
            .map(|s| s.id())
            .collect();
        let crashes: Vec<String> = parsed
            .iter()
            .filter_map(|(s, r)| r.as_ref().err().map(|e| format!("{s}: crash: {e}")))
            .collect();
        let accepted: BTreeMap<Skin, &Parsed> = parsed
            .iter()
            .filter_map(|(s, r)| r.as_ref().ok().filter(|p| p.accepted()).map(|p| (*s, p)))
            .collect();
        let errors = || -> String {
            parsed
                .iter()
                .filter_map(|(s, r)| r.as_ref().ok().map(|p| format!("{s}: {:?}", p.errors)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if accepted.is_empty() && crashes.is_empty() && missing.is_empty() {
            // G-PAR scope rule: rejected in all three skins is unsupported (it costs D1).
            task.fail(format!("{id}: rejected in all three skins ({})", errors()));
            continue;
        }
        let p1_problem = if !missing.is_empty() {
            Some(format!("{id}: missing skin file(s) {missing:?}"))
        } else if !crashes.is_empty() {
            Some(format!("{id}: {}", crashes.join("; ")))
        } else if accepted.len() < Skin::ALL.len() {
            Some(format!(
                "{id}: accepted in {} of 3 skins ({})",
                accepted.len(),
                errors()
            ))
        } else {
            let first = &accepted[&Skin::En].kcf;
            (!accepted.values().all(|p| &p.kcf == first))
                .then(|| format!("{id}: KCF differs across skins"))
        };
        if let Some(problem) = p1.check(p1_problem.is_none(), || p1_problem.clone().unwrap()) {
            task.fail(problem);
            continue;
        }

        // P2: every ordered pair and both 3-hop cycles must give KCF equal to the source's.
        let kcf = |s: Skin| &accepted[&s].kcf;
        let reparse = |text: &str, skin: Skin, want: &Vec<u8>| {
            hook.parse(text, skin)
                .map(|p| p.accepted() && &p.kcf == want)
                .unwrap_or(false)
        };
        for x in Skin::ALL {
            for y in Skin::ALL.into_iter().filter(|y| *y != x) {
                let ok = hook
                    .convert(&c.sources[&x], x, y)
                    .map(|out| reparse(&out, y, kcf(x)))
                    .unwrap_or(false);
                if let Some(w) = p2.check(ok, || format!("{id}: {x}->{y} is not KCF-equal to {x}"))
                {
                    task.fail(w);
                }
            }
        }
        for path in CYCLES {
            let mut cur = Ok(c.sources[&path[0]].clone());
            for w in path.windows(2) {
                cur = cur.and_then(|t| hook.convert(&t, w[0], w[1]));
            }
            let ok = cur
                .map(|t| reparse(&t, path[3], kcf(path[0])))
                .unwrap_or(false);
            if let Some(w) = p2.check(ok, || {
                format!(
                    "{id}: 3-hop {}->{}->{}->{} is not KCF-equal",
                    path[0], path[1], path[2], path[3]
                )
            }) {
                task.fail(w);
            }
        }

        // Formatting: fmt(fmt(x)) == fmt(x) and kcf(fmt(x)) == kcf(x), every skin.
        if format_unavailable.is_some() {
            task.undecided = true;
            continue;
        }
        for (&s, src) in &c.sources {
            let ok = hook.format(src, s).is_ok_and(|f1| {
                hook.format(&f1, s).is_ok_and(|f2| f2 == f1) && reparse(&f1, s, kcf(s))
            });
            if !ok {
                task.fail(format!(
                    "{id}: {s}: fmt is not idempotent or changes the KCF"
                ));
            }
        }
    }

    let (mut passed, mut failed, mut undecided) = (Vec::new(), Vec::new(), Vec::new());
    for (name, st) in tasks {
        match (st.failure, st.undecided) {
            (Some(why), _) => failed.push((name, why)),
            (None, true) => undecided.push(name),
            (None, false) => passed.push(name),
        }
    }
    ParityReport {
        p1: p1.outcome("no corpus case is accepted in any skin, so P1 has no case in scope"),
        p2: p2.outcome("no corpus case is accepted in all three skins, so P2 has no case in scope"),
        syntax: SyntaxOutcome::Measured {
            passed,
            failed,
            undecided,
            format_unavailable,
        },
    }
}

/// The task numbers named by a tier check scope such as `"T01-T15, T31-T33, T48-T50; dev"`.
pub fn scope_tasks(scope: &str) -> Vec<u32> {
    let num = |t: &str| t.strip_prefix('T').and_then(|n| n.parse::<u32>().ok());
    let mut out = Vec::new();
    for tok in scope.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let range = match tok.split_once('-') {
            Some((a, b)) => num(a).zip(num(b)),
            None => num(tok).map(|n| (n, n)),
        };
        if let Some((a, b)) = range {
            out.extend(a..=b);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// `t07-name` → 7.
pub fn task_number(task_dir: &str) -> Option<u32> {
    let rest = task_dir.strip_prefix('t')?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The hook this build uses: [`KalamParserHook`] with the `parser-hook` feature, otherwise
/// [`NoParserHook`].
pub fn default_hook() -> Box<dyn ParseHook> {
    #[cfg(feature = "parser-hook")]
    {
        Box::new(KalamParserHook::new())
    }
    #[cfg(not(feature = "parser-hook"))]
    {
        Box::new(NoParserHook)
    }
}

/// A build without the parser: every parser-dependent result is unavailable.
pub struct NoParserHook;

impl ParseHook for NoParserHook {
    fn name(&self) -> String {
        "kalam-eval built without the `parser-hook` feature".into()
    }
    fn probe(&self) -> Result<(), String> {
        Err("no parser linked".into())
    }
    fn probe_format(&self) -> Result<(), String> {
        Err("no parser linked".into())
    }
    fn parse(&self, _: &str, _: Skin) -> Result<Parsed, String> {
        Err("no parser linked".into())
    }
    fn convert(&self, _: &str, _: Skin, _: Skin) -> Result<String, String> {
        Err("no parser linked".into())
    }
    fn format(&self, _: &str, _: Skin) -> Result<String, String> {
        Err("no parser linked".into())
    }
}

#[cfg(feature = "parser-hook")]
/// Runs `f`, turning a panic (such as a stub's `todo!()`) into `Err(message)`.
fn catch<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|p| {
        p.downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into())
    })
}

#[cfg(feature = "parser-hook")]
fn codes(diags: &[kalam_diagnostics::Diagnostic]) -> String {
    diags
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// The real hook: `kalam_parser::parse_source`, canonical form KCF
/// (`kalam_syntax::kcf::serialize`) or, while that is a stub, the AST shape
/// (`kalam_ast::shape`); `kalam_fmt::convert` is the skin printer and `kalam_fmt::format`
/// the formatter.
#[cfg(feature = "parser-hook")]
pub struct KalamParserHook {
    /// `kalam_syntax::kcf::serialize` works (probed once at construction).
    kcf: bool,
    /// `kalam_parser::parse_source` on an empty file (probed once at construction).
    probe: Result<(), String>,
}

#[cfg(feature = "parser-hook")]
impl KalamParserHook {
    pub fn new() -> Self {
        match catch(|| kalam_parser::parse_source("", Skin::En, kalam_base::FileId(0))) {
            Err(e) => KalamParserHook {
                kcf: false,
                probe: Err(format!(
                    "panicked on an empty file ({e}); the parser is still a stub"
                )),
            },
            Ok(out) => KalamParserHook {
                kcf: catch(|| kalam_syntax::kcf::serialize(&out.cst)).is_ok(),
                probe: Ok(()),
            },
        }
    }
}

#[cfg(feature = "parser-hook")]
impl Default for KalamParserHook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "parser-hook")]
impl ParseHook for KalamParserHook {
    fn name(&self) -> String {
        if self.kcf {
            "kalam_parser::parse_source, canonical form = KCF (kalam_syntax::kcf::serialize)".into()
        } else {
            "kalam_parser::parse_source, canonical form = AST shape with trivia (kalam_ast::shape; \
             kalam_syntax::kcf::serialize is not available yet)"
                .into()
        }
    }

    fn probe(&self) -> Result<(), String> {
        self.probe.clone()
    }

    fn probe_format(&self) -> Result<(), String> {
        catch(|| kalam_fmt::format("", Skin::En, &kalam_fmt::FmtConfig::default()))
            .map(|_| ())
            .map_err(|e| format!("kalam_fmt::format panicked on an empty file ({e}); the formatter is still a stub"))
    }

    fn parse(&self, text: &str, skin: Skin) -> Result<Parsed, String> {
        catch(|| {
            let file = kalam_base::FileId(0);
            // `parse_source` returns lexer and parser diagnostics together.
            let out = kalam_parser::parse_source(text, skin, file);
            let kcf = if self.kcf {
                kalam_syntax::kcf::serialize(&out.cst)
            } else {
                kalam_ast::shape::shape(&out.ast, kalam_ast::shape::ShapeOptions::default())
                    .into_bytes()
            };
            let errors = out
                .diagnostics
                .iter()
                .filter(|d| d.is_error())
                .map(|d| d.code.to_string())
                .collect();
            Parsed { errors, kcf }
        })
    }

    fn convert(&self, text: &str, from: Skin, to: Skin) -> Result<String, String> {
        catch(|| kalam_fmt::convert(text, from, to, &kalam_fmt::FmtConfig::default()))?
            .map_err(|d| format!("convert failed: {}", codes(&d)))
    }

    fn format(&self, text: &str, skin: Skin) -> Result<String, String> {
        catch(|| kalam_fmt::format(text, skin, &kalam_fmt::FmtConfig::default()))?
            .map_err(|d| format!("fmt failed: {}", codes(&d)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_hook_never_reports_a_stub_as_measured() {
        // Whatever state the parser is in, a probe failure must leave everything unavailable,
        // and a probe success must come with a working parse of an empty file.
        let hook = default_hook();
        let r = measure(hook.as_ref(), &[]);
        match hook.probe() {
            Err(_) => {
                assert!(matches!(r.p1, Outcome::Unavailable(_)));
                assert!(matches!(r.syntax, SyntaxOutcome::Unavailable(_)));
            }
            Ok(()) => assert!(hook.parse("", Skin::En).is_ok()),
        }
        // with no cases, P1/P2 are never a vacuous 100%
        assert!(r.p1.pct().is_none());
        assert!(r.p2.pct().is_none());
    }

    /// A fake parser: a file is "accepted" unless it contains `!`; its KCF is the text with
    /// every skin keyword spelling (`EN_`, `AR_`, `FR_`) replaced by `K_`. The converter
    /// swaps the prefixes; the formatter trims trailing spaces.
    struct Fake {
        available: bool,
        format_available: bool,
        crash_on: Option<&'static str>,
        lossy_convert: bool,
    }

    impl Default for Fake {
        fn default() -> Self {
            Fake {
                available: true,
                format_available: true,
                crash_on: None,
                lossy_convert: false,
            }
        }
    }

    fn prefix(s: Skin) -> &'static str {
        match s {
            Skin::En => "EN_",
            Skin::Ar => "AR_",
            Skin::Franco => "FR_",
        }
    }

    impl ParseHook for Fake {
        fn name(&self) -> String {
            "fake".into()
        }
        fn probe(&self) -> Result<(), String> {
            if self.available {
                Ok(())
            } else {
                Err("parser is a stub".into())
            }
        }
        fn probe_format(&self) -> Result<(), String> {
            if self.format_available {
                Ok(())
            } else {
                Err("formatter is a stub".into())
            }
        }
        fn parse(&self, text: &str, skin: Skin) -> Result<Parsed, String> {
            if self.crash_on.is_some_and(|c| text.contains(c)) {
                return Err("panicked".into());
            }
            let errors = if text.contains('!') {
                vec!["K0201".to_string()]
            } else {
                vec![]
            };
            let kcf = text
                .replace(prefix(skin), "K_")
                .trim_end()
                .as_bytes()
                .to_vec();
            Ok(Parsed { errors, kcf })
        }
        fn convert(&self, text: &str, from: Skin, to: Skin) -> Result<String, String> {
            let out = text.replace(prefix(from), prefix(to));
            Ok(if self.lossy_convert && to == Skin::Ar {
                out.replace("# c", "")
            } else {
                out
            })
        }
        fn format(&self, text: &str, _skin: Skin) -> Result<String, String> {
            Ok(text
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n"))
        }
    }

    fn case(task: &str, en: &str) -> ParityCase {
        let mut sources = BTreeMap::new();
        for s in Skin::ALL {
            sources.insert(s, en.replace("EN_", prefix(s)));
        }
        ParityCase {
            task: task.into(),
            case: "main".into(),
            sources,
            ..Default::default()
        }
    }

    #[test]
    fn stub_parser_makes_everything_unavailable() {
        let hook = Fake {
            available: false,
            ..Default::default()
        };
        let r = measure(&hook, &[case("t01-a", "EN_fn x # c")]);
        assert!(matches!(r.p1, Outcome::Unavailable(ref w) if w.contains("stub")));
        assert!(matches!(r.p2, Outcome::Unavailable(_)));
        assert!(matches!(r.syntax, SyntaxOutcome::Unavailable(_)));
    }

    #[test]
    fn equal_kcf_across_skins_passes_p1_and_p2() {
        let r = measure(&Fake::default(), &[case("t01-a", "EN_fn x # c")]);
        assert_eq!(r.p1.pct(), Some(100.0));
        // 6 ordered pairs + 2 three-hop cycles
        assert!(
            matches!(
                r.p2,
                Outcome::Measured {
                    passed: 8,
                    total: 8,
                    ..
                }
            ),
            "{:?}",
            r.p2
        );
        match r.syntax {
            SyntaxOutcome::Measured { passed, failed, .. } => {
                assert_eq!(passed, vec!["t01-a".to_string()]);
                assert!(failed.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn differing_kcf_fails_p1() {
        let mut c = case("t01-a", "EN_fn x");
        c.sources.insert(Skin::Ar, "AR_fn y".into());
        let r = measure(&Fake::default(), &[c]);
        assert!(
            matches!(r.p1, Outcome::Measured { passed: 0, total: 1, ref failures } if failures[0].contains("KCF")),
            "{:?}",
            r.p1
        );
    }

    #[test]
    fn scope_rule_rejected_everywhere_is_excluded_partial_rejection_fails() {
        // rejected in all three skins: unsupported, not a parity failure (it costs D1)
        let all_bad = case("t02-b", "EN_fn !");
        let r = measure(
            &Fake::default(),
            &[case("t01-a", "EN_fn x"), all_bad.clone()],
        );
        assert!(
            matches!(
                r.p1,
                Outcome::Measured {
                    passed: 1,
                    total: 1,
                    ..
                }
            ),
            "{:?}",
            r.p1
        );
        match &r.syntax {
            SyntaxOutcome::Measured { failed, .. } => {
                assert_eq!(failed.len(), 1);
                assert_eq!(failed[0].0, "t02-b");
            }
            other => panic!("{other:?}"),
        }
        // only all-rejected cases: nothing in P1 scope, so unavailable, never 100%
        let r = measure(&Fake::default(), &[all_bad]);
        assert!(matches!(r.p1, Outcome::Unavailable(_)), "{:?}", r.p1);

        // accepted in some skins only: a parity failure
        let mut some = case("t03-c", "EN_fn x");
        some.sources.insert(Skin::Franco, "FR_fn !".into());
        let r = measure(&Fake::default(), &[some]);
        assert!(
            matches!(
                r.p1,
                Outcome::Measured {
                    passed: 0,
                    total: 1,
                    ..
                }
            ),
            "{:?}",
            r.p1
        );
    }

    #[test]
    fn lossy_conversion_fails_p2() {
        let hook = Fake {
            lossy_convert: true,
            ..Default::default()
        };
        let r = measure(&hook, &[case("t01-a", "EN_fn x # c")]);
        assert_eq!(r.p1.pct(), Some(100.0));
        match &r.p2 {
            Outcome::Measured {
                passed,
                total,
                failures,
            } => {
                assert!(passed < total);
                assert!(failures.iter().any(|f| f.contains("ar")), "{failures:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_crash_after_a_good_probe_is_a_failure() {
        let hook = Fake {
            crash_on: Some("boom"),
            ..Default::default()
        };
        let r = measure(&hook, &[case("t01-a", "EN_fn boom")]);
        assert!(
            matches!(
                r.p1,
                Outcome::Measured {
                    passed: 0,
                    total: 1,
                    ..
                }
            ),
            "{:?}",
            r.p1
        );
    }

    #[test]
    fn missing_formatter_leaves_parse_level_passes_undecided() {
        let hook = Fake {
            format_available: false,
            ..Default::default()
        };
        let r = measure(&hook, &[case("t01-a", "EN_fn x"), case("t02-b", "EN_fn !")]);
        match r.syntax {
            SyntaxOutcome::Measured {
                passed,
                failed,
                undecided,
                format_unavailable,
            } => {
                assert!(passed.is_empty(), "no pass without the formatter");
                assert_eq!(undecided, vec!["t01-a".to_string()]);
                assert_eq!(failed.len(), 1);
                assert!(format_unavailable.is_some());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lex_stage_rejection_cases_count_for_syntax_but_not_parity() {
        let mut t50 = case("t50-x", "EN_fn !");
        t50.lex_stage_rejection = true;
        t50.lex_rejection_matched = true;
        let r = measure(&Fake::default(), &[case("t01-a", "EN_fn x"), t50.clone()]);
        assert!(
            matches!(r.p1, Outcome::Measured { total: 1, .. }),
            "{:?}",
            r.p1
        );
        match &r.syntax {
            SyntaxOutcome::Measured { passed, .. } => {
                assert!(passed.contains(&"t50-x".to_string()))
            }
            other => panic!("{other:?}"),
        }
        t50.lex_rejection_matched = false;
        let r = measure(&Fake::default(), &[t50]);
        assert!(
            matches!(r.syntax, SyntaxOutcome::Measured { ref failed, .. } if failed.len() == 1)
        );
    }

    #[test]
    fn missing_skin_file_fails_the_case() {
        let mut c = case("t01-a", "EN_fn x");
        c.sources.remove(&Skin::Franco);
        let r = measure(&Fake::default(), &[c]);
        assert!(
            matches!(
                r.p1,
                Outcome::Measured {
                    passed: 0,
                    total: 1,
                    ..
                }
            ),
            "{:?}",
            r.p1
        );
    }

    #[test]
    fn task_scope_parsing() {
        let t = scope_tasks("T01-T15, T31-T33, T48-T50; dev + held-out; x3 skins");
        assert_eq!(t.len(), 21);
        assert!(t.contains(&1) && t.contains(&15) && t.contains(&32) && t.contains(&50));
        assert!(!t.contains(&16));
        assert_eq!(scope_tasks("T07"), vec![7]);
        assert!(scope_tasks("every corpus case").is_empty());
        assert_eq!(task_number("t01-hello"), Some(1));
        assert_eq!(task_number("t50-trojan-source"), Some(50));
        assert_eq!(task_number("INDEX.md"), None);
    }
}
