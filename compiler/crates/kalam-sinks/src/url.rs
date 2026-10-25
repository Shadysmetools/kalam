//! `url"…"` (spec §17.6): the scheme and the host must be literal (K1631). Every other hole
//! is a component (path segment, query key or value, fragment), percent-encoded.
//!
//! A hole is in the scheme or host when it comes:
//! - at the very start of the literal (it could supply `https://evil.example`);
//! - inside a scheme, or right after one with no `//authority` written (`https:{x}` could
//!   supply `//evil.example`);
//! - inside `//authority`, before the `/`, `?` or `#` that ends it.
//!
//! A literal that starts with `/`, `?`, `#` or `.`, or with a path that has no scheme
//! (`users/{id}`), is relative: its holes are components.

use crate::{HoleRequirement, Site, TemplateCheck};

/// Stands for a hole in the joined template.
const HOLE: char = '\u{0}';

pub(crate) fn check(parts: &[String], site: &Site<'_>) -> TemplateCheck {
    // The template with one HOLE per hole, as chars.
    let mut text: Vec<char> = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            text.push(HOLE);
        }
        text.extend(part.chars().filter(|&c| c != HOLE));
    }
    let end = literal_prefix_end(&text);
    let mut holes = Vec::new();
    let mut diagnostics = Vec::new();
    let mut hole = 0;
    for (pos, &c) in text.iter().enumerate() {
        if c != HOLE {
            continue;
        }
        if pos < end {
            diagnostics.push(site.hole("K1631", hole));
            holes.push(HoleRequirement::Forbidden);
        } else {
            holes.push(HoleRequirement::UrlComponent);
        }
        hole += 1;
    }
    TemplateCheck { holes, diagnostics }
}

/// The end of the part of the URL that must be literal (scheme and authority): a hole
/// before this position is K1631.
fn literal_prefix_end(t: &[char]) -> usize {
    let authority_end = |from: usize| {
        t[from..]
            .iter()
            .position(|c| matches!(c, '/' | '?' | '#'))
            .map_or(t.len(), |p| from + p)
    };
    let starts_authority = |at: usize| t.get(at) == Some(&'/') && t.get(at + 1) == Some(&'/');
    match t.first() {
        None => return 0,
        Some(&HOLE) => return 1,
        _ => {}
    }
    if starts_authority(0) {
        return authority_end(2);
    }
    // A scheme: a letter, then letters, digits, `+`, `-` or `.`, then `:`.
    let mut i = 0;
    while let Some(&c) = t.get(i) {
        let ok = if i == 0 {
            c.is_ascii_alphabetic()
        } else {
            c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
        };
        if !ok {
            break;
        }
        i += 1;
    }
    match t.get(i) {
        // A hole inside the scheme.
        Some(&HOLE) if i > 0 => i + 1,
        Some(&':') if i > 0 => {
            let rest = i + 1;
            if starts_authority(rest) {
                authority_end(rest + 2)
            } else if t.get(rest) == Some(&HOLE) {
                rest + 1
            } else {
                0
            }
        }
        // Relative.
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{codes, holes};
    use crate::HoleRequirement as R;
    use crate::Tag;

    fn url(t: &str) -> Vec<&'static str> {
        codes(Tag::Url, t)
    }

    #[test]
    fn conformance_cases() {
        assert_eq!(url("https://{host}/users"), vec!["K1631"]);
        let t = "https://api.example.com/users/{id}?q={term}";
        assert!(url(t).is_empty());
        assert_eq!(holes(Tag::Url, t), vec![R::UrlComponent; 2]);
        assert!(url("https://example.com/home").is_empty());
    }

    #[test]
    fn scheme_and_host_must_be_literal() {
        for t in [
            "{u}",
            "{scheme}://example.com/",
            "http{s}://example.com/",
            "https:{rest}",
            "https://api.{domain}/x",
            "https://example.com:{port}/x",
            "https://{user}@example.com/",
            "//{host}/x",
        ] {
            assert_eq!(url(t), vec!["K1631"], "{t}");
        }
    }

    #[test]
    fn relative_urls_and_components() {
        for t in [
            "/{page}",
            "/users/{id}",
            "?q={q}",
            "#{frag}",
            "./{f}",
            "users/{id}",
            "mailto:team@example.com?subject={s}",
            "https://example.com?q={q}",
            "https://example.com#{f}",
        ] {
            assert!(url(t).is_empty(), "{t}");
        }
    }

    #[test]
    fn odd_input_never_panics() {
        for t in [
            "",
            ":",
            "//",
            "https://",
            "a:{b}",
            "{a}{b}",
            "\u{0}",
            "https://مثال.مصر/{x}",
        ] {
            let _ = url(t);
        }
    }
}
