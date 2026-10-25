//! Files, items, blocks and statements (spec §8.2 "File and items", "Blocks and statements",
//! §8.3 rule 1 for `if` at statement and rhs start).

use kalam_ast::*;
use kalam_lexer::{Op, TokenKind};
use kalam_skins::Concept;
use kalam_syntax::SyntaxKind;

use crate::{codes, mk_span, Parser};

impl<'t> Parser<'t> {
    // ----- file ---------------------------------------------------------------------------

    pub(crate) fn file(&mut self) -> File {
        let start = 0;
        let mut uses = Vec::new();
        let mut items: Vec<Item> = Vec::new();
        loop {
            let before = self.pos;
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Newline | TokenKind::Dedent => {
                    self.bump();
                }
                TokenKind::Indent => {
                    let span = self.cur_span();
                    self.error_at(codes::UNEXPECTED_INDENT, span);
                    self.bump();
                }
                _ if self.at_use() => {
                    let u = self.use_decl();
                    if !items.is_empty() {
                        self.diags.push(kalam_diagnostics::Diagnostic::new(
                            codes::USE_AFTER_ITEM,
                            self.file,
                            u.span,
                        ));
                    }
                    uses.push(u);
                }
                _ => items.push(self.item()),
            }
            if self.pos == before && !self.at_eof() {
                let m = self.start_node(SyntaxKind::Error);
                self.bump();
                self.finish_node(m);
            }
        }
        File {
            uses,
            items,
            span: mk_span(start, self.last_end),
        }
    }

    fn at_use(&self) -> bool {
        self.at_kw(Concept::KwUse)
            || (self.at_kw(Concept::KwExport) && self.nth(1) == TokenKind::Keyword(Concept::KwUse))
    }

    /// `[export] use module_path [use_tail] NEWLINE`
    fn use_decl(&mut self) -> UseDecl {
        let m = self.start_node(SyntaxKind::UseDecl);
        let start = self.start();
        let export = self.eat_kw(Concept::KwExport);
        self.bump(); // use
        let mut path = Vec::new();
        if self.at_word() {
            let t = self.bump();
            path.push(self.word_ident(t));
            while self.at_op(Op::Slash) && self.nth_is_word(1) {
                self.bump();
                let t = self.bump();
                path.push(self.word_ident(t));
            }
        } else {
            self.expected("a module path");
        }
        let tail = if self.at_op(Op::Dot) && self.nth(1) == TokenKind::Op(Op::LBrace) {
            self.bump();
            let open = self.bump().span;
            let items = self.delimited(Op::RBrace, open, "`}`", |p| p.use_item());
            UseTail::Items(items)
        } else if self.eat_kw(Concept::KwAs) {
            UseTail::Alias(self.ident("a module alias"))
        } else {
            UseTail::None
        };
        let span = self.span_from(start);
        self.end_line(None);
        self.finish_node(m);
        UseDecl {
            export,
            path,
            tail,
            span,
        }
    }

    fn use_item(&mut self) -> Option<UseItem> {
        if self.at_kw(Concept::KwSelf) {
            let t = self.bump();
            return Some(UseItem::SelfModule(mk_span(t.span.start, t.span.end)));
        }
        if !self.at_word() {
            self.expected("a name");
            return None;
        }
        let t = self.bump();
        let name = self.word_ident(t);
        let alias = if self.eat_kw(Concept::KwAs) {
            Some(self.ident("an alias"))
        } else {
            None
        };
        Some(UseItem::Name { name, alias })
    }

    // ----- items --------------------------------------------------------------------------

    /// The keyword that starts the item at the cursor, after attributes and visibility.
    fn item_keyword(&self) -> TokenKind {
        let mut i = 0;
        loop {
            match self.nth(i) {
                TokenKind::Op(Op::At) => {
                    while !matches!(self.nth(i), TokenKind::Newline | TokenKind::Eof) {
                        i += 1;
                    }
                    i += 1;
                }
                TokenKind::Keyword(Concept::KwExport) | TokenKind::Keyword(Concept::KwOpaque) => {
                    i += 1
                }
                k => return k,
            }
        }
    }

    pub(crate) fn item(&mut self) -> Item {
        let kind = match self.item_keyword() {
            TokenKind::Keyword(Concept::KwFn) => SyntaxKind::FnDecl,
            TokenKind::Keyword(Concept::KwType) => SyntaxKind::TypeDecl,
            TokenKind::Keyword(Concept::KwEnum) => SyntaxKind::EnumDecl,
            TokenKind::Keyword(Concept::KwTrait) => SyntaxKind::TraitDecl,
            TokenKind::Keyword(Concept::KwImpl) => SyntaxKind::ImplDecl,
            TokenKind::Keyword(Concept::KwLet) => SyntaxKind::ConstDecl,
            TokenKind::Keyword(Concept::KwExtern) => SyntaxKind::ExternDecl,
            TokenKind::Keyword(Concept::KwTest) => SyntaxKind::TestDecl,
            _ => SyntaxKind::Error,
        };
        let m = self.start_node(kind);
        let start = self.start();
        let attrs = self.attributes();
        let vis = if self.eat_kw(Concept::KwExport) {
            if self.eat_kw(Concept::KwOpaque) {
                Visibility::ExportOpaque
            } else {
                Visibility::Export
            }
        } else {
            Visibility::Private
        };
        let kind = match self.peek() {
            TokenKind::Keyword(Concept::KwFn) => ItemKind::Fn(self.fn_decl(false)),
            TokenKind::Keyword(Concept::KwType) => self.type_decl(),
            TokenKind::Keyword(Concept::KwEnum) => self.enum_decl(),
            TokenKind::Keyword(Concept::KwTrait) => self.trait_decl(),
            TokenKind::Keyword(Concept::KwImpl) => self.impl_decl(),
            TokenKind::Keyword(Concept::KwLet) => self.const_decl(),
            TokenKind::Keyword(Concept::KwExtern) => self.extern_decl(),
            TokenKind::Keyword(Concept::KwTest) => self.test_decl(),
            _ => match self.item_mistake() {
                Some(kind) => kind,
                None => {
                    let span = self.cur_span();
                    self.error_at(codes::EXPECTED_ITEM, span);
                    self.skip_line();
                    ItemKind::Error
                }
            },
        };
        let span = self.span_from(start);
        self.finish_node(m);
        Item {
            attrs,
            vis,
            kind,
            span,
        }
    }

    pub(crate) fn attributes(&mut self) -> Vec<Attribute> {
        let mut attrs = Vec::new();
        while self.at_op(Op::At) && !self.at_sync_fn_type() {
            let m = self.start_node(SyntaxKind::Attribute);
            let start = self.start();
            self.bump();
            let mut path = Vec::new();
            if self.at_word() {
                let t = self.bump();
                path.push(self.word_ident(t));
                while self.at_op(Op::Dot) && self.nth_is_word(1) {
                    self.bump();
                    let t = self.bump();
                    path.push(self.word_ident(t));
                }
            } else {
                self.expected("an attribute name");
            }
            let args = if self.at_op(Op::LParen) {
                Some(self.call_args())
            } else {
                None
            };
            let span = self.span_from(start);
            attrs.push(Attribute { path, args, span });
            self.end_line(None);
            self.finish_node(m);
        }
        attrs
    }

    fn at_sync_fn_type(&self) -> bool {
        self.nth(2) == TokenKind::Keyword(Concept::KwFn)
    }

    /// `fn_decl = fn_sig ( block | "=>" expr NEWLINE )`; trait members may end at NEWLINE.
    pub(crate) fn fn_decl(&mut self, trait_member: bool) -> FnDecl {
        let start = self.start();
        let sig = self.fn_sig();
        let body = self.fn_body(trait_member, None);
        FnDecl {
            sig,
            body,
            span: self.span_from(start),
        }
    }

    /// The body after a signature. `owner`: a mistake already reported for this function
    /// (K0902), which a `{ … }` body adds its edits to instead of a second report.
    pub(crate) fn fn_body(&mut self, trait_member: bool, owner: Option<usize>) -> FnBody {
        if self.at_op(Op::Colon) || self.at_op(Op::LBrace) {
            FnBody::Block(self.block_owned(owner).0)
        } else if self.eat_op(Op::FatArrow) {
            let e = self.expr();
            self.end_line(None);
            FnBody::Expr(e)
        } else if trait_member && self.at_line_end() {
            self.end_line(None);
            FnBody::None
        } else {
            self.expected("`:` or `=>`");
            self.skip_line();
            FnBody::Block(Block {
                stmts: Vec::new(),
                inline: false,
                span: self.cur_span(),
            })
        }
    }

    /// `fn_sig = "fn" NAME [generics] "(" [param_list] ")" ["->" type]`
    pub(crate) fn fn_sig(&mut self) -> FnSig {
        let start = self.start();
        self.bump(); // fn
        let name = self.decl_name("a function name");
        let generics = self.generics();
        let params = if self.at_op(Op::LParen) {
            let open = self.bump().span;
            self.delimited(Op::RParen, open, "`)`", |p| p.param())
        } else {
            self.expected("`(`");
            Vec::new()
        };
        let ret = if self.eat_op(Op::Arrow) {
            Some(self.ty())
        } else {
            None
        };
        FnSig {
            name,
            generics,
            params,
            ret,
            span: self.span_from(start),
        }
    }

    fn param(&mut self) -> Option<Param> {
        if self.at_kw(Concept::KwSelf) {
            let t = self.bump();
            return Some(Param::SelfParam(mk_span(t.span.start, t.span.end)));
        }
        let start = self.start();
        if self.peek() != TokenKind::Ident {
            self.expected("a parameter name");
            return None;
        }
        let t = self.bump();
        let name = self.word_ident(t);
        let ty = if self.at_op(Op::Colon) {
            self.bump();
            self.ty()
        } else if let Some(untyped) = self.untyped.as_mut() {
            // After `def` (K0902): the fix gives this parameter a `_` type hole.
            untyped.push(name.span.end);
            Type {
                kind: TypeKind::Error,
                span: name.span,
            }
        } else {
            self.expected("`:` and a type");
            Type {
                kind: TypeKind::Error,
                span: self.cur_span(),
            }
        };
        let default = if self.eat_op(Op::Eq) {
            Some(self.expr())
        } else {
            None
        };
        Some(Param::Named {
            name,
            ty,
            default,
            span: self.span_from(start),
        })
    }

    /// An item name: an identifier, or a core name (`main`); K0413 is name resolution's job.
    pub(crate) fn decl_name(&mut self, what: &str) -> Name {
        match self.peek() {
            TokenKind::Ident | TokenKind::Core(_) => {
                let t = self.bump();
                self.name_of(t)
            }
            _ => {
                self.expected(what);
                Name {
                    kind: NameKind::Ident(String::new()),
                    span: self.cur_span(),
                }
            }
        }
    }

    pub(crate) fn generics(&mut self) -> Vec<GenericParam> {
        if self.at_op(Op::Lt) && self.nth(1) == TokenKind::Ident {
            return self.angle_generics();
        }
        if !self.at_op(Op::LBracket) {
            return Vec::new();
        }
        let open = self.bump().span;
        self.delimited(Op::RBracket, open, "`]`", |p| {
            if p.peek() != TokenKind::Ident {
                p.expected("a type parameter");
                return None;
            }
            let start = p.start();
            let t = p.bump();
            let name = p.word_ident(t);
            let mut bounds = Vec::new();
            if p.eat_op(Op::Colon) {
                bounds.push(p.ty());
                while p.eat_op(Op::Plus) {
                    bounds.push(p.ty());
                }
            }
            Some(GenericParam {
                name,
                bounds,
                span: p.span_from(start),
            })
        })
    }

    /// `: NEWLINE INDENT` opening an indented member list. Returns false (after reporting)
    /// if there is no indented block.
    pub(crate) fn open_body(&mut self) -> bool {
        if !self.at_op(Op::Colon) {
            self.expected("`:`");
            self.skip_line();
            return false;
        }
        self.bump();
        if !self.at_op_newline() {
            self.expected("a new line after `:`");
            self.skip_line();
            return false;
        }
        self.bump();
        if self.peek() != TokenKind::Indent {
            let span = self.cur_span();
            self.error_at(codes::EXPECTED_BLOCK, span);
            return false;
        }
        self.bump();
        true
    }

    fn at_op_newline(&self) -> bool {
        self.peek() == TokenKind::Newline
    }

    /// Parses members until the closing DEDENT, recovering line by line.
    pub(crate) fn body_members<T>(
        &mut self,
        mut member: impl FnMut(&mut Self) -> Option<T>,
    ) -> Vec<T> {
        let mut out = Vec::new();
        loop {
            let before = self.pos;
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Dedent => {
                    self.bump();
                    break;
                }
                TokenKind::Newline => {
                    self.bump();
                }
                TokenKind::Indent => {
                    let span = self.cur_span();
                    self.error_at(codes::UNEXPECTED_INDENT, span);
                    self.skip_block_tokens();
                }
                _ => {
                    if let Some(x) = member(self) {
                        out.push(x);
                    }
                }
            }
            if self.pos == before && !self.at_eof() {
                self.skip_line();
                if self.pos == before {
                    self.bump();
                }
            }
        }
        out
    }

    fn type_decl(&mut self) -> ItemKind {
        self.bump(); // type
        let name = self.decl_name("a type name");
        let generics = self.generics();
        let body = if self.eat_op(Op::Eq) {
            let t = self.ty();
            self.end_line(None);
            TypeBody::Alias(t)
        } else if self.at_op(Op::LBrace) {
            self.brace_record(None, false);
            TypeBody::Record(Vec::new())
        } else if self.open_body() {
            TypeBody::Record(self.body_members(|p| p.field_decl()))
        } else {
            TypeBody::Record(Vec::new())
        };
        ItemKind::Type(TypeDecl {
            name,
            generics,
            body,
        })
    }

    pub(crate) fn field_decl(&mut self) -> Option<FieldDecl> {
        let start = self.start();
        let attrs = self.attributes();
        if !self.at_word() {
            self.expected("a field name");
            self.skip_line();
            return None;
        }
        let t = self.bump();
        let name = self.word_ident(t);
        let ty = if self.expect_op(Op::Colon, "`:` and a type") {
            self.ty()
        } else {
            Type {
                kind: TypeKind::Error,
                span: self.cur_span(),
            }
        };
        let default = if self.eat_op(Op::Eq) {
            Some(self.expr())
        } else {
            None
        };
        let span = self.span_from(start);
        self.end_line(None);
        Some(FieldDecl {
            attrs,
            name,
            ty,
            default,
            span,
        })
    }

    fn enum_decl(&mut self) -> ItemKind {
        self.bump(); // enum
        let name = self.decl_name("an enum name");
        let generics = self.generics();
        let variants = if self.at_op(Op::LBrace) {
            self.brace_record(None, true);
            Vec::new()
        } else if self.open_body() {
            self.body_members(|p| p.variant())
        } else {
            Vec::new()
        };
        ItemKind::Enum(EnumDecl {
            name,
            generics,
            variants,
        })
    }

    fn variant(&mut self) -> Option<Variant> {
        let start = self.start();
        let attrs = self.attributes();
        if self.peek() != TokenKind::Ident {
            self.expected("a variant name");
            self.skip_line();
            return None;
        }
        let t = self.bump();
        let name = self.word_ident(t);
        let fields = if self.at_op(Op::LParen) {
            let open = self.bump().span;
            Some(self.delimited(Op::RParen, open, "`)`", |p| {
                let start = p.start();
                let label = if p.at_word() && p.nth(1) == TokenKind::Op(Op::Colon) {
                    let t = p.bump();
                    p.bump();
                    Some(p.word_ident(t))
                } else {
                    None
                };
                let ty = p.ty();
                Some(VariantField {
                    label,
                    ty,
                    span: p.span_from(start),
                })
            }))
        } else {
            None
        };
        let span = self.span_from(start);
        self.end_line(None);
        Some(Variant {
            attrs,
            name,
            fields,
            span,
        })
    }

    fn trait_decl(&mut self) -> ItemKind {
        self.bump(); // trait
        let name = self.decl_name("a trait name");
        let generics = self.generics();
        let members = if self.at_op(Op::LBrace) {
            self.brace_members();
            Vec::new()
        } else if self.open_body() {
            self.body_members(|p| {
                if p.at_kw(Concept::KwFn) {
                    Some(p.fn_decl(true))
                } else {
                    p.expected("`fn`");
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
    }

    fn impl_decl(&mut self) -> ItemKind {
        let impl_start = self.start();
        self.bump(); // impl
        let generics = self.generics();
        let ty = self.ty();
        let (ty, as_trait) = if self.eat_kw(Concept::KwAs) {
            (ty, Some(self.ty()))
        } else if self.at_kw(Concept::KwFor) {
            let (target, tr) = self.impl_for(impl_start, ty);
            (target, Some(tr))
        } else {
            (ty, None)
        };
        let members = if self.at_op(Op::LBrace) {
            self.brace_members();
            Vec::new()
        } else if self.open_body() {
            self.body_members(|p| {
                let start = p.start();
                let attrs = p.attributes();
                let export = p.eat_kw(Concept::KwExport);
                if p.at_kw(Concept::KwFn) {
                    let func = p.fn_decl(false);
                    Some(ImplMember {
                        attrs,
                        export,
                        func,
                        span: p.span_from(start),
                    })
                } else {
                    p.expected("`fn`");
                    p.skip_line();
                    None
                }
            })
        } else {
            Vec::new()
        };
        ItemKind::Impl(ImplDecl {
            generics,
            ty,
            as_trait,
            members,
        })
    }

    pub(crate) fn const_decl(&mut self) -> ItemKind {
        let let_tok = self.bump(); // `let`, or a `const` already reported (K0912)
        if self.at_let_mut() {
            self.module_let_mut(let_tok);
        }
        let name = self.decl_name("a name");
        let ty = if self.eat_op(Op::Colon) {
            Some(self.ty())
        } else {
            None
        };
        let value = if self.expect_op(Op::Eq, "`=`") {
            self.expr()
        } else {
            self.error_expr()
        };
        self.end_line(None);
        ItemKind::Const(ConstDecl { name, ty, value })
    }

    fn extern_decl(&mut self) -> ItemKind {
        self.bump(); // extern
        let decl = if self.at_kw(Concept::KwFn) {
            ExternDecl::Fn(self.fn_sig())
        } else if self.eat_kw(Concept::KwType) {
            let name = self.decl_name("a type name");
            let generics = self.generics();
            ExternDecl::Type { name, generics }
        } else {
            self.expected("`fn` or `type`");
            self.skip_line();
            return ItemKind::Error;
        };
        self.end_line(None);
        ItemKind::Extern(decl)
    }

    fn test_decl(&mut self) -> ItemKind {
        self.bump(); // test
        let name = if self.peek() == TokenKind::Str {
            let t = self.bump();
            self.str_lit(t, 0)
        } else {
            self.expected("a test name string");
            StrLit {
                parts: Vec::new(),
                triple: false,
                span: self.cur_span(),
            }
        };
        let body = self.block();
        ItemKind::Test(TestDecl { name, body })
    }

    // ----- blocks and statements ----------------------------------------------------------

    /// `block = ":" ( NEWLINE INDENT stmt { stmt } DEDENT | simple_line NEWLINE )`
    pub(crate) fn block(&mut self) -> Block {
        self.block_owned(None).0
    }

    /// [`Self::block`], also accepting `{ … }` (K0905). `owner`: a diagnostic already
    /// reported for this construct, which takes the brace edits instead of a new K0905.
    /// Returns the block and the diagnostic that owns its braces, if any.
    pub(crate) fn block_owned(&mut self, owner: Option<usize>) -> (Block, Option<usize>) {
        if self.at_op(Op::LBrace) {
            return self.brace_block(owner);
        }
        let m = self.start_node(SyntaxKind::Block);
        let start = self.start();
        let b = if !self.at_op(Op::Colon) {
            self.expected("`:`");
            self.skip_line();
            Block {
                stmts: Vec::new(),
                inline: false,
                span: self.span_from(start),
            }
        } else {
            self.bump();
            if self.peek() == TokenKind::Newline {
                self.bump();
                if self.peek() == TokenKind::Indent {
                    if !self.enter() {
                        self.leave();
                        self.skip_block_tokens();
                        Block {
                            stmts: Vec::new(),
                            inline: false,
                            span: self.span_from(start),
                        }
                    } else {
                        self.bump();
                        let stmts = self.stmt_list();
                        self.leave();
                        Block {
                            stmts,
                            inline: false,
                            span: self.span_from(start),
                        }
                    }
                } else {
                    let span = self.cur_span();
                    self.error_at(codes::EXPECTED_BLOCK, span);
                    Block {
                        stmts: Vec::new(),
                        inline: false,
                        span: self.span_from(start),
                    }
                }
            } else if matches!(self.peek(), TokenKind::Dedent | TokenKind::Eof) {
                let span = self.cur_span();
                self.error_at(codes::EXPECTED_BLOCK, span);
                Block {
                    stmts: Vec::new(),
                    inline: false,
                    span: self.span_from(start),
                }
            } else {
                let stmt = self.line_stmt();
                Block {
                    stmts: vec![stmt],
                    inline: true,
                    span: self.span_from(start),
                }
            }
        };
        self.finish_node(m);
        (b, None)
    }

    /// Statements up to and including the block's DEDENT.
    fn stmt_list(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        let mut extra_indents = 0usize;
        loop {
            let before = self.pos;
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Dedent => {
                    self.bump();
                    if extra_indents == 0 {
                        break;
                    }
                    extra_indents -= 1;
                }
                TokenKind::Newline => {
                    self.bump();
                }
                TokenKind::Indent => {
                    let span = self.cur_span();
                    self.error_at(codes::UNEXPECTED_INDENT, span);
                    self.bump();
                    extra_indents += 1;
                }
                _ => stmts.push(self.stmt()),
            }
            if self.pos == before && !self.at_eof() {
                let m = self.start_node(SyntaxKind::Error);
                self.bump();
                self.finish_node(m);
            }
        }
        stmts
    }

    /// `stmt` (spec §8.2), including `binding_head (if_form | match_form)`.
    pub(crate) fn stmt(&mut self) -> Stmt {
        let start = self.start();
        let mark = self.diags.len();
        self.line_done = false;
        if self.peek() == TokenKind::Ident {
            let m = self.start_node(SyntaxKind::Stmt);
            let found = self.stmt_mistake();
            self.finish_node(m);
            if let Some(kind) = found {
                return Stmt {
                    kind,
                    span: self.span_from(start),
                };
            }
        }
        let kind = match self.peek() {
            TokenKind::Keyword(Concept::KwIf) => {
                let m = self.start_node(SyntaxKind::IfForm);
                let (e, needs_newline) = self.if_construct(false);
                if needs_newline {
                    self.end_line(Some(mark));
                }
                self.finish_node(m);
                StmtKind::Expr(e)
            }
            TokenKind::Keyword(Concept::KwMatch) => StmtKind::Expr(self.match_form()),
            TokenKind::Keyword(Concept::KwFor) => {
                let m = self.start_node(SyntaxKind::ForStmt);
                let for_tok = self.bump();
                let kind = if self.at_c_for() {
                    self.c_for(for_tok)
                } else {
                    let pat = self.pattern();
                    self.expect_kw(Concept::KwIn, "`in`");
                    let iter = self.expr();
                    self.check_range_call(&iter);
                    let body = self.block();
                    StmtKind::For { pat, iter, body }
                };
                self.finish_node(m);
                kind
            }
            TokenKind::Keyword(Concept::KwWhile) => {
                let m = self.start_node(SyntaxKind::WhileStmt);
                self.bump();
                let cond = self.or_expr();
                let body = self.block();
                self.finish_node(m);
                StmtKind::While { cond, body }
            }
            TokenKind::Keyword(Concept::KwFn) if self.nth(1) != TokenKind::Op(Op::LParen) => {
                let m = self.start_node(SyntaxKind::FnDecl);
                let f = self.fn_decl(false);
                self.finish_node(m);
                StmtKind::Fn(f)
            }
            _ => {
                let m = self.start_node(SyntaxKind::Stmt);
                let (kind, needs_newline) = self.simple_line(true);
                if needs_newline {
                    self.end_line(Some(mark));
                }
                self.finish_node(m);
                kind
            }
        };
        Stmt {
            kind,
            span: self.span_from(start),
        }
    }

    /// The single statement of a one-line block: `simple_line NEWLINE`.
    fn line_stmt(&mut self) -> Stmt {
        let m = self.start_node(SyntaxKind::Stmt);
        let start = self.start();
        let mark = self.diags.len();
        let (kind, _) = self.simple_line(false);
        let span = self.span_from(start);
        self.end_line(Some(mark));
        self.finish_node(m);
        Stmt { kind, span }
    }

    /// `simple_line`, plus (when `forms`) a multi-line `if`/`match` value form after a
    /// binding head. Returns the statement and whether it still needs its NEWLINE.
    fn simple_line(&mut self, forms: bool) -> (StmtKind, bool) {
        match self.peek() {
            TokenKind::Keyword(Concept::KwBreak) => {
                self.bump();
                (StmtKind::Break, true)
            }
            TokenKind::Keyword(Concept::KwContinue) => {
                self.bump();
                (StmtKind::Continue, true)
            }
            TokenKind::Keyword(Concept::KwPass) => {
                self.bump();
                (StmtKind::Pass, true)
            }
            TokenKind::Keyword(Concept::KwReturn) => {
                self.bump();
                if self.at_line_end() {
                    (StmtKind::Return(None), true)
                } else {
                    let (e, nl) = self.rhs(forms);
                    (StmtKind::Return(Some(e)), nl)
                }
            }
            TokenKind::Keyword(Concept::KwLet) => {
                let let_tok = self.bump();
                if self.at_let_mut() {
                    // `let mut x` (K0912): parsed as `var x`.
                    self.let_mut(let_tok);
                    self.var_rest(forms, let_tok.span.start)
                } else {
                    self.let_rest(forms, let_tok.span.start)
                }
            }
            TokenKind::Keyword(Concept::KwVar) => {
                let var_tok = self.bump();
                self.var_rest(forms, var_tok.span.start)
            }
            TokenKind::Ident if self.at_const_stmt() => {
                // `const x = …` (K0912): parsed as `let x = …`.
                let t = self.cur();
                self.const_word();
                self.bump();
                self.let_rest(forms, t.span.start)
            }
            TokenKind::Ident if self.at_throw() => self.throw_stmt(),
            TokenKind::Op(Op::Plus | Op::Minus) if self.at_prefix_incr() => self.prefix_incr(),
            _ => {
                self.pending_incr = None;
                let e = self.expr();
                self.finish_postfix_incr(&e);
                let op = match self.peek() {
                    TokenKind::Op(Op::Eq) => Some(AssignOp::Assign),
                    TokenKind::Op(Op::PlusEq) => Some(AssignOp::Add),
                    TokenKind::Op(Op::MinusEq) => Some(AssignOp::Sub),
                    TokenKind::Op(Op::StarEq) => Some(AssignOp::Mul),
                    TokenKind::Op(Op::SlashEq) => Some(AssignOp::Div),
                    _ => None,
                };
                match op {
                    None => (StmtKind::Expr(e), true),
                    Some(op) => {
                        if !is_place(&e) && !matches!(e.kind, ExprKind::Error) {
                            self.diags.push(kalam_diagnostics::Diagnostic::new(
                                codes::NOT_A_PLACE,
                                self.file,
                                e.span,
                            ));
                        }
                        self.bump();
                        let (value, nl) = self.rhs(forms);
                        (
                            StmtKind::Assign {
                                place: e,
                                op,
                                value,
                            },
                            nl,
                        )
                    }
                }
            }
        }
    }

    /// `pattern [":" type] "=" rhs` after `let`; `binding_start` is the `let` (or the
    /// reported `const`).
    fn let_rest(&mut self, forms: bool, binding_start: u32) -> (StmtKind, bool) {
        self.block_lambda = None;
        let pat = self.pattern();
        let ty = if self.eat_op(Op::Colon) {
            Some(self.ty())
        } else {
            None
        };
        let (value, nl) = if self.expect_op(Op::Eq, "`=`") {
            self.rhs(forms)
        } else {
            (self.error_expr(), true)
        };
        self.hoist_block_lambda(binding_start, &pat, ty.is_some());
        (StmtKind::Let { pat, ty, value }, nl)
    }

    /// `NAME [":" type] "=" rhs` after `var` (or a reported `let mut`).
    fn var_rest(&mut self, forms: bool, binding_start: u32) -> (StmtKind, bool) {
        self.block_lambda = None;
        let name = self.ident("a variable name");
        let ty = if self.eat_op(Op::Colon) {
            Some(self.ty())
        } else {
            None
        };
        let (value, nl) = if self.expect_op(Op::Eq, "`=`") {
            self.rhs(forms)
        } else {
            (self.error_expr(), true)
        };
        let pat = Pat {
            kind: PatKind::Bind(name.clone()),
            span: name.span,
        };
        self.hoist_block_lambda(binding_start, &pat, ty.is_some());
        (StmtKind::Var { name, ty, value }, nl)
    }

    /// The right-hand side after a binding head: an expression, or (statement position
    /// only) a multi-line `if`/`match` value form (§8.3 rule 1, §10.3).
    fn rhs(&mut self, forms: bool) -> (Expr, bool) {
        if forms && self.at_kw(Concept::KwIf) {
            self.if_construct(true)
        } else if forms && self.at_kw(Concept::KwMatch) {
            (self.match_form(), false)
        } else {
            (self.expr(), true)
        }
    }

    /// `if` at statement start (`value == false`) or in rhs position (`value == true`),
    /// §8.3 rule 1. Returns the expression and whether a NEWLINE must still end the line
    /// (true only for `if_inline`).
    pub(crate) fn if_construct(&mut self, value: bool) -> (Expr, bool) {
        let start = self.start();
        let if_span = self.cur_span();
        self.bump(); // if
        let cond = self.or_expr();
        if !self.at_op(Op::Colon) && !self.at_op(Op::LBrace) {
            self.expected("`:`");
            self.skip_line();
            return (
                Expr {
                    kind: ExprKind::Error,
                    span: self.span_from(start),
                },
                false,
            );
        }
        let mut branches = Vec::new();
        let mut else_body = None;
        // The diagnostic owning this chain's `{ … }` bodies (K0905), so one chain written
        // with braces is one finding.
        let mut owner = None;
        if self.at_op(Op::LBrace) || self.nth(1) == TokenKind::Newline {
            let (body, o) = self.block_owned(None);
            owner = o;
            branches.push(IfBranch { cond, body });
        } else {
            // One token decides (§8.4 point 1): read a simple_line, then `else` or not.
            let block_start = self.start();
            self.bump(); // :
            let line_start = self.start();
            let mark = self.diags.len();
            let (kind, nl) = self.simple_line(false);
            let line_span = self.span_from(line_start);
            let kind = match kind {
                StmtKind::Expr(then) if self.at_kw(Concept::KwElse) => {
                    let body = self.inline_arm(then);
                    branches.push(IfBranch { cond, body });
                    let else_body = self.inline_else_chain(&mut branches, if_span);
                    return (
                        Expr {
                            kind: ExprKind::If(IfExpr {
                                branches,
                                else_body,
                                inline: true,
                            }),
                            span: self.span_from(start),
                        },
                        true,
                    );
                }
                other => other,
            };
            if nl {
                self.end_line(Some(mark));
            }
            let body = Block {
                stmts: vec![Stmt {
                    kind,
                    span: line_span,
                }],
                inline: true,
                span: self.span_from(block_start),
            };
            branches.push(IfBranch { cond, body });
        }
        // Block-form `else if` / `else` clauses on the following lines (and `elif`, K0903).
        loop {
            if owner.is_some()
                && self.peek() == TokenKind::Newline
                && (self.nth(1) == TokenKind::Keyword(Concept::KwElse)
                    || self.nth_plain(1) == Some("elif"))
            {
                // `}` then `else` on the next line.
                self.bump();
            }
            if self.at_elif() {
                self.elif_word();
                let cond = self.or_expr();
                let (body, o) = self.block_owned(owner);
                owner = owner.or(o);
                branches.push(IfBranch { cond, body });
                continue;
            }
            if !self.at_kw(Concept::KwElse) {
                break;
            }
            let else_span = self.bump().span;
            if self.eat_kw(Concept::KwIf) {
                let cond = self.or_expr();
                let chain = if self.at_op(Op::LBrace) && owner.is_none() {
                    // `else if (c) {` (K0903).
                    Some(self.else_if_braces(else_span, &cond))
                } else {
                    owner
                };
                let (body, o) = self.block_owned(chain);
                owner = owner.or(o);
                branches.push(IfBranch { cond, body });
            } else {
                else_body = Some(self.block_owned(owner).0);
                break;
            }
        }
        if value && else_body.is_none() {
            self.diags.push(kalam_diagnostics::Diagnostic::new(
                codes::IF_WITHOUT_ELSE,
                self.file,
                if_span,
            ));
        }
        (
            Expr {
                kind: ExprKind::If(IfExpr {
                    branches,
                    else_body,
                    inline: false,
                }),
                span: self.span_from(start),
            },
            false,
        )
    }

    /// `match_form = "match" expr ":" NEWLINE INDENT case_arm { case_arm } DEDENT`
    pub(crate) fn match_form(&mut self) -> Expr {
        let m = self.start_node(SyntaxKind::MatchForm);
        let start = self.start();
        self.bump(); // match
        let scrutinee = Box::new(self.expr());
        let arms = if self.at_op(Op::LBrace) {
            self.brace_match(&scrutinee, start);
            Vec::new()
        } else if self.open_body() {
            // One K0925 for every arm of this `match` written with `=>`/`->`.
            let mut owner = None;
            self.body_members(|p| {
                if p.at_kw(Concept::KwCase) {
                    Some(p.case_arm(true, &mut owner))
                } else if p.at_bare_arm() {
                    Some(p.case_arm(false, &mut owner))
                } else {
                    p.expected("`case`");
                    p.skip_line();
                    None
                }
            })
        } else {
            Vec::new()
        };
        self.finish_node(m);
        Expr {
            kind: ExprKind::Match(MatchExpr { scrutinee, arms }),
            span: self.span_from(start),
        }
    }

    /// `case pattern [if guard] block`; `has_case == false` for an arm written without
    /// `case` (`P => e`, K0925), which is reported here.
    pub(crate) fn case_arm(&mut self, has_case: bool, owner: &mut Option<usize>) -> CaseArm {
        let m = self.start_node(SyntaxKind::CaseArm);
        let start = self.start();
        if has_case {
            self.bump(); // case
        } else {
            self.bare_arm(owner);
        }
        let pat = self.pattern();
        let guard = if self.eat_kw(Concept::KwIf) {
            Some(self.or_expr())
        } else {
            None
        };
        self.arm_arrow(owner);
        let body = self.block();
        self.finish_node(m);
        CaseArm {
            pat,
            guard,
            body,
            span: self.span_from(start),
        }
    }

    /// Ends a logical line. If something else is there, reports it (unless an error was
    /// already reported since `mark`) and skips the rest of the line.
    pub(crate) fn end_line(&mut self, mark: Option<usize>) {
        if std::mem::take(&mut self.line_done) || self.brace_depth > 0 {
            return;
        }
        match self.peek() {
            TokenKind::Newline => {
                self.bump();
            }
            TokenKind::Dedent | TokenKind::Eof => {}
            _ => {
                if mark.map_or(true, |m| self.diags.len() == m) {
                    self.expected("end of line");
                }
                self.skip_line();
            }
        }
    }

    // ----- small helpers ------------------------------------------------------------------

    /// A word token: identifier, keyword or core name (§5.3 rule 8 positions).
    pub(crate) fn at_word(&self) -> bool {
        self.nth_is_word(0)
    }

    pub(crate) fn nth_is_word(&self, n: usize) -> bool {
        matches!(
            self.nth(n),
            TokenKind::Ident | TokenKind::Keyword(_) | TokenKind::Core(_)
        )
    }

    /// An identifier, or an error identifier (empty name) after reporting.
    pub(crate) fn ident(&mut self, what: &str) -> Ident {
        if self.peek() == TokenKind::Ident {
            let t = self.bump();
            self.word_ident(t)
        } else {
            self.expected(what);
            Ident {
                name: String::new(),
                span: self.cur_span(),
            }
        }
    }

    pub(crate) fn error_expr(&self) -> Expr {
        Expr {
            kind: ExprKind::Error,
            span: self.cur_span(),
        }
    }

    /// A comma-separated list closed by `close`, after the opening bracket (`open`).
    /// Handles trailing commas, reports K0251 for an unclosed list and K0250 for junk.
    pub(crate) fn delimited<T>(
        &mut self,
        close: Op,
        open: kalam_base::Span,
        close_text: &str,
        mut item: impl FnMut(&mut Self) -> Option<T>,
    ) -> Vec<T> {
        let mut out = Vec::new();
        loop {
            if self.eat_op(close) {
                break;
            }
            if crate::is_layout(self.peek()) {
                let span = mk_span(open.start, open.end);
                self.error_at(codes::UNCLOSED_DELIMITER, span);
                break;
            }
            let before = self.pos;
            if let Some(x) = item(self) {
                out.push(x);
            }
            if self.eat_op(Op::Comma) {
                continue;
            }
            if self.eat_op(close) {
                break;
            }
            if crate::is_layout(self.peek()) {
                let span = mk_span(open.start, open.end);
                self.error_at(codes::UNCLOSED_DELIMITER, span);
                break;
            }
            self.expected(&format!("`,` or {close_text}"));
            self.skip_in_brackets(close);
            if self.pos == before && !crate::is_layout(self.peek()) && !self.at_op(close) {
                self.bump();
            }
            if self.eat_op(Op::Comma) {
                continue;
            }
        }
        out
    }

    /// Skips tokens (balancing nested brackets) up to `close`, a `,`, or a layout token.
    fn skip_in_brackets(&mut self, close: Op) {
        let m = self.start_node(SyntaxKind::Error);
        let mut depth = 0usize;
        loop {
            match self.peek() {
                k if crate::is_layout(k) => break,
                TokenKind::Op(Op::LParen)
                | TokenKind::Op(Op::LBracket)
                | TokenKind::Op(Op::LBrace) => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::Op(Op::RParen)
                | TokenKind::Op(Op::RBracket)
                | TokenKind::Op(Op::RBrace) => {
                    if depth == 0 {
                        if self.at_op(close) {
                            break;
                        }
                        self.bump();
                    } else {
                        depth -= 1;
                        self.bump();
                    }
                }
                TokenKind::Op(Op::Comma) if depth == 0 => break,
                _ => {
                    self.bump();
                }
            }
        }
        self.finish_node(m);
    }
}

/// `place = IDENT { "." member | "[" expr "]" }` (spec §8.2; the root must be a `var`,
/// which name resolution checks).
pub(crate) fn is_place(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Name(NameKind::Ident(_)) => true,
        ExprKind::Member { base, .. } | ExprKind::Index { base, .. } => is_place(base),
        _ => false,
    }
}
