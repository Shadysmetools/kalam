//! Regression (M1/M2 fuzz oracles `convert_output_rejected` and `convert_roundtrip`): every
//! input the fuzzer saved under `eval/fuzz/oracle/` (copied read-only into
//! `tests/oracle_cases/m1/`), and the M2 iter 5 verifier's conversions of them
//! (`tests/oracle_cases/v5r/`), now converts cleanly.
//!
//! The rule behind it (spec §4.3 G1/G2): after conversion every token boundary must re-lex
//! identically. Conversion re-spells tokens but copies the bytes between them, so two tokens
//! written with nothing between them (`return"x"`, `` return`Some` ``, `x.0`) can merge, split
//! or change kind once one of them is re-spelled. The converter now checks every such glued
//! pair and separates them with a space, or keeps backticks, whenever the target spelling
//! would not re-lex as the same two tokens.
//!
//! For every clean input `src` in skin X, the oracle is the fuzzer's (`kalam-fuzz`
//! `target.rs`): `convert(src, X→X)` and `convert(src, X→Y)` pass their own lexer and parser,
//! have `src`'s AST, and `convert(convert(src, X→Y), Y→X) == convert(src, X→X)`. Inputs that
//! are not clean in their own skin are skipped, as the fuzzer skips them; at least the M1
//! inputs must all be clean.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use kalam_base::Skin;
use kalam_fmt::FmtConfig;

fn cases(dir: &str) -> Vec<(String, Skin, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("oracle_cases")
        .join(dir);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            let skin = Skin::from_path(&name)?;
            let text = String::from_utf8_lossy(&std::fs::read(&p).ok()?).into_owned();
            Some((format!("{dir}/{name}"), skin, text))
        })
        .collect()
}

/// Runs the oracle on every clean case of `dir`; the names of the cases that are not clean
/// in their own skin (and so are skipped, as the fuzzer skips them).
fn run(dir: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut skipped = Vec::new();
    for (name, skin, text) in cases(dir) {
        if !errors(&text, skin).is_empty() {
            skipped.push(name);
            continue;
        }
        checked += 1;
        for config in configs() {
            failures.extend(oracle_failures(&name, &text, skin, &config));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(checked > 0, "{dir}: no clean case");
    skipped
}

/// The v5r cases that are not clean in their own skin: the old converter's rejected outputs
/// (`if"x?…"` and `law"x?…"`, the keyword-string bug this file guards against).
const SKIPPED_V5R: &[&str] = &[
    "v5r/out_convert_output_rejected-5_en.kl",
    "v5r/out_convert_output_rejected-5_franco.franco.kl",
];

#[test]
fn m1_fuzz_oracle_inputs_convert_cleanly() {
    // All eighteen are clean (the fuzzer saved them because they are): every input under
    // `eval/fuzz/oracle/` on 2026-10-26, including `-7` to `-14` and `convert_roundtrip-4`,
    // found on the frozen M1 binary after the first copy. This converter handles them all.
    assert_eq!(run("m1"), Vec::<String>::new());
    assert_eq!(cases("m1").len(), 18);
}

#[test]
fn verifier_conversions_convert_cleanly() {
    // The old converter's outputs that its own lexer rejected are not inputs the fuzzer
    // would convert; they are listed so a lexer change cannot silently shrink the set.
    let skipped = run("v5r");
    eprintln!("skipped: {skipped:?}");
    assert_eq!(skipped, SKIPPED_V5R);
    assert_eq!(cases("v5r").len(), 30);
}

fn assert_oracle(text: &str, from: Skin) {
    let failures: Vec<String> = configs()
        .iter()
        .flat_map(|c| oracle_failures("case", text, from, c))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn keyword_glued_to_a_string() {
    // oracle/convert_output_rejected-5.ar.kl: `إذا"x"` became `law"x"` / `if"x"`, which
    // lex as a tagged literal with an unknown tag (K0260).
    assert_oracle(
        "دالة f(س: منطقي):\n    إذا س == صح: أرجع\n    ليكن ن = إذا\"x\" == \"y\": 1 وإلا: 2\n",
        Skin::Ar,
    );
    assert_oracle("dala f() -> nass:\n    rag3\"x\"\n", Skin::Franco);
    assert_oracle("دالة f() -> نص:\n    أرجع\"x\"\n", Skin::Ar);
}

#[test]
fn other_keywords_glued_to_strings() {
    // An Arabic keyword glued to a string (`في"abc"`) is `in"abc"`/`fi"abc"` elsewhere: a
    // tagged literal. A string glued *before* a keyword is fine in every skin.
    assert_oracle(
        "دالة f(أ: منطقي) -> نص:\n    لكل ح في\"abc\":\n        اطبع(ح)\n    ليكن ب = أ و\"y\" == \"y\" أو ليس\"y\" == \"z\"\n    أرجع\"x\"\n",
        Skin::Ar,
    );
    assert_oracle(
        "fn f(a: Bool) -> String:\n    let s = if a: \"t\"else: \"f\"\n    return s\n",
        Skin::En,
    );
}

#[test]
fn the_space_is_added_in_every_skin_including_the_source() {
    let cfg = FmtConfig::default();
    let src = "دالة f() -> نص:\n    أرجع\"x\"\n";
    let same = kalam_fmt::convert(src, Skin::Ar, Skin::Ar, &cfg).unwrap();
    assert_eq!(same, "دالة f() -> نص:\n    أرجع \"x\"\n");
    let en = kalam_fmt::convert(src, Skin::Ar, Skin::En, &cfg).unwrap();
    assert_eq!(en, "fn f() -> String:\n    return \"x\"\n");
    let franco = kalam_fmt::convert(src, Skin::Ar, Skin::Franco, &cfg).unwrap();
    assert_eq!(franco, "dala f() -> nass:\n    rag3 \"x\"\n");
}

#[test]
fn member_index_chains_follow_the_digit_setting() {
    // DR-26 (FOUNDER-QUEUE Q54): right after `.`, a digit run of either system is a member
    // index, so `t.٠.١` lexes as `t . ٠ . ١`. A chain of member indices follows the digit
    // setting like a lone index; nothing is forced back to ASCII.
    let src = "fn f(t: ((Int, Int), Int)) -> Int:\n    return t.0.1 + t.1 + t.0.x.1 + t.0.0.10\n";
    assert_oracle(src, Skin::En);
    let config = FmtConfig {
        ar_digits: kalam_fmt::ArDigits::ArabicIndic,
    };
    let ar = kalam_fmt::convert(src, Skin::En, Skin::Ar, &config).unwrap();
    assert!(
        ar.ends_with(
            "أرجع t.\u{0660}.\u{0661} + t.\u{0661} + t.\u{0660}.x.\u{0661} + t.\u{0660}.\u{0660}.\u{0661}\u{0660}\n"
        ),
        "{ar}"
    );
    assert!(errors(&ar, Skin::Ar).is_empty(), "{ar}");
    assert_oracle(&ar, Skin::Ar);
    // `kalam fmt` prints member indices on its own path: the same rule holds there.
    // (`format_strict`: `format` would hide a printer failure behind its fallback.)
    let formatted = kalam_fmt::format_strict(&ar, Skin::Ar, &config).unwrap();
    assert_eq!(formatted, ar);
    // And back: the Arabic-Indic chain converts to the English source.
    let en = kalam_fmt::convert(&ar, Skin::Ar, Skin::En, &config).unwrap();
    assert_eq!(en, src);
}
