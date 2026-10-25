//! Name resolution: the module graph, scopes, imports and visibility (spec §13, §10.1,
//! §5.3, §5.4; ARCHITECTURE.md §7). It runs on the skin-free AST (`kalam_ast::Ast`), so all
//! three skins resolve identically.
//!
//! # What it checks
//!
//! - **Module graph** (§13.1): one file is one module, and its path is
//!   `<package>/<path without src/ and without the skin suffix>` ([`ModulePath::from_file`]).
//!   Two skin files for one module (`users.kl` next to `users.ar.kl`) are K0301. `use`
//!   paths are absolute; an import cycle is K1201. A `use` of a module that should exist
//!   but does not (an in-package path in a whole-package check, or a `std/…` path when the
//!   stdlib is loaded) is K0401 on the path.
//! - **Visibility** (§13.2): only `export`ed items (and `export use` re-exports) can be
//!   imported or reached as `module.name`. A module item without `export` reached that way
//!   is K0418 "item is private", with the item's module path (FOUNDER-QUEUE Q23); the
//!   imported name is still bound to the item, so its uses do not cascade. A name the module
//!   does not have at all is K0401. A failed `export use` (a private item, a missing name, a
//!   missing module) is reported once, at the `export use`; the name is still exported (as
//!   the private item or an opaque stand-in), so modules importing it from there get nothing
//!   more. An exported signature that names a private type of its own module is K0303.
//! - **One namespace per scope** (DR-11) and **no shadowing** (DR-28, §10.1): a declaration
//!   that reuses a name visible at that point (an enclosing local or parameter, a module
//!   item, an import or a prelude name) is K0405; declaring a core name (§7.2) is K0413
//!   (`fn main` at module level is the one allowed core-name declaration). That covers item
//!   names, local `fn`s and bare core names in `let`/`for`/`case` patterns (`let Int = 1`);
//!   the positions where the AST keeps a plain identifier (`var`, parameters, `as`, list
//!   rest) are the parser's K0413. Only the file's own skin spelling is a core name: another
//!   skin's (`let عدد = 1` in an English file) is an ordinary name (FOUNDER-QUEUE Q21). A backticked
//!   word spelled like a core name (`` `print` `` in an English file) is an ordinary
//!   identifier: the same AST in another skin has it without backticks, and every check
//!   here gives the same verdict in every skin (parity P1). For the same reason the
//!   SEC0104 check against core names uses their spellings in all three skins.
//! - `export opaque` on anything but a `type` or an `enum` is K0302; a record field named
//!   `with` is K0417 (§9.4: `with` is every record's update method).
//! - **Skeletons** (§5.3, DR-04, DECISIONS D19): two *distinct* names with the same
//!   [`skeleton()`] visible in one scope are SEC0104, reported once at the later declaration
//!   with a label at the earlier one. The fold covers the Persian/Urdu letters ی ک ە ھ ۀ.
//! - **Undefined names**, decided in this order for a name with no visible declaration:
//!   1. another skin's keyword or core name (`print` in an Arabic file): K0201 with a
//!      machine fix to the file's own spelling. This is the "undefined name" half of spec
//!      §5.4, now decided by real scopes (the lexer's file-local approximation, FOUNDER-QUEUE
//!      Q9, still reports the cases it can see; interpolation holes are covered here);
//!   2. a variant of an enum declared in another module, written without its dot
//!      (`Circle(r)`): K0917 with a machine fix inserting `.` (same-file bare variants are
//!      the parser's);
//!   3. exactly one visible declaration with the same skeleton (`کتاب` used, `كتاب`
//!      declared): K0401 with a machine fix to the declared spelling. Uses are never folded
//!      silently (FOUNDER-QUEUE Q7);
//!   4. an Arabic name glued to `و` (`وصح`) whose rest is a visible name, keyword or core
//!      name: K0401 with the "did you mean `و صح`" suggestion (§5.4 last bullet);
//!   5. otherwise K0401, with a suggestion by edit distance over spellings and skeletons.
//! - **Implicit members** (`.Circle`, §9.5): left to the type checker, which needs the
//!   expected type. Resolve records every variant in the program with that exact name
//!   ([`Program::implicit_candidates`]). When the whole program is known (stdlib loaded, no
//!   opaque modules) and no enum has such a variant, it reports K0401 and records an empty
//!   candidate list, so typeck must not report the name again.
//!
//! Not done here: glob imports (`use a.*`, K1203) and module-level `var` (K1202) are the
//! parser's (it reads them as `use a` and `let`); `internal/` visibility needs dependency
//! packages, which are opaque for now.
//!
//! # Opaque modules
//!
//! A module whose source is not part of the input is **opaque**: importing it binds
//! [`DefKind::Module`] / [`DefKind::Import`] symbols, and its members are never checked.
//! This is how a single file is checked ([`resolve_file`]: every import is opaque) and how
//! `std/…` behaves until a stdlib directory is loaded ([`Options::std_loaded`]).
//!
//! # The stdlib
//!
//! Stdlib modules ([`SourceModule::std`]) come from a directory of signature stubs
//! (`stdlib/src/http.kl` is `std/http`). When `std/prelude` is among them, its exports *are*
//! the prelude: they replace the built-in prelude symbols of the same name (so `Ordering`,
//! `Cell` and `Show` resolve to their stub declarations), and its core-named declarations
//! (`export extern type Int`) are the core names' stubs ([`Program::core_stub`]). Stdlib
//! modules may declare core names and prelude names, and a stdlib parameter may share its
//! name with a module item (parameter names are API labels: `serve(router: …)` next to
//! `fn router()`); user code gets the strict rules.
//!
//! # Output: the resolved program (the API the type checker consumes)
//!
//! [`resolve_program`] returns a [`Program`] and the diagnostics.
//!
//! - **Symbols.** Every declaration gets a [`DefId`] (alias [`SymbolId`]), an index into
//!   [`Program::defs`]. Ids are deterministic: the 19 core names (in `Concept` order), then
//!   the prelude items (in [`prelude::PRELUDE`] order), then each loaded module in path
//!   order with its items in source order (an item's variants, fields and methods right
//!   after it), then the prelude modules `log` and `json` when they are not loaded, then
//!   opaque modules and their imports in import order, then locals in walk order. The same
//!   input always gives the same ids.
//! - **Uses.** Every name occurrence that refers to something is a key in [`Program::uses`]:
//!   a [`NameRef`] (file + the identifier's byte span) mapped to a [`Res`]. This covers
//!   expression names, type names, pattern paths, `module.item`, `Enum.Variant`,
//!   `Type.assoc_fn` (inherent impls), interpolation holes and `use` items. Labels, record
//!   field accesses and method calls (`x.f`) are not names here: typeck resolves them.
//!   `Res::Error` marks a use that already has a diagnostic (treat it as `Ty::Error`).
//! - **Declarations.** Every declaring occurrence (item names, variants, fields, methods,
//!   generic parameters, parameters, `let`/`var`/`for`/`case`/lambda bindings, local `fn`s)
//!   is a key in [`Program::decls`], mapped to its [`DefId`].
//! - **Tables.** [`Program::variants_of`] (an enum's variants in order),
//!   [`Program::fields_of`] (a record's or variant's fields), [`Program::methods_of`] (the
//!   inherent methods of a type, and a trait's methods), [`Program::implicit_candidates`]
//!   (for `.V`), [`Program::core`] and [`Program::prelude`].
//! - [`Def::parent`] links a variant to its enum, a field to its record or variant, a method
//!   to its impl (and the impl to nothing), a trait method to its trait, and a local or
//!   parameter to the item that owns its body.
//!
//! Typical consumer loop: walk the AST of each module; for a name at span `s` look up
//! `program.use_at(file, s)`; for a binding look up `program.decl_at(file, s)`; then use
//! [`Program::def`] for its kind, module, parent and visibility.

mod prelude_impl;
mod skeleton_table;
mod suggest;
mod walk;

pub mod prelude {
    //! The implicit prelude (spec §13.1): names visible in every module.
    pub use crate::prelude_impl::{PRELUDE, PRELUDE_MODULES};
}
pub mod skeleton;

use std::collections::BTreeMap;

use kalam_ast::Ast;
use kalam_base::{FileId, FileSpan, Skin, Span};
use kalam_diagnostics::Diagnostic;
use kalam_skins::Concept;

pub use skeleton::skeleton;

/// A definition anywhere in the program (item, local, parameter, prelude item).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefId(pub u32);

/// The name the M2 task and the type checker use for a [`DefId`].
pub type SymbolId = DefId;

/// Index into [`Program::modules`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(pub u32);

/// `package/path/to/module`, without the skin suffix.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last path segment: the name `use a/b` binds.
    pub fn last_segment(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }
}

/// The file id used in the spans of built-in symbols (core names, prelude items), which
/// have no source.
pub const BUILTIN_FILE: FileId = FileId(u32::MAX);

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
    /// A prelude item that is not a core name: `BigInt`, `Cell`, `Net`, `Show`, … (§13.1).
    Prelude,
    /// A core name (§7.2): `Int`, `print`, `Some`, `World`, … Spelled per skin, one symbol
    /// in every skin.
    Core(Concept),
    /// A module. `use std/http` binds `http` to it; `log` and `json` are prelude modules.
    Module,
    /// A name imported from an opaque module (see the crate docs): nothing more is known.
    Import,
    /// A generic parameter of an item, impl or method.
    GenericParam,
    /// A record field or a labelled variant field.
    Field,
    /// A function in an `impl` block.
    Method,
    /// A function in a `trait` block.
    TraitMethod,
    /// An `impl` block (it has no name; `Def::name` is empty).
    Impl,
    /// A local `fn` declared in a body.
    LocalFn,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub id: DefId,
    pub kind: DefKind,
    /// Exact NFC spelling as declared (never folded). Core names use their English
    /// spelling; built-in symbols have no source span (see [`BUILTIN_FILE`]).
    pub name: String,
    /// The declaring identifier's span.
    pub span: FileSpan,
    /// `export` (variants of an exported, non-opaque enum count as exported).
    pub exported: bool,
    /// `export opaque` on a type or enum.
    pub opaque: bool,
    /// The module that declares it (`None` for built-ins).
    pub module: Option<ModuleId>,
    /// See the crate docs ("Output").
    pub parent: Option<DefId>,
    /// [`skeleton()`] of `name`.
    pub skeleton: String,
}

/// A resolved name occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Res {
    Def(DefId),
    /// Unresolved; a diagnostic was reported for it (or for its cause).
    Error,
}

/// A name occurrence: the file and the identifier's byte span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NameRef {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl NameRef {
    pub fn new(file: FileId, span: Span) -> NameRef {
        NameRef {
            file,
            start: span.start,
            end: span.end,
        }
    }

    pub fn span(self) -> Span {
        Span::new(self.start, self.end)
    }
}

/// One module of the program.
#[derive(Clone, Debug)]
pub struct Module {
    pub id: ModuleId,
    pub path: ModulePath,
    /// The module's own symbol ([`DefKind::Module`]).
    pub def: DefId,
    /// `None` for an opaque module.
    pub file: Option<FileId>,
    pub skin: Option<Skin>,
    /// The source is part of the input (not opaque).
    pub loaded: bool,
    /// From the stdlib directory.
    pub std: bool,
    /// Every name `use` or `module.name` can reach: `export`ed items and re-exports.
    pub exports: BTreeMap<String, DefId>,
    /// The module scope: imports and items, by name.
    pub scope: BTreeMap<String, DefId>,
}

/// One source file given to [`resolve_program`].
pub struct SourceModule<'a> {
    pub path: ModulePath,
    pub file: FileId,
    pub skin: Skin,
    pub ast: &'a Ast,
    /// From the stdlib directory (its modules may redefine prelude names).
    pub std: bool,
}

/// What the input covers, which decides when a missing module is an error.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The package's short name: the first segment of its own module paths (`notes` in
    /// `use notes/db`).
    pub package: String,
    /// Every module of the package is in the input, so a missing `package/…` module is
    /// K0401. When false (single-file checks), such modules are opaque.
    pub whole_package: bool,
    /// The stdlib directory was loaded, so a missing `std/…` module is K0401 and the
    /// program can be known completely. When false, `std/…` modules are opaque.
    pub std_loaded: bool,
}

/// The resolved program. See the crate docs ("Output").
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub defs: Vec<Def>,
    pub modules: Vec<Module>,
    pub uses: BTreeMap<NameRef, Res>,
    pub decls: BTreeMap<NameRef, DefId>,
    pub implicit: BTreeMap<NameRef, Vec<DefId>>,
    pub(crate) variants: BTreeMap<DefId, Vec<DefId>>,
    pub(crate) fields: BTreeMap<DefId, Vec<DefId>>,
    pub(crate) methods: BTreeMap<DefId, Vec<DefId>>,
    pub(crate) core: BTreeMap<Concept, DefId>,
    pub(crate) prelude: BTreeMap<String, DefId>,
    pub(crate) core_stubs: BTreeMap<Concept, DefId>,
}

impl Program {
    pub fn def(&self, id: DefId) -> &Def {
        &self.defs[id.0 as usize]
    }

    pub fn module(&self, id: ModuleId) -> &Module {
        &self.modules[id.0 as usize]
    }

    pub fn module_by_path(&self, path: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.path.0 == path)
    }

    /// What the name at `span` in `file` refers to.
    pub fn use_at(&self, file: FileId, span: Span) -> Option<Res> {
        self.uses.get(&NameRef::new(file, span)).copied()
    }

    /// The symbol the declaring name at `span` in `file` introduces.
    pub fn decl_at(&self, file: FileId, span: Span) -> Option<DefId> {
        self.decls.get(&NameRef::new(file, span)).copied()
    }

    /// For `.V` at `span`: every variant named `V` in the program (exact spelling), in id
    /// order. `None` if `span` is not an implicit member. An empty list means K0401 was
    /// reported.
    pub fn implicit_candidates(&self, file: FileId, span: Span) -> Option<&[DefId]> {
        self.implicit
            .get(&NameRef::new(file, span))
            .map(|v| v.as_slice())
    }

    /// An enum's variants, in declaration order.
    pub fn variants_of(&self, enum_def: DefId) -> &[DefId] {
        self.variants.get(&enum_def).map_or(&[], |v| v.as_slice())
    }

    /// A record's fields or a variant's labelled fields, in declaration order.
    pub fn fields_of(&self, def: DefId) -> &[DefId] {
        self.fields.get(&def).map_or(&[], |v| v.as_slice())
    }

    /// The inherent methods of a record, enum, alias or extern type (from every `impl T:`
    /// block in the program), or a trait's methods.
    pub fn methods_of(&self, def: DefId) -> &[DefId] {
        self.methods.get(&def).map_or(&[], |v| v.as_slice())
    }

    /// The symbol of a core name.
    pub fn core(&self, concept: Concept) -> Option<DefId> {
        self.core.get(&concept).copied()
    }

    /// The stdlib's signature stub for a core name (`export extern type Int`, `export fn
    /// print[T: Show](value: T)` in `std/prelude`), when the stdlib is loaded. Uses of the
    /// core name still resolve to its [`DefKind::Core`] symbol.
    pub fn core_stub(&self, concept: Concept) -> Option<DefId> {
        self.core_stubs.get(&concept).copied()
    }

    /// The symbol of a prelude name (`Cell`, `log`, `Show`, …). With the stdlib loaded,
    /// `std/prelude`'s exports are the prelude, so this is their declaration.
    pub fn prelude(&self, name: &str) -> Option<DefId> {
        self.prelude.get(name).copied()
    }

    /// The module that declares `def`.
    pub fn module_of(&self, def: DefId) -> Option<&Module> {
        self.def(def).module.map(|m| self.module(m))
    }
}

/// Resolves a whole program: package modules and, when loaded, stdlib modules.
/// Diagnostics point into each module's own file.
pub fn resolve_program(
    sources: &[SourceModule<'_>],
    options: &Options,
) -> (Program, Vec<Diagnostic>) {
    walk::Builder::run(sources, options)
}

/// Resolves one file on its own: every `use` is opaque (no stdlib, no other modules), so
/// only the file's own names are checked. This is what `kalam check FILE` runs.
pub fn resolve_file(file: FileId, skin: Skin, ast: &Ast) -> (Program, Vec<Diagnostic>) {
    let source = SourceModule {
        path: ModulePath("main".to_string()),
        file,
        skin,
        ast,
        std: false,
    };
    resolve_program(&[source], &Options::default())
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
        assert_eq!(
            ModulePath::from_file("notes", "src/db.kl").last_segment(),
            "db"
        );
    }
}
