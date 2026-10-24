//! The type checker (spec §9–§14, ARCHITECTURE.md §8).
//!
//! - Bidirectional checking with HM unification inside bodies; mandatory signatures on
//!   top-level, `impl` and local functions; monomorphic local `let` (§9.9).
//! - Literal typing from the expected type (§5.6); no implicit conversions (K0601/K0602).
//! - Traits with coherence and the orphan rule; method resolution over inherent and trait
//!   methods with exactly one candidate (DR-27, K1103).
//! - Maranget exhaustiveness and usefulness (K1001, K1002); must-use (K0730, K1303).
//! - `?` conversion rules (§14.3, K1301/K1302); sink typing via `kalam-sinks` (K16xx).
//! - Capability types have no constructor, no `Decode`, `Eq` or `Hash` (K1501, §16).
//! - `Ty::Error` poisons follow-on errors, so the first root cause is reported alone.
//!
//! Status: type representation and built-in trait facts.

use kalam_resolve::DefId;

/// Capability types (spec §16.1). They have no public constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Capability {
    World,
    Net,
    Fs,
    Env,
    Clock,
    Rand,
    Proc,
    Stdin,
    Screen,
}

/// Safe string kinds (spec §17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SinkType {
    Sql,
    Html,
    Command,
    Url,
    Path,
    PathSegment,
    Regex,
}

/// The suspension bit of a function type (spec §15.3). `Var` is an effect variable on a
/// function-typed parameter, instantiated per call site by `kalam-effects`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Suspension {
    Sync,
    Suspending,
    Var(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    BigInt,
    Float,
    Decimal,
    Bool,
    String,
    Bytes,
    Unit,
    Never,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Tuple(Vec<Ty>),
    Fn {
        params: Vec<Ty>,
        ret: Box<Ty>,
        suspension: Suspension,
    },
    /// A user record, enum or alias target, with type arguments.
    Adt(DefId, Vec<Ty>),
    Capability(Capability),
    Sink(SinkType),
    Secret(Box<Ty>),
    AnyError,
    /// An opaque JS value from `extern` code (spec §19.2).
    JsValue,
    /// A type parameter of the enclosing generic item.
    Param(u32),
    /// An inference variable.
    Var(u32),
    /// Poison: unifies with everything and suppresses follow-on diagnostics.
    Error,
}

/// Built-in traits (spec §9.7 table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinTrait {
    Eq,
    Hash,
    Ord,
    Show,
    Encode,
    Decode,
    Row,
    Message,
    Error,
}

/// The "never implemented by" column of spec §9.7, for built-in types. User types are
/// answered by derives and impls; component types are checked recursively by the caller.
pub fn never_implements(ty: &Ty, tr: BuiltinTrait) -> bool {
    use BuiltinTrait as T;
    match ty {
        Ty::Float => matches!(tr, T::Eq | T::Hash | T::Ord),
        Ty::Fn { .. } => matches!(tr, T::Eq | T::Hash | T::Show | T::Encode | T::Decode),
        Ty::Capability(_) => matches!(tr, T::Eq | T::Hash | T::Encode | T::Decode | T::Row),
        Ty::Secret(_) => matches!(tr, T::Eq | T::Hash | T::Ord | T::Encode),
        Ty::Sink(SinkType::Sql | SinkType::Html | SinkType::Command | SinkType::Path) => {
            matches!(tr, T::Decode | T::Row)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_trait_exclusions() {
        assert!(never_implements(&Ty::Float, BuiltinTrait::Eq));
        assert!(!never_implements(&Ty::Decimal, BuiltinTrait::Eq));
        assert!(never_implements(&Ty::Secret(Box::new(Ty::String)), BuiltinTrait::Encode));
        assert!(!never_implements(&Ty::Secret(Box::new(Ty::String)), BuiltinTrait::Show));
        assert!(never_implements(&Ty::Capability(Capability::Net), BuiltinTrait::Decode));
        assert!(never_implements(&Ty::Sink(SinkType::Sql), BuiltinTrait::Decode));
        assert!(!never_implements(&Ty::Sink(SinkType::Url), BuiltinTrait::Decode));
    }
}
