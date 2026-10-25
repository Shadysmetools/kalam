//! Known LLM mistakes (spec §21.3, K0901–K0926): detection, machine-applicable fixes and
//! recovery, plus the cascade filter of [`crate::merge_diagnostics`].
//!
//! Rules every detector follows:
//! - **Only in the obvious syntactic position** (§21.3): `def` must start an item and be
//!   followed by a name and `(`, `===` must be one glued token run, `null` must stand where
//!   an expression goes, and so on. A backticked raw identifier is never a mistake, and a
//!   bare word the file declares (FOUNDER-QUEUE Q9 style, see [`declared_names`]) is an
//!   ordinary name, so `let def = 1` and `fn len(x: Int)` stay legal.
//! - **One mistake, one report**: the K09xx code is reported *instead of* the generic
//!   K0250/K0255 at that site, and the parser recovers as if the canonical form had been
//!   written, so nothing cascades from it.
//! - **Fixes are spelled in the file's skin** (keywords from the builtin keyword table).
//!   A fix is `Machine` only when the rewrite is local and certain; otherwise it is a
//!   `Suggestion` (or absent when no rewrite is certain enough to offer).

use std::collections::HashSet;

use kalam_ast::*;
use kalam_base::{LineIndex, Span};
use kalam_diagnostics::{Applicability, Diagnostic, Edit, Fix};
use kalam_lexer::{Op, Token, TokenKind};
use kalam_skins::{Concept, KeywordTable};
use kalam_syntax::SyntaxKind;

use crate::grammar::is_place;
use crate::{codes, is_layout, mk_span, Parser};

/// How far (in tokens) a lookahead or lookbehind for a mistake's shape may scan. A
/// ternary, a block header or a `for (…)` header is a few dozen tokens; the cap keeps every
/// scan constant-time, because the lexer joins everything inside brackets into one logical
/// line and an unbounded scan per `?` would be quadratic. Hitting it means "no mistake".
const SCAN_CAP: usize = 256;

const DEF_WORDS: &[&str] = &["def", "function", "func"];
const CLASS_WORDS: &[&str] = &["class", "struct", "record", "interface"];
const NULL_WORDS: &[&str] = &["null", "nil", "undefined"];
const HANDLER_WORDS: &[&str] = &["catch", "except", "finally"];
const FREE_FUNCTIONS: &[&str] = &["len", "str", "int"];

/// What the hoisting fix of a multi-line lambda (K0921) needs.
pub(crate) struct BlockLambda {
    pub diag: usize,
    /// The `fn` keyword of the lambda.
    pub fn_span: Span,
    /// End offsets of parameter names written without a type.
    pub untyped: Vec<u32>,
}

fn edit(span: Span, insert: impl Into<String>) -> Edit {
    Edit {
        span,
        insert: insert.into(),
    }
}

fn at(offset: u32) -> Span {
    mk_span(offset, offset)
}

fn machine(title: &str, edits: Vec<Edit>) -> Fix {
    Fix {
        title: title.to_string(),
        applicability: Applicability::Machine,
        edits,
    }
}

fn suggestion(title: &str, edits: Vec<Edit>) -> Fix {
    Fix {
        title: title.to_string(),
        applicability: Applicability::Suggestion,
        edits,
    }
}

/// A token that can start an expression.
pub(crate) fn starts_expr(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Int
            | TokenKind::Float
            | TokenKind::Str
            | TokenKind::TaggedStr
            | TokenKind::Ident
            | TokenKind::Core(_)
            | TokenKind::Keyword(
                Concept::KwTrue
                    | Concept::KwFalse
                    | Concept::KwSelf
                    | Concept::KwNot
                    | Concept::KwIf
                    | Concept::KwFn
                    | Concept::KwUnsafe
            )
            | TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace | Op::Dot | Op::Minus)
    )
}

/// An expression that needs no parentheses before `.method()`.
fn is_postfix_level(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Name(_)
            | ExprKind::Call { .. }
            | ExprKind::Member { .. }
            | ExprKind::Index { .. }
            | ExprKind::Paren(_)
            | ExprKind::Str(_)
            | ExprKind::List(_)
            | ExprKind::Map(_)
            | ExprKind::Tuple(_)
            | ExprKind::SelfValue
            | ExprKind::ImplicitMember(_)
            | ExprKind::Try(_)
    )
}

/// An expression that may stand as a range bound without parentheses (§10.6: `..` binds
/// looser than `+` and tighter than comparisons).
fn is_range_operand(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Binary { op, .. } => matches!(
            op,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::IntDiv | BinOp::Rem
        ),
        ExprKind::Neg(_) | ExprKind::Int(_) | ExprKind::Float(_) => true,
        _ => is_postfix_level(e),
    }
}

/// Whether `[start, end)` lies inside one of `spans` (sorted by start, disjoint): a binary
/// search, so the filters below stay linear in the number of diagnostics.
fn inside_any(spans: &[Span], start: u32, end: u32) -> bool {
    let i = spans.partition_point(|s| s.start <= start);
    i > 0 && end <= spans[i - 1].end
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_name_char) && !s.starts_with(|c: char| c.is_ascii_digit())
}

/// Every name the file declares anywhere (scope-insensitive, FOUNDER-QUEUE Q9 style): the
/// word after `let`/`var`/`fn`/`for`/`type`/`enum`/`trait`/`as`, a parameter or label
/// (`(x:` / `, x:`), and every word on a `use` line.
pub(crate) fn declared_names(text: &str, toks: &[Token]) -> HashSet<String> {
    let word = |t: &Token| -> String {
        let s = (t.span.start.min(t.span.end) as usize).min(text.len());
        let e = (t.span.end.max(t.span.start) as usize).min(text.len());
        let w = text.get(s..e).unwrap_or("");
        w.trim_matches('`').to_string()
    };
    let mut out = HashSet::new();
    let mut in_use = false;
    let mut line_start = true;
    // Open brackets, innermost last: true for the parameter list of `fn(…)`.
    let mut brackets: Vec<bool> = Vec::new();
    let mut in_import = false;
    for (i, t) in toks.iter().enumerate() {
        let starts_line = line_start;
        if line_start {
            in_use = matches!(
                t.kind,
                TokenKind::Keyword(Concept::KwUse | Concept::KwExport)
            );
            in_import = t.kind == TokenKind::Keyword(Concept::KwUse);
        }
        line_start = matches!(
            t.kind,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        );
        match t.kind {
            TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => {
                let after_fn = i > 0 && toks[i - 1].kind == TokenKind::Keyword(Concept::KwFn);
                brackets.push(t.kind == TokenKind::Op(Op::LParen) && after_fn);
            }
            TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                brackets.pop();
            }
            _ => {}
        }
        let prev = i.checked_sub(1).map(|j| toks[j].kind);
        let next = toks.get(i + 1).map(|n| n.kind);
        // A word reserved in this skin, in a position where it can only be a name (a `use`
        // item, a label `(x:` / `, x:`), counts like an identifier there, so the set is the
        // same in every skin: `None: 1` is a name token in Franco, a core name in English.
        if matches!(t.kind, TokenKind::Keyword(_) | TokenKind::Core(_)) {
            let label = matches!(prev, Some(TokenKind::Op(Op::LParen | Op::Comma)))
                && next == Some(TokenKind::Op(Op::Colon));
            if (in_import && !starts_line) || label {
                out.insert(word(t));
            }
            continue;
        }
        if t.kind != TokenKind::Ident {
            continue;
        }
        let declares = in_use
            || matches!(
                prev,
                Some(TokenKind::Keyword(
                    Concept::KwLet
                        | Concept::KwVar
                        | Concept::KwFn
                        | Concept::KwFor
                        | Concept::KwType
                        | Concept::KwEnum
                        | Concept::KwTrait
                        | Concept::KwAs
                ))
            )
            || (matches!(prev, Some(TokenKind::Op(Op::LParen | Op::Comma)))
                && next == Some(TokenKind::Op(Op::Colon)));
        // A name starting a parameter of a lambda, `fn(x) => …` or `fn(a, x) => …` (§12).
        // Typed parameters (`x:`) are declarations anyway.
        let lambda_param = brackets.last() == Some(&true)
            && matches!(prev, Some(TokenKind::Op(Op::LParen | Op::Comma)));
        if declares || lambda_param {
            out.insert(word(t));
        }
    }
    out
}

/// Drops findings that only restate another one, and turns a JavaScript template literal
/// (`` `…${x}…` ``, which the lexer reports as stray backticks) into one K0913.
pub(crate) fn drop_cascades(
    text: &str,
    lexer: Vec<Diagnostic>,
    parser: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    let lines = LineIndex::new(text);
    let line = |d: &Diagnostic| lines.line_of(d.span.start.min(d.span.end).min(text.len() as u32));
    let layout_lines: HashSet<usize> = lexer
        .iter()
        .filter(|d| matches!(d.code, "K0120" | "K0122"))
        .map(line)
        .collect();
    let string_lines: HashSet<usize> = lexer
        .iter()
        .filter(|d| matches!(d.code, "K0129" | "K0131"))
        .map(line)
        .collect();
    let mut rewritten: Vec<Span> = parser
        .iter()
        .filter(|d| matches!(d.code, "K0925" | "K0926"))
        .map(|d| d.span)
        .collect();
    rewritten.sort_by_key(|s| s.start);
    let mut lexer: Vec<Diagnostic> = lexer
        .into_iter()
        .filter(|d| {
            // The `;` of a C-style `for` header or a `switch` arm: the K0926/K0925 fix
            // rewrites that whole region.
            !(d.code == "K0904" && inside_any(&rewritten, d.span.start, d.span.end))
        })
        .collect();
    let mut parser: Vec<Diagnostic> = parser
        .into_iter()
        .filter(|d| {
            let l = line(d);
            // The lexer already said what is wrong with this line's indentation.
            let layout = matches!(d.code, "K0120" | "K0252" | "K0255") && layout_lines.contains(&l);
            // An unterminated string swallowed the closing bracket.
            let swallowed = d.code == "K0251" && string_lines.contains(&l);
            !(layout || swallowed)
        })
        .collect();
    templates(text, &mut lexer, &mut parser);
    lexer.extend(parser);
    lexer
}

/// `` `…${x}…` `` (K0913): the lexer reports each backtick (and `$`) as K0105, and the
/// parser stumbles inside. One K0913 replaces them all.
fn templates(text: &str, lexer: &mut Vec<Diagnostic>, parser: &mut Vec<Diagnostic>) {
    let ticks: Vec<Span> = lexer
        .iter()
        .filter(|d| d.code == "K0105" && text.get(d.span.range()) == Some("`"))
        .map(|d| d.span)
        .collect();
    let file = match lexer.first() {
        Some(d) => d.file,
        None => return,
    };
    let mut regions = Vec::new();
    let mut i = 0;
    while i + 1 < ticks.len() {
        let (a, b) = (ticks[i], ticks[i + 1]);
        let inner = text.get(a.end as usize..b.start as usize).unwrap_or("");
        if !inner.contains('\n') && inner.contains("${") {
            regions.push((a, b, inner));
            i += 2;
        } else {
            i += 1;
        }
    }
    if regions.is_empty() {
        return;
    }
    let spans: Vec<Span> = regions
        .iter()
        .map(|(a, b, _)| mk_span(a.start, b.end))
        .collect();
    let inside = |d: &Diagnostic| {
        !d.code.starts_with("SEC") && inside_any(&spans, d.span.start, d.span.start)
    };
    lexer.retain(|d| !inside(d));
    parser.retain(|d| !inside(d));
    for (a, b, inner) in regions {
        let mut d = Diagnostic::new(codes::F_STRING, file, mk_span(a.start, b.end));
        if !inner.contains('"') {
            let mut edits = vec![edit(a, "\""), edit(b, "\"")];
            for (k, _) in inner.match_indices("${") {
                let s = a.end + k as u32;
                edits.push(edit(mk_span(s, s + 1), ""));
            }
            d = d.fix(machine("use a plain string with `{…}` holes", edits));
        }
        lexer.push(d);
    }
}

impl<'t> Parser<'t> {
    // ----- small helpers -------------------------------------------------------------------

    /// The canonical spelling of `c` in the file's skin.
    pub(crate) fn spell(&self, c: Concept) -> &'static str {
        KeywordTable::builtin().spelling(c, self.skin)
    }

    fn slice(&self, s: Span) -> &'t str {
        let a = (s.start.min(s.end) as usize).min(self.text.len());
        let b = (s.end.max(s.start) as usize).min(self.text.len());
        self.text.get(a..b).unwrap_or("")
    }

    /// The text of the `n`th token if it is a plain (not backticked) identifier.
    pub(crate) fn nth_plain(&self, n: usize) -> Option<&'t str> {
        let t = self.toks.get(self.pos + n)?;
        if t.kind != TokenKind::Ident {
            return None;
        }
        let s = self.tok_text(*t);
        (!s.starts_with('`')).then_some(s)
    }

    pub(crate) fn nth_plain_in(&self, n: usize, words: &[&str]) -> bool {
        self.nth_plain(n).is_some_and(|w| words.contains(&w))
    }

    /// A plain identifier from `words` that the file does not declare.
    pub(crate) fn at_mistake_word(&self, words: &[&str]) -> bool {
        self.nth_plain(0)
            .is_some_and(|w| words.contains(&w) && !self.declared.contains(w))
    }

    /// Two `op` tokens with nothing between them (`++`, `**`).
    pub(crate) fn at_glued_pair(&self, op: Op) -> bool {
        self.at_op(op)
            && self.nth(1) == TokenKind::Op(op)
            && match (self.toks.get(self.pos), self.toks.get(self.pos + 1)) {
                (Some(a), Some(b)) => a.span.end == b.span.start,
                _ => false,
            }
    }

    /// Whether the current token touches the previous one.
    fn glued_to_prev(&self) -> bool {
        match (
            self.pos.checked_sub(1).and_then(|i| self.toks.get(i)),
            self.toks.get(self.pos),
        ) {
            (Some(a), Some(b)) => a.span.end == b.span.start,
            _ => false,
        }
    }

    /// Reports a §21.3 mistake and returns its index in `diags`.
    pub(crate) fn mistake(&mut self, code: &'static str, span: Span, fix: Option<Fix>) -> usize {
        self.last_error_pos = Some(self.pos);
        let mut d = Diagnostic::new(code, self.file, span);
        if let Some(f) = fix {
            d = d.fix(f);
        }
        self.diags.push(d);
        self.diags.len() - 1
    }

    /// Adds edits to the first fix of diagnostic `i` (creating it if needed). A non-machine
    /// part makes the whole fix a suggestion.
    pub(crate) fn extend_fix(&mut self, i: usize, title: &str, edits: Vec<Edit>, is_machine: bool) {
        let Some(d) = self.diags.get_mut(i) else {
            return;
        };
        match d.fixes.first_mut() {
            Some(f) => {
                f.edits.extend(edits);
                if !is_machine {
                    f.applicability = Applicability::Suggestion;
                }
            }
            None => d.fixes.push(if is_machine {
                machine(title, edits)
            } else {
                suggestion(title, edits)
            }),
        }
    }

    fn set_applicability(&mut self, i: usize, a: Applicability) {
        if let Some(f) = self.diags.get_mut(i).and_then(|d| d.fixes.first_mut()) {
            f.applicability = a;
        }
    }

    fn line_start(&self, offset: u32) -> u32 {
        let o = (offset as usize).min(self.text.len());
        self.text[..o].rfind('\n').map_or(0, |i| i + 1) as u32
    }

    /// End of the line holding `offset` (the offset of its `\n`, or the text length).
    fn line_end(&self, offset: u32) -> u32 {
        let o = (offset as usize).min(self.text.len());
        self.text[o..].find('\n').map_or(self.text.len(), |i| o + i) as u32
    }

    /// The indentation (in spaces) of the line holding `offset`.
    fn line_indent(&self, offset: u32) -> usize {
        let s = self.line_start(offset) as usize;
        self.text[s..].bytes().take_while(|&b| b == b' ').count()
    }

    fn same_line(&self, a: u32, b: u32) -> bool {
        let (a, b) = (a.min(b) as usize, a.max(b) as usize);
        !self.text.get(a..b).unwrap_or("\n").contains('\n')
    }

    /// Whether the logical line from token `from` on holds a `:` or `{` outside brackets.
    pub(crate) fn line_has_colon_or_brace(&self, from: usize) -> bool {
        let mut depth = 0usize;
        let mut i = self.pos + from;
        while let Some(t) = self.toks.get(i).filter(|_| i < self.pos + SCAN_CAP) {
            match t.kind {
                k if is_layout(k) => return false,
                TokenKind::Op(Op::Colon | Op::LBrace) if depth == 0 => return true,
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1)
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// At `?`: is this the `c ? a : b` of K0910 (an expression follows, and a `:` outside
    /// brackets later on the line is followed by more code), not the `?` operator?
    ///
    /// A line holding a block keyword (`if`, `else`, `while`, `for`, `case`, `match`) is
    /// never read as a ternary: there the `:` belongs to the block or the inline `if`
    /// (`if f()?(x): g()`, `let v = if a: b?(c) else: d`).
    pub(crate) fn ternary_ahead(&self) -> bool {
        if !self.at_op(Op::Question)
            || !starts_expr(self.nth(1))
            || self.nth(1) == TokenKind::Op(Op::Dot)
        {
            return false;
        }
        let block_kw = |k: TokenKind| {
            matches!(
                k,
                TokenKind::Keyword(
                    Concept::KwIf
                        | Concept::KwElse
                        | Concept::KwWhile
                        | Concept::KwFor
                        | Concept::KwCase
                        | Concept::KwMatch
                )
            )
        };
        // Before the `?` on its line: no block keyword, and no unclosed `{` (inside a map
        // literal the `:` after `k?(…)` is the entry's).
        let mut closers = 0usize;
        let before = &self.toks[..self.pos.min(self.toks.len())];
        let line = before
            .iter()
            .rev()
            .take(SCAN_CAP)
            .take_while(|t| !is_layout(t.kind));
        if line.clone().count() == SCAN_CAP {
            return false; // a line this long holds no ternary worth rewriting
        }
        for t in line {
            match t.kind {
                k if block_kw(k) => return false,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => closers += 1,
                TokenKind::Op(Op::LBrace) if closers == 0 => return false,
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => {
                    closers = closers.saturating_sub(1)
                }
                _ => {}
            }
        }
        let mut depth = 0usize;
        let mut i = self.pos + 1;
        while let Some(t) = self.toks.get(i).filter(|_| i < self.pos + SCAN_CAP) {
            match t.kind {
                k if is_layout(k) => return false,
                k if block_kw(k) => return false,
                // A `,` or `=` ends the expression first (`f(x: g?(), y: 1)`).
                TokenKind::Op(Op::Comma | Op::Eq) if depth == 0 => return false,
                TokenKind::Op(Op::Colon) if depth == 0 => {
                    return self.toks.get(i + 1).is_some_and(|n| !is_layout(n.kind))
                }
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// At `def`/`function`/`func` followed by a name and `(` (or `[`).
    pub(crate) fn at_def(&self) -> bool {
        self.nth_plain_in(0, DEF_WORDS)
            && matches!(self.nth(1), TokenKind::Ident | TokenKind::Core(_))
            && matches!(self.nth(2), TokenKind::Op(Op::LParen | Op::LBracket))
    }

    /// Skips header tokens up to a `:` or `{` outside brackets (not consuming it) or the
    /// end of the line.
    fn skip_header(&mut self) {
        let m = self.start_node(SyntaxKind::Error);
        let mut depth = 0usize;
        loop {
            match self.peek() {
                k if is_layout(k) => break,
                TokenKind::Op(Op::Colon | Op::LBrace) if depth == 0 => break,
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1);
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
        self.finish_node(m);
    }

    /// At `{`: skips the balanced `{ … }` (all on one logical line, the lexer joins it).
    /// Returns the `{` and, when found, the matching `}`.
    fn skip_braces(&mut self) -> (Token, Option<Token>) {
        let m = self.start_node(SyntaxKind::Error);
        let open = self.bump();
        let mut depth = 0usize;
        let close = loop {
            match self.peek() {
                k if is_layout(k) => break None,
                TokenKind::Op(Op::RBrace) if depth == 0 => break Some(self.bump()),
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1);
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        };
        self.finish_node(m);
        (open, close)
    }

    /// Keeps only §21.3 diagnostics reported since `mark` (a body whose shape is already
    /// reported: what else is wrong inside shows up once the fix is applied).
    fn keep_mistakes_since(&mut self, mark: usize) {
        if mark >= self.diags.len() {
            return;
        }
        let tail = self.diags.split_off(mark);
        self.diags
            .extend(tail.into_iter().filter(|d| d.code.starts_with("K09")));
        self.pending_incr = None;
        self.block_lambda = None;
    }

    fn error_stmt(&self, start: u32) -> StmtKind {
        StmtKind::Expr(Expr {
            kind: ExprKind::Error,
            span: self.span_from(start),
        })
    }

    // ----- items ---------------------------------------------------------------------------

    /// An item written with another language's keyword (`def`, `async fn`, `class`,
    /// `const`), parsed as the Kalam item it stands for. `None` if there is none here.
    pub(crate) fn item_mistake(&mut self) -> Option<ItemKind> {
        if self.at_mistake_word(&["async"])
            && (self.nth(1) == TokenKind::Keyword(Concept::KwFn) || self.nth_plain_in(1, DEF_WORDS))
        {
            self.async_word();
            return Some(if self.at_kw(Concept::KwFn) {
                ItemKind::Fn(self.fn_decl(false))
            } else {
                ItemKind::Fn(self.def_decl())
            });
        }
        if self.at_def() {
            return Some(ItemKind::Fn(self.def_decl()));
        }
        if self.nth_plain_in(0, CLASS_WORDS)
            && matches!(self.nth(1), TokenKind::Ident | TokenKind::Core(_))
        {
            return Some(self.class_decl());
        }
        if self.at_mistake_word(&["const"])
            && matches!(self.nth(1), TokenKind::Ident | TokenKind::Core(_))
            && matches!(self.nth(2), TokenKind::Op(Op::Eq | Op::Colon))
        {
            let t = self.cur();
            let kw = self.spell(Concept::KwLet);
            self.mistake(
                codes::CONST_OR_LET_MUT,
                t.span,
                Some(Fix::machine(format!("write `{kw}`"), t.span, kw)),
            );
            return Some(self.const_decl());
        }
        None
    }

    /// `async` before `fn`/`def` (K0907): reported with a fix that deletes it, then skipped.
    fn async_word(&mut self) {
        let t = self.cur();
        let next = self
            .toks
            .get(self.pos + 1)
            .map_or(t.span.end, |n| n.span.start);
        let span = mk_span(t.span.start, next);
        self.mistake(
            codes::ASYNC_AWAIT,
            t.span,
            Some(Fix::machine("delete `async`", span, "")),
        );
        self.bump();
    }

    /// `def f(x):`, `function f(x) {`, `func f(x)` (K0902), parsed as `fn`. Parameters
    /// without a type are accepted; the fix gives each a `_` type hole.
    pub(crate) fn def_decl(&mut self) -> FnDecl {
        let word = self.cur();
        let idx = self.mistake(codes::DEF_KEYWORD, word.span, None);
        let start = self.start();
        self.untyped = Some(Vec::new());
        let sig = self.fn_sig();
        let untyped = self.untyped.take().unwrap_or_default();
        let mut edits = vec![edit(word.span, self.spell(Concept::KwFn))];
        edits.extend(untyped.into_iter().map(|e| edit(at(e), ": _")));
        let kw = self.spell(Concept::KwFn);
        self.extend_fix(
            idx,
            &format!("write `{kw}` and leave type holes"),
            edits,
            true,
        );
        let body = self.fn_body(false, Some(idx));
        FnDecl {
            sig,
            body,
            span: self.span_from(start),
        }
    }

    /// `class X:`, `struct X {`, `record X:` (→ `type`) and `interface X` (→ `trait`), K0920.
    fn class_decl(&mut self) -> ItemKind {
        let word = self.cur();
        let interface = self.tok_text(word) == "interface";
        let kw = self.spell(if interface {
            Concept::KwTrait
        } else {
            Concept::KwType
        });
        let idx = self.mistake(
            codes::CLASS,
            word.span,
            Some(Fix::machine(format!("write `{kw}`"), word.span, kw)),
        );
        self.bump();
        let name = self.decl_name("a type name");
        let generics = self.generics();
        // Base classes (`(Base)`, `extends`, `implements`) have no Kalam form.
        if self.at_op(Op::LParen) || self.nth_plain_in(0, &["extends", "implements"]) {
            let from = self.last_end;
            self.skip_header();
            let to = self.last_end;
            self.extend_fix(idx, "", vec![edit(mk_span(from, to), "")], false);
        }
        if self.at_op(Op::LBrace) {
            self.brace_record(Some(idx), false);
            return if interface {
                ItemKind::Trait(TraitDecl {
                    name,
                    generics,
                    members: Vec::new(),
                })
            } else {
                ItemKind::Type(TypeDecl {
                    name,
                    generics,
                    body: TypeBody::Record(Vec::new()),
                })
            };
        }
        let open = self.open_body();
        let mark = self.diags.len();
        let kind = if interface {
            let members = if open {
                self.body_members(|p| {
                    if p.at_kw(Concept::KwFn) {
                        Some(p.fn_decl(true))
                    } else {
                        p.skip_line();
                        None
                    }
                })
            } else {
                Vec::new()
            };
            ItemKind::Trait(TraitDecl {
                name,
                generics,
                members,
            })
        } else {
            let fields = if open {
                self.body_members(|p| {
                    if p.at_word() && p.nth(1) == TokenKind::Op(Op::Colon) {
                        p.field_decl()
                    } else if p.at_kw(Concept::KwFn) {
                        p.fn_decl(false);
                        None
                    } else if p.at_def() {
                        p.def_decl();
                        None
                    } else {
                        p.skip_line();
                        None
                    }
                })
            } else {
                Vec::new()
            };
            ItemKind::Type(TypeDecl {
                name,
                generics,
                body: TypeBody::Record(fields),
            })
        };
        // Methods (in Kalam they go in an `impl`) and Python-style members inside a class
        // are part of the class's shape: the K0920 already reported is the finding.
        self.diags.truncate(mark);
        self.pending_incr = None;
        self.block_lambda = None;
        kind
    }

    /// At `{` after a type, struct or enum header (K0905, or the caller's K0920): skips
    /// the braces. The fix rewrites `{ a: Int, b: Int }` as an indented member list.
    pub(crate) fn brace_record(&mut self, owner: Option<usize>, enum_like: bool) {
        let header_end = self.last_end;
        let header_start = self.line_start(header_end);
        let (open, close) = self.skip_braces();
        let Some(close) = close else {
            let idx = owner.unwrap_or_else(|| self.mistake(codes::BRACE_BLOCK, open.span, None));
            self.extend_fix(idx, "", Vec::new(), false);
            return;
        };
        let indent = " ".repeat(self.line_indent(header_start) + 4);
        let content = self.slice(mk_span(open.span.end, close.span.start));
        let mut members = Vec::new();
        let mut ok = !content.contains(['"', '#']);
        for piece in content.split([',', ';', '\n', '\u{060C}', '\u{061B}']) {
            let piece = piece.trim();
            let piece = piece.strip_prefix("pub ").unwrap_or(piece).trim();
            if piece.is_empty() {
                continue;
            }
            let field = piece
                .split_once(':')
                .is_some_and(|(n, t)| is_name(n.trim()) && !t.trim().is_empty());
            let variant = enum_like
                && is_name(piece.split('(').next().unwrap_or("").trim())
                && (!piece.contains('(') || piece.ends_with(')'));
            ok &= field || variant;
            members.push(piece.to_string());
        }
        ok &= !members.is_empty();
        let mut text = String::from(":");
        for m in &members {
            text.push('\n');
            text.push_str(&indent);
            text.push_str(m);
        }
        let edits = vec![edit(mk_span(header_end, close.span.end), text)];
        match owner {
            Some(i) => self.extend_fix(i, "", edits, ok),
            None => {
                let f = if ok {
                    machine("use `:` and an indented member list", edits)
                } else {
                    suggestion("use `:` and an indented member list", edits)
                };
                self.mistake(codes::BRACE_BLOCK, open.span, Some(f));
            }
        }
    }

    /// At `{` after an `impl` or `trait` header (K0905): skips the braces; the fix turns
    /// them into `:` and indentation when the body is already indented.
    pub(crate) fn brace_members(&mut self) {
        let header_end = self.last_end;
        let (open, close) = self.skip_braces();
        let (edits, ok) = self.brace_edits(header_end, open, close, false);
        let f = if ok {
            machine("use `:` and indentation", edits)
        } else {
            suggestion("use `:` and indentation", edits)
        };
        self.mistake(codes::BRACE_BLOCK, open.span, Some(f));
    }

    // ----- blocks --------------------------------------------------------------------------

    /// At `{` where a block goes (K0905, or added to `owner`): parses the statements inside
    /// (the lexer joined their lines) and returns the block and the diagnostic that owns it.
    pub(crate) fn brace_block(&mut self, owner: Option<usize>) -> (Block, Option<usize>) {
        let m = self.start_node(SyntaxKind::Block);
        let start = self.start();
        let header_end = self.last_end;
        let open = self.bump();
        // Report first so the K0905 comes before anything found inside.
        let idx = match owner {
            Some(i) => i,
            None => self.mistake(codes::BRACE_BLOCK, open.span, None),
        };
        let mark = self.diags.len();
        self.brace_depth += 1;
        let mut stmts = Vec::new();
        let close = loop {
            match self.peek() {
                TokenKind::Op(Op::RBrace) => break Some(self.bump()),
                k if is_layout(k) => break None,
                _ => {}
            }
            let before = self.pos;
            if !self.enter() {
                self.leave();
                break None;
            }
            stmts.push(self.stmt());
            self.leave();
            if self.pos == before {
                let e = self.start_node(SyntaxKind::Error);
                self.bump();
                self.finish_node(e);
            }
        };
        self.brace_depth -= 1;
        self.keep_mistakes_since(mark);
        let (edits, ok) = self.brace_edits(header_end, open, close, stmts.len() <= 1);
        self.extend_fix(idx, "use `:` and indentation", edits, ok);
        let inline = close.is_some_and(|c| self.same_line(open.span.start, c.span.start));
        self.finish_node(m);
        (
            Block {
                stmts,
                inline,
                span: self.span_from(start),
            },
            Some(idx),
        )
    }

    /// Edits turning `header {` … `}` into `header:` plus indentation. Machine-applicable
    /// for `{ one statement }` on one line, or a multi-line body whose lines are indented
    /// deeper than the header and whose `}` starts its line.
    fn brace_edits(
        &self,
        header_end: u32,
        open: Token,
        close: Option<Token>,
        single: bool,
    ) -> (Vec<Edit>, bool) {
        let Some(close) = close else {
            return (vec![edit(mk_span(header_end, open.span.end), ":")], false);
        };
        let (o, c) = (open.span.end, close.span.start);
        if self.same_line(open.span.start, close.span.start) {
            let inner = self.slice(mk_span(o, c));
            let lead = (inner.len() - inner.trim_start().len()) as u32;
            let trail = (inner.len() - inner.trim_end().len()) as u32;
            if inner.trim().is_empty() {
                let pass = format!(": {}", self.spell(Concept::KwPass));
                return (vec![edit(mk_span(header_end, close.span.end), pass)], true);
            }
            return (
                vec![
                    edit(mk_span(header_end, o + lead), ": "),
                    edit(mk_span(c - trail, close.span.end), ""),
                ],
                single,
            );
        }
        let mut ok = self.slice(mk_span(o, self.line_end(o))).trim().is_empty();
        let header_indent = self.line_indent(open.span.start);
        let body = self.slice(mk_span(self.line_end(o), self.line_start(c)));
        for line in body.split('\n').filter(|l| !l.trim().is_empty()) {
            ok &= line.bytes().take_while(|&b| b == b' ').count() > header_indent;
        }
        let mut edits = vec![edit(mk_span(header_end, o), ":")];
        let close_line = self.line_start(c);
        let before = self.slice(mk_span(close_line, c));
        let after_end = self.line_end(close.span.end);
        let after = self.slice(mk_span(close.span.end, after_end));
        if !before.trim().is_empty() {
            let code_end = close_line + before.trim_end().len() as u32;
            edits.push(edit(mk_span(code_end, close.span.end), ""));
        } else if after.trim().is_empty() {
            let end = (after_end as usize + 1).min(self.text.len()) as u32;
            edits.push(edit(mk_span(close_line, end), ""));
        } else {
            let lead = (after.len() - after.trim_start().len()) as u32;
            edits.push(edit(mk_span(c, close.span.end + lead), ""));
        }
        (edits, ok)
    }

    // ----- statements ----------------------------------------------------------------------

    /// A statement written with another language's construct (`try:`, `catch`, `switch`,
    /// `async fn`, `def`), parsed and reported as one finding. `None` if there is none here.
    pub(crate) fn stmt_mistake(&mut self) -> Option<StmtKind> {
        let start = self.start();
        if self.at_mistake_word(&["try"])
            && matches!(self.nth(1), TokenKind::Op(Op::Colon | Op::LBrace))
        {
            return Some(self.try_stmt());
        }
        if self.at_mistake_word(HANDLER_WORDS) && self.line_has_colon_or_brace(1) {
            let t = self.cur();
            self.mistake(codes::TRY_CATCH, t.span, None);
            self.bump();
            self.skip_line();
            return Some(self.error_stmt(start));
        }
        if self.at_mistake_word(&["switch"]) && self.line_has_colon_or_brace(1) {
            return Some(self.switch_stmt());
        }
        if self.at_mistake_word(&["async"])
            && self.nth(1) == TokenKind::Keyword(Concept::KwFn)
            && self.nth(2) != TokenKind::Op(Op::LParen)
        {
            self.async_word();
            return Some(StmtKind::Fn(self.fn_decl(false)));
        }
        if self.at_def() {
            return Some(StmtKind::Fn(self.def_decl()));
        }
        None
    }

    /// `try:` / `try {` with optional `catch`/`except`/`finally` clauses: one K0908.
    fn try_stmt(&mut self) -> StmtKind {
        let start = self.start();
        let t = self.cur();
        let braces = self.nth(1) == TokenKind::Op(Op::LBrace);
        let idx = self.mistake(codes::TRY_CATCH, t.span, None);
        self.bump();
        let (body, _) = self.block_owned(Some(idx));
        let mut handlers = false;
        loop {
            if self.peek() == TokenKind::Newline && self.nth_plain_in(1, HANDLER_WORDS) && braces {
                self.bump();
            }
            if !self.nth_plain_in(0, HANDLER_WORDS) {
                break;
            }
            handlers = true;
            self.bump();
            self.skip_header();
            if matches!(self.peek(), TokenKind::Op(Op::Colon | Op::LBrace)) {
                let _ = self.block_owned(Some(idx));
            } else {
                break;
            }
        }
        if let Some(d) = self.diags.get_mut(idx) {
            d.fixes.clear();
        }
        if !handlers && !braces && !body.inline && !body.stmts.is_empty() {
            // Unwrap the block: errors are values; `?` passes them up (a suggestion, since
            // calls that can fail still need their `?`).
            let header = self.line_start(t.span.start);
            let header_end = (self.line_end(t.span.start) as usize + 1).min(self.text.len());
            let mut edits = vec![edit(mk_span(header, header_end as u32), "")];
            let unit = self
                .line_indent(header_end as u32)
                .saturating_sub(self.line_indent(t.span.start));
            let end = body.span.end as usize;
            let mut line = header_end;
            while line < end {
                let spaces = self.text[line..].bytes().take_while(|&b| b == b' ').count();
                if spaces >= unit && unit > 0 {
                    edits.push(edit(mk_span(line as u32, (line + unit) as u32), ""));
                }
                match self.text[line..].find('\n') {
                    Some(n) => line += n + 1,
                    None => break,
                }
            }
            if let Some(d) = self.diags.get_mut(idx) {
                d.fixes.push(suggestion(
                    "remove `try:` and put `?` after calls that can fail",
                    edits,
                ));
            }
        }
        self.error_stmt(start)
    }

    /// `switch x { case 0 -> … }` or `switch x:` (K0925), rewritten as `match`.
    fn switch_stmt(&mut self) -> StmtKind {
        let start = self.start();
        let sw = self.cur();
        let idx = self.mistake(codes::SWITCH, sw.span, None);
        self.bump();
        let scrut = self.or_expr();
        let kw = self.spell(Concept::KwMatch);
        if self.at_op(Op::LBrace) {
            let (open, close) = self.skip_braces();
            match close {
                Some(close) => {
                    if let Some(d) = self.diags.get_mut(idx) {
                        d.span = mk_span(sw.span.start, close.span.end);
                    }
                    match self.arms_rewrite(&scrut, open, close, sw.span.start) {
                        Some(text) => self.extend_fix(
                            idx,
                            &format!("write `{kw}` with `case` arms"),
                            vec![edit(mk_span(sw.span.start, close.span.end), text)],
                            true,
                        ),
                        None => self.extend_fix(
                            idx,
                            &format!("write `{kw}` with `case` arms"),
                            vec![edit(sw.span, kw)],
                            false,
                        ),
                    }
                }
                None => self.extend_fix(idx, "", vec![edit(sw.span, kw)], false),
            }
        } else if self.at_op(Op::Colon) {
            self.extend_fix(idx, &format!("write `{kw}`"), vec![edit(sw.span, kw)], true);
            if self.open_body() {
                let mark = self.diags.len();
                let case = self.spell(Concept::KwCase);
                self.body_members(|p| {
                    if p.at_kw(Concept::KwCase) {
                        Some(p.case_arm(true, &mut Some(idx)))
                    } else if p.nth_plain_in(0, &["default"])
                        && p.nth(1) == TokenKind::Op(Op::Colon)
                    {
                        let d = p.bump();
                        p.extend_fix(idx, "", vec![edit(d.span, format!("{case} _"))], true);
                        let _ = p.block();
                        None
                    } else {
                        p.skip_line();
                        None
                    }
                });
                self.keep_mistakes_since(mark);
            }
        } else {
            self.expected("`{` or `:`");
            self.skip_line();
        }
        self.error_stmt(start)
    }

    /// `match x:` plus one `case P: S` line per arm, from the source lines of a braced
    /// `switch`/`match` body. `None` if an arm is not one line of `case P -> S`,
    /// `case P: S`, `P => S`, `default: S` or `_ => S`.
    fn arms_rewrite(&self, scrut: &Expr, open: Token, close: Token, header: u32) -> Option<String> {
        let indent = " ".repeat(self.line_indent(header) + 4);
        let case = self.spell(Concept::KwCase);
        let scrut_text = match &scrut.kind {
            ExprKind::Paren(inner) => self.slice(inner.span),
            _ => self.slice(scrut.span),
        };
        let mut out = format!("{} {}:", self.spell(Concept::KwMatch), scrut_text.trim());
        let body = self.slice(mk_span(open.span.end, close.span.start));
        let table = KeywordTable::builtin().skin(self.skin);
        let mut arms = 0;
        for line in body.split('\n') {
            let l = line
                .trim()
                .trim_end_matches([',', ';', '\u{060C}', '\u{061B}'])
                .trim();
            if l.is_empty() || l == "break" {
                continue;
            }
            let first = l.split_whitespace().next().unwrap_or("");
            let is_case = first == "case"
                || table
                    .lookup_word(first)
                    .is_some_and(|w| w.concept == Concept::KwCase);
            let (rest, colon_ok, default) = if is_case {
                (l[first.len()..].trim_start(), true, false)
            } else if let Some(r) = l.strip_prefix("default") {
                (r.trim_start(), true, true)
            } else {
                (l, false, false)
            };
            let sep = ["->", "=>"]
                .iter()
                .filter_map(|s| rest.find(s).map(|i| (i, 2)))
                .min()
                .or_else(|| colon_ok.then(|| rest.find(':').map(|i| (i, 1))).flatten())?;
            let pat = if default { "_" } else { rest[..sep.0].trim() };
            let stmt = rest[sep.0 + sep.1..].trim();
            let pat = if pat == "default" { "_" } else { pat };
            if pat.is_empty() || stmt.is_empty() || stmt.starts_with('{') || stmt.contains(';') {
                return None;
            }
            out.push('\n');
            out.push_str(&indent);
            out.push_str(&format!("{case} {pat}: {stmt}"));
            arms += 1;
        }
        (arms > 0).then_some(out)
    }

    /// `let mut x` (K0912), at `mut` after `let`: reported with a fix to `var`.
    pub(crate) fn at_let_mut(&self) -> bool {
        self.nth_plain_in(0, &["mut"]) && matches!(self.nth(1), TokenKind::Ident)
    }

    pub(crate) fn let_mut(&mut self, let_tok: Token) {
        let mut_tok = self.cur();
        let kw = self.spell(Concept::KwVar);
        let span = mk_span(let_tok.span.start, mut_tok.span.end);
        self.mistake(
            codes::CONST_OR_LET_MUT,
            span,
            Some(Fix::machine(format!("write `{kw}`"), span, kw)),
        );
        self.bump();
    }

    /// `let mut X = …` at module level (K0912), at `mut`: there is no module-level `var`
    /// (no globals), so the suggested fix drops `mut`.
    pub(crate) fn module_let_mut(&mut self, let_tok: Token) {
        let m = self.cur();
        let next = self
            .toks
            .get(self.pos + 1)
            .map_or(m.span.end, |n| n.span.start);
        let idx = self.mistake(
            codes::CONST_OR_LET_MUT,
            mk_span(let_tok.span.start, m.span.end),
            None,
        );
        self.extend_fix(
            idx,
            "drop `mut`",
            vec![edit(mk_span(m.span.start, next), "")],
            false,
        );
        self.bump();
    }

    /// `const x = …` in a block (K0912), at `const`.
    pub(crate) fn at_const_stmt(&self) -> bool {
        self.at_mistake_word(&["const"])
            && self.nth(1) == TokenKind::Ident
            && matches!(self.nth(2), TokenKind::Op(Op::Eq | Op::Colon))
    }

    pub(crate) fn const_word(&mut self) {
        let t = self.cur();
        let kw = self.spell(Concept::KwLet);
        self.mistake(
            codes::CONST_OR_LET_MUT,
            t.span,
            Some(Fix::machine(format!("write `{kw}`"), t.span, kw)),
        );
    }

    /// `throw e` / `raise e` (K0908) at statement start, parsed as `return Err(e)`.
    pub(crate) fn at_throw(&self) -> bool {
        self.at_mistake_word(&["throw", "raise"]) && starts_expr(self.nth(1))
    }

    pub(crate) fn throw_stmt(&mut self) -> (StmtKind, bool) {
        let t = self.bump();
        let idx = self.mistake(codes::TRY_CATCH, t.span, None);
        let e = self.expr();
        let ret = format!(
            "{} {}(",
            self.spell(Concept::KwReturn),
            self.spell(Concept::CoreErr)
        );
        let edits = vec![
            edit(mk_span(t.span.start, e.span.start), ret),
            edit(at(e.span.end), ")"),
        ];
        self.extend_fix(idx, "return the error as a value", edits, true);
        (StmtKind::Return(Some(e)), true)
    }

    /// `++x` / `--x` as a statement (K0909).
    pub(crate) fn at_prefix_incr(&self) -> bool {
        (self.at_glued_pair(Op::Plus) || self.at_glued_pair(Op::Minus))
            && self.nth(2) == TokenKind::Ident
    }

    pub(crate) fn prefix_incr(&mut self) -> (StmtKind, bool) {
        let plus = self.at_op(Op::Plus);
        let a = self.bump();
        let b = self.bump();
        let span = mk_span(a.span.start, b.span.end);
        let idx = self.mistake(codes::INCREMENT, span, None);
        let e = self.expr();
        let ins = if plus { " += 1" } else { " -= 1" };
        let ok = is_place(&e) && self.at_line_end();
        self.extend_fix(
            idx,
            &format!("write `x{}`", ins),
            vec![edit(span, ""), edit(at(e.span.end), ins)],
            ok,
        );
        (StmtKind::Expr(e), true)
    }

    /// After a statement expression: a postfix `x++` that is the whole statement gets a
    /// machine fix (`x += 1`); elsewhere its fix stays a suggestion.
    pub(crate) fn finish_postfix_incr(&mut self, e: &Expr) {
        if let Some((idx, place)) = self.pending_incr.take() {
            let whole = self
                .diags
                .get(idx)
                .is_some_and(|d| d.span.end == e.span.end);
            if place && whole && self.at_line_end() {
                self.set_applicability(idx, Applicability::Machine);
            }
        }
    }

    /// `for (init; cond; step)` and JavaScript `for (x of xs)` (K0926), at `(` after `for`.
    pub(crate) fn at_c_for(&self) -> bool {
        if !self.at_op(Op::LParen) {
            return false;
        }
        let mut depth = 0usize;
        let mut i = self.pos;
        while let Some(t) = self.toks.get(i).filter(|_| i < self.pos + SCAN_CAP) {
            match t.kind {
                k if is_layout(k) => return false,
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth -= 1;
                    if depth == 0 {
                        return self.toks.get(i + 1).map(|n| n.kind)
                            != Some(TokenKind::Keyword(Concept::KwIn));
                    }
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Parses `( … )` after `for` and its block; returns the loop statement.
    pub(crate) fn c_for(&mut self, for_tok: Token) -> StmtKind {
        let first = self.pos + 1;
        let (open, close) = {
            let m = self.start_node(SyntaxKind::Error);
            let open = self.bump();
            let mut depth = 0usize;
            let close = loop {
                match self.peek() {
                    k if is_layout(k) => break None,
                    TokenKind::Op(Op::RParen) if depth == 0 => break Some(self.bump()),
                    TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => {
                        depth += 1;
                        self.bump();
                    }
                    TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                        depth = depth.saturating_sub(1);
                        self.bump();
                    }
                    _ => {
                        self.bump();
                    }
                }
            };
            self.finish_node(m);
            (open, close)
        };
        let end = close.map_or(self.last_end, |c| c.span.end);
        let span = mk_span(for_tok.span.start, end);
        let idx = self.mistake(codes::C_FOR, span, None);
        let mut name = None;
        if let Some(close) = close {
            if let Some((n, rewrite)) = self.c_for_rewrite(first, open, close) {
                self.extend_fix(
                    idx,
                    "loop over a range",
                    vec![edit(mk_span(for_tok.span.end, close.span.end), rewrite)],
                    true,
                );
                name = Some(n);
            }
        }
        let (body, _) = self.block_owned(Some(idx));
        let pat = Pat {
            kind: match name {
                Some(n) => PatKind::Bind(Ident {
                    name: n,
                    span: open.span,
                }),
                None => PatKind::Error,
            },
            span: open.span,
        };
        StmtKind::For {
            pat,
            iter: Expr {
                kind: ExprKind::Error,
                span,
            },
            body,
        }
    }

    /// ` i in a..b` for `(var i = a; i < b; i += 1)`, or ` x in xs` for `(let x of xs)`.
    fn c_for_rewrite(&self, first: usize, open: Token, close: Token) -> Option<(String, String)> {
        let in_kw = self.spell(Concept::KwIn);
        let inner = self.slice(mk_span(open.span.end, close.span.start));
        // Skip a declaration word (`var`, `let`, `const`, `int`, …).
        let mut i = first;
        let decl = |t: &Token| -> bool {
            matches!(t.kind, TokenKind::Keyword(Concept::KwVar | Concept::KwLet))
                || (t.kind == TokenKind::Ident
                    && ["const", "let", "var", "int", "auto", "long"].contains(&self.tok_text(*t)))
        };
        if self.toks.get(i).is_some_and(decl)
            && self
                .toks
                .get(i + 1)
                .is_some_and(|t| t.kind == TokenKind::Ident)
        {
            i += 1;
        }
        let name_tok = *self.toks.get(i)?;
        if name_tok.kind != TokenKind::Ident {
            return None;
        }
        let name = self.tok_text(name_tok).to_string();
        if !is_name(&name) {
            return None;
        }
        let after_name = self.slice(mk_span(name_tok.span.end, close.span.start));
        if !inner.contains([';', '\u{061B}']) {
            // `(x of xs)` / `(x in xs)`
            let next = self.toks.get(i + 1)?;
            let is_of = next.kind == TokenKind::Keyword(Concept::KwIn)
                || (next.kind == TokenKind::Ident && self.tok_text(*next) == "of");
            if !is_of {
                return None;
            }
            let iter = self.slice(mk_span(next.span.end, close.span.start)).trim();
            if iter.is_empty() {
                return None;
            }
            return Some((name.clone(), format!(" {name} {in_kw} {iter}")));
        }
        let parts: Vec<&str> = after_name.split([';', '\u{061B}']).collect();
        let [init, cond, step] = parts.as_slice() else {
            return None;
        };
        let lo = init.trim().strip_prefix('=')?.trim();
        let cond = cond.trim().strip_prefix(name.as_str())?.trim_start();
        let (inclusive, hi) = if let Some(h) = cond.strip_prefix("<=") {
            (true, h.trim())
        } else {
            (false, cond.strip_prefix('<')?.trim())
        };
        let step: String = step.chars().filter(|c| !c.is_whitespace()).collect();
        let steps = [
            format!("{name}++"),
            format!("++{name}"),
            format!("{name}+=1"),
            format!("{name}={name}+1"),
        ];
        if lo.is_empty() || hi.is_empty() || !steps.contains(&step) {
            return None;
        }
        let range = if inclusive { "..=" } else { ".." };
        Some((name.clone(), format!(" {name} {in_kw} {lo}{range}{hi}")))
    }

    /// `for i in range(n)` (K0926): the iterable is a call to an undeclared `range`.
    pub(crate) fn check_range_call(&mut self, iter: &Expr) {
        let ExprKind::Call { callee, args } = &iter.kind else {
            return;
        };
        if self.slice(callee.span) != "range"
            || self.declared.contains("range")
            || args.is_empty()
            || args.len() > 3
            || args.iter().any(|a| a.label.is_some())
        {
            return;
        }
        let bound = |e: &Expr| -> String {
            let s = self.slice(e.span);
            if is_range_operand(e) {
                s.to_string()
            } else {
                format!("({s})")
            }
        };
        let fix = match args.as_slice() {
            [n] => Some(format!("0..{}", bound(&n.value))),
            [a, b] => Some(format!("{}..{}", bound(&a.value), bound(&b.value))),
            _ => None,
        };
        let fix = fix.map(|t| Fix::machine("loop over a range", iter.span, t));
        let d = Diagnostic::new(codes::C_FOR, self.file, iter.span);
        self.diags.push(match fix {
            Some(f) => d.fix(f),
            None => d,
        });
    }

    // ----- if chains -----------------------------------------------------------------------

    /// At `elif` (K0903) where an `else if` branch can go: a plain `elif` whose line holds
    /// a `:` or `{` (so `elif = 1` and `elif(x)` stay ordinary code).
    pub(crate) fn at_elif(&self) -> bool {
        self.nth_plain_in(0, &["elif"]) && self.line_has_colon_or_brace(1)
    }

    pub(crate) fn elif_word(&mut self) {
        let t = self.cur();
        let text = format!(
            "{} {}",
            self.spell(Concept::KwElse),
            self.spell(Concept::KwIf)
        );
        self.mistake(
            codes::ELIF,
            t.span,
            Some(Fix::machine(format!("write `{text}`"), t.span, text)),
        );
        self.bump();
    }

    /// `else if (c) {` (K0903) when the chain has no brace owner yet: reported at `else`,
    /// with edits dropping the condition's parentheses (the brace edits follow).
    pub(crate) fn else_if_braces(&mut self, else_span: Span, cond: &Expr) -> usize {
        let mut edits = Vec::new();
        if let ExprKind::Paren(inner) = &cond.kind {
            edits.push(edit(mk_span(cond.span.start, inner.span.start), ""));
            edits.push(edit(mk_span(inner.span.end, cond.span.end), ""));
        }
        let idx = self.mistake(codes::ELIF, else_span, None);
        self.extend_fix(idx, "write `else if c:`", edits, true);
        idx
    }

    // ----- expressions ---------------------------------------------------------------------

    /// After `==`/`!=` was consumed: a glued `=` makes it `===`/`!==` (K0901).
    pub(crate) fn triple_equals(&mut self, op_tok: Token) {
        if !(self.at_op(Op::Eq) && self.glued_to_prev()) {
            return;
        }
        let eq = self.bump();
        let span = mk_span(op_tok.span.start, eq.span.end);
        let text = if op_tok.kind == TokenKind::Op(Op::EqEq) {
            "=="
        } else {
            "!="
        };
        self.mistake(
            codes::TRIPLE_EQUALS,
            span,
            Some(Fix::machine(format!("write `{text}`"), span, text)),
        );
    }

    /// `c ? a : b` (K0910), at `?` after the condition: parsed as `if c: a else: b`.
    pub(crate) fn ternary(&mut self, cond: Expr) -> Expr {
        let start = cond.span.start;
        self.bump(); // ?
        let then = self.expr();
        self.expect_op(Op::Colon, "`:`");
        let other = self.expr();
        let span = self.span_from(start);
        let edits = vec![
            edit(
                at(cond.span.start),
                format!("{} ", self.spell(Concept::KwIf)),
            ),
            edit(mk_span(cond.span.end, then.span.start), ": "),
            edit(
                mk_span(then.span.end, other.span.start),
                format!(" {}: ", self.spell(Concept::KwElse)),
            ),
        ];
        let d = Diagnostic::new(codes::TERNARY, self.file, span)
            .fix(machine("write an inline `if`", edits));
        self.diags.push(d);
        let then_block = self.inline_arm(then);
        let else_block = self.inline_arm(other);
        Expr {
            kind: ExprKind::If(IfExpr {
                branches: vec![IfBranch {
                    cond,
                    body: then_block,
                }],
                else_body: Some(else_block),
                inline: true,
            }),
            span,
        }
    }

    /// In a postfix chain, at a glued `++`/`--` not followed by an operand (K0909).
    pub(crate) fn at_postfix_incr(&self) -> bool {
        (self.at_glued_pair(Op::Plus) || self.at_glued_pair(Op::Minus)) && !starts_expr(self.nth(2))
    }

    pub(crate) fn postfix_incr(&mut self, e: &Expr) {
        let plus = self.at_op(Op::Plus);
        let a = self.bump();
        let b = self.bump();
        let span = mk_span(a.span.start, b.span.end);
        let ins = if plus { " += 1" } else { " -= 1" };
        let idx = self.mistake(
            codes::INCREMENT,
            span,
            Some(Fix::suggestion(format!("write `x{ins}`"), span, ins)),
        );
        self.pending_incr = Some((idx, is_place(e)));
    }

    /// In a postfix chain, at a glued `**` (K0923): `a ** b` is parsed as `a.pow(b)`.
    pub(crate) fn power(&mut self, base: Expr) -> ExprKind {
        let a = self.bump();
        let b = self.bump();
        let star = mk_span(a.span.start, b.span.end);
        let idx = self.mistake(codes::POWER, star, None);
        let rhs = if self.enter() {
            self.unary()
        } else {
            self.error_expr()
        };
        self.leave();
        let literal = matches!(base.kind, ExprKind::Int(_) | ExprKind::Float(_));
        let mut edits = Vec::new();
        if literal {
            edits.push(edit(at(base.span.start), "("));
        }
        edits.push(edit(
            mk_span(base.span.end, rhs.span.start),
            if literal { ").pow(" } else { ".pow(" },
        ));
        edits.push(edit(at(rhs.span.end), ")"));
        self.extend_fix(idx, "call `.pow`", edits, true);
        let rhs_span = rhs.span;
        ExprKind::Call {
            callee: Box::new(Expr {
                span: base.span,
                kind: ExprKind::Member {
                    base: Box::new(base),
                    member: Member::Name(Ident {
                        name: "pow".into(),
                        span: star,
                    }),
                },
            }),
            args: vec![Arg {
                label: None,
                value: rhs,
                span: rhs_span,
            }],
        }
    }

    /// Checks a call just parsed: `len(xs)`/`str(x)`/`int(s)` (K0918) and `x.unwrap()`
    /// (K0915). `print(a, b)` (K0916) and `f[Int](x)` (K0922) are well-formed calls the
    /// grammar accepts (a print arity and an index by a type name): they are the type
    /// checker's to report, not the parser's.
    pub(crate) fn check_call(&mut self, callee: &Expr, args: &[Arg], end: u32) {
        let call = mk_span(callee.span.start, end);
        match &callee.kind {
            ExprKind::Name(NameKind::Ident(n))
                if FREE_FUNCTIONS.contains(&n.as_str())
                    && self.slice(callee.span) == n
                    && !self.declared.contains(n.as_str())
                    && args.len() == 1
                    && args[0].label.is_none() =>
            {
                let arg = &args[0].value;
                let src = self.slice(arg.span);
                let recv = if is_postfix_level(arg) && !matches!(arg.kind, ExprKind::Try(_)) {
                    src.to_string()
                } else {
                    format!("({src})")
                };
                let (text, ok) = match n.as_str() {
                    "len" => (format!("{recv}.len()"), true),
                    "str" => (
                        format!("\"{{{src}}}\""),
                        !src.contains(['"', '{', '}', '\n']),
                    ),
                    _ => (format!("{recv}.to_int()?"), true),
                };
                let f = if ok {
                    Fix::machine("use the method", call, text)
                } else {
                    Fix::suggestion("use the method", call, text)
                };
                let d = Diagnostic::new(codes::FREE_FUNCTION, self.file, call).fix(f);
                self.diags.push(d);
            }
            ExprKind::Member {
                member: Member::Name(m),
                ..
            } if m.name == "unwrap" && self.slice(m.span) == "unwrap" && args.is_empty() => {
                let span = mk_span(m.span.start, end);
                let d = Diagnostic::new(codes::UNWRAP, self.file, span).fix(Fix::machine(
                    "state the invariant with `expect`",
                    span,
                    "expect(\"…\")",
                ));
                self.diags.push(d);
            }
            _ => {}
        }
    }

    /// Bare-word mistakes where an expression starts: `null`/`nil`/`undefined` (K0906),
    /// `await x` and `async fn(…)` (K0907), `lambda x: …` (K0921). `None` if none.
    pub(crate) fn primary_mistake(&mut self) -> Option<Expr> {
        let start = self.start();
        if self.at_mistake_word(NULL_WORDS) {
            let t = self.bump();
            let none = self.spell(Concept::CoreNone);
            self.mistake(
                codes::NULL_WORD,
                t.span,
                Some(Fix::machine(format!("write `{none}`"), t.span, none)),
            );
            return Some(Expr {
                kind: ExprKind::Name(NameKind::Core(Concept::CoreNone)),
                span: self.span_from(start),
            });
        }
        let operand =
            |k: TokenKind| starts_expr(k) && !matches!(k, TokenKind::Op(Op::Minus | Op::Dot));
        if (self.at_mistake_word(&["await"]) && operand(self.nth(1)))
            || (self.at_mistake_word(&["async"])
                && self.nth(1) == TokenKind::Keyword(Concept::KwFn)
                && self.nth(2) == TokenKind::Op(Op::LParen))
        {
            let t = self.bump();
            let next = self.cur_span().start;
            let span = mk_span(t.span.start, next);
            let word = self.tok_text(t);
            self.mistake(
                codes::ASYNC_AWAIT,
                t.span,
                Some(Fix::machine(format!("delete `{word}`"), span, "")),
            );
            if !self.enter() {
                self.leave();
                return Some(self.error_expr());
            }
            let e = self.primary();
            self.leave();
            return Some(e);
        }
        if self.at_mistake_word(&["lambda"])
            && matches!(self.nth(1), TokenKind::Ident | TokenKind::Op(Op::Colon))
        {
            return Some(self.python_lambda());
        }
        None
    }

    /// `lambda x, y: body` (K0921) → `fn(x, y) => body`.
    fn python_lambda(&mut self) -> Expr {
        let start = self.start();
        let t = self.bump();
        let mut params = Vec::new();
        let mut ok = true;
        while self.peek() == TokenKind::Ident {
            let p = self.bump();
            params.push(LambdaParam {
                name: self.word_ident(p),
                ty: None,
            });
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        let colon_end = if self.at_op(Op::Colon) {
            self.bump().span.end
        } else {
            ok = false;
            self.last_end
        };
        let idx = self.mistake(codes::MULTI_LINE_LAMBDA, t.span, None);
        let body = self.expr();
        let comma = self.spell(Concept::PunctComma);
        let names: Vec<&str> = params.iter().map(|p| p.name.name.as_str()).collect();
        let text = format!(
            "{}({}) =>",
            self.spell(Concept::KwFn),
            names.join(&format!("{comma} "))
        );
        self.extend_fix(
            idx,
            "write a lambda",
            vec![edit(mk_span(t.span.start, colon_end), text)],
            ok,
        );
        Expr {
            kind: ExprKind::Lambda(Lambda {
                params,
                ret: None,
                body: Box::new(body),
            }),
            span: self.span_from(start),
        }
    }

    /// `f"…{x}"` (K0913): the tag is `f`. Reported with a fix that deletes it.
    pub(crate) fn f_string(&mut self, t: Token, tag: &str) {
        if tag == "f" {
            let span = mk_span(t.span.start, t.span.start + 1);
            self.mistake(
                codes::F_STRING,
                span,
                Some(Fix::machine("delete the `f`", span, "")),
            );
        }
    }

    // ----- lambdas -------------------------------------------------------------------------

    /// `fn(x):` (K0921) at the `:`. A body on the next lines is parsed as a block (the
    /// statement's line is then done); a body on the same line is fixed to `=> body`.
    pub(crate) fn lambda_colon(&mut self, fn_span: Span, params: &[LambdaParam]) -> Expr {
        let colon = self.cur();
        let idx = self.mistake(codes::MULTI_LINE_LAMBDA, colon.span, None);
        if self.nth(1) == TokenKind::Newline && self.nth(2) == TokenKind::Indent {
            let start = self.start();
            let _ = self.block();
            self.line_done = true;
            self.block_lambda = Some(BlockLambda {
                diag: idx,
                fn_span,
                untyped: params
                    .iter()
                    .filter(|p| p.ty.is_none())
                    .map(|p| p.name.span.end)
                    .collect(),
            });
            return Expr {
                kind: ExprKind::Error,
                span: self.span_from(start),
            };
        }
        self.bump();
        let mut edits = vec![edit(colon.span, " =>")];
        if self.at_kw(Concept::KwReturn) {
            let r = self.bump();
            let next = self.cur_span().start;
            edits.push(edit(mk_span(r.span.start, next), ""));
        }
        let body = self.expr();
        let colon_start = colon.span.start;
        // `fn(x):` glued: `) =>`; the edit keeps one space either way.
        let prev_end = self.toks[..self.pos]
            .iter()
            .rev()
            .find(|t| t.span.end <= colon_start && t.span.end > 0)
            .map_or(colon_start, |t| t.span.end);
        edits[0] = edit(mk_span(prev_end, colon.span.end), " =>");
        self.extend_fix(idx, "write `=>`", edits, true);
        body
    }

    /// After `let NAME = fn(…):` + block: the K0921 fix hoists the lambda into a local
    /// `fn NAME(…):` declaration.
    pub(crate) fn hoist_block_lambda(&mut self, binding_start: u32, pat: &Pat, ty: bool) {
        let Some(bl) = self.block_lambda.take() else {
            return;
        };
        let PatKind::Bind(name) = &pat.kind else {
            return;
        };
        if ty {
            return;
        }
        let mut edits = vec![edit(
            mk_span(binding_start, bl.fn_span.end),
            format!("{} {}", self.spell(Concept::KwFn), name.name),
        )];
        edits.extend(bl.untyped.iter().map(|&e| edit(at(e), ": _")));
        self.extend_fix(bl.diag, "declare a local function", edits, true);
    }

    // ----- types ---------------------------------------------------------------------------

    /// `List<T>`, `Vec<T>`, `Array<T>` (K0914), at `<` after a type name.
    pub(crate) fn angle_args(&mut self, path: &[Name], type_start: u32) -> Vec<Type> {
        let lt = self.bump();
        let (idx, own) = match self.angle_owner {
            Some(i) => (i, false),
            None => {
                let i = self.mistake(
                    codes::ANGLE_GENERICS,
                    mk_span(type_start, lt.span.end),
                    None,
                );
                self.angle_owner = Some(i);
                (i, true)
            }
        };
        let mut edits = vec![edit(lt.span, "[")];
        if let [n] = path {
            let text = self.slice(n.span);
            if text == "Vec" || text == "Array" {
                edits.push(edit(n.span, self.spell(Concept::CoreList)));
            }
        }
        let mut args = Vec::new();
        loop {
            args.push(self.ty());
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        let ok = if self.at_op(Op::Gt) {
            let gt = self.bump();
            edits.push(edit(gt.span, "]"));
            true
        } else {
            self.expected("`>`");
            false
        };
        self.extend_fix(idx, "write type arguments in `[…]`", edits, ok);
        if own {
            self.angle_owner = None;
        }
        args
    }

    /// `T[]` (K0914), at `[` `]` after a type: `List[T]`.
    pub(crate) fn array_suffix(&mut self, type_start: u32) {
        let lb = self.bump();
        let rb = self.bump();
        let span = mk_span(lb.span.start, rb.span.end);
        let edits = vec![
            edit(
                at(type_start),
                format!("{}[", self.spell(Concept::CoreList)),
            ),
            edit(span, "]"),
        ];
        match self.angle_owner {
            Some(i) => self.extend_fix(i, "", edits, true),
            None => {
                self.mistake(
                    codes::ANGLE_GENERICS,
                    mk_span(type_start, rb.span.end),
                    Some(machine("write `List[T]`", edits)),
                );
            }
        }
    }

    /// Generic parameters written `<T>` (K0914), at `<` after a declaration name.
    pub(crate) fn angle_generics(&mut self) -> Vec<GenericParam> {
        let lt = self.bump();
        let idx = self.mistake(codes::ANGLE_GENERICS, lt.span, None);
        let mut out = Vec::new();
        let mut edits = vec![edit(lt.span, "[")];
        while self.peek() == TokenKind::Ident {
            let start = self.start();
            let t = self.bump();
            let name = self.word_ident(t);
            let mut bounds = Vec::new();
            if self.eat_op(Op::Colon) {
                bounds.push(self.ty());
            }
            out.push(GenericParam {
                name,
                bounds,
                span: self.span_from(start),
            });
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        let ok = if self.at_op(Op::Gt) {
            let gt = self.bump();
            edits.push(edit(gt.span, "]"));
            true
        } else {
            self.expected("`>`");
            false
        };
        self.extend_fix(idx, "write type parameters in `[…]`", edits, ok);
        out
    }

    /// `impl Show for Money:` (K0919), at `for` after the first type.
    pub(crate) fn impl_for(&mut self, impl_start: u32, first: Type) -> (Type, Type) {
        self.bump(); // for
        let target = self.ty();
        let span = mk_span(impl_start, target.span.end);
        let text = format!(
            "{} {} {}",
            self.slice(target.span),
            self.spell(Concept::KwAs),
            self.slice(first.span)
        );
        let as_kw = self.spell(Concept::KwAs);
        self.mistake(
            codes::IMPL_FOR,
            span,
            Some(Fix::machine(
                format!("write `impl Type {as_kw} Trait`"),
                mk_span(first.span.start, target.span.end),
                text,
            )),
        );
        (target, first)
    }

    // ----- match arms ----------------------------------------------------------------------

    /// After a `case` pattern: `->`/`=>` instead of `:` (K0925). The arrow is reported, then
    /// read as the `:`; a trailing `,` on the arm's line is dropped from the token stream.
    pub(crate) fn arm_arrow(&mut self, owner: &mut Option<usize>) {
        if !matches!(self.peek(), TokenKind::Op(Op::Arrow | Op::FatArrow)) {
            return;
        }
        let arrow = self.cur();
        let mut edits = vec![edit(mk_span(self.last_end, arrow.span.end), ":")];
        if let Some(comma) = self.drop_trailing_comma() {
            edits.push(edit(comma, ""));
        }
        match *owner {
            Some(i) => self.extend_fix(i, "", edits, true),
            None => {
                *owner = Some(self.mistake(
                    codes::SWITCH,
                    arrow.span,
                    Some(machine("write `:` after the pattern", edits)),
                ));
            }
        }
        if let Some(t) = self.toks.get_mut(self.pos) {
            t.kind = TokenKind::Op(Op::Colon);
        }
    }

    /// A `,` that ends the current logical line, outside brackets: removed from the token
    /// stream (its text stays in the CST as trivia) and its span returned.
    fn drop_trailing_comma(&mut self) -> Option<Span> {
        let mut depth = 0usize;
        let mut i = self.pos;
        let mut last = None;
        while let Some(t) = self.toks.get(i).filter(|_| i < self.pos + SCAN_CAP) {
            match t.kind {
                k if is_layout(k) => break,
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1)
                }
                _ => {}
            }
            last = Some((i, t.kind, depth));
            i += 1;
        }
        if i >= self.pos + SCAN_CAP {
            return None; // not one short arm line
        }
        match last {
            Some((j, TokenKind::Op(Op::Comma), 0)) if j > self.pos => {
                let t = self.toks.remove(j);
                Some(t.span)
            }
            _ => None,
        }
    }

    /// In a `match` body, a member without `case` whose line holds `=>` or `->` (Rust and
    /// Scala arms, K0925).
    pub(crate) fn at_bare_arm(&self) -> bool {
        let mut depth = 0usize;
        let mut i = self.pos;
        while let Some(t) = self.toks.get(i).filter(|_| i < self.pos + SCAN_CAP) {
            match t.kind {
                k if is_layout(k) => return false,
                TokenKind::Op(Op::Arrow | Op::FatArrow) if depth == 0 => return i > self.pos,
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1)
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// A bare arm (K0925): the `case ` insertion, reported once per `match` body (`owner`);
    /// the arrow edits join it.
    pub(crate) fn bare_arm(&mut self, owner: &mut Option<usize>) {
        let start = self.start();
        let case = self.spell(Concept::KwCase);
        let edits = vec![edit(at(start), format!("{case} "))];
        match *owner {
            Some(i) => self.extend_fix(i, "", edits, true),
            None => {
                let span = self.cur_span();
                *owner = Some(self.mistake(
                    codes::SWITCH,
                    span,
                    Some(machine(&format!("write `{case} pattern:`"), edits)),
                ));
            }
        }
    }

    /// `match x {` (K0925): a braced body of arms, rewritten as indented `case` arms.
    pub(crate) fn brace_match(&mut self, scrut: &Expr, match_start: u32) {
        let (open, close) = self.skip_braces();
        let idx = self.mistake(codes::SWITCH, open.span, None);
        let text = close.and_then(|c| self.arms_rewrite(scrut, open, c, match_start));
        match (text, close) {
            (Some(text), Some(c)) => {
                if let Some(d) = self.diags.get_mut(idx) {
                    d.span = mk_span(match_start, c.span.end);
                }
                self.extend_fix(
                    idx,
                    "write `case` arms",
                    vec![edit(mk_span(match_start, c.span.end), text)],
                    true,
                )
            }
            _ => self.extend_fix(idx, "", Vec::new(), false),
        }
    }

    // ----- after the parse -----------------------------------------------------------------

    /// `Circle(r)` / `Red` used without the leading `.` (K0917), for variants of enums
    /// declared in this file. Variants from other modules need name resolution (M2).
    pub(crate) fn bare_variants(&mut self, file: &File) {
        let mut variants = HashSet::new();
        let mut enum_spans = Vec::new();
        for item in &file.items {
            if let ItemKind::Enum(e) = &item.kind {
                enum_spans.push(item.span);
                for v in &e.variants {
                    variants.insert(v.name.name.clone());
                }
            }
        }
        variants.retain(|v| !self.declared.contains(v));
        if variants.is_empty() {
            return;
        }
        let mut found = Vec::new();
        for (i, t) in self.toks.iter().enumerate() {
            if t.kind != TokenKind::Ident {
                continue;
            }
            // §5.3 rule 7: the backticks are not part of the name.
            let text = self.tok_text(*t).trim_matches('`');
            if !variants.contains(text) || inside_any(&enum_spans, t.span.start, t.span.end) {
                continue;
            }
            let prev = i.checked_sub(1).map(|j| self.toks[j].kind);
            let next = self.toks.get(i + 1).map(|n| n.kind);
            if prev == Some(TokenKind::Op(Op::Dot)) {
                continue;
            }
            // A label or field name (`f(Red: 1)`, `Red: Int`), but `case Red:` is a pattern.
            if next == Some(TokenKind::Op(Op::Colon))
                && prev != Some(TokenKind::Keyword(Concept::KwCase))
            {
                continue;
            }
            found.push(t.span);
        }
        for span in found {
            let d = Diagnostic::new(codes::BARE_VARIANT, self.file, span).fix(Fix::machine(
                "write the variant with a leading `.`",
                at(span.start),
                ".",
            ));
            self.diags.push(d);
        }
    }

    /// Orders every fix's edits by position (several detectors add edits out of order).
    pub(crate) fn sort_fix_edits(&mut self) {
        for d in &mut self.diags {
            for f in &mut d.fixes {
                f.edits.sort_by_key(|e| (e.span.start, e.span.end));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalam_base::{FileId, Skin};

    fn diags(src: &str, skin: Skin) -> Vec<Diagnostic> {
        let out = crate::parse_source(src, skin, FileId(0));
        assert_eq!(out.cst.text(), src, "CST must stay lossless");
        out.diagnostics
    }

    fn codes_of(src: &str) -> Vec<&'static str> {
        diags(src, Skin::En).iter().map(|d| d.code).collect()
    }

    /// Applies the first fix of every diagnostic (edits are sorted and disjoint).
    fn apply(src: &str, ds: &[Diagnostic]) -> String {
        let mut edits: Vec<&Edit> = ds
            .iter()
            .flat_map(|d| d.fixes.first())
            .flat_map(|f| &f.edits)
            .collect();
        edits.sort_by_key(|e| (e.span.start, e.span.end));
        let mut out = String::new();
        let mut at = 0usize;
        for e in edits {
            let (s, t) = (e.span.start as usize, e.span.end as usize);
            assert!(s >= at, "overlapping edits in {src:?}");
            out.push_str(&src[at..s]);
            out.push_str(&e.insert);
            at = t;
        }
        out.push_str(&src[at..]);
        out
    }

    /// `src` gives exactly `code` (once), on `line`, with a machine fix whose result has no
    /// error at all.
    #[track_caller]
    fn fixes_to_clean(src: &str, skin: Skin, code: &str, line: usize) -> String {
        let ds = diags(src, skin);
        let got: Vec<(&str, usize)> = ds
            .iter()
            .map(|d| {
                (
                    d.code,
                    src[..d.span.start as usize].matches('\n').count() + 1,
                )
            })
            .collect();
        assert_eq!(got, vec![(code, line)], "{src}");
        let f = ds[0]
            .fixes
            .first()
            .unwrap_or_else(|| panic!("{code} has no fix: {src}"));
        assert_eq!(f.applicability, Applicability::Machine, "{code}: {src}");
        let fixed = apply(src, &ds);
        let after = diags(&fixed, skin);
        assert!(
            after.iter().all(|d| !d.is_error()),
            "fix of {code} leaves {:?}:\n{fixed}",
            after.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        fixed
    }

    fn en(src: &str, code: &str, line: usize) -> String {
        fixes_to_clean(src, Skin::En, code, line)
    }

    #[test]
    fn k0901_triple_equals() {
        let f = en("fn f(a: Int) -> Bool:\n    return a === 1\n", "K0901", 2);
        assert!(f.contains("a == 1"));
        let f = en("fn f(a: Int) -> Bool:\n    return a !== 1\n", "K0901", 2);
        assert!(f.contains("a != 1"));
        // Not glued: an ordinary syntax error, not K0901.
        assert_eq!(codes_of("fn f():\n    let x = a == = b\n"), vec!["K0250"]);
    }

    #[test]
    fn k0902_def_function_func() {
        let f = en("def f(x):\n    return x\n", "K0902", 1);
        assert_eq!(f, "fn f(x: _):\n    return x\n");
        en("function f(x, y: Int) -> Int:\n    return y\n", "K0902", 1);
        en(
            "fn main():\n    func g(a: Int):\n        pass\n    g(1)\n",
            "K0902",
            2,
        );
        // `def` as a name stays legal.
        assert!(codes_of("fn main():\n    let def = 1\n    print(\"{def}\")\n").is_empty());
        assert!(codes_of("fn main():\n    `def`(1)\n").is_empty());
    }

    #[test]
    fn k0903_elif_and_else_if_braces() {
        let f = en(
            "fn f(n: Int):\n    if n < 0:\n        pass\n    elif n == 0:\n        pass\n",
            "K0903",
            4,
        );
        assert!(f.contains("else if n == 0:"));
        en(
            "fn f(n: Int):\n    if n < 0:\n        pass\n    else if (n == 0) {\n        pass\n    }\n",
            "K0903",
            4,
        );
        assert!(codes_of("fn main():\n    let elif = 1\n    elif = 2\n").len() <= 1);
    }

    #[test]
    fn k0905_brace_blocks() {
        let f = en(
            "fn f(n: Int) -> Int:\n    if n < 0 { return 1 }\n    return 2\n",
            "K0905",
            2,
        );
        assert!(f.contains("if n < 0: return 1\n"), "{f}");
        let f = en(
            "fn f(n: Int) -> Int:\n    if n < 0 {\n        return 1\n    } else {\n        return 2\n    }\n",
            "K0905",
            2,
        );
        assert_eq!(
            f,
            "fn f(n: Int) -> Int:\n    if n < 0:\n        return 1\n    else:\n        return 2\n"
        );
        en("fn f() {\n    pass\n}\n", "K0905", 1);
        en("fn f():\n    while x {\n        g()\n    }\n", "K0905", 2);
        en("type P {\n    x: Int,\n    y: Int,\n}\n", "K0905", 1);
    }

    #[test]
    fn k0906_null_words() {
        let f = en("fn f():\n    let x = null\n", "K0906", 2);
        assert!(f.contains("= None"));
        en("fn f():\n    g(nil)\n", "K0906", 2);
        assert!(codes_of("fn f(undefined: Int):\n    print(\"{undefined}\")\n").is_empty());
    }

    #[test]
    fn k0907_async_await() {
        en("async fn f():\n    pass\n", "K0907", 1);
        let f = en("fn f():\n    let x = await g()\n", "K0907", 2);
        assert!(f.contains("let x = g()"));
    }

    #[test]
    fn k0908_try_catch_throw() {
        let src = "fn main():\n    try:\n        print(\"x\")\n";
        assert_eq!(codes_of(src), vec!["K0908"]);
        let src = "fn main():\n    try:\n        a()\n    except E as e:\n        b()\n    c()\n";
        assert_eq!(codes_of(src), vec!["K0908"]);
        let src = "fn main():\n    try {\n        a()\n    } catch (e) {\n        b()\n    }\n";
        assert_eq!(codes_of(src), vec!["K0908"]);
        let f = en(
            "fn f() -> Result[Int, String]:\n    throw \"bad\"\n",
            "K0908",
            2,
        );
        assert!(f.contains("return Err(\"bad\")"));
        en(
            "fn f() -> Result[Int, String]:\n    raise E(1)\n",
            "K0908",
            2,
        );
    }

    #[test]
    fn k0909_increment() {
        let f = en("fn f():\n    var i = 0\n    i++\n", "K0909", 3);
        assert!(f.contains("i += 1"));
        en("fn f():\n    var i = 0\n    i--\n", "K0909", 3);
        en("fn f():\n    var i = 0\n    ++i\n", "K0909", 3);
        // Not a statement: reported, fix only suggested.
        let ds = diags("fn f():\n    var i = 0\n    g(i++)\n", Skin::En);
        assert_eq!(ds.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["K0909"]);
        assert_eq!(ds[0].fixes[0].applicability, Applicability::Suggestion);
        // `a - -1` and `x--1` are ordinary arithmetic.
        assert!(codes_of("fn f(x: Int) -> Int:\n    return x--1\n").is_empty());
    }

    #[test]
    fn k0910_ternary() {
        let f = en(
            "fn f(n: Int) -> String:\n    let s = n > 0 ? \"pos\" : \"neg\"\n    return s\n",
            "K0910",
            2,
        );
        assert!(f.contains("let s = if n > 0: \"pos\" else: \"neg\""), "{f}");
        // The `?` operator is untouched.
        assert!(
            codes_of("fn f() -> Result[Int, E]:\n    let x = g()?\n    return Ok(x)\n").is_empty()
        );
        assert!(codes_of("fn f() -> Result[Int, E]:\n    return Ok(g()? - 1)\n").is_empty());
        // `?` then a call, a member or an inline `if`: never a ternary.
        for src in [
            "fn f() -> Result[Int, E]:\n    if g()?(1): return Ok(1)\n    return Ok(2)\n",
            "fn f() -> Result[Int, E]:\n    for x in g()?.items: h(x)\n    return Ok(2)\n",
            "fn f() -> Result[Int, E]:\n    let v = if a: b?(c) else: d\n    return Ok(v)\n",
            "fn f() -> Result[Int, E]:\n    h(x: g?(), y: 1)\n    return Ok(1)\n",
            "fn f() -> Result[Int, E]:\n    let m = {k?(): {}}\n    return Ok(1)\n",
        ] {
            assert!(codes_of(src).is_empty(), "{src}");
        }
    }

    #[test]
    fn k0912_const_and_let_mut() {
        let f = en("fn f():\n    let mut x = 1\n    x = 2\n", "K0912", 2);
        assert!(f.contains("var x = 1"));
        let f = en("fn f():\n    const x = 1\n    print(\"{x}\")\n", "K0912", 2);
        assert!(f.contains("let x = 1"));
        en("const WIDTH = 80\n", "K0912", 1);
        assert!(codes_of("fn f():\n    let mut = 1\n    print(\"{mut}\")\n").is_empty());
    }

    #[test]
    fn k0913_f_strings_and_templates() {
        let f = en("fn f(n: Int):\n    print(f\"n {n}\")\n", "K0913", 2);
        assert!(f.contains("print(\"n {n}\")"));
        let f = en("fn f(n: Int):\n    print(`n ${n}`)\n", "K0913", 2);
        assert!(f.contains("print(\"n {n}\")"), "{f}");
    }

    #[test]
    fn k0914_angle_generics() {
        let f = en("fn f(xs: List<Int>) -> Int:\n    return 1\n", "K0914", 1);
        assert!(f.contains("List[Int]"));
        let f = en(
            "fn f(xs: Vec<Map<String, Int>>) -> Int:\n    return 1\n",
            "K0914",
            1,
        );
        assert!(f.contains("List[Map[String, Int]]"), "{f}");
        let f = en("fn f(xs: Int[]) -> Int:\n    return 1\n", "K0914", 1);
        assert!(f.contains("List[Int]"), "{f}");
        en(
            "fn first<T>(xs: List[T]) -> T:\n    return xs[0]\n",
            "K0914",
            1,
        );
    }

    #[test]
    fn k0915_k0916_k0918_calls() {
        en(
            "fn f(x: Option[Int]) -> Int:\n    return x.unwrap()\n",
            "K0915",
            2,
        );
        // `print(a, b)` (K0916) is a well-formed call: the type checker's finding.
        assert!(codes_of("fn f(n: Int):\n    print(\"total\", n)\n").is_empty());
        let f = en(
            "fn f(xs: List[Int]) -> Int:\n    return len(xs)\n",
            "K0918",
            2,
        );
        assert!(f.contains("xs.len()"));
        en("fn f(x: Int) -> String:\n    return str(x)\n", "K0918", 2);
        // A file that declares `len` calls its own function.
        assert!(codes_of(
            "fn len(x: Int) -> Int:\n    return x\nfn f() -> Int:\n    return len(1)\n"
        )
        .is_empty());
    }

    #[test]
    fn k0917_bare_variants_of_local_enums() {
        let src = "enum Shape:\n    Circle(r: Float)\n    Empty\n\nfn f() -> Shape:\n    return Circle(1.0)\n";
        let f = en(src, "K0917", 6);
        assert!(f.contains("return .Circle(1.0)"));
        assert!(codes_of(
            "enum Shape:\n    Circle(r: Float)\n\nfn f() -> Shape:\n    return .Circle(1.0)\n"
        )
        .is_empty());
    }

    #[test]
    fn k0917_ignores_backticks_and_sees_lambda_parameters() {
        let e = "enum E:\n    Red\n\n";
        // §5.3 rule 7: the backticks are not part of the name, so both spellings are K0917.
        for use_ in ["Red", "`Red`"] {
            let src = format!("{e}fn f() -> E:\n    return {use_}\n");
            assert_eq!(codes_of(&src), vec!["K0917"], "{src:?}");
        }
        // A declaring position counts whether or not the word is reserved in this skin
        // (fuzz, iteration 6: a Franco file's label `None:` is a name there, but a core-name
        // token in English, so only the English conversion got K0917).
        for src in [
            "enum E:\n    Red\n\nfn f():\n    g(a: 1, Red: 2)\n    return Red\n",
            "enum E:\n    `None`\n\nfn f():\n    g(a: 1, None: 2)\n    return `None`\n",
            "use std.{x, None}\n\nenum E:\n    `None`\n\nfn f():\n    return `None`\n",
        ] {
            assert!(codes_of(src).is_empty(), "{src:?}: {:?}", codes_of(src));
        }
        // A lambda parameter declares the name (FOUNDER-QUEUE Q9), in either spelling.
        for body in [
            "let h = fn(Red) => Red",
            "let h = fn(`Red`) => `Red`",
            "let h = fn(a, Red) => Red",
        ] {
            let src = format!("{e}fn f():\n    {body}\n");
            assert!(codes_of(&src).is_empty(), "{src:?}: {:?}", codes_of(&src));
        }
    }

    #[test]
    fn k0919_impl_for() {
        let f = en(
            "type Money:\n    cents: Int\n\nimpl Show for Money:\n    fn show(self) -> String => \"x\"\n",
            "K0919",
            4,
        );
        assert!(f.contains("impl Money as Show:"));
    }

    #[test]
    fn k0920_class_struct_interface() {
        let f = en("class Point:\n    x: Int\n", "K0920", 1);
        assert!(f.starts_with("type Point:"));
        let f = en("struct Point {\n    x: Int,\n    y: Int,\n}\n", "K0920", 1);
        assert_eq!(f, "type Point:\n    x: Int\n    y: Int\n");
        en("interface Shape:\n    fn area(self) -> Float\n", "K0920", 1);
        // Methods inside a class: still one finding.
        assert_eq!(
            codes_of("class P:\n    x: Int\n    def get(self):\n        return self.x\n"),
            vec!["K0920"]
        );
    }

    #[test]
    fn k0921_lambdas() {
        let src = "fn main():\n    let double = fn(x: Int):\n        return x * 2\n    print(\"{double(2)}\")\n";
        let f = en(src, "K0921", 2);
        assert!(
            f.contains("fn double(x: Int):\n        return x * 2"),
            "{f}"
        );
        let f = en(
            "fn main():\n    let g = lambda x: x * 2\n    print(\"{g(1)}\")\n",
            "K0921",
            2,
        );
        assert!(f.contains("fn(x) => x * 2"), "{f}");
    }

    #[test]
    fn k0923_power() {
        let f = en("fn f(x: Int) -> Int:\n    return x ** 2\n", "K0923", 2);
        assert!(f.contains("x.pow(2)"));
        let f = en("fn f() -> Int:\n    return 2 ** 3\n", "K0923", 2);
        assert!(f.contains("(2).pow(3)"), "{f}");
        // `f[Int](x)` (K0922) is a well-formed index-then-call: the type checker's finding.
        assert!(codes_of("fn f():\n    let r = parse[Int](s)\n").is_empty());
    }

    #[test]
    fn k0925_switch_and_arrows() {
        let src = "fn f(n: Int) -> String:\n    switch n {\n        case 0 -> return \"zero\"\n        default -> return \"other\"\n    }\n";
        let f = en(src, "K0925", 2);
        assert!(
            f.contains(
                "match n:\n        case 0: return \"zero\"\n        case _: return \"other\""
            ),
            "{f}"
        );
        en(
            "fn f(n: Int) -> Int:\n    match n:\n        case 0 => 1\n        case _ => 2\n",
            "K0925",
            3,
        );
        let f = en(
            "fn f(n: Int) -> Int:\n    match n:\n        0 => 1,\n        _ => 2,\n",
            "K0925",
            3,
        );
        assert!(f.contains("case 0: 1\n        case _: 2\n"), "{f}");
    }

    #[test]
    fn k0926_c_for_and_range() {
        let f = en(
            "fn f(n: Int):\n    for (var i = 0; i < n; i++):\n        print(\"{i}\")\n",
            "K0926",
            2,
        );
        assert!(f.contains("for i in 0..n:"), "{f}");
        let f = en(
            "fn f(n: Int):\n    for i in range(n):\n        print(\"{i}\")\n",
            "K0926",
            2,
        );
        assert!(f.contains("for i in 0..n:"), "{f}");
        let f = en(
            "fn f(xs: List[Int]):\n    for (let x of xs) {\n        print(\"{x}\")\n    }\n",
            "K0926",
            2,
        );
        assert!(f.contains("for x in xs:"), "{f}");
        // A tuple pattern in parentheses is ordinary Kalam.
        assert!(codes_of(
            "fn f(ps: List[(Int, Int)]):\n    for (a, b) in ps:\n        print(\"{a}{b}\")\n"
        )
        .is_empty());
    }

    #[test]
    fn fixes_are_spelled_in_the_files_skin() {
        let f = fixes_to_clean("def f(x):\n    rag3 x\n", Skin::Franco, "K0902", 1);
        assert!(f.starts_with("dala f(x: _):"), "{f}");
        let f = fixes_to_clean(
            "دالة f(n: عدد) -> نص:\n    ليكن s = n > 0 ? \"a\" : \"b\"\n    أرجع s\n",
            Skin::Ar,
            "K0910",
            2,
        );
        assert!(f.contains("إذا n > 0: \"a\" وإلا: \"b\""), "{f}");
    }

    /// Every seeded K09xx case of `eval/suites/seeded-errors`, in every skin: the expected
    /// code is the only finding, and applying its machine fix leaves a file with no error.
    #[test]
    fn seeded_mistakes_fix_to_clean_files_in_every_skin() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../eval/suites/seeded-errors");
        let Ok(dirs) = std::fs::read_dir(&root) else {
            return; // suites not checked out
        };
        let mut checked = 0;
        let mut failures = Vec::new();
        for dir in dirs.flatten().map(|d| d.path()) {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            if !name.starts_with("k09") {
                continue;
            }
            for f in std::fs::read_dir(&dir).unwrap().flatten() {
                let file = f.file_name().to_string_lossy().into_owned();
                let Some(skin) = Skin::from_path(&file) else {
                    continue;
                };
                let src = std::fs::read_to_string(f.path()).unwrap();
                let ds = diags(&src, skin);
                checked += 1;
                let want = name[..5].to_uppercase();
                if ds.len() != 1 || ds[0].code != want {
                    failures.push(format!(
                        "{name}/{file}: {:?}",
                        ds.iter().map(|d| d.code).collect::<Vec<_>>()
                    ));
                    continue;
                }
                match ds[0].fixes.first() {
                    Some(fx) if fx.applicability == Applicability::Machine => {
                        let fixed = apply(&src, &ds);
                        let left: Vec<_> = diags(&fixed, skin)
                            .iter()
                            .filter(|d| d.is_error())
                            .map(|d| d.code)
                            .collect();
                        if !left.is_empty() {
                            failures.push(format!("{name}/{file}: fix leaves {left:?}\n{fixed}"));
                        }
                    }
                    // `try:` (K0908) has no certain rewrite: the calls inside need `?`.
                    _ if want == "K0908" => {}
                    _ => failures.push(format!("{name}/{file}: no machine fix")),
                }
            }
        }
        assert!(checked >= 60, "only {checked} files");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn cascades_are_dropped() {
        // An unterminated string swallowed the `)`: only the lexer's K0129.
        let ds = diags("fn main():\n    print(\"abc)\n", Skin::En);
        assert_eq!(ds.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["K0129"]);
    }

    /// Every lookahead for a mistake's shape is capped (`SCAN_CAP`): a huge bracketed
    /// "line" (the lexer joins everything inside brackets) stays linear, never quadratic.
    #[test]
    fn mistake_scans_stay_linear_on_huge_joined_lines() {
        let timed = |src: String| {
            let t = std::time::Instant::now();
            let out = crate::parse_source(&src, Skin::En, FileId(0));
            assert_eq!(out.cst.text(), src);
            let secs = t.elapsed().as_secs_f64();
            assert!(secs < 30.0, "took {secs:.1}s (quadratic?)");
            out.diagnostics.len()
        };
        // 50 000 `?` each followed by an expression start, in one call.
        let mut src = String::from("fn main() -> Result[Int, E]:\n    f(");
        src.push_str(&"x?-1, ".repeat(50_000));
        src.push_str(")\n");
        assert_eq!(timed(src), 0);
        // 20 000 `catch`/`switch`/`elif` words inside one `{ … }` body.
        let mut src = String::from("fn main():\n    if a {\n");
        src.push_str(&"        catch switch elif\n".repeat(20_000));
        src.push_str("    }\n");
        timed(src);
        // 20 000 `for (` headers, nested in brackets.
        let mut src = String::from("fn main():\n    for (");
        src.push_str(&"for (".repeat(20_000));
        src.push('\n');
        timed(src);
        // 20 000 ternaries in one list: each still found.
        let mut src = String::from("fn main():\n    let xs = [");
        src.push_str(&"c ? 1 : 2, ".repeat(20_000));
        src.push_str("]\n");
        assert_eq!(timed(src), 20_000);
    }
}
