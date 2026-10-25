//! Fix-its and recovery helpers of the body checker, kept apart from the checker's core:
//!
//! - well-formed local annotations and constructed values (§9.3, §9.6, [`crate::wf`]);
//! - K1001's missing arms as `case` arms with `panic("TODO")` bodies (§11 rule 1, Q32);
//! - K1110's `let snapshot = v` before the closure, with the uses renamed (§12.3);
//! - K1302's `.ok_or(…)` and K1301's `.map_err(…)` and `impl F as From[E]` (§14.3);
//! - `//` for an `Int / Int` division where an `Int` is expected (FOUNDER-QUEUE Q31).
//!
//! Every inserted keyword and core name is spelled the way the file's skin spells it.

use kalam_base::Span;
use kalam_diagnostics::{Applicability, Edit, Fix};
use kalam_resolve::DefId;
use kalam_skins::{Concept, KeywordTable};

use super::{BodyCx, Origin};
use crate::error::{TypeError, TypeErrorKind};
use crate::exhaust::{self, DPat, Spelling};
use crate::hir::{self, Arm, BinOp, BodyValue, Expr, ExprKind, ItemKind, LocalId};
use crate::traits::{Answer, BuiltinTrait, TraitId, TraitRef};
use crate::ty::Ty;
use crate::wf::{Failure, Written};

/// A statement being checked: where a fix can insert a line before it (K1110).
pub(super) struct StmtCtx {
    /// The closure whose body holds the statement (0: the item's own body).
    pub closure: u32,
    pub start: u32,
    /// Its column when it starts its own line (`None` in a one-line block).
    pub indent: Option<u32>,
    /// A `while` loop: a closure in its condition is made again on every iteration, so a
    /// copy taken once before the loop would change what it sees (no fix there).
    pub while_head: bool,
}

/// The uses of one `var` inside the closures one statement creates (K1110).
pub(super) struct Capture {
    pub local: LocalId,
    /// The statement in the `var`'s own scope that creates the closure.
    pub at: Option<(u32, Option<u32>)>,
    pub uses: Vec<Span>,
}

impl BodyCx<'_, '_> {
    /// The declared names of the generic parameters in scope (messages show `T`, not `T0`).
    pub(super) fn param_names(&self) -> Vec<String> {
        self.generics.iter().map(|g| g.name.clone()).collect()
    }

    /// Adds an error made outside [`BodyCx::err`], naming the generic parameters in scope.
    pub(super) fn push_error(&mut self, mut e: TypeError) {
        e.params = self.param_names();
        self.out.errors.push(e);
    }

    fn trait_text(&self, tr: &TraitRef) -> String {
        match tr.id {
            TraitId::Builtin(b) => b.name().to_string(),
            TraitId::User(d) => self.res().name_of(d),
        }
    }

    /// A local annotation (`let x: T`, a lambda or local `fn` signature): lowered, then
    /// checked for well-formedness; a failing component is poison afterwards, so the
    /// value's uses do not report the same root cause again.
    pub(super) fn local_type(&mut self, t: &hir::Type) -> Ty {
        let ty = self.lower_local_type(t);
        let cx = self.cx;
        let generics = self.generics.clone();
        let answer =
            |ty: &Ty, tr: &TraitRef| cx.res.traits.implements(&cx.res.adts, &generics, ty, tr);
        let mut failures = Vec::new();
        let checked = Written {
            prog: cx.prog,
            adts: &cx.res.adts,
            answer: &answer,
        }
        .check(t, &ty, &mut failures);
        self.report_wf(&failures);
        checked
    }

    fn report_wf(&mut self, failures: &[Failure]) {
        let mut seen: Vec<Span> = Vec::new();
        for f in failures {
            self.reported.insert((f.ty.clone(), f.tr.clone()));
            if seen.contains(&f.span) {
                continue;
            }
            seen.push(f.span);
            let name = self.trait_text(&f.tr);
            let e = TypeError::new(
                TypeErrorKind::MissingTrait {
                    ty: f.ty.clone(),
                    trait_name: name.clone(),
                },
                self.file,
                f.span,
            )
            .with_label(f.span, f.label(&name));
            self.push_error(e);
        }
    }

    /// A record or enum value of `adt[targs]` is built here: its parameters' bounds hold.
    pub(super) fn require_adt_bounds(&mut self, adt: DefId, targs: &[Ty], span: Span) {
        let Some(generics) = self.res().adts.get(&adt).map(|a| a.generics.clone()) else {
            return;
        };
        for (g, arg) in generics.iter().zip(targs) {
            for b in &g.bounds {
                let tr = TraitRef {
                    id: b.id,
                    args: b.args.iter().map(|t| t.subst(targs)).collect(),
                };
                self.require(arg, tr, span, Origin::Bound);
            }
        }
    }

    /// The witness spellings of this file's skin; `None` when the skin is unknown.
    fn spelling(&self) -> Option<Spelling> {
        Some(Spelling {
            some: self.keyword(Concept::CoreSome)?,
            none: self.keyword(Concept::CoreNone)?,
            ok: self.keyword(Concept::CoreOk)?,
            err: self.keyword(Concept::CoreErr)?,
            true_: self.keyword(Concept::KwTrue)?,
            false_: self.keyword(Concept::KwFalse)?,
            comma: self.keyword(Concept::PunctComma)?,
        })
    }

    /// K1001's fix (§11 rule 1): each missing case as a `case` arm with a `panic("TODO")`
    /// body, after the last arm and at its column, in the file's skin.
    pub(super) fn missing_arms_fix(
        &self,
        arms: &[Arm],
        st: &Ty,
        witnesses: &[DPat],
    ) -> Option<Fix> {
        let last = arms.last()?;
        let indent = last.indent?;
        let sp = self.spelling()?;
        let case = self.keyword(Concept::KwCase)?;
        let panic = self.keyword(Concept::CorePanic)?;
        let cx = exhaust::Cx {
            adts: &self.res().adts,
        };
        let pad = " ".repeat(indent as usize);
        let mut text = String::new();
        for w in witnesses {
            let pat = cx.show_in(w, st, &sp);
            text.push_str(&format!("\n{pad}{case} {pat}: {panic}(\"TODO\")"));
        }
        if text.is_empty() {
            return None;
        }
        Some(Fix::machine(
            "add the missing cases",
            Span::new(last.span.end, last.span.end),
            text,
        ))
    }

    /// A name for a new binding that no local of this body and no item of this module uses
    /// (so the fix cannot cause K0405), nor any name in `taken`.
    fn fresh_name(&self, base: &str, taken: &[String]) -> String {
        let used = |n: &str| {
            taken.iter().any(|t| t == n)
                || self.decls.iter().any(|d| d.name == n)
                || self
                    .prog()
                    .items()
                    .iter()
                    .any(|i| i.module == self.module && i.name == n)
        };
        if !used(base) {
            return base.to_string();
        }
        let mut i = 2;
        loop {
            let n = format!("{base}_{i}");
            if !used(&n) {
                return n;
            }
            i += 1;
        }
    }

    /// Records a use of the `var` `l` (declared in closure `var_closure`) from inside a
    /// closure (§12.3, K1110). Uses are grouped per `var` and creating statement.
    pub(super) fn note_capture(&mut self, l: LocalId, span: Span, var_closure: u32) {
        let at = self
            .stmt_stack
            .iter()
            .rev()
            .find(|s| s.closure == var_closure)
            .map(|s| (s.start, if s.while_head { None } else { s.indent }));
        if let Some(c) = self
            .captures
            .iter_mut()
            .find(|c| c.local == l && c.at == at)
        {
            c.uses.push(span);
            return;
        }
        self.captures.push(Capture {
            local: l,
            at,
            uses: vec![span],
        });
    }

    /// One K1110 per captured `var` and creating statement, at its first use, with the fix
    /// that copies it into a constant before that statement and uses the copy.
    pub(super) fn capture_errors(&mut self) {
        let captures = std::mem::take(&mut self.captures);
        let mut taken: Vec<String> = Vec::new();
        for c in captures {
            let Some(&first) = c.uses.first() else {
                continue;
            };
            let name = self
                .decls
                .get(c.local.0 as usize)
                .map_or(String::new(), |d| d.name.clone());
            let mut e = TypeError::new(
                TypeErrorKind::CapturesVar { name: name.clone() },
                self.file,
                first,
            );
            if let (Some((start, Some(indent))), Some(kw)) = (c.at, self.keyword(Concept::KwLet)) {
                let snap = self.fresh_name("snapshot", &taken);
                taken.push(snap.clone());
                let mut edits = vec![Edit {
                    span: Span::new(start, start),
                    insert: format!("{kw} {snap} = {name}\n{}", " ".repeat(indent as usize)),
                }];
                edits.extend(c.uses.iter().map(|u| Edit {
                    span: *u,
                    insert: snap.clone(),
                }));
                e.fixes.push(Fix {
                    title: format!(
                        "copy `{name}` into `{snap}` before the closure, and use `{snap}` in it"
                    ),
                    applicability: Applicability::Machine,
                    edits,
                });
            }
            self.push_error(e);
        }
    }

    /// K1302's fix (§14.3): `.ok_or(<error>)` before the `?`. A `String` error gets a text,
    /// in the file's language; any other error is the reader's choice.
    pub(super) fn ok_or_fix(&self, inner: &Expr, err_ty: &Ty) -> Fix {
        let at = Span::new(inner.span.end, inner.span.end);
        if *err_ty == Ty::String {
            let text = match self.prog().skins.get(&self.file) {
                Some(kalam_base::Skin::Ar) => "قيمة مفقودة",
                Some(kalam_base::Skin::Franco) => "qeema mesh mawgooda",
                _ => "missing value",
            };
            return Fix::machine(
                "turn the missing value into an error with `.ok_or(…)`",
                at,
                format!(".ok_or(\"{text}\")"),
            );
        }
        Fix::suggestion(
            "turn the missing value into an error with `.ok_or(…)`: replace `error` with the error to return",
            at,
            ".ok_or(error)",
        )
    }

    /// K1301's fixes (§14.3 rule 4): convert the error at the call with `.map_err(…)`, or
    /// add `impl F as From[E]` after the function (when `F` is this module's own type, as
    /// the orphan rule requires).
    pub(super) fn convert_error_fixes(&self, inner: &Expr, e: &Ty, f: &Ty) -> Vec<Fix> {
        let mut out = Vec::new();
        let Some(fn_kw) = self.keyword(Concept::KwFn) else {
            return out;
        };
        let at = Span::new(inner.span.end, inner.span.end);
        let p = self.fresh_name("failure", &[]);
        let show = TraitRef::builtin(BuiltinTrait::Show);
        if *f == Ty::String && self.answer(e, &show) == Answer::Yes {
            out.push(Fix::machine(
                "turn the error into text with `.map_err(…)`",
                at,
                format!(".map_err({fn_kw}({p}) => \"{{{p}}}\")"),
            ));
        } else {
            out.push(Fix::suggestion(
                "convert the error at the call with `.map_err(…)`: write the conversion after `=>`",
                at,
                format!(".map_err({fn_kw}({p}) => …)"),
            ));
        }
        if let Some(fix) = self.impl_from_fix(e, f) {
            out.push(fix);
        }
        out
    }

    fn impl_from_fix(&self, e: &Ty, f: &Ty) -> Option<Fix> {
        let (Ty::Adt(ed, ea), Ty::Adt(fd, fa)) = (e, f) else {
            return None;
        };
        if !ea.is_empty() || !fa.is_empty() {
            return None;
        }
        let res = self.res();
        if res.adts.get(fd).map(|a| &a.module) != Some(&self.module) {
            return None;
        }
        let item = self.prog().item(self.owner)?;
        let ItemKind::Fn(fd_decl) = &item.kind else {
            return None;
        };
        if fd_decl.parent.is_some() {
            return None;
        }
        let end = match &fd_decl.body.as_ref()?.value {
            BodyValue::Block(b) => b.span.end,
            BodyValue::Expr(x) => x.span.end,
        };
        let table = KeywordTable::builtin();
        let skin = *self.prog().skins.get(&self.file)?;
        let kw = |c: Concept| table.spelling(c, skin).to_string();
        let (en, fname) = (res.name_of(*ed), res.name_of(*fd));
        let text = format!(
            "\n\n{} {fname} {} From[{en}]:\n    {} from(value: {en}) -> {fname} => {}(\"TODO\")",
            kw(Concept::KwImpl),
            kw(Concept::KwAs),
            kw(Concept::KwFn),
            kw(Concept::CorePanic),
        );
        Some(Fix::suggestion(
            format!("add `impl {fname} as From[{en}]` and write the conversion"),
            Span::new(end, end),
            text,
        ))
    }

    /// FOUNDER-QUEUE Q31: `Int / Int` is a `Float`; where an `Int` is expected, suggest the
    /// whole-number division `//`.
    pub(super) fn int_division_fix(&mut self, e: &Expr, found: &Ty, expected: &Ty) -> Option<Fix> {
        if !(*found == Ty::Float && *expected == Ty::Int) {
            return None;
        }
        let ExprKind::Binary {
            op: BinOp::Div,
            lhs,
            rhs,
        } = &e.kind
        else {
            return None;
        };
        let mut int = |x: &Expr| {
            let t = self
                .out
                .exprs
                .iter()
                .rev()
                .find(|(id, _)| *id == x.id)
                .map(|(_, t)| t.clone());
            t.is_some_and(|t| self.infer.resolve(&t) == Ty::Int)
        };
        if !(int(lhs) && int(rhs)) {
            return None;
        }
        Some(Fix::suggestion(
            "divide whole numbers with `//`, which rounds down",
            Span::new(lhs.span.end, rhs.span.start),
            " // ",
        ))
    }

    /// Q31 for `x /= n` on an `Int` `x`: `x = x // n`.
    pub(super) fn div_assign_fix(&self, place: &Expr, value: &Expr) -> Option<Fix> {
        let ExprKind::Local(l) = place.kind else {
            return None;
        };
        let name = self.decls.get(l.0 as usize)?.name.clone();
        Some(Fix::suggestion(
            "divide whole numbers with `//`, which rounds down",
            Span::new(place.span.end, value.span.start),
            format!(" = {name} // "),
        ))
    }
}
