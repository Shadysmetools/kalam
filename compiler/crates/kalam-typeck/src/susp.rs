//! Suspension results (spec §15), filled in by `kalam-effects` after type checking.
//!
//! The type checker records only suspension *constraints* ([`crate::unify::SuspFlow`]);
//! `kalam-effects` solves them over the call graph and stores the answer here, in
//! [`crate::TypeckResults::suspension`], so the JS backend and the tooling (LSP inlay
//! hints, hover, `.kli` interfaces) read it next to the types.
//!
//! **Effect polymorphism.** A bit is a [`SuspBit`]: it suspends if `always` is set, or if
//! any of the listed effect parameters of the enclosing declaration is instantiated with a
//! suspending function (spec §15.3: `map` suspends exactly when its `f` does). An effect
//! parameter index is the `i` of [`crate::Suspension::Param`] in the declaration's
//! [`crate::Scheme`]. A declaration therefore has at most two instances: the sync one
//! (every effect parameter sync) and the suspending one (`f$s`).

use std::collections::HashMap;

use kalam_resolve::DefId;

use crate::hir::{ExprId, LocalId};

/// A monotone suspension bit: `always ∨ ⋁ params`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SuspBit {
    /// Suspends in every instance.
    pub always: bool,
    /// Suspends when one of these effect parameters of the enclosing declaration is
    /// suspending. Sorted, without duplicates.
    pub params: Vec<u32>,
}

impl SuspBit {
    /// Never suspends.
    pub fn sync() -> SuspBit {
        SuspBit::default()
    }

    /// Suspends in every instance.
    pub fn suspending() -> SuspBit {
        SuspBit {
            always: true,
            params: Vec::new(),
        }
    }

    /// Suspends exactly when effect parameter `i` does.
    pub fn param(i: u32) -> SuspBit {
        SuspBit {
            always: false,
            params: vec![i],
        }
    }

    /// True if no instance suspends.
    pub fn is_sync(&self) -> bool {
        !self.always && self.params.is_empty()
    }

    /// True if some instance suspends (the suspending instance, for a polymorphic one).
    pub fn may_suspend(&self) -> bool {
        !self.is_sync()
    }

    /// Whether the instance in which `suspending(i)` tells each effect parameter's bit
    /// suspends.
    pub fn suspends_in(&self, suspending: &dyn Fn(u32) -> bool) -> bool {
        self.always || self.params.iter().any(|&i| suspending(i))
    }

    /// `self ∨= other`. Returns true if `self` changed.
    pub fn join(&mut self, other: &SuspBit) -> bool {
        let mut changed = false;
        if other.always && !self.always {
            self.always = true;
            changed = true;
        }
        for &p in &other.params {
            if let Err(at) = self.params.binary_search(&p) {
                self.params.insert(at, p);
                changed = true;
            }
        }
        changed
    }
}

/// The solved suspension of a whole program. Empty until `kalam-effects` has run
/// (`solved` is false then).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suspensions {
    /// True once `kalam-effects` has filled this in.
    pub solved: bool,
    /// Every function and method: the bit of its body, over its own effect parameters.
    /// Required trait methods (no body) join the bits of every impl of them in the program.
    pub fns: HashMap<DefId, SuspBit>,
    /// Effect parameters of a declaration that must be instantiated with a sync function,
    /// because the body passes them where `@sync` is required (K1401 at the call site).
    pub sync_params: HashMap<DefId, Vec<u32>>,
    /// Every call that may suspend, keyed by `(owner body, call expression)`, over the
    /// owner's effect parameters (the `⏸` inlay of spec §15.2). A call that is absent never
    /// suspends.
    pub calls: HashMap<(DefId, ExprId), SuspBit>,
    /// The bit of every lambda, keyed by `(owner body, lambda expression)`.
    pub lambdas: HashMap<(DefId, ExprId), SuspBit>,
    /// The bit of every local `fn`, keyed by `(owner body, its local)`.
    pub local_fns: HashMap<(DefId, LocalId), SuspBit>,
}

impl Suspensions {
    /// The bit of a function or method (sync if unknown).
    pub fn of_fn(&self, def: DefId) -> SuspBit {
        self.fns.get(&def).cloned().unwrap_or_default()
    }

    /// The bit of a call site (sync if it never suspends).
    pub fn of_call(&self, owner: DefId, call: ExprId) -> SuspBit {
        self.calls.get(&(owner, call)).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_is_a_monotone_union() {
        let mut b = SuspBit::param(2);
        assert!(b.join(&SuspBit::param(0)));
        assert!(!b.join(&SuspBit::param(2)));
        assert_eq!(b.params, vec![0, 2]);
        assert!(b.may_suspend() && !b.always);
        assert!(b.suspends_in(&|i| i == 0));
        assert!(!b.suspends_in(&|i| i == 1));
        assert!(b.join(&SuspBit::suspending()));
        assert!(b.suspends_in(&|_| false));
        assert!(SuspBit::sync().is_sync());
    }
}
