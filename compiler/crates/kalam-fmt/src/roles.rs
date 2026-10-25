//! Which word tokens are identifiers, and which of those stand where a reserved word is an
//! ordinary identifier (spec §5.3 rule 8).
//!
//! The parser accepts a keyword or core-name token as an identifier in a few positions
//! (`at_word`, `nth_is_word` and `member_ident` in `kalam-parser`): a named-argument label, a
//! record field name, a variant or pattern field label, a member after `.`, an implicit
//! member, a later path segment, a use module path and an attribute path. Such a token must never be
//! translated or canonicalized (it is a name, `f(match: 1)` keeps `match`), and it never needs
//! backticks in any skin. Every other identifier is backtick-escaped when it would lex as a
//! reserved word of the target skin (spec §4.3).
//!
//! The map is keyed by the token's start offset in the file. Identifiers inside interpolation
//! holes carry absolute offsets too (the parser shifts hole spans), so holes use the same map.

use std::collections::HashMap;

use kalam_ast::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// A §5.3 rule 8 position: never escaped.
    Free,
    /// Any other identifier: escaped if reserved in the target skin.
    Bound,
}

#[derive(Default)]
pub(crate) struct Roles {
    map: HashMap<u32, Role>,
}

impl Roles {
    pub fn of(ast: &Ast) -> Roles {
        let mut r = Roles::default();
        r.file(&ast.file);
        r
    }

    pub fn get(&self, start: u32) -> Option<Role> {
        self.map.get(&start).copied()
    }

    fn ident(&mut self, i: &Ident, role: Role) {
        if !i.name.is_empty() {
            self.map.insert(i.span.start, role);
        }
    }

    fn name(&mut self, n: &Name, role: Role) {
        if let NameKind::Ident(_) = n.kind {
            self.map.insert(n.span.start, role);
        }
    }

    fn file(&mut self, f: &File) {
        for u in &f.uses {
            for p in &u.path {
                self.ident(p, Role::Free);
            }
            match &u.tail {
                UseTail::None => {}
                UseTail::Alias(a) => self.ident(a, Role::Bound),
                UseTail::Items(items) => {
                    for it in items {
                        if let UseItem::Name { name, alias } = it {
                            // Not a §5.3 rule 8 position: the target skin's `self` keyword
                            // there would parse as `UseItem::SelfModule`.
                            self.ident(name, Role::Bound);
                            if let Some(a) = alias {
                                self.ident(a, Role::Bound);
                            }
                        }
                    }
                }
            }
        }
        for it in &f.items {
            self.item(it);
        }
    }

    fn attrs(&mut self, attrs: &[Attribute]) {
        for a in attrs {
            for p in &a.path {
                self.ident(p, Role::Free);
            }
            if let Some(args) = &a.args {
                self.args(args);
            }
        }
    }

    fn item(&mut self, it: &Item) {
        self.attrs(&it.attrs);
        match &it.kind {
            ItemKind::Fn(f) => self.fn_decl(f),
            ItemKind::Type(t) => {
                self.name(&t.name, Role::Bound);
                self.generics(&t.generics);
                match &t.body {
                    TypeBody::Record(fields) => {
                        for f in fields {
                            self.attrs(&f.attrs);
                            self.ident(&f.name, Role::Free);
                            self.ty(&f.ty);
                            if let Some(d) = &f.default {
                                self.expr(d);
                            }
                        }
                    }
                    TypeBody::Alias(t) => self.ty(t),
                }
            }
            ItemKind::Enum(e) => {
                self.name(&e.name, Role::Bound);
                self.generics(&e.generics);
                for v in &e.variants {
                    self.attrs(&v.attrs);
                    self.ident(&v.name, Role::Bound);
                    for f in v.fields.iter().flatten() {
                        if let Some(l) = &f.label {
                            self.ident(l, Role::Free);
                        }
                        self.ty(&f.ty);
                    }
                }
            }
            ItemKind::Trait(t) => {
                self.name(&t.name, Role::Bound);
                self.generics(&t.generics);
                for m in &t.members {
                    self.fn_decl(m);
                }
            }
            ItemKind::Impl(i) => {
                self.generics(&i.generics);
                self.ty(&i.ty);
                if let Some(t) = &i.as_trait {
                    self.ty(t);
                }
                for m in &i.members {
                    self.attrs(&m.attrs);
                    self.fn_decl(&m.func);
                }
            }
            ItemKind::Const(c) => {
                self.name(&c.name, Role::Bound);
                if let Some(t) = &c.ty {
                    self.ty(t);
                }
                self.expr(&c.value);
            }
            ItemKind::Extern(ExternDecl::Fn(sig)) => self.sig(sig),
            ItemKind::Extern(ExternDecl::Type { name, generics }) => {
                self.name(name, Role::Bound);
                self.generics(generics);
            }
            ItemKind::Test(t) => {
                self.str_lit(&t.name);
                self.block(&t.body);
            }
            ItemKind::Error => {}
        }
    }

    fn generics(&mut self, gs: &[GenericParam]) {
        for g in gs {
            self.ident(&g.name, Role::Bound);
            for b in &g.bounds {
                self.ty(b);
            }
        }
    }

    fn sig(&mut self, s: &FnSig) {
        self.name(&s.name, Role::Bound);
        self.generics(&s.generics);
        for p in &s.params {
            if let Param::Named {
                name, ty, default, ..
            } = p
            {
                self.ident(name, Role::Bound);
                self.ty(ty);
                if let Some(d) = default {
                    self.expr(d);
                }
            }
        }
        if let Some(r) = &s.ret {
            self.ty(r);
        }
    }

    fn fn_decl(&mut self, f: &FnDecl) {
        self.sig(&f.sig);
        match &f.body {
            FnBody::Block(b) => self.block(b),
            FnBody::Expr(e) => self.expr(e),
            FnBody::None => {}
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Expr(e) => self.expr(e),
            StmtKind::Let { pat, ty, value } => {
                self.pat(pat);
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(value);
            }
            StmtKind::Var { name, ty, value } => {
                self.ident(name, Role::Bound);
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(value);
            }
            StmtKind::Assign { place, value, .. } => {
                self.expr(place);
                self.expr(value);
            }
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    self.expr(e);
                }
            }
            StmtKind::For { pat, iter, body } => {
                self.pat(pat);
                self.expr(iter);
                self.block(body);
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            StmtKind::Fn(f) => self.fn_decl(f),
            StmtKind::Break | StmtKind::Continue | StmtKind::Pass | StmtKind::Error => {}
        }
    }

    fn args(&mut self, args: &[Arg]) {
        for a in args {
            if let Some(l) = &a.label {
                self.ident(l, Role::Free);
            }
            self.expr(&a.value);
        }
    }

    fn str_lit(&mut self, s: &StrLit) {
        for p in &s.parts {
            if let StrPart::Hole(e) = p {
                self.expr(e);
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
        kalam_ast::stack::deeper(|| self.expr_inner(e));
    }

    fn expr_inner(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Str(s) => self.str_lit(s),
            ExprKind::Tagged { lit, .. } => self.str_lit(lit),
            ExprKind::Name(NameKind::Ident(_)) => {
                self.map.insert(e.span.start, Role::Bound);
            }
            ExprKind::ImplicitMember(i) | ExprKind::UnsafeMember(i) => self.ident(i, Role::Free),
            ExprKind::Paren(x) | ExprKind::Try(x) | ExprKind::Neg(x) | ExprKind::Not(x) => {
                self.expr(x)
            }
            ExprKind::Tuple(xs) | ExprKind::List(xs) => {
                for x in xs {
                    self.expr(x);
                }
            }
            ExprKind::Map(entries) => {
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee);
                self.args(args);
            }
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            ExprKind::Member { base, member } => {
                self.expr(base);
                if let Member::Name(n) = member {
                    self.ident(n, Role::Free);
                }
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Range { lo, hi, .. } => {
                self.expr(lo);
                self.expr(hi);
            }
            ExprKind::Lambda(l) => {
                for p in &l.params {
                    self.ident(&p.name, Role::Bound);
                    if let Some(t) = &p.ty {
                        self.ty(t);
                    }
                }
                if let Some(r) = &l.ret {
                    self.ty(r);
                }
                self.expr(&l.body);
            }
            ExprKind::If(i) => {
                for b in &i.branches {
                    self.expr(&b.cond);
                    self.block(&b.body);
                }
                if let Some(b) = &i.else_body {
                    self.block(b);
                }
            }
            ExprKind::Match(m) => {
                self.expr(&m.scrutinee);
                for a in &m.arms {
                    self.pat(&a.pat);
                    if let Some(g) = &a.guard {
                        self.expr(g);
                    }
                    self.block(&a.body);
                }
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Name(NameKind::Core(_))
            | ExprKind::SelfValue
            | ExprKind::Unit
            | ExprKind::Error => {}
        }
    }

    fn pat_fields(&mut self, f: &PatFields) {
        for pf in &f.fields {
            if let Some(l) = &pf.label {
                self.ident(l, Role::Free);
            }
            self.pat(&pf.pat);
        }
    }

    fn pat(&mut self, p: &Pat) {
        kalam_ast::stack::deeper(|| self.pat_inner(p));
    }

    fn pat_inner(&mut self, p: &Pat) {
        match &p.kind {
            PatKind::Bind(i) => self.ident(i, Role::Bound),
            PatKind::Str(s) => self.str_lit(s),
            PatKind::Implicit { name, fields } => {
                self.ident(name, Role::Free);
                if let Some(f) = fields {
                    self.pat_fields(f);
                }
            }
            PatKind::Path { path, fields } => {
                for (i, n) in path.iter().enumerate() {
                    self.name(n, if i == 0 { Role::Bound } else { Role::Free });
                }
                if let Some(f) = fields {
                    self.pat_fields(f);
                }
            }
            PatKind::Ctor { inner, .. } => {
                if let Some(i) = inner {
                    self.pat(i);
                }
            }
            PatKind::Tuple(ps) | PatKind::Or(ps) => {
                for p in ps {
                    self.pat(p);
                }
            }
            PatKind::List(elems) => {
                for e in elems {
                    match e {
                        ListPatElem::Pat(p) => self.pat(p),
                        ListPatElem::Rest(Some(i)) => self.ident(i, Role::Bound),
                        ListPatElem::Rest(None) => {}
                    }
                }
            }
            PatKind::As { pat, name } => {
                self.pat(pat);
                self.ident(name, Role::Bound);
            }
            PatKind::Wild
            | PatKind::Int(_)
            | PatKind::Range { .. }
            | PatKind::Bool(_)
            | PatKind::Error => {}
        }
    }

    fn ty(&mut self, t: &Type) {
        kalam_ast::stack::deeper(|| self.ty_inner(t));
    }

    fn ty_inner(&mut self, t: &Type) {
        match &t.kind {
            TypeKind::Named { path, args } => {
                for (i, n) in path.iter().enumerate() {
                    self.name(n, if i == 0 { Role::Bound } else { Role::Free });
                }
                for a in args {
                    self.ty(a);
                }
            }
            TypeKind::Tuple(ts) => {
                for t in ts {
                    self.ty(t);
                }
            }
            TypeKind::Fn { params, ret, .. } => {
                for p in params {
                    self.ty(p);
                }
                self.ty(ret);
            }
            TypeKind::Unit | TypeKind::Error => {}
        }
    }
}
