//! Spec codes the parser reports in place of a generic parse error (M2 iteration 2):
//! - K1202 module-level `var` (§13.4) and K1203 glob import (§13.1), not K0255/K0250;
//! - K0413 for the file's own core name where the AST stores a plain identifier (`var`
//!   names, function and lambda parameters, `as` names, list-rest names; §7.2). Core names
//!   in `let`/`for`/`case` patterns stay a path the resolver reports (they are a `Name`);
//!   another skin's core name is an ordinary identifier (FOUNDER-QUEUE Q21);
//! - the §5.4 hint for `و` glued to the next word, as a machine fix (FOUNDER-QUEUE Q22);
//! - K1005 for a `Float` literal pattern (`case 1.5:`, §11 rule 6), not K0250 (M2 iteration 3).

mod common;

use common::*;
use kalam_base::{Skin, Span};
use kalam_diagnostics::{Applicability, Diagnostic};

/// Every diagnostic (lexer and parser) of `src` read in `skin`; the CST stays lossless.
fn diags(src: &str, skin: Skin) -> Vec<Diagnostic> {
    let out = kalam_parser::parse_source(src, skin, kalam_base::FileId(0));
    assert_eq!(out.cst.text(), src, "CST must stay lossless");
    out.diagnostics
}

fn codes(src: &str, skin: Skin) -> Vec<&'static str> {
    diags(src, skin).iter().map(|d| d.code).collect()
}

fn text_at(src: &str, span: Span) -> &str {
    &src[span.start as usize..span.end as usize]
}

/// Applies the first fix of `d` to `src`.
fn apply(src: &str, d: &Diagnostic) -> String {
    let fix = d.fixes.first().expect("a fix");
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|e| std::cmp::Reverse(e.span.start));
    let mut out = src.to_string();
    for e in edits {
        out.replace_range(e.span.start as usize..e.span.end as usize, &e.insert);
    }
    out
}

// ---- K1202, K1203 ----

#[test]
fn module_level_var_is_k1202_in_every_skin() {
    for (src, skin, kw) in [
        (
            "var counter = 0\n\nfn main():\n    print(\"{counter}\")\n",
            Skin::En,
            "var",
        ),
        (
            "متغير counter = 0\n\nدالة رئيسي():\n    اطبع(\"{counter}\")\n",
            Skin::Ar,
            "متغير",
        ),
        (
            "motghayer counter = 0\n\ndala ra2isi():\n    ektb(\"{counter}\")\n",
            Skin::Franco,
            "motghayer",
        ),
    ] {
        let ds = diags(src, skin);
        assert_eq!(
            ds.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["K1202"],
            "{skin}"
        );
        assert_eq!(text_at(src, ds[0].span), kw);
        // The rest of the line still parses (as a constant), so nothing cascades.
        let (out, _) = parse_text(src, skin);
        assert!(shape_of(&out).contains("counter"), "{}", shape_of(&out));
    }
}

#[test]
fn module_level_var_suggests_let() {
    let src = "var limit: Int = 10\n";
    let ds = diags(src, Skin::En);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].fixes[0].applicability, Applicability::Suggestion);
    assert_eq!(apply(src, &ds[0]), "let limit: Int = 10\n");
    assert_eq!(codes(&apply(src, &ds[0]), Skin::En), Vec::<&str>::new());
}

#[test]
fn var_inside_a_function_is_fine() {
    assert_eq!(
        codes("fn main():\n    var x = 0\n    x += 1\n", Skin::En),
        Vec::<&str>::new()
    );
}

#[test]
fn glob_import_is_k1203_in_every_skin() {
    for (src, skin) in [
        ("use std/http.*\n\nfn main():\n    print(\"x\")\n", Skin::En),
        (
            "استخدم std/http.*\n\nدالة رئيسي():\n    اطبع(\"x\")\n",
            Skin::Ar,
        ),
        (
            "hat std/http.*\n\ndala ra2isi():\n    ektb(\"x\")\n",
            Skin::Franco,
        ),
    ] {
        let ds = diags(src, skin);
        assert_eq!(
            ds.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["K1203"],
            "{skin}"
        );
        assert_eq!(text_at(src, ds[0].span), ".*");
    }
}

#[test]
fn glob_import_keeps_the_use() {
    let src = "use std/http.*\nuse std/json\n";
    let (out, _) = parse_text(src, Skin::En);
    let s = shape_of(&out);
    assert!(s.contains("std/http") && s.contains("std/json"), "{s}");
    assert_eq!(codes(src, Skin::En), vec!["K1203"]);
}

// ---- K0413 where the AST stores an identifier ----

#[test]
fn own_skin_core_name_as_var_name_is_k0413() {
    for (src, skin, name) in [
        ("fn f():\n    var Int = 1\n", Skin::En, "Int"),
        ("دالة f():\n    متغير اطبع = 1\n", Skin::Ar, "اطبع"),
        ("dala f():\n    motghayer ektb = 1\n", Skin::Franco, "ektb"),
    ] {
        let ds = diags(src, skin);
        assert_eq!(
            ds.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["K0413"],
            "{skin}"
        );
        assert_eq!(text_at(src, ds[0].span), name);
    }
}

#[test]
fn own_skin_core_name_as_parameter_is_k0413() {
    assert_eq!(
        codes("fn f(Int: Int):\n    pass\n", Skin::En),
        vec!["K0413"]
    );
    assert_eq!(
        codes("fn f(x: Int, print: String):\n    pass\n", Skin::En),
        vec!["K0413"]
    );
    assert_eq!(
        codes("دالة f(عدد: عدد):\n    تخطى\n", Skin::Ar),
        vec!["K0413"]
    );
    assert_eq!(
        codes("dala f(adad: adad):\n    fot\n", Skin::Franco),
        vec!["K0413"]
    );
}

#[test]
fn own_skin_core_name_as_lambda_parameter_is_k0413() {
    assert_eq!(
        codes("fn f():\n    let g = fn(print) => 1\n", Skin::En),
        vec!["K0413"]
    );
    assert_eq!(
        codes(
            "fn f():\n    let g = fn(x: Int, World: Int) => x\n",
            Skin::En
        ),
        vec!["K0413"]
    );
}

#[test]
fn own_skin_core_name_after_as_or_rest_is_k0413() {
    assert_eq!(
        codes(
            "fn f(x: Int):\n    match x:\n        case 1 as Int: pass\n        case _: pass\n",
            Skin::En
        ),
        vec!["K0413"]
    );
    assert_eq!(
        codes(
            "fn f(x: List[Int]):\n    match x:\n        case [a, ..List]: pass\n        case _: pass\n",
            Skin::En
        ),
        vec!["K0413"]
    );
}

#[test]
fn other_skin_core_names_are_ordinary_identifiers() {
    // FOUNDER-QUEUE Q21 (spec §5.4, H0202).
    for (src, skin) in [
        ("fn f(عدد: Int):\n    var adad = عدد\n", Skin::En),
        ("دالة f(print: عدد):\n    متغير Int = print\n", Skin::Ar),
        (
            "dala f(Int: adad):\n    motghayer عدد = Int\n",
            Skin::Franco,
        ),
        ("fn f():\n    let g = fn(اطبع) => اطبع\n", Skin::En),
    ] {
        assert_eq!(codes(src, skin), Vec::<&str>::new(), "{skin}: {src}");
    }
}

#[test]
fn backticked_core_spelling_is_an_ordinary_identifier() {
    assert_eq!(
        codes("fn f(`Int`: Int):\n    var `print` = `Int`\n", Skin::En),
        Vec::<&str>::new()
    );
}

#[test]
fn core_name_in_a_let_pattern_is_left_to_the_resolver() {
    // `let`/`for`/`case` patterns keep the core name as a path (a `Name` can hold it).
    assert_eq!(
        codes("fn f():\n    let Int = 1\n", Skin::En),
        Vec::<&str>::new()
    );
    assert_eq!(
        codes(
            "fn f(xs: List[Int]):\n    for print in xs:\n        pass\n",
            Skin::En
        ),
        Vec::<&str>::new()
    );
}

// ---- Q22: `و` glued to the next word ----

fn glued_and(src: &str) -> Diagnostic {
    let ds = diags(src, Skin::Ar);
    assert_eq!(
        ds.len(),
        1,
        "{:?}",
        ds.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let d = ds.into_iter().next().unwrap();
    assert_eq!(d.code, "K0250");
    assert_eq!(d.fixes.len(), 1);
    assert_eq!(d.fixes[0].applicability, Applicability::Machine);
    d
}

#[test]
fn glued_waw_in_an_if_condition_gets_a_machine_fix() {
    let src = "دالة f(أ: منطقي، ب: منطقي):\n    إذا أ وب:\n        تخطى\n";
    let d = glued_and(src);
    assert_eq!(text_at(src, d.span), "وب");
    let fixed = apply(src, &d);
    assert_eq!(
        fixed,
        "دالة f(أ: منطقي، ب: منطقي):\n    إذا أ و ب:\n        تخطى\n"
    );
    assert_eq!(codes(&fixed, Skin::Ar), Vec::<&str>::new());
}

#[test]
fn glued_waw_before_a_keyword_or_core_name() {
    for (src, fixed) in [
        (
            "دالة f(أ: منطقي):\n    طالما أ وليس أ:\n        تخطى\n",
            "دالة f(أ: منطقي):\n    طالما أ و ليس أ:\n        تخطى\n",
        ),
        (
            "دالة f(أ: منطقي):\n    إذا أ وصح:\n        تخطى\n",
            "دالة f(أ: منطقي):\n    إذا أ و صح:\n        تخطى\n",
        ),
        (
            "دالة f(أ: منطقي، ب: منطقي):\n    ليكن ج = أ أو ب وأ\n",
            "دالة f(أ: منطقي، ب: منطقي):\n    ليكن ج = أ أو ب و أ\n",
        ),
    ] {
        let d = glued_and(src);
        let out = apply(src, &d);
        assert_eq!(out, fixed);
        assert_eq!(codes(&out, Skin::Ar), Vec::<&str>::new(), "{out}");
    }
}

#[test]
fn glued_waw_in_a_match_guard() {
    let src = "دالة f(أ: منطقي، ب: منطقي):\n    حسب أ:\n        عندما س إذا س وب: تخطى\n        عندما _: تخطى\n";
    let d = glued_and(src);
    assert_eq!(codes(&apply(src, &d), Skin::Ar), Vec::<&str>::new());
}

#[test]
fn glued_waw_hint_is_arabic_only_and_needs_a_rest() {
    // A lone `و` is the keyword; in an English file a `و…` word is just a name (the
    // generic parse error stays, without the hint).
    let src = "fn f(a: Bool, وب: Bool):\n    if a وب:\n        pass\n";
    let ds = diags(src, Skin::En);
    assert_eq!(ds.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["K0250"]);
    assert!(ds[0].fixes.is_empty());
}

// ---- K1005 ----

#[test]
fn float_literal_pattern_is_k1005_in_every_skin() {
    // §11 rule 6: "`Float` patterns are not allowed (K1005)". The literal, a negative one
    // and a range of them are each one K1005 (never the generic K0250), and the arm's body
    // still parses.
    for (src, skin) in [
        (
            "fn f(x: Float) -> Int:
    return match x:
        case 1.5: 1
        case _: 0
",
            Skin::En,
        ),
        (
            "fn f(x: Float) -> Int:
    return match x:
        case -0.5: 1
        case _: 0
",
            Skin::En,
        ),
        (
            "fn f(x: Float) -> Int:
    return match x:
        case 0.0..=1.0: 1
        case _: 0
",
            Skin::En,
        ),
        (
            "دالة f(x: كسري) -> عدد:
    أرجع حسب x:
        عندما 1.5: 1
        عندما _: 0
",
            Skin::Ar,
        ),
        (
            "dala f(x: kasri) -> adad:
    rag3 hasab x:
        lama 1.5: 1
        lama _: 0
",
            Skin::Franco,
        ),
    ] {
        let ds = diags(src, skin);
        assert_eq!(
            ds.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["K1005"],
            "{src}"
        );
        let at = text_at(src, ds[0].span);
        assert!(at.contains('.') && !at.contains(':'), "{at:?}");
    }
}
