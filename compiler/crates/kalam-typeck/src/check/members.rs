//! Member and variant names the checker settles itself (FOUNDER-QUEUE Q34, Q35):
//!
//! - a function-typed record field called with method syntax (`job.run(1)`) is K1108, with
//!   the machine fix `(job.run)(1)`; the call is then checked as that call, so nothing
//!   cascades (spec §12.2: `x.m(…)` is always a method of `x`'s type);
//! - `.V` where no enum of a fully known program has a variant `V` is K0401, exactly as
//!   name resolution reports it when it knows the whole program (project mode). In a
//!   single-file check resolution does not know the program, so the checker says it.

use std::collections::BTreeSet;

use kalam_base::Span;
use kalam_diagnostics::{Applicability, Edit, Fix};

use super::BodyCx;
use crate::adt::AdtKind;
use crate::error::{TypeError, TypeErrorKind};
use crate::hir::{Arg, Expr};
use crate::ty::Ty;

impl BodyCx<'_, '_> {
    /// `x.f(…)` where `x`'s record type has no method `f` but a field `f` that holds a
    /// function: K1108, then the call is checked as `(x.f)(…)`. `None` when `f` is no such
    /// field (the caller reports K0420).
    pub(super) fn field_called_as_method(
        &mut self,
        e: &Expr,
        receiver: &Expr,
        rt: &Ty,
        name: &str,
        args: &[Arg],
    ) -> Option<Ty> {
        let Ty::Adt(d, targs) = rt else {
            return None;
        };
        let adt = self.res().adts.get(d)?;
        // Outside an opaque type's module its fields are private: K0420, as for any name.
        if adt.opaque && adt.module != self.module {
            return None;
        }
        let AdtKind::Record(fields) = &adt.kind else {
            return None;
        };
        let ft = fields.iter().find(|f| f.name == name)?.ty.subst(targs);
        let Ty::Fn { params, ret, .. } = self.infer.shallow(&ft) else {
            return None;
        };
        let mut err = TypeError::new(
            TypeErrorKind::FieldCalledAsMethod {
                ty: rt.clone(),
                name: name.to_string(),
            },
            self.file,
            e.span,
        );
        if let Some(at) = self.prog().method_names.get(&(self.owner, e.id)) {
            err.fixes.push(Fix {
                title: format!("call the function stored in the field `{name}`"),
                applicability: Applicability::Machine,
                edits: vec![
                    Edit {
                        span: Span::new(receiver.span.start, receiver.span.start),
                        insert: "(".into(),
                    },
                    Edit {
                        span: Span::new(at.end, at.end),
                        insert: ")".into(),
                    },
                ],
            });
        }
        self.push_error(err);
        for (a, p) in args.iter().zip(&params) {
            self.check(&a.value, p);
        }
        for a in args.iter().skip(params.len()) {
            self.synth(&a.value);
        }
        Some(*ret)
    }

    /// `.name` (an expression, call or pattern at `span`, starting with the dot) names no
    /// variant of any enum of a fully known program: reports K0401 at the name, as resolve
    /// does, with the closest variant name as a suggestion, and returns true. False when a
    /// variant has that name or the program is not fully known.
    pub(super) fn unknown_variant(&mut self, span: Span, name: &str) -> bool {
        if !self.prog().complete {
            return false;
        }
        let mut names = BTreeSet::new();
        for adt in self.res().adts.values() {
            if let AdtKind::Enum(variants) = &adt.kind {
                for v in variants {
                    if v.name == name {
                        return false;
                    }
                    names.insert(v.name.clone());
                }
            }
        }
        // The name right after the dot (resolve reports at the identifier).
        let start = span.start.saturating_add(1).min(span.end);
        let at = Span::new(start, start.saturating_add(name.len() as u32).min(span.end));
        let mut err = TypeError::new(
            TypeErrorKind::UnknownVariant {
                name: name.to_string(),
            },
            self.file,
            at,
        );
        if let Some(best) = kalam_resolve::closest(name, names.iter().map(String::as_str)) {
            err.fixes.push(Fix::suggestion(
                format!("write `.{best}`"),
                at,
                best.to_string(),
            ));
        }
        self.push_error(err);
        true
    }
}
