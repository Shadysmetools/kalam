//! Type errors as data, and their mapping to spec §27 codes.
//!
//! A few failures have no §27 code yet. They use **provisional** codes, listed in
//! [`PROVISIONAL_CODES`], until the founder assigns codes (spec erratum proposal):
//! `K0600` type mismatch, `K0603` operator not defined for a type, `K1105` argument
//! count or label error, `K1106` impl violates coherence or the orphan rule.
//!
//! Args per code (what [`TypeError::to_diagnostic`] supplies; every variant mapped to a code
//! supplies at least the args listed for it in kalam-diagnostics' `EMITTER_ARGS`):
//! K0126 `literal`, `type`; K0418 `name`, `module`; K0420 `type`, `name`; K0601/K0602 `left`,
//! `right`; K0603 `type` (+ `op` or `trait`); K0730 `type`; K0803 `name`; K1001 `missing`;
//! K1004 `name`; K1103 `name`; K1104 `names`; K1105 `detail`; K1110 `name`; K1301
//! `found`, `expected`; K1601 `expected`, `found`; K1641 `found`; K0600 `expected`/`found`
//! or `type`.

use kalam_base::{FileId, Span};
use kalam_diagnostics::{Diagnostic, Fix};
use kalam_resolve::DefId;

use crate::ty::{SinkType, Ty};

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
    LiteralOutOfRange { literal: String, ty: Ty },
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
    /// An arm no value can reach (K1002).
    UnreachableArm,
    /// Refutable pattern in `let` or `for` (K1003).
    RefutableLet,
    /// The alternatives of an or-pattern bind different names, or one name at different
    /// types (K1004).
    OrPatternBindings { name: String },
    /// `Float` literal pattern (K1005).
    FloatPattern,
    /// Block-bodied function with a non-`()` return lacks `return` on some path (K1101).
    MissingReturn,
    /// More than one method candidate (K1103).
    AmbiguousMethod {
        name: String,
        candidates: Vec<DefId>,
    },
    /// Two adjacent parameters of one type, both passed by position, to an exported
    /// function (K1104).
    NeedsLabels { names: String },
    /// A closure uses a `var` of an enclosing scope (K1110).
    CapturesVar { name: String },
    /// No such field or method (K0420, the hallucinated-API guard).
    UnknownMember { ty: Ty, name: String },
    /// A field, constructor, `.with` or variant of an `export opaque` type used outside its
    /// module (K0418, "item is private", FOUNDER-QUEUE Q23). `module` is the type's module
    /// path: the catalog template names it, as resolve's K0418 does.
    PrivateMember { name: String, module: String },
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
    /// `e?` cannot turn the error type `found` into the function's `expected` (K1301).
    TryCannotConvert { found: Ty, expected: Ty },
    /// `?` on an `Option` in a function that does not return `Option` (K1302).
    TryOptionInResult,
    /// A `path"…"` hole that is not a `PathSegment` (K1641).
    PathHole { found: Ty },
    /// `f[T](x)`: type arguments written at a call site (K0922, DR-25).
    ExplicitTypeArgs,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeError {
    pub kind: TypeErrorKind,
    pub file: FileId,
    pub span: Span,
    /// Fix-its (machine-applicable or suggestions), in the file's own text.
    pub fixes: Vec<Fix>,
}

impl TypeError {
    pub fn new(kind: TypeErrorKind, file: FileId, span: Span) -> TypeError {
        TypeError {
            kind,
            file,
            span,
            fixes: Vec::new(),
        }
    }

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
            K::UnreachableArm => "K1002",
            K::RefutableLet => "K1003",
            K::OrPatternBindings { .. } => "K1004",
            K::FloatPattern => "K1005",
            K::MissingReturn => "K1101",
            K::AmbiguousMethod { .. } => "K1103",
            K::NeedsLabels { .. } => "K1104",
            K::CapturesVar { .. } => "K1110",
            K::UnknownMember { .. } => "K0420",
            K::PrivateMember { .. } => "K0418",
            K::RecordNeedsNamedArgs => "K0416",
            K::PrintArity => "K0916",
            K::BadArguments { .. } => "K1105",
            K::EncodeSecret => "K0612",
            K::OrphanImpl | K::OverlappingImpl { .. } => "K1106",
            K::TryCannotConvert { .. } => "K1301",
            K::TryOptionInResult => "K1302",
            K::PathHole { .. } => "K1641",
            K::ExplicitTypeArgs => "K0922",
        }
    }

    /// Converts to a diagnostic. `name` renders ADT names in messages.
    pub fn to_diagnostic(&self, name: &dyn Fn(DefId) -> String) -> Diagnostic {
        use TypeErrorKind as K;
        let mut d = Diagnostic::new(self.code(), self.file, self.span);
        let show = |t: &Ty| t.display(name);
        d = match &self.kind {
            K::Mismatch { expected, found } => {
                d.arg("expected", show(expected)).arg("found", show(found))
            }
            K::TryCannotConvert { expected, found } => {
                d.arg("found", show(found)).arg("expected", show(expected))
            }
            K::MixedNumeric { left, right } | K::BadEquality { left, right } => {
                d.arg("left", show(left)).arg("right", show(right))
            }
            K::NoOperator { op, ty } => d.arg("op", *op).arg("type", show(ty)),
            K::MissingTrait { ty, trait_name } => {
                d.arg("type", show(ty)).arg("trait", trait_name.clone())
            }
            K::LiteralOutOfRange { literal, ty } => {
                d.arg("literal", literal.clone()).arg("type", show(ty))
            }
            K::InfiniteType { ty }
            | K::UnusedValue { ty }
            | K::NotCallable { ty }
            | K::NotIterable { ty } => d.arg("type", show(ty)),
            K::PathHole { found } => d.arg("found", show(found)),
            K::ImplicitMemberNeedsType { name }
            | K::OrPatternBindings { name }
            | K::CapturesVar { name } => d.arg("name", name.clone()),
            // Both in backticks, like resolve's K0418 (`walk.rs` `private_diag`).
            K::PrivateMember { name, module } => d
                .arg("name", format!("`{name}`"))
                .arg("module", format!("`{module}`")),
            K::UnknownMember { ty, name } => d.arg("type", show(ty)).arg("name", name.clone()),
            K::AmbiguousMethod { name, .. } => d.arg("name", name.clone()),
            K::NeedsLabels { names } => d.arg("names", names.clone()),
            K::NonExhaustive { missing } => d.arg("missing", missing.join(", ")),
            K::BadArguments { detail } => d.arg("detail", detail.clone()),
            _ => d,
        };
        for fix in &self.fixes {
            d = d.fix(fix.clone());
        }
        d
    }
}

/// The tag that builds a sink type from a literal (`Sql` ← `sql"…"`), for K1601's fix.
pub fn tag_of(sink: SinkType) -> Option<&'static str> {
    Some(match sink {
        SinkType::Sql => "sql",
        SinkType::Html => "html",
        SinkType::Command => "cmd",
        SinkType::Url => "url",
        SinkType::Regex => "regex",
        // A `path"…"` hole must be a `PathSegment` (K1641), so a `String` piece can never
        // become a hole: no mechanical rewrite exists.
        SinkType::Path | SinkType::PathSegment => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_given_string_is_k1601() {
        let e = TypeError::new(
            TypeErrorKind::Mismatch {
                expected: Ty::Sink(SinkType::Sql),
                found: Ty::String,
            },
            FileId(0),
            Span::new(0, 1),
        );
        assert_eq!(e.code(), "K1601");
        let d = e.to_diagnostic(&|_| String::new());
        assert_eq!(d.args[0], ("expected", "Sql".to_string()));
    }

    #[test]
    fn new_codes_follow_spec_section_27() {
        let at = |kind| TypeError::new(kind, FileId(0), Span::new(0, 1));
        assert_eq!(at(TypeErrorKind::UnreachableArm).code(), "K1002");
        assert_eq!(
            at(TypeErrorKind::OrPatternBindings { name: "x".into() }).code(),
            "K1004"
        );
        assert_eq!(at(TypeErrorKind::TryOptionInResult).code(), "K1302");
        assert_eq!(
            at(TypeErrorKind::PathHole { found: Ty::String }).code(),
            "K1641"
        );
        let k0126 = at(TypeErrorKind::LiteralOutOfRange {
            literal: "9223372036854775808".into(),
            ty: Ty::Int,
        })
        .to_diagnostic(&|_| String::new());
        assert_eq!(
            k0126.args,
            vec![
                ("literal", "9223372036854775808".to_string()),
                ("type", "Int".to_string())
            ]
        );
    }
}
