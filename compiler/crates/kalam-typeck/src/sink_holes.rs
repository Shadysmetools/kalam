//! The sink checks of tagged literals (spec §17.3–§17.6), after every body is checked.
//!
//! The rules live in `kalam-sinks`: [`kalam_sinks::check_template`] parses each literal and
//! classifies its holes, and [`kalam_sinks::check_hole`] judges each hole's type. This pass
//! only summarizes the checker's (zonked) hole types as [`HoleType`]s, asking the trait
//! environment for `Show` and `Message`, and records what comes back as type errors.
//!
//! `path"…"` holes are not judged here: the checker reports K1641 with its fix itself.

use kalam_sinks::{HoleKind, HoleType, HoleVerdict, SinkKind, StaticParts, Tag};

use crate::check::TypeckResults;
use crate::error::{TypeError, TypeErrorKind};
use crate::traits::{Answer, BuiltinTrait, TraitRef};
use crate::ty::{GenericParam, SinkType, Ty};

/// Runs the sink checks of every tagged literal in `res.tagged`.
pub(crate) fn check(res: &mut TypeckResults) {
    let mut errors = Vec::new();
    for lit in &res.tagged {
        let parts = StaticParts {
            parts: lit.parts.clone(),
            hole_spans: lit.holes.iter().map(|(s, _)| *s).collect(),
        };
        let out = kalam_sinks::check_template(lit.tag, &parts, lit.file, lit.span);
        for d in out.diagnostics {
            errors.push(TypeError::new(
                TypeErrorKind::Sink(d.clone()),
                d.file,
                d.span,
            ));
        }
        if lit.tag == Tag::Path {
            continue;
        }
        let generics: &[GenericParam] = res
            .signatures
            .get(&lit.owner)
            .map_or(&[], |s| s.generics.as_slice());
        for (req, (span, ty)) in out.holes.iter().zip(&lit.holes) {
            let summary = summarize(res, generics, ty);
            let kind = match kalam_sinks::check_hole(req, &summary, lit.file, *span) {
                HoleVerdict::Ok => continue,
                HoleVerdict::Report(d) => TypeErrorKind::Sink(d),
                HoleVerdict::MissingTrait(name) => TypeErrorKind::MissingTrait {
                    ty: ty.clone(),
                    trait_name: name.to_string(),
                },
                HoleVerdict::ExpectedText => TypeErrorKind::Mismatch {
                    expected: Ty::String,
                    found: ty.clone(),
                },
            };
            let mut err = TypeError::new(kind, lit.file, *span);
            // Generic parameters show by their declared names (`T`, not `T0`).
            err.params = generics.iter().map(|g| g.name.clone()).collect();
            errors.push(err);
        }
    }
    res.errors.extend(errors);
}

fn summarize(res: &TypeckResults, generics: &[GenericParam], ty: &Ty) -> HoleType {
    if matches!(ty, Ty::Error | Ty::Never) || ty.has_vars() {
        return HoleType::unknown();
    }
    let holds = |tr: BuiltinTrait| {
        res.traits
            .implements(&res.adts, generics, ty, &TraitRef::builtin(tr))
            != Answer::No
    };
    HoleType {
        kind: kind_of(res, ty),
        show: holds(BuiltinTrait::Show),
        message: holds(BuiltinTrait::Message),
    }
}

fn kind_of(res: &TypeckResults, ty: &Ty) -> HoleKind {
    match ty {
        Ty::String => HoleKind::String,
        Ty::Int => HoleKind::Int,
        Ty::Bool => HoleKind::Bool,
        Ty::Float | Ty::BigInt | Ty::Decimal => HoleKind::Number,
        Ty::Bytes => HoleKind::Bytes,
        Ty::Sink(s) => HoleKind::Sink(match s {
            SinkType::Sql => SinkKind::Sql,
            SinkType::Html => SinkKind::Html,
            SinkType::Command => SinkKind::Command,
            SinkType::Url => SinkKind::Url,
            SinkType::Path => SinkKind::Path,
            SinkType::PathSegment => SinkKind::PathSegment,
            SinkType::Regex => SinkKind::Regex,
        }),
        Ty::List(t) => HoleKind::List(Box::new(kind_of(res, t))),
        Ty::Option(t) => HoleKind::Option(Box::new(kind_of(res, t))),
        Ty::Adt(d, _) => HoleKind::Named(res.name_of(*d)),
        Ty::Error | Ty::Never | Ty::Var(_) => HoleKind::Unknown,
        _ => HoleKind::Other,
    }
}
