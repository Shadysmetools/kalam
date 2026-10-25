//! Name resolution: the module graph, scopes and imports, producing HIR.
//!
//! Rules implemented here (ARCHITECTURE.md §7):
//! - Module path = package name + file path without `src/` and without the skin suffix.
//!   `users.kl` next to `users.ar.kl` is K0301. Imports are absolute (`use notes/db`),
//!   globs are K1203, cycles are K1201 (Tarjan SCC over `use` edges).
//! - **One namespace per scope** (DR-11) and **no shadowing** (DR-28): reusing any visible
//!   name is K0405, or K0413 for a core name.
//! - **Skeletons** (spec §5.3, DR-04): `skeleton = UTS39_skeleton(arabic_fold(NFKC(id)))`.
//!   Two distinct identifiers with one skeleton in a scope, or exported from one module, are
//!   SEC0104. An unresolved use whose skeleton matches exactly one declaration is K0401 with
//!   a machine fix to the declared spelling. Uses are never folded silently.
//! - Visibility: `export`, `export opaque`, `internal/` (K0303 for private types in exported
//!   signatures). The prelude is imported implicitly (spec §13.1).
//! - Implicit members (`.Circle`) stay unresolved here; typeck resolves them against the
//!   expected type (K0803).
//!
//! Status: core HIR identifiers only.

use kalam_base::{FileId, FileSpan};
use kalam_diagnostics::Diagnostic;
use kalam_syntax::Cst;

/// A definition anywhere in the program (item, local, parameter, prelude item).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefId(pub u32);

/// `package/path/to/module`, without the skin suffix.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModulePath(pub String);

impl ModulePath {
    /// `src/web/views.ar.kl` in package `@team/notes` → `@team/notes/web/views`.
    pub fn from_file(package: &str, relative_path: &str) -> ModulePath {
        let path = relative_path.strip_prefix("src/").unwrap_or(relative_path);
        let stem = path
            .strip_suffix(".ar.kl")
            .or_else(|| path.strip_suffix(".franco.kl"))
            .or_else(|| path.strip_suffix(".kl"))
            .unwrap_or(path);
        ModulePath(format!("{package}/{stem}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefKind {
    Fn,
    Record,
    Enum,
    Variant,
    Alias,
    Trait,
    Const,
    ExternFn,
    ExternType,
    Local,
    Param,
    ModuleAlias,
    /// A core name or prelude item (spec §7.2, §13.1).
    Prelude,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub id: DefId,
    pub kind: DefKind,
    /// Exact NFC spelling as declared (never folded).
    pub name: String,
    pub span: FileSpan,
    pub exported: bool,
}

/// The resolved module: HIR items plus the definition table.
pub struct ResolvedModule {
    pub path: ModulePath,
    pub file: FileId,
    pub defs: Vec<Def>,
}

pub fn resolve_module(
    _path: ModulePath,
    _file: FileId,
    _cst: &Cst,
) -> (ResolvedModule, Vec<Diagnostic>) {
    todo!("name resolution (Phase 1)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_path_drops_src_and_skin_suffix() {
        assert_eq!(
            ModulePath::from_file("@team/notes", "src/web/views.ar.kl"),
            ModulePath("@team/notes/web/views".to_string())
        );
        assert_eq!(
            ModulePath::from_file("@team/notes", "src/internal/money.franco.kl"),
            ModulePath("@team/notes/internal/money".to_string())
        );
        assert_eq!(
            ModulePath::from_file("@team/notes", "src/main.kl"),
            ModulePath("@team/notes/main".to_string())
        );
    }
}
