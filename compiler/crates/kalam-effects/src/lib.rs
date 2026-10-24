//! Effects: suspension inference (no `async`, D12), `@sync` checks, capability sets and
//! audit counters (spec §15, §16; ARCHITECTURE.md §9).
//!
//! **Suspension.** A function suspends if it calls a base suspending function (stdlib I/O
//! primitives marked `@suspends`, externs with `@js(async: true)`) or calls a function value
//! whose suspension bit is set. The condition is monotone, so the fixpoint is reverse
//! reachability from the base set: one worklist pass, and mutual recursion needs no special
//! handling. Function-typed parameters get effect variables; a polymorphic function has at
//! most two instances, `f` (sync) and `f$s` (suspending). Function types stored in data are
//! suspending unless written `@sync fn(…)` (DR-13).
//!
//! **Capabilities.** For each exported function, the capability types reachable from its
//! parameters and captures, plus its package's extern effects, form its capability set
//! (spec §16.2). It is written to the `.kli` interface and printed by `kalam caps`.

use std::collections::VecDeque;

use kalam_typeck::Capability;

/// A call graph over the functions of a program (indices are function numbers).
/// A call through a function value whose bit is set is modelled as an edge to a node
/// that is in the base set.
#[derive(Clone, Debug, Default)]
pub struct CallGraph {
    /// `base[f]` is true for stdlib primitives and async externs.
    pub base: Vec<bool>,
    /// `calls[f]` lists the functions `f` calls directly.
    pub calls: Vec<Vec<usize>>,
}

impl CallGraph {
    pub fn add_fn(&mut self, base_suspending: bool) -> usize {
        self.base.push(base_suspending);
        self.calls.push(Vec::new());
        self.base.len() - 1
    }

    pub fn add_call(&mut self, caller: usize, callee: usize) {
        self.calls[caller].push(callee);
    }
}

/// Returns the suspension bit of every function.
pub fn infer_suspension(graph: &CallGraph) -> Vec<bool> {
    let n = graph.calls.len();
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (caller, callees) in graph.calls.iter().enumerate() {
        for &callee in callees {
            callers[callee].push(caller);
        }
    }
    let mut suspends = graph.base.clone();
    let mut work: Vec<usize> = (0..n).filter(|&f| suspends[f]).collect();
    while let Some(callee) = work.pop() {
        for &caller in &callers[callee] {
            if !suspends[caller] {
                suspends[caller] = true;
                work.push(caller);
            }
        }
    }
    suspends
}

/// The shortest call chain from `f` to a base suspending function, for K1401/K1402
/// messages ("`update` calls `save_score`, which calls `db.exec` (suspends)").
pub fn suspension_chain(graph: &CallGraph, f: usize) -> Option<Vec<usize>> {
    let n = graph.calls.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut seen = vec![false; n];
    let mut queue = VecDeque::from([f]);
    seen[f] = true;
    while let Some(current) = queue.pop_front() {
        if graph.base[current] {
            let mut chain = vec![current];
            let mut at = current;
            while let Some(p) = parent[at] {
                chain.push(p);
                at = p;
            }
            chain.reverse();
            return Some(chain);
        }
        for &next in &graph.calls[current] {
            if !seen[next] {
                seen[next] = true;
                parent[next] = Some(current);
                queue.push_back(next);
            }
        }
    }
    None
}

/// The capability types a function can exercise (spec §16.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    pub capabilities: Vec<Capability>,
    /// True if the function (transitively) uses `print` or `log.*` (deviation X9).
    pub console: bool,
    /// True if the function reaches an `extern` declaration.
    pub ffi: bool,
}

/// Counters reported by `kalam audit`, computed on HIR so they are identical in every skin
/// (04 L13).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditCounts {
    pub unsafe_calls: u32,
    pub externs: u32,
    pub secret_exposes: u32,
    pub positional_overrides: u32,
    pub discarded_results: u32,
    pub legacy_crypto_imports: u32,
    pub template_impls: u32,
    pub shell_commands: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspension_propagates_to_callers_including_recursion() {
        let mut g = CallGraph::default();
        let db_exec = g.add_fn(true); // stdlib primitive
        let save = g.add_fn(false);
        let update = g.add_fn(false);
        let pure_a = g.add_fn(false);
        let pure_b = g.add_fn(false);
        g.add_call(save, db_exec);
        g.add_call(update, save);
        g.add_call(pure_a, pure_b); // mutual recursion without I/O stays sync
        g.add_call(pure_b, pure_a);
        assert_eq!(infer_suspension(&g), vec![true, true, true, false, false]);
        assert_eq!(suspension_chain(&g, update), Some(vec![update, save, db_exec]));
        assert_eq!(suspension_chain(&g, pure_a), None);
    }
}
