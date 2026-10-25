//! Checks on declarations that need the whole program collected first (pass 1's end):
//!
//! - **Well-formed written types** ([`crate::wf`]): `Map` keys and `Set` elements are
//!   `Eq + Hash`, and generic records and enums get arguments that satisfy their bounds,
//!   in signatures, record fields, variant payloads, alias targets and impl heads (spec
//!   §9.3, §9.6). A generic parameter that needs a bound it does not declare
//!   (`fn wrap[T](x: T) -> Set[T]`) is reported once, at the declaration, and the bound is
//!   then assumed, so its uses stay quiet.
//! - **The entry point** (§14.3, §16.1 rule 1): `main` takes nothing or one `World`, and
//!   returns `()` or `Result[(), E]` with `E: Error`.
//! - **Derives** (§9.7): an explicit `@derive(Tr)` needs every field (every payload of an
//!   enum) to implement `Tr` (`Decode` for `Row` and `Message`), with the type's own
//!   parameters assumed to; otherwise it is K0603 at the derive, and the type is then
//!   taken to implement `Tr` so uses do not report the same root cause again. `Encode` on
//!   a `Secret` field is K0612 instead.

use kalam_base::{FileId, Span};
use kalam_resolve::DefId;

use crate::adt::AdtKind;
use crate::check::TypeckResults;
use crate::error::{TypeError, TypeErrorKind};
use crate::hir::{self, HirProgram, ItemKind, TypeKind};
use crate::traits::{Answer, BuiltinTrait, TraitId, TraitRef};
use crate::ty::{Capability, GenericParam, Ty};
use crate::wf::{Failure, Why, Written};

/// Runs every declaration check, adding errors to `res.errors` and patching `res` for
/// recovery (assumed bounds and derives, poisoned field and signature types).
pub fn check(prog: &HirProgram, res: &mut TypeckResults) {
    // Types first, so the bounds they assume are known to the impls and functions that
    // use them; impl heads before methods, which see the impl's parameters.
    for item in prog.items() {
        match &item.kind {
            ItemKind::Record(_) | ItemKind::Enum(_) => adt_fields(prog, res, item),
            ItemKind::Alias(a) => alias(prog, res, item, a),
            _ => {}
        }
    }
    for item in prog.items() {
        if let ItemKind::Impl(i) = &item.kind {
            impl_head(prog, res, item, i);
        }
    }
    for item in prog.items() {
        if let ItemKind::Fn(f) = &item.kind {
            fn_signature(prog, res, item, f);
        }
    }
    for item in prog.items() {
        derives(res, item);
    }
    for &main in &prog.mains {
        entry_point(prog, res, main);
    }
}

fn trait_name(res: &TypeckResults, tr: &TraitRef) -> String {
    match tr.id {
        TraitId::Builtin(b) => b.name().to_string(),
        TraitId::User(d) => res.name_of(d),
    }
}

fn names(generics: &[GenericParam]) -> Vec<String> {
    generics.iter().map(|g| g.name.clone()).collect()
}

/// Reports each failure (once per span) as K0603 with its rule as a label.
fn report(
    res: &mut TypeckResults,
    file: FileId,
    params: &[String],
    failures: &[Failure],
) -> Vec<(u32, TraitRef)> {
    let mut assumed = Vec::new();
    let mut seen: Vec<Span> = Vec::new();
    for f in failures {
        if let Ty::Param(i) = f.ty {
            assumed.push((i, f.tr.clone()));
        }
        if seen.contains(&f.span) {
            continue;
        }
        seen.push(f.span);
        let tname = trait_name(res, &f.tr);
        let mut e = TypeError::new(
            TypeErrorKind::MissingTrait {
                ty: f.ty.clone(),
                trait_name: tname.clone(),
            },
            file,
            f.span,
        )
        .with_label(f.span, f.label(&tname));
        e.params = params.to_vec();
        res.errors.push(e);
    }
    assumed
}

/// Runs the written-type check of `t` (lowered as `ty`) under `generics`.
fn written(
    prog: &HirProgram,
    res: &TypeckResults,
    generics: &[GenericParam],
    t: &hir::Type,
    ty: &Ty,
    failures: &mut Vec<Failure>,
) -> Ty {
    let answer = |ty: &Ty, tr: &TraitRef| res.traits.implements(&res.adts, generics, ty, tr);
    let w = Written {
        prog,
        adts: &res.adts,
        answer: &answer,
    };
    w.check(t, ty, failures)
}

fn assume(generics: &mut [GenericParam], assumed: &[(u32, TraitRef)]) {
    for (i, tr) in assumed {
        if let Some(g) = generics.get_mut(*i as usize) {
            if !g.bounds.contains(tr) {
                g.bounds.push(tr.clone());
            }
        }
    }
}

fn fn_signature(prog: &HirProgram, res: &mut TypeckResults, item: &hir::Item, f: &hir::FnDecl) {
    let Some(scheme) = res.signatures.get(&item.def).cloned() else {
        return;
    };
    let Ty::Fn {
        params,
        ret,
        suspension,
    } = &scheme.ty
    else {
        return;
    };
    let mut failures = Vec::new();
    let new_params: Vec<Ty> = f
        .params
        .iter()
        .zip(params)
        .map(|(p, ty)| written(prog, res, &scheme.generics, &p.ty, ty, &mut failures))
        .collect();
    let new_ret = match &f.ret {
        Some(t) => written(prog, res, &scheme.generics, t, ret, &mut failures),
        None => (**ret).clone(),
    };
    if failures.is_empty() {
        return;
    }
    let assumed = report(res, item.file, &names(&scheme.generics), &failures);
    if let Some(s) = res.signatures.get_mut(&item.def) {
        assume(&mut s.generics, &assumed);
        s.ty = Ty::Fn {
            params: new_params,
            ret: Box::new(new_ret),
            suspension: *suspension,
        };
    }
}

fn adt_fields(prog: &HirProgram, res: &mut TypeckResults, item: &hir::Item) {
    let Some(adt) = res.adts.get(&item.def).cloned() else {
        return;
    };
    let mut failures = Vec::new();
    let new_kind = match (&item.kind, &adt.kind) {
        (ItemKind::Record(r), AdtKind::Record(fields)) => {
            let mut fields = fields.clone();
            for (decl, field) in r.fields.iter().zip(fields.iter_mut()) {
                field.ty = written(prog, res, &adt.generics, &decl.ty, &field.ty, &mut failures);
            }
            AdtKind::Record(fields)
        }
        (ItemKind::Enum(e), AdtKind::Enum(variants)) => {
            let mut variants = variants.clone();
            for (decl, v) in e.variants.iter().zip(variants.iter_mut()) {
                for (fd, field) in decl.fields.iter().zip(v.fields.iter_mut()) {
                    field.ty = written(prog, res, &adt.generics, &fd.ty, &field.ty, &mut failures);
                }
            }
            AdtKind::Enum(variants)
        }
        _ => return,
    };
    if failures.is_empty() {
        return;
    }
    let assumed = report(res, item.file, &names(&adt.generics), &failures);
    if let Some(a) = res.adts.get_mut(&item.def) {
        assume(&mut a.generics, &assumed);
        a.kind = new_kind;
    }
}

/// An alias's target is checked once, at the alias, with the alias's parameters.
fn alias(prog: &HirProgram, res: &mut TypeckResults, item: &hir::Item, a: &hir::AliasDecl) {
    let generics: Vec<GenericParam> = a
        .generics
        .iter()
        .map(|g| GenericParam {
            name: g.name.clone(),
            bounds: g
                .bounds
                .iter()
                .map(|b| TraitRef {
                    id: b.id,
                    args: Vec::new(),
                })
                .collect(),
        })
        .collect();
    let Some(ty) = simple_lower(res, &a.target) else {
        return;
    };
    let mut failures = Vec::new();
    written(prog, res, &generics, &a.target, &ty, &mut failures);
    report(res, item.file, &names(&generics), &failures);
}

/// The lowered form of a written type for the declaration checks that have no lowered
/// copy at hand (alias targets): built-in types, records and enums, generic parameters,
/// tuples and functions. `None` when it names something else (another alias).
fn simple_lower(res: &TypeckResults, t: &hir::Type) -> Option<Ty> {
    use crate::hir::BuiltinType as B;
    let all =
        |ts: &[hir::Type]| -> Option<Vec<Ty>> { ts.iter().map(|t| simple_lower(res, t)).collect() };
    Some(match &t.kind {
        TypeKind::Builtin(b, args) => {
            let a = all(args)?;
            let one = |i: usize| Box::new(a.get(i).cloned().unwrap_or(Ty::Error));
            match b {
                B::Int => Ty::Int,
                B::BigInt => Ty::BigInt,
                B::Float => Ty::Float,
                B::Decimal => Ty::Decimal,
                B::Bool => Ty::Bool,
                B::String => Ty::String,
                B::Bytes => Ty::Bytes,
                B::Never => Ty::Never,
                B::AnyError => Ty::AnyError,
                B::JsValue => Ty::JsValue,
                B::Capability(c) => Ty::Capability(*c),
                B::Sink(s) => Ty::Sink(*s),
                B::List => Ty::List(one(0)),
                B::Set => Ty::Set(one(0)),
                B::Option => Ty::Option(one(0)),
                B::Range => Ty::Range(one(0)),
                B::Secret => Ty::Secret(one(0)),
                B::Map => Ty::Map(one(0), one(1)),
                B::Result => Ty::Result(one(0), one(1)),
            }
        }
        TypeKind::Def(d, args) if res.adts.contains_key(d) => Ty::Adt(*d, all(args)?),
        TypeKind::Param(i) => Ty::Param(*i),
        TypeKind::Unit => Ty::Unit,
        TypeKind::Tuple(ts) => Ty::Tuple(all(ts)?),
        TypeKind::Fn { params, ret, .. } => Ty::Fn {
            params: all(params)?,
            ret: Box::new(simple_lower(res, ret)?),
            suspension: crate::ty::Suspension::Suspending,
        },
        TypeKind::Error => Ty::Error,
        _ => return None,
    })
}

fn impl_head(prog: &HirProgram, res: &mut TypeckResults, item: &hir::Item, i: &hir::ImplItem) {
    let Some(index) = res.traits.impls.iter().position(|d| d.def == item.def) else {
        return;
    };
    let decl = res.traits.impls[index].clone();
    let mut failures = Vec::new();
    written(
        prog,
        res,
        &decl.generics,
        &i.self_ty,
        &decl.self_ty,
        &mut failures,
    );
    if let (Some(b), Some(tr)) = (&i.as_trait, &decl.trait_ref) {
        for (t, ty) in b.args.iter().zip(&tr.args) {
            written(prog, res, &decl.generics, t, ty, &mut failures);
        }
        // The trait's own parameters' bounds hold for its arguments (`impl X as Tr[A]`
        // with `trait Tr[T: Show]` needs `A: Show`).
        if let Some(td) = res.traits.traits.get(&tr.id) {
            let owner = trait_name(res, tr);
            for ((t, ty), g) in b.args.iter().zip(&tr.args).zip(&td.generics) {
                for bound in &g.bounds {
                    let bound = TraitRef {
                        id: bound.id,
                        args: bound.args.iter().map(|a| a.subst(&tr.args)).collect(),
                    };
                    let answer = res.traits.implements(&res.adts, &decl.generics, ty, &bound);
                    if answer == Answer::No {
                        failures.push(Failure {
                            span: t.span,
                            ty: ty.clone(),
                            tr: bound,
                            why: Why::Param {
                                owner: owner.clone(),
                                param: g.name.clone(),
                            },
                        });
                    }
                }
            }
        }
    }
    if failures.is_empty() {
        return;
    }
    let assumed = report(res, item.file, &names(&decl.generics), &failures);
    assume(&mut res.traits.impls[index].generics, &assumed);
    // Its methods see the impl's parameters first (§8.1): they assume the bounds too.
    for (_, m) in &decl.methods {
        if let Some(s) = res.signatures.get_mut(m) {
            assume(&mut s.generics, &assumed);
        }
    }
}

fn derives(res: &mut TypeckResults, item: &hir::Item) {
    let (list, spans) = match &item.kind {
        ItemKind::Record(r) => (&r.derives, &r.derive_spans),
        ItemKind::Enum(e) => (&e.derives, &e.derive_spans),
        _ => return,
    };
    let Some(adt) = res.adts.get(&item.def).cloned() else {
        return;
    };
    let args: Vec<Ty> = (0..adt.generics.len() as u32).map(Ty::Param).collect();
    let comps = adt.component_types(&args);
    // One field breaking several derives (`@derive(Eq, Hash)` with a `Float` field) is one
    // root cause: the first broken derive is reported, every broken one is assumed.
    let mut reported = false;
    for (b, span) in list.iter().zip(spans) {
        let field_trait = match b {
            BuiltinTrait::Row | BuiltinTrait::Message => BuiltinTrait::Decode,
            other => *other,
        };
        let tr = TraitRef::builtin(field_trait);
        // A derive is conditional on the type's own parameters (`Box[T]: Eq` when `T: Eq`).
        let generics: Vec<GenericParam> = adt
            .generics
            .iter()
            .map(|g| {
                let mut g = g.clone();
                g.bounds.push(tr.clone());
                g
            })
            .collect();
        let failing = comps.iter().find(|c| {
            !(*b == BuiltinTrait::Encode && c.contains_secret())
                && res.traits.implements(&res.adts, &generics, c, &tr) == Answer::No
        });
        let Some(c) = failing else { continue };
        if let Some(a) = res.adts.get_mut(&item.def) {
            a.assumed.push(*b);
        }
        if std::mem::replace(&mut reported, true) {
            continue;
        }
        let tname = field_trait.name();
        let mut e = TypeError::new(
            TypeErrorKind::MissingTrait {
                ty: c.clone(),
                trait_name: tname.to_string(),
            },
            item.file,
            *span,
        )
        .with_label(
            *span,
            format!("`@derive({})` needs every field to be `{tname}`", b.name()),
        );
        e.params = names(&adt.generics);
        res.errors.push(e);
    }
}

/// §14.3: "`main` may return `()` or `Result[(), E]` with `E: Error`"; §16.1 rule 1: "Only
/// `main` receives the root capability: `fn main(world: World)`".
fn entry_point(prog: &HirProgram, res: &mut TypeckResults, main: DefId) {
    let Some(item) = prog.item(main) else { return };
    let ItemKind::Fn(f) = &item.kind else { return };
    let Some(Ty::Fn { params, ret, .. }) = res.signatures.get(&main).map(|s| s.ty.clone()) else {
        return;
    };
    let file = item.file;
    let mut errors = Vec::new();
    let mismatch = |expected: Ty, found: Ty, span: Span, why: &str| {
        TypeError::new(TypeErrorKind::Mismatch { expected, found }, file, span)
            .with_label(span, why.to_string())
    };
    for (i, (p, ty)) in f.params.iter().zip(&params).enumerate() {
        let world = Ty::Capability(Capability::World);
        if i > 0 {
            errors.push(mismatch(
                Ty::Unit,
                ty.clone(),
                p.span,
                "`main` takes nothing, or the `World` alone",
            ));
        } else if !matches!(ty, Ty::Capability(Capability::World) | Ty::Error) {
            errors.push(mismatch(
                world,
                ty.clone(),
                p.ty.span,
                "`main` takes nothing, or the `World` alone",
            ));
        }
    }
    let why = "`main` returns `()` or `Result[(), E]` where `E` is an `Error`";
    match (&*ret, &f.ret) {
        (Ty::Unit | Ty::Error, _) | (_, None) => {}
        (Ty::Result(ok, err), Some(t)) => {
            let (ok_span, err_span) = match &t.kind {
                TypeKind::Builtin(_, args) if args.len() == 2 => (args[0].span, args[1].span),
                _ => (t.span, t.span),
            };
            if !matches!(**ok, Ty::Unit | Ty::Error) {
                errors.push(mismatch(Ty::Unit, (**ok).clone(), ok_span, why));
            }
            let error = TraitRef::builtin(BuiltinTrait::Error);
            if res.traits.implements(&res.adts, &[], err, &error) == Answer::No {
                errors.push(
                    TypeError::new(
                        TypeErrorKind::MissingTrait {
                            ty: (**err).clone(),
                            trait_name: "Error".into(),
                        },
                        file,
                        err_span,
                    )
                    .with_label(err_span, why),
                );
            }
        }
        (other, Some(t)) => {
            errors.push(mismatch(Ty::Unit, other.clone(), t.span, why));
        }
    }
    res.errors.extend(errors);
}
