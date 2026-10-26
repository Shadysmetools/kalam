//! Differential tests: the type checker and the reference interpreter agree (M2 iteration 6).
//!
//! - **Constants** (FOUNDER-QUEUE Q33, Q40): a list of `let`s is checked once as module
//!   constants and run once inside `main`. Either the checker reports one K1204 at the
//!   operation the interpreter panics on, with the interpreter's panic message as its label,
//!   or neither complains and both print the same text. Covered here: structural `==`/`!=`
//!   on lists, maps, variants, `Option` and `Result` (b12/b13), the `pow` methods (Q45), and
//!   the two `Decimal` traps with their separate messages (Q48).
//! - **Literals** (Q48, Q52): a `Float` or `Decimal` literal is K0126 exactly when the
//!   interpreter cannot hold it, and every literal the checker accepts runs (never exit 70)
//!   with the value the interpreter's own `Decimal.parse` gives.

use kalam_driver::{check_source, run_interp, RunOutcome};
use kalam_interp::num::Decimal;

/// The text from 1-based `line`:`col` (in characters) to the end of that line.
fn rest_of_line(src: &str, line: u32, col: u32) -> String {
    let text = src.lines().nth(line as usize - 1).unwrap_or("");
    text.chars().skip(col as usize - 1).collect()
}

/// The name the last `let` of `lets` binds.
fn last_name(lets: &[&str]) -> String {
    lets.last()
        .and_then(|l| l.strip_prefix("let "))
        .and_then(|l| l.split([' ', ':']).next())
        .expect("a `let`")
        .to_string()
}

/// `decls`, then `lets` as module constants, and the same `lets` inside `main`; `main`
/// prints the last one.
fn programs(decls: &str, lets: &[&str]) -> (String, String) {
    let last = last_name(lets);
    let constants = format!(
        "{decls}{}\n\nfn main():\n    print(\"{{{last}}}\")\n",
        lets.join("\n")
    );
    let body: Vec<String> = lets.iter().map(|l| format!("    {l}")).collect();
    let in_main = format!(
        "{decls}fn main():\n{}\n    print(\"{{{last}}}\")\n",
        body.join("\n")
    );
    (constants, in_main)
}

fn run(path: &str, src: &str) -> (RunOutcome, String) {
    let mut out = Vec::new();
    let outcome = run_interp(path, src.as_bytes(), &mut out).unwrap();
    (outcome, String::from_utf8(out).unwrap())
}

/// The constants are one K1204 at `at`, labelled "this panics: `why`"; inside `main` the
/// same `lets` panic with `why` at `at`, before anything is printed.
fn panics(decls: &str, lets: &[&str], at: &str, why: &str) {
    let (constants, in_main) = programs(decls, lets);
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
    match run("main.kl", &in_main) {
        (
            RunOutcome::Panicked {
                message,
                at: Some((line, col)),
            },
            out,
        ) => {
            assert_eq!(message, why, "{in_main}");
            let rest = rest_of_line(&in_main, line, col);
            assert!(rest.starts_with(at), "panicked at `{rest}`\n{in_main}");
            assert!(out.is_empty(), "{in_main}");
        }
        (other, _) => panic!(
            "expected a panic, got exit {}: {}\n{in_main}",
            other.exit_code(),
            other.stderr_text("main.kl")
        ),
    }
}

/// The constants check clean, and both programs run and print `printed`.
fn agrees(decls: &str, lets: &[&str], printed: &str) {
    let (constants, in_main) = programs(decls, lets);
    let checked = check_source("main.kl", &constants).unwrap();
    let codes: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.is_empty(), "{codes:?}\n{constants}");
    for src in [&constants, &in_main] {
        match run("main.kl", src) {
            (RunOutcome::Done, out) => assert_eq!(out, format!("{printed}\n"), "{src}"),
            (other, _) => panic!(
                "expected a run, got exit {}: {}\n{src}",
                other.exit_code(),
                other.stderr_text("main.kl")
            ),
        }
    }
}

const SHAPE: &str = "enum Shape:\n    Dot\n    Circle(radius: Int)\n    Rect(w: Int, h: Int)\n\n";
const DIV: &str = "division by zero";
const MODULO: &str = "modulo by zero";
const OVERFLOW: &str = "integer overflow";
const DIGITS: &str = "Decimal overflow (more than 34 digits)";
const EXPONENT: &str = "Decimal exponent out of range";

// -- Q40: structural equality decides `and`/`or` ------------------------------------------

#[test]
fn b12_list_equality_decides_the_short_circuit() {
    panics(
        "",
        &["let L = [1, 2]", "let BOOM = L == [1, 2] and (1 // 0 == 0)"],
        "1 // 0",
        DIV,
    );
    agrees(
        "",
        &["let L = [1, 2]", "let OK = L == [1, 3] and (1 // 0 == 0)"],
        "false",
    );
    agrees(
        "",
        &["let L = [1, 2]", "let OK = L != [1] or (1 // 0 == 0)"],
        "true",
    );
    // `+` concatenates; nested lists and tuples compare element by element.
    panics(
        "",
        &[
            "let L = [[1], [2, 3]] + [[4]]",
            "let BOOM = L == [[1], [2, 3], [4]] and (7 % 0 == 1)",
        ],
        "7 % 0",
        MODULO,
    );
    // A known difference decides even when another element is not evaluated here.
    panics(
        "",
        &["let BOOM = [\"{1}\", \"a\"] == [\"{1}\", \"b\"] or (1 // 0 == 0)"],
        "1 // 0",
        DIV,
    );
}

#[test]
fn b13_option_result_and_enum_equality() {
    panics(
        "",
        &[
            "let O: Option[Int] = Some(3)",
            "let BOOM = O == Some(3) and (7 % 0 == 1)",
        ],
        "7 % 0",
        MODULO,
    );
    agrees(
        "",
        &[
            "let O: Option[Int] = None",
            "let OK = O == Some(3) and (7 % 0 == 1)",
        ],
        "false",
    );
    panics(
        "",
        &[
            "let R: Result[Int, String] = Err(\"no\")",
            "let BOOM = R != Ok(1) and (1 // 0 == 0)",
        ],
        "1 // 0",
        DIV,
    );
    // Variant payloads in declaration order, whatever order the labels are written in.
    panics(
        SHAPE,
        &[
            "let S = Shape.Rect(h: 2, w: 1)",
            "let BOOM = S == Shape.Rect(w: 1, h: 2) and (1 // 0 == 0)",
        ],
        "1 // 0",
        DIV,
    );
    agrees(
        SHAPE,
        &[
            "let S = Shape.Circle(radius: 1)",
            "let OK = S == Shape.Dot or [S] == [Shape.Circle(radius: 2)]",
        ],
        "false",
    );
    panics(
        SHAPE,
        &[
            "let S: Shape = .Dot",
            "let BOOM = S == Shape.Dot and (1 // 0 == 0)",
        ],
        "1 // 0",
        DIV,
    );
}

#[test]
fn map_equality_ignores_order_and_keeps_the_last_value() {
    panics(
        "",
        &[
            "let M = {\"a\": 1, \"b\": 2, \"a\": 3}",
            "let BOOM = M == {\"b\": 2, \"a\": 3} and (1 // 0 == 0)",
        ],
        "1 // 0",
        DIV,
    );
    agrees(
        "",
        &[
            "let M = {\"a\": 1, \"b\": 2}",
            "let OK = M == {\"a\": 1} or M == {\"a\": 2, \"b\": 2}",
        ],
        "false",
    );
    // `Decimal` keys compare by value: 1.5 and 1.50 are one key.
    panics(
        "",
        &[
            "let M: Map[Decimal, Int] = {1.5: 1, 1.50: 2}",
            "let N: Map[Decimal, Int] = {1.5: 2}",
            "let BOOM = M == N and (1 // 0 == 0)",
        ],
        "1 // 0",
        DIV,
    );
}

// -- Q45: `pow` in constants ------------------------------------------------------------------

#[test]
fn pow_in_constants_matches_the_interpreter() {
    agrees("", &["let X = (3).pow(4)"], "81");
    agrees("", &["let D: Decimal = 2.5", "let X = D.pow(3)"], "15.625");
    agrees("", &["let D: Decimal = 1.50", "let X = D.pow(2)"], "2.2500");
    agrees(
        "",
        &["let B: BigInt = 2", "let X = B.pow(100)"],
        "1267650600228229401496703205376",
    );
    agrees("", &["let X = (2.0).pow(0.5)"], "1.4142135623730951");
    panics("", &["let X = (2).pow(63)"], "(2).pow(63)", OVERFLOW);
    panics(
        "",
        &["let X = (2).pow(-1)"],
        "(2).pow(-1)",
        "negative exponent -1 in `Int.pow`",
    );
    panics(
        "",
        &["let D: Decimal = 2.5", "let X = D.pow(200)"],
        "D.pow(200)",
        DIGITS,
    );
    panics(
        "",
        &["let D: Decimal = 1e6000", "let X = D.pow(2)"],
        "D.pow(2)",
        EXPONENT,
    );
    panics(
        "",
        &["let D: Decimal = 2", "let X = D.pow(-3)"],
        "D.pow(-3)",
        "negative exponent -3 in `Decimal.pow`",
    );
    panics(
        "",
        &["let B: BigInt = 2", "let X = B.pow(70000)"],
        "B.pow(70000)",
        "BigInt.pow result too large (65536 bits or more)",
    );
    // A result past the checker's size bound is not evaluated: no verdict is guessed, and
    // the interpreter computes it.
    let (constants, _) = programs("", &["let B: BigInt = 3", "let X = B.pow(5000)"]);
    let checked = check_source("main.kl", &constants).unwrap();
    assert!(checked.diagnostics.is_empty(), "{constants}");
    // Other method calls are still not constant expressions.
    let checked = check_source(
        "main.kl",
        "let X = (3).abs()\n\nfn main():\n    print(\"{X}\")\n",
    )
    .unwrap();
    let codes: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["K1204"]);
}

// -- Q48: the two Decimal traps -----------------------------------------------------------------

#[test]
fn decimal_traps_have_separate_messages() {
    let nines = "let A: Decimal = 9999999999999999999999999999999999";
    panics("", &[nines, "let B = A + 2"], "A + 2", DIGITS);
    panics(
        "",
        &["let E: Decimal = 1e6144", "let F = E * 10"],
        "E * 10",
        EXPONENT,
    );
    panics(
        "",
        &["let E: Decimal = 1e6111", "let F = E * E"],
        "E * E",
        EXPONENT,
    );
    panics(
        "",
        &["let T: Decimal = 1e-6176", "let U = T * 0.1"],
        "T * 0.1",
        EXPONENT,
    );
    // Clamping: 1e6111 × 10 is 10 × 10^6111, and 1e6144 is 10^33 × 10^6111.
    agrees(
        "",
        &["let E: Decimal = 1e6111", "let F = E * 10 == 1e6112"],
        "true",
    );
    agrees(
        "",
        &["let E: Decimal = 1e6144", "let F = E == 10e6143"],
        "true",
    );
}

// -- Q48, Q52: literals -----------------------------------------------------------------------------

/// Splitmix64, for a reproducible sample of literals.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Literals near the edges of decimal128 and binary64: a significand of 1 to 40 digits
/// (some ending in zeros), an optional fraction, an exponent near a limit.
fn edge_literals() -> Vec<String> {
    let mut rng = Rng(0x4b41_4c41_4d06);
    let exponents = [
        -6215, -6177, -6176, -6175, -6143, -400, -324, -1, 0, 1, 300, 307, 308, 309, 6077, 6110,
        6111, 6112, 6143, 6144, 6145, 6200,
    ];
    let mut out = vec![
        "1e6144".to_string(),
        "1e6145".to_string(),
        "1e-6176".to_string(),
        "1e-6177".to_string(),
        "1.0e-6176".to_string(),
        "1.7976931348623157e308".to_string(),
        "1.7976931348623159e308".to_string(),
        "1e309".to_string(),
        "1e-400".to_string(),
        format!("1{}.0", "0".repeat(60)),
        format!("1{}1.0", "0".repeat(40)),
    ];
    for _ in 0..90 {
        let len = 1 + rng.below(40) as usize;
        let zeros = rng.below(4) == 0;
        let mut digits = String::new();
        for i in 0..len {
            let d = if i == 0 {
                1 + rng.below(9)
            } else if zeros && i + 3 >= len {
                0
            } else {
                rng.below(10)
            };
            digits.push(char::from(b'0' + d as u8));
        }
        let split = rng.below(len as u64 + 1) as usize;
        let mantissa = if split == 0 || split == len {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..split], &digits[split..])
        };
        let e = exponents[rng.below(exponents.len() as u64) as usize];
        out.push(format!("{mantissa}e{e}"));
    }
    out
}

/// `fn main(): let x: <ty> = <lit>; print(x)`: its K0126 verdict and, when it runs, its
/// output.
fn literal_program(ty: &str, lit: &str) -> (bool, Option<String>) {
    let src = format!("fn main():\n    let x: {ty} = {lit}\n    print(\"{{x}}\")\n");
    let checked = check_source("main.kl", &src).unwrap();
    let codes: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
    if !codes.is_empty() {
        assert_eq!(codes, ["K0126"], "{src}");
        return (false, None);
    }
    match run("main.kl", &src) {
        (RunOutcome::Done, out) => (true, Some(out.trim_end().to_string())),
        (other, _) => panic!(
            "the checker accepts `{lit}` as `{ty}`, but the run exits {}: {}",
            other.exit_code(),
            other.stderr_text("main.kl")
        ),
    }
}

#[test]
fn decimal_literals_are_k0126_exactly_when_the_interpreter_cannot_hold_them() {
    let mut accepted = 0;
    for lit in edge_literals() {
        let want = Decimal::parse(&lit);
        let (ok, out) = literal_program("Decimal", &lit);
        assert_eq!(
            ok,
            want.is_ok(),
            "`{lit}`: checker accepts {ok}, interpreter {want:?}"
        );
        if let (Some(out), Ok(d)) = (out, want) {
            assert_eq!(out, d.to_string(), "`{lit}`");
            accepted += 1;
        }
    }
    assert!(accepted > 40, "only {accepted} literals in range");
}

#[test]
fn float_literals_are_k0126_exactly_when_they_overflow() {
    let mut rejected = 0;
    for (i, lit) in edge_literals().iter().enumerate() {
        let infinite = lit.parse::<f64>().unwrap().is_infinite();
        let (ok, _) = literal_program("Float", lit);
        assert_eq!(ok, !infinite, "`{lit}`");
        rejected += usize::from(!ok);
        if i < 12 {
            // Negated, the same verdict.
            let (ok, _) = literal_program("Float", &format!("-{lit}"));
            assert_eq!(ok, !infinite, "`-{lit}`");
        }
    }
    assert!(rejected > 10, "only {rejected} literals overflow");
    // Without an annotation a float literal is a `Float`.
    let checked = check_source(
        "main.kl",
        "fn main():\n    let x = 1e7000\n    print(\"{x}\")\n",
    )
    .unwrap();
    let codes: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["K0126"]);
}

#[test]
fn integer_literals_beyond_u128_run_in_every_context_that_holds_them() {
    let two_128 = "340282366920938463463374607431768211456";
    assert_eq!(
        literal_program("BigInt", two_128).1.as_deref(),
        Some(two_128)
    );
    assert_eq!(
        literal_program("BigInt", &format!("-{two_128}"))
            .1
            .as_deref(),
        Some(format!("-{two_128}").as_str())
    );
    // Written in hex: 2^132.
    assert_eq!(
        literal_program("BigInt", "0x10_0000_0000_0000_0000_0000_0000_0000_0000")
            .1
            .as_deref(),
        Some("5444517870735015415413993718908291383296")
    );
    assert_eq!(
        literal_program("Float", two_128).1.as_deref(),
        Some("3.402823669209385e38")
    );
    assert!(!literal_program("Float", &format!("1{}", "0".repeat(400))).0);
    // As a `Decimal`: trailing zeros fit; 39 significant digits do not.
    let big_round = format!("1{}", "0".repeat(60));
    assert_eq!(
        literal_program("Decimal", &big_round).1,
        Some(Decimal::parse(&big_round).unwrap().to_string())
    );
    assert!(!literal_program("Decimal", "340282366920938463463374607431768211455").0);
    assert!(!literal_program("Int", two_128).0);
}

// -- Q46 --------------------------------------------------------------------------------------

#[test]
fn std_task_re_exports_cell() {
    let src = "use std/task.{Cell}\n\nfn main():\n    let c = Cell.new(1)\n    c.set(2)\n";
    let checked = check_source("main.kl", src).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
}
