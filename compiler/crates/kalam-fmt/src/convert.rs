//! The layout-preserving token rewriter behind `kalam convert` (spec §4.3).
//!
//! Every byte between tokens (spaces, newlines, comments, blank lines) is copied unchanged,
//! so conversion keeps line numbers. Tokens are re-spelled for the target skin:
//! - keywords and core names → the target's canonical spelling (variants are canonicalized);
//! - identifiers keep their exact text, with backticks added when the name would lex as a
//!   reserved word of the target skin, or is reserved in some skin and not declared in the
//!   file (Q16), and the position is not a §5.3 rule 8 position, or when it touches another
//!   word with no space between them (Q26, [`glued`]); removed otherwise ([`crate::roles`]);
//! - `,`/`?` ↔ `،`/`؟` (always `،`/`؟` in `ar`);
//! - number digit glyphs (`ar` target only, per `[skin.ar] digits`);
//! - string and tagged-literal text is copied, but each interpolation hole is code and is
//!   rewritten like any other code (§4.4).

use kalam_base::{FileId, Skin};
use kalam_lexer::{Op, Token, TokenKind};
use kalam_skins::{Concept, KeywordTable};

use crate::roles::{Role, Roles};
use crate::{convert_digits, to_ascii_digits, FmtConfig};

pub(crate) struct Rewriter<'a> {
    pub from: Skin,
    pub to: Skin,
    pub config: &'a FmtConfig,
    pub table: &'a KeywordTable,
    pub roles: &'a Roles,
}

impl Rewriter<'_> {
    /// Rewrites `text[range]`, whose tokens are `tokens` (spans relative to `text`); `base` is
    /// the absolute file offset of `text[0]`, used to look identifiers up in the role map.
    pub fn rewrite(
        &self,
        text: &str,
        tokens: &[Token],
        base: u32,
        range: std::ops::Range<usize>,
        out: &mut String,
    ) {
        let mut copied = range.start;
        for (i, tok) in tokens.iter().enumerate() {
            let (s, e) = (tok.span.start as usize, tok.span.end as usize);
            if s < range.start || e > range.end || s >= e {
                continue;
            }
            let original = &text[s..e];
            let replacement = match tok.kind {
                TokenKind::Keyword(_) | TokenKind::Core(_) if self.respelled(tok, base) => {
                    let mut w = self.word(tok, original, base, false);
                    if let Some(p) = solid_before(tokens, i) {
                        if !self.boundary_holds(p, tok, text, base) {
                            w.insert(0, ' ');
                        }
                    }
                    if let Some(n) = solid_after(tokens, i) {
                        if !self.boundary_holds(tok, n, text, base) {
                            w.push(' ');
                        }
                    }
                    Some(w)
                }
                TokenKind::Ident | TokenKind::Keyword(_) | TokenKind::Core(_) => {
                    Some(self.word(tok, original, base, glued(tokens, i)))
                }
                TokenKind::Op(Op::Comma) => Some(self.punct(Concept::PunctComma)),
                TokenKind::Op(Op::Question) => Some(self.punct(Concept::PunctTry)),
                // DR-26: the lexer reads only an ASCII digit run right after `.` as a member
                // index, so a chain of numeric member indices (`t.0.1`) stays ASCII in every
                // skin: with Arabic-Indic digits, `t.٠.١` would re-lex as `t . ٠.١`, one
                // float (SEC0120). A lone index (`t.0`) follows the digit setting like any
                // other literal.
                TokenKind::Int if in_index_chain(tokens, i) => Some(to_ascii_digits(original)),
                TokenKind::Int | TokenKind::Float => {
                    Some(convert_digits(original, self.to, self.config))
                }
                TokenKind::Str | TokenKind::TaggedStr => {
                    Some(self.string(original, base + s as u32))
                }
                _ => None,
            };
            if let Some(r) = replacement {
                out.push_str(&text[copied..s]);
                out.push_str(&r);
                copied = e;
            }
        }
        out.push_str(&text[copied..range.end]);
    }

    /// A keyword or core-name token spelled from the keyword table (not one standing as a
    /// name, which [`crate::roles`] spells).
    fn respelled(&self, tok: &Token, base: u32) -> bool {
        self.roles.get(base + tok.span.start).is_none()
    }

    /// How `tok` is written in `skin`, as far as a re-spelled word touching it can tell: a
    /// string literal is `""` (only its opening quote touches the word), and every token this
    /// converter does not re-spell is its own text.
    fn sample(&self, tok: &Token, text: &str, base: u32, skin: Skin) -> String {
        let original = &text[tok.span.range()];
        match tok.kind {
            TokenKind::Keyword(c) | TokenKind::Core(c) if self.respelled(tok, base) => {
                self.table.spelling(c, skin).to_string()
            }
            TokenKind::Op(Op::Comma) => self.table.spelling(Concept::PunctComma, skin).into(),
            TokenKind::Op(Op::Question) => self.table.spelling(Concept::PunctTry, skin).into(),
            TokenKind::Int | TokenKind::Float => convert_digits(original, skin, self.config),
            TokenKind::Str => "\"\"".to_string(),
            TokenKind::TaggedStr => {
                let tag = original.find('"').unwrap_or(original.len());
                format!("{}\"\"", &original[..tag])
            }
            _ => original.to_string(),
        }
    }

    /// The general rule for glued tokens (spec §4.3 G1/G2). Conversion copies the bytes
    /// between tokens, so two tokens written with nothing between them stay glued, and the
    /// pair must re-lex as the same two tokens once either is re-spelled. `a` and `b` touch;
    /// this is true if, in every skin, their spellings lex as two tokens of the same kinds,
    /// split at the same place. Otherwise the converter separates them with one space, in
    /// every target skin (the source skin included), so the decision never depends on the
    /// target and every round trip stays byte-identical. Example: `أرجع"x"` is a keyword
    /// and a string, but `return"x"` and `rag3"x"`/`law"x"` lex as tagged literals.
    /// (A glued identifier keeps its backticks instead, see [`glued`].)
    fn boundary_holds(&self, a: &Token, b: &Token, text: &str, base: u32) -> bool {
        let samples: Vec<(Skin, String, String)> = Skin::ALL
            .into_iter()
            .map(|s| {
                (
                    s,
                    self.sample(a, text, base, s),
                    self.sample(b, text, base, s),
                )
            })
            .collect();
        let key = samples
            .iter()
            .map(|(_, l, r)| format!("{l}\u{0}{r}"))
            .collect::<Vec<_>>()
            .join("\u{1}");
        self.roles.boundary(key, || {
            samples
                .iter()
                .all(|(skin, l, r)| relexes_as_pair(l, r, a.kind, b.kind, *skin))
        })
    }

    fn punct(&self, c: Concept) -> String {
        self.table.spelling(c, self.to).to_string()
    }

    /// An identifier, keyword or core-name token. `glued`: see [`glued`].
    fn word(&self, tok: &Token, original: &str, base: u32, glued: bool) -> String {
        let role = self.roles.get(base + tok.span.start);
        match (role, tok.kind) {
            (None, TokenKind::Keyword(c) | TokenKind::Core(c)) => {
                self.table.spelling(c, self.to).to_string()
            }
            (role, _) => {
                let name = strip_backticks(original);
                let spelled =
                    self.roles
                        .spell(name, role.unwrap_or(Role::Bound), self.to, self.table);
                if glued && !spelled.starts_with('`') {
                    format!("`{name}`")
                } else {
                    spelled
                }
            }
        }
    }

    /// A string or tagged-literal token at absolute offset `abs`: the literal text is copied
    /// and every hole is rewritten. The hole scan mirrors `kalam-parser`'s `str_lit` exactly,
    /// so hole tokens get the same absolute offsets as the AST's hole identifiers.
    fn string(&self, raw: &str, abs: u32) -> String {
        let tag_len = if raw.starts_with('"') {
            0
        } else {
            raw.find('"').unwrap_or(raw.len())
        };
        let body = &raw[tag_len..];
        let triple = body.starts_with("\"\"\"");
        let q = (if triple { 3 } else { 1 }).min(body.len());
        let close = if triple {
            body.len() >= 6 && body.ends_with("\"\"\"")
        } else {
            body.len() >= 2 && body.ends_with('"')
        };
        let inner_end = if close { body.len() - q } else { body.len() };
        let Some(inner) = body.get(q..inner_end) else {
            return raw.to_string();
        };
        let inner_abs = abs + (tag_len + q) as u32;

        let mut out = String::with_capacity(raw.len());
        out.push_str(&raw[..tag_len + q]);
        let mut copied = 0usize;
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
                    let Some(end) = end else {
                        break;
                    };
                    let code = &inner[i + 1..end];
                    let lead = code.len() - code.trim_start().len();
                    let trimmed = code.trim();
                    out.push_str(&inner[copied..i + 1 + lead]);
                    let hole_abs = inner_abs + (i + 1 + lead) as u32;
                    let lexed = kalam_lexer::lex_fragment(trimmed, self.from, FileId(0));
                    let toks: Vec<Token> = lexed
                        .tokens
                        .into_iter()
                        .filter(|t| !matches!(t.kind, TokenKind::Str | TokenKind::TaggedStr))
                        .collect();
                    self.rewrite(trimmed, &toks, hole_abs, 0..trimmed.len(), &mut out);
                    copied = i + 1 + lead + trimmed.len();
                    while matches!(chars.peek(), Some(&(k, _)) if k <= end) {
                        chars.next();
                    }
                }
                _ => {}
            }
        }
        out.push_str(&inner[copied..]);
        out.push_str(&body[inner_end..]);
        out
    }
}

/// The word token `tokens[i]` touches another word or number token, or a string literal
/// right after it, with no byte between them. That only happens next to a backticked name
/// (`` return`Some` ``, `` `fe`ka ``): such a name keeps its backticks in every skin, or the two
/// words would glue into one (`أرجعSome` is SEC0105, `rag3Some` a different name) and the round
/// trip would break (FOUNDER-QUEUE Q26). The test depends only on the token layout, which
/// conversion keeps, so the decision is the same in every skin (spec §4.3 G2).
fn glued(tokens: &[Token], i: usize) -> bool {
    let Some(tok) = tokens.get(i) else {
        return false;
    };
    let solid = |t: &&Token| t.span.end > t.span.start;
    let wordish = |k: TokenKind| {
        matches!(
            k,
            TokenKind::Ident
                | TokenKind::Keyword(_)
                | TokenKind::Core(_)
                | TokenKind::Int
                | TokenKind::Float
        )
    };
    let before = tokens[..i].iter().rev().find(solid);
    let after = tokens[i + 1..].iter().find(solid);
    before.is_some_and(|p| p.span.end == tok.span.start && wordish(p.kind))
        || after.is_some_and(|n| {
            n.span.start == tok.span.end
                && (wordish(n.kind) || matches!(n.kind, TokenKind::Str | TokenKind::TaggedStr))
        })
}

/// The solid token right before `tokens[i]`, if nothing separates them.
fn solid_before(tokens: &[Token], i: usize) -> Option<&Token> {
    let tok = tokens.get(i)?;
    tokens[..i]
        .iter()
        .rev()
        .find(|t| t.span.end > t.span.start)
        .filter(|p| p.span.end == tok.span.start && !layout(p.kind))
}

/// The solid token right after `tokens[i]`, if nothing separates them.
fn solid_after(tokens: &[Token], i: usize) -> Option<&Token> {
    let tok = tokens.get(i)?;
    tokens[i + 1..]
        .iter()
        .find(|t| t.span.end > t.span.start)
        .filter(|n| n.span.start == tok.span.end && !layout(n.kind))
}

fn layout(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
    )
}

/// `tokens[i]` is a numeric member index (it follows a `.`, DR-26) next to another one:
/// `.` and a number come right after it, or right before its own `.`.
fn in_index_chain(tokens: &[Token], i: usize) -> bool {
    let solid = |t: &&Token| t.span.end > t.span.start;
    let mut before = tokens[..i].iter().rev().filter(solid).map(|t| t.kind);
    let mut after = tokens[i + 1..].iter().filter(solid).map(|t| t.kind);
    let dot = TokenKind::Op(Op::Dot);
    if before.next() != Some(dot) {
        return false;
    }
    let index_before = before.next() == Some(TokenKind::Int) && before.next() == Some(dot);
    let index_after = after.next() == Some(dot) && after.next() == Some(TokenKind::Int);
    index_before || index_after
}

/// `left` followed directly by `right` lexes in `skin` as exactly two tokens, split between
/// them, of kinds `a` and `b` (a number may come back as either number kind).
fn relexes_as_pair(left: &str, right: &str, a: TokenKind, b: TokenKind, skin: Skin) -> bool {
    let joined = format!("{left}{right}");
    let lexed = kalam_lexer::lex_fragment(&joined, skin, FileId(0));
    let solid: Vec<&Token> = lexed
        .tokens
        .iter()
        .filter(|t| t.span.end > t.span.start)
        .collect();
    let number = |k: TokenKind| matches!(k, TokenKind::Int | TokenKind::Float);
    let same = |x: TokenKind, y: TokenKind| x == y || (number(x) && number(y));
    solid.len() == 2
        && solid[0].span.end as usize == left.len()
        && same(solid[0].kind, a)
        && same(solid[1].kind, b)
}

pub(crate) fn strip_backticks(word: &str) -> &str {
    word.strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(word)
}
