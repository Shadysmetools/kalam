//! Impl conformance (spec §9.7, FOUNDER-QUEUE Q27, K1107): an `impl T as Trait` writes
//! exactly the trait's methods, each with the trait's signature after substitution.
//!
//! Generic code calls the trait's declaration (`x.name()` with `T: Named`, `F.from(e)` behind
//! `?`), and concrete code calls the impl's method. When the two differ, a value of the
//! wrong type crosses that boundary unchecked, so every difference is K1107:
//!
//! - a trait method the impl does not write, or an impl method the trait does not declare;
//! - `self` in one and not the other;
//! - a parameter count, a parameter type or the return type that differs once the trait's
//!   type parameters are its arguments in the impl (`From[E1]`: `E` is `E1`), `Self` is the
//!   impl's type and the method's own type parameters are paired in order;
//! - a different number of method type parameters, or a bound the impl adds (the impl may
//!   require less than the trait, never more).
//!
//! The type arguments of the trait reference must match the trait's parameters in number.
//!
//! **Recovery.** A mismatching method keeps its written signature for checking its own
//! body, but every caller sees the trait's signature (the contract), so the root cause is
//! reported once, at the impl, and not again at each call.
//!
//! **Fewer bounds (FOUNDER-QUEUE Q41).** A method that requires less of its type parameters
//! than the trait is no K1107, and its body is checked against what it wrote; but every
//! caller, a concrete receiver included, sees the trait's bounds, so `First(…).pick(xs)`
//! and a call through `P: Picker` accept the same arguments.

use kalam_base::Span;
use kalam_resolve::DefId;

use crate::check::TypeckResults;
use crate::error::{TypeError, TypeErrorKind};
use crate::hir::{HirProgram, ItemKind};
use crate::traits::{ImplDecl, TraitDecl, TraitId, TraitRef};
use crate::ty::{GenericParam, Scheme, Ty};
use crate::unify::InferCtx;

/// What the conformance check found.
#[derive(Debug, Default)]
pub struct Outcome {
    pub errors: Vec<TypeError>,
    /// Impl methods whose written signature differs from the trait's, with the trait's
    /// signature for that impl: the scheme callers must see.
    pub contracts: Vec<(DefId, Scheme)>,
    /// For each of `contracts`, whether the trait's method takes `self`: callers call it the
    /// trait's way (`x.m()` or `T.m()`).
    pub selfs: Vec<(DefId, bool)>,
}

/// Checks every trait impl of `res.traits` against its trait.
pub fn check(prog: &HirProgram, res: &TypeckResults) -> Outcome {
    let mut out = Outcome::default();
    for imp in &res.traits.impls {
        let Some(tr) = &imp.trait_ref else { continue };
        let mut cx = ImplCx {
            prog,
            res,
            imp,
            tr,
            out: &mut out,
        };
        cx.check();
    }
    out
}

struct ImplCx<'a> {
    prog: &'a HirProgram,
    res: &'a TypeckResults,
    imp: &'a ImplDecl,
    tr: &'a TraitRef,
    out: &'a mut Outcome,
}

impl ImplCx<'_> {
    fn names(&self, generics: &[GenericParam]) -> Vec<String> {
        generics.iter().map(|g| g.name.clone()).collect()
    }

    fn show(&self, t: &Ty, params: &[String]) -> String {
        t.display_with(&|d| self.res.name_of(d), params)
    }

    /// The trait as written in the impl head: `Named`, `From[E1]`.
    fn trait_text(&self) -> String {
        let name = match self.tr.id {
            TraitId::Builtin(b) => b.name().to_string(),
            TraitId::User(d) => self.res.name_of(d),
        };
        if self.tr.args.is_empty() {
            return name;
        }
        let params = self.names(&self.imp.generics);
        let args: Vec<String> = self.tr.args.iter().map(|t| self.show(t, &params)).collect();
        format!("{name}[{}]", args.join(", "))
    }

    fn report(&mut self, span: Span, detail: String) {
        let kind = TypeErrorKind::ImplMismatch {
            trait_name: self.trait_text(),
            detail,
        };
        self.out
            .errors
            .push(TypeError::new(kind, self.imp.span.file, span));
    }

    /// The span of an impl method's name.
    fn method_span(&self, m: DefId) -> Span {
        self.prog.item(m).map_or(self.imp.span.span, |i| i.span)
    }

    fn check(&mut self) {
        let head = self.imp.span.span;
        let Some(decl) = self.res.traits.traits.get(&self.tr.id) else {
            // A marker trait (`Eq`, `Ord`, `Encode`, …) declares no methods.
            for (name, m) in self.imp.methods.clone() {
                let span = self.method_span(m);
                self.report(span, format!("`{name}` is not a method of the trait"));
            }
            return;
        };
        if self.tr.args.len() != decl.generics.len() {
            self.report(
                head,
                format!(
                    "the trait takes {} type argument(s), found {}",
                    decl.generics.len(),
                    self.tr.args.len()
                ),
            );
            return;
        }
        // A trait method with a body is a default the impl may leave out.
        let required = |m: DefId| {
            matches!(
                self.prog.item(m).map(|i| &i.kind),
                Some(ItemKind::Fn(f)) if f.body.is_none()
            )
        };
        let missing: Vec<String> = decl
            .methods
            .iter()
            .filter(|(n, m)| required(*m) && !self.imp.methods.iter().any(|(i, _)| i == n))
            .map(|(n, _)| format!("`{n}`"))
            .collect();
        match missing.as_slice() {
            [] => {}
            [one] => self.report(head, format!("the method {one} is missing")),
            many => self.report(head, format!("the methods {} are missing", many.join(", "))),
        }
        for (name, m) in self.imp.methods.clone() {
            let span = self.method_span(m);
            match decl.methods.iter().find(|(n, _)| *n == name) {
                None => self.report(span, format!("`{name}` is not a method of the trait")),
                Some((_, tm)) => {
                    if let Some(detail) = self.compare(decl, &name, m, *tm) {
                        self.report(span, detail);
                    }
                }
            }
        }
    }

    /// The first difference between the impl method `m` and the trait method `tm`, if any.
    /// Any difference also records the trait's signature as `m`'s contract (FOUNDER-QUEUE
    /// Q37: callers always see the trait's signature, whatever the impl wrote: its `self`,
    /// its number of type parameters, their bounds, its parameter and return types).
    fn compare(&mut self, decl: &TraitDecl, name: &str, m: DefId, tm: DefId) -> Option<String> {
        let fn_decl = |d: DefId| match self.prog.item(d).map(|i| &i.kind) {
            Some(ItemKind::Fn(f)) => Some(f),
            _ => None,
        };
        let (f, tf) = (fn_decl(m)?, fn_decl(tm)?);
        let is = self.res.signatures.get(&m)?;
        let ts = self.res.signatures.get(&tm)?;
        let nt = decl.generics.len();
        let ni = self.imp.generics.len();
        // The trait method's scheme: the trait's generics, the method's own, then `Self`.
        let kt = ts.generics.len().saturating_sub(nt + 1);
        let ki = is.generics.len().saturating_sub(ni);
        let params = self.names(&is.generics);
        let args: Vec<Ty> = (0..ts.generics.len())
            .map(|j| {
                if j < nt {
                    self.tr.args.get(j).cloned().unwrap_or(Ty::Error)
                } else if j < nt + kt {
                    Ty::Param((ni + (j - nt)) as u32)
                } else {
                    self.imp.self_ty.clone()
                }
            })
            .collect();
        let expected = ts.ty.subst(&args);
        // The method's own type parameters keep the impl's names when it has as many as the
        // trait, so the trait's bounds read naturally in messages about callers.
        let contract = Scheme {
            generics: self
                .imp
                .generics
                .iter()
                .cloned()
                .chain((0..kt).map(|j| {
                    let g = &ts.generics[nt + j];
                    let own = (kt == ki).then(|| is.generics[ni + j].name.clone());
                    GenericParam {
                        name: own.unwrap_or_else(|| g.name.clone()),
                        bounds: g
                            .bounds
                            .iter()
                            .map(|b| TraitRef {
                                id: b.id,
                                args: b.args.iter().map(|t| t.subst(&args)).collect(),
                            })
                            .collect(),
                    }
                }))
                .collect(),
            effect_params: ts.effect_params,
            ty: expected.clone(),
        };
        let detail = if f.has_self != tf.has_self {
            Some(if tf.has_self {
                format!("`{name}` takes no `self` here, but the trait's `{name}` does")
            } else {
                format!("`{name}` takes `self` here, but the trait's `{name}` does not")
            })
        } else if kt != ki {
            Some(format!(
                "`{name}` has {ki} type parameter(s) here, but {kt} in the trait"
            ))
        } else {
            self.signature_difference(name, f.has_self, &f.params, is, &expected, &params)
                .or_else(|| {
                    // The impl may require less of its type parameters than the trait,
                    // never more.
                    (0..kt).find_map(|j| {
                        let allowed = &contract.generics[ni + j].bounds;
                        let extra = is.generics[ni + j]
                            .bounds
                            .iter()
                            .find(|b| !allowed.contains(b))?;
                        let bound = match extra.id {
                            TraitId::Builtin(b) => b.name().to_string(),
                            TraitId::User(d) => self.res.name_of(d),
                        };
                        Some(format!(
                            "type parameter `{}` of `{name}` needs `{bound}` here, which the trait does not require",
                            is.generics[ni + j].name
                        ))
                    })
                })
        };
        if detail.is_some() {
            self.out.contracts.push((m, contract));
            self.out.selfs.push((m, tf.has_self));
        } else if let Some(bounded) = with_trait_bounds(is, &contract, ni, kt) {
            // FOUNDER-QUEUE Q41: the impl may require less of its type parameters than the
            // trait, but every caller, a concrete receiver included, still proves what the
            // trait requires. The impl's body keeps its own signature.
            self.out.contracts.push((m, bounded));
        }
        detail
    }

    /// The first parameter or return type of the impl method (`is`) that differs from the
    /// trait's (`expected`, already in the impl's terms).
    fn signature_difference(
        &self,
        name: &str,
        has_self: bool,
        decls: &[crate::hir::ParamDecl],
        is: &Scheme,
        expected: &Ty,
        params: &[String],
    ) -> Option<String> {
        let (
            Ty::Fn {
                params: ep,
                ret: er,
                ..
            },
            Ty::Fn {
                params: fp,
                ret: fr,
                ..
            },
        ) = (expected, &is.ty)
        else {
            return None;
        };
        let skip = usize::from(has_self);
        if ep.len() != fp.len() {
            return Some(format!(
                "`{name}` takes {} parameter(s) here, but {} in the trait",
                fp.len().saturating_sub(skip),
                ep.len().saturating_sub(skip)
            ));
        }
        if let Some(i) = (0..ep.len()).find(|&i| !same(&ep[i], &fp[i])) {
            let pname = decls.get(i).map_or("?", |p| p.name.as_str());
            return Some(format!(
                "parameter `{pname}` of `{name}` is `{}` here, but `{}` in the trait",
                self.show(&fp[i], params),
                self.show(&ep[i], params)
            ));
        }
        (!same(er, fr)).then(|| {
            format!(
                "`{name}` returns `{}` here, but `{}` in the trait",
                self.show(fr, params),
                self.show(er, params)
            )
        })
    }
}

/// The two types are the same (rigid generic parameters compare by index; the poison
/// `Error` matches anything, so an already-reported type error is not reported again).
/// Suspension bits are `kalam-effects`' and are not compared.
fn same(a: &Ty, b: &Ty) -> bool {
    InferCtx::new().unify(a, b).is_ok()
}

/// The impl method's scheme `is` with the bounds of its own `kt` type parameters (after
/// the impl's `ni`) taken from the trait's `contract`, when the impl declares fewer of
/// them; `None` when they are the same. Only the bounds change: the types and suspension
/// bits stay the impl's (they already match the trait's).
fn with_trait_bounds(is: &Scheme, contract: &Scheme, ni: usize, kt: usize) -> Option<Scheme> {
    let fewer = (ni..ni + kt).any(|j| match (is.generics.get(j), contract.generics.get(j)) {
        (Some(own), Some(trait_param)) => {
            trait_param.bounds.iter().any(|b| !own.bounds.contains(b))
        }
        _ => false,
    });
    if !fewer {
        return None;
    }
    let mut bounded = is.clone();
    for j in ni..ni + kt {
        if let (Some(own), Some(trait_param)) =
            (bounded.generics.get_mut(j), contract.generics.get(j))
        {
            own.bounds = trait_param.bounds.clone();
        }
    }
    Some(bounded)
}
