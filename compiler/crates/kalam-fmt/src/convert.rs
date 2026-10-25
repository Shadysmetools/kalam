//! The layout-preserving token rewriter behind `kalam convert` (spec §4.3).
//!
//! Every byte between tokens (spaces, newlines, comments, blank lines) is copied unchanged,
//! so conversion keeps line numbers. Tokens are re-spelled for the target skin:
//! - keywords and core names → the target's canonical spelling (variants are canonicalized);
//! - identifiers keep their exact text, with backticks added when the name would lex as a
//!   reserved word of the target skin and the position is not a §5.3 rule 8 position, and
//!   removed otherwise ([`crate::roles`]);
//! - `,`/`?` ↔ `،`/`؟` (always `،`/`؟` in `ar`);
//! - number digit glyphs (`ar` target only, per `[skin.ar] digits`);
//! - string and tagged-literal text is copied, but each interpolation hole is code and is
//!   rewritten like any other code (§4.4).

use kalam_base::{FileId, Skin};
use kalam_lexer::{Op, Token, TokenKind};
use kalam_skins::{Concept, KeywordTable};

use crate::roles::{Role, Roles};
use crate::{convert_digits, FmtConfig};

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
        for tok in tokens {
            let (s, e) = (tok.span.start as usize, tok.span.end as usize);
            if s < range.start || e > range.end || s >= e {
                continue;
            }
            let original = &text[s..e];
            let replacement = match tok.kind {
                TokenKind::Ident | TokenKind::Keyword(_) | TokenKind::Core(_) => {
                    Some(self.word(tok, original, base))
                }
                TokenKind::Op(Op::Comma) => Some(self.punct(Concept::PunctComma)),
                TokenKind::Op(Op::Question) => Some(self.punct(Concept::PunctTry)),
                // Member indices (`t.0`) are plain decimals, so they follow the same rule.
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

    fn punct(&self, c: Concept) -> String {
        self.table.spelling(c, self.to).to_string()
    }

    /// An identifier, keyword or core-name token.
    fn word(&self, tok: &Token, original: &str, base: u32) -> String {
        let role = self.roles.get(base + tok.span.start);
        match (role, tok.kind) {
            (None, TokenKind::Keyword(c) | TokenKind::Core(c)) => {
                self.table.spelling(c, self.to).to_string()
            }
            (role, _) => {
                let name = strip_backticks(original);
                escape(name, role.unwrap_or(Role::Bound), self.to, self.table)
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
                    let lexed = kalam_lexer::lex(trimmed, self.from, FileId(0));
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

pub(crate) fn strip_backticks(word: &str) -> &str {
    word.strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(word)
}

/// The identifier `name` as written in skin `to` at a position with `role`.
pub(crate) fn escape(name: &str, role: Role, to: Skin, table: &KeywordTable) -> String {
    if role == Role::Bound && table.skin(to).is_reserved(name) {
        format!("`{name}`")
    } else {
        name.to_string()
    }
}
