//! The resolver proper: symbol creation, the module graph, scopes and the body walk.
//!
//! Phases (each over every module, so module items are visible in any order):
//! 1. built-in symbols (core names, prelude);
//! 2. modules and their items (K0301 for a second skin file of one module);
//! 3. `export use` re-exports, to a fixpoint;
//! 4. module scopes: imports, then items (K0405, K0413, SEC0104, K0401 on bad imports);
//! 5. inherent `impl` heads, so `Type.assoc_fn` resolves from any module;
//! 6. bodies (locals, uses, undefined names, implicit members);
//! 7. import cycles (K1201) and private types in exported signatures (K0303).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use kalam_ast::{
    Block, EnumDecl, Expr, ExprKind, ExternDecl, FnBody, FnDecl, FnSig, GenericParam, Ident,
    ImplDecl, Item, ItemKind, ListPatElem, Member, Name, NameKind, Param, Pat, PatFields, PatKind,
    Stmt, StmtKind, StrLit, StrPart, TraitDecl, Type, TypeBody, TypeDecl, TypeKind, UseDecl,
    UseItem, UseTail, Visibility,
};
use kalam_base::{FileId, FileSpan, Skin, Span};
use kalam_diagnostics::{Diagnostic, Fix};
use kalam_skins::{Concept, KeywordTable, Kind};

use crate::prelude_impl::{PRELUDE, PRELUDE_MODULES};
use crate::skeleton::skeleton;
use crate::suggest::closest;
use crate::{
    Def, DefId, DefKind, Module, ModuleId, ModulePath, NameRef, Options, Program, Res,
    SourceModule, BUILTIN_FILE,
};

/// One scope's names: exact spelling → symbol, and skeleton → the first symbol with it.
#[derive(Clone, Debug, Default)]
struct Scope {
    names: HashMap<String, DefId>,
    skels: HashMap<String, DefId>,
}

impl Scope {
    fn insert(&mut self, name: &str, skel: String, def: DefId, overwrite: bool) {
        if overwrite {
            self.names.insert(name.to_string(), def);
        } else {
            self.names.entry(name.to_string()).or_insert(def);
        }
        self.skels.entry(skel).or_insert(def);
    }
}

/// Where a name is used: K0917 (bare variant) applies to values and patterns, not types.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Value,
    Type,
}

/// How a module path of a `use` resolves.
enum Target {
    Loaded(ModuleId),
    Opaque,
    Missing,
}

/// `U+0639 U+0644 U+0649`.
fn code_points(s: &str) -> String {
    s.chars()
        .map(|c| format!("U+{:04X}", c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

/// How SEC0104 shows a name: the spelling and its code points (spec §5.3).
fn shown(name: &str) -> String {
    format!("`{name}` ({})", code_points(name))
}

fn path_text(path: &[Ident]) -> String {
    path.iter()
        .map(|i| i.name.as_str())
        .collect::<Vec<_>>()
        .join("/")
}

fn path_span(path: &[Ident], fallback: Span) -> Span {
    match (path.first(), path.last()) {
        (Some(a), Some(b)) => a.span.cover(b.span),
        _ => fallback,
    }
}

pub(crate) struct Builder<'s> {
    sources: &'s [SourceModule<'s>],
    opts: Options,
    table: &'static KeywordTable,
    p: Program,
    diags: Vec<Diagnostic>,
    by_path: BTreeMap<String, ModuleId>,
    /// Module → index into `sources` (loaded modules only).
    src_of: BTreeMap<ModuleId, usize>,
    /// Source index → item index → symbol.
    item_defs: BTreeMap<usize, Vec<Option<DefId>>>,
    /// Module scopes, by `ModuleId`.
    scopes: Vec<Scope>,
    prelude_scope: Scope,
    /// Per skin index: (skeleton, spelling, symbol) of each core name.
    core_names: [Vec<(String, String, DefId)>; 3],
    opaque_imports: BTreeMap<(ModuleId, String), DefId>,
    /// An opaque or missing module was imported somewhere: the program is not fully known.
    any_opaque: bool,
    /// `use` edges between loaded modules: (from, to, file, span of the `use`).
    edges: Vec<(ModuleId, ModuleId, FileId, Span)>,
    /// Loaded modules each module imports from.
    imports_of: BTreeMap<ModuleId, BTreeSet<ModuleId>>,
    /// Variant name → variants with that exact name, in id order.
    variant_index: BTreeMap<String, Vec<DefId>>,
}

impl<'s> Builder<'s> {
    pub(crate) fn run(
        sources: &'s [SourceModule<'s>],
        opts: &Options,
    ) -> (Program, Vec<Diagnostic>) {
        let mut b = Builder {
            sources,
            opts: opts.clone(),
            table: KeywordTable::builtin(),
            p: Program::default(),
            diags: Vec::new(),
            by_path: BTreeMap::new(),
            src_of: BTreeMap::new(),
            item_defs: BTreeMap::new(),
            scopes: Vec::new(),
            prelude_scope: Scope::default(),
            core_names: [Vec::new(), Vec::new(), Vec::new()],
            opaque_imports: BTreeMap::new(),
            any_opaque: false,
            edges: Vec::new(),
            imports_of: BTreeMap::new(),
            variant_index: BTreeMap::new(),
        };
        b.builtins();
        b.modules();
        b.prelude_modules();
        b.reexports();
        b.adopt_std_prelude();
        b.module_scopes();
        b.impl_heads();
        b.bodies();
        b.cycles();
        b.private_types();
        let mut diags = std::mem::take(&mut b.diags);
        diags.sort_by_key(|d| (d.file, d.span.start, d.span.end));
        (b.p, diags)
    }

    // ---- symbols ------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn new_def(
        &mut self,
        kind: DefKind,
        name: &str,
        span: FileSpan,
        exported: bool,
        opaque: bool,
        module: Option<ModuleId>,
        parent: Option<DefId>,
    ) -> DefId {
        let id = DefId(self.p.defs.len() as u32);
        self.p.defs.push(Def {
            id,
            kind,
            name: name.to_string(),
            span,
            exported,
            opaque,
            module,
            parent,
            skeleton: skeleton(name),
        });
        id
    }

    fn builtin(&mut self, kind: DefKind, name: &str) -> DefId {
        let span = FileSpan::new(BUILTIN_FILE, Span::default());
        self.new_def(kind, name, span, true, false, None, None)
    }

    fn builtins(&mut self) {
        for &c in Concept::ALL {
            if self.table.kind(c) != Kind::CoreName {
                continue;
            }
            let en = self.table.spelling(c, Skin::En).to_string();
            let id = self.builtin(DefKind::Core(c), &en);
            self.p.core.insert(c, id);
            for skin in Skin::ALL {
                let spelling = self.table.spelling(c, skin).to_string();
                self.core_names[skin.index()].push((skeleton(&spelling), spelling, id));
            }
        }
        for name in PRELUDE {
            let id = self.builtin(DefKind::Prelude, name);
            self.p.prelude.insert(name.to_string(), id);
            self.prelude_scope.insert(name, skeleton(name), id, false);
        }
    }

    fn name_of(&self, n: &Name, skin: Skin) -> (String, Option<Concept>) {
        match &n.kind {
            NameKind::Ident(s) => (s.clone(), None),
            NameKind::Core(c) => (self.table.spelling(*c, skin).to_string(), Some(*c)),
        }
    }

    fn new_module(&mut self, path: &str, source: Option<usize>) -> ModuleId {
        let id = ModuleId(self.p.modules.len() as u32);
        let (file, skin, std) = match source {
            Some(i) => (
                Some(self.sources[i].file),
                Some(self.sources[i].skin),
                self.sources[i].std,
            ),
            None => (None, None, path.starts_with("std/")),
        };
        let mp = ModulePath(path.to_string());
        let span = FileSpan::new(file.unwrap_or(BUILTIN_FILE), Span::default());
        let def = self.new_def(
            DefKind::Module,
            mp.last_segment(),
            span,
            true,
            false,
            Some(id),
            None,
        );
        self.p.modules.push(Module {
            id,
            path: mp,
            def,
            file,
            skin,
            loaded: source.is_some(),
            std,
            exports: BTreeMap::new(),
            scope: BTreeMap::new(),
        });
        self.scopes.push(Scope::default());
        self.by_path.insert(path.to_string(), id);
        if let Some(i) = source {
            self.src_of.insert(id, i);
        }
        id
    }

    /// Loaded modules in path order, each with its items (K0301 for a second file).
    fn modules(&mut self) {
        let mut order: Vec<usize> = (0..self.sources.len()).collect();
        order.sort_by(|&a, &b| {
            let (sa, sb) = (&self.sources[a], &self.sources[b]);
            (&sa.path, sa.file).cmp(&(&sb.path, sb.file))
        });
        for i in order {
            let src = &self.sources[i];
            if self.by_path.contains_key(&src.path.0) {
                self.diags.push(
                    Diagnostic::new("K0301", src.file, Span::default())
                        .arg("name", format!("`{}`", src.path.0)),
                );
                continue;
            }
            let m = self.new_module(&src.path.0, Some(i));
            self.declare_items(m, i);
        }
    }

    /// `log` and `json` (§13.1): the loaded `std/log`/`std/json` if present, else opaque.
    fn prelude_modules(&mut self) {
        for (name, path) in PRELUDE_MODULES {
            let m = match self.by_path.get(*path) {
                Some(&m) => m,
                None => self.new_module(path, None),
            };
            let def = self.p.modules[m.0 as usize].def;
            self.p.prelude.insert(name.to_string(), def);
            self.prelude_scope.insert(name, skeleton(name), def, false);
        }
    }

    fn declare_items(&mut self, m: ModuleId, src: usize) {
        let source: &'s SourceModule<'s> = &self.sources[src];
        let file = source.file;
        let skin = source.skin;
        let fs = |span: Span| FileSpan::new(file, span);
        let mut defs = Vec::with_capacity(source.ast.file.items.len());
        for item in &source.ast.file.items {
            let exported = item.vis != Visibility::Private;
            let opaque = item.vis == Visibility::ExportOpaque;
            self.opaque_only_on_types(item, file);
            let named = |b: &mut Builder<'s>, kind: DefKind, n: &Name| -> DefId {
                let (name, _) = b.name_of(n, skin);
                let id = b.new_def(kind, &name, fs(n.span), exported, opaque, Some(m), None);
                b.p.decls.insert(NameRef::new(file, n.span), id);
                id
            };
            let def = match &item.kind {
                ItemKind::Fn(f) => Some(named(self, DefKind::Fn, &f.sig.name)),
                ItemKind::Type(t) => {
                    let kind = match t.body {
                        TypeBody::Record(_) => DefKind::Record,
                        TypeBody::Alias(_) => DefKind::Alias,
                    };
                    let id = named(self, kind, &t.name);
                    if let TypeBody::Record(fields) = &t.body {
                        let mut seen = Scope::default();
                        let mut ids = Vec::new();
                        for f in fields {
                            if f.name.name == "with" {
                                // §9.4: `with` is the update method of every record.
                                self.diags.push(Diagnostic::new("K0417", file, f.name.span));
                            }
                            self.member_name_unique(&mut seen, &f.name, file);
                            let fid = self.new_def(
                                DefKind::Field,
                                &f.name.name,
                                fs(f.name.span),
                                exported && !opaque,
                                false,
                                Some(m),
                                Some(id),
                            );
                            self.p.decls.insert(NameRef::new(file, f.name.span), fid);
                            ids.push(fid);
                        }
                        self.p.fields.insert(id, ids);
                    }
                    Some(id)
                }
                ItemKind::Enum(e) => {
                    let id = named(self, DefKind::Enum, &e.name);
                    self.declare_variants(id, e, m, file, exported && !opaque);
                    Some(id)
                }
                ItemKind::Trait(t) => {
                    let id = named(self, DefKind::Trait, &t.name);
                    let mut ids = Vec::new();
                    for f in &t.members {
                        let (name, _) = self.name_of(&f.sig.name, skin);
                        let mid = self.new_def(
                            DefKind::TraitMethod,
                            &name,
                            fs(f.sig.name.span),
                            exported,
                            false,
                            Some(m),
                            Some(id),
                        );
                        self.p
                            .decls
                            .insert(NameRef::new(file, f.sig.name.span), mid);
                        ids.push(mid);
                    }
                    self.p.methods.insert(id, ids);
                    Some(id)
                }
                ItemKind::Impl(imp) => {
                    let id = self.new_def(
                        DefKind::Impl,
                        "",
                        fs(item.span),
                        false,
                        false,
                        Some(m),
                        None,
                    );
                    for member in &imp.members {
                        let (name, _) = self.name_of(&member.func.sig.name, skin);
                        let mid = self.new_def(
                            DefKind::Method,
                            &name,
                            fs(member.func.sig.name.span),
                            member.export,
                            false,
                            Some(m),
                            Some(id),
                        );
                        self.p
                            .decls
                            .insert(NameRef::new(file, member.func.sig.name.span), mid);
                    }
                    Some(id)
                }
                ItemKind::Const(c) => Some(named(self, DefKind::Const, &c.name)),
                ItemKind::Extern(ExternDecl::Fn(sig)) => {
                    Some(named(self, DefKind::ExternFn, &sig.name))
                }
                ItemKind::Extern(ExternDecl::Type { name, .. }) => {
                    Some(named(self, DefKind::ExternType, name))
                }
                ItemKind::Test(_) | ItemKind::Error => None,
            };
            if let Some(id) = def {
                let d = &self.p.defs[id.0 as usize];
                if d.exported && d.kind != DefKind::Impl {
                    let name = d.name.clone();
                    self.p.modules[m.0 as usize]
                        .exports
                        .entry(name)
                        .or_insert(id);
                }
            }
            defs.push(def);
        }
        self.item_defs.insert(src, defs);
    }

    /// K0302: `export opaque` is for a `type` or an `enum` (§13.2, §8.2).
    fn opaque_only_on_types(&mut self, item: &Item, file: FileId) {
        if item.vis != Visibility::ExportOpaque {
            return;
        }
        let name = match &item.kind {
            ItemKind::Type(_) | ItemKind::Enum(_) | ItemKind::Error => return,
            ItemKind::Fn(f) => Some(&f.sig.name),
            ItemKind::Trait(t) => Some(&t.name),
            ItemKind::Const(c) => Some(&c.name),
            ItemKind::Extern(ExternDecl::Fn(sig)) => Some(&sig.name),
            ItemKind::Extern(ExternDecl::Type { name, .. }) => Some(name),
            ItemKind::Impl(_) | ItemKind::Test(_) => None,
        };
        let span = name.map_or(item.span, |n| Span::new(item.span.start, n.span.end));
        self.diags.push(Diagnostic::new("K0302", file, span));
    }

    fn declare_variants(
        &mut self,
        enum_id: DefId,
        e: &EnumDecl,
        m: ModuleId,
        file: FileId,
        exported: bool,
    ) {
        let fs = |span: Span| FileSpan::new(file, span);
        let mut seen = Scope::default();
        let mut ids = Vec::new();
        for v in &e.variants {
            self.member_name_unique(&mut seen, &v.name, file);
            let vid = self.new_def(
                DefKind::Variant,
                &v.name.name,
                fs(v.name.span),
                exported,
                false,
                Some(m),
                Some(enum_id),
            );
            self.p.decls.insert(NameRef::new(file, v.name.span), vid);
            self.variant_index
                .entry(v.name.name.clone())
                .or_default()
                .push(vid);
            let mut fids = Vec::new();
            let mut seen_fields = Scope::default();
            for f in v.fields.iter().flatten() {
                if let Some(label) = &f.label {
                    self.member_name_unique(&mut seen_fields, label, file);
                    let fid = self.new_def(
                        DefKind::Field,
                        &label.name,
                        fs(label.span),
                        exported,
                        false,
                        Some(m),
                        Some(vid),
                    );
                    self.p.decls.insert(NameRef::new(file, label.span), fid);
                    fids.push(fid);
                }
            }
            if !fids.is_empty() {
                self.p.fields.insert(vid, fids);
            }
            ids.push(vid);
        }
        self.p.variants.insert(enum_id, ids);
    }

    /// Variants of one enum and fields of one record or variant share a namespace: a
    /// repeated name is K0405, a lookalike SEC0104.
    fn member_name_unique(&mut self, seen: &mut Scope, name: &Ident, file: FileId) {
        let skel = skeleton(&name.name);
        if seen.names.contains_key(&name.name) {
            self.diags.push(
                Diagnostic::new("K0405", file, name.span).arg("name", format!("`{}`", name.name)),
            );
            return;
        }
        if let Some(&other) = seen.skels.get(&skel) {
            let other_name = self.p.defs[other.0 as usize].name.clone();
            let other_span = self.p.defs[other.0 as usize].span;
            let mut d = Diagnostic::new("SEC0104", file, name.span)
                .arg("name", shown(&name.name))
                .arg("other", shown(&other_name));
            if other_span.file == file {
                d = d.label(other_span.span, format!("`{other_name}` is declared here"));
            }
            self.diags.push(d);
        }
        // A placeholder id: only the spelling and skeleton matter here, and the real
        // symbol is created right after; record it by the next id.
        let next = DefId(self.p.defs.len() as u32);
        seen.insert(&name.name, skel, next, false);
    }

    // ---- module graph -------------------------------------------------------------------

    fn classify(&self, path: &str) -> Target {
        if let Some(&m) = self.by_path.get(path) {
            if self.p.modules[m.0 as usize].loaded {
                return Target::Loaded(m);
            }
            return Target::Opaque;
        }
        let first = path.split('/').next().unwrap_or("");
        let must_exist = if first == "std" {
            self.opts.std_loaded
        } else {
            first == self.opts.package && self.opts.whole_package
        };
        if must_exist {
            Target::Missing
        } else {
            Target::Opaque
        }
    }

    fn opaque_module(&mut self, path: &str) -> ModuleId {
        match self.by_path.get(path) {
            Some(&m) => m,
            None => self.new_module(path, None),
        }
    }

    fn opaque_import(&mut self, m: ModuleId, name: &str) -> DefId {
        if let Some(&d) = self.opaque_imports.get(&(m, name.to_string())) {
            return d;
        }
        let span = FileSpan::new(BUILTIN_FILE, Span::default());
        let d = self.new_def(DefKind::Import, name, span, true, false, Some(m), None);
        self.opaque_imports.insert((m, name.to_string()), d);
        d
    }

    fn uses_of(&self, src: usize) -> &'s [UseDecl] {
        let sources: &'s [SourceModule<'s>] = self.sources;
        &sources[src].ast.file.uses
    }

    fn loaded_modules(&self) -> Vec<(ModuleId, usize)> {
        self.src_of.iter().map(|(&m, &i)| (m, i)).collect()
    }

    /// `export use` re-exports, repeated until nothing changes (a chain of re-exports
    /// across modules needs one round per link).
    fn reexports(&mut self) {
        let modules = self.loaded_modules();
        for _round in 0..=modules.len() {
            let mut changed = false;
            for &(m, src) in &modules {
                for u in self.uses_of(src) {
                    if !u.export {
                        continue;
                    }
                    let path = path_text(&u.path);
                    let target = match self.classify(&path) {
                        Target::Loaded(t) => t,
                        Target::Opaque => self.opaque_module(&path),
                        Target::Missing => continue,
                    };
                    let loaded = self.p.modules[target.0 as usize].loaded;
                    let mut add: Vec<(String, DefId)> = Vec::new();
                    let tdef = self.p.modules[target.0 as usize].def;
                    let last = self.p.modules[target.0 as usize]
                        .path
                        .last_segment()
                        .to_string();
                    match &u.tail {
                        UseTail::None => add.push((last, tdef)),
                        UseTail::Alias(a) => add.push((a.name.clone(), tdef)),
                        UseTail::Items(items) => {
                            for it in items {
                                match it {
                                    UseItem::SelfModule(_) => add.push((last.clone(), tdef)),
                                    UseItem::Name { name, alias } => {
                                        let bound = alias.as_ref().unwrap_or(name).name.clone();
                                        let found = if loaded {
                                            self.p.modules[target.0 as usize]
                                                .exports
                                                .get(&name.name)
                                                .copied()
                                        } else {
                                            Some(self.opaque_import(target, &name.name))
                                        };
                                        if let Some(d) = found {
                                            add.push((bound, d));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let exports = &mut self.p.modules[m.0 as usize].exports;
                    for (name, d) in add {
                        if let std::collections::btree_map::Entry::Vacant(e) = exports.entry(name) {
                            e.insert(d);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// When the stdlib's `std/prelude` is loaded, its exports are the prelude (§13.1): they
    /// replace the built-in prelude symbols of the same name, and its core-named items are
    /// the core names' signature stubs ([`Program::core_stub`]).
    fn adopt_std_prelude(&mut self) {
        let Some(&m) = self.by_path.get("std/prelude") else {
            return;
        };
        if !self.p.modules[m.0 as usize].loaded {
            return;
        }
        let exports = self.p.modules[m.0 as usize].exports.clone();
        let en = self.table.skin(Skin::En);
        for (name, def) in exports {
            match en.lookup_word(&name) {
                Some(w) if w.kind == Kind::CoreName => {
                    self.p.core_stubs.insert(w.concept, def);
                }
                Some(_) => {}
                None => {
                    let skel = skeleton(&name);
                    self.prelude_scope.names.insert(name.clone(), def);
                    self.prelude_scope.skels.insert(skel, def);
                    self.p.prelude.insert(name, def);
                }
            }
        }
    }

    /// Phase 4: every module scope, imports first, then items.
    fn module_scopes(&mut self) {
        for (m, src) in self.loaded_modules() {
            let source: &'s SourceModule<'s> = &self.sources[src];
            let (file, skin, std) = (source.file, source.skin, source.std);
            for u in self.uses_of(src) {
                self.import(m, file, std, u);
            }
            let defs = self.item_defs.get(&src).cloned().unwrap_or_default();
            for (item, def) in source.ast.file.items.iter().zip(defs) {
                let Some(def) = def else { continue };
                let name = match &item.kind {
                    ItemKind::Fn(f) => Some((&f.sig.name, true)),
                    ItemKind::Type(TypeDecl { name, .. })
                    | ItemKind::Enum(EnumDecl { name, .. })
                    | ItemKind::Trait(TraitDecl { name, .. })
                    | ItemKind::Extern(ExternDecl::Type { name, .. }) => Some((name, false)),
                    ItemKind::Const(c) => Some((&c.name, false)),
                    ItemKind::Extern(ExternDecl::Fn(sig)) => Some((&sig.name, false)),
                    _ => None,
                };
                let Some((n, is_fn)) = name else { continue };
                match &n.kind {
                    NameKind::Core(Concept::CoreMain) if is_fn => {
                        let (text, _) = self.name_of(n, skin);
                        self.scopes[m.0 as usize].insert(&text, skeleton(&text), def, false);
                        self.p.modules[m.0 as usize].scope.insert(text, def);
                    }
                    // The stdlib declares the core names' signature stubs (`std/prelude`).
                    NameKind::Core(_) if std => {
                        let (text, _) = self.name_of(n, skin);
                        self.scopes[m.0 as usize].insert(&text, skeleton(&text), def, false);
                        self.p.modules[m.0 as usize].scope.insert(text, def);
                    }
                    NameKind::Core(_) => {
                        let (text, _) = self.name_of(n, skin);
                        self.diags.push(
                            Diagnostic::new("K0413", file, n.span).arg("name", format!("`{text}`")),
                        );
                    }
                    NameKind::Ident(text) => {
                        let ctx = BindCtx {
                            m,
                            file,
                            std,
                            api_label: false,
                            locals: &[],
                        };
                        self.bind(&ctx, text, n.span, def, false, false);
                    }
                }
            }
        }
    }

    fn import(&mut self, m: ModuleId, file: FileId, std: bool, u: &UseDecl) {
        let path = path_text(&u.path);
        let span = path_span(&u.path, u.span);
        let target = match self.classify(&path) {
            Target::Loaded(t) => {
                self.edges.push((m, t, file, u.span));
                self.imports_of.entry(m).or_default().insert(t);
                t
            }
            Target::Opaque => {
                self.any_opaque = true;
                self.opaque_module(&path)
            }
            Target::Missing => {
                self.any_opaque = true;
                let known: Vec<String> = self
                    .p
                    .modules
                    .iter()
                    .filter(|x| x.loaded)
                    .map(|x| x.path.0.clone())
                    .collect();
                let mut d = Diagnostic::new("K0401", file, span).arg("name", format!("`{path}`"));
                if let Some(best) = closest(&path, known.iter().map(String::as_str)) {
                    d = d.fix(Fix::suggestion(format!("write `{best}`"), span, best));
                }
                self.diags.push(d);
                self.opaque_module(&path)
            }
        };
        let tm = &self.p.modules[target.0 as usize];
        let (tdef, loaded) = (tm.def, tm.loaded);
        let last = tm.path.last_segment().to_string();
        if let Some(seg) = u.path.last() {
            self.p
                .uses
                .insert(NameRef::new(file, seg.span), Res::Def(tdef));
        }
        let ctx = BindCtx {
            m,
            file,
            std,
            api_label: false,
            locals: &[],
        };
        match &u.tail {
            UseTail::None => {
                if let Some(seg) = u.path.last() {
                    self.bind(&ctx, &last, seg.span, tdef, true, false);
                }
            }
            UseTail::Alias(a) => {
                self.p
                    .uses
                    .insert(NameRef::new(file, a.span), Res::Def(tdef));
                self.bind(&ctx, &a.name, a.span, tdef, true, false);
            }
            UseTail::Items(items) => {
                for it in items {
                    match it {
                        UseItem::SelfModule(s) => {
                            self.p.uses.insert(NameRef::new(file, *s), Res::Def(tdef));
                            self.bind(&ctx, &last, *s, tdef, true, false);
                        }
                        UseItem::Name { name, alias } => {
                            let found = if loaded {
                                let exports = &self.p.modules[target.0 as usize].exports;
                                match exports.get(&name.name) {
                                    Some(&d) => Some(d),
                                    None => {
                                        let names: Vec<String> = exports.keys().cloned().collect();
                                        let mut d = Diagnostic::new("K0401", file, name.span)
                                            .arg("name", format!("`{}`", name.name));
                                        if let Some(best) =
                                            closest(&name.name, names.iter().map(String::as_str))
                                        {
                                            d = d.fix(Fix::suggestion(
                                                format!("write `{best}`"),
                                                name.span,
                                                best,
                                            ));
                                        }
                                        self.diags.push(d);
                                        None
                                    }
                                }
                            } else {
                                Some(self.opaque_import(target, &name.name))
                            };
                            let res = found.map_or(Res::Error, Res::Def);
                            self.p.uses.insert(NameRef::new(file, name.span), res);
                            let bound = alias.as_ref().unwrap_or(name);
                            if alias.is_some() {
                                self.p.uses.insert(NameRef::new(file, bound.span), res);
                            }
                            // An unknown import still binds the name (to an opaque symbol),
                            // so its uses do not cascade into more K0401s.
                            let def = match found {
                                Some(d) => d,
                                None => self.opaque_import(target, &name.name),
                            };
                            self.bind(&ctx, &bound.name, bound.span, def, true, false);
                        }
                    }
                }
            }
        }
    }

    // ---- binding checks -----------------------------------------------------------------

    /// Declares `name` (at `span`, meaning `def`) in the innermost scope of `ctx`, after the
    /// no-shadowing (K0405), core-name (K0413) and skeleton (SEC0104) checks. An `import`
    /// may repeat a prelude name (`use std/task.{Cell}`). `local` binds in `ctx.locals`'
    /// innermost scope (the caller pushes it), else in the module scope.
    fn bind(
        &mut self,
        ctx: &BindCtx<'_>,
        name: &str,
        span: Span,
        def: DefId,
        import: bool,
        local: bool,
    ) -> bool {
        // An identifier spelled like a core name (`` `print` `` in an English file) is an
        // ordinary name, never the core name: the same AST in another skin has it without
        // backticks (§4.3, §5.4), so the verdict must not depend on the skin. K0413 is for
        // declarations written with the core name itself (`NameKind::Core`).
        let BindCtx {
            m,
            file,
            std,
            api_label,
            ..
        } = *ctx;
        let skip_prelude = std || import;
        let exact = ctx
            .locals
            .iter()
            .rev()
            .find_map(|s| s.names.get(name).copied())
            .or_else(|| {
                if api_label {
                    None
                } else {
                    self.scopes[m.0 as usize].names.get(name).copied()
                }
            })
            .or_else(|| {
                if skip_prelude {
                    None
                } else {
                    self.prelude_scope.names.get(name).copied()
                }
            });
        let skel = skeleton(name);
        let mut ok = true;
        match exact {
            Some(prev) if prev == def => {}
            Some(prev) => {
                ok = false;
                let prev_span = self.p.defs[prev.0 as usize].span;
                let mut d = Diagnostic::new("K0405", file, span).arg("name", format!("`{name}`"));
                if prev_span.file == file && prev_span.span != span {
                    d = d.label(prev_span.span, format!("`{name}` is already declared here"));
                }
                self.diags.push(d);
            }
            None => {
                if let Some((other, other_span)) = self.lookalike(ctx, &skel, name) {
                    ok = false;
                    let mut d = Diagnostic::new("SEC0104", file, span)
                        .arg("name", shown(name))
                        .arg("other", shown(&other));
                    if let Some(os) = other_span.filter(|s| s.file == file) {
                        d = d.label(os.span, format!("`{other}` is declared here"));
                    }
                    self.diags.push(d);
                }
            }
        }
        // A local binding is inserted by the caller (`Walker::declare`), which owns the
        // local scopes.
        if !local {
            self.scopes[m.0 as usize].insert(name, skel, def, false);
            self.p.modules[m.0 as usize]
                .scope
                .entry(name.to_string())
                .or_insert(def);
        }
        ok
    }

    /// A visible name with the same skeleton and another spelling: its spelling and, for a
    /// source declaration, its span.
    fn lookalike(
        &self,
        ctx: &BindCtx<'_>,
        skel: &str,
        name: &str,
    ) -> Option<(String, Option<FileSpan>)> {
        let from_def = |d: DefId| {
            let def = &self.p.defs[d.0 as usize];
            (def.name != name).then(|| (def.name.clone(), Some(def.span)))
        };
        for s in ctx.locals.iter().rev() {
            if let Some(found) = s.skels.get(skel).and_then(|&d| from_def(d)) {
                return Some(found);
            }
        }
        if let Some(found) = self.scopes[ctx.m.0 as usize]
            .skels
            .get(skel)
            .and_then(|&d| from_def(d))
        {
            return Some(found);
        }
        if ctx.std {
            return None;
        }
        if let Some(found) = self
            .prelude_scope
            .skels
            .get(skel)
            .and_then(|&d| from_def(d))
        {
            return Some((found.0, None));
        }
        // Core names in every skin, so the verdict is the same after `kalam convert`.
        self.core_names
            .iter()
            .flatten()
            .find(|(s, spelling, _)| s == skel && spelling != name)
            .map(|(_, spelling, _)| (spelling.clone(), None))
    }

    // ---- impls and bodies ---------------------------------------------------------------

    /// Phase 5: inherent impl members become `methods_of(type)`.
    fn impl_heads(&mut self) {
        for (m, src) in self.loaded_modules() {
            let source: &'s SourceModule<'s> = &self.sources[src];
            let defs = self.item_defs.get(&src).cloned().unwrap_or_default();
            for (item, def) in source.ast.file.items.iter().zip(defs) {
                let (ItemKind::Impl(imp), Some(impl_def)) = (&item.kind, def) else {
                    continue;
                };
                if imp.as_trait.is_some() {
                    continue;
                }
                let TypeKind::Named { path, .. } = &imp.ty.kind else {
                    continue;
                };
                let Some(target) = self.quiet_path(m, path) else {
                    continue;
                };
                let methods: Vec<DefId> = self
                    .p
                    .defs
                    .iter()
                    .filter(|d| d.kind == DefKind::Method && d.parent == Some(impl_def))
                    .map(|d| d.id)
                    .collect();
                self.p.methods.entry(target).or_default().extend(methods);
            }
        }
    }

    /// Resolves a type path in module scope without reporting anything.
    fn quiet_path(&self, m: ModuleId, path: &[Name]) -> Option<DefId> {
        let first = path.first()?;
        let mut cur = match &first.kind {
            NameKind::Ident(s) => self.scopes[m.0 as usize]
                .names
                .get(s)
                .or_else(|| self.prelude_scope.names.get(s))
                .copied()?,
            NameKind::Core(c) => *self.p.core.get(c)?,
        };
        for seg in &path[1..] {
            let NameKind::Ident(s) = &seg.kind else {
                return None;
            };
            let d = &self.p.defs[cur.0 as usize];
            if d.kind != DefKind::Module {
                return None;
            }
            cur = *self.p.modules[d.module?.0 as usize].exports.get(s)?;
        }
        Some(cur)
    }

    fn bodies(&mut self) {
        for (m, src) in self.loaded_modules() {
            let source: &'s SourceModule<'s> = &self.sources[src];
            let defs = self.item_defs.get(&src).cloned().unwrap_or_default();
            let mut w = Walker {
                m,
                file: source.file,
                skin: source.skin,
                std: source.std,
                locals: Vec::new(),
                owner: None,
                or_seen: None,
                b: self,
            };
            for (item, def) in source.ast.file.items.iter().zip(defs) {
                w.item(item, def);
            }
        }
    }

    /// The whole program is known: the stdlib is loaded and no import is opaque.
    fn complete(&self) -> bool {
        self.opts.std_loaded && !self.any_opaque
    }

    // ---- K1201, K0303 -------------------------------------------------------------------

    fn cycles(&mut self) {
        let n = self.p.modules.len();
        let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
        for &(a, b, _, _) in &self.edges {
            adj[a.0 as usize].insert(b.0 as usize);
        }
        for scc in tarjan(&adj) {
            let self_loop = scc.len() == 1 && adj[scc[0]].contains(&scc[0]);
            if scc.len() < 2 && !self_loop {
                continue;
            }
            let members: BTreeSet<usize> = scc.iter().copied().collect();
            let start = *scc
                .iter()
                .min_by(|&&a, &&b| self.p.modules[a].path.cmp(&self.p.modules[b].path))
                .unwrap_or(&scc[0]);
            let Some(&(_, to, file, span)) = self
                .edges
                .iter()
                .find(|(a, b, _, _)| a.0 as usize == start && members.contains(&(b.0 as usize)))
            else {
                continue;
            };
            // The cycle, walked from `start` through `to` back to `start`.
            let mut chain = vec![self.p.modules[start].path.0.clone()];
            let mut cur = to.0 as usize;
            let mut guard = 0;
            while cur != start && guard <= n {
                chain.push(self.p.modules[cur].path.0.clone());
                cur = shortest_step(&adj, cur, start, &members).unwrap_or(start);
                guard += 1;
            }
            chain.push(self.p.modules[start].path.0.clone());
            self.diags
                .push(Diagnostic::new("K1201", file, span).arg("cycle", chain.join(" → ")));
        }
    }

    /// K0303: an exported signature names a private type of its own module.
    fn private_types(&mut self) {
        for (m, src) in self.loaded_modules() {
            let source: &'s SourceModule<'s> = &self.sources[src];
            let file = source.file;
            let mut types: Vec<&'s Type> = Vec::new();
            for item in &source.ast.file.items {
                let exported = item.vis != Visibility::Private;
                let opaque = item.vis == Visibility::ExportOpaque;
                let sig_types = |sig: &'s FnSig, out: &mut Vec<&'s Type>| {
                    for p in &sig.params {
                        if let Param::Named { ty, .. } = p {
                            out.push(ty);
                        }
                    }
                    out.extend(sig.ret.iter());
                };
                match &item.kind {
                    ItemKind::Impl(imp) => {
                        for mem in imp.members.iter().filter(|x| x.export) {
                            sig_types(&mem.func.sig, &mut types);
                        }
                    }
                    _ if !exported => {}
                    ItemKind::Fn(f) => sig_types(&f.sig, &mut types),
                    ItemKind::Extern(ExternDecl::Fn(sig)) => sig_types(sig, &mut types),
                    ItemKind::Trait(t) => {
                        for f in &t.members {
                            sig_types(&f.sig, &mut types);
                        }
                    }
                    ItemKind::Const(c) => types.extend(c.ty.iter()),
                    ItemKind::Type(t) if !opaque => match &t.body {
                        TypeBody::Record(fields) => types.extend(fields.iter().map(|f| &f.ty)),
                        TypeBody::Alias(ty) => types.push(ty),
                    },
                    ItemKind::Enum(e) if !opaque => {
                        for v in &e.variants {
                            types.extend(v.fields.iter().flatten().map(|f| &f.ty));
                        }
                    }
                    _ => {}
                }
            }
            let mut seen = BTreeSet::new();
            while let Some(t) = types.pop() {
                match &t.kind {
                    TypeKind::Named { path, args } => {
                        types.extend(args.iter());
                        let last = path.iter().rev().find_map(|seg| {
                            match self.p.uses.get(&NameRef::new(file, seg.span)) {
                                Some(Res::Def(d)) => Some((seg, *d)),
                                _ => None,
                            }
                        });
                        let Some((seg, d)) = last else { continue };
                        let def = &self.p.defs[d.0 as usize];
                        let private_type = matches!(
                            def.kind,
                            DefKind::Record
                                | DefKind::Enum
                                | DefKind::Alias
                                | DefKind::Trait
                                | DefKind::ExternType
                        ) && def.module == Some(m)
                            && !def.exported;
                        if private_type && seen.insert((seg.span.start, seg.span.end)) {
                            let name = def.name.clone();
                            self.diags.push(
                                Diagnostic::new("K0303", file, seg.span)
                                    .arg("name", format!("`{name}`"))
                                    .label(
                                        def.span.span,
                                        format!("`{name}` is declared here without `export`"),
                                    ),
                            );
                        }
                    }
                    TypeKind::Tuple(ts) => types.extend(ts.iter()),
                    TypeKind::Fn { params, ret, .. } => {
                        types.extend(params.iter());
                        types.push(ret);
                    }
                    TypeKind::Unit | TypeKind::Error => {}
                }
            }
        }
    }
}

/// The context a binding check needs.
struct BindCtx<'l> {
    m: ModuleId,
    file: FileId,
    std: bool,
    /// A stdlib parameter: its name is an API label (`serve(router: …)` next to
    /// `fn router()`), so only locals count as shadowed (FOUNDER fork, M2 report).
    api_label: bool,
    locals: &'l [Scope],
}

/// Strongly connected components (Tarjan), each sorted, in a deterministic order.
fn tarjan(adj: &[BTreeSet<usize>]) -> Vec<Vec<usize>> {
    struct T<'a> {
        adj: &'a [BTreeSet<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<Vec<usize>>,
    }
    fn visit(t: &mut T<'_>, v: usize) {
        t.index[v] = Some(t.next);
        t.low[v] = t.next;
        t.next += 1;
        t.stack.push(v);
        t.on[v] = true;
        let adj = t.adj;
        for &w in &adj[v] {
            match t.index[w] {
                None => {
                    visit(t, w);
                    t.low[v] = t.low[v].min(t.low[w]);
                }
                Some(iw) if t.on[w] => t.low[v] = t.low[v].min(iw),
                Some(_) => {}
            }
        }
        if Some(t.low[v]) == t.index[v] {
            let mut scc = Vec::new();
            while let Some(w) = t.stack.pop() {
                t.on[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            scc.sort_unstable();
            t.out.push(scc);
        }
    }
    let n = adj.len();
    let mut t = T {
        adj,
        index: vec![None; n],
        low: vec![0; n],
        on: vec![false; n],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for v in 0..n {
        if t.index[v].is_none() {
            visit(&mut t, v);
        }
    }
    t.out.sort();
    t.out
}

/// The first step on a shortest path from `from` to `to` inside `members` (BFS).
fn shortest_step(
    adj: &[BTreeSet<usize>],
    from: usize,
    to: usize,
    members: &BTreeSet<usize>,
) -> Option<usize> {
    let mut first: BTreeMap<usize, usize> = BTreeMap::new();
    let mut queue = std::collections::VecDeque::new();
    for &w in &adj[from] {
        if members.contains(&w) && !first.contains_key(&w) {
            first.insert(w, w);
            queue.push_back(w);
        }
    }
    while let Some(v) = queue.pop_front() {
        if v == to {
            return first.get(&v).copied();
        }
        let step = first[&v];
        for &w in &adj[v] {
            if members.contains(&w) && !first.contains_key(&w) {
                first.insert(w, step);
                queue.push_back(w);
            }
        }
    }
    None
}

// ---- the body walk ----------------------------------------------------------------------

struct Walker<'w, 's> {
    b: &'w mut Builder<'s>,
    m: ModuleId,
    file: FileId,
    skin: Skin,
    std: bool,
    locals: Vec<Scope>,
    /// The item whose body is being walked (the parent of its locals).
    owner: Option<DefId>,
    /// Inside an or-pattern: names bound so far, and whether this alternative reuses them.
    or_seen: Option<(HashMap<String, DefId>, bool)>,
}

impl Walker<'_, '_> {
    fn record(&mut self, span: Span, res: Res) {
        self.b.p.uses.insert(NameRef::new(self.file, span), res);
    }

    fn push(&mut self) {
        self.locals.push(Scope::default());
    }

    fn pop(&mut self) {
        self.locals.pop();
    }

    /// Declares a local binding (parameter, `let`, pattern name, generic, local `fn`).
    fn bind_local(&mut self, name: &Ident, kind: DefKind) -> Option<DefId> {
        if name.name == "_" {
            return None;
        }
        if let Some((seen, reuse)) = &self.or_seen {
            if *reuse {
                if let Some(&d) = seen.get(&name.name) {
                    self.b.p.decls.insert(NameRef::new(self.file, name.span), d);
                    return Some(d);
                }
            }
        }
        let def = self.b.new_def(
            kind,
            &name.name,
            FileSpan::new(self.file, name.span),
            false,
            false,
            Some(self.m),
            self.owner,
        );
        self.declare(&name.name, name.span, def);
        if let Some((seen, _)) = &mut self.or_seen {
            seen.insert(name.name.clone(), def);
        }
        Some(def)
    }

    /// Checks and inserts an already-created local symbol into the innermost scope.
    fn declare(&mut self, name: &str, span: Span, def: DefId) {
        self.b.p.decls.insert(NameRef::new(self.file, span), def);
        let kind = self.b.p.defs[def.0 as usize].kind;
        let ctx = BindCtx {
            m: self.m,
            file: self.file,
            std: self.std,
            api_label: self.std && kind == DefKind::Param,
            locals: &self.locals,
        };
        self.b.bind(&ctx, name, span, def, false, true);
        if let Some(scope) = self.locals.last_mut() {
            scope.insert(name, skeleton(name), def, true);
        }
    }

    fn lookup(&self, name: &str) -> Option<DefId> {
        self.locals
            .iter()
            .rev()
            .find_map(|s| s.names.get(name).copied())
            .or_else(|| self.b.scopes[self.m.0 as usize].names.get(name).copied())
            .or_else(|| self.b.prelude_scope.names.get(name).copied())
    }

    /// Every visible spelling, sorted (locals, module scope, prelude, core names).
    fn visible_names(&self) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for s in &self.locals {
            out.extend(s.names.keys().cloned());
        }
        out.extend(self.b.scopes[self.m.0 as usize].names.keys().cloned());
        out.extend(self.b.prelude_scope.names.keys().cloned());
        out.extend(
            self.b.core_names[self.skin.index()]
                .iter()
                .map(|(_, spelling, _)| spelling.clone()),
        );
        out.into_iter().collect()
    }

    fn core_use(&mut self, c: Concept, span: Span) -> Option<DefId> {
        let def = if c == Concept::CoreMain {
            let main = self.b.table.spelling(c, self.skin).to_string();
            self.b.scopes[self.m.0 as usize].names.get(&main).copied()
        } else {
            None
        }
        .or_else(|| self.b.p.core.get(&c).copied());
        if let Some(d) = def {
            self.record(span, Res::Def(d));
        }
        def
    }

    fn name_use(&mut self, n: &Name, ctx: Ctx) -> Option<DefId> {
        match &n.kind {
            NameKind::Ident(s) => self.ident_use(s, n.span, ctx),
            NameKind::Core(c) => self.core_use(*c, n.span),
        }
    }

    fn ident_use(&mut self, name: &str, span: Span, ctx: Ctx) -> Option<DefId> {
        match self.lookup(name) {
            Some(d) => {
                self.record(span, Res::Def(d));
                Some(d)
            }
            None => {
                self.record(span, Res::Error);
                self.undefined(name, span, ctx);
                None
            }
        }
    }

    /// The decision order of the crate docs for a name with no visible declaration.
    fn undefined(&mut self, name: &str, span: Span, ctx: Ctx) {
        let file = self.file;
        // 1. Another skin's keyword or core name (spec §5.4, the undefined-name half).
        if let Some(w) = self.b.table.cross_skin_word(name, self.skin) {
            let expected = self.b.table.spelling(w.concept, self.skin).to_string();
            self.b.diags.push(
                Diagnostic::new("K0201", file, span)
                    .arg("found", name)
                    .arg("expected", expected.clone())
                    .fix(Fix::machine(format!("write `{expected}`"), span, expected)),
            );
            return;
        }
        // 2. A variant of an enum from another module, written without its dot.
        if ctx == Ctx::Value && self.foreign_variant(name) {
            self.b
                .diags
                .push(Diagnostic::new("K0917", file, span).fix(Fix::machine(
                    "write the variant with a leading `.`",
                    Span::new(span.start, span.start),
                    ".",
                )));
            return;
        }
        let names = self.visible_names();
        // 3. Exactly one visible declaration with the same skeleton (FOUNDER-QUEUE Q7).
        let skel = skeleton(name);
        let same: Vec<&String> = names.iter().filter(|n| skeleton(n) == skel).collect();
        let mut d = Diagnostic::new("K0401", file, span).arg("name", format!("`{name}`"));
        if let [declared] = same.as_slice() {
            let declared = declared.to_string();
            d = d.fix(Fix::machine(format!("write `{declared}`"), span, declared));
            self.b.diags.push(d);
            return;
        }
        // 4. `و` glued to a name, keyword or core name (spec §5.4 last bullet).
        if let Some(rest) = name.strip_prefix('و').filter(|r| !r.is_empty()) {
            let known = self.lookup(rest).is_some()
                || self.b.table.skin(Skin::Ar).lookup_word(rest).is_some();
            if known {
                let fixed = format!("و {rest}");
                d = d.fix(Fix::suggestion(format!("write `{fixed}`"), span, fixed));
                self.b.diags.push(d);
                return;
            }
        }
        // 5. The closest visible name.
        if let Some(best) = closest(name, names.iter().map(String::as_str)) {
            let best = best.to_string();
            d = d.fix(Fix::suggestion(format!("write `{best}`"), span, best));
        }
        self.b.diags.push(d);
    }

    /// `name` is a variant of an exported enum in a module this module imports.
    fn foreign_variant(&self, name: &str) -> bool {
        let Some(imported) = self.b.imports_of.get(&self.m) else {
            return false;
        };
        self.b.variant_index.get(name).is_some_and(|vs| {
            vs.iter().any(|&v| {
                let def = &self.b.p.defs[v.0 as usize];
                def.exported
                    && def
                        .module
                        .is_some_and(|m| m != self.m && imported.contains(&m))
            })
        })
    }

    /// `.Name`: every variant with that spelling (typeck picks by the expected type).
    fn implicit(&mut self, id: &Ident) {
        let cands = self
            .b
            .variant_index
            .get(&id.name)
            .cloned()
            .unwrap_or_default();
        if cands.is_empty() && self.b.complete() {
            let mut d =
                Diagnostic::new("K0401", self.file, id.span).arg("name", format!("`.{}`", id.name));
            if let Some(best) = closest(&id.name, self.b.variant_index.keys().map(String::as_str)) {
                let best = best.to_string();
                d = d.fix(Fix::suggestion(format!("write `.{best}`"), id.span, best));
            }
            self.b.diags.push(d);
        }
        self.b
            .p
            .implicit
            .insert(NameRef::new(self.file, id.span), cands);
    }

    /// `base.name` where `base` statically denotes `target`: a module member, an enum
    /// variant or an inherent associated function. Anything else is typeck's (fields,
    /// methods).
    fn member(&mut self, target: DefId, name: &Ident) -> Option<DefId> {
        let def = &self.b.p.defs[target.0 as usize];
        match def.kind {
            DefKind::Module => {
                let module = &self.b.p.modules[def.module?.0 as usize];
                if !module.loaded {
                    return None;
                }
                match module.exports.get(&name.name).copied() {
                    Some(d) => {
                        self.record(name.span, Res::Def(d));
                        Some(d)
                    }
                    None => {
                        let names: Vec<String> = module.exports.keys().cloned().collect();
                        self.record(name.span, Res::Error);
                        let mut d = Diagnostic::new("K0401", self.file, name.span)
                            .arg("name", format!("`{}`", name.name));
                        if let Some(best) = closest(&name.name, names.iter().map(String::as_str)) {
                            let best = best.to_string();
                            d = d.fix(Fix::suggestion(format!("write `{best}`"), name.span, best));
                        }
                        self.b.diags.push(d);
                        None
                    }
                }
            }
            DefKind::Enum
            | DefKind::Record
            | DefKind::Alias
            | DefKind::ExternType
            | DefKind::Trait => {
                let found = self
                    .b
                    .p
                    .variants_of(target)
                    .iter()
                    .chain(self.b.p.methods_of(target))
                    .copied()
                    .find(|&d| self.b.p.defs[d.0 as usize].name == name.name);
                if let Some(d) = found {
                    self.record(name.span, Res::Def(d));
                }
                found
            }
            _ => None,
        }
    }

    fn path_use(&mut self, path: &[Name], ctx: Ctx) -> Option<DefId> {
        let (first, rest) = path.split_first()?;
        let mut cur = self.name_use(first, ctx)?;
        for seg in rest {
            let NameKind::Ident(s) = &seg.kind else {
                return None;
            };
            let id = Ident {
                name: s.clone(),
                span: seg.span,
            };
            cur = self.member(cur, &id)?;
        }
        Some(cur)
    }

    // ---- items ----

    fn item(&mut self, item: &Item, def: Option<DefId>) {
        self.owner = def;
        match &item.kind {
            ItemKind::Fn(f) => self.func(f),
            ItemKind::Type(t) => {
                self.push();
                self.generics(&t.generics);
                match &t.body {
                    TypeBody::Record(fields) => {
                        for f in fields {
                            self.ty(&f.ty);
                            if let Some(e) = &f.default {
                                self.expr(e);
                            }
                        }
                    }
                    TypeBody::Alias(ty) => self.ty(ty),
                }
                self.pop();
            }
            ItemKind::Enum(e) => {
                self.push();
                self.generics(&e.generics);
                for v in &e.variants {
                    for f in v.fields.iter().flatten() {
                        self.ty(&f.ty);
                    }
                }
                self.pop();
            }
            ItemKind::Trait(t) => {
                self.push();
                self.generics(&t.generics);
                for f in &t.members {
                    self.owner = self.b.p.decl_at(self.file, f.sig.name.span);
                    self.func(f);
                }
                self.pop();
            }
            ItemKind::Impl(imp) => self.impl_block(imp),
            ItemKind::Const(c) => {
                if let Some(t) = &c.ty {
                    self.ty(t);
                }
                self.expr(&c.value);
            }
            ItemKind::Extern(ExternDecl::Fn(sig)) => {
                self.push();
                self.generics(&sig.generics);
                for p in &sig.params {
                    if let Param::Named { ty, .. } = p {
                        self.ty(ty);
                    }
                }
                if let Some(r) = &sig.ret {
                    self.ty(r);
                }
                self.pop();
            }
            ItemKind::Extern(ExternDecl::Type { .. }) => {}
            ItemKind::Test(t) => self.block(&t.body),
            ItemKind::Error => {}
        }
        self.owner = None;
    }

    fn impl_block(&mut self, imp: &ImplDecl) {
        self.push();
        self.generics(&imp.generics);
        self.ty(&imp.ty);
        if let Some(t) = &imp.as_trait {
            self.ty(t);
        }
        for member in &imp.members {
            self.owner = self.b.p.decl_at(self.file, member.func.sig.name.span);
            self.func(&member.func);
        }
        self.pop();
    }

    fn generics(&mut self, gs: &[GenericParam]) {
        for g in gs {
            self.bind_local(&g.name, DefKind::GenericParam);
        }
        for g in gs {
            for bound in &g.bounds {
                self.ty(bound);
            }
        }
    }

    fn func(&mut self, f: &FnDecl) {
        let saved = self.owner;
        self.push();
        self.generics(&f.sig.generics);
        for p in &f.sig.params {
            if let Param::Named {
                name, ty, default, ..
            } = p
            {
                self.ty(ty);
                if let Some(e) = default {
                    self.expr(e);
                }
                self.bind_local(name, DefKind::Param);
            }
        }
        if let Some(r) = &f.sig.ret {
            self.ty(r);
        }
        match &f.body {
            FnBody::Block(b) => self.block(b),
            FnBody::Expr(e) => {
                self.expr(e);
            }
            FnBody::None => {}
        }
        self.pop();
        self.owner = saved;
    }

    // ---- statements ----

    fn block(&mut self, b: &Block) {
        self.push();
        self.stmts(&b.stmts);
        self.pop();
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Expr(e) => {
                self.expr(e);
            }
            StmtKind::Let { pat, ty, value } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(value);
                self.pat(pat);
            }
            StmtKind::Var { name, ty, value } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(value);
                self.bind_local(name, DefKind::Local);
            }
            StmtKind::Assign { place, value, .. } => {
                self.expr(place);
                self.expr(value);
            }
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    self.expr(e);
                }
            }
            StmtKind::For { pat, iter, body } => {
                self.expr(iter);
                self.push();
                self.pat(pat);
                self.stmts(&body.stmts);
                self.pop();
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            StmtKind::Fn(f) => {
                let local = match &f.sig.name.kind {
                    NameKind::Ident(s) => {
                        let id = Ident {
                            name: s.clone(),
                            span: f.sig.name.span,
                        };
                        self.bind_local(&id, DefKind::LocalFn)
                    }
                    NameKind::Core(c) => {
                        let text = self.b.table.spelling(*c, self.skin).to_string();
                        self.b.diags.push(
                            Diagnostic::new("K0413", self.file, f.sig.name.span)
                                .arg("name", format!("`{text}`")),
                        );
                        None
                    }
                };
                let saved = self.owner;
                self.owner = local.or(saved);
                self.func(f);
                self.owner = saved;
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Pass | StmtKind::Error => {}
        }
    }

    // ---- expressions ----

    fn expr(&mut self, e: &Expr) -> Option<DefId> {
        kalam_ast::stack::deeper(|| self.expr_inner(e))
    }

    fn strlit(&mut self, lit: &StrLit) {
        for part in &lit.parts {
            if let StrPart::Hole(e) = part {
                self.expr(e);
            }
        }
    }

    fn expr_inner(&mut self, e: &Expr) -> Option<DefId> {
        match &e.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Unit
            | ExprKind::SelfValue
            | ExprKind::UnsafeMember(_)
            | ExprKind::Error => None,
            ExprKind::Str(lit) | ExprKind::Tagged { lit, .. } => {
                self.strlit(lit);
                None
            }
            ExprKind::Name(NameKind::Ident(s)) => self.ident_use(s, e.span, Ctx::Value),
            ExprKind::Name(NameKind::Core(c)) => self.core_use(*c, e.span),
            ExprKind::ImplicitMember(id) => {
                self.implicit(id);
                None
            }
            ExprKind::Paren(x) | ExprKind::Try(x) | ExprKind::Neg(x) | ExprKind::Not(x) => {
                self.expr(x);
                None
            }
            ExprKind::Tuple(xs) | ExprKind::List(xs) => {
                for x in xs {
                    self.expr(x);
                }
                None
            }
            ExprKind::Map(pairs) => {
                for (k, v) in pairs {
                    self.expr(k);
                    self.expr(v);
                }
                None
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee);
                for a in args {
                    self.expr(&a.value);
                }
                None
            }
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
                None
            }
            ExprKind::Member { base, member } => {
                let target = self.expr(base);
                match (target, member) {
                    (Some(t), Member::Name(id)) => self.member(t, id),
                    _ => None,
                }
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
                None
            }
            ExprKind::Range { lo, hi, .. } => {
                self.expr(lo);
                self.expr(hi);
                None
            }
            ExprKind::Lambda(l) => {
                self.push();
                for p in &l.params {
                    if let Some(t) = &p.ty {
                        self.ty(t);
                    }
                    self.bind_local(&p.name, DefKind::Param);
                }
                if let Some(r) = &l.ret {
                    self.ty(r);
                }
                self.expr(&l.body);
                self.pop();
                None
            }
            ExprKind::If(ifx) => {
                for br in &ifx.branches {
                    self.expr(&br.cond);
                    self.block(&br.body);
                }
                if let Some(b) = &ifx.else_body {
                    self.block(b);
                }
                None
            }
            ExprKind::Match(mx) => {
                self.expr(&mx.scrutinee);
                for arm in &mx.arms {
                    self.push();
                    self.pat(&arm.pat);
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    self.stmts(&arm.body.stmts);
                    self.pop();
                }
                None
            }
        }
    }

    // ---- types and patterns ----

    fn ty(&mut self, t: &Type) {
        kalam_ast::stack::deeper(|| self.ty_inner(t))
    }

    fn ty_inner(&mut self, t: &Type) {
        match &t.kind {
            TypeKind::Named { path, args } => {
                self.path_use(path, Ctx::Type);
                for a in args {
                    self.ty(a);
                }
            }
            TypeKind::Tuple(ts) => {
                for x in ts {
                    self.ty(x);
                }
            }
            TypeKind::Fn { params, ret, .. } => {
                for x in params {
                    self.ty(x);
                }
                self.ty(ret);
            }
            TypeKind::Unit | TypeKind::Error => {}
        }
    }

    fn pat(&mut self, p: &Pat) {
        kalam_ast::stack::deeper(|| self.pat_inner(p))
    }

    fn pat_fields(&mut self, fields: &Option<PatFields>) {
        for f in fields.iter().flat_map(|f| &f.fields) {
            self.pat(&f.pat);
        }
    }

    fn pat_inner(&mut self, p: &Pat) {
        match &p.kind {
            PatKind::Wild
            | PatKind::Int(_)
            | PatKind::Range { .. }
            | PatKind::Str(_)
            | PatKind::Bool(_)
            | PatKind::Error => {}
            PatKind::Bind(id) => {
                self.bind_local(id, DefKind::Local);
            }
            PatKind::Implicit { name, fields } => {
                self.implicit(name);
                self.pat_fields(fields);
            }
            PatKind::Path { path, fields } => {
                self.path_use(path, Ctx::Value);
                self.pat_fields(fields);
            }
            PatKind::Ctor { inner, .. } => {
                if let Some(x) = inner {
                    self.pat(x);
                }
            }
            PatKind::Tuple(ps) => {
                for x in ps {
                    self.pat(x);
                }
            }
            PatKind::List(elems) => {
                for el in elems {
                    match el {
                        ListPatElem::Pat(x) => self.pat(x),
                        ListPatElem::Rest(Some(id)) => {
                            self.bind_local(id, DefKind::Local);
                        }
                        ListPatElem::Rest(None) => {}
                    }
                }
            }
            PatKind::Or(alts) => {
                let outer = self.or_seen.take();
                let mut seen = HashMap::new();
                for (i, alt) in alts.iter().enumerate() {
                    self.or_seen = Some((std::mem::take(&mut seen), i > 0));
                    self.pat(alt);
                    seen = self.or_seen.take().map(|(s, _)| s).unwrap_or_default();
                }
                self.or_seen = outer.map(|(mut map, reuse)| {
                    map.extend(seen);
                    (map, reuse)
                });
            }
            PatKind::As { pat, name } => {
                self.pat(pat);
                self.bind_local(name, DefKind::Local);
            }
        }
    }
}
