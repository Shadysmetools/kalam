//! The type checker (spec §9–§14, ARCHITECTURE.md §8 and §8.1).
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
//! - Exhaustiveness (K1001; shallow in this iteration), must-use (K0730, K1303).
//! - Capability types (§16), sink types for tagged literals (§17) and `Secret[T]` (§18)
//!   are built-in types whose trait exclusions follow §9.7.
//! - `Ty::Error` poisons follow-on errors, so the first root cause is reported alone.
//!
//! Modules: [`ty`] (types, schemes), [`unify`] (union-find inference core), [`traits`]
//! (trait/impl data, coherence, solving), [`adt`] (records and enums), [`hir`] (the
//! resolved-program interface this crate consumes), [`check`] (the checker), [`error`]
//! (errors and codes), [`builder`] (hand-built HIR for tests and tools).

pub mod adt;
pub mod builder;
pub mod check;
pub mod error;
pub mod hir;
pub mod traits;
pub mod ty;
pub mod unify;

pub use check::{check_program, TypeckResults};
pub use error::{TypeError, TypeErrorKind};
pub use hir::ResolvedProgram;
pub use traits::{never_implements, BuiltinTrait, TraitId, TraitRef};
pub use ty::{Capability, GenericParam, Scheme, SinkType, Suspension, Ty};

#[cfg(test)]
mod tests;
