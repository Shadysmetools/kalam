//! The formatter: prints the AST in one canonical layout (spec §21.1), re-attaching comments
//! and blank lines from the source.
//!
//! Layout rules:
//! - 4-space indentation; one statement per line; 100 columns, measured on the English-skin
//!   spelling so every skin gets the same line breaks ([`crate::doc`]).
//! - A line that does not fit breaks at the outermost group first: a method chain of two or
//!   more calls breaks into leading-dot continuation lines (spec §5.8 rule 2); a bracket
//!   breaks one item per line with a trailing comma.
//! - A one-line block (`if x: return y`) stays on one line only if it fits; otherwise it is
//!   indented. A multi-line block is never joined.
//! - Spaces: around binary operators, `=`, `->`, `=>` and after `,` and `:`; none inside
//!   brackets, around `.`, `..`, `..=`, or before `?` and `(`.
//! - Items are separated by exactly one blank line, except between two consecutive one-line
//!   `let` constants or `extern` declarations, where the source's blank lines are kept
//!   (at most one). Elsewhere blank lines are kept, at most one, and never at the start of a
//!   block or before `else`.
//!
//! Comments: an own-line comment is printed on its own line at the indentation of the code
//! that follows it (or, after a block's last statement, at the block's indentation if it was
//! indented at least that deep). A comment at the end of a line stays at the end of that
//! line, after two spaces. A simple statement that has a comment *inside* it (for example
//! between the items of a broken list) is printed as written, re-spelled for the skin. Any
//! other comment position makes formatting fail, and [`crate::format`] then falls back to the
//! layout-preserving re-spelling.

use kalam_ast::*;
use kalam_base::Skin;
use kalam_skins::{Concept, KeywordTable};

use crate::convert::Rewriter;
use crate::doc::{self, concat, flat_text, flat_width, text, Doc, INDENT, WIDTH};
use crate::roles::{Role, Roles};
use crate::source::Source;
use crate::{convert_digits, is_number, to_ascii_digits, FmtConfig};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sep {
    /// No blank line before the next line.
    None,
    /// A blank line if the source had one.
    Preserve,
    /// Exactly one blank line.
    Force,
}

/// Expression context: may a method chain here break into leading-dot continuation lines?
#[derive(Clone, Copy)]
struct Ctx {
    chain: bool,
}

const TAIL: Ctx = Ctx { chain: true };
const INNER: Ctx = Ctx { chain: false };

pub(crate) struct Printer<'s, 'a> {
    src: &'s Source<'a>,
    roles: &'s Roles,
    skin: Skin,
    config: &'s FmtConfig,
    table: &'static KeywordTable,
    pub out: String,
    consumed: Vec<bool>,
    last_line: Option<usize>,
    floor: u32,
    sep: Sep,
    pub failed: Option<String>,
}

impl<'s, 'a> Printer<'s, 'a> {
    pub fn new(src: &'s Source<'a>, roles: &'s Roles, config: &'s FmtConfig) -> Self {
        Printer {
            src,
            roles,
            skin: src.skin,
            config,
            table: KeywordTable::builtin(),
            out: String::with_capacity(src.text.len() + 64),
            consumed: vec![false; src.comments.len()],
            last_line: None,
            floor: 0,
            sep: Sep::None,
            failed: None,
        }
    }

    /// Comments that were not placed (each one makes the formatting invalid).
    pub fn unplaced(&self) -> Vec<String> {
        self.src
            .comments
            .iter()
            .zip(&self.consumed)
            .filter(|(_, done)| !**done)
            .map(|(c, _)| format!("line {}: {}", c.line + 1, c.text))
            .collect()
    }

    fn fail(&mut self, why: &str) -> Doc {
        if self.failed.is_none() {
            self.failed = Some(why.to_string());
        }
        text("")
    }

    // ----- lines, comments and blank lines ------------------------------------------------

    fn blank_before(&mut self, line: usize) {
        let blank = !self.out.is_empty()
            && match self.sep {
                Sep::None => false,
                Sep::Force => true,
                Sep::Preserve => self
                    .last_line
                    .is_some_and(|last| self.src.blank_between(last, line)),
            };
        if blank {
            self.out.push('\n');
        }
        self.sep = Sep::Preserve;
    }

    fn indent(&mut self, ind: usize) {
        self.out.extend(std::iter::repeat(' ').take(ind));
    }

    /// Emits one line: `doc` at `ind`, then the comment that ended the source line whose
    /// last token ends at `key`, if any.
    fn emit_line(&mut self, ind: usize, doc: &Doc, src_start: u32, src_end: u32, key: Option<u32>) {
        self.blank_before(self.src.line_of(src_start));
        self.indent(ind);
        doc::layout(doc, ind, &mut self.out);
        self.finish_line(src_end, key);
    }

    fn finish_line(&mut self, src_end: u32, key: Option<u32>) {
        self.last_line = Some(self.src.line_of(src_end));
        self.floor = self.floor.max(src_end);
        if let Some(i) = key.and_then(|k| self.src.trailing.get(&k).copied()) {
            if !self.consumed[i] {
                self.consumed[i] = true;
                let c = &self.src.comments[i];
                self.out.push_str("  ");
                self.out.push_str(&c.text);
                self.last_line = Some(c.line);
            }
        }
        self.out.push('\n');
    }

    fn emit_comment(&mut self, i: usize, ind: usize) {
        let line = self.src.comments[i].line;
        self.blank_before(line);
        self.indent(ind);
        self.out.push_str(&self.src.comments[i].text);
        self.out.push('\n');
        self.consumed[i] = true;
        self.last_line = Some(line);
    }

    /// Own-line comments between the last printed code and `upto`, at indentation `ind`.
    fn gap(&mut self, upto: u32, ind: usize) {
        for i in 0..self.src.comments.len() {
            let c = &self.src.comments[i];
            if c.start >= upto {
                break;
            }
            if !self.consumed[i] && c.own_line && c.start >= self.floor {
                self.emit_comment(i, ind);
            }
        }
    }

    /// After a multi-line block's last statement: the own-line comments that follow it and
    /// are indented at least as deep as the block belong to the block.
    fn block_tail(&mut self, ind: usize) {
        let limit = self
            .src
            .next_token(self.floor)
            .map(|t| t.span.start)
            .unwrap_or(u32::MAX);
        for i in 0..self.src.comments.len() {
            let c = &self.src.comments[i];
            if c.start >= limit {
                break;
            }
            if self.consumed[i] || !c.own_line || c.start < self.floor {
                continue;
            }
            if c.col < ind {
                break;
            }
            self.emit_comment(i, ind);
        }
    }

    fn has_comment_inside(&self, span: kalam_base::Span) -> bool {
        self.src
            .comments
            .iter()
            .any(|c| c.start > span.start && c.start < span.end)
    }

    /// Prints `span` as written (re-spelled for the skin): a statement with a comment inside.
    fn verbatim(&mut self, ind: usize, span: kalam_base::Span) {
        self.blank_before(self.src.line_of(span.start));
        self.indent(ind);
        self.verbatim_rest(span);
    }

    /// Writes `span` as written (re-spelled) after what is already on the line, and ends it.
    fn verbatim_rest(&mut self, span: kalam_base::Span) {
        let rw = Rewriter {
            from: self.skin,
            to: self.skin,
            config: self.config,
            table: self.table,
            roles: self.roles,
        };
        rw.rewrite(
            self.src.text,
            &self.src.tokens,
            0,
            span.range(),
            &mut self.out,
        );
        for (i, c) in self.src.comments.iter().enumerate() {
            if c.start > span.start && c.start < span.end {
                self.consumed[i] = true;
            }
        }
        self.finish_line(span.end, Some(span.end));
    }

    // ----- spellings ----------------------------------------------------------------------

    fn kw(&self, c: Concept) -> Doc {
        let w = self.table.spelling(c, Skin::En).chars().count();
        Doc::Text(self.table.spelling(c, self.skin).to_string(), w)
    }

    fn kw_sp(&self, c: Concept) -> Doc {
        concat(vec![self.kw(c), text(" ")])
    }

    fn id(&self, name: &str, role: Role) -> Doc {
        let w = self
            .roles
            .spell(name, role, Skin::En, self.table)
            .chars()
            .count();
        Doc::Text(self.roles.spell(name, role, self.skin, self.table), w)
    }

    fn name_kind(&self, n: &NameKind, role: Role) -> Doc {
        match n {
            NameKind::Ident(s) => self.id(s, role),
            NameKind::Core(c) => self.kw(*c),
        }
    }

    fn comma(&self) -> Doc {
        Doc::Text(
            self.table
                .spelling(Concept::PunctComma, self.skin)
                .to_string(),
            1,
        )
    }

    fn digits(&self, ascii: &str) -> Doc {
        let w = ascii.chars().count();
        Doc::Text(convert_digits(ascii, self.skin, self.config), w)
    }

    /// A numeric literal expression. A run that is not a valid number is copied from the
    /// source as written (a guard: the lexer reports such runs, in interpolation holes too,
    /// so an accepted file should not contain one; see [`convert_digits`]).
    fn number(&self, ascii: &str, span: kalam_base::Span) -> Doc {
        if is_number(ascii) {
            return self.digits(ascii);
        }
        let raw = &self.src.text[span.range()];
        Doc::Text(raw.to_string(), raw.chars().count())
    }

    /// `open item, item close`, breaking one item per line with a trailing comma.
    fn list(&self, open: &str, items: Vec<Doc>, close: &str) -> Doc {
        self.list_with(open, items, close, false)
    }

    fn list_with(&self, open: &str, items: Vec<Doc>, close: &str, one_tuple: bool) -> Doc {
        if items.is_empty() {
            return text(format!("{open}{close}"));
        }
        let mut inner = vec![Doc::Soft];
        for (i, it) in items.into_iter().enumerate() {
            if i > 0 {
                inner.push(self.comma());
                inner.push(Doc::Line);
            }
            inner.push(it);
        }
        if one_tuple {
            inner.push(self.comma());
        } else {
            inner.push(Doc::BreakOnly(
                self.table
                    .spelling(Concept::PunctComma, self.skin)
                    .to_string(),
            ));
        }
        Doc::Group(vec![text(open), Doc::Indent(inner), Doc::Soft, text(close)])
    }

    // ----- file and items -----------------------------------------------------------------

    pub fn file(&mut self) {
        let file = &self.src.ast.file;
        self.sep = Sep::Preserve;
        for u in &file.uses {
            self.gap(u.span.start, 0);
            let doc = self.use_decl(u);
            self.emit_line(0, &doc, u.span.start, u.span.end, Some(u.span.end));
        }
        let mut prev: Option<&Item> = None;
        for it in &file.items {
            let grouped = |i: &Item| matches!(i.kind, ItemKind::Const(_) | ItemKind::Extern(_));
            self.sep = match prev {
                Some(p) if grouped(p) && grouped(it) => Sep::Preserve,
                None if file.uses.is_empty() => Sep::Preserve,
                _ => Sep::Force,
            };
            self.gap(it.span.start, 0);
            self.item(it, 0);
            prev = Some(it);
        }
        self.sep = Sep::Preserve;
        self.gap(u32::MAX, 0);
    }

    fn use_decl(&self, u: &UseDecl) -> Doc {
        let mut parts = Vec::new();
        if u.export {
            parts.push(self.kw_sp(Concept::KwExport));
        }
        parts.push(self.kw_sp(Concept::KwUse));
        for (i, p) in u.path.iter().enumerate() {
            if i > 0 {
                parts.push(text("/"));
            }
            parts.push(self.id(&p.name, Role::Free));
        }
        match &u.tail {
            UseTail::None => {}
            UseTail::Alias(a) => {
                parts.push(text(" "));
                parts.push(self.kw_sp(Concept::KwAs));
                parts.push(self.id(&a.name, Role::Bound));
            }
            UseTail::Items(items) => {
                parts.push(text("."));
                let docs = items
                    .iter()
                    .map(|it| match it {
                        UseItem::SelfModule(_) => self.kw(Concept::KwSelf),
                        UseItem::Name { name, alias } => {
                            let mut d = vec![self.id(&name.name, Role::Bound)];
                            if let Some(a) = alias {
                                d.push(text(" "));
                                d.push(self.kw_sp(Concept::KwAs));
                                d.push(self.id(&a.name, Role::Bound));
                            }
                            concat(d)
                        }
                    })
                    .collect();
                parts.push(self.list("{", docs, "}"));
            }
        }
        concat(parts)
    }

    fn attribute(&mut self, a: &Attribute) -> Doc {
        let mut parts = vec![text("@")];
        for (i, p) in a.path.iter().enumerate() {
            if i > 0 {
                parts.push(text("."));
            }
            parts.push(self.id(&p.name, Role::Free));
        }
        if let Some(args) = &a.args {
            parts.push(self.args(args));
        }
        concat(parts)
    }

    /// Attribute lines (one per line). Returns the start of the code after them.
    fn attr_lines(&mut self, attrs: &[Attribute], ind: usize, start: u32) -> u32 {
        for (i, a) in attrs.iter().enumerate() {
            if i > 0 {
                self.sep = Sep::None;
                self.gap(a.span.start, ind);
            }
            let doc = self.attribute(a);
            self.emit_line(ind, &doc, a.span.start, a.span.end, Some(a.span.end));
        }
        match attrs.last() {
            None => start,
            Some(last) => {
                let next = self
                    .src
                    .next_token(last.span.end)
                    .map(|t| t.span.start)
                    .unwrap_or(last.span.end);
                self.sep = Sep::None;
                self.gap(next, ind);
                next
            }
        }
    }

    fn vis(&self, v: Visibility) -> Doc {
        match v {
            Visibility::Private => text(""),
            Visibility::Export => self.kw_sp(Concept::KwExport),
            Visibility::ExportOpaque => concat(vec![
                self.kw_sp(Concept::KwExport),
                self.kw_sp(Concept::KwOpaque),
            ]),
        }
    }

    fn generics(&mut self, gs: &[GenericParam]) -> Doc {
        if gs.is_empty() {
            return text("");
        }
        let docs = gs
            .iter()
            .map(|g| {
                let mut d = vec![self.id(&g.name.name, Role::Bound)];
                for (i, b) in g.bounds.iter().enumerate() {
                    d.push(text(if i == 0 { ": " } else { " + " }));
                    d.push(self.ty(b));
                }
                concat(d)
            })
            .collect();
        self.list("[", docs, "]")
    }

    fn item(&mut self, it: &Item, ind: usize) {
        let start = self.attr_lines(&it.attrs, ind, it.span.start);
        let vis = self.vis(it.vis);
        match &it.kind {
            ItemKind::Fn(f) => self.fn_decl(ind, vis, f, start),
            ItemKind::Type(t) => {
                let mut head = vec![vis, self.kw_sp(Concept::KwType)];
                head.push(self.name_kind(&t.name.kind, Role::Bound));
                head.push(self.generics(&t.generics));
                match &t.body {
                    TypeBody::Alias(ty) => {
                        head.push(text(" = "));
                        head.push(self.ty(ty));
                        let end = it.span.end;
                        self.emit_line(ind, &concat(head), start, end, Some(end));
                    }
                    TypeBody::Record(fields) => {
                        self.body_header(ind, head, start);
                        self.sep = Sep::None;
                        for f in fields {
                            self.gap(f.span.start, ind + INDENT);
                            let s = self.attr_lines(&f.attrs, ind + INDENT, f.span.start);
                            let mut d = vec![self.id(&f.name.name, Role::Free), text(": ")];
                            d.push(self.ty(&f.ty));
                            if let Some(e) = &f.default {
                                d.push(text(" = "));
                                d.push(self.expr(e, TAIL));
                            }
                            self.emit_line(
                                ind + INDENT,
                                &concat(d),
                                s,
                                f.span.end,
                                Some(f.span.end),
                            );
                        }
                        self.block_tail(ind + INDENT);
                    }
                }
            }
            ItemKind::Enum(e) => {
                let mut head = vec![vis, self.kw_sp(Concept::KwEnum)];
                head.push(self.name_kind(&e.name.kind, Role::Bound));
                head.push(self.generics(&e.generics));
                self.body_header(ind, head, start);
                self.sep = Sep::None;
                for v in &e.variants {
                    self.gap(v.span.start, ind + INDENT);
                    let s = self.attr_lines(&v.attrs, ind + INDENT, v.span.start);
                    let mut d = vec![self.id(&v.name.name, Role::Bound)];
                    if let Some(fields) = &v.fields {
                        let docs = fields
                            .iter()
                            .map(|f| {
                                let mut fd = Vec::new();
                                if let Some(l) = &f.label {
                                    fd.push(self.id(&l.name, Role::Free));
                                    fd.push(text(": "));
                                }
                                fd.push(self.ty(&f.ty));
                                concat(fd)
                            })
                            .collect();
                        d.push(self.list("(", docs, ")"));
                    }
                    self.emit_line(ind + INDENT, &concat(d), s, v.span.end, Some(v.span.end));
                }
                self.block_tail(ind + INDENT);
            }
            ItemKind::Trait(t) => {
                let mut head = vec![vis, self.kw_sp(Concept::KwTrait)];
                head.push(self.name_kind(&t.name.kind, Role::Bound));
                head.push(self.generics(&t.generics));
                self.body_header(ind, head, start);
                self.sep = Sep::None;
                for m in &t.members {
                    self.gap(m.span.start, ind + INDENT);
                    self.fn_decl(ind + INDENT, text(""), m, m.span.start);
                }
                self.block_tail(ind + INDENT);
            }
            ItemKind::Impl(i) => {
                let mut head = vec![vis, self.kw(Concept::KwImpl), self.generics(&i.generics)];
                head.push(text(" "));
                head.push(self.ty(&i.ty));
                if let Some(t) = &i.as_trait {
                    head.push(text(" "));
                    head.push(self.kw_sp(Concept::KwAs));
                    head.push(self.ty(t));
                }
                self.body_header(ind, head, start);
                self.sep = Sep::None;
                for m in &i.members {
                    self.gap(m.span.start, ind + INDENT);
                    let s = self.attr_lines(&m.attrs, ind + INDENT, m.span.start);
                    let prefix = if m.export {
                        self.kw_sp(Concept::KwExport)
                    } else {
                        text("")
                    };
                    self.fn_decl(ind + INDENT, prefix, &m.func, s);
                }
                self.block_tail(ind + INDENT);
            }
            ItemKind::Const(c) => {
                let span = kalam_base::Span::new(start, it.span.end);
                if self.has_comment_inside(span) {
                    self.verbatim(ind, span);
                    return;
                }
                let mut d = vec![vis, self.kw_sp(Concept::KwLet)];
                d.push(self.name_kind(&c.name.kind, Role::Bound));
                if let Some(t) = &c.ty {
                    d.push(text(": "));
                    d.push(self.ty(t));
                }
                d.push(text(" = "));
                d.push(self.expr(&c.value, TAIL));
                self.emit_line(ind, &concat(d), start, it.span.end, Some(it.span.end));
            }
            ItemKind::Extern(x) => {
                let mut d = vec![vis, self.kw_sp(Concept::KwExtern)];
                match x {
                    ExternDecl::Fn(sig) => d.push(self.sig(sig)),
                    ExternDecl::Type { name, generics } => {
                        d.push(self.kw_sp(Concept::KwType));
                        d.push(self.name_kind(&name.kind, Role::Bound));
                        d.push(self.generics(generics));
                    }
                }
                self.emit_line(ind, &concat(d), start, it.span.end, Some(it.span.end));
            }
            ItemKind::Test(t) => {
                let head = concat(vec![
                    vis,
                    self.kw_sp(Concept::KwTest),
                    self.str_lit(&t.name, None),
                ]);
                self.header_block(ind, head, start, &t.body);
            }
            ItemKind::Error => {
                self.fail("an item did not parse");
            }
        }
    }

    /// `head:` for a type, enum, trait or impl body.
    fn body_header(&mut self, ind: usize, mut head: Vec<Doc>, start: u32) {
        head.push(text(":"));
        let colon = self.src.block_colon_end(start).unwrap_or(start);
        self.emit_line(ind, &concat(head), start, colon, Some(colon));
    }

    fn sig(&mut self, s: &FnSig) -> Doc {
        let mut d = vec![self.kw_sp(Concept::KwFn)];
        d.push(self.name_kind(&s.name.kind, Role::Bound));
        d.push(self.generics(&s.generics));
        let params = s
            .params
            .iter()
            .map(|p| match p {
                Param::SelfParam(_) => self.kw(Concept::KwSelf),
                Param::Named {
                    name, ty, default, ..
                } => {
                    let mut pd = vec![self.id(&name.name, Role::Bound), text(": "), self.ty(ty)];
                    if let Some(e) = default {
                        pd.push(text(" = "));
                        pd.push(self.expr(e, TAIL));
                    }
                    concat(pd)
                }
            })
            .collect();
        d.push(self.list("(", params, ")"));
        if let Some(r) = &s.ret {
            d.push(text(" -> "));
            d.push(self.ty(r));
        }
        concat(d)
    }

    fn fn_decl(&mut self, ind: usize, prefix: Doc, f: &FnDecl, start: u32) {
        let head = concat(vec![prefix, self.sig(&f.sig)]);
        match &f.body {
            FnBody::Block(b) => self.header_block(ind, head, start, b),
            FnBody::Expr(_)
                if self.has_comment_inside(kalam_base::Span::new(start, f.span.end)) =>
            {
                self.verbatim(ind, kalam_base::Span::new(start, f.span.end));
            }
            FnBody::Expr(e) => {
                let doc = concat(vec![head, text(" => "), self.expr(e, TAIL)]);
                self.emit_line(ind, &doc, start, f.span.end, Some(f.span.end));
            }
            FnBody::None => {
                let end = f.sig.span.end;
                self.emit_line(ind, &head, start, end, Some(end));
            }
        }
    }

    // ----- blocks and statements ----------------------------------------------------------

    /// `head: stmt` when the block was one line and still fits, otherwise `head:` and the
    /// indented statements.
    fn header_block(&mut self, ind: usize, head: Doc, start: u32, b: &Block) {
        if b.inline && b.stmts.len() == 1 && self.has_comment_inside(b.stmts[0].span) {
            // The statement keeps its layout, so it also stays on the header line: indented
            // one level deeper, its continuation lines would no longer be deeper than it.
            self.blank_before(self.src.line_of(start));
            self.indent(ind);
            doc::layout(&concat(vec![head, text(": ")]), ind, &mut self.out);
            self.verbatim_rest(b.stmts[0].span);
            return;
        }
        if b.inline && b.stmts.len() == 1 {
            let s = &b.stmts[0];
            if let Some(line) = self.simple_stmt(&s.kind) {
                let doc = concat(vec![head.clone(), text(": "), line]);
                if flat_width(&doc).is_some_and(|w| ind + w <= WIDTH) {
                    self.emit_line(ind, &doc, start, s.span.end, Some(s.span.end));
                    return;
                }
            }
        }
        let colon = b.span.start + 1;
        self.emit_line(
            ind,
            &concat(vec![head, text(":")]),
            start,
            colon,
            Some(colon),
        );
        self.stmts(&b.stmts, ind + INDENT);
    }

    fn stmts(&mut self, stmts: &[Stmt], ind: usize) {
        self.sep = Sep::None;
        for s in stmts {
            self.gap(s.span.start, ind);
            self.stmt(s, ind);
        }
        self.block_tail(ind);
    }

    fn stmt(&mut self, s: &Stmt, ind: usize) {
        let start = s.span.start;
        match &s.kind {
            StmtKind::For { pat, iter, body } => {
                let head = concat(vec![
                    self.kw_sp(Concept::KwFor),
                    self.pat(pat),
                    text(" "),
                    self.kw_sp(Concept::KwIn),
                    self.expr(iter, INNER),
                ]);
                self.header_block(ind, head, start, body);
            }
            StmtKind::While { cond, body } => {
                let head = concat(vec![self.kw_sp(Concept::KwWhile), self.expr(cond, INNER)]);
                self.header_block(ind, head, start, body);
            }
            StmtKind::Fn(f) => self.fn_decl(ind, text(""), f, start),
            StmtKind::Expr(e) if is_form(e) => self.form(ind, text(""), e, start),
            StmtKind::Let { value, .. }
            | StmtKind::Var { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Return(Some(value))
                if is_form(value) =>
            {
                let prefix = self.stmt_prefix(&s.kind);
                self.form(ind, prefix, value, start);
            }
            kind => {
                if self.has_comment_inside(s.span) {
                    self.verbatim(ind, s.span);
                    return;
                }
                let doc = match self.simple_stmt(kind) {
                    Some(d) => d,
                    None => self.fail("unexpected statement"),
                };
                self.emit_line(ind, &doc, start, s.span.end, Some(s.span.end));
            }
        }
    }

    /// `let p: T = `, `var x = `, `place += `, `return ` (before a value).
    fn stmt_prefix(&mut self, kind: &StmtKind) -> Doc {
        match kind {
            StmtKind::Let { pat, ty, .. } => {
                let mut d = vec![self.kw_sp(Concept::KwLet), self.pat(pat)];
                if let Some(t) = ty {
                    d.push(text(": "));
                    d.push(self.ty(t));
                }
                d.push(text(" = "));
                concat(d)
            }
            StmtKind::Var { name, ty, .. } => {
                let mut d = vec![self.kw_sp(Concept::KwVar), self.id(&name.name, Role::Bound)];
                if let Some(t) = ty {
                    d.push(text(": "));
                    d.push(self.ty(t));
                }
                d.push(text(" = "));
                concat(d)
            }
            StmtKind::Assign { place, op, .. } => {
                let op = match op {
                    AssignOp::Assign => " = ",
                    AssignOp::Add => " += ",
                    AssignOp::Sub => " -= ",
                    AssignOp::Mul => " *= ",
                    AssignOp::Div => " /= ",
                };
                concat(vec![self.expr(place, INNER), text(op)])
            }
            StmtKind::Return(_) => self.kw_sp(Concept::KwReturn),
            _ => text(""),
        }
    }

    /// A `simple_line` (spec §8.2) as one doc; `None` for compound statements.
    fn simple_stmt(&mut self, kind: &StmtKind) -> Option<Doc> {
        Some(match kind {
            StmtKind::Expr(e) => self.expr(e, TAIL),
            StmtKind::Let { value, .. }
            | StmtKind::Var { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Return(Some(value)) => {
                let prefix = self.stmt_prefix(kind);
                concat(vec![prefix, self.expr(value, TAIL)])
            }
            StmtKind::Return(None) => self.kw(Concept::KwReturn),
            StmtKind::Break => self.kw(Concept::KwBreak),
            StmtKind::Continue => self.kw(Concept::KwContinue),
            StmtKind::Pass => self.kw(Concept::KwPass),
            StmtKind::For { .. } | StmtKind::While { .. } | StmtKind::Fn(_) | StmtKind::Error => {
                return None
            }
        })
    }

    /// A multi-line `if` or `match` form, after `prefix` (a binding head or nothing).
    fn form(&mut self, ind: usize, prefix: Doc, e: &Expr, start: u32) {
        match &e.kind {
            ExprKind::If(ifx) => {
                for (i, br) in ifx.branches.iter().enumerate() {
                    let mut head = Vec::new();
                    let mut at = start;
                    if i == 0 {
                        head.push(prefix.clone());
                    } else {
                        at = self.else_line(ind);
                        head.push(self.kw_sp(Concept::KwElse));
                    }
                    head.push(self.kw_sp(Concept::KwIf));
                    head.push(self.expr(&br.cond, INNER));
                    self.header_block(ind, concat(head), at, &br.body);
                }
                if let Some(b) = &ifx.else_body {
                    let at = self.else_line(ind);
                    let head = self.kw(Concept::KwElse);
                    self.header_block(ind, head, at, b);
                }
            }
            ExprKind::Match(m) => {
                let head = concat(vec![
                    prefix,
                    self.kw_sp(Concept::KwMatch),
                    self.expr(&m.scrutinee, INNER),
                    text(":"),
                ]);
                let colon = self
                    .src
                    .next_token(m.scrutinee.span.end)
                    .map(|t| t.span.end)
                    .unwrap_or(m.scrutinee.span.end);
                self.emit_line(ind, &head, start, colon, Some(colon));
                let ind = ind + INDENT;
                self.sep = Sep::None;
                for arm in &m.arms {
                    self.gap(arm.span.start, ind);
                    let mut head = vec![self.kw_sp(Concept::KwCase), self.pat(&arm.pat)];
                    if let Some(g) = &arm.guard {
                        head.push(text(" "));
                        head.push(self.kw_sp(Concept::KwIf));
                        head.push(self.expr(g, INNER));
                    }
                    self.header_block(ind, concat(head), arm.span.start, &arm.body);
                }
                self.block_tail(ind);
            }
            _ => {
                self.fail("not a block form");
            }
        }
    }

    /// Before an `else` line at `ind`: comments since the previous branch, no blank lines.
    /// Returns the source offset of the `else` keyword.
    fn else_line(&mut self, ind: usize) -> u32 {
        let at = self
            .src
            .next_token(self.floor)
            .map(|t| t.span.start)
            .unwrap_or(self.floor);
        self.sep = Sep::None;
        self.gap(at, ind);
        self.sep = Sep::None;
        at
    }

    // ----- expressions --------------------------------------------------------------------

    /// Call arguments. When the last argument is a non-empty list or map literal, the call
    /// may hug it (`draw.group([` … `])`, spec §28.8) instead of breaking every argument.
    fn args(&mut self, args: &[Arg]) -> Doc {
        let mut labels = Vec::new();
        let mut values = Vec::new();
        for a in args {
            labels.push(
                a.label
                    .as_ref()
                    .map(|l| concat(vec![self.id(&l.name, Role::Free), text(": ")])),
            );
            values.push(self.expr(&a.value, TAIL));
        }
        let docs: Vec<Doc> = labels
            .iter()
            .zip(&values)
            .map(|(l, v)| match l {
                Some(l) => concat(vec![l.clone(), v.clone()]),
                None => v.clone(),
            })
            .collect();
        let full = self.list("(", docs, ")");
        let huggable = args.last().is_some_and(|a| match &a.value.kind {
            ExprKind::List(xs) => !xs.is_empty(),
            ExprKind::Map(es) => !es.is_empty(),
            _ => false,
        });
        if !huggable {
            return full;
        }
        let n = args.len();
        let mut prefix = vec![text("(")];
        for i in 0..n - 1 {
            if let Some(l) = &labels[i] {
                prefix.push(l.clone());
            }
            prefix.push(values[i].clone());
            prefix.push(self.comma());
            prefix.push(text(" "));
        }
        if let Some(l) = &labels[n - 1] {
            prefix.push(l.clone());
        }
        Doc::Hug {
            prefix,
            last: Box::new(values[n - 1].clone()),
            suffix: vec![text(")")],
            full: Box::new(full),
        }
    }

    fn str_lit(&mut self, s: &StrLit, tag: Option<&str>) -> Doc {
        let q = if s.triple { "\"\"\"" } else { "\"" };
        let mut parts = vec![text(format!("{}{q}", tag.unwrap_or("")))];
        for p in &s.parts {
            match p {
                StrPart::Text(t) => parts.push(text(t.clone())),
                StrPart::Hole(e) => {
                    let d = self.expr(e, INNER);
                    let mut s = String::from("{");
                    flat_text(&d, &mut s);
                    s.push('}');
                    parts.push(Doc::Text(s, flat_width(&d).unwrap_or(0) + 2));
                }
            }
        }
        parts.push(text(q));
        concat(parts)
    }

    /// One level of the expression walk; deep chains continue on a big stack
    /// ([`kalam_ast::stack`]), so a valid program never overflows the caller's stack.
    fn expr(&mut self, e: &Expr, ctx: Ctx) -> Doc {
        kalam_ast::stack::deeper(|| self.expr_inner(e, ctx))
    }

    fn expr_inner(&mut self, e: &Expr, ctx: Ctx) -> Doc {
        match &e.kind {
            ExprKind::Call { .. }
            | ExprKind::Index { .. }
            | ExprKind::Member { .. }
            | ExprKind::Try(_) => self.postfix(e, ctx),
            ExprKind::Int(i) => self.number(&i.text, e.span),
            ExprKind::Float(f) => self.number(&f.text, e.span),
            ExprKind::Str(s) => self.str_lit(s, None),
            ExprKind::Tagged { tag, lit } => self.str_lit(lit, Some(tag)),
            ExprKind::Bool(b) => self.kw(if *b {
                Concept::KwTrue
            } else {
                Concept::KwFalse
            }),
            ExprKind::Name(n) => self.name_kind(n, Role::Bound),
            ExprKind::SelfValue => self.kw(Concept::KwSelf),
            ExprKind::ImplicitMember(i) => concat(vec![text("."), self.id(&i.name, Role::Free)]),
            ExprKind::UnsafeMember(i) => concat(vec![
                self.kw(Concept::KwUnsafe),
                text("."),
                self.id(&i.name, Role::Free),
            ]),
            ExprKind::Unit => text("()"),
            ExprKind::Paren(x) => concat(vec![text("("), self.expr(x, TAIL), text(")")]),
            ExprKind::Tuple(xs) => {
                let docs = xs.iter().map(|x| self.expr(x, TAIL)).collect();
                self.list_with("(", docs, ")", xs.len() == 1)
            }
            ExprKind::List(xs) => {
                let docs = xs.iter().map(|x| self.expr(x, TAIL)).collect();
                self.list("[", docs, "]")
            }
            ExprKind::Map(entries) => {
                let docs = entries
                    .iter()
                    .map(|(k, v)| concat(vec![self.expr(k, TAIL), text(": "), self.expr(v, TAIL)]))
                    .collect();
                self.list("{", docs, "}")
            }
            ExprKind::Neg(x) => {
                let inner = self.expr(x, INNER);
                let mut s = String::new();
                flat_text(&inner, &mut s);
                // `- -x`: two minus signs never touch.
                let sign = if s.starts_with('-') { "- " } else { "-" };
                concat(vec![text(sign), inner])
            }
            ExprKind::Not(x) => concat(vec![self.kw_sp(Concept::KwNot), self.expr(x, INNER)]),
            ExprKind::Binary { op, lhs, rhs } => {
                let op = match op {
                    BinOp::Or => self.kw(Concept::KwOr),
                    BinOp::And => self.kw(Concept::KwAnd),
                    BinOp::Eq => text("=="),
                    BinOp::NotEq => text("!="),
                    BinOp::Lt => text("<"),
                    BinOp::Le => text("<="),
                    BinOp::Gt => text(">"),
                    BinOp::Ge => text(">="),
                    BinOp::Add => text("+"),
                    BinOp::Sub => text("-"),
                    BinOp::Mul => text("*"),
                    BinOp::Div => text("/"),
                    BinOp::IntDiv => text("//"),
                    BinOp::Rem => text("%"),
                };
                concat(vec![
                    self.expr(lhs, INNER),
                    text(" "),
                    op,
                    text(" "),
                    self.expr(rhs, INNER),
                ])
            }
            ExprKind::Range { lo, hi, inclusive } => concat(vec![
                self.expr(lo, INNER),
                text(if *inclusive { "..=" } else { ".." }),
                self.expr(hi, INNER),
            ]),
            ExprKind::Lambda(l) => {
                let params = l
                    .params
                    .iter()
                    .map(|p| {
                        let mut d = vec![self.id(&p.name.name, Role::Bound)];
                        if let Some(t) = &p.ty {
                            d.push(text(": "));
                            d.push(self.ty(t));
                        }
                        concat(d)
                    })
                    .collect();
                let mut d = vec![self.kw(Concept::KwFn), self.list("(", params, ")")];
                if let Some(r) = &l.ret {
                    d.push(text(" -> "));
                    d.push(self.ty(r));
                }
                d.push(text(" => "));
                d.push(self.expr(&l.body, INNER));
                concat(d)
            }
            ExprKind::If(ifx) if ifx.inline => {
                let mut d = Vec::new();
                for (i, br) in ifx.branches.iter().enumerate() {
                    if i > 0 {
                        d.push(text(" "));
                        d.push(self.kw_sp(Concept::KwElse));
                    }
                    d.push(self.kw_sp(Concept::KwIf));
                    d.push(self.expr(&br.cond, INNER));
                    d.push(text(": "));
                    d.push(self.arm(&br.body));
                }
                if let Some(b) = &ifx.else_body {
                    d.push(text(" "));
                    d.push(self.kw(Concept::KwElse));
                    d.push(text(": "));
                    d.push(self.arm(b));
                }
                concat(d)
            }
            ExprKind::If(_) | ExprKind::Match(_) => self.fail("a block form inside an expression"),
            ExprKind::Error => self.fail("an expression did not parse"),
        }
    }

    /// The expression of an inline-`if` arm.
    fn arm(&mut self, b: &Block) -> Doc {
        match b.stmts.as_slice() {
            [Stmt {
                kind: StmtKind::Expr(e),
                ..
            }] => self.expr(e, INNER),
            _ => self.fail("an inline `if` arm is not one expression"),
        }
    }

    /// A postfix chain. Two or more `.name(…)` calls after the head break, if the line is
    /// too long, into leading-dot continuation lines one level deeper (spec §5.8 rule 2).
    /// A head no wider than one indent keeps its first call (`http.router()`).
    fn postfix(&mut self, e: &Expr, ctx: Ctx) -> Doc {
        let mut ops: Vec<&Expr> = Vec::new();
        let mut cur = e;
        while let ExprKind::Call { callee: b, .. }
        | ExprKind::Index { base: b, .. }
        | ExprKind::Member { base: b, .. }
        | ExprKind::Try(b) = &cur.kind
        {
            ops.push(cur);
            cur = b;
        }
        ops.reverse();
        let mut head = vec![self.expr(cur, INNER)];
        let mut segments: Vec<Vec<Doc>> = Vec::new();
        for (i, op) in ops.iter().enumerate() {
            let starts_segment =
                matches!(
                    &op.kind,
                    ExprKind::Member {
                        member: Member::Name(_),
                        ..
                    }
                ) && matches!(ops.get(i + 1).map(|o| &o.kind), Some(ExprKind::Call { .. }));
            let d = match &op.kind {
                ExprKind::Call { args, .. } => self.args(args),
                ExprKind::Index { index, .. } => {
                    concat(vec![text("["), self.expr(index, TAIL), text("]")])
                }
                ExprKind::Member { member, .. } => match member {
                    Member::Name(n) => concat(vec![text("."), self.id(&n.name, Role::Free)]),
                    // A chain of numeric member indices (`t.0.1`) stays ASCII, as in
                    // `convert` (DR-26: `t.٠.١` would re-lex as `t . ٠.١`, SEC0120).
                    Member::Index { text: t, .. }
                        if [i.wrapping_sub(1), i + 1]
                            .iter()
                            .any(|&j| ops.get(j).is_some_and(|o| is_index(o))) =>
                    {
                        let ascii = to_ascii_digits(t);
                        let w = ascii.chars().count();
                        concat(vec![text("."), Doc::Text(ascii, w)])
                    }
                    Member::Index { text: t, .. } => concat(vec![text("."), self.digits(t)]),
                },
                _ => Doc::Text(
                    self.table
                        .spelling(Concept::PunctTry, self.skin)
                        .to_string(),
                    1,
                ),
            };
            if starts_segment {
                segments.push(vec![d]);
            } else if let Some(seg) = segments.last_mut() {
                seg.push(d);
            } else {
                head.push(d);
            }
        }
        let head_short = flat_width(&concat(head.clone())).is_some_and(|w| w <= INDENT);
        if head_short && !segments.is_empty() {
            head.extend(segments.remove(0));
        }
        if !ctx.chain || segments.len() < 2 {
            head.extend(segments.into_iter().flatten());
            return concat(head);
        }
        let mut tail = Vec::new();
        for seg in segments {
            tail.push(Doc::Soft);
            tail.extend(seg);
        }
        Doc::Group(vec![concat(head), Doc::Indent(tail)])
    }

    // ----- patterns and types -------------------------------------------------------------

    fn int_bound(&self, b: &IntPatBound) -> Doc {
        let d = self.digits(&b.lit.text);
        if b.negative {
            concat(vec![text("-"), d])
        } else {
            d
        }
    }

    fn pat_fields(&mut self, f: &PatFields) -> Doc {
        let mut docs: Vec<Doc> = f
            .fields
            .iter()
            .map(|pf| {
                let mut d = Vec::new();
                if let Some(l) = &pf.label {
                    d.push(self.id(&l.name, Role::Free));
                    d.push(text(": "));
                }
                d.push(self.pat(&pf.pat));
                concat(d)
            })
            .collect();
        if f.rest {
            docs.push(text(".."));
        }
        self.list("(", docs, ")")
    }

    fn pat(&mut self, p: &Pat) -> Doc {
        kalam_ast::stack::deeper(|| self.pat_inner(p))
    }

    fn pat_inner(&mut self, p: &Pat) -> Doc {
        match &p.kind {
            PatKind::Wild => text("_"),
            PatKind::Bind(i) => self.id(&i.name, Role::Bound),
            PatKind::Int(b) => self.int_bound(b),
            PatKind::Range { lo, hi, inclusive } => concat(vec![
                self.int_bound(lo),
                text(if *inclusive { "..=" } else { ".." }),
                self.int_bound(hi),
            ]),
            PatKind::Str(s) => self.str_lit(s, None),
            PatKind::Bool(b) => self.kw(if *b {
                Concept::KwTrue
            } else {
                Concept::KwFalse
            }),
            PatKind::Implicit { name, fields } => {
                let mut d = vec![text("."), self.id(&name.name, Role::Free)];
                if let Some(f) = fields {
                    d.push(self.pat_fields(f));
                }
                concat(d)
            }
            PatKind::Path { path, fields } => {
                let mut d = Vec::new();
                for (i, n) in path.iter().enumerate() {
                    if i > 0 {
                        d.push(text("."));
                    }
                    let role = if i == 0 { Role::Bound } else { Role::Free };
                    d.push(self.name_kind(&n.kind, role));
                }
                if let Some(f) = fields {
                    d.push(self.pat_fields(f));
                }
                concat(d)
            }
            PatKind::Ctor { ctor, inner } => {
                let mut d = vec![self.kw(*ctor)];
                if let Some(i) = inner {
                    d.push(text("("));
                    d.push(self.pat(i));
                    d.push(text(")"));
                }
                concat(d)
            }
            PatKind::Tuple(ps) => {
                let docs = ps.iter().map(|p| self.pat(p)).collect();
                self.list("(", docs, ")")
            }
            PatKind::List(elems) => {
                let docs = elems
                    .iter()
                    .map(|e| match e {
                        ListPatElem::Pat(p) => self.pat(p),
                        ListPatElem::Rest(None) => text(".."),
                        ListPatElem::Rest(Some(i)) => {
                            concat(vec![text(".."), self.id(&i.name, Role::Bound)])
                        }
                    })
                    .collect();
                self.list("[", docs, "]")
            }
            PatKind::Or(ps) => {
                let mut d = Vec::new();
                for (i, p) in ps.iter().enumerate() {
                    if i > 0 {
                        d.push(text(" | "));
                    }
                    d.push(self.pat(p));
                }
                concat(d)
            }
            PatKind::As { pat, name } => concat(vec![
                self.pat(pat),
                text(" "),
                self.kw_sp(Concept::KwAs),
                self.id(&name.name, Role::Bound),
            ]),
            PatKind::Error => self.fail("a pattern did not parse"),
        }
    }

    fn ty(&mut self, t: &Type) -> Doc {
        kalam_ast::stack::deeper(|| self.ty_inner(t))
    }

    fn ty_inner(&mut self, t: &Type) -> Doc {
        match &t.kind {
            TypeKind::Named { path, args } => {
                let mut d = Vec::new();
                for (i, n) in path.iter().enumerate() {
                    if i > 0 {
                        d.push(text("."));
                    }
                    let role = if i == 0 { Role::Bound } else { Role::Free };
                    d.push(self.name_kind(&n.kind, role));
                }
                if !args.is_empty() {
                    let docs = args.iter().map(|a| self.ty(a)).collect();
                    d.push(self.list("[", docs, "]"));
                }
                concat(d)
            }
            TypeKind::Unit => text("()"),
            TypeKind::Tuple(ts) => {
                let docs = ts.iter().map(|t| self.ty(t)).collect();
                self.list_with("(", docs, ")", ts.len() == 1)
            }
            TypeKind::Fn { sync, params, ret } => {
                let mut d = Vec::new();
                if *sync {
                    d.push(text("@sync "));
                }
                d.push(self.kw(Concept::KwFn));
                let docs = params.iter().map(|p| self.ty(p)).collect();
                d.push(self.list("(", docs, ")"));
                d.push(text(" -> "));
                d.push(self.ty(ret));
                concat(d)
            }
            TypeKind::Error => self.fail("a type did not parse"),
        }
    }
}

/// A multi-line `if` or `match` form (only at statement level or after a binding head).
fn is_form(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::If(i) => !i.inline,
        ExprKind::Match(_) => true,
        _ => false,
    }
}

/// `e` is a numeric member access (`t.0`).
fn is_index(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Member {
            member: Member::Index { .. },
            ..
        }
    )
}
