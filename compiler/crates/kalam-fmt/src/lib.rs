//! The printer: formatting and lossless skin conversion (ARCHITECTURE.md §13, spec §4.3,
//! §21.1).
//!
//! - [`format`] (`kalam fmt`) prints the AST in its one canonical layout, in the file's own
//!   skin: 4-space indentation, canonical spacing, 100-column line breaking, comments and
//!   blank lines re-attached, and every keyword in its canonical spelling (Franco variants,
//!   Arabic hamza/alef forms and Persian/Urdu letters, D19). Identifiers are never rewritten;
//!   unneeded backticks are removed. It is idempotent and keeps the AST ([`normalized_shape`]).
//! - [`convert`] (`kalam convert --to X`) re-spells a file in another skin and keeps its
//!   layout byte for byte, so line numbers never change. Converting a formatted file gives a
//!   formatted file, because line widths are measured on the English spelling in every skin.
//!
//! What changes between skins: keywords and core names (concept → target spelling), `,`/`?`
//! ↔ `،`/`؟` (always `،`/`؟` in `ar`), digit glyphs (`ar` target only, per
//! `[skin.ar] digits`), and code inside interpolation holes, which converts like any other
//! code. Everything else is copied byte for byte: identifiers, string text, comments, blank
//! lines, attribute names and module paths. An identifier that would lex as a reserved word of
//! the target skin is written in backticks, except where a reserved word is an ordinary
//! identifier (§5.3 rule 8: after `.`, labels, field names); backticks that are no longer
//! needed are removed. Both directions use one rule, so every round trip is byte-identical.

mod convert;
mod doc;
mod printer;
mod roles;
mod source;
mod verify;

use std::fmt;

use kalam_base::Skin;
use kalam_diagnostics::Diagnostic;
use kalam_lexer::to_ascii_digits;
use kalam_skins::KeywordTable;

pub use verify::{comment_texts, normalized_shape};

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
/// Returns the source's error diagnostics instead if it does not lex and parse cleanly.
pub fn convert(
    text: &str,
    from: Skin,
    to: Skin,
    config: &FmtConfig,
) -> Result<String, Vec<Diagnostic>> {
    convert_with(text, from, to, config, KeywordTable::builtin())
}

/// [`convert`] with an explicit keyword table for the target spellings.
pub fn convert_with(
    text: &str,
    from: Skin,
    to: Skin,
    config: &FmtConfig,
    table: &KeywordTable,
) -> Result<String, Vec<Diagnostic>> {
    let src = source::Source::parse(text, from)?;
    let roles = roles::Roles::of(&src.ast);
    let rw = convert::Rewriter {
        from,
        to,
        config,
        table,
        roles: &roles,
    };
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    rw.rewrite(text, &src.tokens, 0, 0..text.len(), &mut out);
    Ok(out)
}

/// Re-spells a numeric literal for the target skin. KCF stores values, not glyphs, so this
/// never changes a program's identity.
pub(crate) fn convert_digits(literal: &str, to: Skin, config: &FmtConfig) -> String {
    let ascii = to_ascii_digits(literal);
    let want_arabic = to == Skin::Ar && config.ar_digits == ArDigits::ArabicIndic;
    let plain_decimal = ascii
        .chars()
        .all(|c| c.is_ascii_digit() || c == '_' || c == '.');
    if !want_arabic || !plain_decimal {
        // Hex, binary, octal and exponent forms are ASCII in every skin (K0128).
        return ascii;
    }
    ascii
        .chars()
        .map(|c| match c {
            '0'..='9' => {
                char::from_u32(0x0660 + (c as u32 - '0' as u32)).expect("Arabic-Indic digit")
            }
            '.' => '\u{066B}',
            other => other,
        })
        .collect()
}

/// Why [`format_strict`] produced no output.
#[derive(Clone, Debug)]
pub enum FmtError {
    /// The source has lexer or parser errors (they are returned, nothing is printed).
    Source(Vec<Diagnostic>),
    /// The formatter could not lay the file out without moving a comment, or its output
    /// failed the self-check (it must parse to the same AST with the same comments).
    Unformattable(String),
}

impl fmt::Display for FmtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FmtError::Source(d) => {
                let codes: Vec<&str> = d.iter().map(|d| d.code).collect();
                write!(f, "the source has errors: {}", codes.join(", "))
            }
            FmtError::Unformattable(why) => write!(f, "cannot format: {why}"),
        }
    }
}

/// `kalam fmt`: canonical formatting in the file's own skin (spec §21.1).
///
/// If the layout cannot be normalized without moving a comment (a comment inside a block
/// header or between an item's parts), the file is still re-spelled canonically but keeps
/// its layout, which is always safe ([`format_strict`] reports such files instead).
pub fn format(text: &str, skin: Skin, config: &FmtConfig) -> Result<String, Vec<Diagnostic>> {
    match format_strict(text, skin, config) {
        Ok(out) => Ok(out),
        Err(FmtError::Source(d)) => Err(d),
        Err(FmtError::Unformattable(_)) => convert(text, skin, skin, config),
    }
}

/// [`format`] without the layout-preserving fallback, and with its self-check: the output
/// parses cleanly to the same AST ([`normalized_shape`]) with the same comments.
pub fn format_strict(text: &str, skin: Skin, config: &FmtConfig) -> Result<String, FmtError> {
    let src = source::Source::parse(text, skin).map_err(FmtError::Source)?;
    let roles = roles::Roles::of(&src.ast);
    let mut p = printer::Printer::new(&src, &roles, config);
    p.file();
    if let Some(why) = p.failed.take() {
        return Err(FmtError::Unformattable(why));
    }
    let unplaced = p.unplaced();
    if !unplaced.is_empty() {
        return Err(FmtError::Unformattable(format!(
            "comment(s) in a position the formatter cannot keep: {}",
            unplaced.join("; ")
        )));
    }
    let out = std::mem::take(&mut p.out);
    let again = source::Source::parse(&out, skin).map_err(|d| {
        let codes: Vec<&str> = d.iter().map(|d| d.code).collect();
        FmtError::Unformattable(format!(
            "formatted text does not parse ({})",
            codes.join(", ")
        ))
    })?;
    if normalized_shape(&again.ast) != normalized_shape(&src.ast) {
        return Err(FmtError::Unformattable(
            "formatted text has a different AST".into(),
        ));
    }
    if comment_texts(&again.ast) != comment_texts(&src.ast) {
        return Err(FmtError::Unformattable(
            "formatted text has different comments".into(),
        ));
    }
    Ok(out)
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
        convert(text, from, to, &FmtConfig::default()).expect("source parses cleanly")
    }

    fn fmt(text: &str, skin: Skin) -> String {
        match format_strict(text, skin, &FmtConfig::default()) {
            Ok(s) => s,
            Err(e) => panic!("{e}\n{text}"),
        }
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
        let en = "fn main():\n    let law = 1\n    print(law)\n";
        let franco = conv(en, Skin::En, Skin::Franco);
        assert_eq!(
            franco,
            "dala ra2isi():\n    khali `law` = 1\n    ektb(`law`)\n"
        );
        assert_eq!(conv(&franco, Skin::Franco, Skin::En), en);
        // Franco spelling variants are reserved too (§4.3), and Arabic compares after folding.
        let en = "fn main():\n    let lw = 1\n    let اذا = lw\n";
        let franco = conv(en, Skin::En, Skin::Franco);
        assert_eq!(
            franco,
            "dala ra2isi():\n    khali `lw` = 1\n    khali اذا = `lw`\n"
        );
        let ar = conv(en, Skin::En, Skin::Ar);
        assert_eq!(ar, "دالة رئيسي():\n    ليكن lw = 1\n    ليكن `اذا` = lw\n");
        assert_eq!(conv(&ar, Skin::Ar, Skin::En), en);
    }

    #[test]
    fn reserved_words_after_dot_and_as_labels_need_no_escape_and_are_never_translated() {
        let en = "fn main():\n    contract.law(match: 1, law: 2)\n";
        let franco = conv(en, Skin::En, Skin::Franco);
        assert_eq!(
            franco,
            "dala ra2isi():\n    contract.law(match: 1, law: 2)\n"
        );
        // `law` is a Franco keyword token here, but in a label position it is a name: it is
        // copied, not translated to `if`.
        assert_eq!(conv(&franco, Skin::Franco, Skin::En), en);
        // A Franco variant used as a label is not canonicalized either.
        let src = "dala ra2isi():\n    f(lw: 1, wala: 2)\n";
        assert_eq!(fmt(src, Skin::Franco), src);
        assert_eq!(
            conv(src, Skin::Franco, Skin::En),
            "fn main():\n    f(lw: 1, wala: 2)\n"
        );
        // Record field names in declarations, too.
        let src = "type T:\n    law: Int\n";
        assert_eq!(
            conv(src, Skin::En, Skin::Franco),
            "haykal T:\n    law: adad\n"
        );
    }

    #[test]
    fn unneeded_backticks_are_removed_by_fmt() {
        let src = "fn main():\n    let `x` = `if`\n    f(`if`: `match`.`law`)\n";
        assert_eq!(
            fmt(src, Skin::En),
            "fn main():\n    let x = `if`\n    f(if: `match`.law)\n"
        );
    }

    #[test]
    fn arabic_punctuation_and_digits() {
        let en = "fn main():\n    f(a, b)?\n";
        assert_eq!(
            conv(en, Skin::En, Skin::Ar),
            "دالة رئيسي():\n    f(a\u{060C} b)\u{061F}\n"
        );
        let config = FmtConfig {
            ar_digits: ArDigits::ArabicIndic,
        };
        let en = "fn main():\n    x = 1.5 + 0x1F + t.0\n";
        let ar = convert(en, Skin::En, Skin::Ar, &config).unwrap();
        assert_eq!(
            ar,
            "دالة رئيسي():\n    x = \u{0661}\u{066B}\u{0665} + 0x1F + t.\u{0660}\n"
        );
        assert_eq!(conv(&ar, Skin::Ar, Skin::En), en);
        assert_eq!(format(&ar, Skin::Ar, &config).unwrap(), ar);
        // The default `ar` setting is ASCII digits: fmt rewrites Arabic-Indic ones.
        let ascii = conv(&ar, Skin::Ar, Skin::Ar);
        assert_eq!(fmt(&ar, Skin::Ar), ascii);
        assert!(ascii.contains("1.5 + 0x1F + t.0"));
    }

    #[test]
    fn source_with_errors_is_not_converted_or_formatted() {
        let cfg = FmtConfig::default();
        assert!(convert("3ala = 1\n", Skin::Franco, Skin::En, &cfg).is_err());
        assert!(convert("law = 1\n", Skin::En, Skin::Franco, &cfg).is_err());
        assert!(format("fn main(:\n", Skin::En, &cfg).is_err());
        assert_eq!(format("", Skin::En, &cfg).unwrap(), "");
    }

    #[test]
    fn interpolation_holes_are_code() {
        let en = "fn main():\n    print(\"{a and not b}, {f(x, y)?} {print}\")\n";
        let ar = conv(en, Skin::En, Skin::Ar);
        assert_eq!(
            ar,
            "دالة رئيسي():\n    اطبع(\"{a و ليس b}, {f(x\u{060C} y)\u{061F}} {اطبع}\")\n"
        );
        assert_eq!(conv(&ar, Skin::Ar, Skin::En), en);
        // Identifiers in holes are escaped like any other identifier; escapes are text.
        let en = "fn main():\n    print(\"\\{law\\} {law} \\u{41}\")\n";
        let franco = conv(en, Skin::En, Skin::Franco);
        assert_eq!(
            franco,
            "dala ra2isi():\n    ektb(\"\\{law\\} {`law`} \\u{41}\")\n"
        );
        assert_eq!(conv(&franco, Skin::Franco, Skin::En), en);
        // fmt normalizes the spacing of hole code.
        assert_eq!(
            fmt("fn main():\n    print(\"{ a+b }\")\n", Skin::En),
            "fn main():\n    print(\"{a + b}\")\n"
        );
    }

    #[test]
    fn fmt_canonicalizes_variant_spellings() {
        // Franco aliases.
        let src = "dala ra2eesy():\n    lw sa7e7:\n        etb3(\"x\")\n    wala:\n        fot\n";
        assert_eq!(
            fmt(src, Skin::Franco),
            "dala ra2isi():\n    law sa7:\n        ektb(\"x\")\n    walla:\n        fot\n"
        );
        // Arabic hamza-less forms and Persian/Urdu letters (D19): لکل (keheh), فی (Farsi yeh).
        let src = "دالة رئيسي():\n    ل\u{06A9}ل n ف\u{06CC} 1..=3:\n        اذا n == 1:\n            اطبع(\"x\")\n";
        assert_eq!(
            fmt(src, Skin::Ar),
            "دالة رئيسي():\n    لكل n في 1..=3:\n        إذا n == 1:\n            اطبع(\"x\")\n"
        );
    }

    #[test]
    fn fmt_normalizes_spacing_and_blank_lines() {
        let src =
            "fn  f( a:Int,b : Int )->Int :\n\n\n    return a+b*  2\nfn g():\n    pass\n\n\n\n";
        assert_eq!(
            fmt(src, Skin::En),
            "fn f(a: Int, b: Int) -> Int:\n    return a + b * 2\n\nfn g():\n    pass\n"
        );
    }

    #[test]
    fn fmt_breaks_long_lines_inside_brackets_and_joins_short_ones() {
        let long = "x".repeat(40);
        let src = format!(
            "fn main():\n    let v = [\n        1,\n        2]\n    f({long}, {long}, {long})\n"
        );
        assert_eq!(
            fmt(&src, Skin::En),
            format!(
                "fn main():\n    let v = [1, 2]\n    f(\n        {long},\n        {long},\n        {long},\n    )\n"
            )
        );
    }

    #[test]
    fn fmt_breaks_long_method_chains_into_leading_dot_lines() {
        let arg = "a".repeat(30);
        let src = format!("fn main():\n    return users.filter({arg}).map({arg}).sort({arg})\n");
        assert_eq!(
            fmt(&src, Skin::En),
            format!(
                "fn main():\n    return users\n        .filter({arg})\n        .map({arg})\n        .sort({arg})\n"
            )
        );
    }

    #[test]
    fn one_line_blocks_stay_only_when_they_fit() {
        let src = "fn main():\n    if x: return 1\n";
        assert_eq!(fmt(src, Skin::En), src);
        let long = "y".repeat(90);
        let src = format!("fn main():\n    if x: return {long}\n");
        assert_eq!(
            fmt(&src, Skin::En),
            format!("fn main():\n    if x:\n        return {long}\n")
        );
    }

    #[test]
    fn comments_are_kept_in_place() {
        let src = "# head\n\n## doc\nfn main():  # trailing\n    # lead\n    a()  # after a\n\n    # before b\n    b()\n    # end of main\n\n# end of file\n";
        assert_eq!(fmt(src, Skin::En), src);
        // A comment inside a broken list keeps the statement as written.
        let src = "fn main():\n    let v = [\n        1,  # one\n        2,\n    ]\n    f()\n";
        assert_eq!(fmt(src, Skin::En), src);
        assert_eq!(
            conv(src, Skin::En, Skin::Ar),
            "دالة رئيسي():\n    ليكن v = [\n        1\u{060C}  # one\n        2\u{060C}\n    ]\n    f()\n"
        );
    }

    #[test]
    fn a_comment_inside_a_block_header_keeps_the_layout() {
        let src = "fn main():\n    while f(\n        a,  # why\n      b):\n        go()\n";
        assert!(matches!(
            format_strict(src, Skin::En, &FmtConfig::default()),
            Err(FmtError::Unformattable(_))
        ));
        // `format` falls back to the layout-preserving re-spelling: same text here (it is
        // already canonically spelled), idempotent, nothing lost.
        let once = format(src, Skin::En, &FmtConfig::default()).unwrap();
        assert_eq!(once, src);
        assert_eq!(
            format(&once, Skin::En, &FmtConfig::default()).unwrap(),
            once
        );
        let franco = "dala ra2eesy():\n    tool_ma f(\n        a,  # why\n      b):\n        g()\n";
        assert_eq!(
            format(franco, Skin::Franco, &FmtConfig::default()).unwrap(),
            "dala ra2isi():\n    tool_ma f(\n        a,  # why\n      b):\n        g()\n"
        );
    }

    #[test]
    fn fmt_rewrites_crlf_to_lf_and_convert_keeps_it() {
        let src = "# head\r\nfn main():  # t\r\n    # c\r\n    a()\r\n";
        assert_eq!(
            fmt(src, Skin::En),
            "# head\nfn main():  # t\n    # c\n    a()\n"
        );
        assert_eq!(
            conv(src, Skin::En, Skin::Franco),
            "# head\r\ndala ra2isi():  # t\r\n    # c\r\n    a()\r\n"
        );
    }

    #[test]
    fn fmt_is_idempotent_on_the_spec_examples() {
        for program in [HELLO, FIZZ] {
            for skin in Skin::ALL {
                let once = fmt(program[skin.index()], skin);
                assert_eq!(once, program[skin.index()]);
                assert_eq!(fmt(&once, skin), once);
            }
        }
    }
}
