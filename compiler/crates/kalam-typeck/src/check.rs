//! The checker: signature collection, then bidirectional checking of every body with HM
//! unification inside it (spec §9.9).
//!
//! **Pass 1 (collect).** ADTs, aliases, traits, impls and every function signature are
//! lowered to [`Ty`]/[`Scheme`] first, so bodies may refer to any item (mutual recursion
//! needs no ordering). Coherence and the orphan rule are checked here.
//!
//! **Pass 2 (bodies).** Constants first, in dependency order, then every function body,
//! record field default and test. Each body gets its own [`InferCtx`]. `check(e,
//! expected)` pushes the expected type down (literals, empty collections, implicit members,
//! lambdas, `Some`/`Ok`/`Err`, value-form `if`/`match`, the result of generic calls);
//! everything else is synthesized and then coerced (`Never` coerces to anything, §9.1).
//! Trait requirements whose type is still a variable are deferred to the end of the body.
//!
//! Spec clauses implemented here (the ones of the M2 v0.1 surface; see also
//! [`crate::exhaust`] and [`crate::lower`]):
//! - §5.6 literal typing from the expected type and K0126 (64-bit `Int`, D18), and
//!   FOUNDER-QUEUE Q25: in an arithmetic operator an integer literal takes the other
//!   operand's `Float`/`Decimal`/`BigInt` type (`1 + 2.5` is `Float`); comparisons keep
//!   §9.8 (`1 == 1.0` is K0602); variables never convert (K0601, with a `.to_float()` fix).
//! - §9.2–§9.3 primitives and composites: `Int / Int → Float`, no `/` on `Decimal`, string
//!   `+`, no string indexing (K0650), `List`/`Map` indexing, map literals, K0802.
//! - §9.4 records: named construction (K0416), defaults, fields, `.with` update, `export
//!   opaque` privacy outside the defining module (K0418).
//! - §9.5 enums: leading-dot and qualified variants, implicit members against the expected
//!   type (K0803), positional or labelled payloads.
//! - §9.6 generics with bounds; §9.7 traits, coherence and the orphan rule, built-in trait
//!   facts and derivation, `impl T as Trait`; §9.8 `Float`'s IEEE `==`/`<` (DR-23), K0602.
//! - §9.9 inference rules 1–6 (return-type inference from the expected type included). At
//!   the end of a body a type nothing fixed is `Never`, unless a trait requirement depends
//!   on it, which is K0804 (proposed; `json.decode(s)` without an expected type).
//! - §9.4/§13.4 module constants and field defaults are constant expressions (K1204,
//!   `consts.rs`, FOUNDER-QUEUE Q33).
//! - §10.2–§10.8: `let` patterns (K1003), `var`, value-form `if`/`match` (K0731),
//!   assignment through a `var` place (K0710), loops over ranges, lists, sets, maps,
//!   discarded values (K0730).
//! - §11 rules 1–10: exhaustiveness and usefulness (K1001 with the missing cases, K1002),
//!   or-patterns (K1004), literal patterns (K1005), list, record and `as` patterns.
//! - §12.1 named/default arguments, K1101, K1104 (adjacent same-type parameters of an
//!   exported function, with a machine fix inserting the labels); §12.2 DR-27 method
//!   resolution over inherent and trait methods (K0420, K1103; a function-typed field called
//!   with method syntax is K1108, proposed, Q35), `Type.f` associated
//!   functions, `Trait.m(x)`; §12.3 lambdas (K0801), local `fn`s, capture of a `var` (K1110).
//! - §14.2 must-use `Result` (K1303); §14.3 `?` with its four rules (`Same`, `From`, boxing
//!   into `AnyError`, K1301) and the `Option` form (K1302).
//! - §17.1 sinks never accept `String` (K1601, with a machine fix that turns `"…" + x`
//!   into a tagged literal), §17.6 `path"…"` holes are `PathSegment` (K1641).
//!
//! Typed hooks left for the next iteration: suspension (`SuspFlow` records, §15),
//! capability rules (K1501–K1504, §16), the context-dependent hole types of `sql`/`html`/
//! `cmd`/`url` literals (§17.3–§17.6 via `kalam-sinks`), `Secret` rules beyond the trait
//! facts (§18), and `unsafe.*` (typed as poison).

use std::collections::{HashMap, HashSet};

use kalam_base::{FileId, Span};
use kalam_diagnostics::{Applicability, Diagnostic, Edit, Fix};
use kalam_resolve::{DefId, ModulePath};

use crate::adt::{AdtDef, AdtKind, FieldDef, VariantDef, VariantField};
use crate::error::{tag_of, TypeError, TypeErrorKind};
use crate::exhaust;
use crate::hir::{
    self, Arg, Arm, AssignOp, BinOp, Block, BodyValue, BuiltinType, Expr, ExprId, ExprKind, FnDecl,
    GenericParamDecl, HirProgram, IfBranch, IntLit, ItemKind, LangItem, LocalId, Pat, PatKind,
    Stmt, StmtKind, StrPart, TraitBound, TypeKind, UnOp,
};
use crate::traits::{
    erase_vars, Answer, BuiltinTrait, CoherenceError, ImplDecl, MethodCandidate, TraitDecl,
    TraitEnv, TraitId, TraitRef,
};
use crate::ty::{Capability, GenericParam, Scheme, SinkType, Suspension, Ty};
use crate::unify::{InferCtx, SuspFlow, UnifyError};

// Fix-its and recovery helpers (K1001, K1110, K1301, K1302, Q31, well-formed locals).
mod fixes;
use fixes::{Capture, StmtCtx};
// K1108 (a field called like a method) and the checker's K0401 for `.V` (Q34, Q35).
mod members;

/// How `e?` turns the error it propagates into the function's error (§14.3), recorded for
/// lowering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TryConversion {
    /// Rule 1: `E == F`, the error propagates unchanged.
    Same,
    /// Rule 2: `impl F as From[E]`, the error becomes `F.from(e)` (the impl's method).
    From { method: Option<DefId> },
    /// Rule 3: `F == AnyError` and `E: Error`, the error is boxed.
    BoxAnyError,
    /// `Option` in an `Option` function: `None` propagates.
    OptionNone,
}

/// A tagged literal (`sql"…"`, §17) as the checker saw it.
#[derive(Clone, Debug, PartialEq)]
pub struct TaggedLiteral {
    /// The body that holds it.
    pub owner: DefId,
    pub expr: ExprId,
    pub file: FileId,
    /// [kalam-sinks] The whole literal, where structural sink findings point.
    pub span: Span,
    pub tag: kalam_sinks::Tag,
    /// The literal text between the holes, escapes decoded (`holes.len() + 1` parts).
    pub parts: Vec<String>,
    /// Each hole's span and type (zonked at the end of its body).
    pub holes: Vec<(Span, Ty)>,
}

/// Everything the later passes need. Keys `(owner, …)` name the body an expression or
/// local belongs to (the function, constant, record or test that owns it).
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
    /// The method each `MethodCall` (and each `Type.f` associated-function use) resolved
    /// to (DR-27).
    pub method_picks: HashMap<(DefId, ExprId), DefId>,
    /// The variant an implicit member (`.Circle`) or qualified variant resolved to.
    pub variant_picks: HashMap<(DefId, ExprId), DefId>,
    /// The conversion each `?` uses (§14.3).
    pub try_conversions: HashMap<(DefId, ExprId), TryConversion>,
    /// Every tagged literal with its hole types: the typed hook for the sink checks of the
    /// next iteration (`kalam-sinks::check_template` classifies each hole's context and
    /// returns the type it requires, §17.3–§17.6; K1641 is already checked here).
    pub tagged: Vec<TaggedLiteral>,
    /// Suspension constraints `from ≤ to` for `kalam-effects`, by owning body.
    pub flows: Vec<(DefId, SuspFlow)>,
    // [kalam-effects hook, §15] Never written by the checker: `kalam-effects` solves
    // `flows` over the call graph and stores the per-function and per-call bits here.
    pub suspension: crate::susp::Suspensions,
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

    /// The code of every error, for compact assertions.
    pub fn codes(&self) -> Vec<&'static str> {
        self.errors.iter().map(TypeError::code).collect()
    }

    /// Every error as a diagnostic, in source order per file; an error that repeats another
    /// one (same code, same place) is dropped.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics_in(&HashMap::new())
    }

    /// [`TypeckResults::diagnostics`] with every type in a message spelled in the skin of
    /// its file (`skins`; English for a file not in the map), spec §21.4 rule 1.
    pub fn diagnostics_in(&self, skins: &HashMap<FileId, kalam_base::Skin>) -> Vec<Diagnostic> {
        let mut errors: Vec<&TypeError> = self.errors.iter().collect();
        errors.sort_by_key(|e| (e.file, e.span.start, e.span.end));
        let mut seen = HashSet::new();
        errors
            .into_iter()
            .filter(|e| seen.insert((e.file, e.span, e.code())))
            .map(|e| {
                let skin = skins.get(&e.file).copied().unwrap_or(kalam_base::Skin::En);
                let sp = kalam_diagnostics::code::TypeSpelling::new(skin);
                e.to_diagnostic_in(&|d| self.name_of(d), sp)
            })
            .collect()
    }
}

/// Type-checks a whole program.
pub fn check_program(prog: &HirProgram) -> TypeckResults {
    crate::on_big_stack(|| check_program_here(prog))
}

fn check_program_here(prog: &HirProgram) -> TypeckResults {
    let mut cx = Collector {
        prog,
        res: TypeckResults::default(),
        trait_ids: HashMap::new(),
        method_impl: HashMap::new(),
        written: HashMap::new(),
        contract_self: HashMap::new(),
    };
    cx.collect();
    cx.check_bodies();
    // [kalam-sinks] §17.3–§17.6: template structure and hole contexts of tagged literals.
    crate::sink_holes::check(&mut cx.res);
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
    prog: &'p HirProgram,
    res: TypeckResults,
    /// Trait items → their trait (a built-in trait for the stdlib's declarations of one).
    trait_ids: HashMap<DefId, TraitId>,
    /// Method → index of its impl in `res.traits.impls`.
    method_impl: HashMap<DefId, usize>,
    /// Impl methods that do not match their trait (K1107): their written signature, used
    /// to check their own body, while `res.signatures` holds the trait's (what callers
    /// see), so the mismatch is reported once, at the impl.
    written: HashMap<DefId, Scheme>,
    /// For those methods, whether the trait's method takes `self` (callers call it the
    /// trait's way, FOUNDER-QUEUE Q37).
    contract_self: HashMap<DefId, bool>,
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
        self.res.errors.push(TypeError::new(kind, file, span));
    }

    fn collect(&mut self) {
        let ids = self.prog.item_ids();
        for &id in &ids {
            let Some(item) = self.prog.item(id) else {
                continue;
            };
            self.res.names.insert(id, item.name.clone());
            match &item.kind {
                ItemKind::Enum(e) => {
                    for v in &e.variants {
                        self.res.names.insert(v.def, v.name.clone());
                    }
                }
                ItemKind::Trait(t) => {
                    let tid = t.builtin.map_or(TraitId::User(id), TraitId::Builtin);
                    self.trait_ids.insert(id, tid);
                }
                _ => {}
            }
        }
        // ADT skeletons first, so field types can name any ADT.
        for &id in &ids {
            let Some(item) = self.prog.item(id) else {
                continue;
            };
            let (kind, derives, opaque, gdecls) = match &item.kind {
                ItemKind::Record(r) => (
                    AdtKind::Record(Vec::new()),
                    r.derives.clone(),
                    r.opaque,
                    &r.generics,
                ),
                ItemKind::Enum(e) => (
                    AdtKind::Enum(Vec::new()),
                    e.derives.clone(),
                    e.opaque,
                    &e.generics,
                ),
                ItemKind::Extern(x) => (AdtKind::Extern, Vec::new(), false, &x.generics),
                _ => continue,
            };
            // The generic parameters' count is known now, so a field written in any
            // module can be arity-checked; their bounds come with the fields below.
            let generics = gdecls
                .iter()
                .map(|g| GenericParam {
                    name: g.name.clone(),
                    bounds: Vec::new(),
                })
                .collect();
            self.res.adts.insert(
                id,
                AdtDef {
                    def: id,
                    name: item.name.clone(),
                    module: item.module.clone(),
                    generics,
                    kind,
                    opaque,
                    derives,
                    assumed: Vec::new(),
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
        // Annotated constants are known before any body is checked.
        for &id in &ids {
            if let Some(hir::Item {
                kind: ItemKind::Const(c),
                file,
                ..
            }) = self.prog.item(id)
            {
                if let Some(t) = &c.ty {
                    let ty = self.data_ty(*file, t, None);
                    self.res.consts.insert(id, ty);
                }
            }
        }
        for err in self.res.traits.check_coherence(&self.res.adts) {
            let (def, kind) = match err {
                CoherenceError::Orphan { impl_def } => (impl_def, TypeErrorKind::OrphanImpl),
                CoherenceError::Overlap { first, second } => {
                    // The later impl is rejected: the first one alone answers (no K1103
                    // on every call that both would match).
                    for imp in self.res.traits.impls.iter_mut() {
                        if imp.def == second {
                            imp.rejected = true;
                        }
                    }
                    (second, TypeErrorKind::OverlappingImpl { other: first })
                }
            };
            if let Some(item) = self.prog.item(def) {
                let (file, span) = (item.file, item.span);
                self.err(file, span, kind);
            }
        }
        // Well-formed declarations, the entry point and derives (spec §9.3, §9.6, §9.7,
        // §14.3), then every impl against its trait (K1107).
        crate::decls::check(self.prog, &mut self.res);
        let outcome = crate::conform::check(self.prog, &self.res);
        self.res.errors.extend(outcome.errors);
        for (m, contract) in outcome.contracts {
            if let Some(own) = self.res.signatures.insert(m, contract) {
                self.written.insert(m, own);
            }
        }
        self.contract_self.extend(outcome.selfs);
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
            ItemKind::Extern(x) => (self.generics(file, &x.generics), AdtKind::Extern),
            _ => return,
        };
        let Some(adt) = self.res.adts.get_mut(&id) else {
            return;
        };
        adt.generics = generics;
        adt.kind = kind;
        // K0612: `@derive(Encode)` on a type with a `Secret` field (§9.7, §18.1).
        let encodes_secret = self.res.adts.get(&id).is_some_and(|adt| {
            adt.derives.contains(&BuiltinTrait::Encode)
                && adt.component_types(&[]).iter().any(|t| t.contains_secret())
        });
        if encodes_secret {
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
                let tid = self
                    .trait_ids
                    .get(&id)
                    .copied()
                    .unwrap_or(TraitId::User(id));
                // A built-in trait is defined by the core, wherever the stdlib declares it.
                let module = match tid {
                    TraitId::Builtin(_) => None,
                    TraitId::User(_) => Some(item.module.clone()),
                };
                self.res.traits.traits.insert(
                    tid,
                    TraitDecl {
                        id: tid,
                        module,
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
                let index = self.res.traits.impls.len();
                for (_, m) in &methods {
                    self.method_impl.insert(*m, index);
                }
                self.res.traits.impls.push(ImplDecl {
                    def: id,
                    module: item.module.clone(),
                    generics,
                    self_ty,
                    trait_ref,
                    methods,
                    span: kalam_base::FileSpan::new(file, item.span),
                    privileged: is_std(&item.module),
                    rejected: false,
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
        let outer = generics.len() as u32;
        generics.extend(self.generics(file, &f.generics));
        if let Some(parent) = f.parent {
            if let Some(ItemKind::Trait(t)) = self.prog.item(parent).map(|i| &i.kind) {
                let tid = self
                    .trait_ids
                    .get(&parent)
                    .copied()
                    .unwrap_or(TraitId::User(parent));
                let args = (0..t.generics.len().min(outer as usize) as u32)
                    .map(Ty::Param)
                    .collect();
                self_ty = Some(Ty::Param(generics.len() as u32));
                generics.push(GenericParam {
                    name: "Self".into(),
                    bounds: vec![TraitRef { id: tid, args }],
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
        let mut const_eval = crate::consts::ConstEval::new(self.prog);
        // Constants first, in dependency order (§13.1: "evaluated once, in dependency
        // order"); a cycle keeps item order.
        for id in self.const_order(&ids) {
            let Some(item) = self.prog.item(id) else {
                continue;
            };
            let ItemKind::Const(c) = &item.kind else {
                continue;
            };
            let file = item.file;
            let module = item.module.clone();
            let declared = self.res.consts.get(&id).cloned();
            let mut b = BodyCx::new(self, id, file, module, Vec::new(), Ty::Unit, &[]);
            let ty = match &declared {
                Some(t) => {
                    b.check(&c.value, t);
                    t.clone()
                }
                None => b.synth(&c.value),
            };
            // A part of an unannotated constant's type that its value leaves open is K0804;
            // then the body is settled, so the type read below is closed.
            if declared.is_none() {
                b.close_constant_type(&ty, c.value.span);
            }
            b.settle();
            let ty = b.infer.resolve(&ty);
            let out = b.finish();
            let clean = out.errors.is_empty();
            self.absorb(id, out);
            let ty = ty.map(&mut |t| matches!(t, Ty::Var(_)).then_some(Ty::Error));
            self.res.consts.insert(id, ty);
            // §13.4 (Q33): a constant expression, no panic, no capability or `Cell` (K1204).
            if let Some(e) = const_eval.constant(self.prog, &self.res, id, clean) {
                self.res.errors.push(e);
            }
        }
        for &id in &ids {
            let Some(item) = self.prog.item(id) else {
                continue;
            };
            match &item.kind {
                ItemKind::Fn(f) => self.check_fn(id, item, f),
                ItemKind::Record(r) => {
                    let file = item.file;
                    let module = item.module.clone();
                    let generics = self.generics(file, &r.generics);
                    let Some(AdtKind::Record(fields)) =
                        self.res.adts.get(&id).map(|a| a.kind.clone())
                    else {
                        continue;
                    };
                    if r.fields.iter().all(|f| f.default.is_none()) {
                        continue;
                    }
                    let mut b = BodyCx::new(self, id, file, module, generics, Ty::Unit, &[]);
                    for (decl, field) in r.fields.iter().zip(&fields) {
                        if let Some(d) = &decl.default {
                            b.check(d, &field.ty);
                        }
                    }
                    let out = b.finish();
                    let clean = out.errors.is_empty();
                    self.absorb(id, out);
                    // §9.4: field defaults are constant expressions (§13.4, K1204).
                    for d in r.fields.iter().filter_map(|f| f.default.as_ref()) {
                        let found =
                            const_eval.field_default(self.prog, &self.res, id, file, d, clean);
                        if let Some(e) = found {
                            self.res.errors.push(e);
                        }
                    }
                }
                ItemKind::Test(body) => {
                    let file = item.file;
                    let module = item.module.clone();
                    let mut b =
                        BodyCx::new(self, id, file, module, Vec::new(), Ty::Unit, &body.locals);
                    match &body.value {
                        BodyValue::Block(block) => b.check_stmts(block),
                        BodyValue::Expr(e) => {
                            b.check(e, &Ty::Unit);
                        }
                    }
                    let out = b.finish();
                    self.absorb(id, out);
                }
                _ => {}
            }
        }
    }

    fn check_fn(&mut self, id: DefId, item: &hir::Item, f: &FnDecl) {
        let Some(body) = &f.body else { return };
        let file = item.file;
        // A method that does not match its trait is checked against what it declares.
        let Some(scheme) = self
            .written
            .get(&id)
            .or_else(|| self.res.signatures.get(&id))
            .cloned()
        else {
            return;
        };
        let Ty::Fn { params, ret, .. } = &scheme.ty else {
            return;
        };
        let mut b = BodyCx::new(
            self,
            id,
            file,
            item.module.clone(),
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
        b.fn_body(&body.value, ret);
        let out = b.finish();
        self.absorb(id, out);
    }

    /// Constants ordered so each comes after the constants its initializer names.
    fn const_order(&self, ids: &[DefId]) -> Vec<DefId> {
        let consts: Vec<DefId> = ids
            .iter()
            .copied()
            .filter(|d| {
                matches!(
                    self.prog.item(*d).map(|i| &i.kind),
                    Some(ItemKind::Const(_))
                )
            })
            .collect();
        let set: HashSet<DefId> = consts.iter().copied().collect();
        let mut deps: HashMap<DefId, Vec<DefId>> = HashMap::new();
        for &c in &consts {
            if let Some(ItemKind::Const(decl)) = self.prog.item(c).map(|i| &i.kind) {
                let mut found = Vec::new();
                walk_expr(&decl.value, &mut |e| {
                    if let ExprKind::Def(d) = e.kind {
                        if set.contains(&d) {
                            found.push(d);
                        }
                    }
                });
                deps.insert(c, found);
            }
        }
        let mut out = Vec::new();
        let mut state: HashMap<DefId, u8> = HashMap::new();
        fn visit(
            c: DefId,
            deps: &HashMap<DefId, Vec<DefId>>,
            state: &mut HashMap<DefId, u8>,
            out: &mut Vec<DefId>,
        ) {
            if state.get(&c).is_some() {
                return;
            }
            state.insert(c, 1);
            for &d in deps.get(&c).map(|v| v.as_slice()).unwrap_or(&[]) {
                visit(d, deps, state, out);
            }
            state.insert(c, 2);
            out.push(c);
        }
        for &c in &consts {
            visit(c, &deps, &mut state, &mut out);
        }
        out
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
        for (e, c) in out.tries {
            self.res.try_conversions.insert((owner, e), c);
        }
        for mut t in out.tagged {
            t.owner = owner;
            self.res.tagged.push(t);
        }
        self.res
            .flows
            .extend(out.flows.into_iter().map(|f| (owner, f)));
        self.res.errors.extend(out.errors);
    }
}

/// The stdlib package (`std/…` modules) is privileged (stdlib/README.md).
fn is_std(m: &ModulePath) -> bool {
    m.0.starts_with("std/")
}

/// Calls `f` on `e` and every expression inside it (not inside nested blocks' statements
/// other than expressions they contain).
///
/// Iterative, so a deeply nested expression cannot overflow the stack (typeck is a fuzz
/// target).
fn walk_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    let mut stack: Vec<&Expr> = vec![e];
    while let Some(e) = stack.pop() {
        f(e);
        match &e.kind {
            ExprKind::Interp(parts) => {
                for p in parts {
                    if let StrPart::Hole(h) = p {
                        stack.push(h);
                    }
                }
            }
            ExprKind::Tagged { holes, .. } => stack.extend(holes.iter()),
            ExprKind::Call { callee, args } => {
                stack.push(callee);
                stack.extend(args.iter().map(|a| &a.value));
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                stack.push(receiver);
                stack.extend(args.iter().map(|a| &a.value));
            }
            ExprKind::Field { base, .. } | ExprKind::TupleField { base, .. } => stack.push(base),
            ExprKind::Index { base, index } => {
                stack.push(base);
                stack.push(index);
            }
            ExprKind::Unary { operand, .. } => stack.push(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                stack.push(lhs);
                stack.push(rhs);
            }
            ExprKind::Range { lo, hi, .. } => {
                stack.push(lo);
                stack.push(hi);
            }
            ExprKind::Tuple(xs) | ExprKind::List(xs) => stack.extend(xs.iter()),
            ExprKind::Map(pairs) => {
                for (k, v) in pairs {
                    stack.push(k);
                    stack.push(v);
                }
            }
            ExprKind::Try(x) => stack.push(x),
            ExprKind::Lambda { body, .. } => stack.push(body),
            ExprKind::If { branches, .. } => stack.extend(branches.iter().map(|b| &b.cond)),
            ExprKind::Match { scrutinee, .. } => stack.push(scrutinee),
            _ => {}
        }
    }
}

/// Lowers written type syntax. Aliases are expanded here (`type UserId = Int` is the same
/// type as `Int`, §9.4).
fn lower_type(
    prog: &HirProgram,
    adts: &HashMap<DefId, AdtDef>,
    errors: &mut Vec<TypeError>,
    file: FileId,
    t: &hir::Type,
    l: &mut Lower,
) -> Ty {
    lower_type_depth(prog, adts, errors, file, t, l, 0)
}

fn lower_type_depth(
    prog: &HirProgram,
    adts: &HashMap<DefId, AdtDef>,
    errors: &mut Vec<TypeError>,
    file: FileId,
    t: &hir::Type,
    l: &mut Lower,
    depth: u32,
) -> Ty {
    // An alias chain deeper than this is a cycle resolve did not catch: poison it.
    if depth > 64 {
        return Ty::Error;
    }
    let top = l.pos;
    let mut nested = |t: &hir::Type, l: &mut Lower| {
        let saved = l.pos;
        l.pos = Pos::Data;
        let ty = lower_type_depth(prog, adts, errors, file, t, l, depth + 1);
        l.pos = saved;
        ty
    };
    let arity_error = |errors: &mut Vec<TypeError>, want: usize, got: usize| {
        errors.push(TypeError::new(
            TypeErrorKind::BadArguments {
                detail: format!("expected {want} type argument(s), found {got}"),
            },
            file,
            t.span,
        ));
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
            let want = builtin_arity(*b);
            if args.len() != want {
                return arity_error(errors, want, args.len());
            }
            builtin_ty(*b, args)
        }
        TypeKind::Def(d, args) => {
            let args: Vec<Ty> = args.iter().map(|t| nested(t, l)).collect();
            if let Some(adt) = adts.get(d) {
                if adt.generics.len() != args.len() {
                    return arity_error(errors, adt.generics.len(), args.len());
                }
                return Ty::Adt(*d, args);
            }
            match prog.item(*d).map(|i| &i.kind) {
                Some(ItemKind::Alias(a)) => {
                    if a.generics.len() != args.len() {
                        return arity_error(errors, a.generics.len(), args.len());
                    }
                    let target = nested(&a.target, l);
                    target.subst(&args)
                }
                _ => Ty::Error,
            }
        }
    }
}

fn builtin_arity(b: BuiltinType) -> usize {
    match b {
        BuiltinType::List
        | BuiltinType::Set
        | BuiltinType::Option
        | BuiltinType::Range
        | BuiltinType::Secret => 1,
        BuiltinType::Map | BuiltinType::Result => 2,
        _ => 0,
    }
}

/// The type a built-in constructor builds from `args` (which must have its arity).
fn builtin_ty(b: BuiltinType, args: Vec<Ty>) -> Ty {
    let a = |i: usize| Box::new(args.get(i).cloned().unwrap_or(Ty::Error));
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
        BuiltinType::Capability(c) => Ty::Capability(c),
        BuiltinType::Sink(s) => Ty::Sink(s),
        BuiltinType::List => Ty::List(a(0)),
        BuiltinType::Set => Ty::Set(a(0)),
        BuiltinType::Option => Ty::Option(a(0)),
        BuiltinType::Range => Ty::Range(a(0)),
        BuiltinType::Secret => Ty::Secret(a(0)),
        BuiltinType::Map => Ty::Map(a(0), a(1)),
        BuiltinType::Result => Ty::Result(a(0), a(1)),
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
    tries: Vec<(ExprId, TryConversion)>,
    tagged: Vec<TaggedLiteral>,
    flows: Vec<SuspFlow>,
    errors: Vec<TypeError>,
}

/// Why a trait requirement exists (decides the error when it fails).
#[derive(Clone)]
enum Origin {
    Show,
    Eq {
        left: Ty,
        right: Ty,
    },
    Ord {
        op: &'static str,
    },
    Bound,
    /// The key of a map literal (§9.3: `K: Eq + Hash`).
    Key,
}

struct Obligation {
    ty: Ty,
    tr: TraitRef,
    span: Span,
    origin: Origin,
}

/// A piece of a `"…" + x + "…"` chain (K1601's fix).
enum Piece {
    /// A plain one-line string literal: its text between the quotes.
    Lit(String),
    /// Any other operand: its source text stays where it is.
    Hole(Span),
}

struct BodyCx<'c, 'p> {
    cx: &'c Collector<'p>,
    owner: DefId,
    file: FileId,
    module: ModulePath,
    generics: Vec<GenericParam>,
    /// Return types of the function, local `fn` or lambda being checked (innermost last).
    rets: Vec<Ty>,
    infer: InferCtx,
    locals: HashMap<LocalId, Ty>,
    decls: Vec<hir::LocalDecl>,
    /// Closures being checked, innermost last (0 is the item's own body).
    closures: Vec<u32>,
    /// Initializers of `let x = …` with a plain binding (K1601's fix).
    let_inits: HashMap<LocalId, Expr>,
    out: BodyOut,
    obligations: Vec<Obligation>,
    /// Bound and well-formedness failures already reported in this body: the same type
    /// missing the same trait is one root cause (`Set.of([1.5])` then `s.len()`).
    reported: HashSet<(Ty, TraitRef)>,
    /// The statements being checked, innermost last (K1110's fix inserts before one).
    stmt_stack: Vec<StmtCtx>,
    /// `var`s used inside closures, reported at the end of the body (K1110).
    captures: Vec<Capture>,
}

impl<'c, 'p> BodyCx<'c, 'p> {
    fn new(
        cx: &'c Collector<'p>,
        owner: DefId,
        file: FileId,
        module: ModulePath,
        generics: Vec<GenericParam>,
        ret: Ty,
        locals: &[hir::LocalDecl],
    ) -> BodyCx<'c, 'p> {
        BodyCx {
            cx,
            owner,
            file,
            module,
            generics,
            rets: vec![ret],
            infer: InferCtx::new(),
            locals: HashMap::new(),
            decls: locals.to_vec(),
            closures: vec![0],
            let_inits: HashMap::new(),
            out: BodyOut::default(),
            obligations: Vec::new(),
            reported: HashSet::new(),
            stmt_stack: Vec::new(),
            captures: Vec::new(),
        }
    }

    fn res(&self) -> &TypeckResults {
        &self.cx.res
    }

    fn prog(&self) -> &'p HirProgram {
        self.cx.prog
    }

    fn err(&mut self, span: Span, kind: TypeErrorKind) {
        self.push_error(TypeError::new(kind, self.file, span));
    }

    fn err_fix(&mut self, span: Span, kind: TypeErrorKind, fixes: Vec<Fix>) {
        let mut e = TypeError::new(kind, self.file, span);
        e.fixes = fixes;
        self.push_error(e);
    }

    fn record(&mut self, e: &Expr, ty: Ty) -> Ty {
        self.out.exprs.push((e.id, ty.clone()));
        ty
    }

    fn resolve(&mut self, t: &Ty) -> Ty {
        self.infer.resolve(t)
    }

    fn ret(&self) -> Ty {
        self.rets.last().cloned().unwrap_or(Ty::Unit)
    }

    /// Settles the body: the K1110 captures, then the deferred trait requirements, then the
    /// type variables nothing constrained (spec §9.9; the verifier's open item 1).
    ///
    /// - A requirement that fails is the trait's error. One that still waits on a variable
    ///   fails or holds whatever that type is, or it depends on it: then guessing would
    ///   change what the program does (`json.decode(s)` without an expected type, `print`
    ///   of `None`), so it is K0804, once per open type. Its variables become poison.
    /// - Every other variable becomes `Never`. Nothing constrains it, so no value of its
    ///   type is ever made or used (`E` in `Ok(1).is_ok()`, `A` in an unused
    ///   `http.router()`), and `Never` keeps every type closed for KIR. In a body that
    ///   already has an error they become poison instead, as before.
    ///
    /// Idempotent: the constant path settles before reading the constant's type.
    fn settle(&mut self) {
        self.capture_errors();
        let obligations = std::mem::take(&mut self.obligations);
        let clean = self.out.errors.is_empty();
        for o in obligations {
            let ty = self.resolve(&o.ty);
            let tr = TraitRef {
                id: o.tr.id,
                args: o.tr.args.iter().map(|t| self.infer.resolve(t)).collect(),
            };
            match self.answer(&ty, &tr) {
                Answer::Yes => {}
                Answer::No => self.trait_error(&ty, &tr, o.span, o.origin),
                Answer::Unknown => {
                    // `Error` answers `Yes`, so `No` here holds for every open part.
                    let erased = TraitRef {
                        id: tr.id,
                        args: tr.args.iter().map(erase_vars).collect(),
                    };
                    if self.answer(&erase_vars(&ty), &erased) == Answer::No {
                        self.trait_error(&ty, &tr, o.span, o.origin);
                    } else if clean {
                        let trait_name = match tr.id {
                            TraitId::Builtin(b) => b.name().to_string(),
                            TraitId::User(d) => self.res().name_of(d),
                        };
                        let kind = TypeErrorKind::UninferredType { ty: ty.clone() };
                        let label = format!("`{trait_name}` for it depends on what `_` is");
                        let e = TypeError::new(kind, self.file, o.span).with_label(o.span, label);
                        self.push_error(e);
                    }
                    // Reported (or a root cause is): later requirements on it stay quiet.
                    let mut open = Vec::new();
                    for t in std::iter::once(&ty).chain(tr.args.iter()) {
                        t.any(&mut |t| {
                            if let Ty::Var(v) = t {
                                open.push(*v);
                            }
                            false
                        });
                    }
                    for v in open {
                        self.infer.default_to(v, Ty::Error);
                    }
                }
            }
        }
        let default = if self.out.errors.is_empty() {
            Ty::Never
        } else {
            Ty::Error
        };
        for v in 0..self.infer.var_count() as u32 {
            self.infer.default_to(v, default.clone());
        }
    }

    /// An unannotated module constant's type must be closed by its value alone (§9.9 rule
    /// 1): other bodies use the constant, so a part nothing fixed would surface there as
    /// `Never` (`let NONE = None` used as an `Option[Int]`). K0804 at the value, when the
    /// body has no other error; the open parts become poison. Called before [`Self::settle`].
    fn close_constant_type(&mut self, ty: &Ty, span: Span) {
        let ty = self.resolve(ty);
        let mut open = Vec::new();
        ty.any(&mut |t| {
            if let Ty::Var(v) = t {
                open.push(*v);
            }
            false
        });
        if open.is_empty() {
            return;
        }
        if self.out.errors.is_empty() {
            let label = "a module constant takes its type from its value alone";
            let e = TypeError::new(TypeErrorKind::UninferredType { ty }, self.file, span)
                .with_label(span, label);
            self.push_error(e);
        }
        for v in open {
            self.infer.default_to(v, Ty::Error);
        }
    }

    /// Settles the body (see [`Self::settle`]) and zonks every recorded type.
    fn finish(mut self) -> BodyOut {
        self.settle();
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
        for t in out.tagged.iter_mut() {
            for (_, ty) in t.holes.iter_mut() {
                *ty = zonk(ty, &mut self.infer);
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
        // A bound or key requirement that already failed for this type is one root cause.
        if matches!(origin, Origin::Bound | Origin::Key)
            && !self.reported.insert((ty.clone(), tr.clone()))
        {
            return;
        }
        let key = matches!(origin, Origin::Key);
        let kind = match origin {
            Origin::Eq { left, right } => TypeErrorKind::BadEquality { left, right },
            Origin::Ord { op } => TypeErrorKind::NoOperator { op, ty: ty.clone() },
            Origin::Show | Origin::Bound | Origin::Key => TypeErrorKind::MissingTrait {
                ty: ty.clone(),
                trait_name,
            },
        };
        let mut e = TypeError::new(kind, self.file, span);
        if key {
            e = e.with_label(span, "a `Map` key must be `Eq` and `Hash`");
        }
        self.push_error(e);
    }

    /// Requires `ty: tr` now, or at the end of the body if `ty` is still unknown.
    fn require(&mut self, ty: &Ty, tr: TraitRef, span: Span, origin: Origin) {
        let ty = self.resolve(ty);
        let tr = TraitRef {
            id: tr.id,
            args: tr.args.iter().map(|t| self.infer.resolve(t)).collect(),
        };
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
        self.coerce_at(None, found, expected, span);
    }

    /// Like [`Self::coerce`], with the expression for the fixes of K0601 and K1601.
    fn coerce_expr(&mut self, e: &Expr, found: &Ty, expected: &Ty) {
        self.coerce_at(Some(e), found, expected, e.span);
    }

    fn coerce_at(&mut self, e: Option<&Expr>, found: &Ty, expected: &Ty, span: Span) {
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
                // A number of one type where another is expected is a mismatch (K0600, with
                // the conversion as its fix), not a mix of two operands (K0601); an
                // `Int / Int` division where an `Int` is expected suggests `//` (Q31).
                if found.is_numeric() && expected.is_numeric() {
                    let mut fixes: Vec<Fix> = e
                        .and_then(|e| numeric_fix(e, &found, &expected))
                        .into_iter()
                        .collect();
                    if let Some(e) = e {
                        fixes.extend(self.int_division_fix(e, &found, &expected));
                    }
                    self.err_fix(span, TypeErrorKind::Mismatch { expected, found }, fixes);
                    return;
                }
                let mut fixes = Vec::new();
                if let (Ty::Sink(s), Ty::String, Some(e)) = (&expected, &found, e) {
                    fixes.extend(self.sink_fix(e, *s));
                }
                self.err_fix(span, TypeErrorKind::Mismatch { expected, found }, fixes);
            }
        }
    }

    /// A keyword as this file spells it (for fixes that insert one).
    fn keyword(&self, c: kalam_skins::Concept) -> Option<String> {
        let skin = self.prog().skins.get(&self.file).copied()?;
        Some(
            kalam_skins::KeywordTable::builtin()
                .spelling(c, skin)
                .to_string(),
        )
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

    /// Lowers the type of `Type.f`: a generic type written without arguments gets fresh
    /// variables for them.
    fn head_type(&mut self, t: &hir::Type) -> Ty {
        match &t.kind {
            TypeKind::Builtin(b, args) if args.is_empty() && builtin_arity(*b) > 0 => {
                let fresh: Vec<Ty> = (0..builtin_arity(*b)).map(|_| self.infer.fresh()).collect();
                builtin_ty(*b, fresh)
            }
            TypeKind::Def(d, args) if args.is_empty() => {
                match self.res().adts.get(d).map(|a| a.generics.len()) {
                    Some(n) if n > 0 => {
                        let fresh = (0..n).map(|_| self.infer.fresh()).collect();
                        Ty::Adt(*d, fresh)
                    }
                    _ => self.lower_local_type(t),
                }
            }
            _ => self.lower_local_type(t),
        }
    }

    /// The members of `ty` are fully known, so a missing one is K0420 (built-in types need
    /// the stdlib stubs).
    fn members_known(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Adt(d, _) => self.res().adts.contains_key(d) || self.prog().api_complete,
            Ty::Error | Ty::Var(_) | Ty::Never => false,
            _ => self.prog().api_complete,
        }
    }

    fn unknown_member(&mut self, span: Span, ty: Ty, name: &str) {
        if self.members_known(&ty) {
            self.err(
                span,
                TypeErrorKind::UnknownMember {
                    ty,
                    name: name.into(),
                },
            );
        }
    }

    /// K0418 when an `export opaque` type's internals are used outside its module.
    fn check_private(&mut self, adt: DefId, span: Span, name: &str) {
        let private = self
            .res()
            .adts
            .get(&adt)
            .filter(|a| a.opaque && a.module != self.module)
            .map(|a| a.module.0.clone());
        if let Some(module) = private {
            self.err(
                span,
                TypeErrorKind::PrivateMember {
                    name: name.into(),
                    module,
                },
            );
        }
    }

    // -- bodies -------------------------------------------------------------------------

    fn fn_body(&mut self, value: &BodyValue, ret: &Ty) {
        match value {
            BodyValue::Expr(e) => {
                self.check(e, ret);
            }
            BodyValue::Block(block) => {
                let resolved = self.resolve(ret);
                let needs_return = !matches!(resolved, Ty::Unit | Ty::Error);
                // §12.1 (DR-08): a final expression is a missing `return`, not an unused
                // value; the fix inserts `return` before it when its type fits.
                let tail = match block.stmts.split_last() {
                    Some((last, init)) if needs_return => match &last.kind {
                        StmtKind::Expr(e)
                            if !matches!(e.kind, ExprKind::If { .. } | ExprKind::Match { .. }) =>
                        {
                            Some((e, init))
                        }
                        _ => None,
                    },
                    _ => None,
                };
                let Some((e, init)) = tail else {
                    self.check_stmts(block);
                    if needs_return && !self.block_diverges(block) {
                        self.err(block.span, TypeErrorKind::MissingReturn);
                    }
                    return;
                };
                for s in init {
                    self.stmt(s);
                }
                let t = self.synth(e);
                let diverges = init.iter().any(|s| self.stmt_diverges(s))
                    || matches!(self.infer.shallow(&t), Ty::Never);
                if !diverges {
                    let fits = self.infer.try_unify(&t, ret);
                    let fixes = if fits {
                        self.keyword(kalam_skins::Concept::KwReturn)
                            .map(|kw| {
                                Fix::machine(
                                    "return the final value",
                                    Span::new(e.span.start, e.span.start),
                                    format!("{kw} "),
                                )
                            })
                            .into_iter()
                            .collect()
                    } else {
                        Vec::new()
                    };
                    self.err_fix(e.span, TypeErrorKind::MissingReturn, fixes);
                }
            }
        }
    }

    // -- expressions --------------------------------------------------------------------

    /// Checks `e` against `expected` and returns the type recorded for `e`.
    fn check(&mut self, e: &Expr, expected: &Ty) -> Ty {
        kalam_ast::stack::deeper(|| self.check_inner(e, expected))
    }

    fn check_inner(&mut self, e: &Expr, expected: &Ty) -> Ty {
        let exp = self.infer.shallow(expected);
        match &e.kind {
            ExprKind::Int(v) => {
                let t = self.int_literal(v, false, &exp, e.span);
                self.coerce_expr(e, &t, &exp);
                self.record(e, t)
            }
            ExprKind::Float(text) => {
                let t = if exp == Ty::Decimal {
                    Ty::Decimal
                } else {
                    Ty::Float
                };
                // Against another type the mismatch (K0600) is the one report.
                if matches!(exp, Ty::Float | Ty::Decimal | Ty::Var(_) | Ty::Error) {
                    self.number_literal_range(text, &t, e.span);
                }
                self.coerce_expr(e, &t, &exp);
                self.record(e, t)
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } if matches!(operand.kind, ExprKind::Int(_) | ExprKind::Float(_)) => {
                let t = self.literal_operand(e, &exp);
                self.coerce_expr(e, &t, &exp);
                t
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
                    self.coerce_expr(e, &t, &exp);
                    t
                }
            },
            ExprKind::Map(pairs) => match &exp {
                Ty::Map(k, v) => {
                    let (k, v) = ((**k).clone(), (**v).clone());
                    for (ke, ve) in pairs {
                        self.check(ke, &k);
                        self.check(ve, &v);
                    }
                    self.record(e, exp.clone())
                }
                _ => {
                    let t = self.synth(e);
                    self.coerce_expr(e, &t, &exp);
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
                    self.coerce_expr(e, &t, &exp);
                    t
                }
            },
            ExprKind::ImplicitMember(name) => {
                let t = self.variant_value(e, None, name, &[], Some(&exp));
                self.coerce_expr(e, &t, &exp);
                self.record(e, t)
            }
            ExprKind::Call { callee, args } => {
                let t = self.call(e, callee, args, Some(&exp));
                self.coerce_expr(e, &t, &exp);
                self.record(e, t)
            }
            ExprKind::MethodCall {
                receiver,
                name,
                args,
            } => {
                let t = self.method_call(e, receiver, name, args, Some(&exp));
                self.coerce_expr(e, &t, &exp);
                self.record(e, t)
            }
            ExprKind::Try(inner) => {
                let t = self.try_expr(e, inner, Some(&exp));
                self.coerce_expr(e, &t, &exp);
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
            ExprKind::Lambda {
                params,
                ret,
                body,
                closure,
            } => {
                let t = self.lambda(e, params, ret.as_ref(), body, *closure, Some(&exp));
                self.coerce_expr(e, &t, &exp);
                self.record(e, t)
            }
            ExprKind::Binary { op, lhs, rhs } if op.is_arithmetic() => {
                let t = self.binary(e, *op, lhs, rhs, Some(&exp));
                let t = self.record(e, t);
                self.coerce_expr(e, &t, &exp);
                t
            }
            _ => {
                let t = self.synth(e);
                self.coerce_expr(e, &t, &exp);
                t
            }
        }
    }

    /// Infers the type of `e` with no expectation.
    fn synth(&mut self, e: &Expr) -> Ty {
        kalam_ast::stack::deeper(|| self.synth_inner(e))
    }

    fn synth_inner(&mut self, e: &Expr) -> Ty {
        let t = match &e.kind {
            ExprKind::Error | ExprKind::Unsafe(_) | ExprKind::TypeName(_) => Ty::Error,
            ExprKind::Int(v) => self.int_literal(v, false, &Ty::Int, e.span),
            ExprKind::Float(text) => {
                self.number_literal_range(text, &Ty::Float, e.span);
                Ty::Float
            }
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
            ExprKind::Tagged { tag, parts, holes } => self.tagged(e, *tag, parts, holes),
            ExprKind::Local(l) => self.local_use(*l, e.span),
            ExprKind::Def(d) => self.def_value(e, *d),
            ExprKind::Lang(item) => self.lang_value(*item),
            ExprKind::ImplicitMember(name) => {
                if !self.unknown_variant(e.span, name) {
                    self.err(
                        e.span,
                        TypeErrorKind::ImplicitMemberNeedsType { name: name.clone() },
                    );
                }
                Ty::Error
            }
            ExprKind::AssocFn { ty, name } => match self.assoc_fn(e, ty, name) {
                Some((method, self_ty)) => self.method_value(e, method, self_ty.as_ref()),
                None => Ty::Error,
            },
            ExprKind::Call { callee, args } => self.call(e, callee, args, None),
            ExprKind::MethodCall {
                receiver,
                name,
                args,
            } => self.method_call(e, receiver, name, args, None),
            ExprKind::Field { base, name } => self.field(e, base, name),
            ExprKind::TupleField { base, index } => {
                let bt = self.synth(base);
                match self.resolve(&bt) {
                    Ty::Tuple(ts) if (*index as usize) < ts.len() => ts[*index as usize].clone(),
                    Ty::Error | Ty::Var(_) => Ty::Error,
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
                if matches!(operand.kind, ExprKind::Int(_) | ExprKind::Float(_)) {
                    return self.literal_operand(e, &Ty::Int);
                }
                let t = self.synth(operand);
                let r = self.resolve(&t);
                if !(r.is_numeric() || matches!(r, Ty::Error | Ty::Var(_))) {
                    self.err(e.span, TypeErrorKind::NoOperator { op: "-", ty: r });
                    Ty::Error
                } else {
                    t
                }
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(e, *op, lhs, rhs, None),
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
            ExprKind::Map(pairs) => match pairs.split_first() {
                None => {
                    self.err(e.span, TypeErrorKind::EmptyCollectionNeedsType);
                    Ty::Map(Box::new(Ty::Error), Box::new(Ty::Error))
                }
                Some(((k0, v0), rest)) => {
                    let k = self.synth(k0);
                    let v = self.synth(v0);
                    for (ke, ve) in rest {
                        self.check(ke, &k);
                        self.check(ve, &v);
                    }
                    // §9.3: a map's keys are `Eq + Hash` (an annotated literal was checked
                    // where the annotation is written).
                    for tr in crate::wf::key_traits() {
                        self.require(&k, tr, e.span, Origin::Key);
                    }
                    Ty::Map(Box::new(k), Box::new(v))
                }
            },
            ExprKind::Try(inner) => self.try_expr(e, inner, None),
            ExprKind::If {
                branches,
                else_body,
            } => self.if_value(e, branches, else_body.as_ref(), None),
            ExprKind::Match { scrutinee, arms } => self.match_expr(e, scrutinee, arms, None, true),
            ExprKind::Lambda {
                params,
                ret,
                body,
                closure,
            } => self.lambda(e, params, ret.as_ref(), body, *closure, None),
        };
        self.record(e, t)
    }

    /// A use of a local: K1110 when a closure uses a `var` of an enclosing scope (§12.3).
    fn local_use(&mut self, l: LocalId, span: Span) -> Ty {
        if let Some(decl) = self.decls.get(l.0 as usize) {
            let here = self.closures.last().copied().unwrap_or(0);
            if decl.mutable && decl.closure != here {
                // Reported once per `var` and closure-creating statement, with the fix.
                let closure = decl.closure;
                self.note_capture(l, span, closure);
            }
        }
        self.locals.get(&l).cloned().unwrap_or(Ty::Error)
    }

    /// Spec §5.6: an integer literal is `Int` unless the expected type is `Float`,
    /// `Decimal` or `BigInt`. K0126 when an `Int` literal does not fit in 64 bits (the
    /// direct operand of unary `-` counts as negative, so −2⁶³ is legal), and when a
    /// `Float` or `Decimal` one is out of that type's range ([`Self::number_literal_range`]).
    /// A `BigInt` literal has no limit.
    fn int_literal(&mut self, lit: &IntLit, negative: bool, exp: &Ty, span: Span) -> Ty {
        let t = match exp {
            Ty::Float | Ty::Decimal | Ty::BigInt => exp.clone(),
            _ => Ty::Int,
        };
        match t {
            Ty::Int => {
                let limit = if negative {
                    1u128 << 63
                } else {
                    (1u128 << 63) - 1
                };
                if lit.small().map_or(true, |v| v > limit) {
                    let digits = shown_literal(&lit.digits());
                    let literal = if negative {
                        format!("-{digits}")
                    } else {
                        digits
                    };
                    self.err(
                        span,
                        TypeErrorKind::LiteralOutOfRange {
                            literal,
                            ty: Ty::Int,
                        },
                    );
                }
            }
            Ty::Float | Ty::Decimal => self.number_literal_range(&lit.digits(), &t, span),
            _ => {}
        }
        t
    }

    /// K0126 for a number literal typed `Float` or `Decimal` that the type cannot hold
    /// (FOUNDER-QUEUE Q52, Q48), with the reason as a label: a `Float` literal that would be
    /// infinite (an underflow to 0 is not an error), or a `Decimal` literal that decimal128
    /// cannot hold exactly even after clamping (`1e6144` is 10³³ × 10⁶¹¹ and fits; `1e6145`
    /// and `1e-6177` do not). The reference interpreter accepts exactly the other literals.
    fn number_literal_range(&mut self, text: &str, ty: &Ty, span: Span) {
        let why = match ty {
            Ty::Float => {
                let clean: String = text.chars().filter(|c| *c != '_').collect();
                clean
                    .parse::<f64>()
                    .is_ok_and(f64::is_infinite)
                    .then_some("this is beyond the largest `Float` (about 1.8e308)")
            }
            Ty::Decimal => match crate::const_num::Dec::parse(text) {
                Some(Err(f)) => Some(match f {
                    crate::const_num::DecFail::Digits => {
                        "this needs more than 34 significant digits"
                    }
                    crate::const_num::DecFail::Exponent => "Decimal exponent out of range",
                }),
                _ => None,
            },
            _ => None,
        };
        if let Some(why) = why {
            let kind = TypeErrorKind::LiteralOutOfRange {
                literal: shown_literal(text),
                ty: ty.clone(),
            };
            self.push_error(TypeError::new(kind, self.file, span).with_label(span, why));
        }
    }

    /// A numeric literal (or a negated one) typed from `hint` without coercion; both the
    /// literal and the negation are recorded.
    fn literal_operand(&mut self, e: &Expr, hint: &Ty) -> Ty {
        let hint = self.resolve(hint);
        match &e.kind {
            ExprKind::Int(v) => {
                let t = self.int_literal(v, false, &hint, e.span);
                self.record(e, t)
            }
            ExprKind::Float(text) => {
                let t = if hint == Ty::Decimal {
                    Ty::Decimal
                } else {
                    Ty::Float
                };
                self.number_literal_range(text, &t, e.span);
                self.record(e, t)
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                let t = match &operand.kind {
                    ExprKind::Int(v) => self.int_literal(v, true, &hint, e.span),
                    ExprKind::Float(text) => {
                        let t = if hint == Ty::Decimal {
                            Ty::Decimal
                        } else {
                            Ty::Float
                        };
                        self.number_literal_range(text, &t, operand.span);
                        t
                    }
                    _ => Ty::Float,
                };
                self.record(operand, t.clone());
                self.record(e, t)
            }
            _ => self.synth(e),
        }
    }

    fn def_value(&mut self, e: &Expr, d: DefId) -> Ty {
        if self.res().signatures.contains_key(&d) {
            return self.method_value(e, d, None);
        }
        if let Some(t) = self.res().consts.get(&d) {
            return t.clone();
        }
        if let Some((adt, idx)) = self.variant_owner(d) {
            let name = match &self.res().adts[&adt].kind {
                AdtKind::Enum(vs) => vs[idx].name.clone(),
                _ => String::new(),
            };
            return self.variant_value(e, Some(adt), &name, &[], None);
        }
        Ty::Error
    }

    /// The instantiated type of a function or method used as a value (or called).
    /// `self_ty`, for `Type.f`, is unified with the impl's self type.
    fn method_value(&mut self, e: &Expr, d: DefId, self_ty: Option<&Ty>) -> Ty {
        let Some(scheme) = self.res().signatures.get(&d).cloned() else {
            return Ty::Error;
        };
        let (ty, args) = self.infer.instantiate(&scheme);
        if let Some(st) = self_ty {
            if let Some(&i) = self.cx.method_impl.get(&d) {
                let imp_self = self.res().traits.impls[i].self_ty.subst(&args);
                let _ = self.infer.try_unify(&imp_self, st);
            }
        }
        self.require_bounds(&scheme, &args, e.span);
        self.out.insts.push((e.id, args));
        ty
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

    // -- calls ----------------------------------------------------------------------------

    fn call(&mut self, e: &Expr, callee: &Expr, args: &[Arg], exp: Option<&Ty>) -> Ty {
        let exp = exp.map(|t| self.infer.shallow(t));
        match &callee.kind {
            ExprKind::Lang(item) => self.lang_call(e, callee, *item, args, exp.as_ref()),
            ExprKind::ImplicitMember(name) => {
                let t = self.variant_value(e, None, name, args, exp.as_ref());
                self.record(callee, t.clone());
                t
            }
            ExprKind::Def(d)
                if matches!(
                    self.res().adts.get(d).map(|a| &a.kind),
                    Some(AdtKind::Record(_))
                ) =>
            {
                let t = self.construct_record(e, *d, args, exp.as_ref());
                self.record(callee, t.clone());
                t
            }
            ExprKind::Def(d) if self.variant_owner(*d).is_some() => {
                let Some((adt, idx)) = self.variant_owner(*d) else {
                    return Ty::Error;
                };
                let name = match &self.res().adts[&adt].kind {
                    AdtKind::Enum(vs) => vs[idx].name.clone(),
                    _ => String::new(),
                };
                let t = self.variant_value(e, Some(adt), &name, args, exp.as_ref());
                self.record(callee, t.clone());
                t
            }
            ExprKind::Def(d) if self.res().signatures.contains_key(d) => {
                self.fn_call(e, callee, *d, None, args, exp.as_ref())
            }
            // K0922 (§21.3, DR-25): `f[T](x)` writes type arguments at a call site; they
            // come from inference or the expected type. The call is checked without them.
            ExprKind::Index { base, index } if self.is_type_expr(index) => {
                let fix = Fix::machine(
                    "remove the type arguments",
                    Span::new(base.span.end, callee.span.end),
                    "",
                );
                self.err_fix(callee.span, TypeErrorKind::ExplicitTypeArgs, vec![fix]);
                self.record(callee, Ty::Error);
                let t = self.call(e, base, args, exp.as_ref());
                self.record(e, t.clone());
                t
            }
            // [kalam-sinks] §16.1: a capability has no constructor; `World()` is K1501.
            ExprKind::TypeName(hir::Type {
                kind: TypeKind::Builtin(BuiltinType::Capability(c), _),
                ..
            }) => {
                let kind = match c {
                    Capability::World => {
                        TypeErrorKind::Sink(kalam_sinks::caps::construct_world(self.file, e.span))
                    }
                    other => TypeErrorKind::NotCallable {
                        ty: Ty::Capability(*other),
                    },
                };
                self.err(e.span, kind);
                self.record(callee, Ty::Error);
                for a in args {
                    self.synth(&a.value);
                }
                Ty::Error
            }
            ExprKind::AssocFn { ty, name } => match self.assoc_fn(callee, ty, name) {
                Some((method, self_ty)) => {
                    self.fn_call(e, callee, method, self_ty.as_ref(), args, exp.as_ref())
                }
                None => {
                    self.record(callee, Ty::Error);
                    for a in args {
                        self.synth(&a.value);
                    }
                    Ty::Error
                }
            },
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
                        for a in args.iter().skip(params.len()) {
                            self.synth(&a.value);
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
                        for a in args {
                            self.synth(&a.value);
                        }
                        Ty::Error
                    }
                }
            }
        }
    }

    /// A written type in value position: a type name, a record name, or a tuple of them
    /// (`f[Int, String]`).
    fn is_type_expr(&self, e: &Expr) -> bool {
        let mut stack: Vec<&Expr> = vec![e];
        while let Some(e) = stack.pop() {
            match &e.kind {
                ExprKind::TypeName(_) => {}
                ExprKind::Def(d)
                    if matches!(
                        self.res().adts.get(d).map(|a| &a.kind),
                        Some(AdtKind::Record(_))
                    ) => {}
                ExprKind::Tuple(xs) if !xs.is_empty() => stack.extend(xs.iter()),
                _ => return false,
            }
        }
        true
    }

    /// A call of a declared function or method by name (`f(…)`, `Type.f(…)`, `Trait.m(x)`).
    fn fn_call(
        &mut self,
        e: &Expr,
        callee: &Expr,
        d: DefId,
        self_ty: Option<&Ty>,
        args: &[Arg],
        exp: Option<&Ty>,
    ) -> Ty {
        let ft = self.method_value(callee, d, self_ty);
        self.record(callee, ft.clone());
        let Ty::Fn { params, ret, .. } = ft else {
            for a in args {
                self.synth(&a.value);
            }
            return Ty::Error;
        };
        // Return-type inference from the expected type (§9.9 rule 5).
        if let Some(x) = exp {
            let _ = self.infer.try_unify_hint(&ret, x);
        }
        let decl = self.param_decls(d, 0);
        self.check_labels(e.span, d, 0, args);
        self.args_by_name(e.span, &decl, &params, args);
        *ret
    }

    /// `(name, has default)` of each declared parameter of `d`, from `skip` on.
    fn param_decls(&self, d: DefId, skip: usize) -> Vec<(String, bool)> {
        match self.prog().item(d).map(|i| &i.kind) {
            Some(ItemKind::Fn(f)) => f
                .params
                .iter()
                .skip(skip)
                .map(|p| (p.name.clone(), p.default.is_some()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// K1104 (§12.1): an exported function with two adjacent parameters of one type must
    /// get at least one of them by name. `skip` is 1 for a method's `self`, which never
    /// counts.
    fn check_labels(&mut self, span: Span, d: DefId, skip: usize, args: &[Arg]) {
        let Some(item) = self.prog().item(d) else {
            return;
        };
        let ItemKind::Fn(f) = &item.kind else {
            return;
        };
        if !item.exported || f.positional {
            return;
        }
        let Some(Ty::Fn { params, .. }) = self.res().signatures.get(&d).map(|s| s.ty.clone())
        else {
            return;
        };
        let positional = args.iter().take_while(|a| a.label.is_none()).count();
        for i in skip..params.len().saturating_sub(1) {
            if params[i] != params[i + 1] {
                continue;
            }
            let second = i + 1 - skip;
            if second >= positional {
                continue;
            }
            let names = format!(
                "`{}`, `{}`",
                f.params.get(i).map_or("", |p| p.name.as_str()),
                f.params.get(i + 1).map_or("", |p| p.name.as_str())
            );
            // Label every positional argument from the second of the pair on, so no
            // positional argument follows a named one.
            let edits: Vec<Edit> = (second..positional)
                .filter_map(|j| {
                    let name = f.params.get(j + skip)?.name.clone();
                    Some(Edit {
                        span: Span::new(args[j].span.start, args[j].span.start),
                        insert: format!("{name}: "),
                    })
                })
                .collect();
            let fix = Fix {
                title: "name the arguments".into(),
                applicability: Applicability::Machine,
                edits,
            };
            self.err_fix(span, TypeErrorKind::NeedsLabels { names }, vec![fix]);
            return;
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
                    found.filter(|&f| f < params.len())
                }
            };
            match slot {
                Some(s) if slots[s].is_some() => {
                    self.bad_args(a.span, "argument given twice".into());
                    self.synth(&a.value);
                    continue;
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
        // The callee's type is the instance this call uses (closed with the arguments
        // below), so no type stays open for KIR.
        let callee_ty = self.lang_value(item);
        self.record(callee, callee_ty.clone());
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
                    let fixes = self.print_fix(args).into_iter().collect();
                    self.err_fix(e.span, TypeErrorKind::PrintArity, fixes);
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
                    if args.len() == 1 {
                        let used = Ty::func(vec![t], Ty::Unit, Suspension::Sync);
                        let _ = self.infer.try_unify(&callee_ty, &used);
                    }
                }
                Ty::Unit
            }
            LangItem::Panic => {
                arity(self, 1);
                for a in args.iter().take(1) {
                    self.check(&a.value, &Ty::String);
                }
                for a in args.iter().skip(1) {
                    self.synth(&a.value);
                }
                Ty::Never
            }
            LangItem::Assert => {
                arity(self, 2);
                for (a, t) in args.iter().zip([Ty::Bool, Ty::String]) {
                    self.check(&a.value, &t);
                }
                for a in args.iter().skip(2) {
                    self.synth(&a.value);
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
                for a in args {
                    self.synth(&a.value);
                }
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
                for a in args.iter().skip(1) {
                    self.synth(&a.value);
                }
                let used = Ty::func(vec![inner], whole.clone(), Suspension::Sync);
                let _ = self.infer.try_unify(&callee_ty, &used);
                whole
            }
        }
    }

    /// Record construction: named arguments only (K0416); defaulted fields may be omitted.
    fn construct_record(&mut self, e: &Expr, def: DefId, args: &[Arg], exp: Option<&Ty>) -> Ty {
        let Some(adt) = self.res().adts.get(&def).cloned() else {
            return Ty::Error;
        };
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
        self.check_private(def, e.span, &adt.name);
        let targs: Vec<Ty> = adt.generics.iter().map(|_| self.infer.fresh()).collect();
        let ty = Ty::Adt(def, targs.clone());
        if let Some(x) = exp {
            let _ = self.infer.try_unify_hint(&ty, x);
        }
        self.out.insts.push((e.id, targs.clone()));
        // §9.6: the record's type arguments satisfy its parameters' bounds.
        self.require_adt_bounds(def, &targs, e.span);
        if args.iter().any(|a| a.label.is_none()) {
            let fixes = record_label_fix(fields, args).into_iter().collect();
            self.err_fix(e.span, TypeErrorKind::RecordNeedsNamedArgs, fixes);
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

    /// `r.with(field: value, …)` (§9.4): a copy with some fields replaced.
    fn record_update(&mut self, e: &Expr, rt: &Ty, args: &[Arg]) -> Ty {
        let Ty::Adt(d, targs) = rt else {
            return Ty::Error;
        };
        let Some(adt) = self.res().adts.get(d).cloned() else {
            return Ty::Error;
        };
        let AdtKind::Record(fields) = &adt.kind else {
            return Ty::Error;
        };
        self.check_private(*d, e.span, "with");
        let mut given = vec![false; fields.len()];
        for a in args {
            let Some(l) = &a.label else {
                self.err(a.span, TypeErrorKind::RecordNeedsNamedArgs);
                self.synth(&a.value);
                continue;
            };
            match fields.iter().position(|f| &f.name == l) {
                Some(i) if given[i] => {
                    self.bad_args(a.span, "field given twice".into());
                    self.synth(&a.value);
                }
                Some(i) => {
                    given[i] = true;
                    let ft = fields[i].ty.subst(targs);
                    self.check(&a.value, &ft);
                }
                None => {
                    self.unknown_member(a.span, rt.clone(), l);
                    self.synth(&a.value);
                }
            }
        }
        rt.clone()
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
        // `.V` that no enum has: K0401, as resolve says in project mode (Q34).
        if adt.is_none() && self.unknown_variant(e.span, name) {
            for a in args {
                self.synth(&a.value);
            }
            return Ty::Error;
        }
        let exp = exp.map(|t| self.infer.shallow(t));
        let adt = match (adt, &exp) {
            (Some(a), _) => a,
            (None, Some(Ty::Adt(a, _))) if self.res().adts.contains_key(a) => *a,
            (None, Some(Ty::Error)) => {
                for a in args {
                    self.synth(&a.value);
                }
                return Ty::Error;
            }
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
        let Some(def) = self.res().adts.get(&adt).cloned() else {
            return Ty::Error;
        };
        let targs: Vec<Ty> = def.generics.iter().map(|_| self.infer.fresh()).collect();
        let ty = Ty::Adt(adt, targs.clone());
        if let Some(x) = &exp {
            let _ = self.infer.try_unify_hint(&ty, x);
        }
        let Some((_, variant)) = def.variant(name) else {
            self.unknown_member(e.span, ty.clone(), name);
            for a in args {
                self.synth(&a.value);
            }
            return Ty::Error;
        };
        self.check_private(adt, e.span, name);
        self.out.variants.push((e.id, variant.def));
        self.out.insts.push((e.id, targs.clone()));
        // §9.6: the enum's type arguments satisfy its parameters' bounds.
        self.require_adt_bounds(adt, &targs, e.span);
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

    // -- methods (DR-27) ------------------------------------------------------------------

    /// The methods named `name` that apply to `ty`: inherent methods, trait methods some
    /// impl writes out, and the methods of traits `ty` implements by derivation, by a
    /// built-in fact or by a bound of a generic parameter (DR-27).
    fn method_candidates(&self, ty: &Ty, name: &str) -> Vec<MethodCandidate> {
        let res = self.res();
        let mut out = res.traits.method_candidates(ty, name);
        let mut traits: Vec<(&TraitId, &TraitDecl)> = res.traits.traits.iter().collect();
        traits.sort_by_key(|(id, _)| format!("{id:?}"));
        for (tid, decl) in traits {
            let Some((_, method)) = decl.methods.iter().find(|(n, _)| n == name) else {
                continue;
            };
            if out.iter().any(|c| c.trait_id == Some(*tid)) || !decl.generics.is_empty() {
                continue;
            }
            let tr = TraitRef {
                id: *tid,
                args: Vec::new(),
            };
            let applies = match ty {
                Ty::Param(i) => self
                    .generics
                    .get(*i as usize)
                    .is_some_and(|g| g.bounds.contains(&tr)),
                _ => {
                    res.traits
                        .implements(&res.adts, &self.generics, &erase_vars(ty), &tr)
                        == Answer::Yes
                }
            };
            if applies {
                out.push(MethodCandidate {
                    impl_def: *method,
                    method: *method,
                    trait_id: Some(*tid),
                });
            }
        }
        let mut seen = HashSet::new();
        out.retain(|c| seen.insert(c.method));
        out
    }

    fn method_call(
        &mut self,
        e: &Expr,
        receiver: &Expr,
        name: &str,
        args: &[Arg],
        exp: Option<&Ty>,
    ) -> Ty {
        let rt = self.synth(receiver);
        let rt = self.resolve(&rt);
        if matches!(rt, Ty::Error | Ty::Var(_)) {
            for a in args {
                self.synth(&a.value);
            }
            return Ty::Error;
        }
        if name == "with"
            && matches!(&rt, Ty::Adt(d, _) if matches!(self.res().adts.get(d).map(|a| &a.kind), Some(AdtKind::Record(_))))
        {
            return self.record_update(e, &rt, args);
        }
        let candidates = self.method_candidates(&rt, name);
        let method = match candidates.as_slice() {
            [one] => one.method,
            [] => {
                if let Some(t) = self.field_called_as_method(e, receiver, &rt, name, args) {
                    return t;
                }
                self.unknown_member(e.span, rt, name);
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
                for a in args {
                    self.synth(&a.value);
                }
                return Ty::Error;
            }
        };
        self.out.methods.push((e.id, method));
        // [kalam-sinks] §16.1/§20.1: `env.get("API_KEY")` with a secret-looking key (K1503).
        if name == "get" && rt == Ty::Capability(Capability::Env) {
            if let Some(ExprKind::Str(key)) = args.first().map(|a| &a.value.kind) {
                let at = args[0].value.span;
                if let Some(d) = kalam_sinks::caps::env_get_secret_key(key, self.file, at) {
                    self.err(at, TypeErrorKind::Sink(d));
                }
            }
        }
        let Some(scheme) = self.res().signatures.get(&method).cloned() else {
            return Ty::Error;
        };
        let (ft, targs) = self.infer.instantiate(&scheme);
        self.require_bounds(&scheme, &targs, e.span);
        self.out.insts.push((e.id, targs));
        let Ty::Fn { params, ret, .. } = ft else {
            return Ty::Error;
        };
        // A method that does not match its trait is called the trait's way (Q37).
        let has_self = self
            .cx
            .contract_self
            .get(&method)
            .copied()
            .unwrap_or_else(|| {
                matches!(
                    self.prog().item(method).map(|i| &i.kind),
                    Some(ItemKind::Fn(f)) if f.has_self
                )
            });
        if !has_self || params.is_empty() {
            self.bad_args(
                e.span,
                format!("`{name}` is not a method; call it on the type"),
            );
            for a in args {
                self.synth(&a.value);
            }
            return *ret;
        }
        self.coerce(&rt, &params[0], receiver.span);
        if let Some(x) = exp {
            let x = self.infer.shallow(x);
            let _ = self.infer.try_unify_hint(&ret, &x);
        }
        let decl = self.param_decls(method, 1);
        self.check_labels(e.span, method, 1, args);
        self.args_by_name(e.span, &decl, &params[1..], args);
        *ret
    }

    /// `Type.name` that resolve left open: an associated function (or a method used as a
    /// function value) of an inherent or trait impl of the written type. Returns the
    /// method and the type to unify with the impl's self type.
    fn assoc_fn(&mut self, e: &Expr, ty: &hir::Type, name: &str) -> Option<(DefId, Option<Ty>)> {
        let t = self.head_type(ty);
        let t = self.resolve(&t);
        if matches!(t, Ty::Error) {
            return None;
        }
        let candidates = self.res().traits.method_candidates(&t, name);
        match candidates.as_slice() {
            [one] => {
                self.out.methods.push((e.id, one.method));
                Some((one.method, Some(t)))
            }
            [] => {
                self.unknown_member(e.span, t, name);
                None
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
                None
            }
        }
    }

    fn field(&mut self, e: &Expr, base: &Expr, name: &str) -> Ty {
        let bt = self.synth(base);
        let bt = self.resolve(&bt);
        let fields_of = |s: &Self, d: &DefId| match s.res().adts.get(d).map(|a| &a.kind) {
            Some(AdtKind::Record(fs)) => Some(fs.clone()),
            _ => None,
        };
        match &bt {
            Ty::Adt(d, targs) => {
                if let Some(fs) = fields_of(self, d) {
                    if let Some(f) = fs.iter().find(|f| f.name == name) {
                        self.check_private(*d, e.span, name);
                        return f.ty.subst(targs);
                    }
                }
            }
            Ty::Capability(Capability::World) => {
                if let Some(w) = self.prog().world {
                    if let Some(f) =
                        fields_of(self, &w).and_then(|fs| fs.into_iter().find(|f| f.name == name))
                    {
                        return f.ty;
                    }
                }
            }
            Ty::Error | Ty::Var(_) => return Ty::Error,
            _ => {}
        }
        self.unknown_member(e.span, bt, name);
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
            Ty::Error | Ty::Var(_) => {
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

    // -- tagged literals (§17) -------------------------------------------------------------

    fn tagged(&mut self, e: &Expr, tag: kalam_sinks::Tag, parts: &[String], holes: &[Expr]) -> Ty {
        let sink = SinkType::of_tag(tag);
        let mut hole_types = Vec::new();
        for h in holes {
            let t = self.synth(h);
            hole_types.push((h.span, t.clone()));
            // §17.6: a `path"…"` hole must be a `PathSegment` (K1641). The other tags'
            // hole types depend on the hole's context in the literal; they are checked
            // when `kalam-sinks::check_template` classifies them (next iteration).
            if sink == SinkType::Path {
                let r = self.resolve(&t);
                if !matches!(
                    r,
                    Ty::Sink(SinkType::PathSegment) | Ty::Error | Ty::Var(_) | Ty::Never
                ) {
                    let fix = Fix {
                        title: "validate the segment with `PathSegment.parse`".into(),
                        applicability: Applicability::Suggestion,
                        edits: vec![
                            Edit {
                                span: Span::new(h.span.start, h.span.start),
                                insert: "PathSegment.parse(".into(),
                            },
                            Edit {
                                span: Span::new(h.span.end, h.span.end),
                                insert: ")?".into(),
                            },
                        ],
                    };
                    self.err_fix(h.span, TypeErrorKind::PathHole { found: r }, vec![fix]);
                }
            }
        }
        self.out.tagged.push(TaggedLiteral {
            owner: self.owner,
            expr: e.id,
            file: self.file,
            span: e.span,
            tag,
            parts: parts.to_vec(),
            holes: hole_types,
        });
        Ty::Sink(sink)
    }

    /// K1601's fix: `"…" + x + "…"` (or a plain literal, or a `let` bound to one and used
    /// once) becomes `TAG"…{x}…"`, with SQL quotes around a hole dropped (the value is a
    /// bound parameter, §17.3).
    fn sink_fix(&self, e: &Expr, sink: SinkType) -> Option<Fix> {
        let tag = tag_of(sink)?;
        let (target, via_local) = match &e.kind {
            ExprKind::Local(l) => {
                let init = self.let_inits.get(l)?;
                let once = self.prog().local_uses.get(&(self.owner, *l)) == Some(&1);
                (init.clone(), Some(once))
            }
            _ => (e.clone(), None),
        };
        let mut pieces = Vec::new();
        if !self.concat_pieces(&target, &mut pieces) {
            return None;
        }
        let holes: Vec<usize> = pieces
            .iter()
            .enumerate()
            .filter_map(|(i, p)| matches!(p, Piece::Hole(_)).then_some(i))
            .collect();
        let mut texts: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut spans: Vec<Span> = Vec::new();
        for p in &pieces {
            match p {
                Piece::Lit(raw) => current.push_str(raw),
                Piece::Hole(s) => {
                    texts.push(std::mem::take(&mut current));
                    spans.push(*s);
                }
            }
        }
        texts.push(current);
        if sink == SinkType::Sql {
            for i in 0..spans.len() {
                let (before, after) = texts.split_at_mut(i + 1);
                if before[i].ends_with('\'') && after[0].starts_with('\'') {
                    before[i].pop();
                    after[0].remove(0);
                }
            }
        }
        let start = target.span.start;
        let end = target.span.end;
        let edits = if holes.is_empty() {
            vec![Edit {
                span: Span::new(start, end),
                insert: format!("{tag}\"{}\"", texts.concat()),
            }]
        } else {
            let mut edits = Vec::new();
            let mut from = start;
            for (i, s) in spans.iter().enumerate() {
                let open = if i == 0 {
                    format!("{tag}\"{}{{", texts[0])
                } else {
                    format!("}}{}{{", texts[i])
                };
                edits.push(Edit {
                    span: Span::new(from, s.start),
                    insert: open,
                });
                from = s.end;
            }
            edits.push(Edit {
                span: Span::new(from, end),
                insert: format!("}}{}\"", texts[spans.len()]),
            });
            edits
        };
        let applicability = match via_local {
            Some(false) => Applicability::Suggestion,
            _ => Applicability::Machine,
        };
        Some(Fix {
            title: format!("build it with a `{tag}\"…\"` literal: values go in `{{…}}` holes"),
            applicability,
            edits,
        })
    }

    /// Flattens a string `+` chain into pieces; false when it is not one (or has nothing to
    /// rewrite).
    ///
    /// Iterative (left to right with an explicit stack), so a long chain cannot overflow
    /// the stack.
    fn concat_pieces(&self, e: &Expr, out: &mut Vec<Piece>) -> bool {
        let mut stack: Vec<&Expr> = vec![e];
        while let Some(e) = stack.pop() {
            match &e.kind {
                ExprKind::Binary {
                    op: BinOp::Add,
                    lhs,
                    rhs,
                } => {
                    stack.push(rhs);
                    stack.push(lhs);
                }
                ExprKind::Str(_) => match self.prog().raw_strings.get(&(self.owner, e.id)) {
                    Some(raw) => out.push(Piece::Lit(raw.clone())),
                    None => return false,
                },
                ExprKind::Interp(_) => return false,
                _ => out.push(Piece::Hole(e.span)),
            }
        }
        true
    }

    // -- operators --------------------------------------------------------------------------

    fn is_literal(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Float(_) => true,
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => matches!(operand.kind, ExprKind::Int(_) | ExprKind::Float(_)),
            _ => false,
        }
    }

    fn is_float_literal(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Float(_) => true,
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => matches!(operand.kind, ExprKind::Float(_)),
            _ => false,
        }
    }

    /// The operand types of a binary operator, with literal typing: a literal takes its
    /// type from the other operand, whichever side it is on (§5.6, ARCHITECTURE §8.1). Two
    /// literals in an arithmetic operator take the expected type, or `Float` when one of
    /// them is a float literal (FOUNDER-QUEUE Q25); in a comparison each keeps its own type
    /// (§9.8: `1 == 1.0` is K0602).
    fn operands(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, hint: Option<&Ty>) -> (Ty, Ty) {
        let (ll, rl) = (Self::is_literal(lhs), Self::is_literal(rhs));
        if ll && rl {
            if op.is_arithmetic() {
                let float = Self::is_float_literal(lhs) || Self::is_float_literal(rhs);
                let hint = hint.map(|h| self.infer.shallow(h));
                let base = match hint {
                    Some(Ty::Decimal) => Ty::Decimal,
                    Some(Ty::BigInt) if !float => Ty::BigInt,
                    Some(Ty::Float) => Ty::Float,
                    _ if float => Ty::Float,
                    _ => Ty::Int,
                };
                return (
                    self.literal_operand(lhs, &base),
                    self.literal_operand(rhs, &base),
                );
            }
            let lt = self.literal_operand(lhs, &Ty::Int);
            let rt = self.literal_operand(rhs, &Ty::Int);
            return (lt, rt);
        }
        if ll {
            let rt = self.synth(rhs);
            return (self.literal_operand(lhs, &rt), rt);
        }
        let lt = self.synth(lhs);
        let rt = if rl {
            self.literal_operand(rhs, &lt)
        } else {
            self.synth(rhs)
        };
        (lt, rt)
    }

    fn binary(&mut self, e: &Expr, op: BinOp, lhs: &Expr, rhs: &Expr, hint: Option<&Ty>) -> Ty {
        let span = e.span;
        if matches!(op, BinOp::And | BinOp::Or) {
            self.check(lhs, &Ty::Bool);
            self.check(rhs, &Ty::Bool);
            return Ty::Bool;
        }
        let (lt, rt) = self.operands(op, lhs, rhs, hint);
        let (l, r) = (self.resolve(&lt), self.resolve(&rt));
        let is_cmp = op.is_comparison();
        if matches!(l, Ty::Error) || matches!(r, Ty::Error) {
            return if is_cmp { Ty::Bool } else { Ty::Error };
        }
        let is_eq = matches!(op, BinOp::Eq | BinOp::NotEq);
        if l.is_numeric() && r.is_numeric() && l != r {
            if is_eq {
                self.err(span, TypeErrorKind::BadEquality { left: l, right: r });
                return Ty::Bool;
            }
            let fixes = mixed_fix(lhs, &l, rhs, &r).into_iter().collect();
            self.err_fix(
                span,
                TypeErrorKind::MixedNumeric { left: l, right: r },
                fixes,
            );
            return if is_cmp { Ty::Bool } else { Ty::Error };
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
            return if is_cmp { Ty::Bool } else { Ty::Error };
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
            BinOp::And | BinOp::Or => Ty::Bool,
        }
    }

    // -- `?` (§14.3) ------------------------------------------------------------------------

    fn try_expr(&mut self, e: &Expr, inner: &Expr, exp: Option<&Ty>) -> Ty {
        let ret = self.ret();
        let ret = self.infer.shallow(&ret);
        // The expected type of `e?` is the success type of the operand: a generic call's
        // result is inferred from it (`let u: User = json.decode(text)?`).
        let hint = match (&ret, exp) {
            (Ty::Result(_, _), Some(x)) => Some(Ty::result(x.clone(), self.infer.fresh())),
            (Ty::Option(_), Some(x)) => Some(Ty::option(x.clone())),
            _ => None,
        };
        let t = match (&inner.kind, &hint) {
            (ExprKind::Call { callee, args }, Some(h)) => {
                let t = self.call(inner, callee, args, Some(h));
                self.record(inner, t)
            }
            (
                ExprKind::MethodCall {
                    receiver,
                    name,
                    args,
                },
                Some(h),
            ) => {
                let t = self.method_call(inner, receiver, name, args, Some(h));
                self.record(inner, t)
            }
            _ => self.synth(inner),
        };
        let t = self.resolve(&t);
        let ret = self.resolve(&ret);
        match (&t, &ret) {
            (Ty::Error, _) | (_, Ty::Error) => Ty::Error,
            (Ty::Result(ok, err_ty), Ty::Result(_, f)) => {
                let conv = if self.infer.try_unify(err_ty, f) {
                    TryConversion::Same
                } else {
                    let (err_ty, f) = (self.resolve(err_ty), self.resolve(f));
                    let from = TraitRef {
                        id: TraitId::Builtin(BuiltinTrait::From),
                        args: vec![err_ty.clone()],
                    };
                    let error = TraitRef::builtin(BuiltinTrait::Error);
                    if self.answer(&f, &from) != Answer::No {
                        let method = self.conversion_method(&f, &err_ty);
                        TryConversion::From { method }
                    } else if f == Ty::AnyError && self.answer(&err_ty, &error) != Answer::No {
                        TryConversion::BoxAnyError
                    } else {
                        // Rule 4: K1301, with `.map_err(…)` and `impl F as From[E]`.
                        let fixes = self.convert_error_fixes(inner, &err_ty, &f);
                        self.err_fix(
                            e.span,
                            TypeErrorKind::TryCannotConvert {
                                found: err_ty,
                                expected: f,
                            },
                            fixes,
                        );
                        return (**ok).clone();
                    }
                };
                self.out.tries.push((e.id, conv));
                (**ok).clone()
            }
            (Ty::Result(ok, err_ty), Ty::Var(_)) => {
                // A lambda whose return type is not known yet (FOUNDER-QUEUE Q28): `?`
                // returns from the lambda, so it returns a `Result` with this error type.
                // Its success type is the body's, not the operand's.
                let want = Ty::result(self.infer.fresh(), (**err_ty).clone());
                let _ = self.infer.try_unify(&ret, &want);
                self.out.tries.push((e.id, TryConversion::Same));
                (**ok).clone()
            }
            (Ty::Result(ok, err_ty), Ty::Option(_)) => {
                self.err(
                    e.span,
                    TypeErrorKind::TryCannotConvert {
                        found: (**err_ty).clone(),
                        expected: ret.clone(),
                    },
                );
                (**ok).clone()
            }
            (Ty::Result(ok, err_ty), _) => {
                let found = (**err_ty).clone();
                self.try_in_plain_fn(e, found, ret.clone(), false);
                (**ok).clone()
            }
            (Ty::Option(inner_ty), Ty::Option(_)) => {
                self.out.tries.push((e.id, TryConversion::OptionNone));
                (**inner_ty).clone()
            }
            (Ty::Option(inner_ty), Ty::Var(_)) => {
                let fresh = Ty::option(self.infer.fresh());
                let _ = self.infer.try_unify(&ret, &fresh);
                self.out.tries.push((e.id, TryConversion::OptionNone));
                (**inner_ty).clone()
            }
            (Ty::Option(inner_ty), Ty::Result(_, f)) => {
                // K1302, with `.ok_or(…)` before the `?` (§14.3).
                let fix = self.ok_or_fix(inner, f);
                self.err_fix(e.span, TypeErrorKind::TryOptionInResult, vec![fix]);
                (**inner_ty).clone()
            }
            (Ty::Option(inner_ty), _) => {
                let found = Ty::option(self.infer.fresh());
                self.try_in_plain_fn(e, found, ret.clone(), true);
                (**inner_ty).clone()
            }
            (Ty::Var(_), _) => Ty::Error,
            (other, _) => {
                let expected = Ty::result(Ty::Error, Ty::Error);
                self.err(
                    inner.span,
                    TypeErrorKind::Mismatch {
                        expected,
                        found: other.clone(),
                    },
                );
                Ty::Error
            }
        }
    }

    /// `?` in a function (or lambda) that returns neither `Result` nor `Option`: nothing
    /// converts what it would return early into the return type (K1301, never K1302, whose
    /// text is about a `Result` function); the label names the return type.
    fn try_in_plain_fn(&mut self, e: &Expr, found: Ty, ret: Ty, option: bool) {
        let (what, kind) = if option {
            ("`None`", "an `Option`")
        } else {
            ("the error", "a `Result`")
        };
        let label = format!(
            "`?` would return {what} from here, but this function returns `{}`: only a function that returns {kind} can use `?` on it",
            ret.display_with(&|d| self.res().name_of(d), &self.param_names())
        );
        let err = TypeError::new(
            TypeErrorKind::TryCannotConvert {
                found,
                expected: ret,
            },
            self.file,
            e.span,
        )
        .with_label(e.span, label);
        self.push_error(err);
    }

    /// The `from` method of the impl `F as From[E]`, if one is written out.
    fn conversion_method(&self, f: &Ty, e: &Ty) -> Option<DefId> {
        let res = self.res();
        res.traits.impls.iter().find_map(|imp| {
            let tr = imp.trait_ref.as_ref()?;
            if imp.rejected || tr.id != TraitId::Builtin(BuiltinTrait::From) || tr.args.len() != 1 {
                return None;
            }
            let mut cx = InferCtx::new();
            let fresh: Vec<Ty> = imp.generics.iter().map(|_| cx.fresh()).collect();
            let head = Ty::Tuple(vec![imp.self_ty.subst(&fresh), tr.args[0].subst(&fresh)]);
            let goal = Ty::Tuple(vec![erase_vars(f), erase_vars(e)]);
            cx.unify(&goal, &head).ok()?;
            imp.methods
                .iter()
                .find(|(n, _)| n == "from")
                .map(|(_, m)| *m)
        })
    }

    // -- closures ---------------------------------------------------------------------------

    fn lambda(
        &mut self,
        e: &Expr,
        params: &[hir::LambdaParam],
        ret: Option<&hir::Type>,
        body: &Expr,
        closure: u32,
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
        let mut reported = false;
        for (i, p) in params.iter().enumerate() {
            let t = match (&p.ty, &exp_params) {
                (Some(t), Some(ps)) => {
                    let t = self.local_type(t);
                    self.coerce(&ps[i], &t, e.span);
                    t
                }
                (Some(t), None) => self.local_type(t),
                (None, Some(ps)) => ps[i].clone(),
                (None, None) => {
                    if !reported {
                        self.err(e.span, TypeErrorKind::LambdaNeedsTypes);
                        reported = true;
                    }
                    Ty::Error
                }
            };
            self.locals.insert(p.local, t.clone());
            ptys.push(t);
        }
        let declared_ret = ret.map(|t| self.local_type(t));
        let ret_ty = match (&declared_ret, &exp_ret) {
            (Some(r), _) => r.clone(),
            (None, Some(r)) => r.clone(),
            (None, None) => self.infer.fresh(),
        };
        self.closures.push(closure);
        self.rets.push(ret_ty.clone());
        let body_ty = self.check(body, &ret_ty);
        self.rets.pop();
        self.closures.pop();
        let ret_ty = match (&declared_ret, &exp_ret) {
            (None, None) => body_ty,
            _ => ret_ty,
        };
        // A lambda's bit comes from its body (§15.3): an effect variable for `kalam-effects`.
        let s = self.infer.fresh_effect();
        Ty::func(ptys, ret_ty, s)
    }

    fn local_fn(&mut self, local: LocalId, closure: u32, decl: &FnDecl) {
        let mut ptys = Vec::new();
        for p in &decl.params {
            let t = self.local_type(&p.ty);
            ptys.push(t);
        }
        let ret = match &decl.ret {
            Some(t) => self.local_type(t),
            None => Ty::Unit,
        };
        let s = self.infer.fresh_effect();
        self.locals
            .insert(local, Ty::func(ptys.clone(), ret.clone(), s));
        for (p, t) in decl.params.iter().zip(&ptys) {
            self.locals.insert(p.local, t.clone());
        }
        self.closures.push(closure);
        for (p, t) in decl.params.iter().zip(&ptys) {
            if let Some(d) = &p.default {
                self.check(d, t);
            }
        }
        if let Some(body) = &decl.body {
            self.rets.push(ret.clone());
            self.fn_body(&body.value, &ret);
            self.rets.pop();
        }
        self.closures.pop();
    }

    // -- if / match -------------------------------------------------------------------------

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
        let mut patterns_fit = true;
        for arm in arms {
            let before = self.out.errors.len();
            self.pat(&arm.pat, &st);
            patterns_fit &= self.out.errors.len() == before;
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
        // A pattern that does not fit the scrutinee is the root cause: its K1001 (and the
        // fix that would add arms to a broken match) waits until the patterns type-check.
        if patterns_fit {
            self.check_arms(e.span, &st, arms);
        }
        if value {
            result.unwrap_or(Ty::Never)
        } else {
            Ty::Unit
        }
    }

    /// Exhaustiveness (K1001) and usefulness (K1002) of a `match` (§11 rules 1–2).
    fn check_arms(&mut self, span: Span, st: &Ty, arms: &[Arm]) {
        if matches!(st, Ty::Error | Ty::Var(_) | Ty::Never) {
            return;
        }
        let collector: &Collector<'_> = self.cx;
        let cx = exhaust::Cx {
            adts: &collector.res.adts,
        };
        let lowered: Vec<(exhaust::DPat, bool)> = arms
            .iter()
            .map(|a| (cx.lower(&a.pat, st), a.guard.is_some()))
            .collect();
        let report = cx.check_match(st, &lowered);
        for i in report.unreachable {
            if let Some(arm) = arms.get(i) {
                self.err(arm.pat.span, TypeErrorKind::UnreachableArm);
            }
        }
        if !report.missing.is_empty() {
            // §11 rule 1 (Q32): the missing cases as arms, as a machine fix.
            let fixes = self
                .missing_arms_fix(arms, st, &report.witnesses)
                .into_iter()
                .collect();
            self.err_fix(
                span,
                TypeErrorKind::NonExhaustive {
                    missing: report.missing,
                },
                fixes,
            );
        }
    }

    /// K1003: a `let` or `for` pattern must match every value of its type.
    fn check_irrefutable(&mut self, pat: &Pat, ty: &Ty) {
        let ty = self.resolve(ty);
        if matches!(ty, Ty::Error | Ty::Var(_) | Ty::Never) {
            if !pat.is_irrefutable() && !matches!(ty, Ty::Error) {
                self.err(pat.span, TypeErrorKind::RefutableLet);
            }
            return;
        }
        let collector: &Collector<'_> = self.cx;
        let cx = exhaust::Cx {
            adts: &collector.res.adts,
        };
        let p = cx.lower(pat, &ty);
        if !cx.irrefutable(&p, &ty) {
            self.err(pat.span, TypeErrorKind::RefutableLet);
        }
    }

    // -- patterns -------------------------------------------------------------------------

    fn pat(&mut self, p: &Pat, ty: &Ty) {
        kalam_ast::stack::deeper(|| self.pat_inner(p, ty))
    }

    fn pat_inner(&mut self, p: &Pat, ty: &Ty) {
        let span = p.span;
        match &p.kind {
            PatKind::Wild | PatKind::Error => {}
            PatKind::Bind(l) => {
                self.bind_pat_local(*l, ty, span);
            }
            PatKind::As { pat, local } => {
                self.pat(pat, ty);
                self.bind_pat_local(*local, ty, span);
            }
            PatKind::Int { negative, value } => {
                self.coerce(&Ty::Int, ty, span);
                let limit = if *negative {
                    1u128 << 63
                } else {
                    (1u128 << 63) - 1
                };
                if *value > limit {
                    let literal = if *negative {
                        format!("-{value}")
                    } else {
                        value.to_string()
                    };
                    self.err(
                        span,
                        TypeErrorKind::LiteralOutOfRange {
                            literal,
                            ty: Ty::Int,
                        },
                    );
                }
            }
            PatKind::Range { .. } => self.coerce(&Ty::Int, ty, span),
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
            PatKind::List {
                before,
                rest,
                after,
            } => {
                let elem = self.infer.fresh();
                self.coerce(&Ty::list(elem.clone()), ty, span);
                for p in before.iter().chain(after) {
                    self.pat(p, &elem);
                }
                if let Some(Some(l)) = rest {
                    self.bind_pat_local(*l, &Ty::list(elem), span);
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
            } => self.variant_pat(span, *def, name, fields, *rest, ty),
            PatKind::Record { def, fields, rest } => {
                let Some(adt) = self.res().adts.get(def).cloned() else {
                    return;
                };
                let AdtKind::Record(decl) = &adt.kind else {
                    return;
                };
                self.check_private(*def, span, &adt.name);
                let targs: Vec<Ty> = adt.generics.iter().map(|_| self.infer.fresh()).collect();
                self.coerce(&Ty::Adt(*def, targs.clone()), ty, span);
                let mut given = vec![false; decl.len()];
                for f in fields {
                    let slot = match &f.label {
                        Some(l) => decl.iter().position(|d| &d.name == l),
                        None => None,
                    };
                    match slot {
                        Some(i) => {
                            given[i] = true;
                            let ft = decl[i].ty.subst(&targs);
                            self.pat(&f.pat, &ft);
                        }
                        None => {
                            let detail = match &f.label {
                                Some(l) => format!("no field named `{l}`"),
                                None => "record patterns name their fields".into(),
                            };
                            self.bad_args(f.pat.span, detail);
                        }
                    }
                }
                if !rest {
                    let missing: Vec<&str> = decl
                        .iter()
                        .zip(&given)
                        .filter(|(_, g)| !**g)
                        .map(|(d, _)| d.name.as_str())
                        .collect();
                    if !missing.is_empty() {
                        self.bad_args(
                            span,
                            format!("missing field(s) {}; add them or `..`", missing.join(", ")),
                        );
                    }
                }
            }
            PatKind::Or(ps) => self.or_pat(span, ps, ty),
        }
    }

    /// A binding in a pattern. In an or-pattern every alternative binds the same local, so
    /// a second binding must have the first one's type (K1004).
    fn bind_pat_local(&mut self, l: LocalId, ty: &Ty, span: Span) {
        if let Some(prev) = self.locals.get(&l).cloned() {
            if !self.infer.try_unify(&prev, ty) {
                let name = self
                    .decls
                    .get(l.0 as usize)
                    .map_or(String::new(), |d| d.name.clone());
                self.err(span, TypeErrorKind::OrPatternBindings { name });
            }
            return;
        }
        self.locals.insert(l, ty.clone());
    }

    /// §11 rule 5: the alternatives of an or-pattern bind the same names, at the same types.
    fn or_pat(&mut self, span: Span, ps: &[Pat], ty: &Ty) {
        let mut sets: Vec<Vec<LocalId>> = Vec::new();
        for p in ps {
            self.pat(p, ty);
            let mut bound = Vec::new();
            pat_bindings(p, &mut bound);
            bound.sort();
            bound.dedup();
            sets.push(bound);
        }
        if let Some(first) = sets.first() {
            let differs = sets.iter().find_map(|s| {
                s.iter()
                    .find(|l| !first.contains(l))
                    .or_else(|| first.iter().find(|l| !s.contains(l)))
                    .copied()
            });
            if let Some(l) = differs {
                let name = self
                    .decls
                    .get(l.0 as usize)
                    .map_or(String::new(), |d| d.name.clone());
                self.err(span, TypeErrorKind::OrPatternBindings { name });
            }
        }
    }

    fn variant_pat(
        &mut self,
        span: Span,
        def: Option<DefId>,
        name: &str,
        fields: &[hir::PatField],
        rest: bool,
        ty: &Ty,
    ) {
        // `.V` that no enum has: K0401, as resolve says in project mode (Q34).
        if def.is_none() && self.unknown_variant(span, name) {
            for f in fields {
                self.pat(&f.pat, &Ty::Error);
            }
            return;
        }
        let st = self.resolve(ty);
        let (adt, targs) = match (&st, def.and_then(|d| self.variant_owner(d))) {
            (_, Some((a, _))) => {
                let n = self.res().adts[&a].generics.len();
                let targs: Vec<Ty> = (0..n).map(|_| self.infer.fresh()).collect();
                self.coerce(&Ty::Adt(a, targs.clone()), ty, span);
                (a, targs)
            }
            (Ty::Adt(a, targs), None) if self.res().adts.contains_key(a) => (*a, targs.clone()),
            (Ty::Error, _) => {
                for f in fields {
                    self.pat(&f.pat, &Ty::Error);
                }
                return;
            }
            _ => {
                self.err(
                    span,
                    TypeErrorKind::ImplicitMemberNeedsType { name: name.into() },
                );
                for f in fields {
                    self.pat(&f.pat, &Ty::Error);
                }
                return;
            }
        };
        let Some(v) = self.res().adts[&adt].variant(name).map(|(_, v)| v.clone()) else {
            self.unknown_member(span, st, name);
            for f in fields {
                self.pat(&f.pat, &Ty::Error);
            }
            return;
        };
        self.check_private(adt, span, name);
        if fields.len() > v.fields.len() || (!rest && fields.len() != v.fields.len()) {
            self.bad_args(span, format!("`{name}` has {} field(s)", v.fields.len()));
        }
        for (i, f) in fields.iter().enumerate() {
            let slot = match &f.label {
                Some(l) => v
                    .fields
                    .iter()
                    .position(|vf| vf.label.as_deref() == Some(l)),
                None => (i < v.fields.len()).then_some(i),
            };
            match slot.and_then(|s| v.fields.get(s)) {
                Some(vf) => {
                    let ft = vf.ty.subst(&targs);
                    self.pat(&f.pat, &ft);
                }
                None => {
                    if let Some(l) = &f.label {
                        self.bad_args(f.pat.span, format!("`{name}` has no field `{l}`"));
                    }
                    self.pat(&f.pat, &Ty::Error);
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
        // Where K1110's fix can insert `let snapshot = v` (before this statement).
        self.stmt_stack.push(StmtCtx {
            closure: self.closures.last().copied().unwrap_or(0),
            start: s.span.start,
            indent: s.indent,
            while_head: matches!(s.kind, StmtKind::While { .. }),
        });
        self.stmt_inner(s);
        self.stmt_stack.pop();
    }

    fn stmt_inner(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Pass | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(e) => self.expr_stmt(e),
            StmtKind::Let { pat, ty, value } => {
                let t = match ty {
                    Some(t) => {
                        let t = self.local_type(t);
                        self.check(value, &t);
                        t
                    }
                    None => self.synth(value),
                };
                self.pat(pat, &t);
                self.check_irrefutable(pat, &t);
                if let PatKind::Bind(l) = pat.kind {
                    if matches!(
                        value.kind,
                        ExprKind::Binary { op: BinOp::Add, .. } | ExprKind::Str(_)
                    ) {
                        self.let_inits.insert(l, value.clone());
                    }
                }
            }
            StmtKind::Var { local, ty, value } => {
                let t = match ty {
                    Some(t) => {
                        let t = self.local_type(t);
                        self.check(value, &t);
                        t
                    }
                    None => self.synth(value),
                };
                self.locals.insert(*local, t);
            }
            StmtKind::Assign { place, op, value } => self.assign(place, *op, value, s.span),
            StmtKind::Return(value) => {
                let ret = self.ret();
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
                    Ty::Error | Ty::Var(_) => Ty::Error,
                    other => {
                        self.err(iter.span, TypeErrorKind::NotIterable { ty: other });
                        Ty::Error
                    }
                };
                self.pat(pat, &elem);
                self.check_irrefutable(pat, &elem);
                self.check_stmts(body);
            }
            StmtKind::While { cond, body } => {
                self.check(cond, &Ty::Bool);
                self.check_stmts(body);
            }
            StmtKind::LocalFn {
                local,
                closure,
                decl,
            } => self.local_fn(*local, *closure, decl),
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

    /// `x = e`, `x += e`, `p.f = e`, `xs[i] = e`, `m[k] = e` (§10.4): the root must be a
    /// `var` (K0710).
    fn assign(&mut self, place: &Expr, op: AssignOp, value: &Expr, span: Span) {
        let mut root = place;
        while let ExprKind::Field { base, .. }
        | ExprKind::TupleField { base, .. }
        | ExprKind::Index { base, .. } = &root.kind
        {
            root = base;
        }
        let ok = matches!(&root.kind, ExprKind::Local(l) if self.decls.get(l.0 as usize).is_some_and(|d| d.mutable));
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
        let rt = if Self::is_literal(value) {
            self.literal_operand(value, &pt)
        } else {
            self.synth(value)
        };
        let (l, r) = (self.resolve(&pt), self.resolve(&rt));
        if matches!(l, Ty::Error) || matches!(r, Ty::Error) {
            return;
        }
        if l.is_numeric() && r.is_numeric() && l != r {
            let fixes = numeric_fix(value, &r, &l).into_iter().collect();
            self.err_fix(
                span,
                TypeErrorKind::MixedNumeric { left: l, right: r },
                fixes,
            );
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
            // `x /= n` on an `Int` would make `x` a `Float` (Q31): K0600, suggesting `//`.
            (BinOp::Div, Ty::Int) => {
                let fixes = self.div_assign_fix(place, value).into_iter().collect();
                self.err_fix(
                    span,
                    TypeErrorKind::Mismatch {
                        expected: Ty::Int,
                        found: Ty::Float,
                    },
                    fixes,
                );
                return;
            }
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
            (_, t) if t.is_numeric() || matches!(t, Ty::Var(_)) => l.clone(),
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

/// The locals a pattern binds (an or-pattern's first alternative stands for all of them).
/// Iterative, so a deeply nested pattern cannot overflow the stack.
fn pat_bindings(p: &Pat, out: &mut Vec<LocalId>) {
    let mut stack: Vec<&Pat> = vec![p];
    while let Some(p) = stack.pop() {
        match &p.kind {
            PatKind::Bind(l) => out.push(*l),
            PatKind::As { pat, local } => {
                out.push(*local);
                stack.push(pat);
            }
            PatKind::Tuple(ps) => stack.extend(ps.iter()),
            PatKind::Lang {
                inner: Some(inner), ..
            } => stack.push(inner),
            PatKind::Variant { fields, .. } | PatKind::Record { fields, .. } => {
                stack.extend(fields.iter().map(|f| &f.pat))
            }
            PatKind::List {
                before,
                rest,
                after,
            } => {
                stack.extend(before.iter().chain(after));
                if let Some(Some(l)) = rest {
                    out.push(*l);
                }
            }
            PatKind::Or(ps) => stack.extend(ps.first()),
            _ => {}
        }
    }
}

/// The conversion that makes an `Int` operand fit a `Float`/`Decimal`/`BigInt` one
/// (K0601's fix, §10.6: "with a fix to `.to_float()`").
fn numeric_fix(e: &Expr, found: &Ty, expected: &Ty) -> Option<Fix> {
    if *found != Ty::Int {
        return None;
    }
    let postfix = matches!(
        e.kind,
        ExprKind::Local(_)
            | ExprKind::Def(_)
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Call { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::Int(_)
    );
    let (title, edits) = match expected {
        Ty::Float if postfix => (
            "convert with `.to_float()`",
            vec![Edit {
                span: Span::new(e.span.end, e.span.end),
                insert: ".to_float()".into(),
            }],
        ),
        Ty::Float => (
            "convert with `.to_float()`",
            vec![
                Edit {
                    span: Span::new(e.span.start, e.span.start),
                    insert: "(".into(),
                },
                Edit {
                    span: Span::new(e.span.end, e.span.end),
                    insert: ").to_float()".into(),
                },
            ],
        ),
        Ty::Decimal | Ty::BigInt => {
            let ty = if *expected == Ty::Decimal {
                "Decimal"
            } else {
                "BigInt"
            };
            (
                if *expected == Ty::Decimal {
                    "convert with `Decimal.from`"
                } else {
                    "convert with `BigInt.from`"
                },
                vec![
                    Edit {
                        span: Span::new(e.span.start, e.span.start),
                        insert: format!("{ty}.from("),
                    },
                    Edit {
                        span: Span::new(e.span.end, e.span.end),
                        insert: ")".into(),
                    },
                ],
            )
        }
        _ => return None,
    };
    Some(Fix {
        title: title.into(),
        applicability: Applicability::Machine,
        edits,
    })
}

/// K0601's fix for a binary operator: convert whichever operand is the `Int`.
fn mixed_fix(lhs: &Expr, l: &Ty, rhs: &Expr, r: &Ty) -> Option<Fix> {
    numeric_fix(lhs, l, r).or_else(|| numeric_fix(rhs, r, l))
}

/// K0416's fix: name each positional argument after the field at its position.
fn record_label_fix(fields: &[FieldDef], args: &[Arg]) -> Option<Fix> {
    let mut edits = Vec::new();
    for (i, a) in args.iter().enumerate() {
        if a.label.is_some() {
            continue;
        }
        let f = fields.get(i)?;
        edits.push(Edit {
            span: Span::new(a.span.start, a.span.start),
            insert: format!("{}: ", f.name),
        });
    }
    let positional_after_named = args
        .iter()
        .skip_while(|a| a.label.is_none())
        .any(|a| a.label.is_none());
    if edits.is_empty() || positional_after_named {
        return None;
    }
    Some(Fix {
        title: "name the fields".into(),
        applicability: Applicability::Machine,
        edits,
    })
}

/// A literal's digits for a K0126 message: in full up to 40 characters, otherwise the first
/// 20 and the length.
fn shown_literal(text: &str) -> String {
    let n = text.chars().count();
    if n <= 40 {
        text.to_string()
    } else {
        let head: String = text.chars().take(20).collect();
        format!("{head}… ({n} characters)")
    }
}
