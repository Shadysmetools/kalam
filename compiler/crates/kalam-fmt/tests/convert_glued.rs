//! Regression (FOUNDER-QUEUE Q26, M1 fuzz oracles `convert_output_rejected` and
//! `convert_roundtrip`): a backticked identifier written right against another word
//! (`` return`Some` ``, `` `fe`ka ``, `` `ektb`wi ``) must keep its backticks in every target
//! skin. Dropping them glued the two words into one: `أرجعSome` (SEC0105), `feبصفة`, or, worse,
//! `rag3Some` / `ektband`, which lex cleanly as a different program.
//!
//! For each case and every target skin: the conversion lexes and parses cleanly in the
//! target, has the same AST, and converting it back is byte-identical to converting the
//! source to its own skin (spec §4.3 G1, G2).

mod common;

use common::*;
use kalam_base::{FileId, Skin};

fn errors(text: &str, skin: Skin) -> Vec<&'static str> {
    let out = kalam_parser::parse_source(text, skin, FileId(0));
    out.diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code)
        .collect()
}

fn check(text: &str, from: Skin) {
    assert_eq!(errors(text, from), Vec::<&str>::new(), "source:\n{text}");
    let source_tree = tree(&parse_clean(text, from).expect("clean source"));
    let canon = kalam_fmt::convert(text, from, from, &cfg()).expect("self-conversion");
    for to in Skin::ALL {
        let out = kalam_fmt::convert(text, from, to, &cfg()).expect("conversion");
        assert_eq!(
            errors(&out, to),
            Vec::<&str>::new(),
            "{from}->{to} output:\n{out}"
        );
        let out_tree = tree(&parse_clean(&out, to).expect("clean output"));
        assert_eq!(
            out_tree, source_tree,
            "{from}->{to} changed the program:\n{out}"
        );
        let back = kalam_fmt::convert(&out, to, from, &cfg()).expect("conversion back");
        assert_eq!(back, canon, "{from}->{to}->{from} != {from}->{from}");
    }
}

#[test]
fn keyword_then_backticked_core_name() {
    // oracle/convert_output_rejected-1.kl: `أرجعSome` (SEC0105) in ar, `rag3Some` in franco.
    // `Some` is declared (imported), so it is an ordinary name and loses its backticks
    // unless glued (an undeclared one keeps them anyway, FOUNDER-QUEUE Q16).
    check("use std.{`Some`}\n\nfn f():\n    return`Some`\n", Skin::En);
}

#[test]
fn backticked_name_then_keyword_in_a_use_alias() {
    // oracle/convert_output_rejected-2.franco.kl: `` `fe`ka `` became `feas` / `feبصفة`.
    check("hat std.{`fe`ka x}\n", Skin::Franco);
}

#[test]
fn backticked_core_name_then_and() {
    // oracle/convert_roundtrip-1.franco.kl: `` `ektb`wi `` became `ektband` in en, which
    // converted back as an ordinary name.
    check(
        "hat std.{`ektb`}\n\ndala f(x: mante2i):\n    khalli y = `ektb`wi x\n",
        Skin::Franco,
    );
}

#[test]
fn glued_words_inside_an_interpolation_hole() {
    check(
        "hat std.{`ektb`}\n\ndala f(x: mante2i):\n    ektb(\"{`ektb`wi x}\")\n",
        Skin::Franco,
    );
    check(
        "use std.{`Some`}\n\nfn f(x: Int, y: Bool):\n    print(\"{x(`Some`and y)}\")\n",
        Skin::En,
    );
}

#[test]
fn two_backticked_names_glued() {
    check(
        "use std.{`Some`, `None`}\n\nfn f():\n    return`Some`or`None`\n",
        Skin::En,
    );
}

#[test]
fn spaced_words_are_unchanged() {
    // Only glued words keep backticks: a backticked non-reserved name with a space around
    // it still loses them, as before.
    let out = kalam_fmt::convert("fn f():\n    return `x`\n", Skin::En, Skin::Ar, &cfg())
        .expect("conversion");
    assert_eq!(out, "دالة f():\n    أرجع x\n");
}

#[test]
fn other_skin_core_names_as_bindings_convert_cleanly() {
    // FOUNDER-QUEUE Q21: another skin's core name is an ordinary name in every binding
    // position. Converted into the skin that reserves it, it is backticked, so the parser's
    // K0413 (parameters, `var`) never fires on converter output.
    check(
        "fn f(عدد: Int, ektb: Bool):\n    var اطبع = عدد\n    let g = fn(adad) => adad\n    let Int2 = g(اطبع)\n",
        Skin::En,
    );
    check(
        "دالة f(Int: عدد):\n    متغير print = Int\n    ليكن adad = print\n",
        Skin::Ar,
    );
}
