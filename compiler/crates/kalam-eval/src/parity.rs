//! Pluggable measurement hooks for the parser-dependent checks: G-PAR P1 (canonical
//! identity: equal KCF across en/ar/franco), G-PAR P2 (pairwise and 3-hop round-trip,
//! KCF-equal), G-PAR P3 (source fixpoint, byte-identical) and `D1.corpus_syntax_pass` (a
//! task's every case parses, formats and round-trips in all three skins).
//!
//! The harness talks to the compiler only through [`ParseHook`]. The real hook
//! ([`KalamParserHook`], cargo feature `parser-hook`, on by default) uses the same entry
//! points as the `kalam` CLI:
//! - parse: `kalam_parser::parse_source` (lexer and parser diagnostics together);
//! - canonical form: the KCF bytes of `kalam_syntax::kcf::serialize_in` (spec §4.4). KCF
//!   records comments and blank lines, so KCF equality also checks that they are kept;
//! - `kalam convert`: `kalam_driver::convert_source`;
//! - `kalam fmt`: `kalam_driver::format_source`.
//!
//! There is no fallback canonical form: if KCF or the parser panics on an empty file, the
//! probe fails and every parser-dependent result is `Unavailable`. A crash on a corpus case
//! after a successful probe is a failure, never a skip.

use kalam_base::Skin;
use std::collections::BTreeMap;

/// One error diagnostic as the compiler reported it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reported {
    pub code: String,
    /// 1-based line of the primary span's start.
    pub line: usize,
    /// The rendered English message.
    pub message_en: String,
    /// The source text under the primary span, or the whole line when the span is empty.
    pub context: String,
}

impl Reported {
    pub fn new(code: &str, line: usize) -> Reported {
        Reported {
            code: code.to_string(),
            line,
            ..Reported::default()
        }
    }
}

/// One parsed file: its error diagnostics (lex and parse, in the order the compiler reports
/// them) and its canonical skin-free bytes (KCF).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub errors: Vec<Reported>,
    pub kcf: Vec<u8>,
}

impl Parsed {
    pub fn accepted(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn codes(&self) -> Vec<&str> {
        self.errors.iter().map(|r| r.code.as_str()).collect()
    }
}

/// The output of `kalam fmt` for one file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FmtOut {
    pub text: String,
    /// The formatter re-spelled the file but kept its layout (`kalam_driver::Formatted`).
    pub layout_kept: bool,
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
    /// `kalam convert` (X → Y source text).
    fn convert(&self, text: &str, from: Skin, to: Skin) -> Result<String, String>;
    /// `kalam fmt`.
    fn format(&self, text: &str, skin: Skin) -> Result<FmtOut, String>;
    /// The error diagnostics `kalam check` reports for one file's raw bytes. The default
    /// checks UTF-8 text through [`ParseHook::parse`]; bytes that are not UTF-8 are
    /// [`DiagError::NotMeasurable`] (no byte entry point).
    fn diagnose(&self, bytes: &[u8], skin: Skin) -> Result<Vec<Reported>, DiagError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| DiagError::NotMeasurable(no_bytes_api(&self.name())))?;
        self.parse(text, skin)
            .map(|p| p.errors)
            .map_err(DiagError::Crash)
    }
    /// The entry points [`ParseHook::diagnose`] uses, for evidence strings.
    fn diag_route(&self) -> String {
        format!("ParseHook::parse ({})", self.name())
    }
}

/// Why [`ParseHook::diagnose`] has no diagnostics for a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagError {
    /// The compiler crashed: the file counts as a failure.
    Crash(String),
    /// No entry point can check this input (raw non-UTF-8 bytes without a byte API): the
    /// file is neither passed nor failed, and its suite cannot pass.
    NotMeasurable(String),
}

/// The not-measurable reason for a non-UTF-8 file when no byte entry point is linked.
pub fn no_bytes_api(via: &str) -> String {
    format!(
        "not valid UTF-8, and no byte entry point is linked (kalam_driver::check_bytes or \
         kalam_lexer::lex_bytes; build probe: {}), so nothing can report SEC0100 for it \
         through {via}",
        option_env!("KALAM_EVAL_BYTES_API").unwrap_or("not run")
    )
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
    /// P3 source fixpoint: `convert(convert(s, X, Y), Y, Y) == convert(s, X, Y)` byte for
    /// byte, for all 9 ordered (X, Y) pairs of every case in P1 scope.
    pub p3: Outcome,
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

/// Formats one accepted file and checks `fmt(fmt(x)) == fmt(x)` and
/// `kcf(fmt(x)) == kcf(x)`. `Ok(layout_kept)` on success.
pub fn fmt_check(hook: &dyn ParseHook, text: &str, skin: Skin, kcf: &[u8]) -> Result<bool, String> {
    let f1 = hook
        .format(text, skin)
        .map_err(|e| format!("fmt refused it: {e}"))?;
    let f2 = hook
        .format(&f1.text, skin)
        .map_err(|e| format!("fmt refused its own output: {e}"))?;
    if f2.text != f1.text {
        return Err("fmt(fmt(x)) != fmt(x)".into());
    }
    match hook.parse(&f1.text, skin) {
        Err(e) => Err(format!("fmt output crashed the parser: {e}")),
        Ok(p) if !p.accepted() => Err(format!("fmt output does not parse ({:?})", p.codes())),
        Ok(p) if p.kcf != kcf => Err("kcf(fmt(x)) != kcf(x)".into()),
        Ok(_) => Ok(f1.layout_kept),
    }
}

/// Runs P1, P2, P3 and the per-task syntax check over `cases` through `hook`.
pub fn measure(hook: &dyn ParseHook, cases: &[ParityCase]) -> ParityReport {
    if let Err(why) = hook.probe() {
        let msg = format!("{}: {why}", hook.name());
        return ParityReport {
            p1: Outcome::Unavailable(msg.clone()),
            p2: Outcome::Unavailable(msg.clone()),
            p3: Outcome::Unavailable(msg.clone()),
            syntax: SyntaxOutcome::Unavailable(msg),
        };
    }
    let format_unavailable = hook
        .probe_format()
        .err()
        .map(|w| format!("{}: {w}", hook.name()));
    let mut p1 = Tally::default();
    let mut p2 = Tally::default();
    let mut p3 = Tally::default();
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
                .filter_map(|(s, r)| r.as_ref().ok().map(|p| format!("{s}: {:?}", p.codes())))
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

        // P3: print_Y(parse(print_Y(parse_X(s)))) == print_Y(parse_X(s)), byte for byte.
        for x in Skin::ALL {
            for y in Skin::ALL {
                let ok = hook
                    .convert(&c.sources[&x], x, y)
                    .is_ok_and(|once| hook.convert(&once, y, y).is_ok_and(|twice| twice == once));
                if let Some(w) = p3.check(ok, || format!("{id}: {x}->{y} is not a fixpoint")) {
                    task.fail(w);
                }
            }
        }

        // Formatting: fmt(fmt(x)) == fmt(x) and kcf(fmt(x)) == kcf(x), every skin.
        if format_unavailable.is_some() {
            task.undecided = true;
            continue;
        }
        for (&s, src) in &c.sources {
            if let Err(why) = fmt_check(hook, src, s, kcf(s)) {
                task.fail(format!("{id}: {s}: {why}"));
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
        p3: p3.outcome("no corpus case is accepted in all three skins, so P3 has no case in scope"),
        syntax: SyntaxOutcome::Measured {
            passed,
            failed,
            undecided,
            format_unavailable,
        },
    }
}

// The task numbers named by a tier check scope such as `"T01-T15, T31-T33, T48-T50; dev"`.
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
    fn format(&self, _: &str, _: Skin) -> Result<FmtOut, String> {
        Err("no parser linked".into())
    }
}

/// Runs `f`, turning a panic (such as a stub's `todo!()`) into `Err(message)`.
pub fn catch<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|p| {
        p.downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into())
    })
}

/// The 1-based line of `offset` and the text under `start..end` (the whole line when the
/// span is empty).
pub fn locate(text: &str, start: u32, end: u32) -> (usize, String) {
    let index = kalam_base::LineIndex::new(text);
    let start = (start as usize).min(text.len()) as u32;
    let line = index.line_of(start);
    let under = text
        .get(start as usize..(end as usize).min(text.len()))
        .unwrap_or("");
    let context = if under.is_empty() {
        text.get(index.line_range(text, line)).unwrap_or("")
    } else {
        under
    };
    (line + 1, context.to_string())
}

/// The real hook: the same entry points as the `kalam` CLI (see the module docs).
#[cfg(feature = "parser-hook")]
pub struct KalamParserHook {
    /// `kalam_parser::parse_source` and KCF on an empty file (probed once at construction).
    probe: Result<(), String>,
    /// `Ok`: diagnostics go through `kalam_driver::check_source` (`kalam check`,
    /// FOUNDER-QUEUE Q17). `Err(why)`: the driver does not report parser diagnostics, so the
    /// parse hook is the fallback.
    driver: Result<(), String>,
}

/// A file with a parse error and no lexer error: `kalam check` must report it.
#[cfg(feature = "parser-hook")]
const PARSE_ERROR_PROBE: &str = "fn main(:\n";

/// `Ok` when `kalam_driver::check_source` reports every error code the parser reports for
/// [`PARSE_ERROR_PROBE`].
#[cfg(feature = "parser-hook")]
fn driver_runs_the_parser() -> Result<(), String> {
    let errors = |d: &[kalam_diagnostics::Diagnostic]| -> Vec<String> {
        d.iter()
            .filter(|d| d.is_error())
            .map(|d| d.code.to_string())
            .collect()
    };
    let parser = catch(|| {
        kalam_parser::parse_source(PARSE_ERROR_PROBE, Skin::En, kalam_base::FileId(0)).diagnostics
    })
    .map(|d| errors(&d))?;
    if parser.is_empty() {
        return Err("the parser reports no error for the parse-error probe".into());
    }
    let driver = catch(|| kalam_driver::check_source("main.kl", PARSE_ERROR_PROBE))?
        .map(|c| errors(&c.diagnostics))
        .map_err(|e| format!("not a Kalam path: {}", e.0))?;
    if parser.iter().all(|c| driver.contains(c)) {
        Ok(())
    } else {
        Err(format!(
            "kalam_driver::check_source reports {driver:?} where kalam_parser::parse_source \
             reports {parser:?}: `kalam check` does not run the parser (FOUNDER-QUEUE Q17)"
        ))
    }
}

/// Error diagnostics as [`Reported`], located in `text`.
#[cfg(feature = "parser-hook")]
fn reported(text: &str, diagnostics: &[kalam_diagnostics::Diagnostic]) -> Vec<Reported> {
    diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| {
            let (line, context) = locate(text, d.span.start, d.span.end);
            Reported {
                code: d.code.to_string(),
                line,
                message_en: d.message(Skin::En),
                context,
            }
        })
        .collect()
}

/// `kalam_driver::check_bytes` returns a `CheckedFile`, or a `Result` of one.
#[cfg(all(feature = "parser-hook", kalam_check_bytes))]
trait IntoChecked {
    fn into_checked(self) -> Result<kalam_driver::CheckedFile, String>;
}

#[cfg(all(feature = "parser-hook", kalam_check_bytes))]
impl IntoChecked for kalam_driver::CheckedFile {
    fn into_checked(self) -> Result<kalam_driver::CheckedFile, String> {
        Ok(self)
    }
}

#[cfg(all(feature = "parser-hook", kalam_check_bytes))]
impl IntoChecked for Result<kalam_driver::CheckedFile, kalam_driver::NotKalamFile> {
    fn into_checked(self) -> Result<kalam_driver::CheckedFile, String> {
        self.map_err(|e| format!("not a Kalam path: {}", e.0))
    }
}

#[cfg(feature = "parser-hook")]
impl KalamParserHook {
    pub fn new() -> Self {
        let probe = catch(|| {
            let out = kalam_parser::parse_source("", Skin::En, kalam_base::FileId(0));
            kalam_syntax::kcf::serialize_in(&out.cst, Skin::En)
        })
        .map(|_| ())
        .map_err(|e| format!("parse + KCF panicked on an empty file ({e})"));
        KalamParserHook {
            probe,
            driver: driver_runs_the_parser(),
        }
    }

    /// The entry point for bytes that are not UTF-8: `kalam_driver::check_bytes`, else
    /// `kalam_lexer::lex_bytes`, else none (detected at build time, see `build.rs`).
    pub fn bytes_route() -> Option<&'static str> {
        if cfg!(kalam_check_bytes) {
            Some("kalam_driver::check_bytes")
        } else if cfg!(kalam_lex_bytes) {
            Some("kalam_lexer::lex_bytes (lexer diagnostics only)")
        } else {
            None
        }
    }

    #[allow(unused_variables)]
    fn diagnose_bytes(bytes: &[u8], skin: Skin) -> Result<Vec<Reported>, DiagError> {
        #[cfg(kalam_check_bytes)]
        {
            let checked =
                catch(|| kalam_driver::check_bytes(&Self::path(skin), bytes).into_checked())
                    .map_err(DiagError::Crash)?
                    .map_err(DiagError::Crash)?;
            Ok(reported(&checked.text, &checked.diagnostics))
        }
        #[cfg(all(not(kalam_check_bytes), kalam_lex_bytes))]
        {
            // spans index the lossy text (each invalid sequence becomes U+FFFD)
            let out = catch(|| kalam_lexer::lex_bytes(bytes, skin)).map_err(DiagError::Crash)?;
            Ok(reported(&out.text, &out.lexed.diagnostics))
        }
        #[cfg(not(any(kalam_check_bytes, kalam_lex_bytes)))]
        {
            Err(DiagError::NotMeasurable(no_bytes_api(
                "kalam_driver::check_source",
            )))
        }
    }

    fn path(skin: Skin) -> String {
        format!("main{}", skin.extension())
    }
}

#[cfg(feature = "parser-hook")]
impl Default for KalamParserHook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "parser-hook")]
fn source_error(e: kalam_driver::SourceError) -> String {
    match e {
        kalam_driver::SourceError::NotKalam(p) => format!("not a Kalam path: {}", p.0),
        kalam_driver::SourceError::Diagnostics(c) => {
            let codes: Vec<&str> = c
                .diagnostics
                .iter()
                .filter(|d| d.is_error())
                .map(|d| d.code)
                .collect();
            format!("the source has errors {codes:?}")
        }
    }
}

#[cfg(feature = "parser-hook")]
impl ParseHook for KalamParserHook {
    fn name(&self) -> String {
        "kalam_parser::parse_source + KCF (kalam_syntax::kcf::serialize_in); kalam convert = \
         kalam_driver::convert_source; kalam fmt = kalam_driver::format_source"
            .into()
    }

    fn probe(&self) -> Result<(), String> {
        self.probe.clone()
    }

    fn probe_format(&self) -> Result<(), String> {
        let cfg = kalam_driver::FmtConfig::default();
        catch(|| kalam_driver::format_source("main.kl", "", &cfg))
            .map_err(|e| format!("kalam_driver::format_source panicked on an empty file ({e})"))?
            .map(|_| ())
            .map_err(source_error)
    }

    fn parse(&self, text: &str, skin: Skin) -> Result<Parsed, String> {
        catch(|| {
            // `parse_source` returns lexer and parser diagnostics together.
            let out = kalam_parser::parse_source(text, skin, kalam_base::FileId(0));
            Parsed {
                errors: reported(text, &out.diagnostics),
                kcf: kalam_syntax::kcf::serialize_in(&out.cst, skin),
            }
        })
    }

    fn diagnose(&self, bytes: &[u8], skin: Skin) -> Result<Vec<Reported>, DiagError> {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Self::diagnose_bytes(bytes, skin);
        };
        if self.driver.is_err() {
            return self
                .parse(text, skin)
                .map(|p| p.errors)
                .map_err(DiagError::Crash);
        }
        catch(|| kalam_driver::check_source(&Self::path(skin), text))
            .map_err(DiagError::Crash)?
            .map(|c| reported(text, &c.diagnostics))
            .map_err(|e| DiagError::Crash(format!("not a Kalam path: {}", e.0)))
    }

    fn diag_route(&self) -> String {
        let text = match &self.driver {
            Ok(()) => "kalam check (kalam_driver::check_source, FOUNDER-QUEUE Q17)".to_string(),
            Err(why) => format!(
                "FALLBACK to the harness parse hook (kalam_parser::parse_source), because {why}"
            ),
        };
        let bytes = Self::bytes_route()
            .map(str::to_string)
            .unwrap_or_else(|| "no byte entry point linked, so they are not measurable".into());
        format!("{text}; non-UTF-8 files: {bytes}")
    }

    fn convert(&self, text: &str, from: Skin, to: Skin) -> Result<String, String> {
        let cfg = kalam_driver::FmtConfig::default();
        catch(|| kalam_driver::convert_source(&Self::path(from), text, to, &cfg))?
            .map(|c| c.text)
            .map_err(|e| format!("kalam convert failed: {}", source_error(e)))
    }

    fn format(&self, text: &str, skin: Skin) -> Result<FmtOut, String> {
        let cfg = kalam_driver::FmtConfig::default();
        catch(|| kalam_driver::format_source(&Self::path(skin), text, &cfg))?
            .map(|f| FmtOut {
                text: f.text,
                layout_kept: f.layout_kept.is_some(),
            })
            .map_err(|e| format!("kalam fmt failed: {}", source_error(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The build script's signature detection (`build.rs`), compiled into the tests.
    #[allow(dead_code)]
    #[path = "../../../build.rs"]
    mod build_script;

    #[test]
    fn build_probe_accepts_only_the_called_signatures() {
        use build_script::{detect, signature};
        let checked = &["Result<CheckedFile,NotKalamFile>", "CheckedFile"][..];
        let src = "/// doc\npub fn check_bytes(\n    path: &str,\n    bytes: &[u8],\n) -> Result<CheckedFile, NotKalamFile> {\n";
        assert_eq!(
            signature(src, "check_bytes"),
            Some((
                vec!["&str".to_string(), "&[u8]".to_string()],
                "Result<CheckedFile,NotKalamFile>".to_string()
            ))
        );
        assert_eq!(
            detect(src, "check_bytes", &["&str", "&[u8]"], checked),
            Ok(true)
        );
        assert_eq!(
            detect(
                "pub fn check_source(p: &str) {}",
                "check_bytes",
                &["&str", "&[u8]"],
                checked
            ),
            Ok(false)
        );
        let other = "pub fn check_bytes(bytes: Vec<u8>) -> CheckedFile {";
        let why = detect(other, "check_bytes", &["&str", "&[u8]"], checked).unwrap_err();
        assert!(why.contains("Vec<u8>"), "{why}");
        let lex = "pub fn lex_bytes(bytes: &[u8], skin: Skin) -> BytesLexOutput {";
        let lex_sig = (&["&[u8]", "Skin"][..], &["BytesLexOutput"][..]);
        assert_eq!(detect(lex, "lex_bytes", lex_sig.0, lex_sig.1), Ok(true));
        let old = "pub fn lex_bytes(bytes: &[u8], skin: Skin, file: FileId) -> LexOutput {";
        assert!(detect(old, "lex_bytes", lex_sig.0, lex_sig.1).is_err());
        // the cfg this build was compiled with agrees with the lexer source it was built from
        let lexer_src = include_str!("../../kalam-lexer/src/lib.rs");
        assert_eq!(
            detect(lexer_src, "lex_bytes", lex_sig.0, lex_sig.1) == Ok(true),
            cfg!(kalam_lex_bytes)
        );
    }

    #[test]
    fn default_diagnose_parses_text_and_never_guesses_about_bytes() {
        let hook = NoParserHook;
        assert!(matches!(
            hook.diagnose(b"fn main():\n", Skin::En),
            Err(DiagError::Crash(_))
        ));
        match hook.diagnose(b"\xff", Skin::En) {
            Err(DiagError::NotMeasurable(why)) => {
                assert!(
                    why.contains("not valid UTF-8") && why.contains("check_bytes"),
                    "{why}"
                )
            }
            other => panic!("{other:?}"),
        }
        assert!(hook.diag_route().starts_with("ParseHook::parse"));
    }

    /// FOUNDER-QUEUE Q17: diagnostics come from `kalam check` (`kalam_driver::check_source`)
    /// whenever it runs the parser; the parse hook is used only as a named fallback.
    #[cfg(feature = "parser-hook")]
    #[test]
    fn diagnostics_go_through_kalam_check_when_it_parses() {
        let hook = KalamParserHook::new();
        let route = hook.diag_route();
        let codes = |r: Vec<Reported>| -> Vec<(String, usize)> {
            r.into_iter().map(|r| (r.code, r.line)).collect()
        };
        // a lexer error (leading-zero literal) and a parse error in one file
        let src = "fn main():\n    let x = 007\n    print((\n";
        let got = codes(hook.diagnose(src.as_bytes(), Skin::En).unwrap());
        let check = kalam_driver::check_source("main.kl", src).unwrap();
        let via_check: Vec<(String, usize)> = codes(reported(src, &check.diagnostics));
        let via_parse = codes(hook.parse(src, Skin::En).unwrap().errors);
        match driver_runs_the_parser() {
            Ok(()) => {
                assert!(
                    route.starts_with("kalam check (kalam_driver::check_source"),
                    "{route}"
                );
                assert_eq!(got, via_check);
            }
            Err(why) => {
                assert!(route.starts_with("FALLBACK"), "{route}");
                assert!(route.contains(&why), "{route}");
                assert_eq!(got, via_parse);
            }
        }
        assert!(!got.is_empty());
        // the probe itself: the parser reports the probe's error
        let p = kalam_parser::parse_source(PARSE_ERROR_PROBE, Skin::En, kalam_base::FileId(0));
        assert!(p.diagnostics.iter().any(|d| d.is_error()));
        assert!(route.contains("non-UTF-8 files:"), "{route}");
    }

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
                vec![Reported::new("K0201", 1)]
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
        fn format(&self, text: &str, _skin: Skin) -> Result<FmtOut, String> {
            Ok(FmtOut {
                text: text
                    .lines()
                    .map(str::trim_end)
                    .collect::<Vec<_>>()
                    .join("\n"),
                layout_kept: false,
            })
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
        // 9 ordered (X, Y) pairs, X == Y included
        assert!(
            matches!(
                r.p3,
                Outcome::Measured {
                    passed: 9,
                    total: 9,
                    ..
                }
            ),
            "{:?}",
            r.p3
        );
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

    /// A converter whose output changes on every pass: P3 (fixpoint) fails, and so does
    /// the task.
    struct Drifting;

    impl ParseHook for Drifting {
        fn name(&self) -> String {
            "drifting".into()
        }
        fn probe(&self) -> Result<(), String> {
            Ok(())
        }
        fn probe_format(&self) -> Result<(), String> {
            Ok(())
        }
        fn parse(&self, text: &str, _: Skin) -> Result<Parsed, String> {
            Ok(Parsed {
                errors: vec![],
                kcf: text.trim_end().as_bytes().to_vec(),
            })
        }
        fn convert(&self, text: &str, _: Skin, _: Skin) -> Result<String, String> {
            Ok(format!("{text} "))
        }
        fn format(&self, text: &str, _: Skin) -> Result<FmtOut, String> {
            Ok(FmtOut {
                text: text.to_string(),
                layout_kept: false,
            })
        }
    }

    #[test]
    fn a_converter_that_is_not_a_fixpoint_fails_p3() {
        let r = measure(&Drifting, &[case("t01-a", "x")]);
        // trailing spaces are not in this fake KCF, so P2 holds
        assert_eq!(r.p2.pct(), Some(100.0));
        assert_eq!(r.p3.pct(), Some(0.0), "{:?}", r.p3);
        assert!(
            matches!(r.syntax, SyntaxOutcome::Measured { ref failed, .. } if failed.len() == 1)
        );
    }

    #[test]
    fn fmt_check_requires_idempotence_and_kcf_identity() {
        let ok = fmt_check(&Fake::default(), "EN_fn x  ", Skin::En, b"K_fn x");
        assert_eq!(ok, Ok(false));
        let wrong_kcf = fmt_check(&Fake::default(), "EN_fn x", Skin::En, b"other");
        assert!(wrong_kcf.unwrap_err().contains("kcf"));
    }

    #[test]
    fn locate_gives_line_and_span_or_line_text() {
        let text = "a\nb\u{202E}c\nd\n";
        let start = text.find('\u{202E}').unwrap() as u32;
        let (line, ctx) = locate(text, start, start + 3);
        assert_eq!((line, ctx.as_str()), (2, "\u{202E}"));
        let (line, ctx) = locate(text, start, start);
        assert_eq!((line, ctx.as_str()), (2, "b\u{202E}c"));
        assert_eq!(locate("", 9, 9).0, 1);
    }
}
