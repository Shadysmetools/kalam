//! Records and enums after their declarations are lowered to [`Ty`] (spec §9.4, §9.5).

use kalam_resolve::{DefId, ModulePath};

use crate::traits::BuiltinTrait;
use crate::ty::{GenericParam, Ty};

#[derive(Clone, Debug, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub ty: Ty,
    /// Fields with a default may be omitted at construction (§9.4).
    pub has_default: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantField {
    /// `None` for a positional field.
    pub label: Option<String>,
    pub ty: Ty,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantDef {
    pub def: DefId,
    pub name: String,
    /// Empty for a payload-free variant.
    pub fields: Vec<VariantField>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AdtKind {
    Record(Vec<FieldDef>),
    Enum(Vec<VariantDef>),
    /// An `extern type` (`Db`, `Cell[T]`, `Instant`): no visible fields or variants.
    Extern,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdtDef {
    pub def: DefId,
    pub name: String,
    /// The defining module: the orphan rule's "module that defines `T`" (§9.7).
    pub module: ModulePath,
    pub generics: Vec<GenericParam>,
    pub kind: AdtKind,
    /// `export opaque`: fields, construction and `.with` (and an enum's variants) are
    /// private to `module` (§9.4, §9.5).
    pub opaque: bool,
    /// Explicit `@derive(…)` traits (needed for `Ord`, `Encode`, `Decode`, `Row`, `Message`).
    pub derives: Vec<BuiltinTrait>,
    /// Derives already reported at the derive site because a field does not implement the
    /// trait (`@derive(Ord)` on a record with a `Float` field). The type is taken to
    /// implement them, so each use does not report the same root cause again.
    pub assumed: Vec<BuiltinTrait>,
}

impl AdtDef {
    /// Every field and payload type, with the type arguments substituted. Structural
    /// derivation (`Eq`, `Hash`, `Show`, …) holds when it holds for all of them.
    pub fn component_types(&self, args: &[Ty]) -> Vec<Ty> {
        match &self.kind {
            AdtKind::Record(fields) => fields.iter().map(|f| f.ty.subst(args)).collect(),
            AdtKind::Enum(variants) => variants
                .iter()
                .flat_map(|v| v.fields.iter().map(|f| f.ty.subst(args)))
                .collect(),
            AdtKind::Extern => Vec::new(),
        }
    }

    pub fn field(&self, name: &str) -> Option<(usize, &FieldDef)> {
        match &self.kind {
            AdtKind::Record(fields) => fields.iter().enumerate().find(|(_, f)| f.name == name),
            AdtKind::Enum(_) | AdtKind::Extern => None,
        }
    }

    pub fn variant(&self, name: &str) -> Option<(usize, &VariantDef)> {
        match &self.kind {
            AdtKind::Enum(vs) => vs.iter().enumerate().find(|(_, v)| v.name == name),
            AdtKind::Record(_) | AdtKind::Extern => None,
        }
    }

    pub fn variant_by_def(&self, def: DefId) -> Option<(usize, &VariantDef)> {
        match &self.kind {
            AdtKind::Enum(vs) => vs.iter().enumerate().find(|(_, v)| v.def == def),
            AdtKind::Record(_) | AdtKind::Extern => None,
        }
    }
}
