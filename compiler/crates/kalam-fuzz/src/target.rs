//! The fuzz target: one input through lexer → parser → KCF → `kalam check` (name resolution
//! and the type checker) → `kalam fmt` → `kalam convert` → `kalam run --interp`, exactly as
//! the CLI runs them (through `kalam-driver`), on the calling thread's stack (the interpreter
//! on its own thread, under a time budget).
//!
//! A panic propagates to the caller (the worker catches it). Besides crashes, cheap
//! **oracles** are checked; a violation is reported, not panicked, because it is a wrong
//! answer rather than a crash and does not reset the crash-free clock:
//! - `cst_lossless`: the CST reproduces the input byte for byte;
//! - `fmt_output_rejected`: `fmt` output does not lex and parse cleanly in its own skin
//!   (FOUNDER-QUEUE Q16: backticks around another skin's undeclared reserved word are kept);
//! - `fmt_idempotence`: `fmt(fmt(x)) == fmt(x)` when `fmt` normalized the layout;
//! - `convert_output_rejected`: converting a clean file (to any skin, its own included) gives
//!   a file that does not lex and parse cleanly in the target skin (spec §4.3 G1, Q16);
//! - `convert_roundtrip`: `convert(convert(x, X→Y), Y→X) == convert(x, X→X)` (spec §4.3 G2);
//! - `typeck_nondeterministic` (target `typeck`): checking a file that lexes and parses
//!   cleanly twice gives different diagnostics (`kalam check --json` output);
//! - `typeck_fmt_unstable`, `typeck_convert_unstable` (target `typeck`): the type checker's
//!   verdict (any error or none, name resolution included) on a file that lexes and parses
//!   cleanly changes after `kalam fmt`, or after `kalam convert` to another skin (spec §4.3:
//!   a skin is a spelling, never a meaning);
//! - `interp_internal` (target `interp`): a file `kalam check` accepts ends `kalam run
//!   --interp` with an internal error (exit 70: a value of the wrong shape, an ill-formed
//!   KIR). A well-typed program must not go wrong on the reference interpreter (ROADMAP M2).
//!
//! The interpreter has no fuel, so a mutated program can loop forever. It runs on its own
//! thread with [`INTERP_BUDGET`]; past it the input counts as fine, and the worker retires
//! (the supervisor starts a fresh one) so the spinning thread does not outlive it. A budget
//! expiry is never a crash.

use std::io::{self, Write};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::time::Duration;

use kalam_base::{FileId, Skin};
use kalam_driver::{CheckedFile, FmtConfig, RunOutcome};
use kalam_syntax::kcf;

/// What one execution exercises, as listed in `status.json` `targets`. The M2 scope of
/// `D1.fuzz_crash_free_hours` needs a type-checker target there (`typeck`).
pub const TARGETS: &[&str] = &[
    "lexer", "parser", "kcf", "fmt", "convert", "typeck", "interp",
];

/// Wall-clock budget of one `kalam run --interp` (well under the supervisor's hang timeout).
pub const INTERP_BUDGET: Duration = Duration::from_millis(2000);

/// Output a fuzzed program may print before its writes fail (a printing infinite loop then
/// ends quickly, as `RunOutcome::Io`).
const INTERP_OUTPUT_CAP: usize = 1 << 20;

/// What one execution observed.
#[derive(Debug, Default)]
pub struct Observed {
    /// Coarse behaviour features (diagnostic codes, stages reached), used as a cheap
    /// coverage stand-in: an input that shows a new feature is kept in the corpus.
    pub features: Vec<u32>,
    /// `Some((oracle, detail))` if an oracle failed.
    pub oracle: Option<(&'static str, String)>,
    /// A computation outlived its budget on a thread that cannot be stopped: the worker must
    /// exit after replying.
    pub retire: bool,
}

/// Runs `f` on its own thread and waits at most `budget`: `None` if it did not finish (the
/// thread keeps running; retire the process). A panic in `f` is resumed on the caller
/// without running the panic hook again, so the hook's record of where it happened stands.
pub fn with_budget<T: Send + 'static>(
    budget: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("kalam-fuzz-budget".into())
        .spawn(move || {
            let _ = tx.send(catch_unwind(AssertUnwindSafe(f)));
        });
    if spawned.is_err() {
        return None;
    }
    match rx.recv_timeout(budget) {
        Ok(Ok(v)) => Some(v),
        Ok(Err(payload)) => resume_unwind(payload),
        Err(_) => None,
    }
}

/// A `Write` that accepts [`INTERP_OUTPUT_CAP`] bytes and then fails.
struct CappedSink(usize);

impl Write for CappedSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len();
        if self.0 > INTERP_OUTPUT_CAP {
            return Err(io::Error::other("fuzz output cap"));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// `kalam run --interp` on `text` as the file `path`, under `budget` (`None`: over budget).
pub fn interp_budgeted(path: &str, text: &str, budget: Duration) -> Option<RunOutcome> {
    let (path, bytes) = (path.to_string(), text.as_bytes().to_vec());
    with_budget(budget, move || {
        kalam_driver::run_interp(&path, &bytes, &mut CappedSink(0)).ok()
    })
    .flatten()
}

/// The `interp_internal` oracle: only an internal error (exit 70) is a wrong answer on a
/// file `kalam check` accepted; a panic, `Err` from `main`, an unsupported construct or a
/// failed write is the program's own business.
pub fn interp_violation(outcome: &RunOutcome) -> Option<String> {
    match outcome {
        RunOutcome::Internal(why) => Some(why.clone()),
        _ => None,
    }
}

fn outcome_kind(o: &RunOutcome) -> &'static [u8] {
    match o {
        RunOutcome::Rejected(_) => b"rejected",
        RunOutcome::Unsupported(_) => b"unsupported",
        RunOutcome::Done => b"done",
        RunOutcome::MainErr(_) => b"main-err",
        RunOutcome::Panicked { .. } => b"panicked",
        RunOutcome::Internal(_) => b"internal",
        RunOutcome::Io(_) => b"io",
    }
}

/// The type checker's verdict on a file (`kalam check` reports an error) and its error
/// codes, for the detail of a stability violation.
fn verdict(c: &CheckedFile) -> (bool, Vec<&'static str>) {
    let mut codes: Vec<&'static str> = c
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code)
        .collect();
    codes.sort_unstable();
    codes.dedup();
    (!codes.is_empty(), codes)
}

/// `kalam check` on `text` as a `skin` file: its verdict, or `None` if it cannot be checked.
fn verdict_of(text: &str, skin: Skin) -> Option<(bool, Vec<&'static str>)> {
    kalam_driver::check_source(&path_for(skin), text)
        .ok()
        .map(|c| verdict(&c))
}

/// FNV-1a, for feature ids.
pub fn fnv(parts: &[&[u8]]) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for p in parts {
        for b in *p {
            h ^= u32::from(*b);
            h = h.wrapping_mul(0x0100_0193);
        }
        h ^= 0xFF;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn path_for(skin: Skin) -> String {
    format!("fuzz{}", skin.extension())
}

/// Runs the whole pipeline on `text` in `skin`.
pub fn exercise(text: &str, skin: Skin) -> Observed {
    let mut obs = Observed::default();
    let skin_id = skin.id().as_bytes();
    let file = FileId(0);

    // Lexer and parser.
    let lexed = kalam_lexer::lex(text, skin, file);
    let parsed = kalam_parser::parse(text, &lexed, skin, file);
    let mut codes: Vec<&str> = lexed
        .diagnostics
        .iter()
        .chain(parsed.diagnostics.iter())
        .map(|d| d.code)
        .collect();
    let clean = !lexed
        .diagnostics
        .iter()
        .chain(parsed.diagnostics.iter())
        .any(|d| d.is_error());
    for pair in codes.windows(2) {
        obs.features
            .push(fnv(&[b"pair", pair[0].as_bytes(), pair[1].as_bytes()]));
    }
    codes.sort_unstable();
    codes.dedup();
    for c in &codes {
        obs.features.push(fnv(&[b"code", skin_id, c.as_bytes()]));
    }
    let tokens = lexed.tokens.len();
    obs.features.push(fnv(&[
        b"tokens",
        &[(usize::BITS - tokens.leading_zeros()) as u8],
    ]));

    let back = parsed.cst.text();
    if back != text {
        obs.oracle = Some((
            "cst_lossless",
            format!("CST text has {} bytes, input {}", back.len(), text.len()),
        ));
    }

    // KCF (skin inferred and explicit) and its hash.
    let inferred = kcf::serialize(&parsed.cst);
    let explicit = kcf::serialize_in(&parsed.cst, skin);
    let _ = kcf::KcfHash::of(&explicit).to_hex();
    let _ = kcf::hash(&parsed.cst);
    if inferred == explicit {
        obs.features.push(fnv(&[b"kcf-infer-agrees", skin_id]));
    }

    // `kalam check`: lexer and parser, then name resolution and the type checker (target
    // `typeck`) when they are clean. Its codes are coverage features, so inputs that reach
    // new type-checker diagnostics stay in the corpus.
    let path = path_for(skin);
    let mut typed: Option<(bool, Vec<&'static str>)> = None;
    if let Ok(checked) = kalam_driver::check_source(&path, text) {
        let json = checked.render_json();
        let mut all: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
        all.sort_unstable();
        all.dedup();
        for c in &all {
            obs.features.push(fnv(&[b"check", skin_id, c.as_bytes()]));
        }
        if clean {
            let v = verdict(&checked);
            obs.features
                .push(fnv(&[b"typeck", skin_id, &[u8::from(v.0)]]));
            if obs.oracle.is_none() {
                if let Ok(again) = kalam_driver::check_source(&path, text) {
                    if again.render_json() != json {
                        obs.oracle = Some((
                            "typeck_nondeterministic",
                            format!("two checks of one {skin} file give different diagnostics"),
                        ));
                    }
                }
            }
            typed = Some(v);
        }
    }

    // `kalam fmt`.
    let cfg = FmtConfig::default();
    match kalam_driver::format_source(&path, text, &cfg) {
        Ok(f) => {
            obs.features.push(fnv(&[
                b"fmt",
                skin_id,
                &[u8::from(f.layout_kept.is_some())],
            ]));
            if obs.oracle.is_none() {
                if let Some(codes) = errors_in(&f.text, skin) {
                    obs.oracle = Some((
                        "fmt_output_rejected",
                        format!("fmt output of a {skin} file has errors {codes:?}"),
                    ));
                }
            }
            if f.layout_kept.is_none() && obs.oracle.is_none() {
                if let Ok(f2) = kalam_driver::format_source(&path, &f.text, &cfg) {
                    if f2.text != f.text {
                        obs.oracle = Some(("fmt_idempotence", "fmt(fmt(x)) != fmt(x)".into()));
                    }
                }
            }
            if let (Some(before), None) = (&typed, &obs.oracle) {
                if f.text != text {
                    if let Some(after) = verdict_of(&f.text, skin) {
                        if after.0 != before.0 {
                            obs.oracle = Some((
                                "typeck_fmt_unstable",
                                format!(
                                    "kalam check of a {skin} file reports errors {:?}, of its fmt output {:?}",
                                    before.1, after.1
                                ),
                            ));
                        }
                    }
                }
            }
        }
        Err(_) => obs.features.push(fnv(&[b"fmt-refused", skin_id])),
    }

    // `kalam convert` to every other skin, and back.
    if clean {
        let canon = kalam_fmt::convert(text, skin, skin, &cfg).ok();
        if let (Some(c), None) = (&canon, &obs.oracle) {
            if let Some(codes) = errors_in(c, skin) {
                obs.oracle = Some((
                    "convert_output_rejected",
                    format!("the {skin} conversion of a clean {skin} file has errors {codes:?}"),
                ));
            }
        }
        for to in Skin::ALL {
            if to == skin {
                continue;
            }
            let Ok(c) = kalam_driver::convert_source(&path, text, to, &cfg) else {
                obs.features
                    .push(fnv(&[b"convert-refused", skin_id, to.id().as_bytes()]));
                continue;
            };
            obs.features
                .push(fnv(&[b"convert", skin_id, to.id().as_bytes()]));
            if obs.oracle.is_some() || canon.is_none() {
                continue;
            }
            if let Some(codes) = errors_in(&c.text, to) {
                obs.oracle = Some((
                    "convert_output_rejected",
                    format!("the {to} conversion of a clean {skin} file has errors {codes:?}"),
                ));
                continue;
            }
            if let Some(before) = &typed {
                if let Some(after) = verdict_of(&c.text, to) {
                    if after.0 != before.0 {
                        obs.oracle = Some((
                            "typeck_convert_unstable",
                            format!(
                                "kalam check of a {skin} file reports errors {:?}, of its {to} conversion {:?}",
                                before.1, after.1
                            ),
                        ));
                        continue;
                    }
                }
            }
            match kalam_fmt::convert(&c.text, to, skin, &cfg) {
                Err(d) => {
                    let codes: Vec<&str> = d.iter().map(|d| d.code).collect();
                    obs.oracle = Some((
                        "convert_output_rejected",
                        format!("the {to} conversion of a clean {skin} file has errors {codes:?}"),
                    ));
                }
                Ok(back) if Some(&back) != canon.as_ref() => {
                    obs.oracle = Some((
                        "convert_roundtrip",
                        format!("{skin}->{to}->{skin} is not byte-identical to {skin}->{skin}"),
                    ));
                }
                Ok(_) => {}
            }
        }
    }

    // `kalam run --interp` (target `interp`) on a file `kalam check` accepts.
    if typed.as_ref().is_some_and(|v| !v.0) {
        match interp_budgeted(&path, text, INTERP_BUDGET) {
            None => {
                obs.features.push(fnv(&[b"interp-budget", skin_id]));
                obs.retire = true;
            }
            Some(outcome) => {
                obs.features
                    .push(fnv(&[b"interp", skin_id, outcome_kind(&outcome)]));
                if let (Some(why), None) = (interp_violation(&outcome), &obs.oracle) {
                    obs.oracle = Some((
                        "interp_internal",
                        format!("kalam run --interp on a {skin} file kalam check accepts exits 70: {why}"),
                    ));
                }
            }
        }
    }
    obs
}

/// The error codes the lexer and parser report on `text` in `skin`, if any.
fn errors_in(text: &str, skin: Skin) -> Option<Vec<&'static str>> {
    let file = FileId(0);
    let lexed = kalam_lexer::lex(text, skin, file);
    let parsed = kalam_parser::parse(text, &lexed, skin, file);
    let codes: Vec<&'static str> = lexed
        .diagnostics
        .iter()
        .chain(parsed.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| d.code)
        .collect();
    (!codes.is_empty()).then_some(codes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_program_reaches_fmt_and_convert() {
        let o = exercise("fn main():\n    print(\"hi\")\n", Skin::En);
        assert!(o.oracle.is_none(), "{:?}", o.oracle);
        assert!(o.features.contains(&fnv(&[b"fmt", b"en", &[0]])));
        assert!(o.features.contains(&fnv(&[b"convert", b"en", b"ar"])));
    }

    /// One minimized input per root cause of the iteration-5 oracle violations (49 in
    /// `status.json`; 3 saved): each now runs through every oracle cleanly.
    #[test]
    fn oracle_violations_of_iteration_5_stay_fixed() {
        let cases: &[(Skin, &str)] = &[
            // Q16: another skin's undeclared reserved word lost its backticks in `convert`
            // (`Some`, `None` → K0201 in ar/franco) and in `fmt` (`اطبع` → K0201 in en).
            (
                Skin::En,
                "fn main(x: Set[`Some`]):\n    return if x: 1 else: `None`\n",
            ),
            (
                Skin::En,
                "fn main():\n    f(`\u{627}\u{637}\u{628}\u{639}`..x)\n",
            ),
            (Skin::Franco, "dala ra2isi():\n    f(`if`, `Some`)\n"),
            (Skin::Ar, "دالة رئيسي():\n    f(`law`، `print`)\n"),
        ];
        for (skin, text) in cases {
            let o = exercise(text, *skin);
            assert!(o.oracle.is_none(), "{skin}: {:?}\n{text}", o.oracle);
            assert!(
                o.features
                    .contains(&fnv(&[b"fmt", skin.id().as_bytes(), &[0]])),
                "{skin}: fmt must format it without the layout fallback\n{text}"
            );
        }
        // The other two causes were accepted inputs that should have been rejected. A raw name
        // that does not lex bare (mixed scripts, a haraka, an Extended Arabic-Indic digit) lost
        // its backticks (SEC0105, SEC0106 + K0250, SEC0124), and a bad number run in a hole
        // (`۹age`) was re-spelled `9age` (K0127). §5.3 rules 3-5 now hold inside backticks and
        // holes are checked as code, so both are rejected up front and no oracle runs on them.
        let rejected: &[(Skin, &str)] = &[
            (
                Skin::Franco,
                "dala ra2isi(`wal\u{6BE}a`: adad):\n    f(`w\u{651}alla`: `els\u{6F0}e`)\n",
            ),
            (Skin::En, "fn main():\n    print(\"{\u{6F9}age}\")\n"),
        ];
        for (skin, text) in rejected {
            let o = exercise(text, *skin);
            assert!(o.oracle.is_none(), "{skin}: {:?}\n{text}", o.oracle);
            assert!(
                !o.features
                    .contains(&fnv(&[b"fmt", skin.id().as_bytes(), &[0]])),
                "{skin}: the source must be rejected before fmt\n{text}"
            );
        }
    }

    #[test]
    fn the_targets_include_the_type_checker_and_the_interpreter() {
        assert!(TARGETS.contains(&"typeck") && TARGETS.contains(&"interp"));
        assert!(TARGETS.contains(&"lexer") && TARGETS.contains(&"convert"));
    }

    #[test]
    fn a_well_typed_program_is_type_checked_and_interpreted() {
        for (skin, text) in [
            (Skin::En, "fn main():\n    print(\"hi\")\n"),
            (Skin::Ar, "دالة رئيسي():\n    اطبع(\"hi\")\n"),
        ] {
            let o = exercise(text, skin);
            let id = skin.id().as_bytes();
            assert!(o.oracle.is_none(), "{:?}", o.oracle);
            assert!(!o.retire);
            assert!(o.features.contains(&fnv(&[b"typeck", id, &[0]])), "{skin}");
            assert!(
                o.features.contains(&fnv(&[b"interp", id, b"done"])),
                "{skin}"
            );
        }
    }

    #[test]
    fn a_type_error_is_a_feature_and_is_not_interpreted() {
        let o = exercise(
            "fn main():\n    let same = 1 == 1.0\n    print(\"{same}\")\n",
            Skin::En,
        );
        assert!(o.oracle.is_none(), "{:?}", o.oracle);
        assert!(o.features.contains(&fnv(&[b"check", b"en", b"K0602"])));
        assert!(o.features.contains(&fnv(&[b"typeck", b"en", &[1]])));
        assert!(!o.features.iter().any(|f| [b"done".as_slice(), b"rejected"]
            .iter()
            .any(|k| *f == fnv(&[b"interp", b"en", k]))));
    }

    #[test]
    fn a_runtime_panic_is_not_an_oracle_violation() {
        let o = exercise("fn main():\n    panic(\"boom\")\n", Skin::En);
        assert!(o.oracle.is_none(), "{:?}", o.oracle);
        assert!(o.features.contains(&fnv(&[b"interp", b"en", b"panicked"])));
    }

    #[test]
    fn only_an_internal_error_is_an_interpreter_violation() {
        assert_eq!(
            interp_violation(&RunOutcome::Internal("ill-formed KIR: x".into())).as_deref(),
            Some("ill-formed KIR: x")
        );
        assert!(interp_violation(&RunOutcome::Done).is_none());
        assert!(interp_violation(&RunOutcome::Unsupported("x".into())).is_none());
        assert!(interp_violation(&RunOutcome::Panicked {
            message: "x".into(),
            at: None
        })
        .is_none());
    }

    #[test]
    fn a_budget_stops_waiting_and_a_panic_reaches_the_caller() {
        assert_eq!(with_budget(Duration::from_secs(5), || 7), Some(7));
        let spin = with_budget(Duration::from_millis(50), || loop {
            std::thread::sleep(Duration::from_millis(10));
        });
        assert!(spin.is_none());
        let caught = std::panic::catch_unwind(|| {
            with_budget(Duration::from_secs(5), || -> u8 { panic!("inner") })
        });
        assert!(caught.is_err());
    }

    #[test]
    fn a_silent_infinite_loop_is_over_budget_not_a_hang() {
        let text = "fn main():\n    var i = 0\n    while true:\n        i = i + 0\n";
        assert!(kalam_driver::check_source("x.kl", text)
            .map(|c| !c.has_errors())
            .unwrap_or(false));
        assert!(interp_budgeted("x.kl", text, Duration::from_millis(200)).is_none());
    }

    #[test]
    fn garbage_is_survived() {
        let o = exercise("\u{202E}((((\"{\u{0}", Skin::Ar);
        assert!(o.oracle.is_none(), "{:?}", o.oracle);
        assert!(!o.features.is_empty());
    }
}
