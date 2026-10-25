//! `html"…"` (spec §17.4, §20.3): the literal is tokenized as an HTML5 fragment, and each
//! hole is classified by its context (Go `html/template` semantics).
//!
//! Well-formedness (K1610): every non-void element is closed in order, attribute values
//! are quoted, and no tag, comment or `<script>`/`<style>` body is left open. A literal with
//! a hole in a tag or attribute name (K1614) is not also K1610: its tags can't be matched.
//!
//! | Hole position | Requirement or finding |
//! |---|---|
//! | text, `<title>`/`<textarea>` text | [`HoleRequirement::HtmlText`] |
//! | quoted value of `href`, `src`, `action`, … , `k-get`, `k-post` | [`HoleRequirement::Url`] |
//! | quoted value of `k-click`, `k-submit`, `k-input`, `k-change` | [`HoleRequirement::Message`] |
//! | other quoted attribute value | [`HoleRequirement::HtmlAttr`] |
//! | unquoted attribute value | K1611 |
//! | `on*` or `style` value, `<script>`/`<style>` body | K1612 |
//! | tag name, attribute name | K1614 |
//! | comment, `<!…>` declaration | K1615 |
//!
//! An attribute named `k-…` that is not one of the eight directives of §20.3 is K1617.

use crate::{HoleRequirement, Site, TemplateCheck};

/// Elements with no closing tag (HTML5 void elements).
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Attributes whose value is a URL (spec §17.4: `Url` only).
const URL_ATTRS: &[&str] = &[
    "href",
    "src",
    "action",
    "formaction",
    "poster",
    "srcset",
    "cite",
    "data",
    "background",
    "codebase",
    "longdesc",
    "manifest",
    "ping",
    "xlink:href",
    "k-get",
    "k-post",
];

/// Live-component events: the hole must derive `Message` (§20.3).
const MESSAGE_ATTRS: &[&str] = &["k-click", "k-submit", "k-input", "k-change"];

/// Every `k-` directive (§20.3); any other `k-…` attribute is K1617.
const K_ATTRS: &[&str] = &[
    "k-click", "k-submit", "k-input", "k-change", "k-get", "k-post", "k-target", "k-swap",
];

#[derive(Clone, PartialEq, Eq)]
enum State {
    Text,
    /// After `<`.
    TagOpen,
    TagName,
    /// Between attributes.
    InTag,
    AttrName,
    AfterAttrName,
    /// After `=`.
    BeforeValue,
    Quoted(char),
    Unquoted,
    /// After `<!`, with the number of `-` seen.
    Bang(u8),
    Comment,
    /// `<!DOCTYPE …>` and other declarations.
    Decl,
    /// The body of `<script>`, `<style>`, `<title>` or `<textarea>`, up to its end tag.
    Raw,
}

struct Scan {
    state: State,
    tag: String,
    end_tag: bool,
    self_closing: bool,
    attr: String,
    /// The tag of a raw-text body, and the body read so far.
    raw: String,
    buf: String,
    stack: Vec<String>,
    malformed: bool,
    /// A hole is in a tag or attribute name (K1614).
    dynamic_name: bool,
    unknown_k: bool,
}

impl Scan {
    fn feed(&mut self, c: char) {
        match self.state.clone() {
            State::Text => {
                if c == '<' {
                    self.state = State::TagOpen;
                }
            }
            State::TagOpen => match c {
                '/' => {
                    self.start_tag(true);
                }
                '!' => self.state = State::Bang(0),
                c if c.is_alphabetic() => {
                    self.start_tag(false);
                    self.tag.push(c.to_ascii_lowercase());
                }
                // `a < b`: the `<` was text.
                _ => self.state = State::Text,
            },
            State::TagName => match c {
                '>' => self.finish_tag(),
                '/' => {
                    self.self_closing = true;
                    self.state = State::InTag;
                }
                c if c.is_whitespace() => self.state = State::InTag,
                c if c.is_alphanumeric() || c == '-' || c == ':' || c == '_' => {
                    self.tag.push(c.to_ascii_lowercase())
                }
                _ => self.malformed = true,
            },
            State::InTag => match c {
                '>' => self.finish_tag(),
                '/' => self.self_closing = true,
                c if c.is_whitespace() => {}
                '=' | '"' | '\'' | '<' => self.malformed = true,
                c => {
                    self.self_closing = false;
                    self.attr.clear();
                    self.attr.push(c.to_ascii_lowercase());
                    self.state = State::AttrName;
                }
            },
            State::AttrName => match c {
                '=' => {
                    self.attr_done();
                    self.state = State::BeforeValue;
                }
                '>' => {
                    self.attr_done();
                    self.finish_tag();
                }
                '/' => {
                    self.attr_done();
                    self.self_closing = true;
                    self.state = State::InTag;
                }
                c if c.is_whitespace() => {
                    self.attr_done();
                    self.state = State::AfterAttrName;
                }
                '"' | '\'' | '<' => self.malformed = true,
                c => self.attr.push(c.to_ascii_lowercase()),
            },
            State::AfterAttrName => match c {
                '=' => self.state = State::BeforeValue,
                '>' => self.finish_tag(),
                '/' => {
                    self.self_closing = true;
                    self.state = State::InTag;
                }
                c if c.is_whitespace() => {}
                c => {
                    self.attr.clear();
                    self.attr.push(c.to_ascii_lowercase());
                    self.state = State::AttrName;
                }
            },
            State::BeforeValue => match c {
                '"' | '\'' => self.state = State::Quoted(c),
                c if c.is_whitespace() => {}
                '>' => {
                    self.malformed = true;
                    self.finish_tag();
                }
                _ => {
                    // An unquoted literal value.
                    self.malformed = true;
                    self.state = State::Unquoted;
                }
            },
            State::Quoted(q) => {
                if c == q {
                    self.state = State::InTag;
                }
            }
            State::Unquoted => match c {
                '>' => self.finish_tag(),
                c if c.is_whitespace() => self.state = State::InTag,
                _ => {}
            },
            State::Bang(n) => match (c, n) {
                ('-', 0) => self.state = State::Bang(1),
                ('-', 1) => {
                    self.buf.clear();
                    self.state = State::Comment;
                }
                ('>', _) => self.state = State::Text,
                _ => self.state = State::Decl,
            },
            State::Comment => {
                self.buf.push(c);
                if self.buf.ends_with("-->") {
                    self.state = State::Text;
                }
            }
            State::Decl => {
                if c == '>' {
                    self.state = State::Text;
                }
            }
            State::Raw => {
                self.buf.push(c.to_ascii_lowercase());
                let close = format!("</{}", self.raw);
                if self.buf.ends_with(&close) {
                    self.tag = std::mem::take(&mut self.raw);
                    self.end_tag = true;
                    self.self_closing = false;
                    // `</script` is read; the rest of the end tag follows.
                    self.state = State::TagName;
                }
            }
        }
    }

    fn start_tag(&mut self, end: bool) {
        self.tag.clear();
        self.end_tag = end;
        self.self_closing = false;
        self.state = State::TagName;
    }

    fn attr_done(&mut self) {
        if self.attr.starts_with("k-") && !K_ATTRS.contains(&self.attr.as_str()) {
            self.unknown_k = true;
        }
    }

    fn finish_tag(&mut self) {
        self.state = State::Text;
        let name = std::mem::take(&mut self.tag);
        if self.end_tag {
            if self.stack.last() == Some(&name) {
                self.stack.pop();
            } else {
                self.malformed = true;
            }
            return;
        }
        if name.is_empty() {
            self.malformed = true;
            return;
        }
        if self.self_closing || VOID.contains(&name.as_str()) {
            return;
        }
        if matches!(name.as_str(), "script" | "style" | "title" | "textarea") {
            self.raw = name.clone();
            self.buf.clear();
            self.state = State::Raw;
        }
        self.stack.push(name);
    }

    /// What the hole at the current position must be, or the code that forbids it.
    fn hole(&mut self) -> Result<HoleRequirement, &'static str> {
        match self.state.clone() {
            State::Text => Ok(HoleRequirement::HtmlText),
            State::Raw if matches!(self.raw.as_str(), "title" | "textarea") => {
                Ok(HoleRequirement::HtmlText)
            }
            State::Raw => Err("K1612"),
            State::TagOpen | State::TagName => {
                if self.state == State::TagOpen {
                    self.start_tag(false);
                }
                // Stands for the hole, so the name is never empty or a real tag.
                self.tag.push('\u{FFFD}');
                self.dynamic_name = true;
                Err("K1614")
            }
            State::InTag | State::AttrName | State::AfterAttrName => {
                self.attr = "\u{FFFD}".into();
                self.state = State::AttrName;
                self.dynamic_name = true;
                Err("K1614")
            }
            State::BeforeValue | State::Unquoted => {
                self.state = State::Unquoted;
                Err("K1611")
            }
            State::Quoted(_) => {
                let a = self.attr.as_str();
                if a.starts_with("on") || a == "style" {
                    Err("K1612")
                } else if URL_ATTRS.contains(&a) {
                    Ok(HoleRequirement::Url)
                } else if MESSAGE_ATTRS.contains(&a) {
                    Ok(HoleRequirement::Message)
                } else {
                    Ok(HoleRequirement::HtmlAttr)
                }
            }
            State::Bang(_) | State::Comment | State::Decl => Err("K1615"),
        }
    }
}

pub(crate) fn check(parts: &[String], site: &Site<'_>) -> TemplateCheck {
    let mut s = Scan {
        state: State::Text,
        tag: String::new(),
        end_tag: false,
        self_closing: false,
        attr: String::new(),
        raw: String::new(),
        buf: String::new(),
        stack: Vec::new(),
        malformed: false,
        dynamic_name: false,
        unknown_k: false,
    };
    let mut holes = Vec::new();
    let mut diagnostics = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        for c in part.chars() {
            s.feed(c);
        }
        if i + 1 == parts.len() {
            break;
        }
        match s.hole() {
            Ok(req) => holes.push(req),
            Err(code) => {
                diagnostics.push(site.hole(code, i));
                holes.push(HoleRequirement::Forbidden);
            }
        }
    }
    if s.state != State::Text || !s.stack.is_empty() {
        s.malformed = true;
    }
    if s.unknown_k {
        diagnostics.push(site.literal("K1617"));
    }
    if s.malformed && !s.dynamic_name {
        diagnostics.push(site.literal("K1610"));
    }
    TemplateCheck { holes, diagnostics }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{codes, holes};
    use crate::HoleRequirement as R;
    use crate::Tag;

    fn html(t: &str) -> Vec<&'static str> {
        codes(Tag::Html, t)
    }

    #[test]
    fn well_formed_fragments() {
        assert!(html("<p><b>{name}</b></p>").is_empty());
        assert!(html("<footer>Built with Kalam</footer>").is_empty());
        assert!(html("<p>a < b and c > d</p>").is_empty());
        assert!(html("<br><img src=\"/a.png\" alt=\"\"><hr/><x-y a=\"1\" b></x-y>").is_empty());
        assert!(
            html("<!DOCTYPE html><html><head><title>{t} | Cafe</title></head></html>").is_empty()
        );
        assert!(html("<p>hi<!-- note --></p>").is_empty());
        assert!(html(r"<script>if (a < b) \{ x() \}</script>").is_empty());
        assert!(html(r"<STYLE>p \{ color: red \}</Style>").is_empty());
        assert!(html("").is_empty());
        assert!(html("plain text {x}").is_empty());
    }

    #[test]
    fn malformed_fragments_are_k1610() {
        for t in [
            "<p><b>hi</p>",
            "<p>hi",
            "</p>",
            "<p class=a>hi</p>",
            "<p class=>hi</p>",
            "<p",
            "<p>hi<!-- open</p>",
            "<script>x",
            "<p \"x\">hi</p>",
        ] {
            assert_eq!(html(t), vec!["K1610"], "{t}");
        }
    }

    #[test]
    fn conformance_positive_twin() {
        // 17.4.2
        let t = "<p class=\"{name}\" title=\"{count}\">{name}</p><a href=\"{home}\">home</a>";
        assert!(html(t).is_empty());
        assert_eq!(
            holes(Tag::Html, t),
            vec![R::HtmlAttr, R::HtmlAttr, R::HtmlText, R::Url]
        );
    }

    #[test]
    fn hole_contexts() {
        assert_eq!(
            holes(
                Tag::Html,
                "<a href='{u}' k-get=\"{g}\" k-post=\"{p}\" src=\"{s}\">x</a>"
            ),
            vec![R::Url; 4]
        );
        assert_eq!(
            holes(
                Tag::Html,
                "<li class=\"{state}\" k-click=\"{m}\">{title}</li><form k-submit=\"{s}\"></form>"
            ),
            vec![R::HtmlAttr, R::Message, R::HtmlText, R::Message]
        );
        assert_eq!(
            holes(Tag::Html, "<div k-target=\"#{id}\" k-swap=\"{s}\"></div>"),
            vec![R::HtmlAttr, R::HtmlAttr]
        );
        assert_eq!(
            holes(Tag::Html, "<textarea>{t}</textarea><title>{u}</title>"),
            vec![R::HtmlText, R::HtmlText]
        );
    }

    #[test]
    fn forbidden_contexts() {
        assert_eq!(html("<p class={name}>hi</p>"), vec!["K1611"]);
        assert_eq!(html("<p class=x{name}>hi</p>"), vec!["K1611", "K1610"]);
        assert_eq!(html("<script>{name}</script>"), vec!["K1612"]);
        assert_eq!(html(r"<style>p \{ \} {x}</style>"), vec!["K1612"]);
        assert_eq!(html("<p onclick=\"go({x})\">hi</p>"), vec!["K1612"]);
        assert_eq!(html("<p style=\"color: {c}\">hi</p>"), vec!["K1612"]);
        assert_eq!(html("<{name}>hi</{name}>"), vec!["K1614", "K1614"]);
        assert_eq!(html("<h{n}>hi</h1>"), vec!["K1614"]);
        assert_eq!(html("<a {n}=\"x\">hi</a>"), vec!["K1614"]);
        assert_eq!(html("<p>hi<!-- {name} --></p>"), vec!["K1615"]);
        assert_eq!(html("<!DOCTYPE {x}><p></p>"), vec!["K1615"]);
    }

    #[test]
    fn unknown_k_attributes_are_k1617() {
        assert_eq!(
            html("<button k-frobnicate=\"go\">{name}</button>"),
            vec!["K1617"]
        );
        assert_eq!(html("<p k-hidden>x</p>"), vec!["K1617"]);
        assert!(html("<p k-target=\"#a\" k-swap=\"append\">x</p>").is_empty());
        // `k` alone and other prefixes are ordinary attributes.
        assert!(html("<p k=\"1\" kx-a=\"2\">x</p>").is_empty());
    }

    #[test]
    fn multi_line_view_from_the_spec() {
        let t = "\n        <section dir=\"auto\">\n          <h1>{owner}'s tasks</h1>\n          <ul>{items}</ul>\n          <button k-click=\"{m}\">Clear done</button>\n        </section>\n    ";
        assert!(html(t).is_empty());
        assert_eq!(
            holes(Tag::Html, t),
            vec![R::HtmlText, R::HtmlText, R::Message]
        );
    }

    #[test]
    fn arabic_text_and_odd_input_never_panic() {
        assert!(html("<p dir=\"rtl\">مرحبا {الاسم}</p>").is_empty());
        for t in [
            "<",
            "</",
            "<!",
            "<!-",
            "<!--",
            "<a b='",
            "{a}{b}",
            "<{a}",
            "</{a}>",
            "<a {b}",
            "<a b={c}",
            "<script></scrip",
            "<p>\u{0}</p>",
            "<ب>x</ب>",
        ] {
            let _ = html(t);
        }
    }
}
