//! The type checker (spec §9–§14, ARCHITECTURE.md §8 and §8.1).
//!
//! Its input is the real output of name resolution: [`check_resolved`] takes
//! `kalam_resolve::Program` with the modules' skin-free ASTs, lowers them to a small HIR
//! ([`lower`]) and checks that ([`check_program`]).
//!
//! - Bidirectional checking with HM unification inside bodies; mandatory signatures on
//!   top-level, `impl` and local functions; monomorphic local `let` (§9.9). Polymorphism
//!   comes only from annotated generic signatures ([`Scheme`]); nothing is generalized.
//! - Literal typing from the expected type (§5.6) with the K0126 range check for the true
//!   64-bit `Int` (D18); no implicit conversions (K0601/K0602).
//! - Function types carry a suspension bit; fn-typed parameters are effect-polymorphic
//!   (§15.3). Typeck records suspension flows; `kalam-effects` solves them.
//! - Traits with coherence and the orphan rule; method resolution over inherent and trait
//!   methods with exactly one candidate (DR-27, K1103).
//! - Exhaustiveness and usefulness (Maranget, [`exhaust`]: K1001, K1002, K1003), must-use
//!   (K0730, K1303), `?` (§14.3: K1301, K1302).
//! - Capability types (§16), sink types for tagged literals (§17) and `Secret[T]` (§18)
//!   are built-in types whose trait exclusions follow §9.7.
//! - `Ty::Error` poisons follow-on errors, so the first root cause is reported alone.
//!
//! Modules: [`ty`] (types, schemes), [`unify`] (union-find inference core), [`traits`]
//! (trait/impl data, coherence, solving), [`adt`] (records and enums), [`hir`] (the
//! resolved program as HIR), [`lower`] (resolve's `Program` + AST → HIR), [`check`] (the
//! checker), [`exhaust`] (pattern usefulness), [`error`] (errors and codes), [`builder`]
//! (hand-built HIR for tests and tools), [`wf`] (well-formed types: `Map`/`Set` keys,
//! generic bounds), `conform` (impls against their traits, K1107) and `decls`
//! (declaration checks: well-formed signatures and fields, the entry point, derives).

pub mod adt;
pub mod builder;
pub mod check;
mod conform;
mod decls;
pub mod error;
pub mod exhaust;
pub mod hir;
pub mod lower;
mod sink_holes;
pub mod susp;
pub mod traits;
pub mod ty;
pub mod unify;
pub mod wf;

pub use check::{check_program, TryConversion, TypeckResults};
pub use error::{TypeError, TypeErrorKind};
pub use hir::HirProgram;
pub use susp::{SuspBit, Suspensions};
pub use traits::{never_implements, BuiltinTrait, TraitId, TraitRef};
pub use ty::{Capability, GenericParam, Scheme, SinkType, Suspension, Ty};

/// A type-checked program: its HIR and what the checker found.
#[derive(Debug)]
pub struct Checked {
    pub hir: HirProgram,
    pub results: TypeckResults,
}

impl Checked {
    /// Every type error as a diagnostic, in source order per file.
    pub fn diagnostics(&self) -> Vec<kalam_diagnostics::Diagnostic> {
        self.results.diagnostics()
    }
}

/// Type-checks a resolved program. `sources` are the modules given to
/// `kalam_resolve::resolve_program` (same file ids); only modules resolve loaded are
/// checked. Run it only when resolution reported no error (root cause first).
pub fn check_resolved(
    program: &kalam_resolve::Program,
    sources: &[kalam_resolve::SourceModule<'_>],
) -> Checked {
    on_big_stack(|| {
        let hir = lower::lower_program(program, sources);
        let results = check_program(&hir);
        Checked { hir, results }
    })
}

/// [`check_resolved`] for callers that only report: the diagnostics, with the HIR (a deep
/// tree) dropped on the big stack too.
pub fn diagnose(
    program: &kalam_resolve::Program,
    sources: &[kalam_resolve::SourceModule<'_>],
) -> Vec<kalam_diagnostics::Diagnostic> {
    on_big_stack(|| check_resolved(program, sources).diagnostics())
}

/// [`diagnose`] without checking the bodies of the stdlib's functions (their signatures are
/// still used): for checking a program against stdlib stubs whose own findings the caller
/// drops, such as a single-file `kalam check`.
pub fn diagnose_against_std(
    program: &kalam_resolve::Program,
    sources: &[kalam_resolve::SourceModule<'_>],
) -> Vec<kalam_diagnostics::Diagnostic> {
    on_big_stack(|| {
        let hir = lower::lower_program_with(program, sources, false);
        check_program(&hir).diagnostics()
    })
}

/// Type-checks like [`check_resolved`] (with the stdlib's function bodies only if
/// `std_bodies`), then runs `post` on the result. Everything, the drop of the HIR included,
/// happens on the checker's big stack.
///
/// The hook for passes that run after type checking but live in crates that depend on this
/// one: suspension inference in `kalam-effects` (spec §15) fills
/// [`TypeckResults::suspension`] here.
pub fn check_resolved_then<R: Send>(
    program: &kalam_resolve::Program,
    sources: &[kalam_resolve::SourceModule<'_>],
    std_bodies: bool,
    post: impl FnOnce(&mut Checked) -> R + Send,
) -> R {
    on_big_stack(|| {
        let hir = lower::lower_program_with(program, sources, std_bodies);
        let results = check_program(&hir);
        post(&mut Checked { hir, results })
    })
}

/// Stack size of the thread the checker runs on. The checker recurses once per nesting
/// level of an expression, with debug-build frames of several KiB, so the parser's limits
/// (operator chains of 1024 links) need more than the 1 MiB a Windows main thread has.
const CHECK_STACK: usize = 256 << 20;

thread_local! {
    /// Set on the checker's own thread: nested calls run inline.
    static ON_BIG_STACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` on a thread with a [`CHECK_STACK`] stack (only touched pages are committed),
/// or inline when already on one or when threads are unavailable (wasm32), like
/// `kalam_ast::stack::deeper`. A panic in `f` is re-raised on the calling thread.
fn on_big_stack<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    if ON_BIG_STACK.with(std::cell::Cell::get) {
        return f();
    }
    let mut slot = Some(f);
    let slot_ref = &mut slot;
    let moved = std::thread::scope(|s| {
        let spawned = std::thread::Builder::new()
            .name("kalam-typeck".into())
            .stack_size(CHECK_STACK)
            .spawn_scoped(s, move || {
                ON_BIG_STACK.with(|b| b.set(true));
                slot_ref.take().map(|f| f())
            });
        match spawned {
            Ok(handle) => match handle.join() {
                Ok(r) => r,
                Err(payload) => std::panic::resume_unwind(payload),
            },
            Err(_) => None,
        }
    });
    match moved {
        Some(r) => r,
        // The thread could not start, so `f` was never taken: run it here.
        None => match slot.take() {
            Some(f) => f(),
            None => unreachable!("f runs exactly once"),
        },
    }
}

#[cfg(test)]
mod tests;
