//! The printer: formatting, lossless skin conversion and read-only views.
//!
//! One printer serves `kalam fmt` (print in the file's own skin, with layout normalized),
//! `kalam convert --to X` (print in skin X, layout unchanged), `kalam view --skin X
//! [--glossary]` and `kalam diff --skin X` (ARCHITECTURE.md §13, spec §4.3 and §21.1).
//!
//! What changes between skins: keywords and core names (concept → target spelling), `,`/`?`
//! ↔ `،`/`؟` (always `،`/`؟` in `ar`), and digit glyphs (`ar` target only, per
//! `[skin.ar] digits`). Everything else is copied byte for byte: identifiers, strings,
//! comments, blank lines, attribute names and module paths. An identifier that is reserved
//! in the target skin is written in backticks, and backticks that are no longer needed are
//! removed.
//!
//! Status: [`convert`] works at token level on top of the lexer. Phase 0 replaces it with the
//! CST printer, which also converts code inside interpolation holes (a token-level converter
//! sees a string as one token) and applies the formatter rules.

use kalam_base::{FileId, Skin};
use kalam_diagnostics::Diagnostic;
use kalam_lexer::{lex_with, to_ascii_digits, Op, TokenKind};
use kalam_skins::{Concept, KeywordTable};

/// Digit glyphs the `ar` printer emits (`[skin.ar] digits` in `kalam.toml`, spec §6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ArDigits {
    #[default]
    Ascii,
    ArabicIndic,
}

/// Every setting that affects printed output. It is project-level only, never per file,
/// so the fixpoint guarantee G2 holds for a fixed configuration (spec §4.3).
#[derive(Clone, Copy, Debug, Default)]
pub struct FmtConfig {
    pub ar_digits: ArDigits,
}

/// Converts a file from one skin to another with the compiler's embedded keyword table.
/// Returns the source's diagnostics instead if it does not lex cleanly.
pub fn convert(text: &str, from: Skin, to: Skin, config: &FmtConfig) -> Result<String, Vec<Diagnostic>> {
    convert_with(text, from, to, config, KeywordTable::builtin())
}

pub fn convert_with(
    text: &str,
    from: Skin,
    to: Skin,
    config: &FmtConfig,
    table: &KeywordTable,
) -> Result<String, Vec<Diagnostic>> {
    let lexed = lex_with(text, from, FileId(0), table);
    if lexed.has_errors() {
        return Err(lexed.diagnostics);
    }
    let target = table.skin(to);
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut copied_to = 0usize;
    let mut prev: Option<TokenKind> = None;

    for token in &lexed.tokens {
        let original = &text[token.span.range()];
        let after_dot = matches!(prev, Some(TokenKind::Op(Op::Dot)));
        let replacement = match token.kind {
            TokenKind::Keyword(concept) | TokenKind::Core(concept) => {
                Some(table.spelling(concept, to).to_string())
            }
            TokenKind::Op(Op::Comma) => Some(table.spelling(Concept::PunctComma, to).to_string()),
            TokenKind::Op(Op::Question) => Some(table.spelling(Concept::PunctTry, to).to_string()),
            TokenKind::Ident => {
                let name = if token.flags.raw_ident {
                    &original[1..original.len() - 1] // backticks are one byte each
                } else {
                    original
                };
                if !after_dot && target.is_reserved(name) {
                    Some(format!("`{name}`"))
                } else {
                    Some(name.to_string())
                }
            }
            TokenKind::Int | TokenKind::Float if !after_dot => Some(convert_digits(original, to, config)),
            // Strings, tagged strings, member indices and layout tokens are copied as they are.
            _ => None,
        };
        if let Some(text_out) = replacement {
            out.push_str(&text[copied_to..token.span.start as usize]);
            out.push_str(&text_out);
            copied_to = token.span.end as usize;
        }
        prev = Some(token.kind);
    }
    out.push_str(&text[copied_to..]);
    Ok(out)
}

/// Re-spells a numeric literal for the target skin. KCF stores values, not glyphs, so this
/// never changes a program's identity.
fn convert_digits(literal: &str, to: Skin, config: &FmtConfig) -> String {
    let ascii = to_ascii_digits(literal);
    let want_arabic = to == Skin::Ar && config.ar_digits == ArDigits::ArabicIndic;
    let plain_decimal = ascii.chars().all(|c| c.is_ascii_digit() || c == '_' || c == '.');
    if !want_arabic || !plain_decimal {
        // Hex, binary, octal and exponent forms are ASCII in every skin (K0128).
        return ascii;
    }
    ascii
        .chars()
        .map(|c| match c {
            '0'..='9' => char::from_u32(0x0660 + (c as u32 - '0' as u32)).expect("Arabic-Indic digit"),
            '.' => '\u{066B}',
            other => other,
        })
        .collect()
}

/// `kalam fmt`: canonical formatting in the file's own skin (spec §21.1). Phase 0.
pub fn format(_text: &str, _skin: Skin, _config: &FmtConfig) -> Result<String, Vec<Diagnostic>> {
    todo!("CST formatter (Phase 0, step 3)")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: [&str; 3] = [
        include_str!("../../../testdata/skins/hello.kl"),
        include_str!("../../../testdata/skins/hello.ar.kl"),
        include_str!("../../../testdata/skins/hello.franco.kl"),
    ];
    const FIZZ: [&str; 3] = [
        include_str!("../../../testdata/skins/fizzbuzz.kl"),
        include_str!("../../../testdata/skins/fizzbuzz.ar.kl"),
        include_str!("../../../testdata/skins/fizzbuzz.franco.kl"),
    ];

    fn conv(text: &str, from: Skin, to: Skin) -> String {
        convert(text, from, to, &FmtConfig::default()).expect("source lexes cleanly")
    }

    #[test]
    fn spec_examples_convert_exactly_between_all_skins() {
        for program in [HELLO, FIZZ] {
            for from in Skin::ALL {
                for to in Skin::ALL {
                    assert_eq!(
                        conv(program[from.index()], from, to),
                        program[to.index()],
                        "{from} -> {to}"
                    );
                }
            }
        }
    }

    #[test]
    fn reserved_identifiers_are_escaped_and_unescaped() {
        let franco = conv("law = 1\n", Skin::En, Skin::Franco);
        assert_eq!(franco, "`law` = 1\n");
        assert_eq!(conv(&franco, Skin::Franco, Skin::En), "law = 1\n");
        // After `.`, a reserved word needs no escape.
        assert_eq!(conv("contract.law\n", Skin::En, Skin::Franco), "contract.law\n");
    }

    #[test]
    fn arabic_punctuation_and_digits() {
        assert_eq!(conv("f(a, b)?\n", Skin::En, Skin::Ar), "f(a\u{060C} b)\u{061F}\n");
        let config = FmtConfig {
            ar_digits: ArDigits::ArabicIndic,
        };
        let ar = convert("x = 1.5 + 0x1F\n", Skin::En, Skin::Ar, &config).unwrap();
        assert_eq!(ar, "x = \u{0661}\u{066B}\u{0665} + 0x1F\n");
        assert_eq!(conv(&ar, Skin::Ar, Skin::En), "x = 1.5 + 0x1F\n");
    }

    #[test]
    fn source_with_errors_is_not_converted() {
        assert!(convert("3ala = 1\n", Skin::Franco, Skin::En, &FmtConfig::default()).is_err());
    }
}
