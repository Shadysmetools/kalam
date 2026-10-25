//! Error recovery and parse-error codes (PARSE-CODES.md). The parser never panics, reports
//! each error once, resynchronizes at the next NEWLINE/DEDENT, and keeps every byte in the CST.

mod common;

use common::*;
use kalam_base::Skin;
use kalam_parser::codes;

fn errs(src: &str) -> (Vec<&'static str>, String) {
    let (out, _) = parse_text(src, Skin::En);
    assert_eq!(out.cst.text(), src, "CST must stay lossless under errors");
    (parser_codes(&out), shape_of(&out))
}

#[test]
fn bad_statement_is_reported_once_and_parsing_resumes() {
    let (c, s) = errs("fn main():\n    let = 5\n    print(\"ok\")\n");
    assert_eq!(c, vec![codes::UNEXPECTED_TOKEN]);
    assert!(s.contains("(error)"), "{s}");
    assert!(
        s.contains("(call CORE_PRINT (args (arg (str \"ok\"))))"),
        "{s}"
    );
}

#[test]
fn garbage_header_skips_its_block() {
    let (c, s) = errs("fn main():\n    if x y:\n        a()\n        b()\n    done()\n");
    assert_eq!(c, vec![codes::UNEXPECTED_TOKEN], "{s}");
    assert!(s.contains("\"done\""), "{s}");
}

#[test]
fn missing_block_after_colon() {
    let (c, s) = errs("fn main():\nfn g():\n    pass\n");
    assert_eq!(c, vec![codes::EXPECTED_BLOCK]);
    assert!(s.contains("(sig \"g\""), "{s}");
}

#[test]
fn unclosed_bracket_at_eof() {
    let (c, _) = errs("fn main():\n    print(\"x\"\n");
    assert_eq!(c, vec![codes::UNCLOSED_DELIMITER]);
}

#[test]
fn statements_are_not_items() {
    let (c, s) = errs("print(\"x\")\nfn main():\n    pass\n");
    assert_eq!(c, vec![codes::EXPECTED_ITEM]);
    assert!(s.contains("CORE_MAIN"), "{s}");
}

#[test]
fn use_after_item() {
    let (c, s) = errs("fn main():\n    pass\nuse std/json\n");
    assert_eq!(c, vec![codes::USE_AFTER_ITEM]);
    assert!(s.contains("(use \"std/json\")"), "{s}");
}

#[test]
fn positional_after_named_argument() {
    let (c, _) = errs("fn main():\n    f(a: 1, 2)\n");
    assert_eq!(c, vec![codes::POSITIONAL_AFTER_NAMED]);
}

#[test]
fn spec_coded_syntax_errors() {
    assert_eq!(errs("fn main():\n    let r = a < b < c\n").0, vec!["K0720"]);
    assert_eq!(errs("fn main():\n    f() = 1\n").0, vec!["K0710"]);
    assert_eq!(errs("fn main():\n    let x = if a: 1\n").0, vec!["K0731"]);
    assert_eq!(errs("fn main():\n    f(if a: 1)\n").0, vec!["K0731"]);
    assert_eq!(
        errs("fn main():\n    let x = if a:\n        1\n").0,
        vec!["K0731"]
    );
    assert_eq!(errs("fn main():\n    xs.map(fn(x): x)\n").0, vec!["K0921"]);
}

#[test]
fn bad_holes() {
    assert_eq!(
        errs("fn main():\n    print(\"{}\")\n").0,
        vec![codes::BAD_HOLE]
    );
    assert_eq!(
        errs("fn main():\n    print(\"{a b}\")\n").0,
        vec![codes::BAD_HOLE]
    );
    // An unclosed hole keeps the lexer's string open (K0131 for the `"`, K0129 at the line
    // end); the parser adds no hole error of its own. The swallowed `)` then leaves the call
    // unclosed (K0251), a genuine consequence of the unterminated string.
    let (out, lex) = parse_text("fn main():\n    print(\"{a\")\n", Skin::En);
    assert!(lex.contains(&"K0129"), "{lex:?}");
    assert_eq!(parser_codes(&out), vec![codes::UNCLOSED_DELIMITER]);
}

#[test]
fn unexpected_indent_is_k0120() {
    let (c, s) = errs("fn main():\n    a()\n        b()\n    c()\n");
    assert_eq!(c, vec!["K0120"]);
    assert!(s.contains("\"b\"") && s.contains("\"c\""), "{s}");
}

#[test]
fn every_parse_code_is_documented_for_the_spec_owner() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("PARSE-CODES.md"),
    )
    .expect("PARSE-CODES.md next to the crate");
    for code in codes::ALL {
        assert!(
            doc.contains(&format!("| {code} |")),
            "{code} has no row in PARSE-CODES.md"
        );
    }
}

#[test]
fn deep_nesting_is_an_error_not_a_stack_overflow() {
    // Run on a 1 MiB stack: the Windows main-thread default.
    let handle = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            let mut src = String::from("fn main():\n    let x = ");
            src.push_str(&"(".repeat(20_000));
            src.push('1');
            src.push_str(&")".repeat(20_000));
            src.push('\n');
            let (c, _) = errs(&src);
            assert!(c.contains(&codes::TOO_DEEP), "{c:?}");

            let mut src = String::from("fn main():\n    let x = ");
            src.push_str(&"-not ".repeat(5_000));
            src.push_str("1\n");
            let _ = errs(&src);

            let mut src = String::from("fn main():\n");
            for depth in 1..300 {
                src.push_str(&"    ".repeat(depth));
                src.push_str("if x:\n");
            }
            src.push_str(&"    ".repeat(300));
            src.push_str("pass\n");
            let (c, _) = errs(&src);
            assert!(c.contains(&codes::TOO_DEEP), "{c:?}");
        })
        .unwrap();
    handle.join().expect("no panic, no overflow");
}
