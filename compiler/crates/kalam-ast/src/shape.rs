//! A span-free, skin-free S-expression dump of an [`Ast`].
//!
//! Two files that are conversions of each other (spec §4.3) have byte-identical shapes.
//! This is the structural-equality check used by the cross-skin tests, and a readable
//! golden format. Keywords never appear (they are structure), core names appear as their
//! concept IDs (`CORE_PRINT`), numbers ASCII-normalized, identifiers as written.

use crate::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct ShapeOptions {
    /// Replace every identifier with `_` (used for spec §28 "glossary view" blocks, whose
    /// identifiers are display translations of the English names by design, §4.5).
    pub erase_idents: bool,
    /// Leave trivia (comments, blank lines) out of the dump.
    pub skip_trivia: bool,
}

/// Dumps the whole tree (and, unless `skip_trivia`, its trivia).
pub fn shape(ast: &Ast, opts: ShapeOptions) -> String {
    let mut w = W {
        out: String::new(),
        opts,
    };
    w.file(&ast.file);
    if !opts.skip_trivia {
        for t in &ast.trivia {
            w.out.push('\n');
            match t.kind {
                TriviaKind::Comment => w
                    .out
                    .push_str(&format!("(comment {} {:?})", t.line, t.text)),
                TriviaKind::DocComment => w.out.push_str(&format!("(doc {} {:?})", t.line, t.text)),
                TriviaKind::BlankLine => w.out.push_str(&format!("(blank {})", t.line)),
            }
        }
    }
    w.out
}

struct W {
    out: String,
    opts: ShapeOptions,
}

impl W {
    fn s(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// Opens `(tag`; a leading space in `tag` is written before the parenthesis.
    fn open(&mut self, tag: &str) {
        if let Some(rest) = tag.strip_prefix(' ') {
            self.out.push(' ');
            self.out.push('(');
            self.out.push_str(rest);
        } else {
            self.out.push('(');
            self.out.push_str(tag);
        }
    }

    fn close(&mut self) {
        self.out.push(')');
    }

    fn sp(&mut self) {
        self.out.push(' ');
    }

    fn ident(&mut self, i: &Ident) {
        self.sp();
        if self.opts.erase_idents {
            self.s("_");
        } else {
            self.s(&format!("{:?}", i.name));
        }
    }

    fn name_kind(&mut self, n: &NameKind) {
        self.sp();
        match n {
            NameKind::Ident(s) => {
                if self.opts.erase_idents {
                    self.s("_");
                } else {
                    self.s(&format!("{s:?}"));
                }
            }
            NameKind::Core(c) => self.s(c.name()),
        }
    }

    fn name(&mut self, n: &Name) {
        self.name_kind(&n.kind);
    }

    fn opt<T>(&mut self, v: &Option<T>, f: impl FnOnce(&mut W, &T)) {
        match v {
            Some(x) => f(self, x),
            None => self.s(" -"),
        }
    }

    fn file(&mut self, f: &File) {
        self.open("file");
        for u in &f.uses {
            self.s("\n ");
            self.use_decl(u);
        }
        for i in &f.items {
            self.s("\n ");
            self.item(i);
        }
        self.close();
    }

    fn use_decl(&mut self, u: &UseDecl) {
        self.open(if u.export { "export-use" } else { "use" });
        // Module paths are never translated (§4.3); keep them even when erasing identifiers.
        let path: Vec<&str> = u.path.iter().map(|i| i.name.as_str()).collect();
        self.s(&format!(" {:?}", path.join("/")));
        match &u.tail {
            UseTail::None => {}
            UseTail::Alias(a) => {
                self.s(" as");
                self.ident(a);
            }
            UseTail::Items(items) => {
                self.s(" {");
                for it in items {
                    match it {
                        UseItem::SelfModule(_) => self.s(" self"),
                        UseItem::Name { name, alias } => {
                            self.ident(name);
                            if let Some(a) = alias {
                                self.s(" as");
                                self.ident(a);
                            }
                        }
                    }
                }
                self.s(" }");
            }
        }
        self.close();
    }

    fn attrs(&mut self, attrs: &[Attribute]) {
        for a in attrs {
            self.open(" @");
            let path: Vec<&str> = a.path.iter().map(|i| i.name.as_str()).collect();
            self.s(&path.join("."));
            if let Some(args) = &a.args {
                self.args(args);
            }
            self.close();
        }
    }

    fn item(&mut self, i: &Item) {
        self.open("item");
        match i.vis {
            Visibility::Private => {}
            Visibility::Export => self.s(" export"),
            Visibility::ExportOpaque => self.s(" export-opaque"),
        }
        self.attrs(&i.attrs);
        self.sp();
        match &i.kind {
            ItemKind::Fn(f) => self.fn_decl(f),
            ItemKind::Type(t) => {
                self.open("type");
                self.name(&t.name);
                self.generics(&t.generics);
                match &t.body {
                    TypeBody::Alias(ty) => {
                        self.s(" =");
                        self.ty(ty);
                    }
                    TypeBody::Record(fields) => {
                        for f in fields {
                            self.open(" field");
                            self.attrs(&f.attrs);
                            self.ident(&f.name);
                            self.ty(&f.ty);
                            self.opt(&f.default, |w, e| w.expr(e));
                            self.close();
                        }
                    }
                }
                self.close();
            }
            ItemKind::Enum(e) => {
                self.open("enum");
                self.name(&e.name);
                self.generics(&e.generics);
                for v in &e.variants {
                    self.open(" variant");
                    self.attrs(&v.attrs);
                    self.ident(&v.name);
                    if let Some(fields) = &v.fields {
                        self.s(" (");
                        for f in fields {
                            self.open("vf");
                            self.opt(&f.label, |w, l| w.ident(l));
                            self.ty(&f.ty);
                            self.close();
                        }
                        self.s(")");
                    }
                    self.close();
                }
                self.close();
            }
            ItemKind::Trait(t) => {
                self.open("trait");
                self.name(&t.name);
                self.generics(&t.generics);
                for m in &t.members {
                    self.sp();
                    self.fn_decl(m);
                }
                self.close();
            }
            ItemKind::Impl(im) => {
                self.open("impl");
                self.generics(&im.generics);
                self.ty(&im.ty);
                self.opt(&im.as_trait, |w, t| {
                    w.s(" as");
                    w.ty(t)
                });
                for m in &im.members {
                    self.open(" member");
                    if m.export {
                        self.s(" export");
                    }
                    self.attrs(&m.attrs);
                    self.sp();
                    self.fn_decl(&m.func);
                    self.close();
                }
                self.close();
            }
            ItemKind::Const(c) => {
                self.open("const");
                self.name(&c.name);
                self.opt(&c.ty, |w, t| w.ty(t));
                self.expr(&c.value);
                self.close();
            }
            ItemKind::Extern(e) => {
                self.open("extern");
                match e {
                    ExternDecl::Fn(sig) => {
                        self.sp();
                        self.fn_sig(sig);
                    }
                    ExternDecl::Type { name, generics } => {
                        self.s(" type");
                        self.name(name);
                        self.generics(generics);
                    }
                }
                self.close();
            }
            ItemKind::Test(t) => {
                self.open("test");
                self.str_lit(&t.name);
                self.block(&t.body);
                self.close();
            }
            ItemKind::Error => self.s("(error)"),
        }
        self.close();
    }

    fn generics(&mut self, g: &[GenericParam]) {
        if g.is_empty() {
            return;
        }
        self.s(" [");
        for p in g {
            self.open("gp");
            self.ident(&p.name);
            for b in &p.bounds {
                self.ty(b);
            }
            self.close();
        }
        self.s("]");
    }

    fn fn_sig(&mut self, sig: &FnSig) {
        self.open("sig");
        self.name(&sig.name);
        self.generics(&sig.generics);
        self.s(" (");
        for p in &sig.params {
            match p {
                Param::SelfParam(_) => self.s("(self)"),
                Param::Named {
                    name, ty, default, ..
                } => {
                    self.open("param");
                    self.ident(name);
                    self.ty(ty);
                    self.opt(default, |w, e| w.expr(e));
                    self.close();
                }
            }
        }
        self.s(")");
        self.opt(&sig.ret, |w, t| {
            w.s(" ->");
            w.ty(t)
        });
        self.close();
    }

    fn fn_decl(&mut self, f: &FnDecl) {
        self.open("fn ");
        self.fn_sig(&f.sig);
        match &f.body {
            FnBody::Block(b) => self.block(b),
            FnBody::Expr(e) => {
                self.s(" =>");
                self.expr(e);
            }
            FnBody::None => self.s(" required"),
        }
        self.close();
    }

    fn block(&mut self, b: &Block) {
        self.open(if b.inline { " line" } else { " block" });
        for s in &b.stmts {
            self.sp();
            self.stmt(s);
        }
        self.close();
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Expr(e) => {
                self.open("expr");
                self.expr(e);
                self.close();
            }
            StmtKind::Let { pat, ty, value } => {
                self.open("let");
                self.pat(pat);
                self.opt(ty, |w, t| w.ty(t));
                self.expr(value);
                self.close();
            }
            StmtKind::Var { name, ty, value } => {
                self.open("var");
                self.ident(name);
                self.opt(ty, |w, t| w.ty(t));
                self.expr(value);
                self.close();
            }
            StmtKind::Assign { place, op, value } => {
                self.open(match op {
                    AssignOp::Assign => "=",
                    AssignOp::Add => "+=",
                    AssignOp::Sub => "-=",
                    AssignOp::Mul => "*=",
                    AssignOp::Div => "/=",
                });
                self.expr(place);
                self.expr(value);
                self.close();
            }
            StmtKind::Return(e) => {
                self.open("return");
                if let Some(e) = e {
                    self.expr(e);
                }
                self.close();
            }
            StmtKind::Break => self.s("(break)"),
            StmtKind::Continue => self.s("(continue)"),
            StmtKind::Pass => self.s("(pass)"),
            StmtKind::For { pat, iter, body } => {
                self.open("for");
                self.pat(pat);
                self.expr(iter);
                self.block(body);
                self.close();
            }
            StmtKind::While { cond, body } => {
                self.open("while");
                self.expr(cond);
                self.block(body);
                self.close();
            }
            StmtKind::Fn(f) => self.fn_decl(f),
            StmtKind::Error => self.s("(error)"),
        }
    }

    fn str_lit(&mut self, s: &StrLit) {
        self.open(if s.triple { " str3" } else { " str" });
        for p in &s.parts {
            match p {
                StrPart::Text(t) => self.s(&format!(" {t:?}")),
                StrPart::Hole(e) => {
                    self.open(" hole");
                    self.expr(e);
                    self.close();
                }
            }
        }
        self.close();
    }

    fn args(&mut self, args: &[Arg]) {
        self.s(" (args");
        for a in args {
            self.open(" arg");
            if let Some(l) = &a.label {
                self.ident(l);
                self.s(":");
            }
            self.expr(&a.value);
            self.close();
        }
        self.s(")");
    }

    /// Expressions are the only part of the tree that nests up to the parser's height cap
    /// (operator and postfix chains), so this is where the walk is made stack-safe.
    fn expr(&mut self, e: &Expr) {
        crate::stack::deeper(|| self.expr_level(e));
    }

    fn expr_level(&mut self, e: &Expr) {
        self.sp();
        match &e.kind {
            ExprKind::Int(i) => self.s(&format!("(int {})", i.text)),
            ExprKind::Float(f) => self.s(&format!("(float {})", f.text)),
            ExprKind::Str(s) => {
                self.out.pop();
                self.str_lit(s);
            }
            ExprKind::Tagged { tag, lit } => {
                self.s(&format!("(tagged {tag}"));
                self.str_lit(lit);
                self.close();
            }
            ExprKind::Bool(b) => self.s(if *b { "true" } else { "false" }),
            ExprKind::Name(n) => {
                self.out.pop();
                self.name_kind(n);
            }
            ExprKind::SelfValue => self.s("self"),
            ExprKind::ImplicitMember(i) => {
                self.s("(dot");
                self.ident(i);
                self.close();
            }
            ExprKind::UnsafeMember(i) => {
                self.s("(unsafe");
                self.ident(i);
                self.close();
            }
            ExprKind::Unit => self.s("()"),
            ExprKind::Paren(inner) => {
                self.s("(paren");
                self.expr(inner);
                self.close();
            }
            ExprKind::Tuple(xs) => {
                self.s("(tuple");
                for x in xs {
                    self.expr(x);
                }
                self.close();
            }
            ExprKind::List(xs) => {
                self.s("(list");
                for x in xs {
                    self.expr(x);
                }
                self.close();
            }
            ExprKind::Map(kvs) => {
                self.s("(map");
                for (k, v) in kvs {
                    self.s(" (kv");
                    self.expr(k);
                    self.expr(v);
                    self.close();
                }
                self.close();
            }
            ExprKind::Call { callee, args } => {
                self.s("(call");
                self.expr(callee);
                self.args(args);
                self.close();
            }
            ExprKind::Index { base, index } => {
                self.s("(index");
                self.expr(base);
                self.expr(index);
                self.close();
            }
            ExprKind::Member { base, member } => {
                self.s("(.");
                self.expr(base);
                match member {
                    Member::Name(i) => self.ident(i),
                    Member::Index { text, .. } => self.s(&format!(" {text}")),
                }
                self.close();
            }
            ExprKind::Try(inner) => {
                self.s("(?");
                self.expr(inner);
                self.close();
            }
            ExprKind::Neg(inner) => {
                self.s("(neg");
                self.expr(inner);
                self.close();
            }
            ExprKind::Not(inner) => {
                self.s("(not");
                self.expr(inner);
                self.close();
            }
            ExprKind::Binary { op, lhs, rhs } => {
                self.s(&format!("({}", binop(*op)));
                self.expr(lhs);
                self.expr(rhs);
                self.close();
            }
            ExprKind::Range { lo, hi, inclusive } => {
                self.s(if *inclusive { "(..=" } else { "(.." });
                self.expr(lo);
                self.expr(hi);
                self.close();
            }
            ExprKind::Lambda(l) => {
                self.s("(lambda (");
                for p in &l.params {
                    self.open("lp");
                    self.ident(&p.name);
                    self.opt(&p.ty, |w, t| w.ty(t));
                    self.close();
                }
                self.s(")");
                self.opt(&l.ret, |w, t| {
                    w.s(" ->");
                    w.ty(t)
                });
                self.expr(&l.body);
                self.close();
            }
            ExprKind::If(i) => {
                self.s(if i.inline { "(if-inline" } else { "(if" });
                for b in &i.branches {
                    self.s(" (branch");
                    self.expr(&b.cond);
                    self.block(&b.body);
                    self.close();
                }
                if let Some(e) = &i.else_body {
                    self.s(" (else");
                    self.block(e);
                    self.close();
                }
                self.close();
            }
            ExprKind::Match(m) => {
                self.s("(match");
                self.expr(&m.scrutinee);
                for arm in &m.arms {
                    self.s(" (case");
                    self.pat(&arm.pat);
                    if let Some(g) = &arm.guard {
                        self.s(" if");
                        self.expr(g);
                    }
                    self.block(&arm.body);
                    self.close();
                }
                self.close();
            }
            ExprKind::Error => self.s("(error)"),
        }
    }

    fn int_bound(&mut self, b: &IntPatBound) {
        self.s(if b.negative { " -" } else { " " });
        self.s(&b.lit.text);
    }

    fn pat_fields(&mut self, f: &Option<PatFields>) {
        if let Some(f) = f {
            self.s(" (");
            for pf in &f.fields {
                self.open("pf");
                if let Some(l) = &pf.label {
                    self.ident(l);
                    self.s(":");
                }
                self.pat(&pf.pat);
                self.close();
            }
            if f.rest {
                self.s(" ..");
            }
            self.s(")");
        }
    }

    fn pat(&mut self, p: &Pat) {
        self.sp();
        match &p.kind {
            PatKind::Wild => self.s("_"),
            PatKind::Bind(i) => {
                self.s("(bind");
                self.ident(i);
                self.close();
            }
            PatKind::Int(b) => {
                self.s("(int");
                self.int_bound(b);
                self.close();
            }
            PatKind::Range { lo, hi, inclusive } => {
                self.s(if *inclusive { "(range=" } else { "(range" });
                self.int_bound(lo);
                self.int_bound(hi);
                self.close();
            }
            PatKind::Str(s) => {
                self.out.pop();
                self.str_lit(s);
            }
            PatKind::Bool(b) => self.s(if *b { "true" } else { "false" }),
            PatKind::Implicit { name, fields } => {
                self.s("(.");
                self.ident(name);
                self.pat_fields(fields);
                self.close();
            }
            PatKind::Path { path, fields } => {
                self.s("(path");
                for n in path {
                    self.name(n);
                }
                self.pat_fields(fields);
                self.close();
            }
            PatKind::Ctor { ctor, inner } => {
                self.s(&format!("({}", ctor.name()));
                if let Some(i) = inner {
                    self.pat(i);
                }
                self.close();
            }
            PatKind::Tuple(ps) => {
                self.s("(tuple");
                for x in ps {
                    self.pat(x);
                }
                self.close();
            }
            PatKind::List(elems) => {
                self.s("(list");
                for e in elems {
                    match e {
                        ListPatElem::Pat(p) => self.pat(p),
                        ListPatElem::Rest(name) => {
                            self.s(" (..");
                            if let Some(n) = name {
                                self.ident(n);
                            }
                            self.close();
                        }
                    }
                }
                self.close();
            }
            PatKind::Or(ps) => {
                self.s("(or");
                for x in ps {
                    self.pat(x);
                }
                self.close();
            }
            PatKind::As { pat, name } => {
                self.s("(as");
                self.pat(pat);
                self.ident(name);
                self.close();
            }
            PatKind::Error => self.s("(error)"),
        }
    }

    fn ty(&mut self, t: &Type) {
        self.sp();
        match &t.kind {
            TypeKind::Named { path, args } => {
                self.s("(t");
                for n in path {
                    self.name(n);
                }
                if !args.is_empty() {
                    self.s(" [");
                    for a in args {
                        self.ty(a);
                    }
                    self.s("]");
                }
                self.close();
            }
            TypeKind::Unit => self.s("()"),
            TypeKind::Tuple(ts) => {
                self.s("(tuple-t");
                for x in ts {
                    self.ty(x);
                }
                self.close();
            }
            TypeKind::Fn { sync, params, ret } => {
                self.s(if *sync { "(sync-fn (" } else { "(fn-t (" });
                for x in params {
                    self.ty(x);
                }
                self.s(") ->");
                self.ty(ret);
                self.close();
            }
            TypeKind::Error => self.s("(error)"),
        }
    }
}

fn binop(op: BinOp) -> &'static str {
    match op {
        BinOp::Or => "or",
        BinOp::And => "and",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::IntDiv => "//",
        BinOp::Rem => "%",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp() -> Span {
        Span::default()
    }

    fn ident(n: &str, span: Span) -> Ident {
        Ident {
            name: n.into(),
            span,
        }
    }

    fn tiny(span: Span) -> Ast {
        let call = Expr {
            kind: ExprKind::Call {
                callee: Box::new(Expr {
                    kind: ExprKind::Name(NameKind::Core(Concept::CorePrint)),
                    span,
                }),
                args: vec![Arg {
                    label: None,
                    value: Expr {
                        kind: ExprKind::Name(NameKind::Ident("x".into())),
                        span,
                    },
                    span,
                }],
            },
            span,
        };
        let f = FnDecl {
            sig: FnSig {
                name: Name {
                    kind: NameKind::Core(Concept::CoreMain),
                    span,
                },
                generics: vec![],
                params: vec![],
                ret: None,
                span,
            },
            body: FnBody::Block(Block {
                stmts: vec![Stmt {
                    kind: StmtKind::Let {
                        pat: Pat {
                            kind: PatKind::Bind(ident("x", span)),
                            span,
                        },
                        ty: None,
                        value: call,
                    },
                    span,
                }],
                inline: false,
                span,
            }),
            span,
        };
        Ast {
            file: File {
                uses: vec![],
                items: vec![Item {
                    attrs: vec![],
                    vis: Visibility::Private,
                    kind: ItemKind::Fn(f),
                    span,
                }],
                span,
            },
            trivia: vec![],
        }
    }

    #[test]
    fn shape_ignores_spans_and_names_concepts() {
        let a = shape(&tiny(sp()), ShapeOptions::default());
        let b = shape(&tiny(Span::new(3, 9)), ShapeOptions::default());
        assert_eq!(a, b);
        assert!(a.contains("CORE_MAIN"), "{a}");
        assert!(a.contains("(call CORE_PRINT (args (arg \"x\")))"), "{a}");
    }

    #[test]
    fn erase_idents_hides_identifier_text_only() {
        let s = shape(
            &tiny(sp()),
            ShapeOptions {
                erase_idents: true,
                skip_trivia: false,
            },
        );
        assert!(!s.contains("\"x\""), "{s}");
        assert!(s.contains("CORE_PRINT"), "{s}");
    }
}
