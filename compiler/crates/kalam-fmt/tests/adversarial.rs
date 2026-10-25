//! Adversarial round-trip cases written by the independent verifier (not by the formatter's
//! author). Each case is a small program aimed at a place where `fmt` or `convert` is likely
//! to go wrong: comments at block boundaries, identifiers that are another skin's keyword,
//! reserved words in name positions (§5.3 rule 8) inside interpolation holes, Franco aliases,
//! Persian/Urdu letters (D19), deep nesting and very wide lines.
//!
//! The corpus and the generator compare `normalized_shape` plus comment texts, which drop the
//! one-line vs indented form of blocks and where a comment sits relative to INDENT/DEDENT.
//! These tests compare **KCF** as well (spec §4.4), which records both:
//! - `kcf(fmt(x)) == kcf(x)` (M1 checklist item 9), for inputs whose blank lines and block
//!   forms are already canonical (fmt's blank-line and one-line-block normalization changes
//!   KCF by design today; that is a founder-level fork, not asserted here);
//! - KCF is equal at every step of every ordered pair and 3-hop cycle (P1/P2), each step
//!   parsed in its own skin, with no error (K0201 included);
//! - the inferred-skin KCF equals the explicit-skin KCF (a wrong guess would re-lex hole code
//!   in the wrong skin);
//! - `fmt` is idempotent and every round trip from `fmt(x)` is byte-identical (P3).

mod common;

use common::*;
use kalam_base::{FileId, Skin};
use kalam_fmt::{ArDigits, FmtConfig};
use kalam_syntax::kcf;

fn error_codes(text: &str, skin: Skin) -> Vec<&'static str> {
    kalam_parser::parse_source(text, skin, FileId(0))
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code)
        .collect()
}

/// KCF of `text` read in `skin`, after checking that it parses cleanly and that the skin
/// `kcf::serialize` infers gives the same bytes.
#[track_caller]
fn kcf_of(text: &str, skin: Skin) -> String {
    let out = kalam_parser::parse_source(text, skin, FileId(0));
    let errs: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code)
        .collect();
    assert!(errs.is_empty(), "{skin}: does not parse: {errs:?}\n{text}");
    let explicit = kcf::serialize_in(&out.cst, skin);
    let inferred = kcf::serialize(&out.cst);
    assert_eq!(
        String::from_utf8_lossy(&inferred),
        String::from_utf8_lossy(&explicit),
        "{skin}: the inferred skin gives a different KCF\n{text}"
    );
    String::from_utf8(explicit).expect("KCF is UTF-8")
}

#[track_caller]
fn fmt(text: &str, skin: Skin) -> String {
    match kalam_fmt::format_strict(text, skin, &cfg()) {
        Ok(s) => s,
        Err(e) => panic!("{skin}: {e}\n{text}"),
    }
}

#[track_caller]
fn conv(text: &str, from: Skin, to: Skin) -> String {
    match kalam_fmt::convert(text, from, to, &cfg()) {
        Ok(s) => s,
        Err(d) => panic!(
            "{from}->{to}: {:?}\n{text}",
            d.iter().map(|d| d.code).collect::<Vec<_>>()
        ),
    }
}

/// The full battery. `canonical_layout` says that `src`'s blank lines and block forms are
/// already what fmt prints, so `kcf(fmt(src)) == kcf(src)` must hold. Returns `fmt(src)`.
#[track_caller]
fn battery(src: &str, skin: Skin, canonical_layout: bool) -> String {
    let k_src = kcf_of(src, skin);
    let before = parse_clean(src, skin).unwrap();
    let f = fmt(src, skin);
    assert_eq!(fmt(&f, skin), f, "{skin}: fmt is not idempotent\n{f}");
    let after = parse_clean(&f, skin).expect("formatted text parses");
    assert_eq!(tree(&after), tree(&before), "{skin}: fmt changed the AST");
    assert_eq!(
        comments(&after),
        comments(&before),
        "{skin}: fmt lost comments"
    );
    let k_f = kcf_of(&f, skin);
    if canonical_layout {
        assert_eq!(k_f, k_src, "{skin}: kcf(fmt(x)) != kcf(x)\n{src}\n---\n{f}");
    }
    for start in [src, f.as_str()] {
        let k_start = kcf_of(start, skin);
        for path in round_trip_paths(skin) {
            let steps = convert_path(start, &path).unwrap_or_else(|e| panic!("{e}\n{start}"));
            for (i, step) in steps.iter().enumerate() {
                let s = path[i + 1];
                assert!(
                    error_codes(step, s).is_empty(),
                    "{path:?} step {i} ({s}) has errors {:?}\n{step}",
                    error_codes(step, s)
                );
                assert_eq!(
                    kcf_of(step, s),
                    k_start,
                    "{path:?} step {i} ({s}): KCF differs\n{step}"
                );
                if start == f {
                    assert_eq!(
                        fmt(step, s),
                        *step,
                        "{path:?} step {i} ({s}): a converted formatted file is not formatted"
                    );
                }
            }
            if start == f {
                assert_eq!(steps.last().unwrap(), &f, "{path:?} is not byte-identical");
            }
        }
    }
    f
}

// ----- 1. comments in odd places ------------------------------------------------------------

#[test]
fn comment_at_a_dedent_boundary_stays_on_the_same_side_of_the_dedent() {
    // Outer-indent comment after a nested block's last statement, then an in-between one.
    let src = "fn main():\n    if a:\n        if b:\n            f()\n        # mid level\n    # outer level\n    g()\n# top level\nfn h():\n    pass\n";
    // `fn h` directly after `fn main` without a blank line is not canonical (§21.1 rule 4).
    let f = battery(src, Skin::En, false);
    assert!(
        f.contains("        # mid level\n    # outer level\n    g()\n"),
        "{f}"
    );
    // With canonical blank lines, KCF must be equal.
    let src = "fn main():\n    if a:\n        if b:\n            f()\n        # mid level\n    # outer level\n    g()\n\n# top level\nfn h():\n    pass\n";
    assert_eq!(battery(src, Skin::En, true), src);
}

#[test]
fn comment_between_if_block_and_else_and_after_a_header() {
    let src = "fn main():\n    if a:  # why a\n        f()\n    # before else\n    else:  # why not\n        g()  # g\n";
    let f = battery(src, Skin::En, true);
    assert_eq!(f, src);
    for skin in [Skin::Ar, Skin::Franco] {
        battery(&conv(src, Skin::En, skin), skin, true);
    }
}

#[test]
fn comment_only_file_empty_file_no_final_newline_and_crlf() {
    battery("# only a comment\n", Skin::En, true);
    battery("# one\n\n## two\n", Skin::Ar, true);
    assert_eq!(fmt("", Skin::Franco), "");
    // A missing final newline: fmt adds it, KCF does not record it.
    let f = battery("fn main():\n    f()  # end", Skin::En, true);
    assert_eq!(f, "fn main():\n    f()  # end\n");
    // CRLF with comments everywhere.
    let src = "# a\r\nfn main():  # b\r\n    # c\r\n    f(  # d\r\n        1,\r\n    )\r\n";
    battery(src, Skin::En, true);
}

#[test]
fn comment_trailing_spaces_and_a_comment_inside_a_call() {
    // Trailing spaces after a comment: KCF trims them, fmt may too; both must agree.
    let src = "fn main():   \n    f()  # x   \n";
    battery(src, Skin::En, true);
    let src = "fn main():\n    f(\n        a,  # first\n        # own line in the args\n        b,\n    )\n";
    assert_eq!(battery(src, Skin::En, true), src);
}

// ----- 2. identifiers equal to another skin's keyword -------------------------------------

#[test]
fn identifiers_that_are_other_skins_keywords_survive_every_round_trip() {
    let src = "fn main():\n    let law = 1\n    let men = law\n    let lw = 2\n    let `اذا` = lw\n    let `إذا` = 3\n    print(law + men + lw + `اذا` + `إذا`)\n";
    battery(src, Skin::En, true);
    // `if` and `law` as names in an Arabic file, `إذا` as a name in a Franco file.
    let ar = conv(src, Skin::En, Skin::Ar);
    battery(&ar, Skin::Ar, true);
    let fr = conv(src, Skin::En, Skin::Franco);
    assert!(fr.contains("khali `law` = 1"), "{fr}");
    assert!(fr.contains("khali `lw` = 2"), "{fr}");
    battery(&fr, Skin::Franco, true);
}

#[test]
fn a_cross_skin_identifier_in_a_match_guard_and_a_for_loop() {
    // `law` is a Franco keyword; in `ar` it is a plain name standing before `إذا` and `في`.
    let src = "fn f(x: Int):\n    match x:\n        case law if law > 0:\n            print(law)\n        case _:\n            pass\n    for law in [1, 2]:\n        print(law)\n";
    battery(src, Skin::En, true);
    let ar = conv(src, Skin::En, Skin::Ar);
    assert!(ar.contains("عندما law إذا law > 0"), "{ar}");
    battery(&ar, Skin::Ar, true);
}

#[test]
fn a_persian_letter_identifier_that_folds_to_an_arabic_keyword_is_escaped_in_ar() {
    // فی (Farsi yeh) folds to في (KW_IN) under arabic_fold, so in `ar` it must be backticked.
    let src = "fn main():\n    let \u{0641}\u{06CC} = 1\n    print(\u{0641}\u{06CC})\n";
    battery(src, Skin::En, true);
    let ar = conv(src, Skin::En, Skin::Ar);
    assert!(ar.contains("`\u{0641}\u{06CC}`"), "{ar}");
    assert_eq!(conv(&ar, Skin::Ar, Skin::En), src);
}

// ----- 3. reserved words in name positions inside interpolation holes -------------------

#[test]
fn rule_8_positions_inside_holes_are_not_escaped() {
    let src = "fn main():\n    print(\"{x.law} {f(law: 1)} {law}\")\n";
    battery(src, Skin::En, true);
    let fr = conv(src, Skin::En, Skin::Franco);
    assert_eq!(
        fr,
        "dala ra2isi():\n    ektb(\"{x.law} {f(law: 1)} {`law`}\")\n"
    );
    battery(&fr, Skin::Franco, true);
}

#[test]
fn hole_code_with_commas_try_maps_and_escaped_braces() {
    let src = "fn main():\n    print(\"\\{{f(a, b)?}\\} {m[{1: 2}]} {not ok and x.0}\")\n";
    battery(src, Skin::En, true);
    let ar = conv(src, Skin::En, Skin::Ar);
    assert!(ar.contains("{f(a\u{060C} b)\u{061F}}"), "{ar}");
    battery(&ar, Skin::Ar, true);
    // Arabic-Indic digits inside a hole, with the ArabicIndic project setting.
    let config = FmtConfig {
        ar_digits: ArDigits::ArabicIndic,
    };
    let src = "fn main():\n    print(\"{x + 12} and 34\")\n";
    let ar = kalam_fmt::convert(src, Skin::En, Skin::Ar, &config).unwrap();
    assert!(ar.contains("{x + \u{0661}\u{0662}} and 34"), "{ar}");
    assert_eq!(kcf_of(&ar, Skin::Ar), kcf_of(src, Skin::En));
    assert_eq!(kalam_fmt::format(&ar, Skin::Ar, &config).unwrap(), ar);
}

// ----- 4. Franco aliases and Persian/Urdu letters in keywords -----------------------------

#[test]
fn franco_aliases_canonicalize_and_keep_kcf() {
    let src = "dala f(x: adad) -> adad:\n    khaly y = x\n    lw y > 0:\n        rg3 y\n    wala lw y == 0:\n        foot\n    wla:\n        tol_ma sa7e7:\n            kefaya\n    rg3 0\n";
    let f = battery(src, Skin::Franco, true);
    assert_eq!(
        f,
        "dala f(x: adad) -> adad:\n    khali y = x\n    law y > 0:\n        rag3 y\n    walla law y == 0:\n        fot\n    walla:\n        tool_ma sa7:\n            kfaya\n    rag3 0\n"
    );
}

#[test]
fn franco_aliases_in_name_positions_are_names() {
    let src = "dala ra2isi():\n    x.lw(wala: 1, fe: x.rg3)\n";
    assert_eq!(battery(src, Skin::Franco, true), src);
    assert_eq!(
        conv(src, Skin::Franco, Skin::En),
        "fn main():\n    x.lw(wala: 1, fe: x.rg3)\n"
    );
}

#[test]
fn every_d19_letter_inside_an_arabic_keyword_is_folded_and_canonicalized() {
    // ک in لكل, ی in في and ليكن, ھ in هيكل, ە and ۀ in place of ة in دالة.
    let src = "\u{06BE}يكل T:\n    a: عدد\n\nدال\u{06D5} f():\n    ل\u{06CC}\u{06A9}ن x = 1\n    ل\u{06A9}ل n ف\u{06CC} [1]:\n        اطبع(n)\n\nدال\u{06C0} g():\n    تخطى\n";
    let f = battery(src, Skin::Ar, true);
    assert_eq!(
        f,
        "هيكل T:\n    a: عدد\n\nدالة f():\n    ليكن x = 1\n    لكل n في [1]:\n        اطبع(n)\n\nدالة g():\n    تخطى\n"
    );
}

// ----- 5. deep nesting and wide lines -----------------------------------------------------

#[test]
fn sixty_nested_ifs_past_the_column_limit() {
    let mut src = String::from("fn main():\n");
    for d in 1..=60 {
        src.push_str(&"    ".repeat(d));
        src.push_str(&format!("if x{d}:\n"));
    }
    src.push_str(&"    ".repeat(61));
    src.push_str("f(aaaaaaaa, bbbbbbbb)\n");
    // The call no longer fits, so fmt breaks it with a trailing comma (§21.1 rule 1). KCF
    // records that comma, so `kcf(fmt(x)) == kcf(x)` is not asserted here (see the report).
    let f = battery(&src, Skin::En, false);
    assert!(f.contains("f(\n"), "{f}");
    // Once broken, everything (KCF included) is stable.
    battery(&f, Skin::En, true);
}

#[test]
fn brackets_nested_two_hundred_deep_and_a_token_wider_than_the_limit() {
    let nest = |depth: usize| {
        format!(
            "fn main():\n    let v = {}1{}\n",
            "[".repeat(depth),
            "]".repeat(depth)
        )
    };
    // Under the parser's nesting cap (K0259): it breaks, round-trips and terminates.
    let f = battery(&nest(40), Skin::En, false);
    battery(&f, Skin::En, true);
    // Far past the cap: rejected cleanly by fmt and convert, never a crash.
    let deep = nest(200);
    assert!(kalam_fmt::format(&deep, Skin::En, &cfg()).is_err());
    assert!(kalam_fmt::convert(&deep, Skin::En, Skin::Ar, &cfg()).is_err());
    let wide = "w".repeat(150);
    let src = format!(
        "fn main():\n    let {wide} = \"{}\"\n    {wide}({wide})\n",
        "s".repeat(150)
    );
    // The call is broken (with a trailing comma, which KCF records); the rest must hold.
    let f = battery(&src, Skin::En, false);
    assert!(
        f.contains(&format!("    {wide}(\n        {wide},\n    )\n")),
        "{f}"
    );
    battery(&f, Skin::En, true);
}

#[test]
fn a_long_call_in_a_franco_file_breaks_the_same_way_as_in_english() {
    let long = "argument_number".to_string();
    let en = format!(
        "fn main():\n    print({long}1, {long}2, {long}3, {long}4, {long}5, {long}6, {long}7)\n"
    );
    let f = battery(&en, Skin::En, false);
    assert!(f.contains("print(\n"), "{f}");
    let fr = conv(&f, Skin::En, Skin::Franco);
    assert_eq!(fmt(&fr, Skin::Franco), fr);
}

// ----- 6. tagged literals ---------------------------------------------------------------

#[test]
fn tagged_literal_with_a_hole_converts_its_hole_only() {
    let src = "fn main():\n    let q = sql\"select * from t where a = {x and y}\"\n";
    battery(src, Skin::En, true);
    let ar = conv(src, Skin::En, Skin::Ar);
    assert!(
        ar.contains("sql\"select * from t where a = {x و y}\""),
        "{ar}"
    );
}

// ----- 7. odd layouts: fmt re-lays them out, then everything must be stable ----------------

/// Each source is run through the battery (KCF equality with the source is not asserted,
/// because fmt may break, join or re-indent it), and then `fmt(src)` is run through it again
/// with KCF equality asserted.
#[test]
fn odd_layouts_settle_into_a_stable_canonical_form() {
    let z = "z".repeat(30);
    let cases: Vec<(&str, String, Skin)> = vec![
        ("comment at column 0 inside a block", "fn f():\n# c\n    pass\n".into(), Skin::En),
        (
            "comments at odd indents",
            "fn f():\n    a()\n  # two\n    b()\n      # six\n".into(),
            Skin::En,
        ),
        (
            "comment deeper than its block",
            "fn f():\n    if a:\n        b()\n            # deep\n    c()\n".into(),
            Skin::En,
        ),
        (
            "one-line if/else with comments",
            "fn f():\n    if a: pass  # x\n    else: pass  # y\n".into(),
            Skin::En,
        ),
        (
            "one-line match cases",
            "fn f(x: Int):\n    match x:\n        case 1: return 1\n        case _: return 2\n".into(),
            Skin::En,
        ),
        (
            "match with comments between cases",
            "fn f(x: Int):\n    match x:\n        # first\n        case 1:\n            pass\n        # between\n    # match level\n        case _:  # tail\n            pass\n".into(),
            Skin::En,
        ),
        (
            "map literal with comments",
            "fn f():\n    let m = {\n        \"a\": 1,  # one\n        # own\n        \"b\": 2,\n    }\n".into(),
            Skin::En,
        ),
        (
            "hugged list with a comment",
            "fn f():\n    draw.group([\n        a,  # c\n        b,\n    ])\n".into(),
            Skin::En,
        ),
        ("hugged list too long", format!("fn f():\n    draw.group([{z}, {z}, {z}, {z}])\n"), Skin::En),
        (
            "method chain with a comment",
            format!("fn f():\n    let n = users\n        .filter({z})  # keep\n        .map({z})\n        .sort({z})\n"),
            Skin::En,
        ),
        (
            "long chain in ar",
            format!("دالة f():\n    ليكن n = users.filter({z}).map({z}).sort({z})\n"),
            Skin::Ar,
        ),
        (
            "long chain in franco",
            format!("dala f():\n    khali n = users.filter({z}).map({z}).sort({z})\n"),
            Skin::Franco,
        ),
        ("long signature", format!("fn f({z}: Int, {z}x: Int, {z}y: Int, {z}w: Int) -> Int:\n    return 1\n"), Skin::En),
        ("long use line", format!("use std/{z}.{{{z}, {z}, {z}, {z}}}\n\nfn f():\n    pass\n"), Skin::En),
        ("long unbreakable binary expression", format!("fn f():\n    let x = {z} + {z} + {z} + {z}\n"), Skin::En),
        ("long call in a block header", format!("fn f():\n    if g({z}, {z}, {z}, {z}):\n        pass\n"), Skin::En),
        ("long ar call with Arabic digits", format!("دالة f():\n    اطبع({z}، {z}، {z}، {z}، ١٢)\n"), Skin::Ar),
        ("attributes", "@route(\"/x\")\nexport fn f():\n    pass\n\n@a\n@b(1, 2)\nfn g():\n    pass\n".into(), Skin::En),
        ("expression-bodied fn with a comment", "fn f(x: Int) -> Int => x + 1  # c\n".into(), Skin::En),
        ("ar hole with ، and ؟", "دالة f():\n    اطبع(\"{g(a، b)؟}\")\n".into(), Skin::Ar),
        ("brackets inside a hole", "fn f():\n    print(\"{[[[[1]]]]} { {1: law}[1] } {f(match: 1)}\")\n".into(), Skin::En),
        ("cross-skin names in patterns", "fn f():\n    let (law, fi) = (1, 2)\n    for (k, walla) in m:\n        pass\n".into(), Skin::En),
        (
            "cross-skin names in use/as and impl/as",
            "use a/b.{c as ka}\n\ntrait law:\n    fn walla(self) -> Int\n\nimpl fi as law:\n    fn walla(self) -> Int:\n        return 1\n".into(),
            Skin::En,
        ),
        ("cross-skin type and enum names", "type law:\n    walla: Int\n    fi: List[law]\n\nenum E:\n    law\n    walla\n".into(), Skin::En),
        ("not/and/or around cross-skin names", "fn f(law: Bool, fi: Bool):\n    let x = not law and law or fi\n".into(), Skin::En),
        (
            "Arabic text near the column limit",
            format!("دالة f():\n    إذا {} و {}:\n        تخطى\n", "q".repeat(40), "r".repeat(40)),
            Skin::Ar,
        ),
        (
            "multi-line string inside a broken call",
            format!("fn f():\n    g({z}, {z}, \"\"\"\n        hello {{x}}\n          world # not a comment\n        \"\"\")\n"),
            Skin::En,
        ),
    ];
    for (name, src, skin) in &cases {
        let result = std::panic::catch_unwind(|| {
            let f = battery(src, *skin, false);
            battery(&f, *skin, true);
        });
        assert!(result.is_ok(), "case failed: {name}");
    }
}
