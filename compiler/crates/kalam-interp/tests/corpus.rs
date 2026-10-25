//! Acceptance: the dev-corpus tasks the reference interpreter must run (roadmap
//! `M2.interpreter`: T01, T02, T05, T11–T14, T08 if it needs no `Float` formatting), plus the
//! other prelude-only dev tasks, produce their `expected.txt` stdout exactly in all three
//! skins. T05 and T13 are held out (sealed outside the repository) and are not run here.
//!
//! Each file is run the way the harness stages it: under its spec file name (`main.kl`,
//! `main.ar.kl`, `main.franco.kl`, DR-18) through `kalam_driver::run_interp`.

use std::path::PathBuf;

use kalam_driver::{run_interp, RunOutcome};

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../eval/corpus")
}

const SKINS: [(&str, &str); 3] = [
    ("en.kalam", "main.kl"),
    ("ar.kalam", "main.ar.kl"),
    ("franco.kalam", "main.franco.kl"),
];

/// The `--- stdout` section of an `expected.txt` (`kalam-expect/1`).
fn expected_stdout(task: &str) -> String {
    let text =
        std::fs::read_to_string(corpus().join(task).join("expected.txt")).expect("expected.txt");
    let (_, out) = text
        .split_once("--- stdout\n")
        .expect("a `--- stdout` section");
    out.to_string()
}

/// Runs one skin file of `task`: (exit code, stdout, stderr).
fn run_skin(task: &str, file: &str, staged: &str) -> (u8, String, String) {
    let bytes = std::fs::read(corpus().join(task).join(file)).expect("task file");
    let mut out = Vec::new();
    let outcome = run_interp(staged, &bytes, &mut out).expect("a Kalam path");
    let stderr = match &outcome {
        RunOutcome::Rejected(c) => c.render_human(kalam_base::Skin::En),
        other => other.stderr_text(staged),
    };
    (
        outcome.exit_code(),
        String::from_utf8(out).expect("UTF-8"),
        stderr,
    )
}

fn check_task(task: &str, want: &str) {
    for (file, staged) in SKINS {
        let (exit, out, err) = run_skin(task, file, staged);
        assert_eq!(exit, 0, "{task} {file}: {err}");
        assert_eq!(out, want, "{task} {file}");
    }
}

macro_rules! corpus_tasks {
    ($($name:ident => $task:literal,)*) => {$(
        #[test]
        fn $name() {
            check_task($task, &expected_stdout($task));
        }
    )*};
}

corpus_tasks! {
    hello => "t01-hello",
    fizzbuzz => "t02-fizzbuzz",
    number_stats => "t04-number-stats",
    word_frequency => "t06-word-frequency",
    palindrome_arabic => "t07-palindrome-arabic",
    temperature_table => "t08-temperature-table",
    user_record => "t11-user-record",
    shapes_match => "t12-shapes-match",
    pong_headless => "t45-pong-headless",
}

/// T14's `expected.txt` and `ref.py` predate DECISIONS D18: they model `Int` as
/// ±(2^53−1), so `"9007199254740992".to_int()` is a parse error there. Under D18 the text
/// is a valid `Int` (2^53 < 2^63), so `parse_age` rejects it as out of range instead. Every
/// other line must match exactly. When the expectation is regenerated (reviewer-gated, LOOP
/// A3), the stale line disappears and this test asks for its special case to be removed.
#[test]
fn result_try_matches_except_the_line_that_predates_d18() {
    let task = "t14-result-try";
    let stale = "\"9007199254740992\" -> not a number: 9007199254740992\n";
    let d18 = "\"9007199254740992\" -> out of range: 9007199254740992\n";
    let want = expected_stdout(task);
    assert!(
        want.contains(stale),
        "T14's expected.txt no longer has the pre-D18 line: use corpus_tasks! for it"
    );
    check_task(task, &want.replace(stale, d18));
}
