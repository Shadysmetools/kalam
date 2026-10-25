//! Well-formed types (spec §9.3, §9.6): a `Map` key and a `Set` element are `Eq + Hash`,
//! and the type arguments of a generic record or enum satisfy its parameters' bounds.
//!
//! A type is checked where it is formed: every written type (signatures, fields, variant
//! payloads, aliases, impl heads, local annotations) and every value that builds one (map
//! literals, record and variant construction, which the checker requires through its
//! obligations). A failure is K0603 at the most specific written type, with a label that
//! says which rule needs the trait.
//!
//! Recovery, so one root cause is reported once: a failing generic parameter of the
//! declaration being checked is given the missing bound (the declaration is already
//! rejected), and a failing concrete type is replaced by the poison `Ty::Error` in the
//! lowered type, so uses of the value (`m.len()` on a `Map[Float, Int]`) stay quiet.

use std::collections::HashMap;

use kalam_base::Span;
use kalam_resolve::DefId;

use crate::adt::AdtDef;
use crate::hir::{self, BuiltinType, HirProgram, ItemKind, TypeKind};
use crate::traits::{Answer, BuiltinTrait, TraitRef};
use crate::ty::Ty;

/// Which rule a requirement comes from (for the label).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    MapKey,
    SetElement,
    /// The bound of a generic parameter of a record, enum or alias: `(type, parameter)`.
    Param {
        owner: String,
        param: String,
    },
}

/// One unsatisfied requirement of a written type.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    /// The written type that needs the trait (`Float` in `Map[Float, Int]`).
    pub span: Span,
    pub ty: Ty,
    pub tr: TraitRef,
    pub why: Why,
}

impl Failure {
    /// The label text, for example "a `Map` key must be `Eq + Hash`".
    pub fn label(&self, trait_name: &str) -> String {
        match &self.why {
            Why::MapKey => "a `Map` key must be `Eq` and `Hash`".to_string(),
            Why::SetElement => "a `Set` element must be `Eq` and `Hash`".to_string(),
            Why::Param { owner, param } => {
                format!("`{owner}` requires its `{param}` to be `{trait_name}`")
            }
        }
    }
}

/// `Eq` and `Hash`, the requirements of a `Map` key or `Set` element.
pub fn key_traits() -> [TraitRef; 2] {
    [
        TraitRef::builtin(BuiltinTrait::Eq),
        TraitRef::builtin(BuiltinTrait::Hash),
    ]
}

/// The requirements of a value of type `ty` built right here: `Map` keys and `Set`
/// elements anywhere inside it, and the bounds of the generic records and enums in it.
pub fn requirements(ty: &Ty, adts: &HashMap<DefId, AdtDef>, out: &mut Vec<(Ty, TraitRef)>) {
    requirements_depth(ty, adts, out, 0);
}

fn requirements_depth(
    ty: &Ty,
    adts: &HashMap<DefId, AdtDef>,
    out: &mut Vec<(Ty, TraitRef)>,
    depth: u32,
) {
    if depth > 256 {
        return;
    }
    match ty {
        Ty::Map(k, _) | Ty::Set(k) => {
            for tr in key_traits() {
                out.push(((**k).clone(), tr));
            }
        }
        Ty::Adt(d, args) => {
            if let Some(adt) = adts.get(d) {
                for (g, arg) in adt.generics.iter().zip(args) {
                    for b in &g.bounds {
                        out.push((arg.clone(), subst_ref(b, args)));
                    }
                }
            }
        }
        _ => {}
    }
    for c in ty.children() {
        requirements_depth(c, adts, out, depth + 1);
    }
}

fn subst_ref(b: &TraitRef, args: &[Ty]) -> TraitRef {
    TraitRef {
        id: b.id,
        args: b.args.iter().map(|t| t.subst(args)).collect(),
    }
}

/// Checks written types against their lowered forms. `answer` decides whether a type
/// implements a trait (with the bounds of the generic parameters in scope).
pub struct Written<'a> {
    pub prog: &'a HirProgram,
    pub adts: &'a HashMap<DefId, AdtDef>,
    pub answer: &'a dyn Fn(&Ty, &TraitRef) -> Answer,
}

impl Written<'_> {
    /// Returns the lowered type `ty` of the written `t` with every failing concrete
    /// component replaced by `Ty::Error`, and adds each failure to `out`.
    pub fn check(&self, t: &hir::Type, ty: &Ty, out: &mut Vec<Failure>) -> Ty {
        self.walk(t, ty, out, 0)
    }

    /// Records a failure when `ty` (written at `span`) does not implement `tr`. True when
    /// it does, or may: an answer that depends on inference is the checker's.
    fn need(&self, span: Span, ty: &Ty, tr: TraitRef, why: Why, out: &mut Vec<Failure>) -> bool {
        if (self.answer)(ty, &tr) != Answer::No {
            return true;
        }
        out.push(Failure {
            span,
            ty: ty.clone(),
            tr,
            why,
        });
        false
    }

    fn key(&self, t: &hir::Type, ty: &Ty, why: Why, out: &mut Vec<Failure>, depth: u32) -> Ty {
        let inner = self.walk(t, ty, out, depth + 1);
        // Both are recorded (a generic parameter is given every bound it lacks); the
        // caller reports one error per written type.
        let mut ok = true;
        for tr in key_traits() {
            ok &= self.need(t.span, ty, tr, why.clone(), out);
        }
        if ok || matches!(ty, Ty::Param(_)) {
            inner
        } else {
            Ty::Error
        }
    }

    fn walk(&self, t: &hir::Type, ty: &Ty, out: &mut Vec<Failure>, depth: u32) -> Ty {
        if depth > 256 {
            return ty.clone();
        }
        match (&t.kind, ty) {
            (TypeKind::Builtin(BuiltinType::Map, hs), Ty::Map(k, v)) if hs.len() == 2 => {
                let k2 = self.key(&hs[0], k, Why::MapKey, out, depth);
                let v2 = self.walk(&hs[1], v, out, depth + 1);
                Ty::Map(Box::new(k2), Box::new(v2))
            }
            (TypeKind::Builtin(BuiltinType::Set, hs), Ty::Set(e)) if hs.len() == 1 => {
                let e2 = self.key(&hs[0], e, Why::SetElement, out, depth);
                Ty::Set(Box::new(e2))
            }
            (TypeKind::Builtin(_, hs), _) => {
                let kids: Vec<Ty> = ty.children().into_iter().cloned().collect();
                if kids.len() != hs.len() {
                    return ty.clone();
                }
                let new: Vec<Ty> = hs
                    .iter()
                    .zip(&kids)
                    .map(|(h, k)| self.walk(h, k, out, depth + 1))
                    .collect();
                rebuild(ty, new)
            }
            (TypeKind::Tuple(hs), Ty::Tuple(ts)) if hs.len() == ts.len() => Ty::Tuple(
                hs.iter()
                    .zip(ts)
                    .map(|(h, k)| self.walk(h, k, out, depth + 1))
                    .collect(),
            ),
            (
                TypeKind::Fn {
                    params: hp,
                    ret: hr,
                    ..
                },
                Ty::Fn {
                    params,
                    ret,
                    suspension,
                },
            ) if hp.len() == params.len() => Ty::Fn {
                params: hp
                    .iter()
                    .zip(params)
                    .map(|(h, k)| self.walk(h, k, out, depth + 1))
                    .collect(),
                ret: Box::new(self.walk(hr, ret, out, depth + 1)),
                suspension: *suspension,
            },
            (TypeKind::Def(d, hs), Ty::Adt(d2, args)) if d == d2 && hs.len() == args.len() => {
                let Some(adt) = self.adts.get(d) else {
                    return ty.clone();
                };
                let mut new = Vec::new();
                for (i, (h, arg)) in hs.iter().zip(args).enumerate() {
                    let mut a = self.walk(h, arg, out, depth + 1);
                    if let Some(g) = adt.generics.get(i) {
                        let why = Why::Param {
                            owner: adt.name.clone(),
                            param: g.name.clone(),
                        };
                        let mut ok = true;
                        for b in &g.bounds {
                            ok &= self.need(h.span, arg, subst_ref(b, args), why.clone(), out);
                        }
                        if !ok && !matches!(arg, Ty::Param(_)) {
                            a = Ty::Error;
                        }
                    }
                    new.push(a);
                }
                Ty::Adt(*d, new)
            }
            // An alias use: its arguments against the alias's own declared bounds (the
            // expansion was checked at the alias declaration).
            (TypeKind::Def(d, hs), _) => {
                let Some(ItemKind::Alias(a)) = self.prog.item(*d).map(|i| &i.kind) else {
                    return ty.clone();
                };
                if hs.is_empty() || a.generics.iter().all(|g| g.bounds.is_empty()) {
                    return ty.clone();
                }
                let name = self
                    .prog
                    .item(*d)
                    .map(|i| i.name.clone())
                    .unwrap_or_default();
                let mut failed = false;
                for (h, g) in hs.iter().zip(&a.generics) {
                    for b in &g.bounds {
                        // Alias bounds name traits only (their arguments are rare); the
                        // argument is checked as written, against the trait alone.
                        let tr = TraitRef {
                            id: b.id,
                            args: Vec::new(),
                        };
                        let why = Why::Param {
                            owner: name.clone(),
                            param: g.name.clone(),
                        };
                        let arg = self.lowered_arg(h);
                        if let Some(arg) = arg {
                            if !self.need(h.span, &arg, tr, why, out) {
                                failed = true;
                            }
                        }
                    }
                }
                if failed {
                    Ty::Error
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// The type a written alias argument denotes, when it is simple enough to read
    /// without the checker's lowering (a built-in or a non-generic record or enum).
    fn lowered_arg(&self, h: &hir::Type) -> Option<Ty> {
        Some(match &h.kind {
            TypeKind::Builtin(b, args) if args.is_empty() => match b {
                BuiltinType::Int => Ty::Int,
                BuiltinType::BigInt => Ty::BigInt,
                BuiltinType::Float => Ty::Float,
                BuiltinType::Decimal => Ty::Decimal,
                BuiltinType::Bool => Ty::Bool,
                BuiltinType::String => Ty::String,
                BuiltinType::Bytes => Ty::Bytes,
                _ => return None,
            },
            TypeKind::Def(d, args) if args.is_empty() && self.adts.contains_key(d) => {
                Ty::Adt(*d, Vec::new())
            }
            TypeKind::Param(i) => Ty::Param(*i),
            _ => return None,
        })
    }
}

/// `ty` with its direct children replaced by `new` (same count and order as
/// [`Ty::children`]).
fn rebuild(ty: &Ty, new: Vec<Ty>) -> Ty {
    let mut it = new.into_iter();
    let mut next = || Box::new(it.next().unwrap_or(Ty::Error));
    match ty {
        Ty::List(_) => Ty::List(next()),
        Ty::Set(_) => Ty::Set(next()),
        Ty::Option(_) => Ty::Option(next()),
        Ty::Range(_) => Ty::Range(next()),
        Ty::Secret(_) => Ty::Secret(next()),
        Ty::Map(_, _) => {
            let k = next();
            Ty::Map(k, next())
        }
        Ty::Result(_, _) => {
            let t = next();
            Ty::Result(t, next())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adt::{AdtKind, FieldDef};
    use crate::ty::GenericParam;
    use kalam_resolve::ModulePath;

    #[test]
    fn map_keys_set_elements_and_adt_bounds_are_requirements() {
        let mut adts = HashMap::new();
        adts.insert(
            DefId(1),
            AdtDef {
                def: DefId(1),
                name: "Index".into(),
                module: ModulePath("app/a".into()),
                generics: vec![GenericParam {
                    name: "K".into(),
                    bounds: vec![TraitRef::builtin(BuiltinTrait::Hash)],
                }],
                kind: AdtKind::Record(vec![FieldDef {
                    name: "k".into(),
                    ty: Ty::Param(0),
                    has_default: false,
                }]),
                opaque: false,
                derives: Vec::new(),
                assumed: Vec::new(),
            },
        );
        let mut out = Vec::new();
        let ty = Ty::list(Ty::Map(
            Box::new(Ty::Float),
            Box::new(Ty::Adt(DefId(1), vec![Ty::String])),
        ));
        requirements(&ty, &adts, &mut out);
        assert!(out.contains(&(Ty::Float, TraitRef::builtin(BuiltinTrait::Eq))));
        assert!(out.contains(&(Ty::Float, TraitRef::builtin(BuiltinTrait::Hash))));
        assert!(out.contains(&(Ty::String, TraitRef::builtin(BuiltinTrait::Hash))));
        let mut out = Vec::new();
        requirements(&Ty::Set(Box::new(Ty::Int)), &adts, &mut out);
        assert_eq!(out.len(), 2);
    }
}
