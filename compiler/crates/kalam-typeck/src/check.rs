//! The checker: signature collection, then bidirectional checking of every body with HM
//! unification inside it (spec §9.9).
//!
//! **Pass 1 (collect).** ADTs, aliases, traits, impls and every function signature are
//! lowered to [`Ty`]/[`Scheme`] first, so bodies may refer to any item (mutual recursion
//! needs no ordering). Coherence and the orphan rule are checked here.
//!
//! **Pass 2 (bodies).** Each body gets its own [`InferCtx`]. `check(e, expected)` pushes
//! the expected type down (literals, empty collections, implicit members, lambdas,
//! `Some`/`Ok`/`Err`, value-form `if`/`match`); everything else is synthesized and then
//! coerced (`Never` coerces to anything, §9.1). Trait requirements whose type is still a
//! variable are deferred to the end of the body.
//!
//! What is implemented: literals with expected-type typing and K0126; `let`/`var` and
//! assignment (K0710); functions, generic calls, record and variant construction; `if` and
//! `match` in statement and value form (K0731) with a shallow exhaustiveness check (K1001)
//! for `Bool`, `Option`, `Result`, enums and the "final `_` arm" rule for `Int`/`String`;
//! `for` over ranges and collections, `while`; operators with `Float`'s IEEE equality,
//! K0601/K0602; interpolation (`Show`); tagged literals typed as sinks; K0730/K1303;
//! K1101; method calls with DR-27's exactly-one rule.
//!
//! Not yet: Maranget usefulness (K1002 and nested exhaustiveness), `?` (§14.3), hole
//! requirements of tagged literals (`kalam-sinks`), record update `.with`, closures'
//! capture rules (K1110), local `fn` items, and stdlib member typing beyond declared stubs.

use std::collections::HashMap;

use kalam_base::{FileId, Span};
use kalam_resolve::DefId;

use crate::adt::{AdtDef, AdtKind, FieldDef, VariantDef, VariantField};
use crate::error::{TypeError, TypeErrorKind};
use crate::hir::{
    self, Arg, Arm, AssignOp, BinOp, Block, BodyValue, BuiltinType, Expr, ExprId, ExprKind, FnDecl,
    GenericParamDecl, IfBranch, ItemKind, LangItem, LocalId, Pat, PatKind, ResolvedProgram, Stmt,
    StmtKind, StrPart, TraitBound, TypeKind, UnOp,
};
use crate::traits::{
    Answer, BuiltinTrait, CoherenceError, ImplDecl, TraitDecl, TraitEnv, TraitId, TraitRef,
};
use crate::ty::{GenericParam, Scheme, SinkType, Suspension, Ty};
use crate::unify::{InferCtx, SuspFlow, UnifyError};

/// Everything the later passes need. Keys `(owner, …)` name the body an expression or
/// local belongs to (the function or constant that owns it).
#[derive(Debug, Default)]
pub struct TypeckResults {
    pub names: HashMap<DefId, String>,
    pub adts: HashMap<DefId, AdtDef>,
    /// Every function and method, including signature stubs.
    pub signatures: HashMap<DefId, Scheme>,
    pub consts: HashMap<DefId, Ty>,
    pub traits: TraitEnv,
    pub expr_types: HashMap<(DefId, ExprId), Ty>,
    pub local_types: HashMap<(DefId, LocalId), Ty>,
    /// Type arguments chosen at each use of a generic function, record or variant.
    pub instantiations: HashMap<(DefId, ExprId), Vec<Ty>>,
    /// The method each `MethodCall` resolved to (DR-27).
    pub method_picks: HashMap<(DefId, ExprId), DefId>,
    /// The variant an implicit member (`.Circle`) or qualified variant resolved to.
    pub variant_picks: HashMap<(DefId, ExprId), DefId>,
    /// Suspension constraints `from ≤ to` for `kalam-effects`, by owning body.
    pub flows: Vec<(DefId, SuspFlow)>,
    pub errors: Vec<TypeError>,
}

impl TypeckResults {
    pub fn name_of(&self, def: DefId) -> String {
        self.names
            .get(&def)
            .cloned()
            .unwrap_or_else(|| format!("#{}", def.0))
    }

    pub fn display(&self, ty: &Ty) -> String {
        ty.display(&|d| self.name_of(d))
    }

    pub fn expr_ty(&self, owner: DefId, e: &Expr) -> Option<&Ty> {
        self.expr_types.get(&(owner, e.id))
    }

    /// The `(code, span)` of every error, for compact assertions.
    pub fn codes(&self) -> Vec<&'static str> {
        self.errors.iter().map(TypeError::code).collect()
    }
}

/// Type-checks a whole resolved program.
pub fn check_program(prog: &dyn ResolvedProgram) -> TypeckResults {
    let mut cx = Collector {
        prog,
        res: TypeckResults::default(),
    };
    cx.collect();
    cx.check_bodies();
    cx.res
}

// ---------------------------------------------------------------------------------------
// Pass 1: collection
// ---------------------------------------------------------------------------------------

/// Where a written function type appears, which decides its suspension bit (§15.3).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pos {
    /// Directly the type of a declaration's parameter: effect-polymorphic.
    Param,
    /// Directly a local annotation: an effect variable.
    Local,
    /// Inside data (fields, payloads, collection elements, nested types): suspending.
    Data,
}

struct Collector<'p> {
    prog: &'p dyn ResolvedProgram,
    res: TypeckResults,
}

/// State for lowering one written type.
struct Lower<'a> {
    self_ty: Option<Ty>,
    pos: Pos,
    effect_params: &'a mut u32,
    infer: Option<&'a mut InferCtx>,
}

impl Collector<'_> {
    fn err(&mut self, file: FileId, span: Span, kind: TypeErrorKind) {
        self.res.errors.push(TypeError { kind, file, span });
    }

    fn collect(&mut self) {
        let ids = self.prog.item_ids();
        for &id in &ids {
            let Some(item) = self.prog.item(id) else {
                continue;
            };
            self.res.names.insert(id, item.name.clone());
            if let ItemKind::Enum(e) = &item.kind {
                for v in &e.variants {
                    self.res.names.insert(v.def, v.name.clone());
                }
            }
        }
        // ADT skeletons first, so field types can name any ADT.
        for &id in &ids {
            let Some(item) = self.prog.item(id) else {
                continue;
            };
            let (kind, derives, opaque) = match &item.kind {
                ItemKind::Record(r) => (AdtKind::Record(Vec::new()), &r.derives, r.opaque),
                ItemKind::Enum(e) => (AdtKind::Enum(Vec::new()), &e.derives, e.opaque),
                _ => continue,
            };
            self.res.adts.insert(
                id,
                AdtDef {
                    def: id,
                    name: item.name.clone(),
                    module: item.module.clone(),
                    generics: Vec::new(),
                    kind,
                    opaque,
                    derives: derives.clone(),
                },
            );
        }
        for &id in &ids {
            self.collect_adt(id);
        }
        // Traits and impls, then every function signature.
        for &id in &ids {
            self.collect_trait_or_impl(id);
        }
        for &id in &ids {
            if let Some(hir::Item {
                kind: ItemKind::Fn(f),
                ..
            }) = self.prog.item(id)
            {
                let scheme = self.fn_scheme(id, f);
                self.res.signatures.insert(id, scheme);
            }
        }
        for err in self.res.traits.check_coherence(&self.res.adts) {
            let (def, kind) = match err {
                CoherenceError::Orphan { impl_def } => (impl_def, TypeErrorKind::OrphanImpl),
                CoherenceError::Overlap { first, second } => {
                    (second, TypeErrorKind::OverlappingImpl { other: first })
                }
            };
            if let Some(item) = self.prog.item(def) {
                let (file, span) = (item.file, item.span);
                self.err(file, span, kind);
            }
        }
    }

    fn generics(&mut self, file: FileId, decls: &[GenericParamDecl]) -> Vec<GenericParam> {
        decls
            .iter()
            .map(|g| GenericParam {
                name: g.name.clone(),
                bounds: g.bounds.iter().map(|b| self.bound(file, b, None)).collect(),
            })
            .collect()
    }

    fn bound(&mut self, file: FileId, b: &TraitBound, self_ty: Option<Ty>) -> TraitRef {
        let mut n = 0;
        TraitRef {
            id: b.id,
            args: b
                .args
                .iter()
                .map(|t| {
                    let mut l = Lower {
                        self_ty: self_ty.clone(),
                        pos: Pos::Data,
                        effect_params: &mut n,
                        infer: None,
                    };
                    lower_type(
                        self.prog,
                        &self.res.adts,
                        &mut self.res.errors,
                        file,
                        t,
                        &mut l,
                    )
                })
                .collect(),
        }
    }

    fn data_ty(&mut self, file: FileId, t: &hir::Type, self_ty: Option<Ty>) -> Ty {
        let mut n = 0;
        let mut l = Lower {
            self_ty,
            pos: Pos::Data,
            effect_params: &mut n,
            infer: None,
        };
        lower_type(
            self.prog,
            &self.res.adts,
            &mut self.res.errors,
            file,
            t,
            &mut l,
        )
    }

    fn collect_adt(&mut self, id: DefId) {
        let Some(item) = self.prog.item(id) else {
            return;
        };
        let file = item.file;
        let (generics, kind) = match &item.kind {
            ItemKind::Record(r) => {
                let generics = self.generics(file, &r.generics);
                let fields = r
                    .fields
                    .iter()
                    .map(|f| FieldDef {
                        name: f.name.clone(),
                        ty: self.data_ty(file, &f.ty, None),
                        has_default: f.default.is_some(),
                    })
                    .collect();
                (generics, AdtKind::Record(fields))
            }
            ItemKind::Enum(e) => {
                let generics = self.generics(file, &e.generics);
                let variants = e
                    .variants
                    .iter()
                    .map(|v| VariantDef {
                        def: v.def,
                        name: v.name.clone(),
                        fields: v
                            .fields
                            .iter()
                            .map(|f| VariantField {
                                label: f.label.clone(),
                                ty: self.data_ty(file, &f.ty, None),
                            })
                            .collect(),
                    })
                    .collect();
                (generics, AdtKind::Enum(variants))
            }
            _ => return,
        };
        let adt = self.res.adts.get_mut(&id).expect("skeleton");
        adt.generics = generics;
        adt.kind = kind;
        // K0612: `@derive(Encode)` on a type with a `Secret` field (§9.7, §18.1).
        let adt = &self.res.adts[&id];
        if adt.derives.contains(&BuiltinTrait::Encode)
            && adt.component_types(&[]).iter().any(|t| t.contains_secret())
        {
            let span = item.span;
            self.err(file, span, TypeErrorKind::EncodeSecret);
        }
    }

    fn collect_trait_or_impl(&mut self, id: DefId) {
        let Some(item) = self.prog.item(id) else {
            return;
        };
        let file = item.file;
        let methods = |ids: &[DefId]| -> Vec<(String, DefId)> {
            ids.iter()
                .filter_map(|m| self.prog.item(*m).map(|it| (it.name.clone(), *m)))
                .collect()
        };
        match &item.kind {
            ItemKind::Trait(t) => {
                let methods = methods(&t.methods);
                let generics = self.generics(file, &t.generics);
                let tid = TraitId::User(id);
                self.res.traits.traits.insert(
                    tid,
                    TraitDecl {
                        id: tid,
                        module: Some(item.module.clone()),
                        generics,
                        methods,
                    },
                );
            }
            ItemKind::Impl(i) => {
                let methods = methods(&i.methods);
                let generics = self.generics(file, &i.generics);
                let self_ty = self.data_ty(file, &i.self_ty, None);
                let trait_ref = i
                    .as_trait
                    .as_ref()
                    .map(|b| self.bound(file, b, Some(self_ty.clone())));
                self.res.traits.impls.push(ImplDecl {
                    def: id,
                    module: item.module.clone(),
                    generics,
                    self_ty,
                    trait_ref,
                    methods,
                    span: kalam_base::FileSpan::new(file, item.span),
                });
            }
            _ => {}
        }
    }

    /// The generics and `Self` type a method sees from its parent impl or trait.
    fn parent_context(&mut self, f: &FnDecl) -> (Vec<GenericParam>, Option<Ty>) {
        let Some(parent) = f.parent else {
            return (Vec::new(), None);
        };
        let Some(item) = self.prog.item(parent) else {
            return (Vec::new(), None);
        };
        let file = item.file;
        match &item.kind {
            ItemKind::Impl(i) => {
                let generics = self.generics(file, &i.generics);
                let self_ty = self.data_ty(file, &i.self_ty, None);
                (generics, Some(self_ty))
            }
            // Inside a trait, `Self` is an extra generic parameter bounded by the trait,
            // placed after the trait's and the method's own generics.
            ItemKind::Trait(t) => (self.generics(file, &t.generics), None),
            _ => (Vec::new(), None),
        }
    }

    fn fn_scheme(&mut self, id: DefId, f: &FnDecl) -> Scheme {
        let file = self.prog.item(id).map_or(FileId::default(), |i| i.file);
        let (mut generics, mut self_ty) = self.parent_context(f);
        generics.extend(self.generics(file, &f.generics));
        if let Some(parent) = f.parent {
            if matches!(
                self.prog.item(parent).map(|i| &i.kind),
                Some(ItemKind::Trait(_))
            ) {
                self_ty = Some(Ty::Param(generics.len() as u32));
                generics.push(GenericParam {
                    name: "Self".into(),
                    bounds: vec![TraitRef {
                        id: TraitId::User(parent),
                        args: Vec::new(),
                    }],
                });
            }
        }
        let mut effects = 0u32;
        let params = f
            .params
            .iter()
            .map(|p| {
                let mut l = Lower {
                    self_ty: self_ty.clone(),
                    pos: Pos::Param,
                    effect_params: &mut effects,
                    infer: None,
                };
                lower_type(
                    self.prog,
                    &self.res.adts,
                    &mut self.res.errors,
                    file,
                    &p.ty,
                    &mut l,
                )
            })
            .collect();
        let ret = match &f.ret {
            Some(t) => self.data_ty(file, t, self_ty),
            None => Ty::Unit,
        };
        // The declaration's own bit: `@sync` fixes it; otherwise it is one more effect
        // parameter, which `kalam-effects` equates with the inferred bit of the instance.
        let own = if f.sync_attr {
            Suspension::Sync
        } else {
            effects += 1;
            Suspension::Param(effects - 1)
        };
        Scheme {
            generics,
            effect_params: effects,
            ty: Ty::func(params, ret, own),
        }
    }

    // -----------------------------------------------------------------------------------
    // Pass 2: bodies
    // -----------------------------------------------------------------------------------

    fn check_bodies(&mut self) {
        let ids = self.prog.item_ids();
        // Constants first, in the order resolve lists them (dependency order, §8.1).
        for &id in &ids {
            let Some(item) = self.prog.item(id) else {
                continue;
            };
            if let ItemKind::Const(c) = &item.kind {
                let file = item.file;
                let declared = c.ty.as_ref().map(|t| self.data_ty(file, t, None));
                let mut b = BodyCx::new(self, file, Vec::new(), Ty::Unit, &[]);
                let ty = match &declared {
                    Some(t) => {
                        b.check(&c.value, t);
                        t.clone()
                    }
                    None => b.synth(&c.value),
                };
                let ty = b.infer.resolve(&ty);
                let out = b.finish();
                self.absorb(id, out);
                self.res.consts.insert(id, ty);
            }
        }
        for &id in &ids {
            let Some(item) = self.prog.item(id) else {
                continue;
            };
            let ItemKind::Fn(f) = &item.kind else {
                continue;
            };
            let Some(body) = &f.body else { continue };
            let file = item.file;
            let scheme = self.res.signatures[&id].clone();
            let Ty::Fn { params, ret, .. } = &scheme.ty else {
                continue;
            };
            let mut b = BodyCx::new(
                self,
                file,
                scheme.generics.clone(),
                (**ret).clone(),
                &body.locals,
            );
            for (p, ty) in f.params.iter().zip(params) {
                b.locals.insert(p.local, ty.clone());
            }
            // Defaults are checked against their parameter's type (§12.1, DR-35).
            for (p, ty) in f.params.iter().zip(params) {
                if let Some(d) = &p.default {
                    b.check(d, ty);
                }
            }
            match &body.value {
                BodyValue::Expr(e) => {
                    b.check(e, ret);
                }
                BodyValue::Block(block) => {
                    b.check_stmts(block);
                    let needs_return = !matches!(**ret, Ty::Unit | Ty::Error);
                    if needs_return && !b.block_diverges(block) {
                        b.err(block.span, TypeErrorKind::MissingReturn);
                    }
                }
            }
            let out = b.finish();
            self.absorb(id, out);
        }
    }

    fn absorb(&mut self, owner: DefId, out: BodyOut) {
        for (e, t) in out.exprs {
            self.res.expr_types.insert((owner, e), t);
        }
        for (l, t) in out.locals {
            self.res.local_types.insert((owner, l), t);
        }
        for (e, t) in out.insts {
            self.res.instantiations.insert((owner, e), t);
        }
        for (e, d) in out.methods {
            self.res.method_picks.insert((owner, e), d);
        }
        for (e, d) in out.variants {
            self.res.variant_picks.insert((owner, e), d);
        }
        self.res
            .flows
            .extend(out.flows.into_iter().map(|f| (owner, f)));
        self.res.errors.extend(out.errors);
    }
}

/// Lowers written type syntax. Aliases are expanded here (`type UserId = Int` is the same
/// type as `Int`, §9.4).
fn lower_type(
    prog: &dyn ResolvedProgram,
    adts: &HashMap<DefId, AdtDef>,
    errors: &mut Vec<TypeError>,
    file: FileId,
    t: &hir::Type,
    l: &mut Lower,
) -> Ty {
    let top = l.pos;
    let mut nested = |t: &hir::Type, l: &mut Lower| {
        let saved = l.pos;
        l.pos = Pos::Data;
        let ty = lower_type(prog, adts, errors, file, t, l);
        l.pos = saved;
        ty
    };
    let arity_error = |errors: &mut Vec<TypeError>, want: usize, got: usize| {
        errors.push(TypeError {
            kind: TypeErrorKind::BadArguments {
                detail: format!("expected {want} type argument(s), found {got}"),
            },
            file,
            span: t.span,
        });
        Ty::Error
    };
    match &t.kind {
        TypeKind::Error => Ty::Error,
        TypeKind::Unit => Ty::Unit,
        TypeKind::Param(i) => Ty::Param(*i),
        TypeKind::SelfType => l.self_ty.clone().unwrap_or(Ty::Error),
        TypeKind::Tuple(ts) => Ty::Tuple(ts.iter().map(|t| nested(t, l)).collect()),
        TypeKind::Fn { sync, params, ret } => {
            let params = params.iter().map(|t| nested(t, l)).collect();
            let ret = nested(ret, l);
            let suspension = if *sync {
                Suspension::Sync
            } else {
                match top {
                    Pos::Param => {
                        *l.effect_params += 1;
                        Suspension::Param(*l.effect_params - 1)
                    }
                    Pos::Local => match l.infer.as_deref_mut() {
                        Some(cx) => cx.fresh_effect(),
                        None => Suspension::Suspending,
                    },
                    Pos::Data => Suspension::Suspending,
                }
            };
            Ty::func(params, ret, suspension)
        }
        TypeKind::Builtin(b, args) => {
            let args: Vec<Ty> = args.iter().map(|t| nested(t, l)).collect();
            let want = match b {
                BuiltinType::List
                | BuiltinType::Set
                | BuiltinType::Option
                | BuiltinType::Range
                | BuiltinType::Secret => 1,
                BuiltinType::Map | BuiltinType::Result => 2,
                _ => 0,
            };
            if args.len() != want {
                return arity_error(errors, want, args.len());
            }
            let a = |i: usize| Box::new(args[i].clone());
            match b {
                BuiltinType::Int => Ty::Int,
                BuiltinType::BigInt => Ty::BigInt,
                BuiltinType::Float => Ty::Float,
                BuiltinType::Decimal => Ty::Decimal,
                BuiltinType::Bool => Ty::Bool,
                BuiltinType::String => Ty::String,
                BuiltinType::Bytes => Ty::Bytes,
                BuiltinType::Never => Ty::Never,
                BuiltinType::AnyError => Ty::AnyError,
                BuiltinType::JsValue => Ty::JsValue,
                BuiltinType::Capability(c) => Ty::Capability(*c),
                BuiltinType::Sink(s) => Ty::Sink(*s),
                BuiltinType::List => Ty::List(a(0)),
                BuiltinType::Set => Ty::Set(a(0)),
                BuiltinType::Option => Ty::Option(a(0)),
                BuiltinType::Range => Ty::Range(a(0)),
                BuiltinType::Secret => Ty::Secret(a(0)),
                BuiltinType::Map => Ty::Map(a(0), a(1)),
                BuiltinType::Result => Ty::Result(a(0), a(1)),
            }
        }
        TypeKind::Def(d, args) => {
            let args: Vec<Ty> = args.iter().map(|t| nested(t, l)).collect();
            if let Some(adt) = adts.get(d) {
                if adt.generics.len() != args.len() && !adt.generics.is_empty() {
                    return arity_error(errors, adt.generics.len(), args.len());
                }
                return Ty::Adt(*d, args);
            }
            match prog.item(*d).map(|i| &i.kind) {
                Some(ItemKind::Alias(a)) => {
                    if a.generics.len() != args.len() {
                        return arity_error(errors, a.generics.len(), args.len());
                    }
                    // Alias targets are data positions; a cycle (`type A = A`) is resolve's
                    // error, so depth is bounded by the alias chain.
                    let target = nested(&a.target, l);
                    target.subst(&args)
                }
                _ => Ty::Error,
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Body checking
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct BodyOut {
    exprs: Vec<(ExprId, Ty)>,
    locals: Vec<(LocalId, Ty)>,
    insts: Vec<(ExprId, Vec<Ty>)>,
    methods: Vec<(ExprId, DefId)>,
    variants: Vec<(ExprId, DefId)>,
    flows: Vec<SuspFlow>,
    errors: Vec<TypeError>,
}

/// Why a trait requirement exists (decides the error when it fails).
#[derive(Clone)]
enum Origin {
    Show,
    Eq { left: Ty, right: Ty },
    Ord { op: &'static str },
    Bound,
}

struct Obligation {
    ty: Ty,
    tr: TraitRef,
    span: Span,
    origin: Origin,
}

struct BodyCx<'c, 'p> {
    cx: &'c Collector<'p>,
    file: FileId,
    generics: Vec<GenericParam>,
    ret: Ty,
    infer: InferCtx,
    locals: HashMap<LocalId, Ty>,
    mutable: Vec<bool>,
    out: BodyOut,
    obligations: Vec<Obligation>,
}

impl<'c, 'p> BodyCx<'c, 'p> {
    fn new(
        cx: &'c Collector<'p>,
        file: FileId,
        generics: Vec<GenericParam>,
        ret: Ty,
        locals: &[hir::LocalDecl],
    ) -> BodyCx<'c, 'p> {
        BodyCx {
            cx,
            file,
            generics,
            ret,
            infer: InferCtx::new(),
            locals: HashMap::new(),
            mutable: locals.iter().map(|l| l.mutable).collect(),
            out: BodyOut::default(),
            obligations: Vec::new(),
        }
    }

    fn res(&self) -> &TypeckResults {
        &self.cx.res
    }

    fn err(&mut self, span: Span, kind: TypeErrorKind) {
        self.out.errors.push(TypeError {
            kind,
            file: self.file,
            span,
        });
    }

    fn record(&mut self, e: &Expr, ty: Ty) -> Ty {
        self.out.exprs.push((e.id, ty.clone()));
        ty
    }

    fn resolve(&mut self, t: &Ty) -> Ty {
        self.infer.resolve(t)
    }

    /// Solves the deferred obligations and zonks every recorded type. Variables nobody
    /// constrained become `Error` (they can only come from already-reported errors or from
    /// values that are never used, such as an unused `None`).
    fn finish(mut self) -> BodyOut {
        let obligations = std::mem::take(&mut self.obligations);
        for o in obligations {
            let ty = self.resolve(&o.ty);
            let answer = self.answer(&ty, &o.tr);
            if answer == Answer::No {
                self.trait_error(&ty, &o.tr, o.span, o.origin);
            }
        }
        let mut out = std::mem::take(&mut self.out);
        let zonk = |t: &Ty, infer: &mut InferCtx| {
            infer.resolve(t).map(&mut |t| match t {
                Ty::Var(_) => Some(Ty::Error),
                _ => None,
            })
        };
        for (_, t) in out.exprs.iter_mut() {
            *t = zonk(t, &mut self.infer);
        }
        out.locals = self.locals.iter().map(|(l, t)| (*l, t.clone())).collect();
        for (_, t) in out.locals.iter_mut() {
            *t = zonk(t, &mut self.infer);
        }
        for (_, ts) in out.insts.iter_mut() {
            for t in ts.iter_mut() {
                *t = zonk(t, &mut self.infer);
            }
        }
        out.flows = std::mem::take(&mut self.infer.flows);
        out
    }

    fn answer(&self, ty: &Ty, tr: &TraitRef) -> Answer {
        let res = self.res();
        res.traits.implements(&res.adts, &self.generics, ty, tr)
    }

    fn trait_error(&mut self, ty: &Ty, tr: &TraitRef, span: Span, origin: Origin) {
        let trait_name = match tr.id {
            TraitId::Builtin(b) => b.name().to_string(),
            TraitId::User(d) => self.res().name_of(d),
        };
        let kind = match origin {
            Origin::Eq { left, right } => TypeErrorKind::BadEquality { left, right },
            Origin::Ord { op } => TypeErrorKind::NoOperator { op, ty: ty.clone() },
            Origin::Show | Origin::Bound => TypeErrorKind::MissingTrait {
                ty: ty.clone(),
                trait_name,
            },
        };
        self.err(span, kind);
    }

    /// Requires `ty: tr` now, or at the end of the body if `ty` is still unknown.
    fn require(&mut self, ty: &Ty, tr: TraitRef, span: Span, origin: Origin) {
        let ty = self.resolve(ty);
        match self.answer(&ty, &tr) {
            Answer::Yes => {}
            Answer::No => self.trait_error(&ty, &tr, span, origin),
            Answer::Unknown => self.obligations.push(Obligation {
                ty,
                tr,
                span,
                origin,
            }),
        }
    }

    /// `found` flows where `expected` is required. `Never` coerces to anything (§9.1).
    fn coerce(&mut self, found: &Ty, expected: &Ty, span: Span) {
        if matches!(self.infer.shallow(found), Ty::Never) {
            return;
        }
        self.infer.span = span;
        match self.infer.unify(found, expected) {
            Ok(()) => {}
            Err(UnifyError::Occurs { ty, .. }) => {
                let ty = self.resolve(&ty);
                self.err(span, TypeErrorKind::InfiniteType { ty });
            }
            Err(UnifyError::Mismatch { .. }) => {
                let (found, expected) = (self.resolve(found), self.resolve(expected));
                let kind = if found.is_numeric() && expected.is_numeric() {
                    TypeErrorKind::MixedNumeric {
                        left: expected,
                        right: found,
                    }
                } else {
                    TypeErrorKind::Mismatch { expected, found }
                };
                self.err(span, kind);
            }
        }
    }

    fn lower_local_type(&mut self, t: &hir::Type) -> Ty {
        let mut n = 0;
        let mut l = Lower {
            self_ty: None,
            pos: Pos::Local,
            effect_params: &mut n,
            infer: Some(&mut self.infer),
        };
        lower_type(
            self.cx.prog,
            &self.cx.res.adts,
            &mut self.out.errors,
            self.file,
            t,
            &mut l,
        )
    }

    // -- expressions --------------------------------------------------------------------

    /// Checks `e` against `expected` and returns the type recorded for `e`.
    fn check(&mut self, e: &Expr, expected: &Ty) -> Ty {
        let exp = self.infer.shallow(expected);
        match &e.kind {
            ExprKind::Int(v) => {
                let t = self.int_literal(*v, false, &exp, e.span);
                self.coerce(&t, &exp, e.span);
                self.record(e, t)
            }
            ExprKind::Float(_) => {
                let t = if exp == Ty::Decimal {
                    Ty::Decimal
                } else {
                    Ty::Float
                };
                self.coerce(&t, &exp, e.span);
                self.record(e, t)
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } if matches!(operand.kind, ExprKind::Int(_)) => {
                let ExprKind::Int(v) = operand.kind else {
                    unreachable!()
                };
                let t = self.int_literal(v, true, &exp, e.span);
                self.record(operand, t.clone());
                self.coerce(&t, &exp, e.span);
                self.record(e, t)
            }
            ExprKind::List(items) => match &exp {
                Ty::List(elem) => {
                    let elem = (**elem).clone();
                    for item in items {
                        self.check(item, &elem);
                    }
                    self.record(e, exp.clone())
                }
                _ => {
                    let t = self.synth(e);
                    self.coerce(&t, &exp, e.span);
                    t
                }
            },
            ExprKind::Tuple(items) => match &exp {
                Ty::Tuple(ts) if ts.len() == items.len() => {
                    let ts = ts.clone();
                    for (item, t) in items.iter().zip(&ts) {
                        self.check(item, t);
                    }
                    self.record(e, exp.clone())
                }
                _ => {
                    let t = self.synth(e);
                    self.coerce(&t, &exp, e.span);
                    t
                }
            },
            ExprKind::ImplicitMember(name) => {
                let t = self.variant_value(e, None, name, &[], Some(&exp));
                self.coerce(&t, &exp, e.span);
                self.record(e, t)
            }
            ExprKind::Call { callee, args } => {
                let t = self.call(e, callee, args, Some(&exp));
                self.coerce(&t, &exp, e.span);
                self.record(e, t)
            }
            ExprKind::If {
                branches,
                else_body,
            } => {
                let t = self.if_value(e, branches, else_body.as_ref(), Some(&exp));
                self.record(e, t)
            }
            ExprKind::Match { scrutinee, arms } => {
                let t = self.match_expr(e, scrutinee, arms, Some(&exp), true);
                self.record(e, t)
            }
            ExprKind::Lambda { params, body } => {
                let t = self.lambda(e, params, body, Some(&exp));
                self.coerce(&t, &exp, e.span);
                self.record(e, t)
            }
            _ => {
                let t = self.synth(e);
                self.coerce(&t, &exp, e.span);
                t
            }
        }
    }

    /// Infers the type of `e` with no expectation.
    fn synth(&mut self, e: &Expr) -> Ty {
        let t = match &e.kind {
            ExprKind::Error => Ty::Error,
            ExprKind::Int(v) => self.int_literal(*v, false, &Ty::Int, e.span),
            ExprKind::Float(_) => Ty::Float,
            ExprKind::Str(_) => Ty::String,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Unit => Ty::Unit,
            ExprKind::Interp(parts) => {
                for p in parts {
                    if let StrPart::Hole(h) = p {
                        let t = self.synth(h);
                        self.require(
                            &t,
                            TraitRef::builtin(BuiltinTrait::Show),
                            h.span,
                            Origin::Show,
                        );
                    }
                }
                Ty::String
            }
            ExprKind::Tagged { tag, holes, .. } => {
                // Hole requirements come from `kalam-sinks::check_template` (Phase 1 there);
                // until then holes are only type-checked.
                for h in holes {
                    self.synth(h);
                }
                Ty::Sink(SinkType::of_tag(*tag))
            }
            ExprKind::Local(l) => self.locals.get(l).cloned().unwrap_or(Ty::Error),
            ExprKind::Def(d) => self.def_value(e, *d),
            ExprKind::Lang(item) => self.lang_value(*item),
            ExprKind::ImplicitMember(name) => {
                self.err(
                    e.span,
                    TypeErrorKind::ImplicitMemberNeedsType { name: name.clone() },
                );
                Ty::Error
            }
            ExprKind::Call { callee, args } => self.call(e, callee, args, None),
            ExprKind::MethodCall {
                receiver,
                name,
                args,
            } => self.method_call(e, receiver, name, args),
            ExprKind::Field { base, name } => self.field(base, name, e.span),
            ExprKind::TupleField { base, index } => {
                let bt = self.synth(base);
                match self.resolve(&bt) {
                    Ty::Tuple(ts) if (*index as usize) < ts.len() => ts[*index as usize].clone(),
                    Ty::Error => Ty::Error,
                    other => {
                        self.err(
                            e.span,
                            TypeErrorKind::UnknownMember {
                                ty: other,
                                name: index.to_string(),
                            },
                        );
                        Ty::Error
                    }
                }
            }
            ExprKind::Index { base, index } => self.index(base, index, e.span),
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
            } => {
                self.check(operand, &Ty::Bool);
                Ty::Bool
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                if let ExprKind::Int(v) = operand.kind {
                    let t = self.int_literal(v, true, &Ty::Int, e.span);
                    self.record(operand, t.clone());
                    t
                } else {
                    let t = self.synth(operand);
                    let r = self.resolve(&t);
                    if !(r.is_numeric() || matches!(r, Ty::Error | Ty::Var(_))) {
                        self.err(e.span, TypeErrorKind::NoOperator { op: "-", ty: r });
                        Ty::Error
                    } else {
                        t
                    }
                }
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, e.span),
            ExprKind::Range { lo, hi, .. } => {
                self.check(lo, &Ty::Int);
                self.check(hi, &Ty::Int);
                Ty::Range(Box::new(Ty::Int))
            }
            ExprKind::Tuple(items) => Ty::Tuple(items.iter().map(|i| self.synth(i)).collect()),
            ExprKind::List(items) => match items.split_first() {
                None => {
                    self.err(e.span, TypeErrorKind::EmptyCollectionNeedsType);
                    Ty::list(Ty::Error)
                }
                Some((first, rest)) => {
                    let t = self.synth(first);
                    for item in rest {
                        self.check(item, &t);
                    }
                    Ty::list(t)
                }
            },
            ExprKind::If {
                branches,
                else_body,
            } => self.if_value(e, branches, else_body.as_ref(), None),
            ExprKind::Match { scrutinee, arms } => self.match_expr(e, scrutinee, arms, None, true),
            ExprKind::Lambda { params, body } => self.lambda(e, params, body, None),
        };
        self.record(e, t)
    }

    /// Spec §5.6: an integer literal is `Int` unless the expected type is `Float`,
    /// `Decimal` or `BigInt`. K0126 when an `Int` literal does not fit in 64 bits; the
    /// direct operand of unary `-` counts as negative, so −2⁶³ is legal.
    fn int_literal(&mut self, value: Option<u128>, negative: bool, exp: &Ty, span: Span) -> Ty {
        let t = match exp {
            Ty::Float | Ty::Decimal | Ty::BigInt => exp.clone(),
            _ => Ty::Int,
        };
        if t == Ty::Int {
            let limit = if negative {
                1u128 << 63
            } else {
                (1u128 << 63) - 1
            };
            if value.map_or(true, |v| v > limit) {
                self.err(span, TypeErrorKind::LiteralOutOfRange { ty: Ty::Int });
            }
        }
        t
    }

    fn def_value(&mut self, e: &Expr, d: DefId) -> Ty {
        if let Some(scheme) = self.res().signatures.get(&d).cloned() {
            let (ty, args) = self.infer.instantiate(&scheme);
            self.require_bounds(&scheme, &args, e.span);
            self.out.insts.push((e.id, args));
            return ty;
        }
        if let Some(t) = self.res().consts.get(&d) {
            return t.clone();
        }
        if let Some((adt, idx)) = self.variant_owner(d) {
            let name = match &self.res().adts[&adt].kind {
                AdtKind::Enum(vs) => vs[idx].name.clone(),
                AdtKind::Record(_) => String::new(),
            };
            return self.variant_value(e, Some(adt), &name, &[], None);
        }
        Ty::Error
    }

    fn require_bounds(&mut self, scheme: &Scheme, args: &[Ty], span: Span) {
        for (g, arg) in scheme.generics.iter().zip(args) {
            for b in &g.bounds {
                let tr = TraitRef {
                    id: b.id,
                    args: b.args.iter().map(|t| t.subst(args)).collect(),
                };
                self.require(arg, tr, span, Origin::Bound);
            }
        }
    }

    fn variant_owner(&self, d: DefId) -> Option<(DefId, usize)> {
        self.res()
            .adts
            .values()
            .find_map(|a| a.variant_by_def(d).map(|(i, _)| (a.def, i)))
    }

    /// The type of a core value used without a call (`None`, or `Some` as a function).
    fn lang_value(&mut self, item: LangItem) -> Ty {
        let fresh = |s: &mut Self| s.infer.fresh();
        match item {
            LangItem::None => Ty::option(fresh(self)),
            LangItem::Some => {
                let t = fresh(self);
                Ty::func(vec![t.clone()], Ty::option(t), Suspension::Sync)
            }
            LangItem::Ok => {
                let (t, e) = (fresh(self), fresh(self));
                Ty::func(vec![t.clone()], Ty::result(t, e), Suspension::Sync)
            }
            LangItem::Err => {
                let (t, e) = (fresh(self), fresh(self));
                Ty::func(vec![e.clone()], Ty::result(t, e), Suspension::Sync)
            }
            LangItem::Print => {
                let t = fresh(self);
                Ty::func(vec![t], Ty::Unit, Suspension::Sync)
            }
            LangItem::Panic => Ty::func(vec![Ty::String], Ty::Never, Suspension::Sync),
            LangItem::Assert => Ty::func(vec![Ty::Bool, Ty::String], Ty::Unit, Suspension::Sync),
        }
    }

    fn call(&mut self, e: &Expr, callee: &Expr, args: &[Arg], exp: Option<&Ty>) -> Ty {
        let exp = exp.map(|t| self.infer.shallow(t));
        match &callee.kind {
            ExprKind::Lang(item) => self.lang_call(e, callee, *item, args, exp.as_ref()),
            ExprKind::ImplicitMember(name) => {
                let t = self.variant_value(e, None, name, args, exp.as_ref());
                self.record(callee, t.clone());
                t
            }
            ExprKind::Def(d) if self.res().adts.contains_key(d) => {
                let t = self.construct_record(e, *d, args, exp.as_ref());
                self.record(callee, t.clone());
                t
            }
            ExprKind::Def(d) if self.variant_owner(*d).is_some() => {
                let (adt, idx) = self.variant_owner(*d).expect("checked");
                let name = match &self.res().adts[&adt].kind {
                    AdtKind::Enum(vs) => vs[idx].name.clone(),
                    AdtKind::Record(_) => String::new(),
                };
                let t = self.variant_value(e, Some(adt), &name, args, exp.as_ref());
                self.record(callee, t.clone());
                t
            }
            ExprKind::Def(d) if self.res().signatures.contains_key(d) => {
                let ft = self.synth(callee);
                let Ty::Fn { params, ret, .. } = ft else {
                    return Ty::Error;
                };
                // Return-type inference from the expected type (§9.9 rule 5).
                if let Some(x) = &exp {
                    let _ = self.infer.try_unify(&ret, x);
                }
                let decl = match self.cx.prog.item(*d).map(|i| &i.kind) {
                    Some(ItemKind::Fn(f)) => f
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.default.is_some()))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                self.args_by_name(e.span, &decl, &params, args);
                *ret
            }
            _ => {
                let ft = self.synth(callee);
                match self.infer.shallow(&ft) {
                    Ty::Fn { params, ret, .. } => {
                        if args.iter().any(|a| a.label.is_some()) || args.len() != params.len() {
                            self.bad_args(
                                e.span,
                                format!("expected {} positional argument(s)", params.len()),
                            );
                        }
                        for (a, p) in args.iter().zip(&params) {
                            self.check(&a.value, p);
                        }
                        *ret
                    }
                    Ty::Error => {
                        for a in args {
                            self.synth(&a.value);
                        }
                        Ty::Error
                    }
                    Ty::Var(_) => {
                        let params: Vec<Ty> = args.iter().map(|a| self.synth(&a.value)).collect();
                        let ret = self.infer.fresh();
                        let s = self.infer.fresh_effect();
                        let f = Ty::func(params, ret.clone(), s);
                        self.coerce(&ft, &f, callee.span);
                        ret
                    }
                    other => {
                        self.err(callee.span, TypeErrorKind::NotCallable { ty: other });
                        Ty::Error
                    }
                }
            }
        }
    }

    fn bad_args(&mut self, span: Span, detail: String) {
        self.err(span, TypeErrorKind::BadArguments { detail });
    }

    /// Positional arguments first, then named ones; defaults fill the rest (§12.1).
    fn args_by_name(&mut self, span: Span, decl: &[(String, bool)], params: &[Ty], args: &[Arg]) {
        let mut slots: Vec<Option<&Arg>> = vec![None; params.len()];
        let mut seen_named = false;
        for (i, a) in args.iter().enumerate() {
            let slot = match &a.label {
                None if seen_named => {
                    self.bad_args(a.span, "positional argument after a named one".into());
                    None
                }
                None => (i < params.len()).then_some(i),
                Some(l) => {
                    seen_named = true;
                    let found = decl.iter().position(|(n, _)| n == l);
                    if found.is_none() {
                        self.bad_args(a.span, format!("no parameter named `{l}`"));
                    }
                    found
                }
            };
            match slot {
                Some(s) if slots[s].is_some() => {
                    self.bad_args(a.span, "argument given twice".into());
                }
                Some(s) => slots[s] = Some(a),
                None if a.label.is_none() && !seen_named => {
                    self.bad_args(
                        a.span,
                        format!("expected at most {} argument(s)", params.len()),
                    );
                }
                None => {}
            }
            if slot.is_none() {
                self.synth(&a.value);
            }
        }
        for (i, slot) in slots.iter().enumerate() {
            match slot {
                Some(a) => {
                    self.check(&a.value, &params[i]);
                }
                None if decl.get(i).is_some_and(|(_, has_default)| *has_default) => {}
                None => {
                    let name = decl.get(i).map(|(n, _)| n.as_str()).unwrap_or("?");
                    self.bad_args(span, format!("missing argument `{name}`"));
                }
            }
        }
    }

    fn lang_call(
        &mut self,
        e: &Expr,
        callee: &Expr,
        item: LangItem,
        args: &[Arg],
        exp: Option<&Ty>,
    ) -> Ty {
        let callee_ty = self.lang_value(item);
        self.record(callee, callee_ty);
        let arity = |s: &mut Self, n: usize| {
            if args.len() != n {
                s.bad_args(
                    e.span,
                    format!("expected {n} argument(s), found {}", args.len()),
                );
            }
        };
        match item {
            // `print[T: Show](value: T)` (§9.7: `Show` is what `print` uses).
            LangItem::Print => {
                if args.len() > 1 {
                    self.err(e.span, TypeErrorKind::PrintArity);
                } else {
                    arity(self, 1);
                }
                for a in args {
                    let t = self.synth(&a.value);
                    self.require(
                        &t,
                        TraitRef::builtin(BuiltinTrait::Show),
                        a.value.span,
                        Origin::Show,
                    );
                }
                Ty::Unit
            }
            LangItem::Panic => {
                arity(self, 1);
                for a in args.iter().take(1) {
                    self.check(&a.value, &Ty::String);
                }
                Ty::Never
            }
            LangItem::Assert => {
                arity(self, 2);
                for (a, t) in args.iter().zip([Ty::Bool, Ty::String]) {
                    self.check(&a.value, &t);
                }
                Ty::Unit
            }
            LangItem::None => {
                self.err(
                    callee.span,
                    TypeErrorKind::NotCallable {
                        ty: Ty::option(Ty::Error),
                    },
                );
                Ty::Error
            }
            LangItem::Some | LangItem::Ok | LangItem::Err => {
                arity(self, 1);
                let (whole, inner) = match item {
                    LangItem::Some => {
                        let t = match exp {
                            Some(Ty::Option(t)) => (**t).clone(),
                            _ => self.infer.fresh(),
                        };
                        (Ty::option(t.clone()), t)
                    }
                    _ => {
                        let (t, er) = match exp {
                            Some(Ty::Result(t, er)) => ((**t).clone(), (**er).clone()),
                            _ => (self.infer.fresh(), self.infer.fresh()),
                        };
                        let inner = if item == LangItem::Ok {
                            t.clone()
                        } else {
                            er.clone()
                        };
                        (Ty::result(t, er), inner)
                    }
                };
                if let Some(a) = args.first() {
                    self.check(&a.value, &inner);
                }
                whole
            }
        }
    }

    /// Record construction: named arguments only (K0416); defaulted fields may be omitted.
    fn construct_record(&mut self, e: &Expr, def: DefId, args: &[Arg], exp: Option<&Ty>) -> Ty {
        let adt = self.res().adts[&def].clone();
        let AdtKind::Record(fields) = &adt.kind else {
            // An enum name called like a function.
            self.err(
                e.span,
                TypeErrorKind::NotCallable {
                    ty: Ty::Adt(def, vec![]),
                },
            );
            return Ty::Error;
        };
        let targs: Vec<Ty> = adt.generics.iter().map(|_| self.infer.fresh()).collect();
        let ty = Ty::Adt(def, targs.clone());
        if let Some(x) = exp {
            let _ = self.infer.try_unify(&ty, x);
        }
        self.out.insts.push((e.id, targs.clone()));
        if args.iter().any(|a| a.label.is_none()) {
            self.err(e.span, TypeErrorKind::RecordNeedsNamedArgs);
        }
        let mut given = vec![false; fields.len()];
        for (i, a) in args.iter().enumerate() {
            // After K0416, a positional argument is taken as the field at its position so
            // the mistake is reported once, not again as a missing field.
            let field = match &a.label {
                Some(l) => fields.iter().position(|f| &f.name == l),
                None => (i < fields.len()).then_some(i),
            };
            match field {
                Some(i) if given[i] => {
                    self.bad_args(a.span, "field given twice".into());
                    self.synth(&a.value);
                }
                Some(i) => {
                    given[i] = true;
                    let ft = fields[i].ty.subst(&targs);
                    self.check(&a.value, &ft);
                }
                None => {
                    if let Some(l) = &a.label {
                        self.bad_args(a.span, format!("no field named `{l}`"));
                    }
                    self.synth(&a.value);
                }
            }
        }
        for (f, g) in fields.iter().zip(&given) {
            if !g && !f.has_default {
                self.bad_args(e.span, format!("missing field `{}`", f.name));
            }
        }
        ty
    }

    /// A variant value or construction: `.Circle(radius: 2.0)`, `Shape.Empty`. With
    /// `adt = None` the enum comes from the expected type (K0803 without one).
    fn variant_value(
        &mut self,
        e: &Expr,
        adt: Option<DefId>,
        name: &str,
        args: &[Arg],
        exp: Option<&Ty>,
    ) -> Ty {
        let exp = exp.map(|t| self.infer.shallow(t));
        let adt = match (adt, &exp) {
            (Some(a), _) => a,
            (None, Some(Ty::Adt(a, _))) if self.res().adts.contains_key(a) => *a,
            (None, Some(Ty::Error)) => return Ty::Error,
            _ => {
                self.err(
                    e.span,
                    TypeErrorKind::ImplicitMemberNeedsType { name: name.into() },
                );
                for a in args {
                    self.synth(&a.value);
                }
                return Ty::Error;
            }
        };
        let def = self.res().adts[&adt].clone();
        let targs: Vec<Ty> = def.generics.iter().map(|_| self.infer.fresh()).collect();
        let ty = Ty::Adt(adt, targs.clone());
        if let Some(x) = &exp {
            let _ = self.infer.try_unify(&ty, x);
        }
        let Some((_, variant)) = def.variant(name) else {
            self.err(
                e.span,
                TypeErrorKind::UnknownMember {
                    ty: ty.clone(),
                    name: name.into(),
                },
            );
            return Ty::Error;
        };
        self.out.variants.push((e.id, variant.def));
        self.out.insts.push((e.id, targs.clone()));
        let fields = variant.fields.clone();
        let single = fields.len() == 1;
        if args.len() != fields.len() {
            self.bad_args(
                e.span,
                format!(
                    "`{name}` takes {} field(s), found {}",
                    fields.len(),
                    args.len()
                ),
            );
        }
        for (i, a) in args.iter().enumerate() {
            let slot = match &a.label {
                Some(l) => fields.iter().position(|f| f.label.as_deref() == Some(l)),
                // Positional: positional fields, or a single named field (§9.5).
                None if single || fields.get(i).is_some_and(|f| f.label.is_none()) => Some(i),
                None => None,
            };
            match slot.and_then(|s| fields.get(s)) {
                Some(f) => {
                    let ft = f.ty.subst(&targs);
                    self.check(&a.value, &ft);
                }
                None => {
                    self.bad_args(a.span, format!("`{name}` has no such field"));
                    self.synth(&a.value);
                }
            }
        }
        ty
    }

    fn method_call(&mut self, e: &Expr, receiver: &Expr, name: &str, args: &[Arg]) -> Ty {
        let rt = self.synth(receiver);
        let rt = self.resolve(&rt);
        if matches!(rt, Ty::Error | Ty::Var(_)) {
            for a in args {
                self.synth(&a.value);
            }
            return Ty::Error;
        }
        let candidates = self.res().traits.method_candidates(&rt, name);
        let method = match candidates.as_slice() {
            [one] => one.method,
            [] => {
                self.err(
                    e.span,
                    TypeErrorKind::UnknownMember {
                        ty: rt,
                        name: name.into(),
                    },
                );
                for a in args {
                    self.synth(&a.value);
                }
                return Ty::Error;
            }
            many => {
                let candidates = many.iter().map(|c| c.method).collect();
                self.err(
                    e.span,
                    TypeErrorKind::AmbiguousMethod {
                        name: name.into(),
                        candidates,
                    },
                );
                return Ty::Error;
            }
        };
        self.out.methods.push((e.id, method));
        let Some(scheme) = self.res().signatures.get(&method).cloned() else {
            return Ty::Error;
        };
        let (ft, targs) = self.infer.instantiate(&scheme);
        self.require_bounds(&scheme, &targs, e.span);
        self.out.insts.push((e.id, targs));
        let Ty::Fn { params, ret, .. } = ft else {
            return Ty::Error;
        };
        let has_self = matches!(
            self.cx.prog.item(method).map(|i| &i.kind),
            Some(ItemKind::Fn(f)) if f.has_self
        );
        if !has_self || params.is_empty() {
            self.bad_args(
                e.span,
                format!("`{name}` is not a method; call it on the type"),
            );
            return *ret;
        }
        self.coerce(&rt, &params[0], receiver.span);
        let decl = match self.cx.prog.item(method).map(|i| &i.kind) {
            Some(ItemKind::Fn(f)) => f
                .params
                .iter()
                .skip(1)
                .map(|p| (p.name.clone(), p.default.is_some()))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        self.args_by_name(e.span, &decl, &params[1..], args);
        *ret
    }

    fn field(&mut self, base: &Expr, name: &str, span: Span) -> Ty {
        let bt = self.synth(base);
        let bt = self.resolve(&bt);
        if let Ty::Adt(d, targs) = &bt {
            if let Some((_, f)) = self.res().adts.get(d).and_then(|a| a.field(name)) {
                return f.ty.subst(targs);
            }
        }
        if !matches!(bt, Ty::Error) {
            self.err(
                span,
                TypeErrorKind::UnknownMember {
                    ty: bt,
                    name: name.into(),
                },
            );
        }
        Ty::Error
    }

    fn index(&mut self, base: &Expr, index: &Expr, span: Span) -> Ty {
        let bt = self.synth(base);
        match self.resolve(&bt) {
            Ty::List(t) => {
                self.check(index, &Ty::Int);
                *t
            }
            Ty::Map(k, v) => {
                self.check(index, &k);
                *v
            }
            Ty::String => {
                self.synth(index);
                self.err(span, TypeErrorKind::StringIndex);
                Ty::Error
            }
            Ty::Error => {
                self.synth(index);
                Ty::Error
            }
            other => {
                self.synth(index);
                self.err(
                    span,
                    TypeErrorKind::NoOperator {
                        op: "[]",
                        ty: other,
                    },
                );
                Ty::Error
            }
        }
    }

    fn is_literal(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Float(_) => true,
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => Self::is_literal(operand),
            _ => false,
        }
    }

    /// Types a numeric literal using `hint` as its context without coercing, so a
    /// mismatch is reported once, by the operator (K0601), not as a plain mismatch.
    fn operand(&mut self, e: &Expr, hint: &Ty) -> Ty {
        let hint = self.resolve(hint);
        match &e.kind {
            ExprKind::Int(v) => {
                let t = self.int_literal(*v, false, &hint, e.span);
                self.record(e, t)
            }
            ExprKind::Float(_) => {
                let t = if hint == Ty::Decimal {
                    Ty::Decimal
                } else {
                    Ty::Float
                };
                self.record(e, t)
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } if matches!(operand.kind, ExprKind::Int(_)) => {
                let ExprKind::Int(v) = operand.kind else {
                    unreachable!()
                };
                let t = self.int_literal(v, true, &hint, e.span);
                self.record(operand, t.clone());
                self.record(e, t)
            }
            _ => self.synth(e),
        }
    }

    fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, span: Span) -> Ty {
        if matches!(op, BinOp::And | BinOp::Or) {
            self.check(lhs, &Ty::Bool);
            self.check(rhs, &Ty::Bool);
            return Ty::Bool;
        }
        // A literal operand takes its type from the other side, whichever side it is on.
        let (lt, rt) = if Self::is_literal(lhs) && !Self::is_literal(rhs) {
            let rt = self.synth(rhs);
            (self.operand(lhs, &rt), rt)
        } else {
            let lt = self.synth(lhs);
            (lt.clone(), self.operand(rhs, &lt))
        };
        let (l, r) = (self.resolve(&lt), self.resolve(&rt));
        if matches!(l, Ty::Error) || matches!(r, Ty::Error) {
            return match op {
                BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    Ty::Bool
                }
                _ => Ty::Error,
            };
        }
        let is_eq = matches!(op, BinOp::Eq | BinOp::NotEq);
        if l.is_numeric() && r.is_numeric() && l != r {
            self.err(span, TypeErrorKind::MixedNumeric { left: l, right: r });
            return if is_eq || matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge) {
                Ty::Bool
            } else {
                Ty::Error
            };
        }
        let never = matches!(l, Ty::Never) || matches!(r, Ty::Never);
        if !never && self.infer.unify(&rt, &lt).is_err() {
            let kind = if is_eq {
                TypeErrorKind::BadEquality { left: l, right: r }
            } else {
                TypeErrorKind::Mismatch {
                    expected: l,
                    found: r,
                }
            };
            self.err(span, kind);
            return if is_eq { Ty::Bool } else { Ty::Error };
        }
        let t = self.resolve(&lt);
        let open = matches!(t, Ty::Var(_) | Ty::Never);
        let no_op = |s: &mut Self, t: Ty| {
            s.err(
                span,
                TypeErrorKind::NoOperator {
                    op: op.symbol(),
                    ty: t,
                },
            );
            Ty::Error
        };
        match op {
            BinOp::Eq | BinOp::NotEq => {
                // Float `==` is the built-in IEEE operator (§9.2); everything else needs `Eq`.
                if t != Ty::Float {
                    let origin = Origin::Eq {
                        left: t.clone(),
                        right: t.clone(),
                    };
                    self.require(&t, TraitRef::builtin(BuiltinTrait::Eq), span, origin);
                }
                Ty::Bool
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                if t != Ty::Float {
                    let origin = Origin::Ord { op: op.symbol() };
                    self.require(&t, TraitRef::builtin(BuiltinTrait::Ord), span, origin);
                }
                Ty::Bool
            }
            BinOp::Add => match t {
                _ if open || t.is_numeric() => t,
                Ty::String | Ty::List(_) | Ty::Sink(SinkType::Sql | SinkType::Html) => t,
                _ => no_op(self, t),
            },
            BinOp::Sub | BinOp::Mul => {
                if open || t.is_numeric() {
                    t
                } else {
                    no_op(self, t)
                }
            }
            // `/` on two `Int`s is `Float`; `Decimal` has no `/` (use `.div`, §9.2).
            BinOp::Div => match t {
                Ty::Int | Ty::Float => Ty::Float,
                _ if open => t,
                _ => no_op(self, t),
            },
            BinOp::IntDiv | BinOp::Rem => match t {
                Ty::Int | Ty::BigInt => t,
                _ if open => t,
                _ => no_op(self, t),
            },
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }

    fn lambda(
        &mut self,
        e: &Expr,
        params: &[hir::LambdaParam],
        body: &Expr,
        exp: Option<&Ty>,
    ) -> Ty {
        let exp = exp.map(|t| self.infer.shallow(t));
        let (exp_params, exp_ret) = match &exp {
            Some(Ty::Fn {
                params: ps, ret, ..
            }) if ps.len() == params.len() => (Some(ps.clone()), Some((**ret).clone())),
            _ => (None, None),
        };
        let mut ptys = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let t = match (&p.ty, &exp_params) {
                (Some(t), _) => self.lower_local_type(t),
                (None, Some(ps)) => ps[i].clone(),
                (None, None) => {
                    self.err(e.span, TypeErrorKind::LambdaNeedsTypes);
                    Ty::Error
                }
            };
            self.locals.insert(p.local, t.clone());
            ptys.push(t);
        }
        let ret = match &exp_ret {
            Some(r) => self.check(body, r),
            None => self.synth(body),
        };
        // A lambda's bit comes from its body (§15.3): an effect variable for `kalam-effects`.
        let s = self.infer.fresh_effect();
        Ty::func(ptys, ret, s)
    }

    // -- if / match -----------------------------------------------------------------------

    /// Value-form `if` (§10.3): `else` is required (K0731); all arms have one type.
    fn if_value(
        &mut self,
        e: &Expr,
        branches: &[IfBranch],
        else_body: Option<&Block>,
        exp: Option<&Ty>,
    ) -> Ty {
        let mut result = exp.cloned();
        for b in branches {
            self.check(&b.cond, &Ty::Bool);
            self.arm_value(&b.body, &mut result);
        }
        match else_body {
            Some(eb) => self.arm_value(eb, &mut result),
            None => {
                self.err(e.span, TypeErrorKind::IfWithoutElse);
                return Ty::Error;
            }
        }
        result.unwrap_or(Ty::Never)
    }

    /// Checks one arm of a value-form `if`/`match` and joins its type into `result`.
    fn arm_value(&mut self, body: &Block, result: &mut Option<Ty>) {
        let t = self.block_value(body, result.as_ref());
        if result.is_none() && !matches!(self.infer.shallow(&t), Ty::Never) {
            *result = Some(t);
        }
    }

    /// The value of a block in value position: its last expression (§10.3).
    fn block_value(&mut self, block: &Block, exp: Option<&Ty>) -> Ty {
        let Some((last, init)) = block.stmts.split_last() else {
            let t = Ty::Unit;
            if let Some(x) = exp {
                self.coerce(&t, x, block.span);
            }
            return t;
        };
        for s in init {
            self.stmt(s);
        }
        match &last.kind {
            StmtKind::Expr(e) => match exp {
                Some(x) => self.check(e, x),
                None => self.synth(e),
            },
            StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => {
                self.stmt(last);
                Ty::Never
            }
            _ => {
                self.stmt(last);
                let t = Ty::Unit;
                if let Some(x) = exp {
                    self.coerce(&t, x, last.span);
                }
                t
            }
        }
    }

    fn match_expr(
        &mut self,
        e: &Expr,
        scrutinee: &Expr,
        arms: &[Arm],
        exp: Option<&Ty>,
        value: bool,
    ) -> Ty {
        let st = self.synth(scrutinee);
        let mut result = exp.cloned();
        for arm in arms {
            self.pat(&arm.pat, &st);
            if let Some(g) = &arm.guard {
                self.check(g, &Ty::Bool);
            }
            if value {
                self.arm_value(&arm.body, &mut result);
            } else {
                self.check_stmts(&arm.body);
            }
        }
        let st = self.resolve(&st);
        let missing = self.missing_cases(&st, arms);
        if !missing.is_empty() {
            self.err(e.span, TypeErrorKind::NonExhaustive { missing });
        }
        if value {
            result.unwrap_or(Ty::Never)
        } else {
            Ty::Unit
        }
    }

    /// A shallow exhaustiveness check over the top-level constructors (Maranget's full
    /// algorithm replaces it). Only unguarded arms count (§11 rule 2).
    fn missing_cases(&self, st: &Ty, arms: &[Arm]) -> Vec<String> {
        let mut pats: Vec<&Pat> = Vec::new();
        fn flatten<'a>(p: &'a Pat, out: &mut Vec<&'a Pat>) {
            match &p.kind {
                PatKind::Or(ps) => ps.iter().for_each(|p| flatten(p, out)),
                _ => out.push(p),
            }
        }
        for a in arms.iter().filter(|a| a.guard.is_none()) {
            flatten(&a.pat, &mut pats);
        }
        if pats.iter().any(|p| p.is_irrefutable()) {
            return Vec::new();
        }
        let covers = |want: &dyn Fn(&PatKind) -> bool| pats.iter().any(|p| want(&p.kind));
        let full = |inner: &Option<Box<Pat>>| inner.as_ref().map_or(true, |p| p.is_irrefutable());
        match st {
            Ty::Bool => [true, false]
                .into_iter()
                .filter(|b| !covers(&|k| matches!(k, PatKind::Bool(x) if x == b)))
                .map(|b| b.to_string())
                .collect(),
            Ty::Option(_) => {
                let mut m = Vec::new();
                if !covers(
                    &|k| matches!(k, PatKind::Lang { item: LangItem::Some, inner } if full(inner)),
                ) {
                    m.push("Some(_)".into());
                }
                if !covers(&|k| {
                    matches!(
                        k,
                        PatKind::Lang {
                            item: LangItem::None,
                            ..
                        }
                    )
                }) {
                    m.push("None".into());
                }
                m
            }
            Ty::Result(_, _) => {
                let mut m = Vec::new();
                for (item, text) in [(LangItem::Ok, "Ok(_)"), (LangItem::Err, "Err(_)")] {
                    if !covers(
                        &|k| matches!(k, PatKind::Lang { item: i, inner } if *i == item && full(inner)),
                    ) {
                        m.push(text.into());
                    }
                }
                m
            }
            Ty::Adt(d, _) => match self.res().adts.get(d).map(|a| &a.kind) {
                Some(AdtKind::Enum(vs)) => vs
                    .iter()
                    .filter(|v| {
                        !covers(&|k| match k {
                            PatKind::Variant {
                                def, name, fields, ..
                            } => {
                                (def.is_some_and(|d| d == v.def)
                                    || (def.is_none() && *name == v.name))
                                    && fields.iter().all(|f| f.is_irrefutable())
                            }
                            _ => false,
                        })
                    })
                    .map(|v| format!(".{}", v.name))
                    .collect(),
                _ => Vec::new(),
            },
            // §11 rule 6: matching on `Int`, `String` or `Decimal` needs a final `_` arm.
            Ty::Int | Ty::BigInt | Ty::String | Ty::Decimal => vec!["_".into()],
            _ => Vec::new(),
        }
    }

    // -- patterns -------------------------------------------------------------------------

    fn pat(&mut self, p: &Pat, ty: &Ty) {
        let span = p.span;
        match &p.kind {
            PatKind::Wild | PatKind::Error => {}
            PatKind::Bind(l) => {
                self.locals.insert(*l, ty.clone());
            }
            PatKind::Int { .. } | PatKind::Range { .. } => self.coerce(&Ty::Int, ty, span),
            PatKind::Float(_) => self.err(span, TypeErrorKind::FloatPattern),
            PatKind::Str(_) => self.coerce(&Ty::String, ty, span),
            PatKind::Bool(_) => self.coerce(&Ty::Bool, ty, span),
            PatKind::Tuple(ps) => {
                let ts: Vec<Ty> = match self.infer.shallow(ty) {
                    Ty::Tuple(ts) if ts.len() == ps.len() => ts,
                    _ => {
                        let ts: Vec<Ty> = ps.iter().map(|_| self.infer.fresh()).collect();
                        self.coerce(&Ty::Tuple(ts.clone()), ty, span);
                        ts
                    }
                };
                for (p, t) in ps.iter().zip(&ts) {
                    self.pat(p, t);
                }
            }
            PatKind::Lang { item, inner } => {
                let (whole, inner_ty) = match item {
                    LangItem::Some | LangItem::None => {
                        let t = self.infer.fresh();
                        (Ty::option(t.clone()), t)
                    }
                    LangItem::Ok | LangItem::Err => {
                        let (t, e) = (self.infer.fresh(), self.infer.fresh());
                        let inner = if *item == LangItem::Ok {
                            t.clone()
                        } else {
                            e.clone()
                        };
                        (Ty::result(t, e), inner)
                    }
                    _ => (Ty::Error, Ty::Error),
                };
                self.coerce(&whole, ty, span);
                if let Some(p) = inner {
                    self.pat(p, &inner_ty);
                }
            }
            PatKind::Variant {
                def,
                name,
                fields,
                rest,
            } => {
                let st = self.resolve(ty);
                let (adt, targs) = match (&st, def.and_then(|d| self.variant_owner(d))) {
                    (_, Some((a, _))) => {
                        let n = self.res().adts[&a].generics.len();
                        let targs: Vec<Ty> = (0..n).map(|_| self.infer.fresh()).collect();
                        self.coerce(&Ty::Adt(a, targs.clone()), ty, span);
                        (a, targs)
                    }
                    (Ty::Adt(a, targs), None) if self.res().adts.contains_key(a) => {
                        (*a, targs.clone())
                    }
                    (Ty::Error, _) => return,
                    _ => {
                        self.err(
                            span,
                            TypeErrorKind::ImplicitMemberNeedsType { name: name.clone() },
                        );
                        return;
                    }
                };
                let Some((_, v)) = self.res().adts[&adt]
                    .variant(name)
                    .map(|(i, v)| (i, v.clone()))
                else {
                    self.err(
                        span,
                        TypeErrorKind::UnknownMember {
                            ty: st,
                            name: name.clone(),
                        },
                    );
                    return;
                };
                if fields.len() > v.fields.len() || (!rest && fields.len() != v.fields.len()) {
                    self.bad_args(span, format!("`{name}` has {} field(s)", v.fields.len()));
                }
                for (p, f) in fields.iter().zip(&v.fields) {
                    let ft = f.ty.subst(&targs);
                    self.pat(p, &ft);
                }
            }
            PatKind::Or(ps) => {
                for p in ps {
                    self.pat(p, ty);
                }
            }
        }
    }

    // -- statements -----------------------------------------------------------------------

    fn check_stmts(&mut self, block: &Block) {
        for s in &block.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Pass | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(e) => self.expr_stmt(e),
            StmtKind::Let { pat, ty, value } => {
                let t = match ty {
                    Some(t) => {
                        let t = self.lower_local_type(t);
                        self.check(value, &t);
                        t
                    }
                    None => self.synth(value),
                };
                if !pat.is_irrefutable() {
                    self.err(pat.span, TypeErrorKind::RefutableLet);
                }
                self.pat(pat, &t);
            }
            StmtKind::Var { local, ty, value } => {
                let t = match ty {
                    Some(t) => {
                        let t = self.lower_local_type(t);
                        self.check(value, &t);
                        t
                    }
                    None => self.synth(value),
                };
                self.locals.insert(*local, t);
            }
            StmtKind::Assign { place, op, value } => self.assign(place, *op, value, s.span),
            StmtKind::Return(value) => {
                let ret = self.ret.clone();
                match value {
                    Some(v) => {
                        self.check(v, &ret);
                    }
                    None => self.coerce(&Ty::Unit, &ret, s.span),
                }
            }
            StmtKind::For { pat, iter, body } => {
                let it = self.synth(iter);
                let elem = match self.resolve(&it) {
                    Ty::Range(t) | Ty::List(t) | Ty::Set(t) => *t,
                    Ty::Map(k, v) => Ty::Tuple(vec![*k, *v]),
                    Ty::Error => Ty::Error,
                    other => {
                        self.err(iter.span, TypeErrorKind::NotIterable { ty: other });
                        Ty::Error
                    }
                };
                if !pat.is_irrefutable() {
                    self.err(pat.span, TypeErrorKind::RefutableLet);
                }
                self.pat(pat, &elem);
                self.check_stmts(body);
            }
            StmtKind::While { cond, body } => {
                self.check(cond, &Ty::Bool);
                self.check_stmts(body);
            }
        }
    }

    /// Statement-form expression: `if`/`match` arms are statements; any other value that
    /// is not `()` must be used (K0730, K1303 for `Result`).
    fn expr_stmt(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::If {
                branches,
                else_body,
            } => {
                for b in branches {
                    self.check(&b.cond, &Ty::Bool);
                    self.check_stmts(&b.body);
                }
                if let Some(eb) = else_body {
                    self.check_stmts(eb);
                }
                self.record(e, Ty::Unit);
            }
            ExprKind::Match { scrutinee, arms } => {
                let t = self.match_expr(e, scrutinee, arms, None, false);
                self.record(e, t);
            }
            _ => {
                let t = self.synth(e);
                match self.resolve(&t) {
                    Ty::Unit | Ty::Never | Ty::Error | Ty::Var(_) => {}
                    Ty::Result(_, _) => self.err(e.span, TypeErrorKind::UnusedResult),
                    other => self.err(e.span, TypeErrorKind::UnusedValue { ty: other }),
                }
            }
        }
    }

    /// `x = e`, `x += e`, `p.f = e`, `xs[i] = e` (§10.4): the root must be a `var` (K0710).
    fn assign(&mut self, place: &Expr, op: AssignOp, value: &Expr, span: Span) {
        let mut root = place;
        while let ExprKind::Field { base, .. }
        | ExprKind::TupleField { base, .. }
        | ExprKind::Index { base, .. } = &root.kind
        {
            root = base;
        }
        let ok = matches!(&root.kind, ExprKind::Local(l) if self.mutable.get(l.0 as usize).copied().unwrap_or(false));
        if !ok && !matches!(root.kind, ExprKind::Error) {
            self.err(place.span, TypeErrorKind::NotAssignable);
        }
        let pt = self.synth(place);
        let bin = match op {
            AssignOp::Assign => {
                self.check(value, &pt);
                return;
            }
            AssignOp::Add => BinOp::Add,
            AssignOp::Sub => BinOp::Sub,
            AssignOp::Mul => BinOp::Mul,
            AssignOp::Div => BinOp::Div,
        };
        // `x op= e` is `x = x op e` (§10.4), so the result must have `x`'s type.
        let rt = self.operand(value, &pt);
        let (l, r) = (self.resolve(&pt), self.resolve(&rt));
        if l.is_numeric() && r.is_numeric() && l != r {
            self.err(span, TypeErrorKind::MixedNumeric { left: l, right: r });
            return;
        }
        if self.infer.unify(&rt, &pt).is_err() {
            self.err(
                span,
                TypeErrorKind::Mismatch {
                    expected: l,
                    found: r,
                },
            );
            return;
        }
        let result = match (bin, &l) {
            (BinOp::Div, Ty::Int) => Ty::Float,
            (BinOp::Div, Ty::Decimal | Ty::BigInt) => {
                self.err(
                    span,
                    TypeErrorKind::NoOperator {
                        op: "/",
                        ty: l.clone(),
                    },
                );
                return;
            }
            (BinOp::Add, Ty::String | Ty::List(_) | Ty::Sink(SinkType::Sql | SinkType::Html)) => {
                l.clone()
            }
            (_, t) if t.is_numeric() || matches!(t, Ty::Var(_) | Ty::Error) => l.clone(),
            _ => {
                self.err(
                    span,
                    TypeErrorKind::NoOperator {
                        op: bin.symbol(),
                        ty: l.clone(),
                    },
                );
                return;
            }
        };
        self.coerce(&result, &pt, span);
    }

    // -- K1101 ----------------------------------------------------------------------------

    fn block_diverges(&mut self, b: &Block) -> bool {
        b.stmts.iter().any(|s| self.stmt_diverges(s))
    }

    fn stmt_diverges(&mut self, s: &Stmt) -> bool {
        match &s.kind {
            StmtKind::Return(_) => true,
            StmtKind::Expr(e) => match &e.kind {
                ExprKind::If {
                    branches,
                    else_body: Some(eb),
                } => {
                    branches.iter().all(|b| self.block_diverges(&b.body)) && self.block_diverges(eb)
                }
                ExprKind::Match { arms, .. } => {
                    !arms.is_empty() && arms.iter().all(|a| self.block_diverges(&a.body))
                }
                _ => {
                    let t = self
                        .out
                        .exprs
                        .iter()
                        .rev()
                        .find(|(id, _)| *id == e.id)
                        .map(|(_, t)| t.clone());
                    t.is_some_and(|t| matches!(self.infer.shallow(&t), Ty::Never))
                }
            },
            _ => false,
        }
    }
}
