//! Spec §28: 8 programs × 3 skins.
//!
//! - `[converter]` blocks (§28.1, §28.2) must give exactly the English AST.
//! - `[view: glossary]` blocks (§28.3–§28.8) show identifiers through a display glossary
//!   (spec §4.5), so identifier *text* differs by design. They must still parse cleanly and
//!   give the English AST once identifier text is erased. A consistent renaming is not
//!   possible either: in §28.8 the English label `x` is both the record field (Arabic `س`)
//!   and the stdlib label `x` of `draw.rect`, which the glossary keeps.

mod common;

use common::*;
use kalam_ast::{shape, ShapeOptions};
use kalam_base::Skin;

#[test]
fn spec_28_examples_parse_to_the_same_ast_in_every_skin() {
    let blocks = spec_section_28();
    assert_eq!(blocks.len(), 24, "8 programs × 3 skins");

    let mut failures = Vec::new();
    let mut strict = 0;
    let mut erased = 0;
    for chunk in blocks.chunks(3) {
        assert_eq!(
            chunk[0].skin,
            Skin::En,
            "{}: English block first",
            chunk[0].file
        );
        let parsed: Vec<_> = chunk
            .iter()
            .map(|b| {
                let (out, lex) = parse_text(&b.body, b.skin);
                (b, out, lex)
            })
            .collect();
        for (b, out, lex) in &parsed {
            if !lex.is_empty() {
                // Lexer findings are the lexer's business; report them, do not fail here.
                eprintln!(
                    "note: §{} {}: lexer diagnostics {:?}",
                    b.section, b.file, lex
                );
            }
            if !out.diagnostics.is_empty() {
                failures.push(format!(
                    "§{} {}: parser diagnostics {:?}",
                    b.section, b.file, out.diagnostics
                ));
            }
            if out.cst.text() != b.body {
                failures.push(format!("§{} {}: CST not lossless", b.section, b.file));
            }
        }
        let opts_for = |glossary: bool| ShapeOptions {
            erase_idents: glossary,
            skip_trivia: false,
        };
        for (b, out, _) in &parsed[1..] {
            let opts = opts_for(b.glossary);
            let en = shape(&parsed[0].1.ast, opts);
            let other = shape(&out.ast, opts);
            if b.glossary {
                erased += 1;
            } else {
                strict += 1;
            }
            if en != other {
                failures.push(format!(
                    "§{} {} differs from English (glossary={}):\n--- en\n{}\n--- {}\n{}",
                    b.section, b.file, b.glossary, en, b.file, other
                ));
            }
        }
    }
    eprintln!("§28: {strict} strict comparisons, {erased} identifier-erased comparisons");
    assert_eq!(
        strict, 4,
        "§28.1 and §28.2 are converter output (2 programs × 2 skins)"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
