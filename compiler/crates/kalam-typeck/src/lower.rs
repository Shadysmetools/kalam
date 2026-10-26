//! Lowering the resolved program to HIR: `kalam_resolve::Program` plus each module's
//! skin-free AST (`kalam_ast`) → [`HirProgram`] (ARCHITECTURE.md §8.1, "the adapter").
//!
//! Name resolution has already decided what every name means; this pass only reads its
//! tables, never re-resolves:
//!
//! - **Declarations** come from [`Program::decl_at`] (item names, methods, variants,
//!   parameters, `let`/`var`/pattern/lambda bindings, local `fn`s). An impl has no name, so
//!   its symbol is found by its span.
//! - **Uses** come from [`Program::use_at`]: a local becomes [`ExprKind::Local`], an item
//!   [`ExprKind::Def`], a core value name [`ExprKind::Lang`]. `Res::Error` (already
//!   reported) and opaque imports become [`ExprKind::Error`], which the checker treats as
//!   poison.
//! - `base.name` where resolve recorded `name` is a module member, a variant or an
//!   associated function (`Def`). Otherwise, when `base` statically denotes a type
//!   (`Decimal.from`, `HttpError.from`), it is [`ExprKind::AssocFn`]; when `base` is a value
//!   it is a field access, or a method call when it is called (DR-27 is the checker's).
//! - **Types**: core names and the fixed-ASCII prelude types become [`BuiltinType`], both
//!   as the built-in prelude symbols and as their stdlib stubs (`export extern type Int` in
//!   `std/prelude`); generic parameters become `TypeKind::Param(i)` (an impl's generics
//!   first, then the method's); `Self` becomes `TypeKind::SelfType`.
//! - **Built-in traits**: the marker traits (`Eq`, `Hash`, …) are prelude symbols, while
//!   `Show`, `Error`, `From` and `Template` are declared by `std/prelude` when the stdlib
//!   is loaded. Both map to [`TraitId::Builtin`]; the stdlib declaration supplies the
//!   methods ([`TraitItem::builtin`]).
//!
//! String literals are decoded here (escapes, and the indentation of `"""…"""`), so later
//! passes see the value the program means.

use std::collections::HashMap;

use kalam_ast as ast;
use kalam_base::{FileId, Span};
use kalam_resolve::{DefId, DefKind, ModulePath, Program, Res, SourceModule};
use kalam_skins::Concept;

use crate::hir::*;
use crate::traits::{BuiltinTrait, TraitId};
use crate::ty::{Capability, SinkType};

/// Builds the HIR of every loaded module of `program`. `sources` are the modules given to
/// `kalam_resolve::resolve_program`, with the same file ids.
pub fn lower_program(program: &Program, sources: &[SourceModule<'_>]) -> HirProgram {
    lower_program_with(program, sources, true)
}

/// [`lower_program`], optionally without the bodies of stdlib functions and tests: their
/// signatures (with which parameters and fields have defaults) are all a caller that does
/// not check the stdlib itself needs.
pub fn lower_program_with(
    program: &Program,
    sources: &[SourceModule<'_>],
    std_bodies: bool,
) -> HirProgram {
    let env = Env::new(program);
    let mut out = HirProgram::new();
    out.api_complete = program
        .module_by_path("std/prelude")
        .is_some_and(|m| m.loaded);
    out.world = program
        .core_stub(Concept::CoreWorld)
        .filter(|d| program.def(*d).kind == DefKind::Record);
    out.complete = program.modules.iter().all(|m| m.loaded);
    let mut next_synthetic = program.defs.len() as u32;
    for src in sources {
        out.skins.insert(src.file, src.skin);
        if !src.std {
            let comments = src
                .ast
                .trivia
                .iter()
                .filter(|t| {
                    matches!(
                        t.kind,
                        ast::TriviaKind::Comment | ast::TriviaKind::DocComment
                    )
                })
                .map(|t| t.span)
                .collect();
            out.comments.insert(src.file, comments);
        }
    }
    let mut order: Vec<&SourceModule<'_>> = sources.iter().collect();
    order.sort_by(|a, b| (&a.path, a.file).cmp(&(&b.path, b.file)));
    for src in order {
        // A module resolve did not load (a second skin file of one module, K0301) has no
        // declarations: skip it rather than guess.
        let loaded = program
            .modules
            .iter()
            .any(|m| m.loaded && m.file == Some(src.file));
        if !loaded {
            continue;
        }
        let mut m = ModuleLower {
            env: &env,
            file: src.file,
            module: src.path.clone(),
            out: &mut out,
            next_synthetic: &mut next_synthetic,
            bodies: std_bodies || !src.std,
            std: src.std,
        };
        for item in &src.ast.file.items {
            m.item(item);
        }
    }
    out
}

/// What the lowering needs to know about the program as a whole.
struct Env<'p> {
    program: &'p Program,
    builtin_types: HashMap<DefId, BuiltinType>,
    builtin_traits: HashMap<DefId, BuiltinTrait>,
    self_type: Option<DefId>,
    impls: HashMap<(FileId, Span), DefId>,
}

impl<'p> Env<'p> {
    fn new(program: &'p Program) -> Env<'p> {
        use BuiltinType as B;
        let mut builtin_types = HashMap::new();
        let core = [
            (Concept::CoreInt, B::Int),
            (Concept::CoreFloat, B::Float),
            (Concept::CoreDecimal, B::Decimal),
            (Concept::CoreBool, B::Bool),
            (Concept::CoreString, B::String),
            (Concept::CoreList, B::List),
            (Concept::CoreMap, B::Map),
            (Concept::CoreSet, B::Set),
            (Concept::CoreOption, B::Option),
            (Concept::CoreResult, B::Result),
            (Concept::CoreWorld, B::Capability(Capability::World)),
        ];
        for (c, b) in core {
            for d in [program.core(c), program.core_stub(c)]
                .into_iter()
                .flatten()
            {
                builtin_types.insert(d, b);
            }
        }
        let prelude = [
            ("BigInt", B::BigInt),
            ("Bytes", B::Bytes),
            ("Never", B::Never),
            ("Html", B::Sink(SinkType::Html)),
            ("Sql", B::Sink(SinkType::Sql)),
            ("Command", B::Sink(SinkType::Command)),
            ("Url", B::Sink(SinkType::Url)),
            ("Path", B::Sink(SinkType::Path)),
            ("PathSegment", B::Sink(SinkType::PathSegment)),
            ("Regex", B::Sink(SinkType::Regex)),
            ("Secret", B::Secret),
            ("AnyError", B::AnyError),
            ("Net", B::Capability(Capability::Net)),
            ("Fs", B::Capability(Capability::Fs)),
            ("Env", B::Capability(Capability::Env)),
            ("Clock", B::Capability(Capability::Clock)),
            ("Rand", B::Capability(Capability::Rand)),
            ("Proc", B::Capability(Capability::Proc)),
            ("Stdin", B::Capability(Capability::Stdin)),
            ("Screen", B::Capability(Capability::Screen)),
            ("Range", B::Range),
        ];
        for (name, b) in prelude {
            if let Some(d) = program.prelude(name) {
                builtin_types.insert(d, b);
            }
        }
        let mut builtin_traits = HashMap::new();
        let traits = [
            ("Eq", BuiltinTrait::Eq),
            ("Hash", BuiltinTrait::Hash),
            ("Ord", BuiltinTrait::Ord),
            ("Show", BuiltinTrait::Show),
            ("Encode", BuiltinTrait::Encode),
            ("Decode", BuiltinTrait::Decode),
            ("Row", BuiltinTrait::Row),
            ("Message", BuiltinTrait::Message),
            ("Error", BuiltinTrait::Error),
            ("From", BuiltinTrait::From),
            ("Template", BuiltinTrait::Template),
        ];
        for (name, t) in traits {
            if let Some(d) = program.prelude(name) {
                builtin_traits.insert(d, t);
            }
        }
        let impls = program
            .defs
            .iter()
            .filter(|d| d.kind == DefKind::Impl)
            .map(|d| ((d.span.file, d.span.span), d.id))
            .collect();
        Env {
            program,
            builtin_types,
            builtin_traits,
            self_type: program.prelude("Self"),
            impls,
        }
    }

    fn kind(&self, d: DefId) -> Option<DefKind> {
        self.program.defs.get(d.0 as usize).map(|x| x.kind)
    }

    /// The symbol denotes a type (so `Def.name` is an associated function).
    fn is_type(&self, d: DefId) -> bool {
        self.builtin_types.contains_key(&d)
            || matches!(
                self.kind(d),
                Some(DefKind::Record | DefKind::Enum | DefKind::Alias | DefKind::ExternType)
            )
    }

    fn trait_id(&self, d: DefId) -> Option<TraitId> {
        if let Some(&b) = self.builtin_traits.get(&d) {
            return Some(TraitId::Builtin(b));
        }
        (self.kind(d) == Some(DefKind::Trait)).then_some(TraitId::User(d))
    }
}

fn lang_item(c: Concept) -> Option<LangItem> {
    Some(match c {
        Concept::CorePrint => LangItem::Print,
        Concept::CorePanic => LangItem::Panic,
        Concept::CoreAssert => LangItem::Assert,
        Concept::CoreSome => LangItem::Some,
        Concept::CoreNone => LangItem::None,
        Concept::CoreOk => LangItem::Ok,
        Concept::CoreErr => LangItem::Err,
        _ => return None,
    })
}

/// The traits `@derive(…)` may name (§9.7).
fn derive_trait(name: &str) -> Option<BuiltinTrait> {
    Some(match name {
        "Eq" => BuiltinTrait::Eq,
        "Hash" => BuiltinTrait::Hash,
        "Ord" => BuiltinTrait::Ord,
        "Show" => BuiltinTrait::Show,
        "Encode" => BuiltinTrait::Encode,
        "Decode" => BuiltinTrait::Decode,
        "Row" => BuiltinTrait::Row,
        "Message" => BuiltinTrait::Message,
        _ => return None,
    })
}

fn has_attr(attrs: &[ast::Attribute], name: &str) -> bool {
    attrs
        .iter()
        .any(|a| a.path.len() == 1 && a.path[0].name == name)
}

/// The traits of every `@derive(…)`, each with where it is written.
fn derives(attrs: &[ast::Attribute]) -> (Vec<BuiltinTrait>, Vec<Span>) {
    let mut out = Vec::new();
    let mut spans = Vec::new();
    for a in attrs {
        if a.path.len() != 1 || a.path[0].name != "derive" {
            continue;
        }
        for arg in a.args.iter().flatten() {
            if let ast::ExprKind::Name(ast::NameKind::Ident(n)) = &arg.value.kind {
                if let Some(t) = derive_trait(n) {
                    if !out.contains(&t) {
                        out.push(t);
                        spans.push(arg.value.span);
                    }
                }
            }
        }
    }
    (out, spans)
}

// ---------------------------------------------------------------------------------------
// String literals
// ---------------------------------------------------------------------------------------

/// Decodes the escapes of spec §5.7 (`\n \t \r \\ \" \{ \} \0 \u{X…}`). An unknown escape
/// (K0130, the lexer's) is kept as written.
pub fn decode_escapes(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(c @ ('\\' | '"' | '{' | '}')) => out.push(c),
            Some('u') if chars.peek() == Some(&'{') => {
                chars.next();
                let mut hex = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    hex.push(c);
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => {
                        out.push_str("\\u{");
                        out.push_str(&hex);
                        out.push('}');
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// A stand-in for a hole while the indentation of a `"""…"""` literal is stripped
/// (private-use code points cannot appear in source text, SEC0112).
const HOLE: char = '\u{E000}';

/// The raw text parts of a `"""…"""` literal (holes between them) with the first newline
/// dropped and the common indentation of the content lines removed (§5.7). The last line,
/// when it holds only the indentation of the closing quotes, is dropped too.
fn dedent_triple(parts: &[String]) -> Vec<String> {
    let mut joined = parts.join(&HOLE.to_string());
    if let Some(rest) = joined.strip_prefix('\n') {
        joined = rest.to_string();
    } else if let Some(rest) = joined.strip_prefix("\r\n") {
        joined = rest.to_string();
    }
    let mut lines: Vec<&str> = joined.split('\n').collect();
    if lines.len() > 1 && lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
        lines.push("");
    }
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    let stripped: Vec<&str> = lines
        .iter()
        .map(|l| {
            let n = (l.len() - l.trim_start_matches(' ').len()).min(indent);
            &l[n..]
        })
        .collect();
    let text = stripped.join("\n");
    text.split(HOLE).map(str::to_string).collect()
}

// ---------------------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------------------

struct ModuleLower<'a, 'p> {
    env: &'a Env<'p>,
    file: FileId,
    module: ModulePath,
    out: &'a mut HirProgram,
    next_synthetic: &'a mut u32,
    /// Lower function bodies and tests (see [`lower_program_with`]).
    bodies: bool,
    /// A stdlib module.
    std: bool,
}

impl ModuleLower<'_, '_> {
    fn decl(&self, span: Span) -> Option<DefId> {
        self.env.program.decl_at(self.file, span)
    }

    fn add(&mut self, def: DefId, name: &str, span: Span, kind: ItemKind) {
        let exported = self
            .env
            .program
            .defs
            .get(def.0 as usize)
            .is_some_and(|d| d.exported);
        self.out.add(Item {
            def,
            name: name.to_string(),
            module: self.module.clone(),
            file: self.file,
            span,
            exported,
            kind,
        });
    }

    fn name_text(&self, d: DefId) -> String {
        self.env.program.def(d).name.clone()
    }

    fn item(&mut self, item: &ast::Item) {
        match &item.kind {
            ast::ItemKind::Fn(f) => {
                let Some(def) = self.decl(f.sig.name.span) else {
                    return;
                };
                let generics = GenericScope::default();
                let decl = self.func(def, f, &item.attrs, None, &generics);
                let name = self.name_text(def);
                self.add(def, &name, f.sig.name.span, ItemKind::Fn(decl));
                // `main` is a core name (§7.2), spelled per skin: the entry point (§14.3).
                if !self.std && matches!(f.sig.name.kind, ast::NameKind::Core(Concept::CoreMain)) {
                    self.out.mains.push(def);
                }
            }
            ast::ItemKind::Type(t) => {
                let Some(def) = self.decl(t.name.span) else {
                    return;
                };
                let generics = GenericScope::new(self, &t.generics, 0);
                let gdecls = self.generic_decls(&t.generics, &generics);
                let kind = match &t.body {
                    ast::TypeBody::Record(fields) => {
                        let mut body = BodyLower::new(self.env, self.file, def, &generics, None);
                        let fields = fields
                            .iter()
                            .map(|f| FieldDecl {
                                name: f.name.name.clone(),
                                ty: body.ty(&f.ty),
                                default: f.default.as_ref().map(|e| body.expr(e)),
                                span: f.name.span,
                            })
                            .collect();
                        body.finish_into(self.out);
                        let (derives, derive_spans) = derives(&item.attrs);
                        ItemKind::Record(RecordDecl {
                            generics: gdecls,
                            fields,
                            opaque: item.vis == ast::Visibility::ExportOpaque,
                            derives,
                            derive_spans,
                        })
                    }
                    ast::TypeBody::Alias(target) => {
                        let body = BodyLower::new(self.env, self.file, def, &generics, None);
                        ItemKind::Alias(AliasDecl {
                            generics: gdecls,
                            target: body.ty(target),
                        })
                    }
                };
                let name = self.name_text(def);
                self.add(def, &name, t.name.span, kind);
            }
            ast::ItemKind::Enum(e) => {
                let Some(def) = self.decl(e.name.span) else {
                    return;
                };
                let generics = GenericScope::new(self, &e.generics, 0);
                let gdecls = self.generic_decls(&e.generics, &generics);
                let body = BodyLower::new(self.env, self.file, def, &generics, None);
                let mut variants = Vec::new();
                for v in &e.variants {
                    let Some(vdef) = self.decl(v.name.span) else {
                        continue;
                    };
                    variants.push(VariantDecl {
                        def: vdef,
                        name: v.name.name.clone(),
                        fields: v
                            .fields
                            .iter()
                            .flatten()
                            .map(|f| VariantFieldDecl {
                                label: f.label.as_ref().map(|l| l.name.clone()),
                                ty: body.ty(&f.ty),
                            })
                            .collect(),
                        span: v.name.span,
                    });
                }
                let name = self.name_text(def);
                let (derives, derive_spans) = derives(&item.attrs);
                self.add(
                    def,
                    &name,
                    e.name.span,
                    ItemKind::Enum(EnumDecl {
                        generics: gdecls,
                        variants,
                        opaque: item.vis == ast::Visibility::ExportOpaque,
                        derives,
                        derive_spans,
                    }),
                );
            }
            ast::ItemKind::Trait(t) => {
                let Some(def) = self.decl(t.name.span) else {
                    return;
                };
                let generics = GenericScope::new(self, &t.generics, 0);
                let gdecls = self.generic_decls(&t.generics, &generics);
                let mut methods = Vec::new();
                for f in &t.members {
                    let Some(mdef) = self.decl(f.sig.name.span) else {
                        continue;
                    };
                    let decl = self.func(mdef, f, &[], Some(def), &generics);
                    let name = self.name_text(mdef);
                    self.add(mdef, &name, f.sig.name.span, ItemKind::Fn(decl));
                    methods.push(mdef);
                }
                let name = self.name_text(def);
                let builtin = self.env.builtin_traits.get(&def).copied();
                self.add(
                    def,
                    &name,
                    t.name.span,
                    ItemKind::Trait(TraitItem {
                        generics: gdecls,
                        methods,
                        builtin,
                    }),
                );
            }
            ast::ItemKind::Impl(imp) => {
                let Some(&def) = self.env.impls.get(&(self.file, item.span)) else {
                    return;
                };
                let generics = GenericScope::new(self, &imp.generics, 0);
                let gdecls = self.generic_decls(&imp.generics, &generics);
                let body = BodyLower::new(self.env, self.file, def, &generics, None);
                let self_ty = body.ty(&imp.ty);
                let as_trait = imp.as_trait.as_ref().and_then(|t| body.bound(t));
                let mut methods = Vec::new();
                for member in &imp.members {
                    let Some(mdef) = self.decl(member.func.sig.name.span) else {
                        continue;
                    };
                    let decl = self.func(mdef, &member.func, &member.attrs, Some(def), &generics);
                    let name = self.name_text(mdef);
                    self.add(mdef, &name, member.func.sig.name.span, ItemKind::Fn(decl));
                    methods.push(mdef);
                }
                let head = Span::new(item.span.start, imp.ty.span.end.max(item.span.start));
                self.add(
                    def,
                    "",
                    head,
                    ItemKind::Impl(ImplItem {
                        generics: gdecls,
                        self_ty,
                        as_trait,
                        methods,
                    }),
                );
            }
            ast::ItemKind::Const(c) => {
                let Some(def) = self.decl(c.name.span) else {
                    return;
                };
                let generics = GenericScope::default();
                let mut body = BodyLower::new(self.env, self.file, def, &generics, None);
                let ty = c.ty.as_ref().map(|t| body.ty(t));
                let value = body.expr(&c.value);
                body.finish_into(self.out);
                let name = self.name_text(def);
                self.add(
                    def,
                    &name,
                    c.name.span,
                    ItemKind::Const(ConstDecl { ty, value }),
                );
            }
            ast::ItemKind::Extern(ast::ExternDecl::Fn(sig)) => {
                let Some(def) = self.decl(sig.name.span) else {
                    return;
                };
                let f = ast::FnDecl {
                    sig: sig.clone(),
                    body: ast::FnBody::None,
                    span: sig.span,
                };
                let generics = GenericScope::default();
                let decl = self.func(def, &f, &item.attrs, None, &generics);
                let name = self.name_text(def);
                self.add(def, &name, sig.name.span, ItemKind::Fn(decl));
            }
            ast::ItemKind::Extern(ast::ExternDecl::Type { name, generics }) => {
                let Some(def) = self.decl(name.span) else {
                    return;
                };
                // The stdlib stubs of built-in types (`export extern type Int`) are the
                // built-in types themselves.
                if self.env.builtin_types.contains_key(&def) {
                    return;
                }
                let scope = GenericScope::new(self, generics, 0);
                let gdecls = self.generic_decls(generics, &scope);
                let text = self.name_text(def);
                self.add(
                    def,
                    &text,
                    name.span,
                    ItemKind::Extern(ExternTypeDecl { generics: gdecls }),
                );
            }
            ast::ItemKind::Test(_) if !self.bodies => {}
            ast::ItemKind::Test(t) => {
                let def = DefId(*self.next_synthetic);
                *self.next_synthetic += 1;
                let generics = GenericScope::default();
                let mut body = BodyLower::new(self.env, self.file, def, &generics, None);
                let block = body.block(&t.body);
                let locals = body.finish_into(self.out);
                self.add(
                    def,
                    "",
                    t.name.span,
                    ItemKind::Test(Body {
                        locals,
                        value: BodyValue::Block(block),
                    }),
                );
            }
            ast::ItemKind::Error => {}
        }
    }

    fn generic_decls(
        &self,
        gs: &[ast::GenericParam],
        scope: &GenericScope,
    ) -> Vec<GenericParamDecl> {
        let body = BodyLower::new(self.env, self.file, DefId(u32::MAX), scope, None);
        gs.iter()
            .map(|g| GenericParamDecl {
                name: g.name.name.clone(),
                bounds: g.bounds.iter().filter_map(|b| body.bound(b)).collect(),
            })
            .collect()
    }

    /// A function, method or trait method. `outer` holds the generics of the enclosing
    /// impl or trait, which come first.
    fn func(
        &mut self,
        def: DefId,
        f: &ast::FnDecl,
        attrs: &[ast::Attribute],
        parent: Option<DefId>,
        outer: &GenericScope,
    ) -> FnDecl {
        let generics = outer.extended(self, &f.sig.generics);
        let own = self.generic_decls(&f.sig.generics, &generics);
        let mut body = BodyLower::new(self.env, self.file, def, &generics, None);
        // A method is written one level into its `impl` or `trait` (§5.8 rule 1).
        body.line_depth = u32::from(parent.is_some());
        let mut params = Vec::new();
        let mut has_self = false;
        for p in &f.sig.params {
            match p {
                ast::Param::SelfParam(span) => {
                    has_self = true;
                    let local = body.anon_local("self", *span, false);
                    body.self_local = Some(local);
                    params.push(ParamDecl {
                        name: "self".into(),
                        local,
                        ty: Type {
                            kind: TypeKind::SelfType,
                            span: *span,
                        },
                        default: None,
                        span: *span,
                    });
                }
                ast::Param::Named {
                    name,
                    ty,
                    default,
                    span,
                } => {
                    let ty = body.ty(ty);
                    let default = default.as_ref().map(|e| body.expr(e));
                    let local = body.declare(name, false);
                    params.push(ParamDecl {
                        name: name.name.clone(),
                        local,
                        ty,
                        default,
                        span: *span,
                    });
                }
            }
        }
        let ret = f.sig.ret.as_ref().map(|t| body.ty(t));
        let value = match &f.body {
            _ if !self.bodies => None,
            ast::FnBody::Block(b) => Some(BodyValue::Block(body.block(b))),
            ast::FnBody::Expr(e) => Some(BodyValue::Expr(body.expr(e))),
            ast::FnBody::None => None,
        };
        let locals = body.finish_into(self.out);
        FnDecl {
            generics: own,
            has_self,
            params,
            ret,
            sync_attr: has_attr(attrs, "sync"),
            positional: has_attr(attrs, "positional"),
            parent,
            body: value.map(|value| Body { locals, value }),
        }
    }
}

/// Generic parameter symbols in scope and their indices (§9.6).
#[derive(Clone, Default)]
struct GenericScope {
    index: HashMap<DefId, u32>,
    len: u32,
}

impl GenericScope {
    fn new(m: &ModuleLower<'_, '_>, gs: &[ast::GenericParam], start: u32) -> GenericScope {
        let mut s = GenericScope {
            index: HashMap::new(),
            len: start,
        };
        s.push_all(m, gs);
        s
    }

    fn extended(&self, m: &ModuleLower<'_, '_>, gs: &[ast::GenericParam]) -> GenericScope {
        let mut s = self.clone();
        s.push_all(m, gs);
        s
    }

    fn push_all(&mut self, m: &ModuleLower<'_, '_>, gs: &[ast::GenericParam]) {
        for g in gs {
            if let Some(d) = m.decl(g.name.span) {
                self.index.insert(d, self.len);
            }
            self.len += 1;
        }
    }
}

// ---------------------------------------------------------------------------------------
// Bodies
// ---------------------------------------------------------------------------------------

struct BodyLower<'a, 'p> {
    env: &'a Env<'p>,
    file: FileId,
    owner: DefId,
    generics: &'a GenericScope,
    locals: Vec<LocalDecl>,
    local_of: HashMap<DefId, LocalId>,
    next_expr: u32,
    closures: Vec<u32>,
    next_closure: u32,
    self_local: Option<LocalId>,
    raw_strings: Vec<(ExprId, String)>,
    /// The method name's span in each `MethodCall` (K1108's fix, Q35).
    method_names: Vec<(ExprId, Span)>,
    uses: HashMap<LocalId, u32>,
    /// The block level of the source line being lowered: 0 for an item, 1 for a method or
    /// a top-level function's statements, and one more per indented block (4 spaces each,
    /// §5.8 rule 1). It gives the column of statements and `case` arms for fixes that
    /// insert lines (HIR has no source text).
    line_depth: u32,
    /// The statements being lowered are a one-line block (`if c: x = 1`), on the line of
    /// their header.
    inline: bool,
}

impl<'a, 'p> BodyLower<'a, 'p> {
    fn new(
        env: &'a Env<'p>,
        file: FileId,
        owner: DefId,
        generics: &'a GenericScope,
        self_local: Option<LocalId>,
    ) -> BodyLower<'a, 'p> {
        BodyLower {
            env,
            file,
            owner,
            generics,
            locals: Vec::new(),
            local_of: HashMap::new(),
            next_expr: 0,
            closures: vec![0],
            next_closure: 1,
            self_local,
            raw_strings: Vec::new(),
            method_names: Vec::new(),
            uses: HashMap::new(),
            line_depth: 0,
            inline: false,
        }
    }

    /// Hands the side tables to the program and returns the locals.
    fn finish_into(self, out: &mut HirProgram) -> Vec<LocalDecl> {
        for (e, raw) in self.raw_strings {
            out.raw_strings.insert((self.owner, e), raw);
        }
        for (e, at) in self.method_names {
            out.method_names.insert((self.owner, e), at);
        }
        for (l, n) in self.uses {
            out.local_uses.insert((self.owner, l), n);
        }
        self.locals
    }

    fn program(&self) -> &'p Program {
        self.env.program
    }

    fn closure(&self) -> u32 {
        self.closures.last().copied().unwrap_or(0)
    }

    fn anon_local(&mut self, name: &str, span: Span, mutable: bool) -> LocalId {
        self.locals.push(LocalDecl {
            name: name.to_string(),
            mutable,
            span,
            closure: self.closure(),
        });
        LocalId(self.locals.len() as u32 - 1)
    }

    /// The local a declaring identifier introduces (the same one again for a name an
    /// or-pattern's alternatives share).
    fn declare(&mut self, id: &ast::Ident, mutable: bool) -> LocalId {
        match self.program().decl_at(self.file, id.span) {
            Some(d) => {
                if let Some(&l) = self.local_of.get(&d) {
                    return l;
                }
                let l = self.anon_local(&id.name, id.span, mutable);
                self.local_of.insert(d, l);
                l
            }
            None => self.anon_local(&id.name, id.span, mutable),
        }
    }

    fn e(&mut self, kind: ExprKind, span: Span) -> Expr {
        self.next_expr += 1;
        Expr {
            id: ExprId(self.next_expr - 1),
            kind,
            span,
        }
    }

    fn error(&mut self, span: Span) -> Expr {
        self.e(ExprKind::Error, span)
    }

    // -- types --------------------------------------------------------------------------

    fn ty(&self, t: &ast::Type) -> Type {
        kalam_ast::stack::deeper(|| self.ty_inner(t))
    }

    fn ty_inner(&self, t: &ast::Type) -> Type {
        let kind = match &t.kind {
            ast::TypeKind::Unit => TypeKind::Unit,
            ast::TypeKind::Error => TypeKind::Error,
            ast::TypeKind::Tuple(ts) => TypeKind::Tuple(ts.iter().map(|t| self.ty(t)).collect()),
            ast::TypeKind::Fn { sync, params, ret } => TypeKind::Fn {
                sync: *sync,
                params: params.iter().map(|t| self.ty(t)).collect(),
                ret: Box::new(self.ty(ret)),
            },
            ast::TypeKind::Named { path, args } => {
                let args: Vec<Type> = args.iter().map(|t| self.ty(t)).collect();
                match path
                    .last()
                    .and_then(|seg| self.program().use_at(self.file, seg.span))
                {
                    Some(Res::Def(d)) => self.def_type(d, args),
                    _ => TypeKind::Error,
                }
            }
        };
        Type { kind, span: t.span }
    }

    fn def_type(&self, d: DefId, args: Vec<Type>) -> TypeKind {
        if let Some(&b) = self.env.builtin_types.get(&d) {
            return TypeKind::Builtin(b, args);
        }
        if Some(d) == self.env.self_type {
            return TypeKind::SelfType;
        }
        if let Some(&i) = self.generics.index.get(&d) {
            return TypeKind::Param(i);
        }
        match self.env.kind(d) {
            Some(DefKind::Record | DefKind::Enum | DefKind::Alias | DefKind::ExternType) => {
                TypeKind::Def(d, args)
            }
            _ => TypeKind::Error,
        }
    }

    /// A trait bound or `impl … as Trait` reference.
    fn bound(&self, t: &ast::Type) -> Option<TraitBound> {
        let ast::TypeKind::Named { path, args } = &t.kind else {
            return None;
        };
        let seg = path.last()?;
        let Some(Res::Def(d)) = self.program().use_at(self.file, seg.span) else {
            return None;
        };
        Some(TraitBound {
            id: self.env.trait_id(d)?,
            args: args.iter().map(|t| self.ty(t)).collect(),
        })
    }

    /// The type a type-denoting symbol stands for (the base of `Type.name`).
    fn type_of_def(&self, d: DefId, span: Span) -> Type {
        Type {
            kind: self.def_type(d, Vec::new()),
            span,
        }
    }

    // -- names --------------------------------------------------------------------------

    /// What a resolved name means as a value.
    fn def_value(&mut self, d: DefId, span: Span) -> Expr {
        if let Some(&l) = self.local_of.get(&d) {
            *self.uses.entry(l).or_default() += 1;
            return self.e(ExprKind::Local(l), span);
        }
        let record = self.env.kind(d) == Some(DefKind::Record);
        if !record && (self.env.is_type(d) || self.generics.index.contains_key(&d)) {
            let ty = self.type_of_def(d, span);
            return self.e(ExprKind::TypeName(ty), span);
        }
        let kind = match self.env.kind(d) {
            Some(DefKind::Core(c)) => match lang_item(c) {
                Some(item) => ExprKind::Lang(item),
                None => ExprKind::Error,
            },
            Some(
                DefKind::Fn
                | DefKind::Method
                | DefKind::TraitMethod
                | DefKind::ExternFn
                | DefKind::Const
                | DefKind::Variant
                | DefKind::Record,
            ) => ExprKind::Def(d),
            _ => ExprKind::Error,
        };
        self.e(kind, span)
    }

    /// The symbol `e` statically denotes, when it is a name or a resolved `a.b` path.
    fn static_target(&self, e: &ast::Expr) -> Option<DefId> {
        let span = match &e.kind {
            ast::ExprKind::Name(_) => e.span,
            ast::ExprKind::Member {
                member: ast::Member::Name(id),
                ..
            } => id.span,
            _ => return None,
        };
        match self.program().use_at(self.file, span) {
            Some(Res::Def(d)) if !self.local_of.contains_key(&d) => Some(d),
            _ => None,
        }
    }

    // -- expressions --------------------------------------------------------------------

    fn expr(&mut self, e: &ast::Expr) -> Expr {
        kalam_ast::stack::deeper(|| self.expr_inner(e))
    }

    fn str_lit(&mut self, lit: &ast::StrLit, span: Span) -> Expr {
        let raw: Vec<String> = lit
            .parts
            .iter()
            .filter_map(|p| match p {
                ast::StrPart::Text(t) => Some(t.clone()),
                ast::StrPart::Hole(_) => None,
            })
            .collect();
        let has_holes = lit.parts.iter().any(|p| matches!(p, ast::StrPart::Hole(_)));
        // Text parts in order, with an empty text between adjacent holes.
        let mut texts: Vec<String> = Vec::new();
        let mut holes: Vec<&ast::Expr> = Vec::new();
        let mut pending = String::new();
        for p in &lit.parts {
            match p {
                ast::StrPart::Text(t) => pending.push_str(t),
                ast::StrPart::Hole(h) => {
                    texts.push(std::mem::take(&mut pending));
                    holes.push(h);
                }
            }
        }
        texts.push(pending);
        if lit.triple {
            texts = dedent_triple(&texts);
        }
        let texts: Vec<String> = texts.iter().map(|t| decode_escapes(t)).collect();
        if !has_holes {
            let value = texts.concat();
            let out = self.e(ExprKind::Str(value), span);
            if !lit.triple && raw.len() <= 1 {
                self.raw_strings
                    .push((out.id, raw.into_iter().next().unwrap_or_default()));
            }
            return out;
        }
        let mut parts = Vec::new();
        let mut texts = texts.into_iter();
        for h in holes {
            if let Some(t) = texts.next() {
                if !t.is_empty() {
                    parts.push(StrPart::Text(t));
                }
            }
            parts.push(StrPart::Hole(self.expr(h)));
        }
        if let Some(t) = texts.next() {
            if !t.is_empty() {
                parts.push(StrPart::Text(t));
            }
        }
        self.e(ExprKind::Interp(parts), span)
    }

    fn tagged(&mut self, tag: &str, lit: &ast::StrLit, span: Span) -> Expr {
        let mut texts: Vec<String> = Vec::new();
        let mut holes_ast: Vec<&ast::Expr> = Vec::new();
        let mut pending = String::new();
        for p in &lit.parts {
            match p {
                ast::StrPart::Text(t) => pending.push_str(t),
                ast::StrPart::Hole(h) => {
                    texts.push(std::mem::take(&mut pending));
                    holes_ast.push(h);
                }
            }
        }
        texts.push(pending);
        if lit.triple {
            texts = dedent_triple(&texts);
        }
        let parts = texts.iter().map(|t| decode_escapes(t)).collect();
        let holes: Vec<Expr> = holes_ast.into_iter().map(|h| self.expr(h)).collect();
        match kalam_sinks::Tag::from_name(tag) {
            Some(tag) => self.e(ExprKind::Tagged { tag, parts, holes }, span),
            // A user-defined tag (§17.8) is typed with `Template` in the next iteration.
            None => self.e(ExprKind::Error, span),
        }
    }

    fn args(&mut self, args: &[ast::Arg]) -> Vec<Arg> {
        args.iter()
            .map(|a| Arg {
                label: a.label.as_ref().map(|l| l.name.clone()),
                value: self.expr(&a.value),
                span: a.span,
            })
            .collect()
    }

    fn expr_inner(&mut self, e: &ast::Expr) -> Expr {
        let span = e.span;
        let kind = match &e.kind {
            ast::ExprKind::Int(lit) => ExprKind::Int(lit.value),
            ast::ExprKind::Float(lit) => ExprKind::Float(lit.text.clone()),
            ast::ExprKind::Str(lit) => return self.str_lit(lit, span),
            ast::ExprKind::Tagged { tag, lit } => return self.tagged(tag, lit, span),
            ast::ExprKind::Bool(b) => ExprKind::Bool(*b),
            ast::ExprKind::Unit => ExprKind::Unit,
            ast::ExprKind::Name(_) => {
                return match self.program().use_at(self.file, span) {
                    Some(Res::Def(d)) => self.def_value(d, span),
                    _ => self.error(span),
                };
            }
            ast::ExprKind::SelfValue => match self.self_local {
                Some(l) => {
                    *self.uses.entry(l).or_default() += 1;
                    ExprKind::Local(l)
                }
                None => ExprKind::Error,
            },
            // When no variant has this name, resolve reported K0401 if it knew the whole
            // program; the checker says it otherwise (FOUNDER-QUEUE Q34), so the name stays.
            ast::ExprKind::ImplicitMember(id) => ExprKind::ImplicitMember(id.name.clone()),
            ast::ExprKind::UnsafeMember(id) => ExprKind::Unsafe(id.name.clone()),
            ast::ExprKind::Paren(x) => return self.expr(x),
            ast::ExprKind::Tuple(xs) => ExprKind::Tuple(xs.iter().map(|x| self.expr(x)).collect()),
            ast::ExprKind::List(xs) => ExprKind::List(xs.iter().map(|x| self.expr(x)).collect()),
            ast::ExprKind::Map(pairs) => ExprKind::Map(
                pairs
                    .iter()
                    .map(|(k, v)| (self.expr(k), self.expr(v)))
                    .collect(),
            ),
            ast::ExprKind::Call { callee, args } => return self.call(callee, args, span),
            ast::ExprKind::Index { base, index } => ExprKind::Index {
                base: Box::new(self.expr(base)),
                index: Box::new(self.expr(index)),
            },
            ast::ExprKind::Member { base, member } => return self.member(base, member, span),
            ast::ExprKind::Try(x) => ExprKind::Try(Box::new(self.expr(x))),
            ast::ExprKind::Neg(x) => ExprKind::Unary {
                op: UnOp::Neg,
                operand: Box::new(self.expr(x)),
            },
            ast::ExprKind::Not(x) => ExprKind::Unary {
                op: UnOp::Not,
                operand: Box::new(self.expr(x)),
            },
            ast::ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
                op: bin_op(*op),
                lhs: Box::new(self.expr(lhs)),
                rhs: Box::new(self.expr(rhs)),
            },
            ast::ExprKind::Range { lo, hi, inclusive } => ExprKind::Range {
                lo: Box::new(self.expr(lo)),
                hi: Box::new(self.expr(hi)),
                inclusive: *inclusive,
            },
            ast::ExprKind::Lambda(l) => {
                let closure = self.next_closure;
                self.closures.push(closure);
                self.next_closure += 1;
                let params = l
                    .params
                    .iter()
                    .map(|p| {
                        let ty = p.ty.as_ref().map(|t| self.ty(t));
                        let local = if p.name.name == "_" {
                            self.anon_local("_", p.name.span, false)
                        } else {
                            self.declare(&p.name, false)
                        };
                        LambdaParam { local, ty }
                    })
                    .collect();
                let ret = l.ret.as_ref().map(|t| self.ty(t));
                let body = Box::new(self.expr(&l.body));
                self.closures.pop();
                ExprKind::Lambda {
                    params,
                    ret,
                    body,
                    closure,
                }
            }
            ast::ExprKind::If(ifx) => ExprKind::If {
                branches: ifx
                    .branches
                    .iter()
                    .map(|b| IfBranch {
                        cond: self.expr(&b.cond),
                        body: self.block(&b.body),
                    })
                    .collect(),
                else_body: ifx.else_body.as_ref().map(|b| self.block(b)),
            },
            ast::ExprKind::Match(mx) => {
                let scrutinee = Box::new(self.expr(&mx.scrutinee));
                // The arms are one level deeper than the line of the `match` header.
                let saved = (self.line_depth, self.inline);
                self.line_depth += 1;
                self.inline = false;
                let indent = self.line_depth * 4;
                let arms = mx
                    .arms
                    .iter()
                    .map(|a| Arm {
                        pat: self.pat(&a.pat),
                        guard: a.guard.as_ref().map(|g| self.expr(g)),
                        body: self.block(&a.body),
                        span: a.span,
                        indent: Some(indent),
                    })
                    .collect();
                (self.line_depth, self.inline) = saved;
                ExprKind::Match { scrutinee, arms }
            }
            ast::ExprKind::Error => ExprKind::Error,
        };
        self.e(kind, span)
    }

    /// `base.name` outside call position.
    fn member(&mut self, base: &ast::Expr, member: &ast::Member, span: Span) -> Expr {
        let id = match member {
            ast::Member::Index { text, .. } => {
                let index = text.parse::<u32>().unwrap_or(u32::MAX);
                let base = Box::new(self.expr(base));
                return self.e(ExprKind::TupleField { base, index }, span);
            }
            ast::Member::Name(id) => id,
        };
        match self.program().use_at(self.file, id.span) {
            Some(Res::Def(d)) => return self.def_value(d, span),
            Some(Res::Error) => return self.error(span),
            None => {}
        }
        if let Some(t) = self.static_target(base) {
            if self.env.is_type(t) {
                let ty = self.type_of_def(t, base.span);
                let name = id.name.clone();
                return self.e(ExprKind::AssocFn { ty, name }, span);
            }
            if !matches!(
                self.env.kind(t),
                Some(DefKind::Const | DefKind::Fn | DefKind::Variant)
            ) {
                // A module (an opaque one, or a member resolve already reported), a trait
                // or an opaque import: nothing more is known.
                return self.error(span);
            }
        }
        let base = Box::new(self.expr(base));
        self.e(
            ExprKind::Field {
                base,
                name: id.name.clone(),
            },
            span,
        )
    }

    fn call(&mut self, callee: &ast::Expr, args: &[ast::Arg], span: Span) -> Expr {
        if let ast::ExprKind::Member {
            base,
            member: ast::Member::Name(id),
        } = &callee.kind
        {
            let resolved = self.program().use_at(self.file, id.span).is_some();
            let target = self.static_target(base);
            let value_base = match target {
                None => true,
                Some(t) => matches!(
                    self.env.kind(t),
                    Some(DefKind::Const | DefKind::Fn | DefKind::Variant)
                ),
            };
            if !resolved && value_base {
                let receiver = Box::new(self.expr(base));
                let args = self.args(args);
                let call = self.e(
                    ExprKind::MethodCall {
                        receiver,
                        name: id.name.clone(),
                        args,
                    },
                    span,
                );
                self.method_names.push((call.id, id.span));
                return call;
            }
        }
        let callee = Box::new(self.expr(callee));
        let args = self.args(args);
        self.e(ExprKind::Call { callee, args }, span)
    }

    // -- statements ---------------------------------------------------------------------

    fn block(&mut self, b: &ast::Block) -> Block {
        let saved = (self.line_depth, self.inline);
        if !b.inline {
            self.line_depth += 1;
        }
        self.inline = b.inline;
        let stmts = b.stmts.iter().map(|s| self.stmt(s)).collect();
        (self.line_depth, self.inline) = saved;
        Block {
            stmts,
            span: b.span,
        }
    }

    fn stmt(&mut self, s: &ast::Stmt) -> Stmt {
        let kind = match &s.kind {
            ast::StmtKind::Expr(e) => StmtKind::Expr(self.expr(e)),
            ast::StmtKind::Let { pat, ty, value } => {
                let ty = ty.as_ref().map(|t| self.ty(t));
                let value = self.expr(value);
                let pat = self.pat(pat);
                StmtKind::Let { pat, ty, value }
            }
            ast::StmtKind::Var { name, ty, value } => {
                let ty = ty.as_ref().map(|t| self.ty(t));
                let value = self.expr(value);
                let local = self.declare(name, true);
                StmtKind::Var { local, ty, value }
            }
            ast::StmtKind::Assign { place, op, value } => StmtKind::Assign {
                place: self.expr(place),
                op: match op {
                    ast::AssignOp::Assign => AssignOp::Assign,
                    ast::AssignOp::Add => AssignOp::Add,
                    ast::AssignOp::Sub => AssignOp::Sub,
                    ast::AssignOp::Mul => AssignOp::Mul,
                    ast::AssignOp::Div => AssignOp::Div,
                },
                value: self.expr(value),
            },
            ast::StmtKind::Return(e) => StmtKind::Return(e.as_ref().map(|e| self.expr(e))),
            ast::StmtKind::Break => StmtKind::Break,
            ast::StmtKind::Continue => StmtKind::Continue,
            ast::StmtKind::Pass | ast::StmtKind::Error => StmtKind::Pass,
            ast::StmtKind::For { pat, iter, body } => {
                let iter = self.expr(iter);
                let pat = self.pat(pat);
                let body = self.block(body);
                StmtKind::For { pat, iter, body }
            }
            ast::StmtKind::While { cond, body } => StmtKind::While {
                cond: self.expr(cond),
                body: self.block(body),
            },
            ast::StmtKind::Fn(f) => self.local_fn(f),
        };
        Stmt {
            kind,
            span: s.span,
            indent: (!self.inline).then_some(self.line_depth * 4),
        }
    }

    fn local_fn(&mut self, f: &ast::FnDecl) -> StmtKind {
        let name = match &f.sig.name.kind {
            ast::NameKind::Ident(s) => s.clone(),
            ast::NameKind::Core(_) => String::new(),
        };
        let id = ast::Ident {
            name: name.clone(),
            span: f.sig.name.span,
        };
        let local = self.declare(&id, false);
        let closure = self.next_closure;
        self.closures.push(closure);
        self.next_closure += 1;
        let mut params = Vec::new();
        for p in &f.sig.params {
            if let ast::Param::Named {
                name,
                ty,
                default,
                span,
            } = p
            {
                let ty = self.ty(ty);
                let default = default.as_ref().map(|e| self.expr(e));
                let local = self.declare(name, false);
                params.push(ParamDecl {
                    name: name.name.clone(),
                    local,
                    ty,
                    default,
                    span: *span,
                });
            }
        }
        let ret = f.sig.ret.as_ref().map(|t| self.ty(t));
        let value = match &f.body {
            ast::FnBody::Block(b) => Some(BodyValue::Block(self.block(b))),
            ast::FnBody::Expr(e) => Some(BodyValue::Expr(self.expr(e))),
            ast::FnBody::None => None,
        };
        self.closures.pop();
        StmtKind::LocalFn {
            local,
            closure,
            decl: Box::new(FnDecl {
                // Local functions are not generic in v0.1 (§12.3): a generic parameter of
                // one has no index and its uses are poison.
                generics: Vec::new(),
                has_self: false,
                params,
                ret,
                sync_attr: false,
                positional: false,
                parent: None,
                body: value.map(|value| Body {
                    locals: Vec::new(),
                    value,
                }),
            }),
        }
    }

    // -- patterns -----------------------------------------------------------------------

    fn pat(&mut self, p: &ast::Pat) -> Pat {
        kalam_ast::stack::deeper(|| self.pat_inner(p))
    }

    fn pat_fields(&mut self, fields: &Option<ast::PatFields>) -> (Vec<PatField>, bool) {
        match fields {
            None => (Vec::new(), false),
            Some(f) => (
                f.fields
                    .iter()
                    .map(|pf| PatField {
                        label: pf.label.as_ref().map(|l| l.name.clone()),
                        pat: self.pat(&pf.pat),
                    })
                    .collect(),
                f.rest,
            ),
        }
    }

    fn pat_inner(&mut self, p: &ast::Pat) -> Pat {
        let kind = match &p.kind {
            ast::PatKind::Wild => PatKind::Wild,
            ast::PatKind::Bind(id) if id.name == "_" => PatKind::Wild,
            ast::PatKind::Bind(id) => PatKind::Bind(self.declare(id, false)),
            ast::PatKind::Int(b) => PatKind::Int {
                negative: b.negative,
                value: b.lit.value.unwrap_or(u128::MAX),
            },
            ast::PatKind::Range { lo, hi, inclusive } => PatKind::Range {
                lo: signed(lo),
                hi: signed(hi),
                inclusive: *inclusive,
            },
            ast::PatKind::Str(lit) => {
                let raw: String = lit
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        ast::StrPart::Text(t) => Some(t.as_str()),
                        ast::StrPart::Hole(_) => None,
                    })
                    .collect();
                PatKind::Str(decode_escapes(&raw))
            }
            ast::PatKind::Bool(b) => PatKind::Bool(*b),
            // As for `.V` in an expression: the checker reports a name no variant has.
            ast::PatKind::Implicit { name, fields } => {
                let (fields, rest) = self.pat_fields(fields);
                PatKind::Variant {
                    def: None,
                    name: name.name.clone(),
                    fields,
                    rest,
                }
            }
            ast::PatKind::Path { path, fields } => {
                let target = path
                    .last()
                    .and_then(|seg| self.program().use_at(self.file, seg.span));
                let (fields_l, rest) = self.pat_fields(fields);
                match target {
                    Some(Res::Def(d)) => match self.env.kind(d) {
                        Some(DefKind::Variant) => PatKind::Variant {
                            def: Some(d),
                            name: self.program().def(d).name.clone(),
                            fields: fields_l,
                            rest,
                        },
                        Some(DefKind::Record) => PatKind::Record {
                            def: d,
                            fields: fields_l,
                            rest,
                        },
                        _ => PatKind::Error,
                    },
                    _ => PatKind::Error,
                }
            }
            ast::PatKind::Ctor { ctor, inner } => match lang_item(*ctor) {
                Some(item) => PatKind::Lang {
                    item,
                    inner: inner.as_ref().map(|x| Box::new(self.pat(x))),
                },
                None => PatKind::Error,
            },
            ast::PatKind::Tuple(ps) => PatKind::Tuple(ps.iter().map(|x| self.pat(x)).collect()),
            ast::PatKind::List(elems) => {
                let mut before = Vec::new();
                let mut after = Vec::new();
                let mut rest = None;
                for el in elems {
                    match el {
                        ast::ListPatElem::Pat(x) => {
                            let x = self.pat(x);
                            if rest.is_some() {
                                after.push(x);
                            } else {
                                before.push(x);
                            }
                        }
                        ast::ListPatElem::Rest(name) => {
                            let local = name
                                .as_ref()
                                .filter(|n| n.name != "_")
                                .map(|n| self.declare(n, false));
                            rest = Some(local);
                        }
                    }
                }
                PatKind::List {
                    before,
                    rest,
                    after,
                }
            }
            ast::PatKind::Or(alts) => PatKind::Or(alts.iter().map(|x| self.pat(x)).collect()),
            ast::PatKind::As { pat, name } => {
                let inner = Box::new(self.pat(pat));
                let local = self.declare(name, false);
                PatKind::As { pat: inner, local }
            }
            ast::PatKind::Error => PatKind::Error,
        };
        Pat { kind, span: p.span }
    }
}

fn signed(b: &ast::IntPatBound) -> i128 {
    let v = b.lit.value.unwrap_or(u128::MAX).min(i128::MAX as u128) as i128;
    if b.negative {
        -v
    } else {
        v
    }
}

fn bin_op(op: ast::BinOp) -> BinOp {
    match op {
        ast::BinOp::Or => BinOp::Or,
        ast::BinOp::And => BinOp::And,
        ast::BinOp::Eq => BinOp::Eq,
        ast::BinOp::NotEq => BinOp::NotEq,
        ast::BinOp::Lt => BinOp::Lt,
        ast::BinOp::Le => BinOp::Le,
        ast::BinOp::Gt => BinOp::Gt,
        ast::BinOp::Ge => BinOp::Ge,
        ast::BinOp::Add => BinOp::Add,
        ast::BinOp::Sub => BinOp::Sub,
        ast::BinOp::Mul => BinOp::Mul,
        ast::BinOp::Div => BinOp::Div,
        ast::BinOp::IntDiv => BinOp::IntDiv,
        ast::BinOp::Rem => BinOp::Rem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_decode_per_spec_5_7() {
        assert_eq!(
            decode_escapes(r#"a\nb\t\"q\" \{x\} \u{0627}"#),
            "a\nb\t\"q\" {x} ا"
        );
        assert_eq!(decode_escapes(r"\q"), r"\q", "unknown escapes stay (K0130)");
    }

    #[test]
    fn triple_quoted_text_drops_the_first_newline_and_common_indent() {
        let parts = vec!["\n        a\n          b\n    ".to_string()];
        assert_eq!(dedent_triple(&parts), vec!["a\n  b\n".to_string()]);
        let parts = vec!["\n    x ".to_string(), " y\n".to_string()];
        assert_eq!(
            dedent_triple(&parts),
            vec!["x ".to_string(), " y\n".to_string()]
        );
    }
}
