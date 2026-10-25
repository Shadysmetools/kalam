//! Compile-time parsers for tagged literals: `sql"…"`, `html"…"`, `cmd"…"`, `url"…"`,
//! `path"…"` and `regex"…"` (spec §17, report 04 S1–S3), plus the capability rules of
//! spec §16 that need no type information of their own ([`caps`]).
//!
//! The type checker hands each tagged literal's [`StaticParts`] (the literal text the
//! developer wrote, which only the compiler can create) and its hole spans to
//! [`check_template`]. The parser for the tag checks the template's structure and
//! classifies every hole by context, returning what each hole must be:
//!
//! | Tag | Checks | Hole rule |
//! |---|---|---|
//! | `sql` | quotes, comments, one statement (K1602–K1605) | bound parameter; never spliced; identifiers only as `Sql` from `sql.ident` (K1603); `LIKE` takes an escaped pattern (W1606) |
//! | `html` | HTML5 fragment well-formed (K1610), contexts (K1611–K1617) | text: `Show`/`Html`; attribute: escaped; URL attribute: `Url` (K1613); `k-<event>`: `Message` (K1616) |
//! | `cmd` | argv split, literal program (K1621), no shell metacharacters (K1620) | exactly one argv element; option-like guard (W1622) |
//! | `url` | literal scheme and host (K1631) | percent-encoded per component |
//! | `path` | — | `PathSegment` only (K1641, checked by the type checker) |
//! | `regex` | — (RE2 syntax: no §27 code yet) | escaped as literal text |
//!
//! The type checker then summarizes each hole's type as a [`HoleType`] (this crate cannot
//! see `kalam-typeck`'s types: typeck depends on it) and asks [`check_hole`] whether the
//! hole's requirement accepts it. Sink-specific findings come back as finished
//! diagnostics; generic ones (a missing trait, a value that must be text) come back as a
//! [`HoleVerdict`] that typeck reports with its own codes (K0603, K0600).
//!
//! Every parser walks `char`s, never byte offsets, and never panics: typeck is a fuzz
//! target, and literals hold Arabic text.

pub mod caps;
mod cmd;
mod html;
mod sql;
mod url;

use kalam_base::{FileId, Span};
use kalam_diagnostics::Diagnostic;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tag {
    Sql,
    Html,
    Cmd,
    Url,
    Path,
    Regex,
}

impl Tag {
    /// The built-in tags. Tag names are lowercase ASCII and identical in every skin.
    pub fn from_name(name: &str) -> Option<Tag> {
        match name {
            "sql" => Some(Tag::Sql),
            "html" => Some(Tag::Html),
            "cmd" => Some(Tag::Cmd),
            "url" => Some(Tag::Url),
            "path" => Some(Tag::Path),
            "regex" => Some(Tag::Regex),
            _ => None,
        }
    }

    /// The sink type the literal produces.
    pub fn result_type(self) -> &'static str {
        match self {
            Tag::Sql => "Sql",
            Tag::Html => "Html",
            Tag::Cmd => "Command",
            Tag::Url => "Url",
            Tag::Path => "Path",
            Tag::Regex => "Regex",
        }
    }
}

/// The literal text of a tagged literal, split at its holes. Only the compiler constructs
/// this type (spec §17.2, literal provenance).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticParts {
    /// The text between the holes, escapes decoded: `hole_spans.len() + 1` parts.
    pub parts: Vec<String>,
    pub hole_spans: Vec<Span>,
}

/// What a hole must be, decided by its position in the template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoleRequirement {
    /// Any `SqlParam` type; sent as a bound parameter.
    SqlParam,
    /// A `LIKE` pattern: an `SqlParam`, where a raw `String` draws W1606.
    SqlLike,
    /// An identifier position (table, column, `ASC`/`DESC`): only an `Sql` fragment, as
    /// returned by `sql.ident` (K1603 otherwise).
    SqlIdent,
    /// `Html`, `List[Html]`, `Option[Html]` or any `Show` type (escaped).
    HtmlText,
    /// `String`, `Int`, `Bool` or `AttrValue` (attribute-escaped).
    HtmlAttr,
    /// `Url` only (K1613).
    Url,
    /// A type that derives `Message` (live-component events, K1616).
    Message,
    /// `String`, `Int`, `Path`, `PathSegment` or `List[String]` (one argv element each).
    /// `whole_word`: the hole is a whole argv word; `guarded`: a literal `--` word comes
    /// before it (W1622 otherwise, for text values).
    Argv {
        whole_word: bool,
        guarded: bool,
    },
    PathSegment,
    /// A URL component: any `Show` type, percent-encoded.
    UrlComponent,
    /// Escaped as literal text: any `Show` type.
    RegexLiteral,
    /// No value is allowed here; the template check already reported why.
    Forbidden,
}

pub struct TemplateCheck {
    /// One requirement per hole, in order.
    pub holes: Vec<HoleRequirement>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Checks a tagged literal's structure and classifies its holes. `span` is the whole
/// literal: structural findings (K1604, K1610, K1617, K1620) point at it, findings about
/// one hole point at the hole.
pub fn check_template(tag: Tag, parts: &StaticParts, file: FileId, span: Span) -> TemplateCheck {
    let site = Site {
        file,
        span,
        holes: &parts.hole_spans,
    };
    let mut out = match tag {
        Tag::Sql => sql::check(&parts.parts, &site),
        Tag::Html => html::check(&parts.parts, &site),
        Tag::Cmd => cmd::check(&parts.parts, &site),
        Tag::Url => url::check(&parts.parts, &site),
        Tag::Path => uniform(parts, HoleRequirement::PathSegment),
        Tag::Regex => uniform(parts, HoleRequirement::RegexLiteral),
    };
    // A malformed `StaticParts` (never built by the compiler) still gets one requirement
    // per hole.
    out.holes
        .resize(parts.hole_spans.len(), HoleRequirement::Forbidden);
    out
}

fn uniform(parts: &StaticParts, req: HoleRequirement) -> TemplateCheck {
    TemplateCheck {
        holes: vec![req; parts.hole_spans.len()],
        diagnostics: Vec::new(),
    }
}

/// Where findings go: the file, the literal's span and each hole's span.
pub(crate) struct Site<'a> {
    pub file: FileId,
    pub span: Span,
    pub holes: &'a [Span],
}

impl Site<'_> {
    /// A finding about the whole literal.
    pub fn literal(&self, code: &'static str) -> Diagnostic {
        Diagnostic::new(code, self.file, self.span)
    }

    /// A finding about hole `i` (the literal itself if the hole has no span).
    pub fn hole(&self, code: &'static str, i: usize) -> Diagnostic {
        let span = self.holes.get(i).copied().unwrap_or(self.span);
        Diagnostic::new(code, self.file, span)
    }
}

/// A sink type, as far as hole checking needs to know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkKind {
    Sql,
    Html,
    Command,
    Url,
    Path,
    PathSegment,
    Regex,
}

/// The shape of a hole's type, as the type checker sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoleKind {
    /// Not known (a type error was already reported, or inference left it open): never
    /// reported again.
    Unknown,
    String,
    Int,
    Bool,
    /// `Float`, `BigInt` or `Decimal`.
    Number,
    Bytes,
    Sink(SinkKind),
    List(Box<HoleKind>),
    Option(Box<HoleKind>),
    /// A named type: a user or stdlib record, enum or opaque type (`Date`, `LikePattern`).
    Named(String),
    /// Anything else (functions, maps, tuples, capabilities, `Secret`).
    Other,
}

/// A hole's type: its shape and the trait facts that decide the `Show` and `Message`
/// contexts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoleType {
    pub kind: HoleKind,
    pub show: bool,
    pub message: bool,
}

impl HoleType {
    /// A type the checker could not determine (never reported).
    pub fn unknown() -> HoleType {
        HoleType {
            kind: HoleKind::Unknown,
            show: true,
            message: true,
        }
    }
}

/// What [`check_hole`] decided.
#[derive(Clone, Debug, PartialEq)]
pub enum HoleVerdict {
    Ok,
    /// A sink-specific finding (K1603, K1613, K1616, W1606, W1622).
    Report(Diagnostic),
    /// The hole's type lacks the named trait (typeck reports K0603).
    MissingTrait(&'static str),
    /// The context takes text-like values (`String`, `Int`, …): typeck reports a mismatch
    /// against `String` (K0600).
    ExpectedText,
}

/// The stdlib's date and time types, which are `SqlParam`s (spec §17.3).
const SQL_NAMED_PARAMS: &[&str] = &["Instant", "Date", "LikePattern"];

fn sql_param(k: &HoleKind) -> bool {
    match k {
        HoleKind::Unknown
        | HoleKind::String
        | HoleKind::Int
        | HoleKind::Bool
        | HoleKind::Number
        | HoleKind::Bytes
        | HoleKind::Sink(SinkKind::Sql) => true,
        HoleKind::Option(t) | HoleKind::List(t) => sql_param(t),
        HoleKind::Named(n) => SQL_NAMED_PARAMS.contains(&n.as_str()),
        _ => false,
    }
}

/// Does `req` accept a hole of type `ty`? `span` is the hole (where a finding points).
pub fn check_hole(req: &HoleRequirement, ty: &HoleType, file: FileId, span: Span) -> HoleVerdict {
    use HoleKind as K;
    use HoleRequirement as R;
    let report = |code| HoleVerdict::Report(Diagnostic::new(code, file, span));
    if ty.kind == K::Unknown {
        return HoleVerdict::Ok;
    }
    let show = |ty: &HoleType| {
        if ty.show {
            HoleVerdict::Ok
        } else {
            HoleVerdict::MissingTrait("Show")
        }
    };
    match req {
        R::Forbidden | R::PathSegment => HoleVerdict::Ok,
        R::SqlParam if sql_param(&ty.kind) => HoleVerdict::Ok,
        R::SqlLike if ty.kind == K::String => report("W1606"),
        R::SqlLike if sql_param(&ty.kind) => HoleVerdict::Ok,
        R::SqlParam | R::SqlLike => HoleVerdict::MissingTrait("SqlParam"),
        R::SqlIdent => match ty.kind {
            K::Sink(SinkKind::Sql) => HoleVerdict::Ok,
            _ => report("K1603"),
        },
        R::HtmlText => match &ty.kind {
            K::Sink(SinkKind::Html) => HoleVerdict::Ok,
            K::List(t) | K::Option(t) if **t == K::Sink(SinkKind::Html) => HoleVerdict::Ok,
            _ => show(ty),
        },
        R::HtmlAttr => match &ty.kind {
            K::String | K::Int | K::Bool => HoleVerdict::Ok,
            K::Named(n) if n == "AttrValue" => HoleVerdict::Ok,
            _ => HoleVerdict::ExpectedText,
        },
        R::Url => match ty.kind {
            K::Sink(SinkKind::Url) => HoleVerdict::Ok,
            _ => report("K1613"),
        },
        R::Message if ty.message => HoleVerdict::Ok,
        R::Message => report("K1616"),
        R::Argv {
            whole_word,
            guarded,
        } => {
            let text_like = match &ty.kind {
                K::String | K::Sink(SinkKind::PathSegment) => true,
                K::List(t) => **t == K::String,
                K::Int | K::Sink(SinkKind::Path) => false,
                _ => return HoleVerdict::ExpectedText,
            };
            // A text value could start with `-` and be read as an option (spec §17.5,
            // Security 16.5). Numbers and `Path`s (built from a literal) are not warned.
            if text_like && *whole_word && !*guarded {
                report("W1622")
            } else {
                HoleVerdict::Ok
            }
        }
        R::UrlComponent | R::RegexLiteral => show(ty),
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// Splits `text` at `{…}` holes (no nesting, `\{` is a literal brace), like the lexer
    /// does, and checks it. Hole `i` gets the span `i..i+1`; the literal is `100..200`.
    pub fn run(tag: Tag, text: &str) -> TemplateCheck {
        let mut parts = vec![String::new()];
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' if chars.peek() == Some(&'{') || chars.peek() == Some(&'}') => {
                    parts.last_mut().unwrap().push(chars.next().unwrap());
                }
                '{' => {
                    for c in chars.by_ref() {
                        if c == '}' {
                            break;
                        }
                    }
                    parts.push(String::new());
                }
                c => parts.last_mut().unwrap().push(c),
            }
        }
        let hole_spans = (0..parts.len() - 1)
            .map(|i| Span::new(i as u32, i as u32 + 1))
            .collect();
        check_template(
            tag,
            &StaticParts { parts, hole_spans },
            FileId(0),
            Span::new(100, 200),
        )
    }

    /// The codes a template check reports, in order.
    pub fn codes(tag: Tag, text: &str) -> Vec<&'static str> {
        run(tag, text).diagnostics.iter().map(|d| d.code).collect()
    }

    /// The requirements of a template's holes.
    pub fn holes(tag: Tag, text: &str) -> Vec<HoleRequirement> {
        run(tag, text).holes
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn ty(kind: HoleKind) -> HoleType {
        HoleType {
            kind,
            show: true,
            message: false,
        }
    }

    fn verdict(req: HoleRequirement, t: HoleType) -> Option<&'static str> {
        match check_hole(&req, &t, FileId(0), Span::new(0, 1)) {
            HoleVerdict::Ok => None,
            HoleVerdict::Report(d) => Some(d.code),
            HoleVerdict::MissingTrait(_) => Some("trait"),
            HoleVerdict::ExpectedText => Some("text"),
        }
    }

    #[test]
    fn path_and_regex_holes_are_uniform() {
        assert_eq!(
            holes(Tag::Path, "uploads/{name}/{x}"),
            vec![HoleRequirement::PathSegment; 2]
        );
        assert_eq!(
            holes(Tag::Regex, "^[a-z0-9_]\\{3,20\\}{x}$"),
            vec![HoleRequirement::RegexLiteral]
        );
        assert!(codes(Tag::Regex, "^[a-z0-9_]\\{3,20\\}$").is_empty());
    }

    #[test]
    fn unknown_types_are_never_reported() {
        for req in [
            HoleRequirement::SqlIdent,
            HoleRequirement::Url,
            HoleRequirement::Message,
            HoleRequirement::HtmlAttr,
        ] {
            assert_eq!(
                check_hole(&req, &HoleType::unknown(), FileId(0), Span::new(0, 1)),
                HoleVerdict::Ok
            );
        }
    }

    #[test]
    fn sql_holes_take_sql_params() {
        use HoleKind as K;
        let r = HoleRequirement::SqlParam;
        for k in [
            K::String,
            K::Int,
            K::Number,
            K::Bool,
            K::Bytes,
            K::Sink(SinkKind::Sql),
            K::List(Box::new(K::Int)),
            K::Option(Box::new(K::String)),
            K::Named("Date".into()),
        ] {
            assert_eq!(verdict(r.clone(), ty(k.clone())), None, "{k:?}");
        }
        assert_eq!(
            verdict(r.clone(), ty(K::Named("User".into()))),
            Some("trait")
        );
        assert_eq!(verdict(r, ty(K::Other)), Some("trait"));
    }

    #[test]
    fn sql_identifiers_take_only_sql_fragments() {
        let r = HoleRequirement::SqlIdent;
        assert_eq!(verdict(r.clone(), ty(HoleKind::String)), Some("K1603"));
        assert_eq!(verdict(r, ty(HoleKind::Sink(SinkKind::Sql))), None);
    }

    #[test]
    fn like_holes_warn_on_raw_strings() {
        let r = HoleRequirement::SqlLike;
        assert_eq!(verdict(r.clone(), ty(HoleKind::String)), Some("W1606"));
        assert_eq!(verdict(r, ty(HoleKind::Named("LikePattern".into()))), None);
    }

    #[test]
    fn html_contexts() {
        use HoleKind as K;
        let html = || K::Sink(SinkKind::Html);
        assert_eq!(verdict(HoleRequirement::HtmlText, ty(K::String)), None);
        assert_eq!(
            verdict(HoleRequirement::HtmlText, ty(K::List(Box::new(html())))),
            None
        );
        let no_show = HoleType {
            kind: K::Other,
            show: false,
            message: false,
        };
        assert_eq!(verdict(HoleRequirement::HtmlText, no_show), Some("trait"));
        assert_eq!(verdict(HoleRequirement::HtmlAttr, ty(K::Int)), None);
        assert_eq!(
            verdict(HoleRequirement::HtmlAttr, ty(K::List(Box::new(K::String)))),
            Some("text")
        );
        assert_eq!(verdict(HoleRequirement::Url, ty(K::String)), Some("K1613"));
        assert_eq!(
            verdict(HoleRequirement::Url, ty(K::Sink(SinkKind::Url))),
            None
        );
        assert_eq!(
            verdict(HoleRequirement::Message, ty(K::String)),
            Some("K1616")
        );
        let msg = HoleType {
            kind: K::Named("Msg".into()),
            show: true,
            message: true,
        };
        assert_eq!(verdict(HoleRequirement::Message, msg), None);
    }

    #[test]
    fn argv_holes_and_the_option_guard() {
        use HoleKind as K;
        let argv = |whole_word, guarded| HoleRequirement::Argv {
            whole_word,
            guarded,
        };
        assert_eq!(verdict(argv(true, false), ty(K::String)), Some("W1622"));
        assert_eq!(verdict(argv(true, true), ty(K::String)), None);
        assert_eq!(verdict(argv(false, false), ty(K::String)), None);
        assert_eq!(verdict(argv(true, false), ty(K::Int)), None);
        assert_eq!(
            verdict(argv(true, false), ty(K::Sink(SinkKind::Path))),
            None
        );
        assert_eq!(
            verdict(argv(true, false), ty(K::List(Box::new(K::String)))),
            Some("W1622")
        );
        assert_eq!(verdict(argv(true, true), ty(K::Bool)), Some("text"));
    }

    #[test]
    fn warnings_are_warnings() {
        for code in ["W1606", "W1622"] {
            assert!(!Diagnostic::new(code, FileId(0), Span::new(0, 1)).is_error());
        }
    }

    #[test]
    fn random_templates_never_panic_and_classify_every_hole() {
        // Typeck is a fuzz target: every parser must take any text without panicking.
        const ALPHABET: &[char] = &[
            '<', '>', '/', '!', '-', '=', '"', '\'', '`', ' ', '\n', ';', '*', '|', '&', '$', '(',
            ')', ':', '?', '#', '.', 'a', 'k', 's', 'p', 'م', 'ر', '\u{0}', '\u{FFFD}',
        ];
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for _ in 0..2000 {
            let holes = next(4);
            let parts: Vec<String> = (0..=holes)
                .map(|_| {
                    (0..next(12))
                        .map(|_| ALPHABET[next(ALPHABET.len())])
                        .collect()
                })
                .collect();
            let spans: Vec<Span> = (0..holes)
                .map(|i| Span::new(i as u32, i as u32 + 1))
                .collect();
            let sp = StaticParts {
                parts,
                hole_spans: spans,
            };
            for tag in [
                Tag::Sql,
                Tag::Html,
                Tag::Cmd,
                Tag::Url,
                Tag::Path,
                Tag::Regex,
            ] {
                let out = check_template(tag, &sp, FileId(0), Span::new(100, 200));
                assert_eq!(out.holes.len(), holes, "{tag:?} {sp:?}");
            }
        }
    }
}
