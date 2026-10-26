//! FOUNDER-QUEUE Q33 and Q40: a module constant that would panic is K1204 (spec §13.4),
//! and the checker's verdict must agree with the reference interpreter. Each case is a
//! list of `let`s, checked once as module constants (`kalam check` reports K1204) and run
//! once inside `main` (`kalam run --interp` panics): the K1204 points at the operation the
//! interpreter panics on, and its label gives the interpreter's panic message. The cases
//! are the M2 iteration-4 verifier's n04–n06 and their neighbours (records, tuples, `.with`,
//! `Decimal`, `BigInt`).

use kalam_driver::{check_source, run_interp, RunOutcome};

/// The text from 1-based `line`:`col` (in characters) to the end of that line.
fn rest_of_line(src: &str, line: u32, col: u32) -> String {
    let text = src.lines().nth(line as usize - 1).unwrap_or("");
    text.chars().skip(col as usize - 1).collect()
}

/// `decls` (types), then `lets` (one `let` per line) as module constants: one K1204 whose
/// span is the operation `at` and whose label is "this panics: `why`". The same `lets`
/// inside `main` panic with `why` at that operation, before anything is printed.
fn agree(decls: &str, lets: &[&str], at: &str, why: &str) {
    let last = lets
        .last()
        .and_then(|l| l.strip_prefix("let "))
        .and_then(|l| l.split([' ', ':']).next())
        .expect("a `let`");
    let constants = format!(
        "{decls}{}\n\nfn main():\n    print(\"{{{last}}}\")\n",
        lets.join("\n")
    );
    let body: Vec<String> = lets.iter().map(|l| format!("    {l}")).collect();
    let in_main = format!(
        "{decls}fn main():\n{}\n    print(\"{{{last}}}\")\n",
        body.join("\n")
    );
    let checked = check_source("main.kl", &constants).unwrap();
    let codes: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["K1204"], "{constants}");
    let d = &checked.diagnostics[0];
    assert_eq!(
        &constants[d.span.start as usize..d.span.end as usize],
        at,
        "{constants}"
    );
    let label = format!("this panics: {why}");
    assert!(
        d.labels.iter().any(|l| l.text == label),
        "{:?}\n{constants}",
        d.labels
    );
    let mut out = Vec::new();
    match run_interp("main.kl", in_main.as_bytes(), &mut out).unwrap() {
        RunOutcome::Panicked {
            message,
            at: Some((line, col)),
        } => {
            assert_eq!(message, why, "{in_main}");
            let rest = rest_of_line(&in_main, line, col);
            assert!(rest.starts_with(at), "panicked at `{rest}`\n{in_main}");
        }
        other => panic!(
            "expected a panic, got exit {}: {}\n{in_main}",
            other.exit_code(),
            other.stderr_text("main.kl")
        ),
    }
    assert!(out.is_empty(), "{in_main}");
}

const PT: &str = "type Pt:\n    x: Int\n    y: Int\n\n";
const OVERFLOW: &str = "integer overflow";
const DECIMAL: &str = "Decimal overflow (more than 34 digits)";

#[test]
fn n04_through_a_record_field() {
    agree(
        PT,
        &[
            "let P = Pt(x: 9223372036854775807, y: 0)",
            "let Q = P.x + 1",
        ],
        "P.x + 1",
        OVERFLOW,
    );
}

#[test]
fn n05_through_a_tuple_field_and_with() {
    agree(
        PT,
        &[
            "let T = (9223372036854775807, \"a\")",
            "let P = Pt(x: 1, y: 2)",
            "let P2 = P.with(x: T.0)",
            "let S = \"{P2.x * 2}\"",
        ],
        "P2.x * 2",
        OVERFLOW,
    );
    agree(
        "",
        &[
            "let T = ((1, 9223372036854775807), 2)",
            "let U = T.0.1 + T.1",
        ],
        "T.0.1 + T.1",
        OVERFLOW,
    );
}

#[test]
fn n06_decimal_multiplication_beyond_34_digits() {
    agree(
        "",
        &[
            "let D1: Decimal = 9999999999999999999999999999999999.0",
            "let D2: Decimal = D1 * D1 * D1",
        ],
        "D1 * D1",
        DECIMAL,
    );
}

#[test]
fn decimal_sums_and_the_exponent_range() {
    let nines = "let A: Decimal = 9999999999999999999999999999999999";
    agree("", &[nines, "let B = A + 2"], "A + 2", DECIMAL);
    agree("", &[nines, "let B = -A - 2"], "-A - 2", DECIMAL);
    agree(
        "",
        &["let E: Decimal = 1e6111", "let F = E * E"],
        "E * E",
        DECIMAL,
    );
    agree(
        "",
        &["let T: Decimal = 1e-6176", "let U = T * 0.1"],
        "T * 0.1",
        DECIMAL,
    );
}

#[test]
fn bigint_division_by_a_zero_reached_through_large_values() {
    agree(
        "",
        &[
            "let B: BigInt = 340282366920938463463374607431768211455",
            "let Z = B * B - B * B",
            "let Q: BigInt = 10 // Z",
        ],
        "10 // Z",
        "division by zero",
    );
}
