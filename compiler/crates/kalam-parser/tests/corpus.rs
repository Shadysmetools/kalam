//! Every corpus program (eval/corpus, all cases, all three skins) parses with no parser
//! diagnostics, round-trips byte for byte through the CST, and gives the SAME skin-free AST
//! in `en`, `ar` and `franco` (spec §4.3 G1, DECISIONS D6).

mod common;

use common::*;
use kalam_base::Skin;

#[test]
fn corpus_parses_losslessly_to_one_ast_per_case() {
    let cases = corpus_cases();
    assert!(
        cases.len() >= 14,
        "expected the 14 written corpus cases, found {}",
        cases.len()
    );

    let mut files = 0;
    let mut failures = Vec::new();
    for case in &cases {
        assert_eq!(
            case.files.len(),
            3,
            "{}: every case exists in all three skins",
            case.name
        );
        let mut shapes: Vec<(Skin, String)> = Vec::new();
        for (skin, path) in &case.files {
            files += 1;
            let text = std::fs::read_to_string(path).unwrap();
            let (out, _lex) = parse_text(&text, *skin);
            let codes = parser_codes(&out);
            if !codes.is_empty() {
                failures.push(format!(
                    "{}/{}: parser diagnostics {:?}",
                    case.name, skin, out.diagnostics
                ));
            }
            if out.cst.text() != text {
                failures.push(format!("{}/{}: CST is not lossless", case.name, skin));
            }
            shapes.push((*skin, shape_of(&out)));
        }
        let (_, en) = &shapes[0];
        for (skin, s) in &shapes[1..] {
            if s != en {
                failures.push(format!(
                    "{}: {} AST differs from en\n--- en\n{}\n--- {}\n{}",
                    case.name, skin, en, skin, s
                ));
            }
        }
    }
    eprintln!("corpus: {} cases, {} files parsed", cases.len(), files);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn corpus_ast_contains_expected_structure() {
    // Spot checks that the shape is a real tree, not an empty file.
    let text =
        std::fs::read_to_string(repo_root().join("eval/corpus/t12-shapes-match/en.kalam")).unwrap();
    let (out, _) = parse_text(&text, Skin::En);
    let s = shape_of(&out);
    assert!(s.contains("(enum \"Shape\""), "{s}");
    assert!(s.contains("(return (match"), "{s}");
    assert!(
        s.contains("(case (. \"Rect\" ((pf (bind \"w\"))(pf (bind \"h\")))) if (== \"w\" \"h\")"),
        "{s}"
    );
    assert!(
        s.contains("(or (. \"Rect\" ((pf _)(pf _))) (. \"Square\" ((pf _))))"),
        "{s}"
    );
    assert!(s.contains("(. \"Triangle\" ( ..))"), "{s}");
    assert!(s.contains("(if-inline"), "{s}");
}
