//! Traits, impls, coherence, the orphan rule and trait solving (spec §9.7, DR-10, DR-27).
//!
//! - A trait is a built-in trait of the §9.7 table or a user `trait` declaration.
//! - **Coherence:** at most one `impl T as Tr` per program, with no overlap. Two impls of
//!   one trait overlap when their heads (self type and trait arguments) unify after their
//!   own generics are replaced by fresh variables. Bounds are ignored, so the check is
//!   conservative (no negative reasoning), as the spec's "no overlap" requires.
//! - **Orphan rule:** a trait impl must live in the module that defines the trait or the
//!   module that defines the self type's head. Built-in traits and built-in types are
//!   defined by the core (`module = None`), so `impl Int as Show` is always an orphan.
//! - **Solving:** built-in facts (§9.7 "never implemented by" plus structural derivation),
//!   explicit `@derive`s, user impls, and the bounds of the generic parameters in scope.
//!   A recursive ADT is handled coinductively. An unresolved variable answers `Unknown`;
//!   the checker re-asks after the body is solved.
//! - **Method candidates** (DR-27) are the inherent methods of the receiver's type plus the
//!   methods of every trait implemented for it anywhere in the program. Exactly one
//!   candidate must exist (K1103). Imports never matter.

use std::collections::HashMap;

use kalam_base::FileSpan;
use kalam_resolve::{DefId, ModulePath};

use crate::adt::AdtDef;
use crate::ty::{GenericParam, SinkType, Ty};
use crate::unify::InferCtx;

/// Built-in traits (spec §9.7 table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinTrait {
    Eq,
    Hash,
    Ord,
    Show,
    Encode,
    Decode,
    Row,
    Message,
    Error,
    /// `From[E]`, used by `?` (§14.3).
    From,
    /// User-defined tagged literals (§17.8).
    Template,
}

impl BuiltinTrait {
    pub fn name(self) -> &'static str {
        match self {
            BuiltinTrait::Eq => "Eq",
            BuiltinTrait::Hash => "Hash",
            BuiltinTrait::Ord => "Ord",
            BuiltinTrait::Show => "Show",
            BuiltinTrait::Encode => "Encode",
            BuiltinTrait::Decode => "Decode",
            BuiltinTrait::Row => "Row",
            BuiltinTrait::Message => "Message",
            BuiltinTrait::Error => "Error",
            BuiltinTrait::From => "From",
            BuiltinTrait::Template => "Template",
        }
    }

    /// Derived automatically when every component implements it (§9.7 "Derivation").
    pub fn is_auto(self) -> bool {
        matches!(
            self,
            BuiltinTrait::Eq | BuiltinTrait::Hash | BuiltinTrait::Show
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TraitId {
    Builtin(BuiltinTrait),
    User(DefId),
}

/// A trait applied to its arguments: `Show`, `From[DbError]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraitRef {
    pub id: TraitId,
    pub args: Vec<Ty>,
}

impl TraitRef {
    pub fn builtin(tr: BuiltinTrait) -> TraitRef {
        TraitRef {
            id: TraitId::Builtin(tr),
            args: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitDecl {
    pub id: TraitId,
    /// `None` for built-in traits (defined by the core).
    pub module: Option<ModulePath>,
    pub generics: Vec<GenericParam>,
    /// Method name → method definition (its signature is in `TypeckResults::signatures`).
    pub methods: Vec<(String, DefId)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImplDecl {
    pub def: DefId,
    pub module: ModulePath,
    pub generics: Vec<GenericParam>,
    /// The self type, over `Ty::Param(i)` for the impl's generics.
    pub self_ty: Ty,
    /// `None` for an inherent impl (`impl T:`).
    pub trait_ref: Option<TraitRef>,
    pub methods: Vec<(String, DefId)>,
    pub span: FileSpan,
    /// Written by the stdlib, the one privileged package (stdlib/README.md "Where methods
    /// live"): it implements built-in traits for built-in types, so the orphan rule does
    /// not apply to it.
    pub privileged: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Yes,
    No,
    /// The type still contains an inference variable.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoherenceError {
    /// The impl lives in neither the trait's module nor the self type's module.
    Orphan { impl_def: DefId },
    /// Two impls of one trait whose heads unify.
    Overlap { first: DefId, second: DefId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodCandidate {
    /// The impl that writes the method; for a trait method that applies by derivation or
    /// by a generic bound, the trait's own declaration.
    pub impl_def: DefId,
    pub method: DefId,
    /// `None` for an inherent method.
    pub trait_id: Option<TraitId>,
}

#[derive(Clone, Debug, Default)]
pub struct TraitEnv {
    pub traits: HashMap<TraitId, TraitDecl>,
    pub impls: Vec<ImplDecl>,
}

/// How deep trait solving follows components and impl bounds before assuming the goal.
const MAX_SOLVE_DEPTH: usize = 64;

/// `ty` with every inference variable replaced by `Ty::Error` (which unifies with
/// anything), for matching against impl heads in a fresh inference context.
pub fn erase_vars(ty: &Ty) -> Ty {
    ty.map(&mut |t| matches!(t, Ty::Var(_)).then_some(Ty::Error))
}

/// The module that defines the head of `ty`, or `None` for a built-in type.
pub fn head_module<'a>(ty: &Ty, adts: &'a HashMap<DefId, AdtDef>) -> Option<&'a ModulePath> {
    match ty {
        Ty::Adt(d, _) => adts.get(d).map(|a| &a.module),
        _ => None,
    }
}

/// The "never implemented by" column of spec §9.7, for built-in types. User types are
/// answered by derives and impls; component types are checked recursively by the caller.
pub fn never_implements(ty: &Ty, tr: BuiltinTrait) -> bool {
    use BuiltinTrait as T;
    match ty {
        Ty::Float => matches!(tr, T::Eq | T::Hash | T::Ord),
        Ty::Fn { .. } => matches!(tr, T::Eq | T::Hash | T::Show | T::Encode | T::Decode),
        Ty::Capability(_) => matches!(tr, T::Eq | T::Hash | T::Encode | T::Decode | T::Row),
        Ty::Secret(_) => matches!(tr, T::Eq | T::Hash | T::Ord | T::Encode),
        Ty::Sink(SinkType::Sql | SinkType::Html | SinkType::Command | SinkType::Path) => {
            matches!(tr, T::Decode | T::Row)
        }
        _ => false,
    }
}

/// What a built-in type needs in order to implement a built-in trait: `None` if it never
/// does, otherwise the component types that must implement it too.
fn builtin_requirement(ty: &Ty, tr: BuiltinTrait) -> Option<Vec<Ty>> {
    use BuiltinTrait as T;
    if never_implements(ty, tr) {
        return None;
    }
    let data = matches!(
        tr,
        T::Eq | T::Hash | T::Ord | T::Show | T::Encode | T::Decode
    );
    let children = || ty.children().into_iter().cloned().collect::<Vec<_>>();
    match ty {
        Ty::Never => Some(Vec::new()),
        Ty::Int | Ty::BigInt | Ty::Decimal | Ty::Bool | Ty::String | Ty::Bytes => {
            data.then(Vec::new)
        }
        Ty::Float => matches!(tr, T::Show | T::Encode | T::Decode).then(Vec::new),
        Ty::Unit => matches!(tr, T::Eq | T::Hash | T::Ord | T::Show | T::Encode).then(Vec::new),
        Ty::List(_) | Ty::Option(_) | Ty::Tuple(_) => data.then(children),
        Ty::Set(_) | Ty::Map(_, _) | Ty::Result(_, _) => (data && tr != T::Ord).then(children),
        Ty::Range(_) => matches!(tr, T::Eq | T::Show).then(Vec::new),
        Ty::Capability(_) => (tr == T::Show).then(Vec::new),
        Ty::Sink(s) => match tr {
            T::Show => Some(Vec::new()),
            T::Decode => matches!(s, SinkType::Url | SinkType::PathSegment).then(Vec::new),
            _ => None,
        },
        // `Show` prints `[REDACTED]`; `Secret[T]: Decode` when `T: Decode` (§9.7).
        Ty::Secret(inner) => match tr {
            T::Show => Some(Vec::new()),
            T::Decode => Some(vec![(**inner).clone()]),
            _ => None,
        },
        Ty::AnyError => matches!(tr, T::Show | T::Error).then(Vec::new),
        _ => None,
    }
}

impl TraitEnv {
    pub fn trait_module(&self, id: TraitId) -> Option<&ModulePath> {
        self.traits.get(&id).and_then(|t| t.module.as_ref())
    }

    /// The orphan rule and coherence for every trait impl.
    pub fn check_coherence(&self, adts: &HashMap<DefId, AdtDef>) -> Vec<CoherenceError> {
        let mut errors = Vec::new();
        for imp in &self.impls {
            let Some(tr) = &imp.trait_ref else { continue };
            if imp.privileged {
                continue;
            }
            let trait_mod = self.trait_module(tr.id);
            let type_mod = head_module(&imp.self_ty, adts);
            if trait_mod != Some(&imp.module) && type_mod != Some(&imp.module) {
                errors.push(CoherenceError::Orphan { impl_def: imp.def });
            }
        }
        let trait_impls: Vec<&ImplDecl> = self
            .impls
            .iter()
            .filter(|i| i.trait_ref.is_some())
            .collect();
        for (i, a) in trait_impls.iter().enumerate() {
            for b in &trait_impls[i + 1..] {
                if self.heads_overlap(a, b) {
                    errors.push(CoherenceError::Overlap {
                        first: a.def,
                        second: b.def,
                    });
                }
            }
        }
        errors
    }

    fn heads_overlap(&self, a: &ImplDecl, b: &ImplDecl) -> bool {
        let (Some(ta), Some(tb)) = (&a.trait_ref, &b.trait_ref) else {
            return false;
        };
        if ta.id != tb.id || ta.args.len() != tb.args.len() {
            return false;
        }
        let mut cx = InferCtx::new();
        let fa: Vec<Ty> = a.generics.iter().map(|_| cx.fresh()).collect();
        let fb: Vec<Ty> = b.generics.iter().map(|_| cx.fresh()).collect();
        let head = |imp: &ImplDecl, tr: &TraitRef, f: &[Ty]| {
            let mut tys = vec![imp.self_ty.subst(f)];
            tys.extend(tr.args.iter().map(|t| t.subst(f)));
            Ty::Tuple(tys)
        };
        cx.unify(&head(a, ta, &fa), &head(b, tb, &fb)).is_ok()
    }

    /// Does `ty` implement `tr`? `generics` are the generic parameters in scope (their
    /// bounds are assumptions). `ty` must already be resolved.
    pub fn implements(
        &self,
        adts: &HashMap<DefId, AdtDef>,
        generics: &[GenericParam],
        ty: &Ty,
        tr: &TraitRef,
    ) -> Answer {
        let mut assumed = Vec::new();
        self.solve(adts, generics, ty, tr, &mut assumed)
    }

    fn solve(
        &self,
        adts: &HashMap<DefId, AdtDef>,
        generics: &[GenericParam],
        ty: &Ty,
        tr: &TraitRef,
        assumed: &mut Vec<(Ty, TraitRef)>,
    ) -> Answer {
        // An inference variable anywhere in the goal makes the answer depend on it: the
        // checker asks again once the body is solved. (Impl heads are matched in a fresh
        // inference context, which must never see an outer variable.)
        if ty.has_vars() || tr.args.iter().any(Ty::has_vars) {
            return Answer::Unknown;
        }
        match ty {
            Ty::Error => return Answer::Yes,
            Ty::Var(_) => return Answer::Unknown,
            Ty::Param(i) => {
                let held = generics
                    .get(*i as usize)
                    .is_some_and(|g| g.bounds.contains(tr));
                return if held { Answer::Yes } else { Answer::No };
            }
            _ => {}
        }
        // Coinduction: a recursive type (`enum Tree: Node(left: Tree, …)`) assumes the goal.
        // A non-regular one (`Node(next: Tree[List[T]])`) never repeats a goal exactly, so
        // past a fixed depth the goal is assumed too (no unbounded recursion: typeck is a
        // fuzz target).
        if assumed.len() > MAX_SOLVE_DEPTH || assumed.iter().any(|(t, r)| t == ty && r == tr) {
            return Answer::Yes;
        }
        assumed.push((ty.clone(), tr.clone()));
        let answer = self.solve_uncached(adts, generics, ty, tr, assumed);
        assumed.pop();
        answer
    }

    fn all(
        &self,
        adts: &HashMap<DefId, AdtDef>,
        generics: &[GenericParam],
        tys: &[Ty],
        tr: &TraitRef,
        assumed: &mut Vec<(Ty, TraitRef)>,
    ) -> Answer {
        let mut answer = Answer::Yes;
        for t in tys {
            match self.solve(adts, generics, t, tr, assumed) {
                Answer::No => return Answer::No,
                Answer::Unknown => answer = Answer::Unknown,
                Answer::Yes => {}
            }
        }
        answer
    }

    fn solve_uncached(
        &self,
        adts: &HashMap<DefId, AdtDef>,
        generics: &[GenericParam],
        ty: &Ty,
        tr: &TraitRef,
        assumed: &mut Vec<(Ty, TraitRef)>,
    ) -> Answer {
        // A user impl answers first (a custom `Show` overrides the derived one).
        if let Some(answer) = self.solve_by_impl(adts, generics, ty, tr, assumed) {
            return answer;
        }
        let TraitId::Builtin(b) = tr.id else {
            return Answer::No;
        };
        if let Ty::Adt(def, args) = ty {
            let Some(adt) = adts.get(def) else {
                return Answer::No;
            };
            if !(b.is_auto() || adt.derives.contains(&b)) {
                return Answer::No;
            }
            let comps = adt.component_types(args);
            // `Row` decodes each field from one column, and `Message` travels as data
            // (§9.7, §17.3, §20.3): their fields need `Decode`, not the derived trait.
            let field_trait = match b {
                BuiltinTrait::Row | BuiltinTrait::Message => {
                    TraitRef::builtin(BuiltinTrait::Decode)
                }
                _ => tr.clone(),
            };
            return self.all(adts, generics, &comps, &field_trait, assumed);
        }
        match builtin_requirement(ty, b) {
            None => Answer::No,
            Some(comps) => self.all(adts, generics, &comps, tr, assumed),
        }
    }

    /// `Some(answer)` when an impl's head matches `ty` (then its bounds decide).
    fn solve_by_impl(
        &self,
        adts: &HashMap<DefId, AdtDef>,
        generics: &[GenericParam],
        ty: &Ty,
        tr: &TraitRef,
        assumed: &mut Vec<(Ty, TraitRef)>,
    ) -> Option<Answer> {
        for imp in &self.impls {
            let Some(itr) = &imp.trait_ref else { continue };
            if itr.id != tr.id || itr.args.len() != tr.args.len() {
                continue;
            }
            let mut cx = InferCtx::new();
            let fresh: Vec<Ty> = imp.generics.iter().map(|_| cx.fresh()).collect();
            let mut head = vec![imp.self_ty.subst(&fresh)];
            head.extend(itr.args.iter().map(|t| t.subst(&fresh)));
            let mut goal = vec![ty.clone()];
            goal.extend(tr.args.iter().cloned());
            if cx.unify(&Ty::Tuple(goal), &Ty::Tuple(head)).is_err() {
                continue;
            }
            let mut answer = Answer::Yes;
            for (g, arg) in imp.generics.iter().zip(&fresh) {
                let arg = cx.resolve(arg);
                for bound in &g.bounds {
                    let bound = TraitRef {
                        id: bound.id,
                        args: bound
                            .args
                            .iter()
                            .map(|t| cx.resolve(&t.subst(&fresh)))
                            .collect(),
                    };
                    match self.solve(adts, generics, &arg, &bound, assumed) {
                        Answer::No => return Some(Answer::No),
                        Answer::Unknown => answer = Answer::Unknown,
                        Answer::Yes => {}
                    }
                }
            }
            return Some(answer);
        }
        None
    }

    /// True when `imp`'s self type can be `ty` (bounds ignored). Inference variables of
    /// the caller's context are erased first, since they could be anything.
    pub fn impl_matches(imp: &ImplDecl, ty: &Ty) -> bool {
        let ty = erase_vars(ty);
        let mut cx = InferCtx::new();
        let fresh: Vec<Ty> = imp.generics.iter().map(|_| cx.fresh()).collect();
        cx.unify(&ty, &imp.self_ty.subst(&fresh)).is_ok()
    }

    /// Methods named `name` of impls whose self type can be `ty` (DR-27's impl part: the
    /// inherent methods, and the trait methods some impl writes out). The checker adds the
    /// methods of traits `ty` implements by derivation or by a bound, and requires exactly
    /// one candidate.
    pub fn method_candidates(&self, ty: &Ty, name: &str) -> Vec<MethodCandidate> {
        let mut out = Vec::new();
        for imp in &self.impls {
            let Some((_, method)) = imp.methods.iter().find(|(n, _)| n == name) else {
                continue;
            };
            if Self::impl_matches(imp, ty) {
                out.push(MethodCandidate {
                    impl_def: imp.def,
                    method: *method,
                    trait_id: imp.trait_ref.as_ref().map(|t| t.id),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adt::{AdtKind, FieldDef};
    use crate::ty::Capability;

    fn module(p: &str) -> ModulePath {
        ModulePath(p.to_string())
    }

    fn record(def: u32, m: &str, fields: Vec<Ty>) -> AdtDef {
        AdtDef {
            def: DefId(def),
            name: format!("R{def}"),
            module: module(m),
            generics: Vec::new(),
            kind: AdtKind::Record(
                fields
                    .into_iter()
                    .enumerate()
                    .map(|(i, ty)| FieldDef {
                        name: format!("f{i}"),
                        ty,
                        has_default: false,
                    })
                    .collect(),
            ),
            opaque: false,
            derives: Vec::new(),
        }
    }

    fn show_impl(def: u32, m: &str, self_ty: Ty) -> ImplDecl {
        ImplDecl {
            def: DefId(def),
            module: module(m),
            generics: Vec::new(),
            self_ty,
            trait_ref: Some(TraitRef::builtin(BuiltinTrait::Show)),
            methods: vec![("show".into(), DefId(def + 1000))],
            span: FileSpan::default(),
            privileged: false,
        }
    }

    #[test]
    fn spec_trait_exclusions() {
        assert!(never_implements(&Ty::Float, BuiltinTrait::Eq));
        assert!(!never_implements(&Ty::Decimal, BuiltinTrait::Eq));
        assert!(never_implements(
            &Ty::Secret(Box::new(Ty::String)),
            BuiltinTrait::Encode
        ));
        assert!(!never_implements(
            &Ty::Secret(Box::new(Ty::String)),
            BuiltinTrait::Show
        ));
        assert!(never_implements(
            &Ty::Capability(Capability::Net),
            BuiltinTrait::Decode
        ));
        assert!(never_implements(
            &Ty::Sink(SinkType::Sql),
            BuiltinTrait::Decode
        ));
        assert!(!never_implements(
            &Ty::Sink(SinkType::Url),
            BuiltinTrait::Decode
        ));
    }

    #[test]
    fn structural_eq_and_float_special_case() {
        let env = TraitEnv::default();
        let mut adts = HashMap::new();
        adts.insert(DefId(1), record(1, "app/a", vec![Ty::Int, Ty::String]));
        adts.insert(DefId(2), record(2, "app/a", vec![Ty::Int, Ty::Float]));
        let eq = TraitRef::builtin(BuiltinTrait::Eq);
        let ord = TraitRef::builtin(BuiltinTrait::Ord);
        let yes = |t: Ty, r: &TraitRef| env.implements(&adts, &[], &t, r);
        assert_eq!(yes(Ty::Adt(DefId(1), vec![]), &eq), Answer::Yes);
        assert_eq!(
            yes(Ty::Adt(DefId(2), vec![]), &eq),
            Answer::No,
            "Float field"
        );
        assert_eq!(yes(Ty::list(Ty::Float), &eq), Answer::No);
        assert_eq!(yes(Ty::list(Ty::Int), &eq), Answer::Yes);
        assert_eq!(
            yes(Ty::Adt(DefId(1), vec![]), &ord),
            Answer::No,
            "Ord is explicit"
        );
        assert_eq!(
            yes(Ty::Secret(Box::new(Ty::String)), &eq),
            Answer::No,
            "Secret is never Eq"
        );
        assert_eq!(
            yes(
                Ty::Secret(Box::new(Ty::String)),
                &TraitRef::builtin(BuiltinTrait::Decode)
            ),
            Answer::Yes
        );
        assert_eq!(
            yes(
                Ty::Capability(Capability::World),
                &TraitRef::builtin(BuiltinTrait::Show)
            ),
            Answer::Yes
        );
        assert_eq!(yes(Ty::Var(0), &eq), Answer::Unknown);
    }

    #[test]
    fn recursive_types_are_coinductive() {
        let env = TraitEnv::default();
        let mut adts = HashMap::new();
        // type Node: value: Int, next: Option[Node]
        adts.insert(
            DefId(1),
            record(
                1,
                "app/a",
                vec![Ty::Int, Ty::option(Ty::Adt(DefId(1), vec![]))],
            ),
        );
        let eq = TraitRef::builtin(BuiltinTrait::Eq);
        assert_eq!(
            env.implements(&adts, &[], &Ty::Adt(DefId(1), vec![]), &eq),
            Answer::Yes
        );
    }

    #[test]
    fn generic_bounds_are_assumptions() {
        let env = TraitEnv::default();
        let adts = HashMap::new();
        let eq = TraitRef::builtin(BuiltinTrait::Eq);
        let generics = vec![GenericParam {
            name: "T".into(),
            bounds: vec![eq.clone()],
        }];
        assert_eq!(
            env.implements(&adts, &generics, &Ty::list(Ty::Param(0)), &eq),
            Answer::Yes
        );
        assert_eq!(
            env.implements(
                &adts,
                &generics,
                &Ty::Param(0),
                &TraitRef::builtin(BuiltinTrait::Hash)
            ),
            Answer::No
        );
    }

    #[test]
    fn orphan_rule_and_overlap() {
        let mut adts = HashMap::new();
        adts.insert(DefId(1), record(1, "app/money", vec![Ty::Decimal]));
        let money = Ty::Adt(DefId(1), vec![]);
        let mut env = TraitEnv::default();
        env.impls.push(show_impl(10, "app/money", money.clone())); // fine: T's module
        env.impls.push(show_impl(11, "app/other", money.clone())); // orphan + overlap
        env.impls.push(show_impl(12, "app/other", Ty::Int)); // orphan: core trait, core type
        let errors = env.check_coherence(&adts);
        assert!(errors.contains(&CoherenceError::Orphan {
            impl_def: DefId(11)
        }));
        assert!(errors.contains(&CoherenceError::Orphan {
            impl_def: DefId(12)
        }));
        assert!(errors.contains(&CoherenceError::Overlap {
            first: DefId(10),
            second: DefId(11)
        }));
        assert!(!errors.contains(&CoherenceError::Orphan {
            impl_def: DefId(10)
        }));
    }

    #[test]
    fn user_trait_impl_in_trait_module_is_not_orphan_and_generic_impls_overlap() {
        let adts = HashMap::new();
        let tr = TraitId::User(DefId(50));
        let mut env = TraitEnv::default();
        env.traits.insert(
            tr,
            TraitDecl {
                id: tr,
                module: Some(module("lib/pretty")),
                generics: vec![],
                methods: vec![("pretty".into(), DefId(51))],
            },
        );
        let any_list = ImplDecl {
            def: DefId(20),
            module: module("lib/pretty"),
            generics: vec![GenericParam {
                name: "T".into(),
                bounds: vec![],
            }],
            self_ty: Ty::list(Ty::Param(0)),
            trait_ref: Some(TraitRef {
                id: tr,
                args: vec![],
            }),
            methods: vec![("pretty".into(), DefId(21))],
            span: FileSpan::default(),
            privileged: false,
        };
        let int_list = ImplDecl {
            def: DefId(22),
            self_ty: Ty::list(Ty::Int),
            ..any_list.clone()
        };
        env.impls.push(any_list);
        let errors = env.check_coherence(&adts);
        assert!(errors.is_empty(), "{errors:?}");
        env.impls.push(int_list);
        assert_eq!(
            env.check_coherence(&adts),
            vec![CoherenceError::Overlap {
                first: DefId(20),
                second: DefId(22)
            }]
        );
        let user = TraitRef {
            id: tr,
            args: vec![],
        };
        assert_eq!(
            env.implements(&adts, &[], &Ty::list(Ty::String), &user),
            Answer::Yes
        );
        assert_eq!(env.implements(&adts, &[], &Ty::Int, &user), Answer::No);
        assert_eq!(env.method_candidates(&Ty::list(Ty::Int), "pretty").len(), 2);
    }
}
