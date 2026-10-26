//! Adversarial inputs (independent verifier, M1): hostile layout, Arabic/Franco edge cases,
//! Trojan-Source characters, pathological nesting and very large inputs.
//!
//! Every case must hold four things:
//! - no panic, on a 1 MiB stack (the Windows main-thread default);
//! - `cst.text() == source`;
//! - the expected lexer and parser codes;
//! - each K0250 carries its `expected` and `found` args, so a message can be rendered.

mod common;

use std::panic::{catch_unwind, AssertUnwindSafe};

use common::*;
use kalam_base::Skin;
use kalam_parser::{codes, ParseOutput};

/// The outcome of one adversarial parse.
struct Run {
    lex: Vec<&'static str>,
    parse: Vec<&'static str>,
    shape: String,
    out: ParseOutput,
}

/// Parses `src` on a 1 MiB thread, checking that nothing panics, the CST is lossless and
/// each K0250 has its args. A second parse is dropped on that thread too.
///
/// The test's shape dump (`kalam_ast::shape`) runs on an 8 MiB thread, because it is not
/// the parser's code and it needs more than 1 MiB near the height cap. See
/// `shape_of_a_max_height_tree_fits_a_1mib_stack` below.
fn run(src: &str, skin: Skin) -> Run {
    let owned = src.to_string();
    let (out, lex) = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let (out, lex) = catch_unwind(AssertUnwindSafe(|| parse_text(&owned, skin)))
                .unwrap_or_else(|_| panic!("parser panicked on {owned:?}"));
            assert_eq!(out.cst.text(), owned, "CST must be lossless");
            for d in &out.diagnostics {
                if d.code == codes::UNEXPECTED_TOKEN {
                    let names: Vec<_> = d.args.iter().map(|(k, _)| *k).collect();
                    assert!(
                        names.contains(&"expected") && names.contains(&"found"),
                        "K0250 without expected/found args: {:?}",
                        d.args
                    );
                }
            }
            drop(parse_text(&owned, skin));
            (out, lex)
        })
        .unwrap()
        .join()
        .expect("no panic, no stack overflow on a 1 MiB stack");
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || Run {
            lex,
            parse: parser_codes(&out),
            shape: shape_of(&out),
            out,
        })
        .unwrap()
        .join()
        .unwrap()
}

fn codes_of(src: &str, skin: Skin) -> (Vec<&'static str>, Vec<&'static str>) {
    let r = run(src, skin);
    (r.lex, r.parse)
}

const NONE: Vec<&'static str> = Vec::new();

#[test]
fn empty_and_blank_files() {
    for src in [
        "",
        "\n\n\n",
        "\r\n\r\n",
        "# only a comment",
        "    ",
        "\n    \n",
    ] {
        assert_eq!(codes_of(src, Skin::En), (NONE, NONE), "{src:?}");
    }
    assert_eq!(run("", Skin::Ar).shape, "(file)");
}

#[test]
fn crlf_line_endings_parse_like_lf() {
    let lf = "fn main():\n    if x:\n        print(\"x\")\n    pass\n";
    let crlf = lf.replace('\n', "\r\n");
    let (a, b) = (run(lf, Skin::En), run(&crlf, Skin::En));
    assert_eq!((b.lex.clone(), b.parse.clone()), (NONE, NONE));
    assert_eq!(a.shape, b.shape, "CRLF must give the LF tree");
    // No final newline, and an Arabic file.
    assert_eq!(codes_of("fn main():\r\n    pass", Skin::En), (NONE, NONE));
    assert_eq!(
        codes_of("دالة رئيسي():\r\n    اطبع(\"س\")\r\n", Skin::Ar),
        (NONE, NONE)
    );
}

#[test]
fn lone_cr_and_unicode_line_separators_are_sec0113() {
    for src in [
        "fn main():\r    pass\n",
        "fn main():\u{2028}    pass\n",
        "fn main():\u{2029}    pass\n",
    ] {
        let (lex, parse) = codes_of(src, Skin::En);
        assert_eq!(lex, vec!["SEC0113"], "{src:?}");
        assert_eq!(parse, NONE, "{src:?}");
    }
}

#[test]
fn tabs_and_mixed_indentation() {
    assert_eq!(
        codes_of("fn main():\n\tpass\n", Skin::En),
        (vec!["K0103"], NONE)
    );
    // A tab hidden among spaces on a deeper line: the tab and the bad indent are both reported,
    // and both statements are kept.
    let r = run("fn main():\n    a()\n  \t  b()\n", Skin::En);
    assert_eq!((r.lex, r.parse), (vec!["K0103"], vec!["K0120"]));
    assert!(
        r.shape.contains("\"a\"") && r.shape.contains("\"b\""),
        "{}",
        r.shape
    );
}

#[test]
fn bad_indent_unit_and_bad_dedent() {
    assert_eq!(
        codes_of("fn main():\n  pass\n", Skin::En),
        (vec!["K0120"], NONE)
    );
    let r = run("fn main():\n    if x:\n        a()\n      b()\n", Skin::En);
    assert_eq!((r.lex, r.parse), (vec!["K0122"], NONE));
    assert!(r.shape.contains("\"b\""), "{}", r.shape);
    // The whole file indented: one K0120, and the function is still parsed.
    let r = run("    fn main():\n        pass\n", Skin::En);
    assert_eq!(r.parse, vec!["K0120"]);
    assert!(r.shape.contains("CORE_MAIN"), "{}", r.shape);
}

#[test]
fn unterminated_blocks_at_eof() {
    for src in [
        "fn main():\n",
        "fn main():",
        "fn main():\n    if x:\n",
        "fn main():\n    while x:",
        "type T:\n",
        "fn main():\n    match x:\n    g()\n",
    ] {
        assert_eq!(
            codes_of(src, Skin::En),
            (NONE, vec![codes::EXPECTED_BLOCK]),
            "{src:?}"
        );
    }
    // Arabic and Franco skins behave the same.
    assert_eq!(
        codes_of("دالة رئيسي():\n    إذا صح:\n", Skin::Ar),
        (NONE, vec![codes::EXPECTED_BLOCK])
    );
    assert_eq!(
        codes_of("dala ra2isi():\n    law sa7:", Skin::Franco),
        (NONE, vec![codes::EXPECTED_BLOCK])
    );
}

#[test]
fn unclosed_and_stray_brackets() {
    assert_eq!(
        codes_of("fn main():\n    f(1,\n      2,\n", Skin::En),
        (NONE, vec![codes::UNCLOSED_DELIMITER])
    );
    // A stray `)` is one error and the next statement survives.
    let r = run("fn main():\n    f(1))\n    g()\n", Skin::En);
    assert_eq!(r.parse, vec![codes::UNEXPECTED_TOKEN]);
    assert!(r.shape.contains("(call \"g\""), "{}", r.shape);
    // Closers at module level: one K0255 per line.
    assert_eq!(
        codes_of(")\n]\n}\n", Skin::En).1,
        vec![codes::EXPECTED_ITEM; 3]
    );
}

#[test]
fn bidi_controls_and_bom_are_lexer_errors_not_parse_errors() {
    for src in [
        "fn main():\n    let x\u{202E} = 1\n",
        "fn main():\n    print(\"a\u{2067}b\")\n",
        "# \u{202E}evil\nfn main():\n    pass\n",
        "fn main():\n    let x\u{200F} = 1\n",
        "fn main():\n    let x\u{061C} = 1\n",
    ] {
        assert_eq!(codes_of(src, Skin::En), (vec!["SEC0110"], NONE), "{src:?}");
    }
    assert_eq!(
        codes_of("دالة رئيسي():\n    اطبع(\"\u{202E}س\")\n", Skin::Ar),
        (vec!["SEC0110"], NONE)
    );
    assert_eq!(
        codes_of("\u{FEFF}fn main():\n    pass\n", Skin::En),
        (vec!["SEC0100"], NONE)
    );
}

#[test]
fn other_invisible_and_control_characters() {
    // NUL, NBSP as indentation, ZWJ inside a name: each reported by the lexer, no panic.
    assert_eq!(
        codes_of("fn main():\n    let x\0 = 1\n", Skin::En).0,
        vec!["SEC0102"]
    );
    assert!(codes_of("fn main():\n\u{a0}   pass\n", Skin::En)
        .0
        .contains(&"SEC0103"));
    assert!(codes_of("fn main():\n    let a\u{200D}b = 1\n", Skin::En)
        .0
        .contains(&"SEC0111"));
}

#[test]
fn franco_digit_leading_names_like_3ala() {
    // `3ala` (§5.3, X5) is one K0127 from the lexer. The parser adds nothing.
    assert_eq!(
        codes_of("dala ra2isi():\n    khali 3ala = 1\n", Skin::Franco),
        (vec!["K0127"], NONE)
    );
    // The same name in the English and Arabic skins.
    assert_eq!(
        codes_of("fn main():\n    let 3ala = 1\n", Skin::En),
        (vec!["K0127"], NONE)
    );
    assert_eq!(
        codes_of("دالة رئيسي():\n    ليكن ١س = 1\n", Skin::Ar).0,
        vec!["K0127"]
    );
    // A digit-leading spelling of a keyword (`5ali`, DR-03) is never accepted.
    let (lex, parse) = codes_of("dala ra2isi():\n    5ali x = 1\n", Skin::Franco);
    assert_eq!(lex, vec!["K0127"]);
    assert!(parse.len() <= 1, "{parse:?}");
    // A name glued to a number as a call argument: no cascade beyond the lexer error.
    assert_eq!(
        codes_of("dala ra2isi():\n    ektb(3ala)\n", Skin::Franco),
        (vec!["K0127"], NONE)
    );
}

#[test]
fn franco_aliases_and_franco_keywords_as_names() {
    // Aliases (`dalla`, `lw`, `wala`) are accepted and give the canonical tree.
    let alias = run(
        "dalla ra2isi():\n    lw sa7:\n        ektb(\"x\")\n    wala:\n        fot\n",
        Skin::Franco,
    );
    let canon = run(
        "dala ra2isi():\n    law sa7:\n        ektb(\"x\")\n    walla:\n        fot\n",
        Skin::Franco,
    );
    assert_eq!((alias.lex.clone(), alias.parse.clone()), (NONE, NONE));
    assert_eq!(alias.shape, canon.shape);
    // A short Franco keyword used as a name (`fi` = `in`) is an error, reported once.
    assert_eq!(
        codes_of("dala ra2isi():\n    khali fi = 1\n", Skin::Franco),
        (NONE, vec![codes::UNEXPECTED_TOKEN])
    );
}

#[test]
fn keywords_from_another_skin_are_errors_and_parsing_resumes() {
    // Spec §21 rule 6 asks for K0201 here. Today the lexer gives K0201 only for punctuation,
    // so the parser's K0250 is the only report. This test pins "reported, and recovered",
    // not which code.
    for (src, skin) in [
        (
            "دالة رئيسي():\n    if صح:\n        اطبع(\"x\")\n    اطبع(\"y\")\n",
            Skin::Ar,
        ),
        (
            "دالة رئيسي():\n    law صح:\n        تخطى\n    اطبع(\"y\")\n",
            Skin::Ar,
        ),
        (
            "dala ra2isi():\n    إذا sa7:\n        fot\n    ektb(\"y\")\n",
            Skin::Franco,
        ),
        (
            "fn main():\n    إذا true:\n        pass\n    print(\"y\")\n",
            Skin::En,
        ),
    ] {
        let r = run(src, skin);
        // 1 today (K0250); 2 once the lexer adds K0201. Never silent, never a cascade.
        assert!(
            (1..=2).contains(&(r.lex.len() + r.parse.len())),
            "{src:?}: {:?} {:?}",
            r.lex,
            r.parse
        );
        assert!(r.shape.contains("(str \"y\")"), "{src:?}: {}", r.shape);
    }
    // Arabic punctuation in a Franco or English file is K0201 and parses as its ASCII twin.
    assert_eq!(
        codes_of("dala ra2isi():\n    f(1، 2)\n", Skin::Franco),
        (vec!["K0201"], NONE)
    );
    assert_eq!(
        codes_of("fn main():\n    let x = f()\u{061F}\n", Skin::En),
        (vec!["K0201"], NONE)
    );
}

#[test]
fn arabic_punctuation_digits_and_marks() {
    let r = run("دالة رئيسي():\n    ليكن x = f(1، 2)؟\n", Skin::Ar);
    assert_eq!((r.lex, r.parse), (NONE, NONE));
    assert!(r.shape.contains("(? (call \"f\""), "{}", r.shape);
    let r = run("دالة رئيسي():\n    ليكن x = ١٢٣ + ٤٫٥\n", Skin::Ar);
    assert_eq!((r.lex, r.parse), (NONE, NONE));
    assert!(r.shape.contains("(+ (int 123) (float 4.5))"), "{}", r.shape);
    // Mixed digit systems in one literal.
    assert_eq!(
        codes_of("دالة رئيسي():\n    ليكن x = 1٢3\n", Skin::Ar),
        (vec!["SEC0120"], NONE)
    );
    // Harakat and tatweel inside a keyword: lexer errors, no parse cascade.
    for src in ["دَالة رئيسي():\n    تخطى\n", "دالـــة رئيسي():\n    تخطى\n"]
    {
        let (lex, parse) = codes_of(src, Skin::Ar);
        assert!(
            !lex.is_empty() && lex.iter().all(|c| *c == "SEC0106"),
            "{lex:?}"
        );
        assert_eq!(parse, NONE);
    }
}

#[test]
fn keyword_labels_in_every_skin() {
    assert_eq!(
        codes_of("fn main():\n    f(match: 1, if: 2)\n", Skin::En),
        (NONE, NONE)
    );
    assert_eq!(
        codes_of("دالة رئيسي():\n    f(حسب: 1)\n", Skin::Ar),
        (NONE, NONE)
    );
    // A keyword as a binding name is not a label: one error.
    assert_eq!(
        codes_of("fn main():\n    let let = 1\n", Skin::En),
        (NONE, vec![codes::UNEXPECTED_TOKEN])
    );
}

#[test]
fn leading_dot_edge_cases() {
    // Same indentation: a new statement (DR-05), not a continuation.
    let r = run("fn main():\n    x\n    .foo()\n", Skin::En);
    assert_eq!((r.lex, r.parse), (NONE, NONE));
    assert!(
        r.shape.contains("(expr \"x\") (expr (call (dot \"foo\")"),
        "{}",
        r.shape
    );
    // First line of a block never joins its header.
    assert_eq!(
        codes_of("fn main():\n    if x:\n        .foo()\n", Skin::En),
        (NONE, NONE)
    );
    // At module level, and a lone `.` at EOF.
    assert_eq!(
        codes_of(".foo()\n", Skin::En),
        (NONE, vec![codes::EXPECTED_ITEM])
    );
    assert_eq!(
        codes_of("fn main():\n    x\n        .", Skin::En),
        (NONE, vec![codes::UNEXPECTED_TOKEN])
    );
    // Uneven but deeper `.` lines are one chain (eval/suites/conformance/leading-dot-chain):
    // the lexer joins them without touching the indent stack.
    let src = "fn main():\n    let xs = [1, 2, 3, 4]\n    let n = xs\n            .filter(fn(x) => x > 1)\n        .map(fn(x) => x * 2)\n            .len()\n    print(\"{n}\")\n";
    let r = run(src, Skin::En);
    assert_eq!((r.lex, r.parse), (NONE, NONE));
    let flat = run(
        "fn main():\n    let xs = [1, 2, 3, 4]\n    let n = xs.filter(fn(x) => x > 1).map(fn(x) => x * 2).len()\n    print(\"{n}\")\n",
        Skin::En,
    );
    assert_eq!(r.shape, flat.shape);
    // A deeper line that does not start with `.` after a chain is still an unexpected indent.
    assert_eq!(
        codes_of(
            "fn main():\n    let n = xs\n        .len()\n        g()\n",
            Skin::En
        ),
        (NONE, vec![codes::UNEXPECTED_INDENT])
    );
}

#[test]
fn interpolation_holes_are_not_checked_for_undefined_names() {
    // FOUNDER-QUEUE Q9: hole code is re-lexed alone, so the undefined-name half of K0201 is
    // not decided there, in one-line and multi-line (`"""`) holes alike.
    for src in [
        "fn main():\n    let law = 1\n    print(\"{law}\")\n",
        "fn main():\n    let law = 1\n    print(\"\"\"{law +\n        1}\"\"\")\n",
        "fn main():\n    print(\"{kfaya}\")\n",
        "fn main():\n    print(\"\"\"{kfaya}\"\"\")\n",
        "fn main():\n    print(\"\"\"{1 +\n        kfaya}\"\"\")\n",
    ] {
        assert_eq!(codes_of(src, Skin::En), (NONE, NONE), "{src:?}");
    }
}

#[test]
fn strings_left_open() {
    let (lex, parse) = codes_of("fn main():\n    print(\"abc\n    g()\n", Skin::En);
    assert_eq!(lex, vec!["K0129"]);
    assert!(parse.contains(&codes::UNCLOSED_DELIMITER), "{parse:?}");
    assert_eq!(
        codes_of("fn main():\n    let s = \"\"\"abc\n", Skin::En),
        (vec!["K0129"], NONE)
    );
    assert_eq!(
        codes_of("fn main():\n    print(\"{\"a\"}\")\n", Skin::En),
        (vec!["K0131"], NONE)
    );
}

#[test]
fn malformed_headers_and_bodies() {
    assert_eq!(
        codes_of("fn ():\n    pass\n", Skin::En),
        (NONE, vec![codes::UNEXPECTED_TOKEN])
    );
    assert_eq!(
        codes_of("fn main() -> -> ->:\n    pass\n", Skin::En),
        (NONE, vec![codes::UNEXPECTED_TOKEN])
    );
    assert_eq!(
        codes_of("enum E:\n    | | |\n", Skin::En),
        (NONE, vec![codes::UNEXPECTED_TOKEN])
    );
    assert_eq!(
        codes_of(":::\n::\n", Skin::En),
        (NONE, vec![codes::EXPECTED_ITEM; 2])
    );
    assert_eq!(
        codes_of("return 1\n", Skin::En),
        (NONE, vec![codes::EXPECTED_ITEM])
    );
    for src in [
        "fn main():\n    else:\n        pass\n",
        "fn main():\n    case 1:\n        pass\n",
    ] {
        assert_eq!(
            codes_of(src, Skin::En),
            (NONE, vec![codes::UNEXPECTED_TOKEN]),
            "{src:?}"
        );
    }
    // A bad match arm is skipped; the good arm after it is kept.
    let r = run(
        "fn main():\n    match x:\n        foo bar\n        case 1:\n            pass\n",
        Skin::En,
    );
    assert_eq!(r.parse, vec![codes::UNEXPECTED_TOKEN]);
    assert!(r.shape.contains("(case (int 1)"), "{}", r.shape);
    // Lambdas without `=>`: one report per missing arrow.
    assert_eq!(
        codes_of("fn main():\n    f(fn(x) fn(y) fn(z) x)\n", Skin::En).1,
        vec![codes::UNEXPECTED_TOKEN; 3]
    );
    // An over-long integer is the type checker's K0126, not a parse error.
    assert_eq!(
        codes_of(
            "fn main():\n    let x = 99999999999999999999999999999999999999\n",
            Skin::En
        ),
        (NONE, NONE)
    );
}

#[test]
fn pathological_nesting_on_a_small_stack() {
    let cases = [
        format!(
            "fn main():\n    let x = {}1{}\n",
            "[".repeat(5_000),
            "]".repeat(5_000)
        ),
        format!("fn main():\n    let x = {}\n", "(".repeat(50_000)),
        format!("fn main():\n    let x = 1{}\n", " + 1".repeat(50_000)),
        format!("fn main():\n    let x = a{}\n", ".b()".repeat(20_000)),
        format!("fn main():\n    let x = {}1\n", "-".repeat(50_000)),
        format!(
            "fn main():\n    print(\"{}x{}\")\n",
            "{f(\"".repeat(200),
            "\")}".repeat(200)
        ),
        format!("fn main():\n{}", "    f(\n".repeat(20_000)),
    ];
    for src in &cases {
        let r = run(src, Skin::En);
        assert!(r.parse.contains(&codes::TOO_DEEP), "{:?}", r.parse);
        assert_eq!(
            r.parse.iter().filter(|c| **c == codes::TOO_DEEP).count(),
            1,
            "K0259 is reported once per file"
        );
    }
}

#[test]
fn depth_limit_is_one_diagnostic_at_its_spot() {
    // The token where nesting is cut gets K0259 only, never a second K0250 on the same span.
    for src in [
        format!(
            "fn main():\n    let x = {}1{}\n",
            "[".repeat(5_000),
            "]".repeat(5_000)
        ),
        format!("fn main():\n    let x = {}\n", "(".repeat(50_000)),
        format!(
            "fn main():\n    print(\"{}x{}\")\n",
            "{f(\"".repeat(200),
            "\")}".repeat(200)
        ),
    ] {
        let r = run(&src, Skin::En);
        let deep: Vec<_> = r
            .out
            .diagnostics
            .iter()
            .filter(|d| d.code == codes::TOO_DEEP)
            .map(|d| d.span)
            .collect();
        let dup: Vec<_> = r
            .out
            .diagnostics
            .iter()
            .filter(|d| d.code == codes::UNEXPECTED_TOKEN && deep.contains(&d.span))
            .collect();
        assert!(
            dup.is_empty(),
            "K0250 repeats the K0259 spot: {:?}",
            r.parse
        );
    }
    // Silencing the cut-off token must not hide an unclosed bracket.
    for src in [
        format!("fn main():\n    let x = {}\n", "(".repeat(50_000)),
        format!("fn main():\n{}", "    f(\n".repeat(20_000)),
    ] {
        let r = run(&src, Skin::En);
        assert!(
            r.parse.contains(&codes::TOO_DEEP) && r.parse.contains(&codes::UNCLOSED_DELIMITER),
            "{:?}",
            r.parse
        );
    }
}

#[test]
fn large_inputs_stay_linear_and_one_error_per_bad_line() {
    // 20 000 bad statements: exactly one report each, and the function survives.
    let src = "fn main():\n".to_string() + &"    let = 5\n".repeat(20_000);
    let r = run(&src, Skin::En);
    assert_eq!(r.parse.len(), 20_000);
    assert!(r.parse.iter().all(|c| *c == codes::UNEXPECTED_TOKEN));
    // 20 000 lines of bracket and operator soup.
    let r = run(&")(][}{:=>->,.?\n".repeat(20_000), Skin::En);
    assert!(!r.parse.is_empty());
    // A 60 000-line clean file and a 600 KB single line.
    let mut many = String::new();
    for i in 0..20_000 {
        many.push_str(&format!(
            "fn f{i}(a: Int) -> Int:\n    let x = a + {i}\n    return x * 2\n"
        ));
    }
    assert_eq!(codes_of(&many, Skin::En), (NONE, NONE));
    let long = format!("fn main():\n    let x = [{}]\n", "1, ".repeat(200_000));
    assert_eq!(codes_of(&long, Skin::En), (NONE, NONE));
}

/// Known issue, reported (not the parser's code): `kalam_ast::shape` recurses once per tree
/// level with a large debug-build frame. On a 1 MiB stack it overflows between 600 and 800
/// chained `+` terms, below the parser's `MAX_HEIGHT` (1024). kalam-eval calls `shape` on
/// the main thread (1 MiB on Windows), and a stack overflow aborts the process: no
/// `catch_unwind` can catch it. Run with `--ignored` to reproduce. That run aborts the test
/// binary until this is fixed.
#[test]
fn shape_of_a_max_height_tree_fits_a_1mib_stack() {
    let src = format!("fn main():\n    let x = 1{}\n", " + 1".repeat(800));
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let (out, _) = parse_text(&src, Skin::En);
            shape_of(&out).len()
        })
        .unwrap()
        .join()
        .expect("shape within 1 MiB");
}

/// FOUNDER-QUEUE Q5: a word glued to a string is a tagged literal (spec §5.6), and a tag
/// that is not one of the built-in tags (DR-29) is a parse error. Otherwise `foo"x"`, or
/// `return"x"` in an English file, checked clean and then could not run.
#[test]
fn an_unknown_tag_before_a_string_is_a_parse_error() {
    for src in [
        "fn main():\n    let a = foo\"x\"\n",
        "fn f() -> String:\n    return\"big\"\n",
        "fn main():\n    let a = my_tag\"\"\"\n    x\n    \"\"\"\n",
    ] {
        let r = run(src, Skin::En);
        assert_eq!(r.lex, NONE, "{src}");
        assert_eq!(r.parse, vec![codes::UNEXPECTED_TOKEN], "{src}");
    }
    // The built-in tags, and `f"…"` (K0913), are not affected.
    for tag in ["sql", "html", "cmd", "url", "path", "regex"] {
        let src = format!("fn main():\n    let a = {tag}\"x\"\n");
        assert_eq!(codes_of(&src, Skin::En), (NONE, NONE), "{src}");
    }
    assert_eq!(
        codes_of("fn main():\n    let a = f\"x\"\n", Skin::En),
        (NONE, vec![codes::F_STRING])
    );
    // A non-ASCII keyword glued to a string is that keyword and a string.
    assert_eq!(
        codes_of("دالة f() -> نص:\n    أرجع\"x\"\n", Skin::Ar),
        (NONE, NONE)
    );
}
