//! Interpolation holes are code (spec §5.7): a name or number inside `{…}` gets the same
//! §5.3 and §5.6 checks as one outside a string. Source-text problems (§5.1) are reported
//! once, by the string itself.

use kalam_base::{FileId, Skin};
use kalam_diagnostics::Diagnostic;

/// `print("<hole>")` inside `fn main()`, after `decl`.
fn program(decl: &str, string: &str) -> String {
    format!("fn main():\n    {decl}\n    print(\"{string}\")\n")
}

fn diags(src: &str) -> Vec<Diagnostic> {
    kalam_parser::parse_source(src, Skin::En, FileId(0)).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<&'static str> {
    ds.iter().map(|d| d.code).collect()
}

fn arg(d: &Diagnostic, name: &str) -> String {
    d.args
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

#[test]
fn names_and_numbers_in_holes_get_the_code_checks() {
    for (decl, string, code, cp, col) in [
        // Persian heh doachashmeh in a Latin name: two scripts (§5.3 rule 3).
        ("let walha = 1", "{wal\u{06BE}a}", "SEC0105", "U+06BE", 4),
        // An Extended Arabic-Indic digit starting a number in an English file (§5.6).
        ("let x = 1", "{\u{06F9} + x}", "SEC0121", "U+06F9", 1),
        // A haraka inside a name (§5.3 rule 4).
        ("let ab = 1", "{a\u{064E}b}", "SEC0106", "U+064E", 2),
        // A non-ASCII digit inside a name (§5.3 rule 5).
        ("let x1 = 1", "{x\u{0661}}", "SEC0124", "U+0661", 2),
    ] {
        let src = program(decl, string);
        let ds = diags(&src);
        let found: Vec<_> = ds.iter().filter(|d| d.code == code).collect();
        assert_eq!(found.len(), 1, "{src:?}: {:?}", codes(&ds));
        let d = found[0];
        assert_eq!(arg(d, "cp"), cp, "{src:?}");
        // The span points into the hole in the file, not into the hole's own text.
        let string_start = src.find('"').unwrap() + 1;
        let at = d.span.start as usize - string_start;
        assert_eq!(
            src[string_start..string_start + at].chars().count(),
            col,
            "{src:?}"
        );
    }
}

#[test]
fn source_text_problems_in_holes_are_reported_once() {
    for (decl, string, code) in [
        ("let a = 1", "{a\u{202E}}", "SEC0110"),
        ("let ab = 1", "{a\u{200D}b}", "SEC0111"),
        ("let a = 1", "{a\u{0378}}", "SEC0112"),
    ] {
        let src = program(decl, string);
        let ds = diags(&src);
        let n = ds.iter().filter(|d| d.code == code).count();
        assert_eq!(n, 1, "{src:?}: {:?}", codes(&ds));
    }
}

#[test]
fn clean_holes_stay_clean() {
    for (decl, string) in [
        ("let a = 1", "{a + 2}"),
        ("let اسم = 1", "{اسم}"),
        ("let a = 1", "{`a`}"),
    ] {
        let src = program(decl, string);
        let ds = diags(&src);
        assert!(ds.is_empty(), "{src:?}: {:?}", codes(&ds));
    }
}
