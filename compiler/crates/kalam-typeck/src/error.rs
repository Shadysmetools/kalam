//! Type errors as data, and their mapping to spec §27 codes.
//!
//! A few failures have no §27 code yet. They use **provisional** codes, listed in
//! [`PROVISIONAL_CODES`], until the founder assigns codes (spec erratum proposal):
//! `K0600` type mismatch, `K0603` operator not defined for a type, `K1105` argument
//! count or label error, `K1106` impl violates coherence or the orphan rule.

use kalam_base::{FileId, Span};
use kalam_diagnostics::Diagnostic;
use kalam_resolve::DefId;

use crate::ty::Ty;

/// Codes this crate emits that spec §27 does not list yet.
pub const PROVISIONAL_CODES: &[&str] = &["K0600", "K0603", "K1105", "K1106"];

#[derive(Clone, Debug, PartialEq)]
pub enum TypeErrorKind {
    /// Expected one type, found another (K1601 when a sink type meets `String`).
    Mismatch { expected: Ty, found: Ty },
    /// An infinite type would be needed.
    InfiniteType { ty: Ty },
    /// `Int + Float` and friends (K0601).
    MixedNumeric { left: Ty, right: Ty },
    /// `==` between different types, or on a type that is not `Eq` (K0602).
    BadEquality { left: Ty, right: Ty },
    /// The operator is not defined for this type (`/` on `Decimal`, `<` on a non-`Ord`).
    NoOperator { op: &'static str, ty: Ty },
    /// A trait bound or requirement does not hold (`print` needs `Show`).
    MissingTrait { ty: Ty, trait_name: String },
    /// Literal outside its type's range (K0126).
    LiteralOutOfRange { ty: Ty },
    /// `s[i]` on a `String` (K0650).
    StringIndex,
    /// Assignment to something that is not a `var` place (K0710).
    NotAssignable,
    /// Unused non-`()` value (K0730).
    UnusedValue { ty: Ty },
    /// Unused `Result` (K1303).
    UnusedResult,
    /// Value-position `if` without `else` (K0731).
    IfWithoutElse,
    /// Lambda parameter types unknown (K0801).
    LambdaNeedsTypes,
    /// `[]` or `{}` without an expected type (K0802).
    EmptyCollectionNeedsType,
    /// `.Circle` without an expected type (K0803).
    ImplicitMemberNeedsType { name: String },
    /// Non-exhaustive `match` (K1001), with the missing cases as text.
    NonExhaustive { missing: Vec<String> },
    /// Refutable pattern in `let` or `for` (K1003).
    RefutableLet,
    /// `Float` literal pattern (K1005).
    FloatPattern,
    /// Block-bodied function with a non-`()` return lacks `return` on some path (K1101).
    MissingReturn,
    /// More than one method candidate (K1103).
    AmbiguousMethod {
        name: String,
        candidates: Vec<DefId>,
    },
    /// No such field or method (K0420, the hallucinated-API guard).
    UnknownMember { ty: Ty, name: String },
    /// Record construction with a positional argument (K0416).
    RecordNeedsNamedArgs,
    /// `print(a, b)` (K0916).
    PrintArity,
    /// Wrong number of arguments, unknown label, missing or repeated argument.
    BadArguments { detail: String },
    /// The callee is not a function.
    NotCallable { ty: Ty },
    /// `for` over something that is not iterable.
    NotIterable { ty: Ty },
    /// `@derive(Encode)` on a type with a `Secret` field (K0612).
    EncodeSecret,
    /// Trait impl outside the trait's and the type's module.
    OrphanImpl,
    /// Two impls of one trait overlap.
    OverlappingImpl { other: DefId },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeError {
    pub kind: TypeErrorKind,
    pub file: FileId,
    pub span: Span,
}

impl TypeError {
    pub fn code(&self) -> &'static str {
        use TypeErrorKind as K;
        match &self.kind {
            K::Mismatch {
                expected: Ty::Sink(_),
                found: Ty::String,
            } => "K1601",
            K::Mismatch { .. } | K::InfiniteType { .. } => "K0600",
            K::NotCallable { .. } | K::NotIterable { .. } => "K0600",
            K::MixedNumeric { .. } => "K0601",
            K::BadEquality { .. } => "K0602",
            K::NoOperator { .. } | K::MissingTrait { .. } => "K0603",
            K::LiteralOutOfRange { .. } => "K0126",
            K::StringIndex => "K0650",
            K::NotAssignable => "K0710",
            K::UnusedValue { .. } => "K0730",
            K::UnusedResult => "K1303",
            K::IfWithoutElse => "K0731",
            K::LambdaNeedsTypes => "K0801",
            K::EmptyCollectionNeedsType => "K0802",
            K::ImplicitMemberNeedsType { .. } => "K0803",
            K::NonExhaustive { .. } => "K1001",
            K::RefutableLet => "K1003",
            K::FloatPattern => "K1005",
            K::MissingReturn => "K1101",
            K::AmbiguousMethod { .. } => "K1103",
            K::UnknownMember { .. } => "K0420",
            K::RecordNeedsNamedArgs => "K0416",
            K::PrintArity => "K0916",
            K::BadArguments { .. } => "K1105",
            K::EncodeSecret => "K0612",
            K::OrphanImpl | K::OverlappingImpl { .. } => "K1106",
        }
    }

    /// Converts to a diagnostic. `name` renders ADT names in messages.
    pub fn to_diagnostic(&self, name: &dyn Fn(DefId) -> String) -> Diagnostic {
        use TypeErrorKind as K;
        let d = Diagnostic::new(self.code(), self.file, self.span);
        let show = |t: &Ty| t.display(name);
        match &self.kind {
            K::Mismatch { expected, found } => {
                d.arg("expected", show(expected)).arg("found", show(found))
            }
            K::MixedNumeric { left, right } | K::BadEquality { left, right } => {
                d.arg("left", show(left)).arg("right", show(right))
            }
            K::NoOperator { op, ty } => d.arg("op", *op).arg("type", show(ty)),
            K::MissingTrait { ty, trait_name } => {
                d.arg("type", show(ty)).arg("trait", trait_name.clone())
            }
            K::InfiniteType { ty }
            | K::LiteralOutOfRange { ty }
            | K::UnusedValue { ty }
            | K::NotCallable { ty }
            | K::NotIterable { ty } => d.arg("type", show(ty)),
            K::ImplicitMemberNeedsType { name } => d.arg("name", name.clone()),
            K::UnknownMember { ty, name } => d.arg("type", show(ty)).arg("name", name.clone()),
            K::AmbiguousMethod { name, .. } => d.arg("name", name.clone()),
            K::NonExhaustive { missing } => d.arg("missing", missing.join(", ")),
            K::BadArguments { detail } => d.arg("detail", detail.clone()),
            _ => d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::SinkType;

    #[test]
    fn sink_given_string_is_k1601() {
        let e = TypeError {
            kind: TypeErrorKind::Mismatch {
                expected: Ty::Sink(SinkType::Sql),
                found: Ty::String,
            },
            file: FileId(0),
            span: Span::new(0, 1),
        };
        assert_eq!(e.code(), "K1601");
        let d = e.to_diagnostic(&|_| String::new());
        assert_eq!(d.args[0], ("expected", "Sql".to_string()));
    }
}
