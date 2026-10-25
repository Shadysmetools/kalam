//! Kalam Canonical Form (spec §4.4): a skin-free serialization of the CST. Its SHA-256
//! ([`KcfHash`]) is the unit of identity: it is signed, recorded in `kalam.lock`, stored by
//! the registry, and compared by parity gate P1.
//!
//! Two files that are conversions of each other (spec §4.3) have byte-identical KCF
//! (guarantee G1), and so does a file and its `kalam fmt` output (`kcf(fmt(x)) == kcf(x)`).
//!
//! # What is recorded
//!
//! - keywords and core names → their concept ID (`KW_IF`, `CORE_PRINT`), whatever the
//!   spelling (canonical, hamza/alef variant, Franco alias);
//! - a reserved word in a §5.3 rule 8 position (field or method after `.`, a named-argument
//!   or field-pattern label, a record field name, a `use` path segment or item, an attribute
//!   name) → an identifier with its own text, exactly as the parser's AST reads it;
//! - identifiers → their exact code points (no folding), without raw-identifier backticks;
//! - `،`/`,` → `PUNCT_COMMA`, `؟`/`?` → `PUNCT_TRY`, other punctuation as ASCII;
//! - numbers → kind (int with its radix, or float) plus exact value: digit glyphs, `_`
//!   separators, hex digit case and an exponent's `+` are not recorded;
//! - string and tagged literals → the text outside interpolation holes byte for byte
//!   (tag, quotes and escapes included); the code inside each `{…}` hole is re-lexed in the
//!   file's skin and serialized like any other code;
//! - comments and doc comments → byte for byte (trailing spaces trimmed), each marked as on
//!   its own line or at the end of a code line;
//! - logical-line structure → the parser's `NEWLINE`/`INDENT`/`DEDENT` tokens;
//! - blank lines → one record each, except blank lines at the end of the file.
//!
//! Not recorded, because `fmt` may change it and it carries no meaning: spaces between
//! tokens, indentation bytes (the INDENT/DEDENT tokens carry the structure), `\r`, line
//! breaks inside brackets (§5.8 bracket suppression), trailing blank lines and a missing
//! final newline. CST node boundaries are not recorded either: they are implied by the
//! tokens, and leaving them out keeps hashes stable when the CST gets finer.
//!
//! # Byte format (version 1)
//!
//! UTF-8. The magic [`MAGIC`] (`KCF1\n`), then one record per line: a tag, then for most
//! tags a space and a payload. Payloads that hold source text are length-prefixed
//! (`<byte length>:<bytes>`), so identifier or string text can never be mistaken for
//! structure, even when it contains a newline.
//!
//! | Record | Meaning |
//! |---|---|
//! | `K KW_IF` | keyword or core name (concept ID) |
//! | `I 4:name` | identifier |
//! | `P (` · `P PUNCT_COMMA` | punctuation |
//! | `N i 16 31` | integer literal: radix, exact decimal value |
//! | `N f 1.5e-3` | float literal: ASCII digits, no `_`, lowercase `e`, no exponent `+` |
//! | `N ? 3:3ab` | a malformed number (only in files with lexer errors) |
//! | `S` … `/S` | a string or tagged literal, holding `T` and `H` records |
//! | `T 7:"Hello ` | literal text outside holes, byte for byte |
//! | `H` … `/H` | one interpolation hole, holding its code's records |
//! | `R 3:a b` | hole code nested too deep to re-lex (only in invalid files) |
//! | `# own 6:# note` · `# end …` | comment on its own line, or after code |
//! | `## own 8:## docs` | doc comment |
//! | `NL` · `IN` · `DE` | logical NEWLINE, INDENT, DEDENT |
//! | `B` | one blank line |
//! | `X 1:;` | source bytes that are neither tokens, comments nor spaces (lexer errors) |
//!
//! Everything is computed with explicit loops over a flat token list (no recursion over the
//! tree), so a tree at the parser's height cap serializes on a 1 MiB stack.

use std::fmt;

use kalam_base::{FileId, Skin};
use kalam_lexer::{TokenKind, TriviaKind};
use kalam_skins::{is_tatweel_or_haraka, Concept, KeywordTable};

use crate::{Cst, GreenNode, SyntaxKind};

/// The first bytes of every KCF serialization (format version 1).
pub const MAGIC: &[u8] = b"KCF1\n";

/// Strings nest inside interpolation holes only in invalid files (a string literal inside a
/// hole is K0131); past this depth the hole code is recorded raw.
const MAX_HOLE_DEPTH: u32 = 16;

/// SHA-256 of the KCF bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KcfHash(pub [u8; 32]);

impl KcfHash {
    /// The hash of already serialized KCF bytes.
    pub fn of(kcf: &[u8]) -> KcfHash {
        KcfHash(crate::sha256::digest(kcf))
    }

    /// Lowercase hex (64 digits), as `kalam.lock` and review tools show it.
    pub fn to_hex(&self) -> String {
        crate::sha256::hex(&self.0)
    }
}

impl fmt::Display for KcfHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Serializes a CST skin-free. The file's skin (needed only to re-lex interpolation holes)
/// is inferred from its keyword, core-name, punctuation and digit spellings; use
/// [`serialize_in`] when the skin is known.
pub fn serialize(cst: &Cst) -> Vec<u8> {
    let leaves = leaves(&cst.root);
    let skin = infer_skin_of(&leaves);
    serialize_leaves(&leaves, skin)
}

/// Serializes a CST whose file is written in `skin`.
pub fn serialize_in(cst: &Cst, skin: Skin) -> Vec<u8> {
    serialize_leaves(&leaves(&cst.root), skin)
}

/// The KCF hash of a CST (SHA-256 of [`serialize`]).
pub fn hash(cst: &Cst) -> KcfHash {
    KcfHash::of(&serialize(cst))
}

/// The skin a CST was parsed in, as far as its spellings tell: a keyword or core-name
/// spelling, `،`/`؟`, or an Arabic-Indic digit. A file with none of these (no keywords at
/// all) is reported as `en`; its KCF does not depend on the skin unless a hole holds a
/// keyword, which then would also be the only keyword in the file.
pub fn infer_skin(cst: &Cst) -> Skin {
    infer_skin_of(&leaves(&cst.root))
}

// ----- flat token list --------------------------------------------------------------------

/// One CST token with the kind of the node that holds it.
#[derive(Clone, Copy)]
struct Leaf<'a> {
    kind: SyntaxKind,
    text: &'a str,
    parent: SyntaxKind,
}

fn leaves(root: &GreenNode) -> Vec<Leaf<'_>> {
    root.tokens()
        .into_iter()
        .map(|(parent, t)| Leaf {
            kind: t.kind,
            text: &t.text,
            parent,
        })
        .collect()
}

/// Tokens that take part in the grammar: everything except spaces, comments and the empty
/// tokens the parser inserts for missing input. Layout tokens are zero-width but count.
fn significant(l: &Leaf) -> bool {
    match l.kind {
        SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::DocComment => false,
        SyntaxKind::Newline | SyntaxKind::Indent | SyntaxKind::Dedent => true,
        _ => !l.text.is_empty(),
    }
}

fn is_punct(l: Option<&Leaf>, text: &str) -> bool {
    matches!(l, Some(l) if l.kind == SyntaxKind::Punct && l.text == text)
}

fn is_comma(l: Option<&Leaf>) -> bool {
    is_punct(l, ",") || is_punct(l, "\u{060C}")
}

fn is_layout(l: Option<&Leaf>) -> bool {
    matches!(
        l,
        Some(l) if matches!(l.kind, SyntaxKind::Newline | SyntaxKind::Indent | SyntaxKind::Dedent)
    )
}

/// For every leaf: true if it is a keyword or core-name token in a §5.3 rule 8 position,
/// where the parser reads it as an identifier (`word_ident`). These positions mirror the
/// parser's `at_word` call sites.
fn rule_8_identifiers(leaves: &[Leaf]) -> Vec<bool> {
    let sig: Vec<usize> = (0..leaves.len())
        .filter(|&i| significant(&leaves[i]))
        .collect();
    let mut out = vec![false; leaves.len()];
    let mut open: Vec<&str> = Vec::new();
    for (k, &i) in sig.iter().enumerate() {
        let l = &leaves[i];
        if matches!(l.kind, SyntaxKind::Keyword(_) | SyntaxKind::Core(_)) {
            let prev = k.checked_sub(1).map(|p| &leaves[sig[p]]);
            let next = sig.get(k + 1).map(|&n| &leaves[n]);
            out[i] = is_rule_8(l, prev, next, open.last().copied());
        }
        if l.kind == SyntaxKind::Punct {
            match l.text {
                "(" | "[" | "{" => open.push(l.text),
                ")" | "]" | "}" => {
                    open.pop();
                }
                _ => {}
            }
        }
    }
    out
}

fn is_rule_8(l: &Leaf, prev: Option<&Leaf>, next: Option<&Leaf>, open: Option<&str>) -> bool {
    // A member name after `.` (the lexer already makes these identifiers).
    if is_punct(prev, ".") {
        return true;
    }
    let colon_next = is_punct(next, ":");
    match l.parent {
        // `use a/b/c`, `use a/{b, c}`: path segments and item names (but `self`).
        SyntaxKind::UseDecl => {
            let after_use =
                matches!(prev, Some(p) if p.kind == SyntaxKind::Keyword(Concept::KwUse));
            if after_use || is_punct(prev, "/") {
                return true;
            }
            if (is_punct(prev, "{") || is_comma(prev))
                && l.kind != SyntaxKind::Keyword(Concept::KwSelf)
            {
                return true;
            }
        }
        // `@name` (later segments follow `.`).
        SyntaxKind::Attribute if is_punct(prev, "@") => return true,
        // A record field name: first on its line (after any attributes), before `:`.
        SyntaxKind::TypeDecl => {
            let line_start =
                is_layout(prev) || matches!(prev, Some(p) if p.parent == SyntaxKind::Attribute);
            if colon_next && line_start {
                return true;
            }
        }
        _ => {}
    }
    // A label inside `(…)`: named argument, variant field, field pattern.
    open == Some("(") && colon_next && (is_punct(prev, "(") || is_comma(prev))
}

// ----- skin inference ---------------------------------------------------------------------

fn skin_index(skin: Skin) -> usize {
    match skin {
        Skin::En => 0,
        Skin::Ar => 1,
        Skin::Franco => 2,
    }
}

fn infer_skin_of(leaves: &[Leaf]) -> Skin {
    let table = KeywordTable::builtin();
    let ar_only = [false, true, false];
    let mut candidates = [true; 3];
    for l in leaves {
        let mask = match l.kind {
            SyntaxKind::Keyword(c) | SyntaxKind::Core(c) => {
                let word: String = l
                    .text
                    .chars()
                    .filter(|&ch| !is_tatweel_or_haraka(ch))
                    .collect();
                Skin::ALL.map(|s| table.skin(s).lookup_word(&word).map(|m| m.concept) == Some(c))
            }
            SyntaxKind::Punct if matches!(l.text, "\u{060C}" | "\u{061F}") => ar_only,
            SyntaxKind::IntLit | SyntaxKind::FloatLit
                if l.text.chars().any(kalam_lexer::is_arabic_indic_digit) =>
            {
                ar_only
            }
            _ => continue,
        };
        let narrowed = [0, 1, 2].map(|i| candidates[i] && mask[i]);
        // A token no skin explains (only in a broken file) does not wipe out the evidence.
        if narrowed.contains(&true) {
            candidates = narrowed;
        }
        if candidates.iter().filter(|&&c| c).count() == 1 {
            break;
        }
    }
    Skin::ALL
        .into_iter()
        .find(|&s| candidates[skin_index(s)])
        .unwrap_or(Skin::En)
}

// ----- writer -----------------------------------------------------------------------------

struct Ser {
    out: Vec<u8>,
    skin: Skin,
    /// The current physical line already has a token or a comment.
    line_has_content: bool,
    /// Blank lines seen but not yet written: they are written before the next content, so
    /// blank lines at the end of the file are dropped.
    pending_blanks: usize,
}

fn serialize_leaves(leaves: &[Leaf], skin: Skin) -> Vec<u8> {
    let mut s = Ser {
        out: MAGIC.to_vec(),
        skin,
        line_has_content: false,
        pending_blanks: 0,
    };
    s.leaves(leaves, 0);
    s.out
}

impl Ser {
    fn rec(&mut self, tag: &str) {
        self.out.extend_from_slice(tag.as_bytes());
        self.out.push(b'\n');
    }

    fn word(&mut self, tag: &str, word: &str) {
        self.out.extend_from_slice(tag.as_bytes());
        self.out.push(b' ');
        self.out.extend_from_slice(word.as_bytes());
        self.out.push(b'\n');
    }

    fn text(&mut self, tag: &str, text: &str) {
        self.out.extend_from_slice(tag.as_bytes());
        self.out.push(b' ');
        self.out
            .extend_from_slice(text.len().to_string().as_bytes());
        self.out.push(b':');
        self.out.extend_from_slice(text.as_bytes());
        self.out.push(b'\n');
    }

    /// Writes pending blank lines; returns true if the content starts its line.
    fn content(&mut self) -> bool {
        for _ in 0..std::mem::take(&mut self.pending_blanks) {
            self.rec("B");
        }
        let own_line = !self.line_has_content;
        self.line_has_content = true;
        own_line
    }

    fn leaves(&mut self, leaves: &[Leaf], depth: u32) {
        let rule_8 = rule_8_identifiers(leaves);
        for (i, l) in leaves.iter().enumerate() {
            match l.kind {
                SyntaxKind::Whitespace => self.whitespace(l.text),
                SyntaxKind::Comment | SyntaxKind::DocComment => {
                    let own = self.content();
                    let tag = if l.kind == SyntaxKind::Comment {
                        "#"
                    } else {
                        "##"
                    };
                    let place = if own { "own" } else { "end" };
                    let text = l.text.trim_end_matches([' ', '\r']);
                    self.out.extend_from_slice(tag.as_bytes());
                    self.out.push(b' ');
                    self.text(place, text);
                }
                SyntaxKind::Newline => self.rec("NL"),
                SyntaxKind::Indent => self.rec("IN"),
                SyntaxKind::Dedent => self.rec("DE"),
                _ if l.text.is_empty() => {}
                SyntaxKind::Keyword(c) | SyntaxKind::Core(c) => {
                    self.content();
                    if rule_8[i] {
                        self.text("I", strip_backticks(l.text));
                    } else {
                        self.word("K", c.name());
                    }
                }
                SyntaxKind::Ident => {
                    self.content();
                    self.text("I", strip_backticks(l.text));
                }
                SyntaxKind::IntLit | SyntaxKind::FloatLit => {
                    self.content();
                    self.number(l.kind == SyntaxKind::IntLit, l.text);
                }
                SyntaxKind::StrLit | SyntaxKind::TaggedStrLit => {
                    self.content();
                    self.string(l.text, depth);
                }
                SyntaxKind::Punct => {
                    self.content();
                    let p = match l.text {
                        "," | "\u{060C}" => Concept::PunctComma.name(),
                        "?" | "\u{061F}" => Concept::PunctTry.name(),
                        other => other,
                    };
                    self.word("P", p);
                }
                _ => {
                    self.content();
                    self.text("X", l.text);
                }
            }
        }
    }

    /// Spaces and `\r` are layout. A `\n` ends a line, which is blank if it had no content.
    /// Any other byte (left between tokens by a lexer error) is recorded as `X`.
    fn whitespace(&mut self, text: &str) {
        let mut junk_start = None;
        for (i, c) in text.char_indices() {
            let layout = matches!(c, ' ' | '\r' | '\n');
            if layout {
                if let Some(s) = junk_start.take() {
                    self.content();
                    self.text("X", &text[s..i]);
                }
            } else if junk_start.is_none() {
                junk_start = Some(i);
            }
            if c == '\n' {
                if !self.line_has_content {
                    self.pending_blanks += 1;
                }
                self.line_has_content = false;
            }
        }
        if let Some(s) = junk_start {
            self.content();
            self.text("X", &text[s..]);
        }
    }

    fn number(&mut self, int: bool, text: &str) {
        let ascii: String = kalam_lexer::to_ascii_digits(text)
            .chars()
            .filter(|&c| c != '_')
            .collect();
        if int {
            let (radix, digits) = match ascii.get(..2) {
                Some("0x") => (16, &ascii[2..]),
                Some("0b") => (2, &ascii[2..]),
                Some("0o") => (8, &ascii[2..]),
                _ => (10, ascii.as_str()),
            };
            if let Some(value) = to_decimal(digits, radix) {
                self.word("N", &format!("i {radix} {value}"));
                return;
            }
        } else {
            let canonical = ascii.replace('E', "e").replace("e+", "e");
            let well_formed = canonical.starts_with(|c: char| c.is_ascii_digit())
                && canonical
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | '-'));
            if well_formed {
                self.word("N", &format!("f {canonical}"));
                return;
            }
        }
        self.out.extend_from_slice(b"N ");
        self.text("?", &ascii);
    }

    fn string(&mut self, text: &str, depth: u32) {
        self.rec("S");
        for seg in split_holes(text) {
            match seg {
                Segment::Text(t) => self.text("T", t),
                Segment::Hole(code) => self.hole(code, depth + 1),
            }
        }
        self.rec("/S");
    }

    /// Hole code is code (§4.4): it is re-lexed in the file's skin, exactly as the parser
    /// does, and serialized like the rest of the file.
    fn hole(&mut self, code: &str, depth: u32) {
        if depth > MAX_HOLE_DEPTH {
            self.text("R", code);
            return;
        }
        let lexed = kalam_lexer::lex(code, self.skin, FileId(0));
        let slice = |start: u32, end: u32| -> &str {
            let (a, b) = (start.min(end) as usize, start.max(end) as usize);
            code.get(a..b).unwrap_or("")
        };
        let mut items: Vec<(u32, Leaf)> = Vec::new();
        for t in &lexed.tokens {
            let kind = match t.kind {
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => {
                    continue
                }
                TokenKind::Ident => SyntaxKind::Ident,
                TokenKind::Keyword(c) => SyntaxKind::Keyword(c),
                TokenKind::Core(c) => SyntaxKind::Core(c),
                TokenKind::Int => SyntaxKind::IntLit,
                TokenKind::Float => SyntaxKind::FloatLit,
                TokenKind::Str => SyntaxKind::StrLit,
                TokenKind::TaggedStr => SyntaxKind::TaggedStrLit,
                TokenKind::Op(_) => SyntaxKind::Punct,
            };
            let text = slice(t.span.start, t.span.end);
            items.push((t.span.start, leaf(kind, text)));
        }
        for t in &lexed.trivia {
            let kind = match t.kind {
                TriviaKind::Comment => SyntaxKind::Comment,
                TriviaKind::DocComment => SyntaxKind::DocComment,
            };
            items.push((t.span.start, leaf(kind, slice(t.span.start, t.span.end))));
        }
        items.sort_by_key(|(start, _)| *start);
        let hole_leaves: Vec<Leaf> = items.into_iter().map(|(_, l)| l).collect();
        self.rec("H");
        self.leaves(&hole_leaves, depth);
        self.rec("/H");
    }
}

fn leaf(kind: SyntaxKind, text: &str) -> Leaf<'_> {
    Leaf {
        kind,
        text,
        parent: SyntaxKind::Expr,
    }
}

fn strip_backticks(text: &str) -> &str {
    text.strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(text)
}

/// The exact value of a digit string in `radix`, in decimal. `None` if it is empty or holds
/// a non-digit. Arbitrary precision: an integer literal may be a `BigInt` (§5.6).
fn to_decimal(digits: &str, radix: u32) -> Option<String> {
    const BASE: u64 = 1_000_000_000;
    if digits.is_empty() {
        return None;
    }
    // Little-endian limbs in base 10^9.
    let mut limbs: Vec<u64> = vec![0];
    for c in digits.chars() {
        let mut carry = u64::from(c.to_digit(radix)?);
        for limb in limbs.iter_mut() {
            let v = *limb * u64::from(radix) + carry;
            *limb = v % BASE;
            carry = v / BASE;
        }
        while carry > 0 {
            limbs.push(carry % BASE);
            carry /= BASE;
        }
    }
    let mut out = String::new();
    let mut iter = limbs.iter().rev();
    if let Some(top) = iter.next() {
        out.push_str(&top.to_string());
    }
    for limb in iter {
        out.push_str(&format!("{limb:09}"));
    }
    Some(out)
}

enum Segment<'a> {
    Text(&'a str),
    Hole(&'a str),
}

/// Splits a string or tagged-literal token into the literal text outside holes (tag, quotes
/// and escapes included) and the trimmed code of each hole. Mirrors the parser's
/// `str_lit`: `\` escapes the next character, `\u{…}` braces belong to the escape, and a
/// hole's braces may nest. An unclosed hole is literal text.
fn split_holes(text: &str) -> Vec<Segment<'_>> {
    let tag_len = text.find('"').unwrap_or(0);
    let body = &text[tag_len..];
    let triple = body.starts_with("\"\"\"");
    let q = (if triple { 3 } else { 1 }).min(body.len());
    let closed = if triple {
        body.len() >= 6 && body.ends_with("\"\"\"")
    } else {
        body.len() >= 2 && body.ends_with('"')
    };
    let inner_end = if closed { body.len() - q } else { body.len() };
    let inner = body.get(q..inner_end).unwrap_or("");
    let base = tag_len + q;

    let mut out = Vec::new();
    let mut cursor = 0usize; // into `text`
    let mut chars = inner.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, n)) = chars.next() {
                    if n == 'u' && matches!(chars.peek(), Some(&(_, '{'))) {
                        for (_, d) in chars.by_ref() {
                            if d == '}' {
                                break;
                            }
                        }
                    }
                }
            }
            '{' => {
                let mut depth = 1usize;
                let mut end = None;
                for (j, d) in inner[i + 1..].char_indices() {
                    match d {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = Some(i + 1 + j);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let Some(end) = end else { break };
                if base + i > cursor {
                    out.push(Segment::Text(&text[cursor..base + i]));
                }
                out.push(Segment::Hole(inner[i + 1..end].trim()));
                cursor = base + end + 1;
                while matches!(chars.peek(), Some(&(k, _)) if k <= end) {
                    chars.next();
                }
            }
            _ => {}
        }
    }
    if cursor < text.len() {
        out.push(Segment::Text(&text[cursor..]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(text: &str) -> Vec<String> {
        split_holes(text)
            .into_iter()
            .map(|s| match s {
                Segment::Text(t) => format!("T{t}"),
                Segment::Hole(h) => format!("H{h}"),
            })
            .collect()
    }

    #[test]
    fn holes_split_like_the_parser() {
        assert_eq!(segs("\"a {x} b\""), ["T\"a ", "Hx", "T b\""]);
        assert_eq!(segs("\"{ a }{b}\""), ["T\"", "Ha", "Hb", "T\""]);
        assert_eq!(segs("\"\\{x\\}\""), ["T\"\\{x\\}\""]);
        assert_eq!(segs("\"\\u{41}{x}\""), ["T\"\\u{41}", "Hx", "T\""]);
        assert_eq!(segs("\"{ {1: 2}[1] }\""), ["T\"", "H{1: 2}[1]", "T\""]);
        assert_eq!(segs("sql\"select {x}\""), ["Tsql\"select ", "Hx", "T\""]);
        assert_eq!(
            segs("\"\"\"\n{x}\n\"\"\""),
            ["T\"\"\"\n", "Hx", "T\n\"\"\""]
        );
        assert_eq!(segs("\"open {x"), ["T\"open {x"]);
        assert_eq!(segs("\"\""), ["T\"\""]);
        assert_eq!(segs("\""), ["T\""]);
    }

    #[test]
    fn decimal_values_are_exact() {
        assert_eq!(to_decimal("0", 10).as_deref(), Some("0"));
        assert_eq!(to_decimal("1f", 16).as_deref(), Some("31"));
        assert_eq!(to_decimal("1F", 16).as_deref(), Some("31"));
        assert_eq!(to_decimal("101", 2).as_deref(), Some("5"));
        assert_eq!(to_decimal("777", 8).as_deref(), Some("511"));
        assert_eq!(
            to_decimal("ffffffffffffffffffffffffffffffffff", 16).as_deref(),
            Some("87112285931760246646623899502532662132735")
        );
        assert_eq!(
            to_decimal("123456789012345678901234567890", 10).as_deref(),
            Some("123456789012345678901234567890")
        );
        assert_eq!(to_decimal("", 10), None);
        assert_eq!(to_decimal("12a", 10), None);
    }
}
