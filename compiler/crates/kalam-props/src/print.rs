//! A reference printer from AST to source text in one skin, written independently of
//! `kalam-fmt` (so it is an oracle for the parser, not a copy of the formatter).
//!
//! It draws from two random streams:
//! - **layout** (line breaks inside brackets, one-line blocks, blank lines, comments): the
//!   same seed in every skin gives the same layout, so the three printouts of one AST have
//!   equal KCF (spec §4.4);
//! - **spelling** (Franco variants, Arabic hamza/alef and Persian/Urdu letter forms, `،`/`؟`,
//!   Arabic-Indic digits, needless backticks): free per skin, invisible to the AST and KCF.

use kalam_ast::*;
use kalam_base::Skin;
use kalam_skins::{Concept, KeywordTable};

use crate::rng::Rng;

const COMMENT_WORDS: &[&str] = &["note", "TODO: law fi lekol", "تعليق", "if x: y", "a, b?"];

/// Prints `ast` in `skin`.
pub fn print(ast: &Ast, skin: Skin, layout_seed: u64, spell_seed: u64) -> String {
    let mut p = Printer {
        skin,
        table: KeywordTable::builtin(),
        lay: Rng::new(layout_seed),
        spell: Rng::new(spell_seed),
        out: String::new(),
        comments: 0,
    };
    p.file(&ast.file);
    p.out
}

struct Printer<'t> {
    skin: Skin,
    table: &'t KeywordTable,
    lay: Rng,
    spell: Rng,
    out: String,
    comments: u32,
}

fn pad(ind: usize) -> String {
    " ".repeat(ind)
}

impl Printer<'_> {
    // ----- spellings ------------------------------------------------------------------------

    fn kw(&mut self, c: Concept) -> String {
        let canonical = self.table.spelling(c, self.skin).to_string();
        match self.skin {
            Skin::Franco if self.spell.chance(35) => {
                let aliases = &self.table.row(c).franco_aliases;
                if aliases.is_empty() {
                    canonical
                } else {
                    aliases[self.spell.below(aliases.len())].clone()
                }
            }
            Skin::Ar if self.spell.chance(35) => {
                let v: String = canonical
                    .chars()
                    .map(|ch| match ch {
                        'أ' | 'إ' | 'آ' => 'ا',
                        'ي' => *self.spell.pick(&['ي', '\u{06CC}', 'ى']),
                        'ك' => *self.spell.pick(&['ك', '\u{06A9}']),
                        'ة' => *self.spell.pick(&['ة', 'ه', '\u{06D5}']),
                        'ه' => *self.spell.pick(&['ه', '\u{06BE}']),
                        other => other,
                    })
                    .collect();
                let same = self
                    .table
                    .skin(Skin::Ar)
                    .lookup_word(&v)
                    .is_some_and(|m| m.concept == c);
                if same {
                    v
                } else {
                    canonical
                }
            }
            _ => canonical,
        }
    }

    fn comma(&mut self) -> &'static str {
        if self.skin == Skin::Ar && self.spell.chance(70) {
            "\u{060C}"
        } else {
            ","
        }
    }

    fn try_op(&mut self) -> &'static str {
        if self.skin == Skin::Ar && self.spell.chance(70) {
            "\u{061F}"
        } else {
            "?"
        }
    }

    fn reserved(&self, name: &str) -> bool {
        self.table.skin(self.skin).is_reserved(name)
    }

    /// An identifier where a reserved word must be escaped.
    fn bound(&mut self, name: &str) -> String {
        if name != "_" && (self.reserved(name) || self.spell.chance(5)) {
            format!("`{name}`")
        } else {
            name.to_string()
        }
    }

    /// An identifier in a §5.3 rule 8 position (after `.`, a named-argument label, a record
    /// field name in a declaration): a reserved word needs no escape, but may have one.
    fn free(&mut self, name: &str) -> String {
        let esc = if self.reserved(name) {
            self.spell.chance(50)
        } else {
            self.spell.chance(5)
        };
        if esc {
            format!("`{name}`")
        } else {
            name.to_string()
        }
    }

    fn name(&mut self, n: &NameKind) -> String {
        match n {
            NameKind::Ident(s) => self.bound(s),
            NameKind::Core(c) => self.kw(*c),
        }
    }

    fn digits(&mut self, ascii: &str) -> String {
        let plain = ascii
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_' || c == '.');
        if self.skin == Skin::Ar && plain && self.spell.chance(40) {
            ascii
                .chars()
                .map(|c| match c {
                    '0'..='9' => char::from_u32(0x0660 + (c as u32 - '0' as u32)).unwrap_or(c),
                    '.' => '\u{066B}',
                    other => other,
                })
                .collect()
        } else {
            ascii.to_string()
        }
    }

    // ----- layout ---------------------------------------------------------------------------

    fn comment(&mut self, ind: usize) {
        if self.lay.chance(6) {
            self.comments += 1;
            let w = *self.lay.pick(COMMENT_WORDS);
            let n = self.comments;
            self.out.push_str(&format!("{}# c{n} {w}\n", pad(ind)));
        }
    }

    /// A bracketed, comma-separated list, sometimes broken over lines (newlines and
    /// indentation inside brackets are ignored, spec §5.8).
    fn bracket(
        &mut self,
        open: &str,
        items: Vec<String>,
        close: &str,
        ind: usize,
        hole: bool,
        force_trailing: bool,
    ) -> String {
        let broken = !hole && self.lay.chance(8);
        let trailing = force_trailing || (broken && self.lay.chance(50));
        if items.is_empty() {
            return format!("{open}{close}");
        }
        let n = items.len();
        let mut s = String::from(open);
        for (i, it) in items.into_iter().enumerate() {
            if broken {
                s.push('\n');
                s.push_str(&pad(ind + 4 + self.lay.below(3)));
            } else if i > 0 {
                s.push(' ');
            }
            s.push_str(&it);
            if i + 1 < n || trailing {
                s.push_str(self.comma());
            }
        }
        if broken {
            s.push('\n');
            s.push_str(&pad(ind));
        }
        s.push_str(close);
        s
    }

    // ----- items ----------------------------------------------------------------------------

    fn file(&mut self, f: &File) {
        for u in &f.uses {
            self.comment(0);
            self.use_decl(u);
        }
        for (i, it) in f.items.iter().enumerate() {
            if i > 0 || !f.uses.is_empty() {
                let blanks = 1 + self.lay.below(2);
                for _ in 0..blanks {
                    self.out.push('\n');
                }
            }
            self.comment(0);
            self.item(it);
        }
    }

    fn use_decl(&mut self, u: &UseDecl) {
        let kw = self.kw(Concept::KwUse);
        let path: Vec<&str> = u.path.iter().map(|i| i.name.as_str()).collect();
        let mut s = format!("{kw} {}", path.join("/"));
        match &u.tail {
            UseTail::None => {}
            UseTail::Alias(a) => {
                let as_ = self.kw(Concept::KwAs);
                let a = self.bound(&a.name);
                s.push_str(&format!(" {as_} {a}"));
            }
            UseTail::Items(items) => {
                let mut parts = Vec::new();
                for it in items {
                    parts.push(match it {
                        UseItem::SelfModule(_) => self.kw(Concept::KwSelf),
                        UseItem::Name { name, alias } => {
                            let n = self.bound(&name.name);
                            match alias {
                                Some(a) => {
                                    let as_ = self.kw(Concept::KwAs);
                                    let a = self.bound(&a.name);
                                    format!("{n} {as_} {a}")
                                }
                                None => n,
                            }
                        }
                    });
                }
                let b = self.bracket("{", parts, "}", 0, false, false);
                s.push('.');
                s.push_str(&b);
            }
        }
        self.out.push_str(&s);
        self.out.push('\n');
    }

    fn vis(&mut self, v: Visibility) -> String {
        match v {
            Visibility::Private => String::new(),
            Visibility::Export => format!("{} ", self.kw(Concept::KwExport)),
            Visibility::ExportOpaque => format!(
                "{} {} ",
                self.kw(Concept::KwExport),
                self.kw(Concept::KwOpaque)
            ),
        }
    }

    fn item(&mut self, it: &Item) {
        let vis = self.vis(it.vis);
        self.out.push_str(&vis);
        match &it.kind {
            ItemKind::Fn(f) => self.fn_decl(f, 0),
            ItemKind::Const(c) => {
                let kw = self.kw(Concept::KwLet);
                let n = self.name(&c.name.kind);
                let t = match &c.ty {
                    Some(t) => format!(": {}", self.ty(t, 0)),
                    None => String::new(),
                };
                let v = self.expr(&c.value, 0, false);
                self.out.push_str(&format!("{kw} {n}{t} = {v}\n"));
            }
            ItemKind::Type(t) => {
                let kw = self.kw(Concept::KwType);
                let n = self.name(&t.name.kind);
                let g = self.generics(&t.generics);
                match &t.body {
                    TypeBody::Alias(a) => {
                        let a = self.ty(a, 0);
                        self.out.push_str(&format!("{kw} {n}{g} = {a}\n"));
                    }
                    TypeBody::Record(fields) => {
                        self.out.push_str(&format!("{kw} {n}{g}:\n"));
                        for f in fields {
                            self.comment(4);
                            let name = self.free(&f.name.name);
                            let ty = self.ty(&f.ty, 4);
                            let d = match &f.default {
                                Some(e) => format!(" = {}", self.expr(e, 4, false)),
                                None => String::new(),
                            };
                            self.out.push_str(&format!("    {name}: {ty}{d}\n"));
                        }
                    }
                }
            }
            ItemKind::Enum(e) => {
                let kw = self.kw(Concept::KwEnum);
                let n = self.name(&e.name.kind);
                let g = self.generics(&e.generics);
                self.out.push_str(&format!("{kw} {n}{g}:\n"));
                for v in &e.variants {
                    self.comment(4);
                    let name = self.bound(&v.name.name);
                    let fields = match &v.fields {
                        None => String::new(),
                        Some(fs) => {
                            let mut parts = Vec::new();
                            for f in fs {
                                let t = self.ty(&f.ty, 4);
                                parts.push(match &f.label {
                                    Some(l) => format!("{}: {t}", self.bound(&l.name)),
                                    None => t,
                                });
                            }
                            self.bracket("(", parts, ")", 4, false, false)
                        }
                    };
                    self.out.push_str(&format!("    {name}{fields}\n"));
                }
            }
            ItemKind::Test(t) => {
                let kw = self.kw(Concept::KwTest);
                let s = self.str_lit(&t.name, 0, false);
                self.out.push_str(&format!("{kw} {s}"));
                self.block(&t.body, 0);
            }
            other => unreachable!("the generator does not build {other:?}"),
        }
    }

    fn generics(&mut self, gs: &[GenericParam]) -> String {
        if gs.is_empty() {
            return String::new();
        }
        let mut parts = Vec::new();
        for g in gs {
            let n = self.bound(&g.name.name);
            if g.bounds.is_empty() {
                parts.push(n);
            } else {
                let bs: Vec<String> = g.bounds.iter().map(|b| self.ty(b, 0)).collect();
                parts.push(format!("{n}: {}", bs.join(" + ")));
            }
        }
        self.bracket("[", parts, "]", 0, false, false)
    }

    fn fn_decl(&mut self, f: &FnDecl, ind: usize) {
        let kw = self.kw(Concept::KwFn);
        let n = self.name(&f.sig.name.kind);
        let g = self.generics(&f.sig.generics);
        let mut params = Vec::new();
        for p in &f.sig.params {
            match p {
                Param::SelfParam(_) => params.push(self.kw(Concept::KwSelf)),
                Param::Named {
                    name, ty, default, ..
                } => {
                    let n = self.bound(&name.name);
                    let t = self.ty(ty, ind);
                    let d = match default {
                        Some(e) => format!(" = {}", self.expr(e, ind, false)),
                        None => String::new(),
                    };
                    params.push(format!("{n}: {t}{d}"));
                }
            }
        }
        let ps = self.bracket("(", params, ")", ind, false, false);
        let ret = match &f.sig.ret {
            Some(t) => format!(" -> {}", self.ty(t, ind)),
            None => String::new(),
        };
        self.out.push_str(&format!("{kw} {n}{g}{ps}{ret}"));
        match &f.body {
            FnBody::Block(b) => self.block(b, ind),
            FnBody::Expr(e) => {
                let e = self.expr(e, ind, false);
                self.out.push_str(&format!(" => {e}\n"));
            }
            FnBody::None => self.out.push('\n'),
        }
    }

    // ----- blocks and statements ------------------------------------------------------------

    /// Writes `:` and the block (the header is already on the current line).
    fn block(&mut self, b: &Block, ind: usize) {
        let one_line = self.lay.chance(25) && b.stmts.len() == 1 && simple(&b.stmts[0]);
        if one_line {
            self.out.push_str(": ");
            self.stmt_body(&b.stmts[0], ind);
            return;
        }
        self.out.push_str(":\n");
        let inner = ind + 4;
        for (i, s) in b.stmts.iter().enumerate() {
            if i > 0 && self.lay.chance(5) {
                self.out.push('\n');
            }
            self.comment(inner);
            self.out.push_str(&pad(inner));
            self.stmt_body(s, inner);
        }
    }

    /// Writes one statement starting at the current position; `ind` is its line's indentation.
    fn stmt_body(&mut self, s: &Stmt, ind: usize) {
        match &s.kind {
            StmtKind::Expr(e) => self.rhs(e, ind),
            StmtKind::Let { pat, ty, value } => {
                let kw = self.kw(Concept::KwLet);
                let p = self.pat(pat, ind);
                let t = match ty {
                    Some(t) => format!(": {}", self.ty(t, ind)),
                    None => String::new(),
                };
                self.out.push_str(&format!("{kw} {p}{t} = "));
                self.rhs(value, ind);
            }
            StmtKind::Var { name, ty, value } => {
                let kw = self.kw(Concept::KwVar);
                let n = self.bound(&name.name);
                let t = match ty {
                    Some(t) => format!(": {}", self.ty(t, ind)),
                    None => String::new(),
                };
                self.out.push_str(&format!("{kw} {n}{t} = "));
                self.rhs(value, ind);
            }
            StmtKind::Assign { place, op, value } => {
                let p = self.expr(place, ind, false);
                let op = match op {
                    AssignOp::Assign => "=",
                    AssignOp::Add => "+=",
                    AssignOp::Sub => "-=",
                    AssignOp::Mul => "*=",
                    AssignOp::Div => "/=",
                };
                self.out.push_str(&format!("{p} {op} "));
                self.rhs(value, ind);
            }
            StmtKind::Return(v) => {
                let kw = self.kw(Concept::KwReturn);
                self.out.push_str(&kw);
                match v {
                    Some(e) => {
                        self.out.push(' ');
                        self.rhs(e, ind);
                    }
                    None => self.out.push('\n'),
                }
            }
            StmtKind::Break => self.word_line(Concept::KwBreak),
            StmtKind::Continue => self.word_line(Concept::KwContinue),
            StmtKind::Pass => self.word_line(Concept::KwPass),
            StmtKind::For { pat, iter, body } => {
                let kw = self.kw(Concept::KwFor);
                let p = self.pat(pat, ind);
                let in_ = self.kw(Concept::KwIn);
                let it = self.expr(iter, ind, false);
                self.out.push_str(&format!("{kw} {p} {in_} {it}"));
                self.block(body, ind);
            }
            StmtKind::While { cond, body } => {
                let kw = self.kw(Concept::KwWhile);
                let c = self.expr(cond, ind, false);
                self.out.push_str(&format!("{kw} {c}"));
                self.block(body, ind);
            }
            StmtKind::Fn(f) => self.fn_decl(f, ind),
            StmtKind::Error => unreachable!("the generator does not build error statements"),
        }
    }

    fn word_line(&mut self, c: Concept) {
        let w = self.kw(c);
        self.out.push_str(&w);
        self.out.push('\n');
    }

    /// A value or expression statement: a multi-line `if`/`match` form, or one line.
    fn rhs(&mut self, e: &Expr, ind: usize) {
        match &e.kind {
            ExprKind::If(i) if !i.inline => {
                for (n, br) in i.branches.iter().enumerate() {
                    if n > 0 {
                        let el = self.kw(Concept::KwElse);
                        self.out.push_str(&format!("{}{el} ", pad(ind)));
                    }
                    let kw = self.kw(Concept::KwIf);
                    let c = self.expr(&br.cond, ind, false);
                    self.out.push_str(&format!("{kw} {c}"));
                    self.block(&br.body, ind);
                }
                if let Some(b) = &i.else_body {
                    let el = self.kw(Concept::KwElse);
                    self.out.push_str(&format!("{}{el}", pad(ind)));
                    self.block(b, ind);
                }
            }
            ExprKind::Match(m) => {
                let kw = self.kw(Concept::KwMatch);
                let s = self.expr(&m.scrutinee, ind, false);
                self.out.push_str(&format!("{kw} {s}:\n"));
                let arm_ind = ind + 4;
                for arm in &m.arms {
                    self.comment(arm_ind);
                    let case = self.kw(Concept::KwCase);
                    let p = self.pat(&arm.pat, arm_ind);
                    let g = match &arm.guard {
                        Some(g) => {
                            let if_ = self.kw(Concept::KwIf);
                            format!(" {if_} {}", self.expr(g, arm_ind, false))
                        }
                        None => String::new(),
                    };
                    self.out.push_str(&format!("{}{case} {p}{g}", pad(arm_ind)));
                    self.block(&arm.body, arm_ind);
                }
            }
            _ => {
                let s = self.expr(e, ind, false);
                self.out.push_str(&s);
                self.out.push('\n');
            }
        }
    }

    // ----- expressions ----------------------------------------------------------------------

    fn str_lit(&mut self, s: &StrLit, ind: usize, _hole: bool) -> String {
        let mut out = String::from("\"");
        for part in &s.parts {
            match part {
                StrPart::Text(t) => out.push_str(t),
                StrPart::Hole(e) => {
                    let h = self.expr(e, ind, true);
                    out.push('{');
                    out.push_str(&h);
                    out.push('}');
                }
            }
        }
        out.push('"');
        out
    }

    fn list(&mut self, es: &[Expr], ind: usize, hole: bool) -> Vec<String> {
        es.iter().map(|e| self.expr(e, ind, hole)).collect()
    }

    fn expr(&mut self, e: &Expr, ind: usize, hole: bool) -> String {
        match &e.kind {
            ExprKind::Int(i) => self.digits(&i.text),
            ExprKind::Float(f) => self.digits(&f.text),
            ExprKind::Str(s) => self.str_lit(s, ind, hole),
            ExprKind::Tagged { tag, lit } => {
                let s = self.str_lit(lit, ind, hole);
                format!("{tag}{s}")
            }
            ExprKind::Bool(b) => self.kw(if *b {
                Concept::KwTrue
            } else {
                Concept::KwFalse
            }),
            ExprKind::Name(n) => self.name(n),
            ExprKind::SelfValue => self.kw(Concept::KwSelf),
            ExprKind::ImplicitMember(i) => format!(".{}", self.free(&i.name)),
            ExprKind::UnsafeMember(i) => {
                format!("{}.{}", self.kw(Concept::KwUnsafe), self.free(&i.name))
            }
            ExprKind::Unit => "()".to_string(),
            ExprKind::Paren(inner) => format!("({})", self.expr(inner, ind, hole)),
            ExprKind::Tuple(es) => {
                let items = self.list(es, ind, hole);
                self.bracket("(", items, ")", ind, hole, es.len() == 1)
            }
            ExprKind::List(es) => {
                let items = self.list(es, ind, hole);
                self.bracket("[", items, "]", ind, hole, false)
            }
            ExprKind::Map(kvs) => {
                let mut items = Vec::new();
                for (k, v) in kvs {
                    let k = self.expr(k, ind, hole);
                    let v = self.expr(v, ind, hole);
                    items.push(format!("{k}: {v}"));
                }
                self.bracket("{", items, "}", ind, hole, false)
            }
            ExprKind::Call { callee, args } => {
                let c = self.expr(callee, ind, hole);
                let mut items = Vec::new();
                for a in args {
                    let v = self.expr(&a.value, ind, hole);
                    items.push(match &a.label {
                        Some(l) => format!("{}: {v}", self.free(&l.name)),
                        None => v,
                    });
                }
                let b = self.bracket("(", items, ")", ind, hole, false);
                format!("{c}{b}")
            }
            ExprKind::Index { base, index } => {
                let b = self.expr(base, ind, hole);
                let i = self.expr(index, ind, hole);
                format!("{b}[{i}]")
            }
            ExprKind::Member { base, member } => {
                let b = self.expr(base, ind, hole);
                let m = match member {
                    Member::Name(i) => self.free(&i.name),
                    Member::Index { text, .. } => text.clone(),
                };
                format!("{b}.{m}")
            }
            ExprKind::Try(inner) => {
                let b = self.expr(inner, ind, hole);
                format!("{b}{}", self.try_op())
            }
            ExprKind::Neg(inner) => format!("-{}", self.expr(inner, ind, hole)),
            ExprKind::Not(inner) => {
                let kw = self.kw(Concept::KwNot);
                format!("{kw} {}", self.expr(inner, ind, hole))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr(lhs, ind, hole);
                let o = match op {
                    BinOp::Or => self.kw(Concept::KwOr),
                    BinOp::And => self.kw(Concept::KwAnd),
                    BinOp::Eq => "==".into(),
                    BinOp::NotEq => "!=".into(),
                    BinOp::Lt => "<".into(),
                    BinOp::Le => "<=".into(),
                    BinOp::Gt => ">".into(),
                    BinOp::Ge => ">=".into(),
                    BinOp::Add => "+".into(),
                    BinOp::Sub => "-".into(),
                    BinOp::Mul => "*".into(),
                    BinOp::Div => "/".into(),
                    BinOp::IntDiv => "//".into(),
                    BinOp::Rem => "%".into(),
                };
                let r = self.expr(rhs, ind, hole);
                format!("{l} {o} {r}")
            }
            ExprKind::Range { lo, hi, inclusive } => {
                let l = self.expr(lo, ind, hole);
                let h = self.expr(hi, ind, hole);
                format!("{l}{}{h}", if *inclusive { "..=" } else { ".." })
            }
            ExprKind::Lambda(l) => {
                let kw = self.kw(Concept::KwFn);
                let mut ps = Vec::new();
                for p in &l.params {
                    let n = self.bound(&p.name.name);
                    ps.push(match &p.ty {
                        Some(t) => format!("{n}: {}", self.ty(t, ind)),
                        None => n,
                    });
                }
                let ps = self.bracket("(", ps, ")", ind, hole, false);
                let ret = match &l.ret {
                    Some(t) => format!(" -> {}", self.ty(t, ind)),
                    None => String::new(),
                };
                let body = self.expr(&l.body, ind, hole);
                format!("{kw}{ps}{ret} => {body}")
            }
            ExprKind::If(i) => {
                // Only inline `if` reaches here (forms go through `rhs`).
                let mut s = String::new();
                for (n, br) in i.branches.iter().enumerate() {
                    if n > 0 {
                        s.push_str(&format!(" {} ", self.kw(Concept::KwElse)));
                    }
                    let kw = self.kw(Concept::KwIf);
                    let c = self.expr(&br.cond, ind, hole);
                    let arm = self.arm(&br.body, ind, hole);
                    s.push_str(&format!("{kw} {c}: {arm}"));
                }
                if let Some(b) = &i.else_body {
                    let el = self.kw(Concept::KwElse);
                    let arm = self.arm(b, ind, hole);
                    s.push_str(&format!(" {el}: {arm}"));
                }
                s
            }
            other => unreachable!("the generator does not build {other:?} here"),
        }
    }

    fn arm(&mut self, b: &Block, ind: usize, hole: bool) -> String {
        match b.stmts.first().map(|s| &s.kind) {
            Some(StmtKind::Expr(e)) => self.expr(e, ind, hole),
            _ => unreachable!("inline `if` arms hold one expression"),
        }
    }

    // ----- types and patterns ---------------------------------------------------------------

    fn ty(&mut self, t: &Type, ind: usize) -> String {
        match &t.kind {
            TypeKind::Named { path, args } => {
                let mut s = String::new();
                for (i, n) in path.iter().enumerate() {
                    if i > 0 {
                        s.push('.');
                        if let NameKind::Ident(x) = &n.kind {
                            s.push_str(&self.free(x));
                            continue;
                        }
                    }
                    s.push_str(&self.name(&n.kind));
                }
                if !args.is_empty() {
                    let items: Vec<String> = args.iter().map(|a| self.ty(a, ind)).collect();
                    s.push_str(&self.bracket("[", items, "]", ind, false, false));
                }
                s
            }
            TypeKind::Unit => "()".into(),
            TypeKind::Tuple(ts) => {
                let items: Vec<String> = ts.iter().map(|a| self.ty(a, ind)).collect();
                self.bracket("(", items, ")", ind, false, false)
            }
            TypeKind::Fn { sync, params, ret } => {
                let kw = self.kw(Concept::KwFn);
                let items: Vec<String> = params.iter().map(|a| self.ty(a, ind)).collect();
                let ps = self.bracket("(", items, ")", ind, false, false);
                let r = self.ty(ret, ind);
                format!("{}{kw}{ps} -> {r}", if *sync { "@sync " } else { "" })
            }
            TypeKind::Error => unreachable!("the generator does not build error types"),
        }
    }

    fn int_bound(&mut self, b: &IntPatBound) -> String {
        format!(
            "{}{}",
            if b.negative { "-" } else { "" },
            self.digits(&b.lit.text)
        )
    }

    fn pat_fields(&mut self, f: &PatFields, ind: usize) -> String {
        let mut items = Vec::new();
        for pf in &f.fields {
            let p = self.pat(&pf.pat, ind);
            items.push(match &pf.label {
                Some(l) => format!("{}: {p}", self.bound(&l.name)),
                None => p,
            });
        }
        if f.rest {
            items.push("..".into());
        }
        self.bracket("(", items, ")", ind, false, false)
    }

    fn pat(&mut self, p: &Pat, ind: usize) -> String {
        match &p.kind {
            PatKind::Wild => "_".into(),
            PatKind::Bind(i) => self.bound(&i.name),
            PatKind::Int(b) => self.int_bound(b),
            PatKind::Range { lo, hi, inclusive } => {
                let l = self.int_bound(lo);
                let h = self.int_bound(hi);
                format!("{l}{}{h}", if *inclusive { "..=" } else { ".." })
            }
            PatKind::Str(s) => self.str_lit(s, ind, false),
            PatKind::Bool(b) => self.kw(if *b {
                Concept::KwTrue
            } else {
                Concept::KwFalse
            }),
            PatKind::Implicit { name, fields } => {
                let n = self.free(&name.name);
                let f = match fields {
                    Some(f) => self.pat_fields(f, ind),
                    None => String::new(),
                };
                format!(".{n}{f}")
            }
            PatKind::Path { path, fields } => {
                let mut s = String::new();
                for (i, n) in path.iter().enumerate() {
                    if i > 0 {
                        s.push('.');
                        if let NameKind::Ident(x) = &n.kind {
                            s.push_str(&self.free(x));
                            continue;
                        }
                    }
                    s.push_str(&self.name(&n.kind));
                }
                if let Some(f) = fields {
                    s.push_str(&self.pat_fields(f, ind));
                }
                s
            }
            PatKind::Ctor { ctor, inner } => {
                let c = self.kw(*ctor);
                match inner {
                    Some(p) => format!("{c}({})", self.pat(p, ind)),
                    None => c,
                }
            }
            PatKind::Tuple(ps) => {
                let items: Vec<String> = ps.iter().map(|x| self.pat(x, ind)).collect();
                self.bracket("(", items, ")", ind, false, false)
            }
            PatKind::List(es) => {
                let mut items = Vec::new();
                for e in es {
                    items.push(match e {
                        ListPatElem::Pat(x) => self.pat(x, ind),
                        ListPatElem::Rest(None) => "..".into(),
                        ListPatElem::Rest(Some(n)) => format!("..{}", self.bound(&n.name)),
                    });
                }
                self.bracket("[", items, "]", ind, false, false)
            }
            PatKind::Or(ps) => {
                let items: Vec<String> = ps.iter().map(|x| self.pat(x, ind)).collect();
                items.join(" | ")
            }
            PatKind::As { pat, name } => {
                let p = self.pat(pat, ind);
                let as_ = self.kw(Concept::KwAs);
                let n = self.bound(&name.name);
                format!("{p} {as_} {n}")
            }
            PatKind::Error => unreachable!("the generator does not build error patterns"),
        }
    }
}

/// A statement that fits a one-line block (`: stmt`, spec §5.8 rule 4).
fn simple(s: &Stmt) -> bool {
    let form = |e: &Expr| {
        matches!(&e.kind, ExprKind::If(i) if !i.inline) || matches!(e.kind, ExprKind::Match(_))
    };
    match &s.kind {
        StmtKind::Expr(e)
        | StmtKind::Let { value: e, .. }
        | StmtKind::Var { value: e, .. }
        | StmtKind::Assign { value: e, .. }
        | StmtKind::Return(Some(e)) => !form(e),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Pass => true,
        _ => false,
    }
}
