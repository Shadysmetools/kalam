//! A parsed source file plus the layout facts the printer needs: raw tokens, comments
//! (own-line or line-trailing), blank lines and line numbers.

use std::collections::{BTreeSet, HashMap};

use kalam_ast::{Ast, TriviaKind as AstTriviaKind};
use kalam_base::{FileId, LineIndex, Skin};
use kalam_diagnostics::Diagnostic;
use kalam_lexer::{Op, Token, TokenKind, TriviaKind};

/// One `#` or `##` comment of the source.
#[derive(Clone, Debug)]
pub(crate) struct Comment {
    pub start: u32,
    pub text: String,
    /// 0-based source line.
    pub line: usize,
    /// Column in characters (the comment's indentation when it is on its own line).
    pub col: usize,
    /// Nothing but spaces before it on its line.
    pub own_line: bool,
}

pub(crate) struct Source<'a> {
    pub text: &'a str,
    pub skin: Skin,
    pub ast: Ast,
    /// The lexer's tokens, before the parser's leading-dot continuation pass.
    pub tokens: Vec<Token>,
    pub comments: Vec<Comment>,
    /// End offset of the last code token on a line → index of the comment that ends that
    /// line (a line-trailing comment).
    pub trailing: HashMap<u32, usize>,
    /// 0-based lines that are blank (outside string literals).
    pub blank_lines: BTreeSet<usize>,
    pub lines: LineIndex,
}

impl<'a> Source<'a> {
    /// Lexes and parses; any error-severity diagnostic is returned instead.
    pub fn parse(text: &'a str, skin: Skin) -> Result<Source<'a>, Vec<Diagnostic>> {
        let lexed = kalam_lexer::lex(text, skin, FileId(0));
        let parsed = kalam_parser::parse(text, &lexed, skin, FileId(0));
        if lexed.has_errors() || parsed.diagnostics.iter().any(|d| d.is_error()) {
            let mut all = lexed.diagnostics;
            all.extend(parsed.diagnostics);
            all.retain(|d| d.is_error());
            return Err(all);
        }
        let lines = LineIndex::new(text);
        let tokens: Vec<Token> = lexed.tokens;
        let mut comments = Vec::new();
        let mut trailing = HashMap::new();
        for t in &lexed.trivia {
            if !matches!(t.kind, TriviaKind::Comment | TriviaKind::DocComment) {
                continue;
            }
            let start = t.span.start;
            let line = lines.line_of(start);
            let line_start = lines.line_range(text, line).start;
            let before = &text[line_start..start as usize];
            let own_line = before.bytes().all(|b| b == b' ');
            if !own_line {
                // The last code token before the comment ends this line's code.
                let prev = tokens
                    .iter()
                    .rev()
                    .find(|k| !is_layout(k.kind) && k.span.end <= start);
                if let Some(prev) = prev {
                    trailing.insert(prev.span.end, comments.len());
                }
            }
            comments.push(Comment {
                start,
                text: text[t.span.range()].to_string(),
                line,
                col: before.chars().count(),
                own_line,
            });
        }
        let blank_lines = parsed
            .ast
            .trivia
            .iter()
            .filter(|t| t.kind == AstTriviaKind::BlankLine)
            .map(|t| t.line as usize - 1)
            .collect();
        Ok(Source {
            text,
            skin,
            ast: parsed.ast,
            tokens,
            comments,
            trailing,
            blank_lines,
            lines,
        })
    }

    pub fn line_of(&self, offset: u32) -> usize {
        self.lines.line_of(offset.min(self.text.len() as u32))
    }

    /// True if a blank line lies strictly between source lines `a` and `b`.
    pub fn blank_between(&self, a: usize, b: usize) -> bool {
        a + 1 < b && self.blank_lines.range(a + 1..b).next().is_some()
    }

    /// The first code token that starts at or after `offset`.
    pub fn next_token(&self, offset: u32) -> Option<Token> {
        let i = self.tokens.partition_point(|t| t.span.start < offset);
        self.tokens[i..]
            .iter()
            .find(|t| !is_layout(t.kind))
            .copied()
    }

    /// End of the block colon (a `:` directly followed by a NEWLINE) at or after `offset`.
    pub fn block_colon_end(&self, offset: u32) -> Option<u32> {
        let i = self.tokens.partition_point(|t| t.span.start < offset);
        self.tokens[i..].windows(2).find_map(|w| {
            (w[0].kind == TokenKind::Op(Op::Colon) && w[1].kind == TokenKind::Newline)
                .then_some(w[0].span.end)
        })
    }
}

pub(crate) fn is_layout(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
    )
}
