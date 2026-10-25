//! The inference engine core: union-find over type variables, unification with an occurs
//! check, scheme instantiation, and suspension-flow constraints.
//!
//! - Variables live in a union-find forest (union by rank, path compression). A root may
//!   carry a value; `resolve` substitutes values everywhere ("zonking").
//! - `unify(found, expected)` is structural. `Ty::Error` unifies with anything (poison).
//!   `Never` is *not* unified with other types here: `Never <: T` is a coercion that the
//!   checker applies where a value is checked against an expected type (§9.1).
//! - Suspension bits are never unified. A sync function may be used where a suspending
//!   one is expected (§15.3), which is a direction, not an equality, and the bits of
//!   functions depend on the call graph. So unifying two fn types records a
//!   [`SuspFlow`] `found ≤ expected` for `kalam-effects` to solve, plus the span where a
//!   `Coerce::SyncToSusp` may be needed in KIR.
//! - There is no generalization: schemes come only from annotated signatures (§9.9).

use kalam_base::Span;

use crate::ty::{Scheme, Suspension, Ty};

#[derive(Clone, Debug)]
struct Node {
    parent: u32,
    rank: u8,
    value: Option<Ty>,
}

/// A suspension constraint `from ≤ to` (sync ≤ suspending), produced when a value of fn
/// type `from` flows where fn type `to` is expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuspFlow {
    pub from: Suspension,
    pub to: Suspension,
    pub span: Span,
}

/// Why two types do not unify. The types are the innermost mismatching pair, unresolved;
/// callers resolve them for messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnifyError {
    Mismatch {
        found: Ty,
        expected: Ty,
    },
    /// `Var(v)` would have to contain itself (an infinite type).
    Occurs {
        var: u32,
        ty: Ty,
    },
}

#[derive(Clone, Debug, Default)]
pub struct InferCtx {
    nodes: Vec<Node>,
    next_effect: u32,
    /// Suspension constraints for `kalam-effects`.
    pub flows: Vec<SuspFlow>,
    /// The span reported with flows recorded by the current `unify` call.
    pub span: Span,
}

impl InferCtx {
    pub fn new() -> InferCtx {
        InferCtx::default()
    }

    pub fn fresh(&mut self) -> Ty {
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            parent: id,
            rank: 0,
            value: None,
        });
        Ty::Var(id)
    }

    pub fn fresh_effect(&mut self) -> Suspension {
        self.next_effect += 1;
        Suspension::Var(self.next_effect - 1)
    }

    pub fn var_count(&self) -> usize {
        self.nodes.len()
    }

    /// Union-find root with path compression.
    pub fn find(&mut self, v: u32) -> u32 {
        let mut root = v;
        while self.nodes[root as usize].parent != root {
            root = self.nodes[root as usize].parent;
        }
        let mut at = v;
        while self.nodes[at as usize].parent != root {
            let next = self.nodes[at as usize].parent;
            self.nodes[at as usize].parent = root;
            at = next;
        }
        root
    }

    /// Follows variables until a non-variable or an unbound root.
    pub fn shallow(&mut self, ty: &Ty) -> Ty {
        let mut ty = ty.clone();
        while let Ty::Var(v) = ty {
            let root = self.find(v);
            match &self.nodes[root as usize].value {
                Some(t) => ty = t.clone(),
                None => return Ty::Var(root),
            }
        }
        ty
    }

    /// Substitutes every bound variable (unbound variables are returned as their root).
    pub fn resolve(&mut self, ty: &Ty) -> Ty {
        ty.map(&mut |t| match t {
            Ty::Var(_) => {
                let s = self.shallow(t);
                Some(if matches!(s, Ty::Var(_)) {
                    s
                } else {
                    self.resolve(&s)
                })
            }
            _ => None,
        })
    }

    fn occurs(&mut self, root: u32, ty: &Ty) -> bool {
        // `resolve` leaves only unbound roots, so comparing against `root` is exact.
        let ty = self.resolve(ty);
        ty.any(&mut |t| matches!(t, Ty::Var(v) if *v == root))
    }

    fn bind(&mut self, v: u32, ty: Ty) -> Result<(), UnifyError> {
        let root = self.find(v);
        if let Ty::Var(w) = ty {
            let other = self.find(w);
            if other == root {
                return Ok(());
            }
            // Union by rank. Both are unbound roots here.
            let (a, b) = (root as usize, other as usize);
            if self.nodes[a].rank < self.nodes[b].rank {
                self.nodes[a].parent = other;
            } else {
                self.nodes[b].parent = root;
                if self.nodes[a].rank == self.nodes[b].rank {
                    self.nodes[a].rank += 1;
                }
            }
            return Ok(());
        }
        if self.occurs(root, &ty) {
            return Err(UnifyError::Occurs { var: root, ty });
        }
        self.nodes[root as usize].value = Some(ty);
        Ok(())
    }

    /// Makes `found` and `expected` equal. Fn types additionally record `found ≤ expected`
    /// on their suspension bits.
    pub fn unify(&mut self, found: &Ty, expected: &Ty) -> Result<(), UnifyError> {
        let a = self.shallow(found);
        let b = self.shallow(expected);
        let mismatch = |a: &Ty, b: &Ty| {
            Err(UnifyError::Mismatch {
                found: a.clone(),
                expected: b.clone(),
            })
        };
        match (&a, &b) {
            (Ty::Error, _) | (_, Ty::Error) => Ok(()),
            (Ty::Var(v), _) => self.bind(*v, b.clone()),
            (_, Ty::Var(v)) => self.bind(*v, a.clone()),
            (Ty::List(x), Ty::List(y))
            | (Ty::Set(x), Ty::Set(y))
            | (Ty::Option(x), Ty::Option(y))
            | (Ty::Range(x), Ty::Range(y))
            | (Ty::Secret(x), Ty::Secret(y)) => self.unify(x, y),
            (Ty::Map(k1, v1), Ty::Map(k2, v2)) | (Ty::Result(k1, v1), Ty::Result(k2, v2)) => {
                self.unify(k1, k2)?;
                self.unify(v1, v2)
            }
            (Ty::Tuple(xs), Ty::Tuple(ys)) if xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys) {
                    self.unify(x, y)?;
                }
                Ok(())
            }
            (Ty::Adt(d1, xs), Ty::Adt(d2, ys)) if d1 == d2 && xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys) {
                    self.unify(x, y)?;
                }
                Ok(())
            }
            (
                Ty::Fn {
                    params: p1,
                    ret: r1,
                    suspension: s1,
                },
                Ty::Fn {
                    params: p2,
                    ret: r2,
                    suspension: s2,
                },
            ) if p1.len() == p2.len() => {
                // Parameters are contravariant in the bit sense, but types are invariant
                // (no subtyping, §9.6), so plain unification is exact for them.
                for (x, y) in p1.iter().zip(p2) {
                    self.unify(y, x)?;
                }
                self.unify(r1, r2)?;
                if s1 != s2 {
                    self.flows.push(SuspFlow {
                        from: *s1,
                        to: *s2,
                        span: self.span,
                    });
                }
                Ok(())
            }
            _ if a == b => Ok(()),
            _ => mismatch(&a, &b),
        }
    }

    /// Unifies only if the whole unification succeeds; on failure every binding it made
    /// is rolled back. Used for speculative hints (the expected type of a generic call's
    /// result), so a failed hint cannot leave half-bound variables behind.
    pub fn try_unify(&mut self, found: &Ty, expected: &Ty) -> bool {
        let (nodes, flows) = (self.nodes.clone(), self.flows.len());
        if self.unify(found, expected).is_ok() {
            return true;
        }
        self.nodes = nodes;
        self.flows.truncate(flows);
        false
    }

    /// Instantiates a scheme with fresh type and effect variables. Returns the type and
    /// the type arguments chosen for its generics (recorded for lowering).
    pub fn instantiate(&mut self, scheme: &Scheme) -> (Ty, Vec<Ty>) {
        let args: Vec<Ty> = scheme.generics.iter().map(|_| self.fresh()).collect();
        let effects: Vec<Suspension> = (0..scheme.effect_params)
            .map(|_| self.fresh_effect())
            .collect();
        let ty = scheme.ty.subst(&args);
        let ty = instantiate_effects(&ty, &effects);
        (ty, args)
    }
}

/// Replaces `Suspension::Param(i)` with `effects[i]` in every fn type.
pub fn instantiate_effects(ty: &Ty, effects: &[Suspension]) -> Ty {
    ty.map(&mut |t| match t {
        Ty::Fn {
            params,
            ret,
            suspension,
        } => {
            let suspension = match suspension {
                Suspension::Param(i) => effects
                    .get(*i as usize)
                    .copied()
                    .unwrap_or(Suspension::Suspending),
                s => *s,
            };
            Some(Ty::Fn {
                params: params
                    .iter()
                    .map(|p| instantiate_effects(p, effects))
                    .collect(),
                ret: Box::new(instantiate_effects(ret, effects)),
                suspension,
            })
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::GenericParam;

    #[test]
    fn unifies_through_chains_of_variables() {
        let mut cx = InferCtx::new();
        let (a, b, c) = (cx.fresh(), cx.fresh(), cx.fresh());
        cx.unify(&a, &b).unwrap();
        cx.unify(&b, &c).unwrap();
        cx.unify(&Ty::list(c.clone()), &Ty::list(Ty::Int)).unwrap();
        assert_eq!(cx.resolve(&a), Ty::Int);
        assert_eq!(cx.resolve(&Ty::option(b)), Ty::option(Ty::Int));
    }

    #[test]
    fn try_unify_rolls_back_partial_bindings() {
        let mut cx = InferCtx::new();
        let a = cx.fresh();
        // (a, Int) vs (Int, String): binds a := Int, then fails on Int vs String.
        let found = Ty::Tuple(vec![a.clone(), Ty::Int]);
        assert!(!cx.try_unify(&found, &Ty::Tuple(vec![Ty::Int, Ty::String])));
        assert_eq!(cx.resolve(&a), a, "the binding of `a` was undone");
        assert!(cx.try_unify(&a, &Ty::Bool));
        assert_eq!(cx.resolve(&a), Ty::Bool);
    }

    #[test]
    fn occurs_check_rejects_infinite_types() {
        let mut cx = InferCtx::new();
        let a = cx.fresh();
        let err = cx.unify(&a, &Ty::list(a.clone())).unwrap_err();
        assert!(matches!(err, UnifyError::Occurs { .. }));
        // Through an alias variable too.
        let (b, c) = (cx.fresh(), cx.fresh());
        cx.unify(&b, &c).unwrap();
        assert!(matches!(
            cx.unify(&c, &Ty::option(b)),
            Err(UnifyError::Occurs { .. })
        ));
    }

    #[test]
    fn mismatch_reports_innermost_pair_and_error_poisons() {
        let mut cx = InferCtx::new();
        let err = cx
            .unify(&Ty::list(Ty::Int), &Ty::list(Ty::Float))
            .unwrap_err();
        assert_eq!(
            err,
            UnifyError::Mismatch {
                found: Ty::Int,
                expected: Ty::Float
            }
        );
        assert!(cx.unify(&Ty::Error, &Ty::list(Ty::String)).is_ok());
        assert!(cx.unify(&Ty::Never, &Ty::Int).is_err());
    }

    #[test]
    fn fn_types_unify_structurally_and_record_suspension_flow() {
        let mut cx = InferCtx::new();
        let e = cx.fresh_effect();
        let found = Ty::func(vec![Ty::Int], Ty::Bool, Suspension::Sync);
        let expected = Ty::func(vec![Ty::Int], Ty::Bool, e);
        cx.unify(&found, &expected).unwrap();
        assert_eq!(cx.flows.len(), 1);
        assert_eq!(cx.flows[0].from, Suspension::Sync);
        assert_eq!(cx.flows[0].to, e);
    }

    #[test]
    fn instantiation_gives_fresh_variables_and_effects() {
        // fn map[T, U](xs: List[T], f: fn(T) -> U) -> List[U]
        let scheme = Scheme {
            generics: vec![
                GenericParam {
                    name: "T".into(),
                    bounds: vec![],
                },
                GenericParam {
                    name: "U".into(),
                    bounds: vec![],
                },
            ],
            effect_params: 1,
            ty: Ty::func(
                vec![
                    Ty::list(Ty::Param(0)),
                    Ty::func(vec![Ty::Param(0)], Ty::Param(1), Suspension::Param(0)),
                ],
                Ty::list(Ty::Param(1)),
                Suspension::Sync,
            ),
        };
        let mut cx = InferCtx::new();
        let (t1, args1) = cx.instantiate(&scheme);
        let (t2, _) = cx.instantiate(&scheme);
        assert_ne!(t1, t2, "each use gets its own variables");
        let Ty::Fn { params, ret, .. } = &t1 else {
            panic!()
        };
        cx.unify(&params[0], &Ty::list(Ty::Int)).unwrap();
        cx.unify(ret, &Ty::list(Ty::String)).unwrap();
        assert_eq!(cx.resolve(&args1[0]), Ty::Int);
        assert_eq!(cx.resolve(&args1[1]), Ty::String);
        let Ty::Fn { suspension, .. } = &params[1] else {
            panic!()
        };
        assert!(matches!(suspension, Suspension::Var(_)));
    }
}
