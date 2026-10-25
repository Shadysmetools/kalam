//! Recursive-descent parser for the grammar of spec §8.2.
//!
//! Output ([`ParseOutput`]):
//! - a **skin-free AST** (`kalam_ast::Ast`): every skin of one program gives the same tree
//!   (compare with `kalam_ast::shape`), with spans and trivia (comments, blank lines);
//! - a **lossless CST** (`kalam_syntax::Cst`): every byte of the file, including whitespace,
//!   comments and tokens skipped during error recovery, so `cst.text() == source` always;
//! - parse diagnostics with stable codes ([`codes`], `PARSE-CODES.md`).
//!
//! Design:
//! - **LL(1) after left-factoring at the four DR-34 points**, no backtracking: `if` at
//!   statement or rhs start (§8.3 rule 1), `(` unit/group/tuple, expression statement vs.
//!   assignment, named vs. positional argument. Keyword labels (`f(match: 1)`, §5.3 rule 8)
//!   need a second token of lookahead (CRITIQUE P1.3).
//! - **Skin-free:** the parser matches `TokenKind::Keyword(Concept)`, never spellings. A
//!   `Keyword`/`Core` token is accepted as an identifier where §5.3 rule 8 allows it.
//! - **Interpolation holes** are re-lexed in the file's skin and parsed as expressions.
//! - **Leading-dot continuation** (§5.8 rule 2, DR-05) is done by the lexer: a joined `.`
//!   line reaches the parser with no NEWLINE/INDENT/DEDENT before it.
//! - **Error recovery:** a missing token is reported once and inserted virtually. On an
//!   unexpected token the parser skips to the end of the logical line (and any block that
//!   line opens) or to the closing DEDENT, wrapping the skipped tokens in an `Error` node.
//!   The parser never panics: nesting is capped ([`codes::TOO_DEEP`]), every loop makes
//!   progress, and spans from the token stream are clamped before any slicing.
//! - The CST is coarse in this version: nodes for items, blocks, statements, `if`/`match`
//!   forms, case arms, loops, expressions, patterns and types; precedence structure lives in
//!   the AST.

pub mod codes;
mod expr;
mod grammar;

use kalam_ast::{Ast, Ident, Name, NameKind, Trivia as AstTrivia, TriviaKind as AstTriviaKind};
use kalam_base::{FileId, LineIndex, Skin, Span};
use kalam_diagnostics::Diagnostic;
use kalam_lexer::{LexOutput, Op, Token, TokenFlags, TokenKind, Trivia, TriviaKind};
use kalam_skins::Concept;
use kalam_syntax::{Cst, SyntaxKind, TreeBuilder};

pub struct ParseOutput {
    pub ast: Ast,
    pub cst: Cst,
    /// Parser diagnostics only (plus lexer findings inside interpolation holes). Use
    /// [`parse_source`] to get lexer and parser diagnostics together.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses one file's tokens (from `kalam_lexer::lex` with the same `skin`).
pub fn parse(text: &str, tokens: &LexOutput, skin: Skin, file: FileId) -> ParseOutput {
    let mut p = Parser::new(text, skin, file, tokens.tokens.clone(), &tokens.trivia);
    let root = p.start_node(SyntaxKind::File);
    let file_ast = p.file();
    let (cst, diagnostics) = p.finish(root);
    let trivia = collect_trivia(text, tokens);
    ParseOutput {
        ast: Ast {
            file: file_ast,
            trivia,
        },
        cst,
        diagnostics,
    }
}

/// Lexes and parses; the diagnostics are the lexer's followed by the parser's.
pub fn parse_source(text: &str, skin: Skin, file: FileId) -> ParseOutput {
    let lexed = kalam_lexer::lex(text, skin, file);
    let mut out = parse(text, &lexed, skin, file);
    let mut all = lexed.diagnostics;
    all.append(&mut out.diagnostics);
    out.diagnostics = all;
    out
}

/// Recursion depth limit (expressions, patterns, types, blocks). Deep enough for any real
/// program, shallow enough for a 1 MiB stack (the Windows main-thread default).
pub(crate) const MAX_NEST: u32 = 64;
/// Tree-height limit, including postfix/binary/unary chains, so dropping or walking the
/// tree recursively stays within the stack too.
pub(crate) const MAX_HEIGHT: u32 = 1024;

/// A CST node marker. `None` when CST building is off (inside interpolation holes).
#[derive(Clone, Copy)]
pub(crate) struct Marker(Option<usize>);

pub(crate) struct Parser<'t> {
    pub(crate) text: &'t str,
    pub(crate) skin: Skin,
    pub(crate) file: FileId,
    pub(crate) toks: Vec<Token>,
    pub(crate) pos: usize,
    pub(crate) diags: Vec<Diagnostic>,
    builder: TreeBuilder,
    open_nodes: usize,
    cursor: usize,
    trivia: &'t [Trivia],
    trivia_i: usize,
    pub(crate) cst_on: bool,
    /// End of the last consumed significant token (for AST spans).
    pub(crate) last_end: u32,
    pub(crate) nest: u32,
    pub(crate) height: u32,
    too_deep_reported: bool,
    /// Token index of the last reported error, so one bad token yields one diagnostic.
    pub(crate) last_error_pos: Option<usize>,
}

pub(crate) fn is_layout(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
    )
}

/// A span with ordered ends (token spans from fuzzed streams may be reversed).
pub(crate) fn mk_span(a: u32, b: u32) -> Span {
    Span {
        start: a.min(b),
        end: a.max(b),
    }
}

fn floor_boundary(text: &str, p: usize) -> usize {
    let mut p = p.min(text.len());
    while p > 0 && !text.is_char_boundary(p) {
        p -= 1;
    }
    p
}

impl<'t> Parser<'t> {
    pub(crate) fn new(
        text: &'t str,
        skin: Skin,
        file: FileId,
        toks: Vec<Token>,
        trivia: &'t [Trivia],
    ) -> Self {
        Parser {
            text,
            skin,
            file,
            toks,
            pos: 0,
            diags: Vec::new(),
            builder: TreeBuilder::default(),
            open_nodes: 0,
            cursor: 0,
            trivia,
            trivia_i: 0,
            cst_on: true,
            last_end: 0,
            nest: 0,
            height: 0,
            too_deep_reported: false,
            last_error_pos: None,
        }
    }

    // ----- token access -------------------------------------------------------------------

    pub(crate) fn nth(&self, n: usize) -> TokenKind {
        match self.toks.get(self.pos + n) {
            Some(t) => t.kind,
            None => TokenKind::Eof,
        }
    }

    pub(crate) fn peek(&self) -> TokenKind {
        self.nth(0)
    }

    pub(crate) fn cur(&self) -> Token {
        match self.toks.get(self.pos) {
            Some(t) => *t,
            None => {
                let end = self.toks.last().map(|t| t.span.end).unwrap_or(0);
                Token {
                    kind: TokenKind::Eof,
                    span: Span { start: end, end },
                    flags: TokenFlags::default(),
                }
            }
        }
    }

    pub(crate) fn cur_span(&self) -> Span {
        let s = self.cur().span;
        mk_span(s.start, s.end)
    }

    /// Start offset of the next token, for AST spans.
    pub(crate) fn start(&self) -> u32 {
        self.cur_span().start
    }

    pub(crate) fn span_from(&self, start: u32) -> Span {
        mk_span(start, self.last_end.max(start))
    }

    pub(crate) fn at_eof(&self) -> bool {
        self.peek() == TokenKind::Eof
    }

    pub(crate) fn at_op(&self, op: Op) -> bool {
        self.peek() == TokenKind::Op(op)
    }

    pub(crate) fn at_kw(&self, c: Concept) -> bool {
        self.peek() == TokenKind::Keyword(c)
    }

    /// End of a logical line: NEWLINE, or a DEDENT/EOF that closes it.
    pub(crate) fn at_line_end(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
        )
    }

    /// The token's source text (empty if its span is out of range).
    pub(crate) fn tok_text(&self, t: Token) -> &'t str {
        let s = floor_boundary(self.text, t.span.start.min(t.span.end) as usize);
        let e = floor_boundary(self.text, t.span.end.max(t.span.start) as usize);
        self.text.get(s..e).unwrap_or("")
    }

    /// The identifier spelled by a word token (backticks of a raw identifier removed).
    pub(crate) fn word_ident(&self, t: Token) -> Ident {
        let text = self.tok_text(t);
        let name = text
            .strip_prefix('`')
            .and_then(|s| s.strip_suffix('`'))
            .unwrap_or(text);
        Ident {
            name: name.to_string(),
            span: mk_span(t.span.start, t.span.end),
        }
    }

    pub(crate) fn name_of(&self, t: Token) -> Name {
        let kind = match t.kind {
            TokenKind::Core(c) => NameKind::Core(c),
            _ => NameKind::Ident(self.word_ident(t).name),
        };
        Name {
            kind,
            span: mk_span(t.span.start, t.span.end),
        }
    }

    /// Consumes one token (never the EOF token) and adds it to the CST.
    pub(crate) fn bump(&mut self) -> Token {
        let tok = self.cur();
        if tok.kind == TokenKind::Eof {
            return tok;
        }
        self.pos += 1;
        if self.cst_on {
            self.emit_token(tok);
        }
        if !is_layout(tok.kind) {
            self.last_end = tok.span.end.max(tok.span.start);
        }
        tok
    }

    pub(crate) fn eat_op(&mut self, op: Op) -> bool {
        if self.at_op(op) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn eat_kw(&mut self, c: Concept) -> bool {
        if self.at_kw(c) {
            self.bump();
            true
        } else {
            false
        }
    }

    // ----- diagnostics --------------------------------------------------------------------

    pub(crate) fn describe(&self, t: Token) -> String {
        match t.kind {
            TokenKind::Newline => "end of line".into(),
            TokenKind::Indent => "indentation".into(),
            TokenKind::Dedent => "end of block".into(),
            TokenKind::Eof => "end of file".into(),
            _ => format!("`{}`", self.tok_text(t)),
        }
    }

    /// Reports `code` at `span`, unless an error was already reported at this token.
    pub(crate) fn error_at(&mut self, code: &'static str, span: Span) -> bool {
        if self.last_error_pos == Some(self.pos) {
            return false;
        }
        self.last_error_pos = Some(self.pos);
        self.diags.push(Diagnostic::new(code, self.file, span));
        true
    }

    /// K0250 at the current token: "expected `what`, found …".
    pub(crate) fn expected(&mut self, what: &str) {
        if self.last_error_pos == Some(self.pos) {
            return;
        }
        let t = self.cur();
        let found = self.describe(t);
        self.last_error_pos = Some(self.pos);
        self.diags.push(
            Diagnostic::new(
                codes::UNEXPECTED_TOKEN,
                self.file,
                mk_span(t.span.start, t.span.end),
            )
            .arg("expected", what.to_string())
            .arg("found", found),
        );
    }

    pub(crate) fn expect_op(&mut self, op: Op, what: &str) -> bool {
        if self.eat_op(op) {
            true
        } else {
            self.expected(what);
            false
        }
    }

    pub(crate) fn expect_kw(&mut self, c: Concept, what: &str) -> bool {
        if self.eat_kw(c) {
            true
        } else {
            self.expected(what);
            false
        }
    }

    /// Enters one level of recursion. Returns false (after reporting K0259 once) when the
    /// nesting limit is reached; the caller must then return an error node and call `leave`.
    pub(crate) fn enter(&mut self) -> bool {
        self.nest += 1;
        self.height += 1;
        if self.nest > MAX_NEST || self.height > MAX_HEIGHT {
            self.too_deep();
            false
        } else {
            true
        }
    }

    pub(crate) fn leave(&mut self) {
        self.nest = self.nest.saturating_sub(1);
        self.height = self.height.saturating_sub(1);
    }

    pub(crate) fn too_deep(&mut self) {
        // The cut-off token is explained by K0259; no second K0250 on the same spot.
        self.last_error_pos = Some(self.pos);
        if !self.too_deep_reported {
            self.too_deep_reported = true;
            let span = self.cur_span();
            self.diags
                .push(Diagnostic::new(codes::TOO_DEEP, self.file, span));
        }
    }

    // ----- recovery -----------------------------------------------------------------------

    /// Skips to the end of the logical line (consuming its NEWLINE) and then any indented
    /// block that line opens. Stops before a DEDENT that closes the current block.
    pub(crate) fn skip_line(&mut self) {
        let m = self.start_node(SyntaxKind::Error);
        let mut depth = 0usize;
        loop {
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Newline if depth == 0 => {
                    self.bump();
                    if self.peek() == TokenKind::Indent {
                        self.skip_block_tokens();
                    }
                    break;
                }
                TokenKind::Dedent if depth == 0 => break,
                TokenKind::Indent => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::Dedent => {
                    depth -= 1;
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
        self.finish_node(m);
    }

    /// At an INDENT: skips the whole indented block, including its closing DEDENT.
    pub(crate) fn skip_block_tokens(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Indent => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::Dedent => {
                    self.bump();
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    // ----- CST ----------------------------------------------------------------------------

    pub(crate) fn start_node(&mut self, kind: SyntaxKind) -> Marker {
        if !self.cst_on {
            return Marker(None);
        }
        let before = self.open_nodes;
        self.builder.start_node(kind);
        self.open_nodes += 1;
        Marker(Some(before))
    }

    /// Closes the node opened by `m` (and any node left open inside it).
    pub(crate) fn finish_node(&mut self, m: Marker) {
        if let Marker(Some(before)) = m {
            while self.open_nodes > before {
                self.builder.finish_node();
                self.open_nodes -= 1;
            }
        }
    }

    fn syntax_kind(k: TokenKind) -> SyntaxKind {
        match k {
            TokenKind::Ident => SyntaxKind::Ident,
            TokenKind::Keyword(c) => SyntaxKind::Keyword(c),
            TokenKind::Core(c) => SyntaxKind::Core(c),
            TokenKind::Int => SyntaxKind::IntLit,
            TokenKind::Float => SyntaxKind::FloatLit,
            TokenKind::Str => SyntaxKind::StrLit,
            TokenKind::TaggedStr => SyntaxKind::TaggedStrLit,
            TokenKind::Op(_) => SyntaxKind::Punct,
            TokenKind::Newline => SyntaxKind::Newline,
            TokenKind::Indent => SyntaxKind::Indent,
            TokenKind::Dedent | TokenKind::Eof => SyntaxKind::Dedent,
        }
    }

    fn fix(&self, p: u32) -> usize {
        floor_boundary(self.text, p as usize)
    }

    /// Emits source text up to `upto` as whitespace and comment tokens.
    fn emit_gap(&mut self, upto: usize) {
        if upto <= self.cursor {
            return;
        }
        while let Some(t) = self.trivia.get(self.trivia_i).copied() {
            let ts = self.fix(t.span.start.min(t.span.end));
            let te = self.fix(t.span.end.max(t.span.start));
            if ts < self.cursor {
                self.trivia_i += 1;
                continue;
            }
            if ts >= upto {
                break;
            }
            self.emit_text(SyntaxKind::Whitespace, ts);
            let kind = match t.kind {
                TriviaKind::Comment => SyntaxKind::Comment,
                TriviaKind::DocComment => SyntaxKind::DocComment,
            };
            self.emit_text(kind, te.min(upto));
            self.trivia_i += 1;
        }
        self.emit_text(SyntaxKind::Whitespace, upto);
    }

    /// Emits `text[cursor..end]` as one token of `kind` and advances the cursor.
    fn emit_text(&mut self, kind: SyntaxKind, end: usize) {
        if end > self.cursor {
            let slice = self.text.get(self.cursor..end).unwrap_or("");
            self.builder.token(kind, slice);
            self.cursor = end;
        }
    }

    fn emit_token(&mut self, tok: Token) {
        let s = self.fix(tok.span.start.min(tok.span.end));
        let e = self.fix(tok.span.end.max(tok.span.start));
        let kind = Self::syntax_kind(tok.kind);
        if is_layout(tok.kind) {
            // Layout tokens are zero-width; their bytes (newline, indentation) are emitted
            // as whitespace before the next token, so no byte is duplicated.
            self.builder.token(kind, "");
            return;
        }
        self.emit_gap(s);
        if e > self.cursor {
            self.emit_text(kind, e);
        } else {
            self.builder.token(kind, "");
        }
    }

    /// Wraps any unconsumed tokens in an `Error` node, emits trailing text and closes the tree.
    fn finish(mut self, root: Marker) -> (Cst, Vec<Diagnostic>) {
        if self.toks[self.pos.min(self.toks.len())..]
            .iter()
            .any(|t| t.kind != TokenKind::Eof)
        {
            let m = self.start_node(SyntaxKind::Error);
            while self.pos < self.toks.len() {
                let t = self.toks[self.pos];
                self.pos += 1;
                if t.kind != TokenKind::Eof {
                    self.emit_token(t);
                }
            }
            self.finish_node(m);
        }
        let len = self.text.len();
        self.emit_gap(len);
        if let Marker(Some(_)) = root {
            while self.open_nodes > 0 {
                self.builder.finish_node();
                self.open_nodes -= 1;
            }
        }
        (
            Cst {
                root: self.builder.finish(),
            },
            self.diags,
        )
    }
}

/// Comments, doc comments and blank lines (outside string literals), in source order.
fn collect_trivia(text: &str, lexed: &LexOutput) -> Vec<AstTrivia> {
    let lines = LineIndex::new(text);
    let slice = |s: Span| -> String {
        let a = floor_boundary(text, s.start.min(s.end) as usize);
        let b = floor_boundary(text, s.end.max(s.start) as usize);
        text.get(a..b).unwrap_or("").to_string()
    };
    let mut out: Vec<AstTrivia> = lexed
        .trivia
        .iter()
        .map(|t| AstTrivia {
            kind: match t.kind {
                TriviaKind::Comment => AstTriviaKind::Comment,
                TriviaKind::DocComment => AstTriviaKind::DocComment,
            },
            text: slice(t.span),
            line: lines.line_of(t.span.start.min(text.len() as u32)) as u32 + 1,
            span: mk_span(t.span.start, t.span.end),
        })
        .collect();
    let strings: Vec<Span> = lexed
        .tokens
        .iter()
        .filter(|t| matches!(t.kind, TokenKind::Str | TokenKind::TaggedStr))
        .map(|t| mk_span(t.span.start, t.span.end))
        .collect();
    let mut offset = 0usize;
    for (n, line) in text.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches('\n').trim_end_matches('\r');
        if body.bytes().all(|b| b == b' ') {
            let at = offset as u32;
            let in_string = strings.iter().any(|s| s.start < at && at < s.end);
            if !in_string {
                out.push(AstTrivia {
                    kind: AstTriviaKind::BlankLine,
                    text: String::new(),
                    line: n as u32 + 1,
                    span: mk_span(at, at + body.len() as u32),
                });
            }
        }
        offset += line.len();
    }
    out.sort_by_key(|t| (t.span.start, t.kind != AstTriviaKind::BlankLine));
    out
}
