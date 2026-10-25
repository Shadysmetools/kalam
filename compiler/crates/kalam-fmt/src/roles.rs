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
//!
//! **Cross-skin words (FOUNDER-QUEUE Q16).** A word reserved in some skin, used as a plain
//! name that the file does not declare, is K0201 in every skin but the ones that reserve it
//! (spec §5.4, Q9). So such a name keeps its backticks in every skin, and `fmt`/`convert` add
//! them where they are missing: their output always passes their own lexer. "Declared" is
//! the lexer's own file-local, scope-insensitive lookup ([`declared_names`]), computed on a
//! skin-free view of the tokens, so the decision is the same in every skin and every round
//! trip is byte-identical (spec §4.3 G2).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use kalam_ast::*;
use kalam_base::{FileId, Skin};
use kalam_lexer::{Op, Token, TokenKind};
use kalam_skins::{Concept, KeywordTable};

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
    /// Names the file declares, as the lexer counts them ([`declared_names`]).
    declared: HashSet<String>,
    /// [`lexes_bare`] by name.
    bare: Mutex<HashMap<String, bool>>,
}

impl Roles {
    /// The roles of `ast`'s identifiers, and the names declared in `text` (whose tokens are
    /// `tokens`, from the same parse).
    pub fn of(ast: &Ast, text: &str, tokens: &[Token]) -> Roles {
        let mut r = Roles::default();
        r.file(&ast.file);
        r.declared = declared_names(text, tokens, &r);
        r
    }

    pub fn get(&self, start: u32) -> Option<Role> {
        self.map.get(&start).copied()
    }

    fn lexes_bare(&self, name: &str) -> bool {
        let mut cache = self.bare.lock().unwrap_or_else(|e| e.into_inner());
        *cache
            .entry(name.to_string())
            .or_insert_with(|| lexes_bare(name))
    }

    /// The identifier `name` at a position with `role`, as written in skin `to`: in
    /// backticks if it would lex as a reserved word of `to` (spec §4.3), or if it is reserved
    /// in some skin and the file does not declare it (Q16), or, in any position, if it would
    /// not lex bare as one clean name ([`lexes_bare`]). Otherwise bare (§5.3 rule 7: no
    /// unneeded backticks). Only the first test depends on `to`, so the decision is the same
    /// in every skin except where the target skin itself reserves the word.
    pub fn spell(&self, name: &str, role: Role, to: Skin, table: &KeywordTable) -> String {
        let escape = (role == Role::Bound
            && (table.skin(to).is_reserved(name)
                || (table.reserved_in_some_skin(name) && !self.declared.contains(name))))
            || !self.lexes_bare(name);
        if escape {
            format!("`{name}`")
        } else {
            name.to_string()
        }
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

/// Every name the file declares or imports, by the K0201 rule of `kalam-lexer`
/// (`declared_names`, FOUNDER-QUEUE Q9): `let`/`var`/`for`/`case` patterns, `use` lines,
/// the word after `as`, `fn`/`type`/`enum`/`trait`/`impl` names, generic parameters and `fn`
/// parameters (lambdas included). Strings are single tokens, so names inside interpolation
/// holes are never declarations, as in the lexer.
///
/// It runs on a skin-free view of the tokens: a keyword or core-name token that the parser
/// took as a name (any position in the role map: a [`Role::Free`] one such as the label in
/// `Point(match: m)`, or a [`Role::Bound`] one such as `tala3` in a Franco `use` list) counts
/// as a name, whatever skin the file is in, exactly like the same name in backticks. The target skin's lexer sees the same tokens,
/// except that such a word reserved in the target skin is a keyword there and is not counted;
/// but a word reserved in the target is escaped anyway. So this set is the same in every
/// skin, and it never holds a name the target lexer would not count as declared.
fn declared_names(text: &str, tokens: &[Token], roles: &Roles) -> HashSet<String> {
    let name = |j: usize| -> bool {
        tokens.get(j).is_some_and(|t| match t.kind {
            TokenKind::Ident => true,
            // Any position the parser took as a name, Free or Bound: `tala3` (a Franco
            // `export`) in a `use` list is a keyword token, but the same name written
            // `` `tala3` `` is an identifier, and both must count alike or `fmt` is not
            // idempotent (the layout width depends on this set).
            TokenKind::Keyword(_) | TokenKind::Core(_) => roles.get(t.span.start).is_some(),
            _ => false,
        })
    };
    // The skin-free kind: a name is an identifier, whatever its spelling.
    let kind = |j: usize| -> Option<TokenKind> {
        tokens
            .get(j)
            .map(|t| if name(j) { TokenKind::Ident } else { t.kind })
    };
    let is = |j: usize, op: Op| kind(j) == Some(TokenKind::Op(op));
    let add = |j: usize, out: &mut HashSet<String>| {
        if kind(j) == Some(TokenKind::Ident) && !(j > 0 && is(j - 1, Op::Dot)) {
            let w = &text[tokens[j].span.range()];
            out.insert(crate::convert::strip_backticks(w).to_string());
        }
    };
    // Names directly inside the bracket group opened at `open` that start an item (after the
    // opening bracket or a comma): parameters, generic parameters.
    let group = |open: usize, out: &mut HashSet<String>| -> usize {
        let mut depth = 0usize;
        let mut j = open;
        while let Some(k) = kind(j) {
            match k {
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return j + 1;
                    }
                }
                TokenKind::Newline | TokenKind::Eof => return j,
                TokenKind::Ident if depth == 1 && (j == open + 1 || is(j - 1, Op::Comma)) => {
                    add(j, out)
                }
                _ => {}
            }
            j += 1;
        }
        j
    };
    // Names from `from` up to a stop token at bracket depth 0. In a pattern, a name right
    // after a complete operand ends it too.
    let until = |from: usize,
                 stop: &dyn Fn(TokenKind) -> bool,
                 pattern: bool,
                 out: &mut HashSet<String>| {
        let mut depth = 0usize;
        let mut j = from;
        while let Some(k) = kind(j) {
            match k {
                TokenKind::Newline | TokenKind::Eof => break,
                _ if depth == 0 && stop(k) => break,
                TokenKind::Ident
                    if pattern
                        && depth == 0
                        && j > from
                        && kind(j - 1).is_some_and(ends_an_operand) =>
                {
                    break
                }
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1)
                }
                TokenKind::Ident => add(j, out),
                _ => {}
            }
            j += 1;
        }
    };
    let mut out = HashSet::new();
    for i in 0..tokens.len() {
        let Some(TokenKind::Keyword(c)) = kind(i) else {
            continue;
        };
        match c {
            Concept::KwLet | Concept::KwVar => until(
                i + 1,
                &|k| matches!(k, TokenKind::Op(Op::Eq | Op::Colon)),
                true,
                &mut out,
            ),
            Concept::KwFor => until(
                i + 1,
                &|k| k == TokenKind::Keyword(Concept::KwIn),
                true,
                &mut out,
            ),
            Concept::KwCase => until(
                i + 1,
                &|k| {
                    matches!(
                        k,
                        TokenKind::Op(Op::Colon) | TokenKind::Keyword(Concept::KwIf)
                    )
                },
                true,
                &mut out,
            ),
            Concept::KwUse => until(i + 1, &|_| false, false, &mut out),
            Concept::KwAs => add(i + 1, &mut out),
            Concept::KwFn
            | Concept::KwType
            | Concept::KwEnum
            | Concept::KwTrait
            | Concept::KwImpl => {
                let mut j = i + 1;
                if kind(j) == Some(TokenKind::Ident) {
                    add(j, &mut out);
                    j += 1;
                }
                if is(j, Op::LBracket) {
                    j = group(j, &mut out);
                }
                if c == Concept::KwFn && is(j, Op::LParen) {
                    group(j, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

/// A token after which a name would be a second operand side by side with the first
/// (`kalam-lexer`'s `ends_an_operand`).
fn ends_an_operand(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident
            | TokenKind::Core(_)
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Str
            | TokenKind::TaggedStr
            | TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace | Op::Question)
            | TokenKind::Keyword(Concept::KwTrue | Concept::KwFalse | Concept::KwSelf)
    )
}

/// `name`, written without backticks, lexes in every skin as exactly one word token (a name,
/// or a reserved word, which [`Roles::spell`] escapes separately) with no error.
///
/// The lexer applies the identifier rules of spec §5.3 inside backticks too (mixed scripts
/// SEC0105, harakat SEC0106, Arabic-Indic digits SEC0124), so a raw name that would not lex
/// bare is normally rejected before `fmt` sees it. This check stays as the guard that
/// dropping backticks never turns an accepted file into a rejected one. Plain ASCII names
/// always lex bare.
fn lexes_bare(name: &str) -> bool {
    let mut chars = name.chars();
    let ascii_plain = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ascii_plain {
        return true;
    }
    Skin::ALL.into_iter().all(|skin| {
        let lexed = kalam_lexer::lex_fragment(name, skin, FileId(0));
        let mut words = lexed.tokens.iter().filter(|t| {
            !matches!(
                t.kind,
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
            )
        });
        let one_word = matches!(
            (words.next(), words.next()),
            (Some(t), None)
                if matches!(t.kind, TokenKind::Ident | TokenKind::Keyword(_) | TokenKind::Core(_))
                    && !t.flags.raw_ident
                    && t.span.start == 0
                    && t.span.end as usize == name.len()
        );
        one_word && !lexed.diagnostics.iter().any(|d| d.is_error())
    })
}
