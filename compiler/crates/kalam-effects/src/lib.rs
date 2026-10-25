//! Effects: suspension inference (no `async`, D12), `@sync` checks, capability sets and
//! audit counters (spec §15, §16; ARCHITECTURE.md §9).
//!
//! **Suspension** ([`suspension`]). A function suspends if it calls a base suspending
//! function (stdlib I/O primitives marked `@suspends`, externs with `@js(async: true)`) or
//! calls a function value whose suspension bit is set. Function-typed parameters are effect
//! parameters, so a polymorphic function has at most two instances, `f` (sync) and `f$s`
//! (suspending). Function types stored in data are suspending unless written
//! `@sync fn(…)` (DR-13). The result is stored in `TypeckResults::suspension`; K1401,
//! K1402 and W0421 are reported. [`check_resolved`], [`diagnose`] and
//! [`diagnose_against_std`] are the type checker's entry points with this pass added.
//!
//! **Capabilities.** For each exported function, the capability types reachable from its
//! parameters and captures, plus its package's extern effects, form its capability set
//! (spec §16.2). It is written to the `.kli` interface and printed by `kalam caps`.

pub mod suspension;

use kalam_typeck::Capability;

pub use suspension::{analyze, base_attrs, BaseAttrs};

/// Type-checks a resolved program and infers suspension (`checked.results.suspension`).
/// Returns the checked program and the effect diagnostics (K1401, K1402, W0421), which
/// are empty when type checking reported an error.
pub fn check_resolved(
    program: &kalam_resolve::Program,
    sources: &[kalam_resolve::SourceModule<'_>],
) -> (kalam_typeck::Checked, Vec<kalam_diagnostics::Diagnostic>) {
    let attrs = base_attrs(program, sources);
    kalam_typeck::check_resolved_then(program, sources, true, |checked| {
        let diags = analyze(&checked.hir, &mut checked.results, &attrs);
        let checked = kalam_typeck::Checked {
            hir: std::mem::take(&mut checked.hir),
            results: std::mem::take(&mut checked.results),
        };
        (checked, diags)
    })
}

/// Every type and effect diagnostic of a resolved program, in source order per file
/// (`kalam_typeck::diagnose` plus this crate's pass).
pub fn diagnose(
    program: &kalam_resolve::Program,
    sources: &[kalam_resolve::SourceModule<'_>],
) -> Vec<kalam_diagnostics::Diagnostic> {
    diagnose_with(program, sources, true)
}

/// [`diagnose`] without checking the bodies of the stdlib's functions (their signatures and
/// attributes are still used), like `kalam_typeck::diagnose_against_std`.
pub fn diagnose_against_std(
    program: &kalam_resolve::Program,
    sources: &[kalam_resolve::SourceModule<'_>],
) -> Vec<kalam_diagnostics::Diagnostic> {
    diagnose_with(program, sources, false)
}

fn diagnose_with(
    program: &kalam_resolve::Program,
    sources: &[kalam_resolve::SourceModule<'_>],
    std_bodies: bool,
) -> Vec<kalam_diagnostics::Diagnostic> {
    let attrs = base_attrs(program, sources);
    kalam_typeck::check_resolved_then(program, sources, std_bodies, |checked| {
        let effects = analyze(&checked.hir, &mut checked.results, &attrs);
        let mut all = checked.diagnostics();
        all.extend(effects);
        // Stable: within one place, type errors keep coming first.
        all.sort_by_key(|d| (d.file, d.span.start, d.span.end));
        all
    })
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
mod tests;
