//! Expressions (spec §8.2, precedence §10.6), patterns, types, and string literals with
//! interpolation holes (§5.7).

use kalam_ast::*;
use kalam_base::Span;
use kalam_diagnostics::Diagnostic;
use kalam_lexer::{Op, Token, TokenKind};
use kalam_skins::Concept;
use kalam_syntax::SyntaxKind;

use crate::{codes, mk_span, Parser};

// Binding levels, low to high (§10.6 levels 9..3; unary and postfix are handled apart).
const OR: u8 = 1;
const AND: u8 = 2;
const NOT: u8 = 3;
const CMP: u8 = 4;
const RANGE: u8 = 5;
const ADD: u8 = 6;
const MUL: u8 = 7;

fn infix(k: TokenKind) -> Option<(u8, Infix)> {
    Some(match k {
        TokenKind::Keyword(Concept::KwOr) => (OR, Infix::Bin(BinOp::Or)),
        TokenKind::Keyword(Concept::KwAnd) => (AND, Infix::Bin(BinOp::And)),
        TokenKind::Op(Op::EqEq) => (CMP, Infix::Bin(BinOp::Eq)),
        TokenKind::Op(Op::NotEq) => (CMP, Infix::Bin(BinOp::NotEq)),
        TokenKind::Op(Op::Lt) => (CMP, Infix::Bin(BinOp::Lt)),
        TokenKind::Op(Op::Le) => (CMP, Infix::Bin(BinOp::Le)),
        TokenKind::Op(Op::Gt) => (CMP, Infix::Bin(BinOp::Gt)),
        TokenKind::Op(Op::Ge) => (CMP, Infix::Bin(BinOp::Ge)),
        TokenKind::Op(Op::DotDot) => (RANGE, Infix::Range(false)),
        TokenKind::Op(Op::DotDotEq) => (RANGE, Infix::Range(true)),
        TokenKind::Op(Op::Plus) => (ADD, Infix::Bin(BinOp::Add)),
        TokenKind::Op(Op::Minus) => (ADD, Infix::Bin(BinOp::Sub)),
        TokenKind::Op(Op::Star) => (MUL, Infix::Bin(BinOp::Mul)),
        TokenKind::Op(Op::Slash) => (MUL, Infix::Bin(BinOp::Div)),
        TokenKind::Op(Op::SlashSlash) => (MUL, Infix::Bin(BinOp::IntDiv)),
        TokenKind::Op(Op::Percent) => (MUL, Infix::Bin(BinOp::Rem)),
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum Infix {
    Bin(BinOp),
    Range(bool),
}

fn boxed(e: Expr) -> Box<Expr> {
    Box::new(e)
}

/// ASCII-normalizes a numeric literal (`١٢` → `12`, `٫` → `.`).
fn ascii_number(text: &str) -> String {
    kalam_lexer::to_ascii_digits(text)
}

impl<'t> Parser<'t> {
    // ----- expressions --------------------------------------------------------------------

    /// `expr = lambda | if_inline | or_expr`
    pub(crate) fn expr(&mut self) -> Expr {
        let m = self.start_node(SyntaxKind::Expr);
        let e = if !self.enter() {
            self.error_expr()
        } else if self.at_kw(Concept::KwFn) && self.nth(1) == TokenKind::Op(Op::LParen) {
            self.lambda()
        } else if self.at_kw(Concept::KwIf) {
            self.if_inline()
        } else {
            let e = self.binary(OR);
            if self.ternary_ahead() {
                // `c ? a : b` (K0910).
                self.ternary(e)
            } else {
                e
            }
        };
        self.leave();
        self.finish_node(m);
        e
    }

    pub(crate) fn or_expr(&mut self) -> Expr {
        let m = self.start_node(SyntaxKind::Expr);
        let e = if self.enter() {
            self.binary(OR)
        } else {
            self.error_expr()
        };
        self.leave();
        self.finish_node(m);
        e
    }

    /// Operator-precedence parsing of `or_expr` .. `mul_expr` (§8.2, §10.6).
    fn binary(&mut self, min: u8) -> Expr {
        let start = self.start();
        let mut lhs = if self.at_kw(Concept::KwNot) {
            if min > NOT {
                self.expected("an expression (`not` needs parentheses here)");
                return self.error_expr();
            }
            if !self.enter() {
                self.leave();
                return self.error_expr();
            }
            self.bump();
            let inner = self.binary(NOT);
            self.leave();
            Expr {
                kind: ExprKind::Not(boxed(inner)),
                span: self.span_from(start),
            }
        } else {
            self.unary()
        };
        let mut prev_level = 0u8;
        let mut links = 0u32;
        while let Some((level, op)) = infix(self.peek()) {
            if level < min {
                break;
            }
            if level == prev_level && (level == CMP || level == RANGE) {
                // Comparison and range are non-associative (§10.6): `a < b < c` is K0720.
                let span = self.cur_span();
                if level == CMP {
                    self.error_at(codes::CHAINED_COMPARISON, span);
                } else {
                    self.expected("an operator other than a second range");
                }
            }
            prev_level = level;
            links += 1;
            self.height += 1;
            if self.height > crate::MAX_HEIGHT {
                self.too_deep();
                break;
            }
            let op_tok = self.bump();
            if matches!(op, Infix::Bin(BinOp::Eq | BinOp::NotEq)) {
                self.triple_equals(op_tok);
            }
            let rhs = if self.enter() {
                self.binary(level + 1)
            } else {
                self.error_expr()
            };
            self.leave();
            let span = self.span_from(start);
            lhs = Expr {
                kind: match op {
                    Infix::Bin(op) => ExprKind::Binary {
                        op,
                        lhs: boxed(lhs),
                        rhs: boxed(rhs),
                    },
                    Infix::Range(inclusive) => ExprKind::Range {
                        lo: boxed(lhs),
                        hi: boxed(rhs),
                        inclusive,
                    },
                },
                span,
            };
        }
        self.height = self.height.saturating_sub(links);
        if min <= AND {
            self.glued_and();
        }
        lhs
    }

    /// Spec §5.4 last bullet (FOUNDER-QUEUE Q22): in an Arabic file, `و` written against
    /// the next word (`إذا أ وب:`) lexes as one identifier, which is a parse error where an
    /// `and` could continue the expression. It is reported here, as the K0250 the caller
    /// would report, with a machine fix that inserts the space. The fix is offered only when
    /// the rest after `و` lexes as one clean name, core name, `صح`, `خطأ`, `ليس` or `ذات`, so
    /// the fixed line parses. The caller's own error at this token is then suppressed.
    fn glued_and(&mut self) {
        if self.skin != kalam_base::Skin::Ar
            || self.peek() != TokenKind::Ident
            || self.last_error_pos == Some(self.pos)
        {
            return;
        }
        let t = self.cur();
        let text = self.tok_text(t);
        let Some(rest) = text.strip_prefix('و') else {
            return;
        };
        if !self.lexes_as_operand(rest) {
            return;
        }
        let fixed = format!("و {rest}");
        let found = self.describe(t);
        let span = mk_span(t.span.start, t.span.end);
        self.last_error_pos = Some(self.pos);
        self.diags.push(
            Diagnostic::new(codes::UNEXPECTED_TOKEN, self.file, span)
                .arg("expected", format!("`{fixed}`"))
                .arg("found", found)
                .fix(kalam_diagnostics::Fix::machine(
                    format!("write `{fixed}`"),
                    span,
                    fixed,
                )),
        );
    }

    /// `word` (Arabic skin) is exactly one clean token that can start the operand of `and`.
    fn lexes_as_operand(&self, word: &str) -> bool {
        if word.is_empty() {
            return false;
        }
        let lexed = kalam_lexer::lex_fragment(word, kalam_base::Skin::Ar, self.file);
        if lexed.diagnostics.iter().any(|d| d.is_error()) {
            return false;
        }
        let mut toks = lexed.tokens.iter().filter(|t| !crate::is_layout(t.kind));
        let one = match (toks.next(), toks.next()) {
            (Some(t), None) => t,
            _ => return false,
        };
        one.span.start == 0
            && one.span.end as usize == word.len()
            && matches!(
                one.kind,
                TokenKind::Ident
                    | TokenKind::Core(_)
                    | TokenKind::Keyword(
                        Concept::KwTrue | Concept::KwFalse | Concept::KwNot | Concept::KwSelf
                    )
            )
    }

    /// `unary = "-" unary | postfix` (iterative, so long `- - -` chains cannot overflow).
    pub(crate) fn unary(&mut self) -> Expr {
        let mut minus = Vec::new();
        while self.at_op(Op::Minus) {
            minus.push(self.start());
            self.bump();
            self.height += 1;
            if self.height > crate::MAX_HEIGHT {
                self.too_deep();
                break;
            }
        }
        let mut e = self.postfix();
        for start in minus.iter().rev() {
            e = Expr {
                kind: ExprKind::Neg(boxed(e)),
                span: self.span_from(*start),
            };
        }
        self.height = self.height.saturating_sub(minus.len() as u32);
        e
    }

    /// `postfix = primary { call | index | "." member | "?" }`
    fn postfix(&mut self) -> Expr {
        let start = self.start();
        let mut e = self.primary();
        let mut links = 0u32;
        loop {
            let kind = match self.peek() {
                TokenKind::Op(Op::LParen) => {
                    let args = self.call_args();
                    self.check_call(&e, &args, self.last_end);
                    ExprKind::Call {
                        callee: boxed(e),
                        args,
                    }
                }
                TokenKind::Op(Op::Star) if self.at_glued_pair(Op::Star) => self.power(e),
                TokenKind::Op(Op::Plus | Op::Minus) if self.at_postfix_incr() => {
                    self.postfix_incr(&e);
                    e.span = self.span_from(start);
                    continue;
                }
                TokenKind::Op(Op::LBracket) => {
                    let open = self.bump().span;
                    let index = self.expr();
                    if !self.eat_op(Op::RBracket) {
                        if crate::is_layout(self.peek()) {
                            self.error_at(codes::UNCLOSED_DELIMITER, mk_span(open.start, open.end));
                        } else {
                            self.expected("`]`");
                        }
                    }
                    ExprKind::Index {
                        base: boxed(e),
                        index: boxed(index),
                    }
                }
                TokenKind::Op(Op::Dot) => {
                    self.bump();
                    let member = match self.peek() {
                        TokenKind::Int => {
                            let t = self.bump();
                            Member::Index {
                                text: ascii_number(self.tok_text(t)),
                                span: mk_span(t.span.start, t.span.end),
                            }
                        }
                        // The lexer makes every word after `.` an identifier (§5.3 rule 8).
                        TokenKind::Ident | TokenKind::Keyword(_) | TokenKind::Core(_) => {
                            let t = self.bump();
                            Member::Name(self.word_ident(t))
                        }
                        _ => {
                            self.expected("a member name");
                            Member::Name(Ident {
                                name: String::new(),
                                span: self.cur_span(),
                            })
                        }
                    };
                    ExprKind::Member {
                        base: boxed(e),
                        member,
                    }
                }
                TokenKind::Op(Op::Question) if !self.ternary_ahead() => {
                    self.bump();
                    ExprKind::Try(boxed(e))
                }
                _ => break,
            };
            e = Expr {
                kind,
                span: self.span_from(start),
            };
            links += 1;
            self.height += 1;
            if self.height > crate::MAX_HEIGHT {
                self.too_deep();
                break;
            }
        }
        self.height = self.height.saturating_sub(links);
        e
    }

    pub(crate) fn primary(&mut self) -> Expr {
        let start = self.start();
        if self.peek() == TokenKind::Ident {
            if let Some(e) = self.primary_mistake() {
                return e;
            }
        }
        let kind = match self.peek() {
            TokenKind::Int => {
                let t = self.bump();
                ExprKind::Int(self.int_lit(t))
            }
            TokenKind::Float => {
                let t = self.bump();
                ExprKind::Float(FloatLit {
                    text: ascii_number(self.tok_text(t)),
                })
            }
            TokenKind::Str => {
                let t = self.bump();
                ExprKind::Str(self.str_lit(t, 0))
            }
            TokenKind::TaggedStr => {
                let t = self.bump();
                let text = self.tok_text(t);
                let tag_len = text.find('"').unwrap_or(text.len());
                let tag = text[..tag_len].to_string();
                self.f_string(t, &tag);
                ExprKind::Tagged {
                    tag,
                    lit: self.str_lit(t, tag_len),
                }
            }
            TokenKind::Keyword(Concept::KwTrue) => {
                self.bump();
                ExprKind::Bool(true)
            }
            TokenKind::Keyword(Concept::KwFalse) => {
                self.bump();
                ExprKind::Bool(false)
            }
            TokenKind::Keyword(Concept::KwSelf) => {
                self.bump();
                ExprKind::SelfValue
            }
            TokenKind::Ident | TokenKind::Core(_) => {
                let t = self.bump();
                ExprKind::Name(self.name_of(t).kind)
            }
            TokenKind::Op(Op::Dot) => {
                self.bump();
                ExprKind::ImplicitMember(self.member_ident())
            }
            TokenKind::Keyword(Concept::KwUnsafe) => {
                self.bump();
                self.expect_op(Op::Dot, "`.` after `unsafe`");
                ExprKind::UnsafeMember(self.member_ident())
            }
            TokenKind::Op(Op::LParen) => return self.paren(),
            TokenKind::Op(Op::LBracket) => {
                let open = self.bump().span;
                let items = self.delimited(Op::RBracket, open, "`]`", |p| Some(p.expr()));
                ExprKind::List(items)
            }
            TokenKind::Op(Op::LBrace) => {
                let open = self.bump().span;
                let entries = self.delimited(Op::RBrace, open, "`}`", |p| {
                    let k = p.expr();
                    let v = if p.expect_op(Op::Colon, "`:` in a map entry") {
                        p.expr()
                    } else {
                        p.error_expr()
                    };
                    Some((k, v))
                });
                ExprKind::Map(entries)
            }
            TokenKind::Keyword(Concept::KwFn) if self.nth(1) == TokenKind::Op(Op::LParen) => {
                return self.lambda();
            }
            TokenKind::Keyword(Concept::KwIf) => return self.if_inline(),
            _ => {
                self.expected("an expression");
                return self.error_expr();
            }
        };
        Expr {
            kind,
            span: self.span_from(start),
        }
    }

    fn member_ident(&mut self) -> Ident {
        if matches!(
            self.peek(),
            TokenKind::Ident | TokenKind::Keyword(_) | TokenKind::Core(_)
        ) {
            let t = self.bump();
            self.word_ident(t)
        } else {
            self.expected("a member name");
            Ident {
                name: String::new(),
                span: self.cur_span(),
            }
        }
    }

    pub(crate) fn int_lit(&self, t: Token) -> IntLit {
        let raw = self.tok_text(t);
        IntLit {
            text: ascii_number(raw),
            value: kalam_lexer::int_literal_value(raw),
        }
    }

    /// `paren = "(" [ expr [ "," [ expr { "," expr } [ "," ] ] ] ] ")"`: unit, group, tuple.
    fn paren(&mut self) -> Expr {
        let start = self.start();
        let open = self.bump().span;
        if self.eat_op(Op::RParen) {
            return Expr {
                kind: ExprKind::Unit,
                span: self.span_from(start),
            };
        }
        let first = self.expr();
        let kind = if self.at_op(Op::Comma) {
            self.bump();
            let mut items = vec![first];
            items.extend(self.delimited(Op::RParen, open, "`)`", |p| Some(p.expr())));
            ExprKind::Tuple(items)
        } else {
            self.close_paren(open);
            ExprKind::Paren(boxed(first))
        };
        Expr {
            kind,
            span: self.span_from(start),
        }
    }

    fn close_paren(&mut self, open: Span) {
        if !self.eat_op(Op::RParen) {
            if crate::is_layout(self.peek()) {
                self.error_at(codes::UNCLOSED_DELIMITER, mk_span(open.start, open.end));
            } else {
                self.expected("`)`");
            }
        }
    }

    /// `call = "(" [ arg_list ] ")"`; `arg = [ IDENT ":" ] expr` (keyword labels allowed).
    pub(crate) fn call_args(&mut self) -> Vec<Arg> {
        let open = self.bump().span;
        let mut seen_named = false;
        let mut reported = false;
        self.delimited(Op::RParen, open, "`)`", |p| {
            let start = p.start();
            let label = if p.at_word() && p.nth(1) == TokenKind::Op(Op::Colon) {
                let t = p.bump();
                p.bump();
                Some(p.word_ident(t))
            } else {
                None
            };
            let value = p.expr();
            let span = p.span_from(start);
            if label.is_some() {
                seen_named = true;
            } else if seen_named && !reported {
                reported = true;
                p.diags
                    .push(Diagnostic::new(codes::POSITIONAL_AFTER_NAMED, p.file, span));
            }
            Some(Arg { label, value, span })
        })
    }

    /// `lambda = "fn" "(" [ lparam { "," lparam } [ "," ] ] ")" [ "->" type ] "=>" expr`
    fn lambda(&mut self) -> Expr {
        let start = self.start();
        let fn_span = self.bump().span; // fn
        let open = self.bump().span; // (
        let params = self.delimited(Op::RParen, open, "`)`", |p| {
            if !p.at_binding_name() {
                p.expected("a parameter name");
                return None;
            }
            let name = p.binding_ident("a parameter name");
            let ty = if p.eat_op(Op::Colon) {
                Some(p.ty())
            } else {
                None
            };
            Some(LambdaParam { name, ty })
        });
        let ret = if self.eat_op(Op::Arrow) {
            Some(self.ty())
        } else {
            None
        };
        let body = if self.at_op(Op::Colon) {
            // `fn(x): …` is a multi-line lambda (K0921, DR-15).
            self.lambda_colon(fn_span, &params)
        } else {
            self.expect_op(Op::FatArrow, "`=>`");
            self.expr()
        };
        Expr {
            kind: ExprKind::Lambda(Lambda {
                params,
                ret,
                body: boxed(body),
            }),
            span: self.span_from(start),
        }
    }

    /// `if_inline` in expression position (§8.3 rule 2): `else` is required (K0731).
    fn if_inline(&mut self) -> Expr {
        let start = self.start();
        let if_span = self.cur_span();
        self.bump(); // if
        let cond = self.or_expr();
        self.expect_op(Op::Colon, "`:`");
        let then = self.expr();
        let body = self.inline_arm(then);
        let mut branches = vec![IfBranch { cond, body }];
        let else_body = self.inline_else_chain(&mut branches, if_span);
        Expr {
            kind: ExprKind::If(IfExpr {
                branches,
                else_body,
                inline: true,
            }),
            span: self.span_from(start),
        }
    }

    /// Wraps an inline-`if` arm expression as a one-line block.
    pub(crate) fn inline_arm(&self, e: Expr) -> Block {
        let span = e.span;
        Block {
            stmts: vec![Stmt {
                kind: StmtKind::Expr(e),
                span,
            }],
            inline: true,
            span,
        }
    }

    /// `{ "else" "if" or_expr ":" expr } "else" ":" expr` of an inline `if`. Arms are parsed
    /// as `expr` (a superset of the EBNF's `or_expr`) so a lambda or nested inline `if` in an
    /// arm parses, as CRITIQUE P1.5 notes the spec leaves open.
    pub(crate) fn inline_else_chain(
        &mut self,
        branches: &mut Vec<IfBranch>,
        if_span: Span,
    ) -> Option<Block> {
        let mut links = 0;
        loop {
            if !self.eat_kw(Concept::KwElse) {
                self.diags
                    .push(Diagnostic::new(codes::IF_WITHOUT_ELSE, self.file, if_span));
                break None;
            }
            if self.eat_kw(Concept::KwIf) {
                let cond = self.or_expr();
                self.expect_op(Op::Colon, "`:`");
                let then = self.expr();
                let body = self.inline_arm(then);
                branches.push(IfBranch { cond, body });
                links += 1;
                if links > crate::MAX_HEIGHT {
                    self.too_deep();
                    break None;
                }
                continue;
            }
            self.expect_op(Op::Colon, "`:`");
            let e = self.expr();
            break Some(self.inline_arm(e));
        }
    }

    // ----- patterns -----------------------------------------------------------------------

    /// `pattern = bind_pat { "|" bind_pat }`
    pub(crate) fn pattern(&mut self) -> Pat {
        let m = self.start_node(SyntaxKind::Pattern);
        let start = self.start();
        let p = if !self.enter() {
            Pat {
                kind: PatKind::Error,
                span: self.cur_span(),
            }
        } else {
            let first = self.bind_pat();
            if self.at_op(Op::Pipe) {
                let mut alts = vec![first];
                while self.eat_op(Op::Pipe) {
                    alts.push(self.bind_pat());
                    if alts.len() as u32 > crate::MAX_HEIGHT {
                        self.too_deep();
                        break;
                    }
                }
                Pat {
                    kind: PatKind::Or(alts),
                    span: self.span_from(start),
                }
            } else {
                first
            }
        };
        self.leave();
        self.finish_node(m);
        p
    }

    fn bind_pat(&mut self) -> Pat {
        let start = self.start();
        let p = self.prim_pat();
        if self.eat_kw(Concept::KwAs) {
            let name = self.binding_ident("a name after `as`");
            Pat {
                kind: PatKind::As {
                    pat: Box::new(p),
                    name,
                },
                span: self.span_from(start),
            }
        } else {
            p
        }
    }

    fn prim_pat(&mut self) -> Pat {
        let start = self.start();
        let kind = match self.peek() {
            TokenKind::Ident if self.tok_text(self.cur()) == "_" => {
                self.bump();
                PatKind::Wild
            }
            TokenKind::Ident
                if !matches!(
                    self.nth(1),
                    TokenKind::Op(Op::Dot) | TokenKind::Op(Op::LParen)
                ) =>
            {
                let t = self.bump();
                PatKind::Bind(self.word_ident(t))
            }
            TokenKind::Core(c) if c.is_core_ctor() => {
                self.bump();
                let inner = if self.at_op(Op::LParen) {
                    let open = self.bump().span;
                    let inner = self.pattern();
                    self.close_paren(open);
                    Some(Box::new(inner))
                } else {
                    None
                };
                PatKind::Ctor { ctor: c, inner }
            }
            // A bare core name of the file's skin (`let Int = 1`, `for print in xs`) is kept
            // as a one-segment path: a `Name` holds the core name skin-free, so every skin
            // gives the same AST, and name resolution reports K0413 for it (§7.2). Another
            // skin's core name lexes as an identifier and binds (FOUNDER-QUEUE Q21).
            TokenKind::Ident | TokenKind::Core(_) => {
                let t = self.bump();
                let mut path = vec![self.name_of(t)];
                while self.at_op(Op::Dot) && self.nth_is_word(1) {
                    self.bump();
                    let t = self.bump();
                    path.push(Name {
                        kind: NameKind::Ident(self.word_ident(t).name),
                        span: mk_span(t.span.start, t.span.end),
                    });
                }
                let fields = self.pat_fields_opt();
                PatKind::Path { path, fields }
            }
            TokenKind::Op(Op::Minus) | TokenKind::Int => self.int_pat(),
            TokenKind::Str => {
                let t = self.bump();
                PatKind::Str(self.str_lit(t, 0))
            }
            TokenKind::Keyword(Concept::KwTrue) => {
                self.bump();
                PatKind::Bool(true)
            }
            TokenKind::Keyword(Concept::KwFalse) => {
                self.bump();
                PatKind::Bool(false)
            }
            TokenKind::Op(Op::Dot) => {
                self.bump();
                let name = self.member_ident();
                let fields = self.pat_fields_opt();
                PatKind::Implicit { name, fields }
            }
            TokenKind::Op(Op::LParen) => {
                let open = self.bump().span;
                let first = self.pattern();
                if self.eat_op(Op::Comma) {
                    let mut items = vec![first];
                    items.extend(self.delimited(Op::RParen, open, "`)`", |p| Some(p.pattern())));
                    PatKind::Tuple(items)
                } else {
                    // A tuple pattern needs at least two elements (§8.2 `prim_pat`).
                    self.expected("`,` (a tuple pattern has at least two elements)");
                    self.close_paren(open);
                    first.kind
                }
            }
            TokenKind::Op(Op::LBracket) => {
                let open = self.bump().span;
                let elems = self.delimited(Op::RBracket, open, "`]`", |p| {
                    if p.eat_op(Op::DotDot) {
                        let name = if p.at_binding_name() {
                            Some(p.binding_ident("a name"))
                        } else {
                            None
                        };
                        Some(ListPatElem::Rest(name))
                    } else {
                        Some(ListPatElem::Pat(p.pattern()))
                    }
                });
                PatKind::List(elems)
            }
            _ => {
                self.expected("a pattern");
                return Pat {
                    kind: PatKind::Error,
                    span: self.cur_span(),
                };
            }
        };
        Pat {
            kind,
            span: self.span_from(start),
        }
    }

    /// `int_pat = [ "-" ] INT_LIT [ ( ".." | "..=" ) [ "-" ] INT_LIT ]`
    fn int_pat(&mut self) -> PatKind {
        let lo = self.int_bound();
        let inclusive = if self.at_op(Op::DotDot) {
            false
        } else if self.at_op(Op::DotDotEq) {
            true
        } else {
            return PatKind::Int(lo);
        };
        self.bump();
        let hi = self.int_bound();
        PatKind::Range { lo, hi, inclusive }
    }

    fn int_bound(&mut self) -> IntPatBound {
        let negative = self.eat_op(Op::Minus);
        if self.peek() == TokenKind::Int {
            let t = self.bump();
            IntPatBound {
                negative,
                lit: self.int_lit(t),
            }
        } else {
            self.expected("an integer");
            IntPatBound {
                negative,
                lit: IntLit {
                    text: String::new(),
                    value: None,
                },
            }
        }
    }

    /// `[ "(" pat_fields ")" ]`; `pat_fields = pat_field { "," pat_field } [ "," ".." ] | ".."`
    fn pat_fields_opt(&mut self) -> Option<PatFields> {
        if !self.at_op(Op::LParen) {
            return None;
        }
        let open = self.bump().span;
        let mut rest = false;
        let fields = self.delimited(Op::RParen, open, "`)`", |p| {
            if p.eat_op(Op::DotDot) {
                rest = true;
                return None;
            }
            let label = if p.at_word() && p.nth(1) == TokenKind::Op(Op::Colon) {
                let t = p.bump();
                p.bump();
                Some(p.word_ident(t))
            } else {
                None
            };
            let pat = p.pattern();
            Some(PatField { label, pat })
        });
        Some(PatFields { fields, rest })
    }

    // ----- types --------------------------------------------------------------------------

    /// `type = fn_type | "(" ")" | "(" type "," type { "," type } [ "," ] ")" | named_type`
    pub(crate) fn ty(&mut self) -> Type {
        let m = self.start_node(SyntaxKind::Type);
        let start = self.start();
        let t = if !self.enter() {
            Type {
                kind: TypeKind::Error,
                span: self.cur_span(),
            }
        } else {
            let kind = self.ty_kind();
            Type {
                kind,
                span: self.span_from(start),
            }
        };
        self.leave();
        self.finish_node(m);
        t
    }

    fn ty_kind(&mut self) -> TypeKind {
        match self.peek() {
            TokenKind::Op(Op::At) => {
                // `@sync fn(…) -> T`
                self.bump();
                let is_sync =
                    self.peek() == TokenKind::Ident && self.tok_text(self.cur()) == "sync";
                if is_sync {
                    self.bump();
                } else {
                    self.expected("`sync`");
                }
                if self.at_kw(Concept::KwFn) {
                    self.fn_type(true)
                } else {
                    self.expected("`fn`");
                    TypeKind::Error
                }
            }
            TokenKind::Keyword(Concept::KwFn) => self.fn_type(false),
            TokenKind::Op(Op::LParen) => {
                let open = self.bump().span;
                if self.eat_op(Op::RParen) {
                    return TypeKind::Unit;
                }
                let first = self.ty();
                if self.eat_op(Op::Comma) {
                    let mut items = vec![first];
                    items.extend(self.delimited(Op::RParen, open, "`)`", |p| Some(p.ty())));
                    TypeKind::Tuple(items)
                } else {
                    self.expected("`,` (a tuple type has at least two elements)");
                    self.close_paren(open);
                    first.kind
                }
            }
            TokenKind::Ident | TokenKind::Core(_) => {
                let t = self.bump();
                let type_start = t.span.start.min(t.span.end);
                let mut path = vec![self.name_of(t)];
                while self.at_op(Op::Dot) && self.nth_is_word(1) {
                    self.bump();
                    let t = self.bump();
                    path.push(Name {
                        kind: NameKind::Ident(self.word_ident(t).name),
                        span: mk_span(t.span.start, t.span.end),
                    });
                }
                let args = if self.at_op(Op::Lt) {
                    // `List<T>` (K0914).
                    self.angle_args(&path, type_start)
                } else if self.at_op(Op::LBracket) && self.nth(1) != TokenKind::Op(Op::RBracket) {
                    let open = self.bump().span;
                    self.delimited(Op::RBracket, open, "`]`", |p| Some(p.ty()))
                } else {
                    Vec::new()
                };
                // `T[]` (K0914), possibly repeated.
                while self.at_op(Op::LBracket) && self.nth(1) == TokenKind::Op(Op::RBracket) {
                    self.array_suffix(type_start);
                }
                TypeKind::Named { path, args }
            }
            _ => {
                self.expected("a type");
                TypeKind::Error
            }
        }
    }

    /// `fn "(" [ type { "," type } [ "," ] ] ")" "->" type`
    fn fn_type(&mut self, sync: bool) -> TypeKind {
        self.bump(); // fn
        let params = if self.at_op(Op::LParen) {
            let open = self.bump().span;
            self.delimited(Op::RParen, open, "`)`", |p| Some(p.ty()))
        } else {
            self.expected("`(`");
            Vec::new()
        };
        let ret = if self.expect_op(Op::Arrow, "`->`") {
            self.ty()
        } else {
            Type {
                kind: TypeKind::Error,
                span: self.cur_span(),
            }
        };
        TypeKind::Fn {
            sync,
            params,
            ret: Box::new(ret),
        }
    }

    // ----- strings ------------------------------------------------------------------------

    /// Splits a string or tagged-literal token into text parts and parsed holes (§5.7).
    /// `tag_len` is the byte length of the tag before the opening quote.
    pub(crate) fn str_lit(&mut self, t: Token, tag_len: usize) -> StrLit {
        let span = mk_span(t.span.start, t.span.end);
        let raw = self.tok_text(t);
        let body = raw.get(tag_len..).unwrap_or("");
        let triple = body.starts_with("\"\"\"");
        let q = (if triple { 3 } else { 1 }).min(body.len());
        let close = if triple {
            body.len() >= 6 && body.ends_with("\"\"\"")
        } else {
            body.len() >= 2 && body.ends_with('"')
        };
        let inner_end = if close { body.len() - q } else { body.len() };
        let inner = body.get(q..inner_end).unwrap_or("");
        let base = span.start.saturating_add((tag_len + q) as u32);

        let mut parts = Vec::new();
        let mut text = String::new();
        let mut chars = inner.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    text.push(c);
                    if let Some((_, n)) = chars.next() {
                        text.push(n);
                        // `\u{X…}`: these braces belong to the escape, not to a hole.
                        if n == 'u' && matches!(chars.peek(), Some(&(_, '{'))) {
                            for (_, d) in chars.by_ref() {
                                text.push(d);
                                if d == '}' {
                                    break;
                                }
                            }
                        }
                    }
                }
                '{' => {
                    // Find the matching `}` (braces may nest inside the hole's code).
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
                    let hole_span = mk_span(
                        base.saturating_add(i as u32),
                        base.saturating_add(i as u32 + 1),
                    );
                    let Some(end) = end else {
                        // The lexer keeps a string open while a hole is open, so an unclosed
                        // hole is already its K0129 (unterminated string); no second report.
                        text.push_str(&inner[i..]);
                        break;
                    };
                    if !text.is_empty() {
                        parts.push(StrPart::Text(std::mem::take(&mut text)));
                    }
                    let code = &inner[i + 1..end];
                    let lead = code.len() - code.trim_start().len();
                    let trimmed = code.trim();
                    let abs = base.saturating_add((i + 1 + lead) as u32);
                    let e = if trimmed.is_empty() {
                        self.diags.push(
                            Diagnostic::new(codes::BAD_HOLE, self.file, hole_span)
                                .arg("problem", "empty"),
                        );
                        Expr {
                            kind: ExprKind::Error,
                            span: hole_span,
                        }
                    } else {
                        self.hole(trimmed, abs)
                    };
                    parts.push(StrPart::Hole(e));
                    // Skip the hole's characters.
                    while matches!(chars.peek(), Some(&(k, _)) if k <= end) {
                        chars.next();
                    }
                }
                _ => text.push(c),
            }
        }
        if !text.is_empty() {
            parts.push(StrPart::Text(text));
        }
        StrLit {
            parts,
            triple,
            span,
        }
    }

    /// Parses the code of one interpolation hole: it is re-lexed in the file's skin (holes
    /// convert like code, §4.3) and must be exactly one expression.
    fn hole(&mut self, code: &str, abs: u32) -> Expr {
        let lexed = kalam_lexer::lex_fragment(code, self.skin, self.file);
        let shift = |s: Span| Span {
            start: s.start.saturating_add(abs),
            end: s.end.saturating_add(abs),
        };
        let toks: Vec<Token> = lexed
            .tokens
            .iter()
            .filter(|t| {
                !matches!(
                    t.kind,
                    TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
                )
            })
            .map(|t| Token {
                span: shift(t.span),
                ..*t
            })
            .collect();
        for mut d in lexed.diagnostics {
            // Source-text (§5.1) problems were already reported on the whole string, and
            // layout codes do not apply inside a hole. The name and number checks (§5.3
            // rules 3-5, §5.6) are code-only, so the string pass never made them: keep them.
            let code_only = matches!(
                d.code,
                "SEC0105" | "SEC0106" | "SEC0120" | "SEC0121" | "SEC0122" | "SEC0123" | "SEC0124"
            );
            if (d.code.starts_with("SEC") && !code_only)
                || matches!(d.code, "K0103" | "K0120" | "K0122")
            {
                continue;
            }
            d.span = shift(d.span);
            for l in &mut d.labels {
                l.span = shift(l.span);
            }
            for f in &mut d.fixes {
                for e in &mut f.edits {
                    e.span = shift(e.span);
                }
            }
            self.diags.push(d);
        }
        // Parse with the hole's tokens swapped in; the CST keeps the string as one token.
        let saved_toks = std::mem::replace(&mut self.toks, toks);
        let saved_pos = std::mem::replace(&mut self.pos, 0);
        let saved_cst = std::mem::replace(&mut self.cst_on, false);
        let saved_end = self.last_end;
        let saved_err = self.last_error_pos.take();
        let e = self.expr();
        if !self.at_eof() {
            let span = self.cur_span();
            if self.error_at(codes::BAD_HOLE, span) {
                if let Some(d) = self.diags.last_mut() {
                    d.args.push(("problem", "more than one expression".into()));
                }
            }
        }
        self.toks = saved_toks;
        self.pos = saved_pos;
        self.cst_on = saved_cst;
        self.last_end = saved_end;
        self.last_error_pos = saved_err;
        e
    }
}
