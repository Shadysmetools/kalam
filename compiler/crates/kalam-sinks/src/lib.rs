//! Compile-time parsers for tagged literals: `sql"…"`, `html"…"`, `cmd"…"`, `url"…"`,
//! `path"…"` and `regex"…"` (spec §17, report 04 S1–S3).
//!
//! The type checker hands each tagged literal's [`StaticParts`] (the literal text the
//! developer wrote, which only the compiler can create) and its hole spans to the parser for
//! that tag. The parser checks the template's structure and classifies every hole by
//! context, returning the type each hole must have:
//!
//! | Tag | Checks | Hole rule |
//! |---|---|---|
//! | `sql` | quotes, comments, one statement (K1602–K1605) | bound parameter; never spliced; identifiers only via `sql.ident` (K1603) |
//! | `html` | HTML5 fragment well-formed (K1610), contexts (K1611–K1617) | text: `Show`/`Html`; attribute: escaped; URL attribute: `Url` (K1613); `k-<event>`: `Message` (K1616) |
//! | `cmd` | argv split, literal program (K1621), no shell metacharacters (K1620) | exactly one argv element; option-like guard (W1622) |
//! | `url` | literal scheme and host (K1631) | percent-encoded per component |
//! | `path` | — | `PathSegment` only (K1641) |
//! | `regex` | RE2 syntax | escaped as literal text |
//!
//! Status: data types only (Phase 1).

use kalam_base::Span;
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
    pub parts: Vec<String>,
    pub hole_spans: Vec<Span>,
}

/// What a hole must be, decided by its position in the template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoleRequirement {
    /// Any `SqlParam` type; sent as a bound parameter.
    SqlParam,
    /// `Html`, `List[Html]`, `Option[Html]` or any `Show` type (escaped).
    HtmlText,
    /// `String`, `Int`, `Bool` or `AttrValue` (attribute-escaped).
    HtmlAttr,
    /// `Url` only.
    Url,
    /// A type that derives `Message` (live-component events).
    Message,
    /// `String`, `Int`, `Path`, `PathSegment` or `List[String]` (one argv element each).
    Argv,
    PathSegment,
    /// Escaped as literal text.
    RegexLiteral,
}

pub struct TemplateCheck {
    pub holes: Vec<HoleRequirement>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn check_template(_tag: Tag, _parts: &StaticParts) -> TemplateCheck {
    todo!("tagged-literal parsers (Phase 1)")
}
