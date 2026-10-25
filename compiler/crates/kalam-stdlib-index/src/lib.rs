//! The stdlib API index: every public item of the stdlib signature stubs in `stdlib/src`,
//! written to `stdlib/api-index.json`.
//!
//! Consumers: the K0420 hallucinated-API guard (spec §21.3: unknown methods get the closest
//! real names) and `llms.txt` / `llms-full.txt` generation (spec §21.1).
//!
//! **Conventions read from the stubs** (stdlib/README.md):
//! - `@builtin`: provided by the compiler and runtime. A builtin function's body is the
//!   placeholder `panic("builtin")`, and every `panic("builtin")` body needs `@builtin`.
//! - `@suspends`: a base suspending primitive (spec §15.2, ARCHITECTURE §9.1).
//! - `@capability` (on a type): a capability type (spec §16.1). `@handle`: a handle built
//!   from a capability (`Db`, spec §16.1 rule 5). `@console`: ambient console output (D20).
//! - `@stores_callbacks`: function-typed parameters are stored, never called, so the
//!   function itself does not suspend.
//!
//! **Computed fields.** `capability` lists the capability and handle types in the receiver
//! and parameter types, plus `console`. `suspends` is `yes` for `@suspends`, `if_callback`
//! for a function with a non-`@sync` function-typed parameter (effect-polymorphic, spec
//! §15.3), and `no` otherwise; it is absent for non-functions.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use kalam_ast::{
    Attribute, EnumDecl, ExprKind, ExternDecl, FieldDecl, FnBody, FnDecl, FnSig, GenericParam,
    ImplDecl, Item, ItemKind, Name, NameKind, Param, StrPart, TraitDecl, TriviaKind, Type,
    TypeBody, TypeDecl, TypeKind, Variant, Visibility,
};
use kalam_base::{FileId, Skin};
use kalam_diagnostics::Severity;
use kalam_skins::{Concept, KeywordTable};
use serde::Serialize;

/// The index file, relative to the stdlib root.
pub const INDEX_FILE: &str = "api-index.json";
/// The JSON schema id written into the index.
pub const SCHEMA: &str = "kalam-api-index/1";
/// The stub body every `@builtin` function has at M2.
const STUB_MESSAGE: &str = "builtin";
/// The module that holds the prelude (spec §13.1).
const PRELUDE: &str = "std/prelude";

/// Compiler-known marker traits (spec §9.7). The grammar has no member-less trait, so they
/// cannot be declared in the stubs.
const MARKER_TRAITS: &[(&str, &str)] = &[
    (
        "Eq",
        "`==` and `!=`. Derived automatically when every component is `Eq`.",
    ),
    (
        "Hash",
        "`Map` and `Set` keys. Derived automatically when every component is `Hash`.",
    ),
    (
        "Ord",
        "`<`, `<=`, `>`, `>=` and `sorted()`. Explicit `@derive(Ord)`: declaration order.",
    ),
    (
        "Encode",
        "JSON and wire output. Explicit `@derive(Encode)`; never for `Secret[T]` (K0612).",
    ),
    (
        "Decode",
        "Schema-first input (spec §18.3). Explicit `@derive(Decode)`.",
    ),
    (
        "Row",
        "Decoding SQL rows by column name. Explicit `@derive(Row)`.",
    ),
    (
        "Message",
        "Live-component messages (spec §20.3). Explicit `@derive(Message)`, enums only.",
    ),
    (
        "SqlParam",
        "Types accepted in a `sql\"…\"` hole (spec §17.3).",
    ),
    (
        "Iterable",
        "What `for` iterates: `List`, `Set`, `Map` (as pairs), ranges.",
    ),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Spellings {
    pub en: String,
    pub ar: String,
    pub franco: String,
}

/// One public stdlib item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Unique key. Prelude items: `print`, `Int`. Other module items: `std/http.serve`.
    /// Members (methods, fields, variants): `Owner.member`, such as `String.trim`.
    pub path: String,
    /// The module that declares the item.
    pub module: String,
    /// `module`, `type`, `enum`, `trait`, `const`, `fn`, `method`, `assoc_fn`, `field`,
    /// `variant`, `trait_method` or `core_ctor`.
    pub kind: String,
    pub name: String,
    /// The type or trait a member belongs to.
    pub owner: Option<String>,
    /// For impl members, the impl header (`impl[T: Eq] List[T]`, `impl DbError as Error`).
    pub receiver: Option<String>,
    pub signature: String,
    pub capability: Vec<String>,
    /// `yes`, `no` or `if_callback` for functions; absent otherwise.
    pub suspends: Option<String>,
    pub attributes: Vec<String>,
    /// English, Arabic and Franco spellings, for core names only (D21).
    pub spellings: Option<Spellings>,
    pub doc: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApiIndex {
    pub schema: String,
    pub source: String,
    pub items: Vec<Entry>,
}

impl ApiIndex {
    /// Pretty JSON with a final newline. Deterministic: items are sorted by path.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("the index serializes");
        text.push('\n');
        text
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.items
            .binary_search_by(|e| e.path.as_str().cmp(path))
            .ok()
            .map(|i| &self.items[i])
    }
}

/// `stdlib/` next to `compiler/` in this repository.
pub fn default_stdlib_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("stdlib")
}

/// The stub files under `<root>/src`, as sorted `/`-separated paths relative to `root`.
pub fn stub_files(root: &Path) -> Result<Vec<String>, String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) -> Result<(), String> {
        let rd = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = format!("{rel}/{name}");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, &rel, out)?;
            } else if name.ends_with(".kl") {
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(&root.join("src"), "src", &mut out)?;
    out.sort();
    Ok(out)
}

/// `src/net/http.kl` → `std/net/http`.
pub fn module_path(rel: &str) -> String {
    let stem = rel.strip_prefix("src/").unwrap_or(rel);
    let stem = stem.strip_suffix(".kl").unwrap_or(stem);
    format!("std/{stem}")
}

/// Reads and indexes every stub file under `<root>/src`.
pub fn generate(root: &Path) -> Result<ApiIndex, String> {
    let mut sources = Vec::new();
    for rel in stub_files(root)? {
        let text = fs::read_to_string(root.join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
        sources.push((rel, text));
    }
    generate_from_sources(&sources)
}

/// Indexes `(path relative to the stdlib root, text)` pairs (English skin).
pub fn generate_from_sources(sources: &[(String, String)]) -> Result<ApiIndex, String> {
    let mut modules = Vec::new();
    let mut errors = Vec::new();
    for (i, (rel, text)) in sources.iter().enumerate() {
        let out = kalam_parser::parse_source(text, Skin::En, FileId(i as u32));
        for d in &out.diagnostics {
            if d.severity == Severity::Error {
                errors.push(format!("{rel}: {} at byte {}", d.code, d.span.start));
            }
        }
        modules.push(Module {
            path: module_path(rel),
            text,
            ast: out.ast,
        });
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    // Pass 1: which type names are capabilities or handles.
    let mut gated = BTreeSet::new();
    for m in &modules {
        for item in &m.ast.file.items {
            if has_attr(&item.attrs, "capability") || has_attr(&item.attrs, "handle") {
                if let Some(name) = item_name(item) {
                    gated.insert(name);
                }
            }
        }
    }

    // The prelude and the modules it re-exports (`log`, `json`) need no `use`.
    let mut ambient: BTreeSet<String> = BTreeSet::from([PRELUDE.to_string()]);
    for m in modules.iter().filter(|m| m.is_prelude()) {
        for u in m.ast.file.uses.iter().filter(|u| u.export) {
            let path: Vec<&str> = u.path.iter().map(|i| i.name.as_str()).collect();
            ambient.insert(path.join("/"));
        }
    }

    // Pass 2: the entries.
    let mut cx = Cx {
        gated,
        ambient,
        items: Vec::new(),
        errors: Vec::new(),
    };
    for m in &modules {
        cx.module(m);
    }
    cx.synthesized();
    if !cx.errors.is_empty() {
        return Err(cx.errors.join("\n"));
    }
    let mut items = cx.items;
    items.sort_by(|a, b| a.path.cmp(&b.path));
    for pair in items.windows(2) {
        if pair[0].path == pair[1].path {
            return Err(format!("duplicate index path `{}`", pair[0].path));
        }
    }
    Ok(ApiIndex {
        schema: SCHEMA.to_string(),
        source: "stdlib/src".to_string(),
        items,
    })
}

struct Module<'a> {
    path: String,
    text: &'a str,
    ast: kalam_ast::Ast,
}

impl Module<'_> {
    fn is_prelude(&self) -> bool {
        self.path == PRELUDE
    }

    /// The path of a module-level item.
    fn item_path(&self, name: &str) -> String {
        if self.is_prelude() {
            name.to_string()
        } else {
            format!("{}.{name}", self.path)
        }
    }

    /// 1-based line of a byte offset (the trivia convention).
    fn line_of(&self, offset: u32) -> u32 {
        let end = (offset as usize).min(self.text.len());
        self.text.as_bytes()[..end]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() as u32
            + 1
    }

    /// The `##` lines directly above the line of `offset`, joined with spaces.
    fn doc_above(&self, offset: u32) -> String {
        let mut line = self.line_of(offset);
        let mut lines = Vec::new();
        while line > 1 {
            line -= 1;
            match self
                .ast
                .trivia
                .iter()
                .find(|t| t.line == line && t.kind == TriviaKind::DocComment)
            {
                Some(t) => lines.push(strip_comment(&t.text, "##")),
                None => break,
            }
        }
        lines.reverse();
        lines.join(" ")
    }

    /// The `#` comment lines at the top of the file, joined with spaces.
    fn header_doc(&self) -> String {
        let mut lines = Vec::new();
        let mut line = 1;
        while let Some(t) = self
            .ast
            .trivia
            .iter()
            .find(|t| t.line == line && t.kind == TriviaKind::Comment)
        {
            lines.push(strip_comment(&t.text, "#"));
            line += 1;
        }
        lines.join(" ")
    }
}

fn strip_comment(text: &str, marker: &str) -> String {
    let t = text.trim_end();
    let t = t.strip_prefix(marker).unwrap_or(t);
    t.strip_prefix(' ').unwrap_or(t).to_string()
}

struct Cx {
    gated: BTreeSet<String>,
    /// Modules in scope without `use`: the prelude and its `export use` re-exports.
    ambient: BTreeSet<String>,
    items: Vec<Entry>,
    errors: Vec<String>,
}

/// What an impl member needs from its impl block.
struct ImplCx<'a> {
    owner: String,
    header: String,
    trait_name: Option<String>,
    /// `From[DbError]` when the trait takes type arguments: one type may implement it
    /// several times, so the member path carries it (`HttpError.from@From[DbError]`).
    trait_args: Option<String>,
    self_ty: &'a Type,
}

impl Cx {
    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        m: &Module<'_>,
        path: String,
        kind: &str,
        name: &str,
        owner: Option<String>,
        receiver: Option<String>,
        signature: String,
        capability: Vec<String>,
        suspends: Option<&str>,
        attributes: Vec<String>,
        spellings: Option<Spellings>,
        doc: String,
    ) {
        self.items.push(Entry {
            path,
            module: m.path.clone(),
            kind: kind.to_string(),
            name: name.to_string(),
            owner,
            receiver,
            signature,
            capability,
            suspends: suspends.map(str::to_string),
            attributes,
            spellings,
            doc,
        });
    }

    fn module(&mut self, m: &Module<'_>) {
        let header = m.header_doc();
        let signature = if m.is_prelude() {
            "(imported implicitly)".to_string()
        } else {
            format!("use {}", m.path)
        };
        let attrs = if self.ambient.contains(&m.path) {
            vec!["prelude".to_string()]
        } else {
            Vec::new()
        };
        self.push(
            m,
            m.path.clone(),
            "module",
            m.path.rsplit('/').next().unwrap_or(&m.path),
            None,
            None,
            signature,
            Vec::new(),
            None,
            attrs,
            None,
            header,
        );
        for item in &m.ast.file.items {
            self.item(m, item);
        }
    }

    fn item(&mut self, m: &Module<'_>, item: &Item) {
        let public = item.vis != Visibility::Private;
        let doc = m.doc_above(item.span.start);
        let attrs = attr_names(&item.attrs);
        match &item.kind {
            ItemKind::Fn(f) => {
                self.check_stub(m, f, &item.attrs);
                if public {
                    let name = name_text(&f.sig.name);
                    let caps = self.capabilities(&f.sig, None, &item.attrs);
                    self.push(
                        m,
                        m.item_path(&name),
                        "fn",
                        &name,
                        None,
                        None,
                        fn_signature(&f.sig),
                        caps,
                        Some(suspends(&f.sig, &item.attrs)),
                        attrs,
                        core_spellings(&f.sig.name),
                        doc,
                    );
                }
            }
            ItemKind::Type(t) if public => self.type_decl(m, item, t, attrs, doc),
            ItemKind::Enum(e) if public => self.enum_decl(m, item, e, attrs, doc),
            ItemKind::Trait(t) if public => self.trait_decl(m, t, attrs, doc),
            ItemKind::Impl(i) => self.impl_decl(m, i, doc),
            ItemKind::Const(c) if public => {
                let name = name_text(&c.name);
                let ty = c.ty.as_ref().map(type_text).unwrap_or_default();
                self.push(
                    m,
                    m.item_path(&name),
                    "const",
                    &name,
                    None,
                    None,
                    format!("let {name}: {ty}"),
                    Vec::new(),
                    None,
                    attrs,
                    None,
                    doc,
                );
            }
            ItemKind::Extern(ExternDecl::Type { name, generics }) if public => {
                let n = name_text(name);
                let caps = self.type_caps(&n);
                self.push(
                    m,
                    m.item_path(&n),
                    "type",
                    &n,
                    None,
                    None,
                    format!("type {n}{}", generics_text(generics)),
                    caps,
                    None,
                    attrs,
                    core_spellings(name),
                    doc,
                );
            }
            ItemKind::Extern(ExternDecl::Fn(sig)) => self.errors.push(format!(
                "{}: `extern fn {}`: stub functions use `@builtin` and a `panic(\"builtin\")` body",
                m.path,
                name_text(&sig.name)
            )),
            _ => {}
        }
    }

    fn type_caps(&self, name: &str) -> Vec<String> {
        if self.gated.contains(name) {
            vec![name.to_string()]
        } else {
            Vec::new()
        }
    }

    fn type_decl(
        &mut self,
        m: &Module<'_>,
        item: &Item,
        t: &TypeDecl,
        attrs: Vec<String>,
        doc: String,
    ) {
        let name = name_text(&t.name);
        let head = format!("{name}{}", generics_text(&t.generics));
        let signature = match &t.body {
            TypeBody::Alias(ty) => format!("type {head} = {}", type_text(ty)),
            TypeBody::Record(_) if item.vis == Visibility::ExportOpaque => {
                format!("opaque type {head}")
            }
            TypeBody::Record(_) => format!("type {head}"),
        };
        let caps = self.type_caps(&name);
        self.push(
            m,
            m.item_path(&name),
            "type",
            &name,
            None,
            None,
            signature,
            caps.clone(),
            None,
            attrs,
            core_spellings(&t.name),
            doc,
        );
        if item.vis == Visibility::ExportOpaque {
            return;
        }
        if let TypeBody::Record(fields) = &t.body {
            for f in fields {
                self.field(m, &name, f, caps.clone());
            }
        }
    }

    fn field(&mut self, m: &Module<'_>, owner: &str, f: &FieldDecl, caps: Vec<String>) {
        let mut signature = format!("{}: {}", f.name.name, type_text(&f.ty));
        if let Some(d) = &f.default {
            let _ = write!(signature, " = {}", expr_text(&d.kind));
        }
        self.push(
            m,
            format!("{owner}.{}", f.name.name),
            "field",
            &f.name.name,
            Some(owner.to_string()),
            None,
            signature,
            caps,
            None,
            attr_names(&f.attrs),
            None,
            m.doc_above(f.span.start),
        );
    }

    fn enum_decl(
        &mut self,
        m: &Module<'_>,
        item: &Item,
        e: &EnumDecl,
        attrs: Vec<String>,
        doc: String,
    ) {
        let name = name_text(&e.name);
        let head = format!("{name}{}", generics_text(&e.generics));
        let opaque = item.vis == Visibility::ExportOpaque;
        let signature = if opaque {
            format!("opaque enum {head}")
        } else {
            format!("enum {head}")
        };
        self.push(
            m,
            m.item_path(&name),
            "enum",
            &name,
            None,
            None,
            signature,
            Vec::new(),
            None,
            attrs,
            None,
            doc,
        );
        if !opaque {
            for v in &e.variants {
                self.variant(m, &name, v);
            }
        }
    }

    fn variant(&mut self, m: &Module<'_>, owner: &str, v: &Variant) {
        let mut signature = v.name.name.clone();
        if let Some(fields) = &v.fields {
            let parts: Vec<String> = fields
                .iter()
                .map(|f| match &f.label {
                    Some(l) => format!("{}: {}", l.name, type_text(&f.ty)),
                    None => type_text(&f.ty),
                })
                .collect();
            let _ = write!(signature, "({})", parts.join(", "));
        }
        self.push(
            m,
            format!("{owner}.{}", v.name.name),
            "variant",
            &v.name.name,
            Some(owner.to_string()),
            None,
            signature,
            Vec::new(),
            None,
            attr_names(&v.attrs),
            None,
            m.doc_above(v.span.start),
        );
    }

    fn trait_decl(&mut self, m: &Module<'_>, t: &TraitDecl, attrs: Vec<String>, doc: String) {
        let name = name_text(&t.name);
        self.push(
            m,
            m.item_path(&name),
            "trait",
            &name,
            None,
            None,
            format!("trait {name}{}", generics_text(&t.generics)),
            Vec::new(),
            None,
            attrs,
            None,
            doc,
        );
        for f in &t.members {
            let member = name_text(&f.sig.name);
            self.push(
                m,
                format!("{name}.{member}"),
                "trait_method",
                &member,
                Some(name.clone()),
                None,
                fn_signature(&f.sig),
                Vec::new(),
                Some(suspends(&f.sig, &[])),
                Vec::new(),
                None,
                m.doc_above(f.span.start),
            );
        }
    }

    /// `impl_doc`: the `##` lines above the impl, used by members that have none (the
    /// usual case for trait impls such as `impl DbError as Error`).
    fn impl_decl(&mut self, m: &Module<'_>, i: &ImplDecl, impl_doc: String) {
        let Some(owner) = type_head(&i.ty) else {
            self.errors
                .push(format!("{}: an impl for a type with no name", m.path));
            return;
        };
        let mut header = format!("impl{} {}", generics_text(&i.generics), type_text(&i.ty));
        let trait_name = i.as_trait.as_ref().map(|t| {
            let _ = write!(header, " as {}", type_text(t));
            type_head(t).unwrap_or_default()
        });
        let trait_args = i.as_trait.as_ref().and_then(|t| match &t.kind {
            TypeKind::Named { args, .. } if !args.is_empty() => Some(type_text(t)),
            _ => None,
        });
        let icx = ImplCx {
            owner,
            header,
            trait_name,
            trait_args,
            self_ty: &i.ty,
        };
        for member in &i.members {
            self.check_stub(m, &member.func, &member.attrs);
            // Trait-impl methods are public through the trait; inherent ones need `export`.
            if member.export || icx.trait_name.is_some() {
                let mut doc = m.doc_above(member.span.start);
                if doc.is_empty() {
                    doc = impl_doc.clone();
                }
                self.impl_member(m, &icx, &member.func, &member.attrs, doc);
            }
        }
    }

    fn impl_member(
        &mut self,
        m: &Module<'_>,
        icx: &ImplCx<'_>,
        f: &FnDecl,
        attrs: &[Attribute],
        doc: String,
    ) {
        let name = name_text(&f.sig.name);
        let has_self = f
            .sig
            .params
            .iter()
            .any(|p| matches!(p, Param::SelfParam(_)));
        let kind = if has_self { "method" } else { "assoc_fn" };
        let receiver = has_self.then_some(icx.self_ty);
        let mut attr_list = attr_names(attrs);
        if let Some(t) = &icx.trait_name {
            attr_list.push(format!("impl:{t}"));
        }
        let caps = self.capabilities(&f.sig, receiver, attrs);
        let mut path = format!("{}.{name}", icx.owner);
        if let Some(t) = &icx.trait_args {
            let _ = write!(path, "@{t}");
        }
        self.push(
            m,
            path,
            kind,
            &name,
            Some(icx.owner.clone()),
            Some(icx.header.clone()),
            fn_signature(&f.sig),
            caps,
            Some(suspends(&f.sig, attrs)),
            attr_list,
            core_spellings(&f.sig.name),
            doc,
        );
    }

    /// `@builtin` ⟺ the body is exactly `panic("builtin")` (trait signatures excepted).
    fn check_stub(&mut self, m: &Module<'_>, f: &FnDecl, attrs: &[Attribute]) {
        let builtin = has_attr(attrs, "builtin");
        let stub = is_stub_body(&f.body);
        let name = name_text(&f.sig.name);
        if builtin && !stub {
            self.errors.push(format!(
                "{}: `{name}` is `@builtin` but its body is not `panic(\"{STUB_MESSAGE}\")`",
                m.path
            ));
        } else if stub && !builtin {
            self.errors.push(format!(
                "{}: `{name}` has a stub body but no `@builtin`",
                m.path
            ));
        }
    }

    /// Capability and handle types in the receiver and parameter types, plus `console`.
    fn capabilities(
        &self,
        sig: &FnSig,
        receiver: Option<&Type>,
        attrs: &[Attribute],
    ) -> Vec<String> {
        let mut found = Vec::new();
        if let Some(r) = receiver {
            self.collect_gated(r, &mut found);
        }
        for p in &sig.params {
            if let Param::Named { ty, .. } = p {
                self.collect_gated(ty, &mut found);
            }
        }
        if has_attr(attrs, "console") {
            found.push("console".to_string());
        }
        found
    }

    fn collect_gated(&self, ty: &Type, out: &mut Vec<String>) {
        match &ty.kind {
            TypeKind::Named { path, args } => {
                if let [only] = path.as_slice() {
                    let n = name_text(only);
                    if self.gated.contains(&n) && !out.contains(&n) {
                        out.push(n);
                    }
                }
                for a in args {
                    self.collect_gated(a, out);
                }
            }
            TypeKind::Tuple(items) => items.iter().for_each(|t| self.collect_gated(t, out)),
            // A callback's parameters are what the callee hands out, not what it needs.
            TypeKind::Fn { .. } | TypeKind::Unit | TypeKind::Error => {}
        }
    }

    /// Entries the grammar cannot declare: the core constructors and the marker traits.
    fn synthesized(&mut self) {
        let prelude = Module {
            path: PRELUDE.to_string(),
            text: "",
            ast: kalam_ast::Ast {
                file: kalam_ast::File {
                    uses: Vec::new(),
                    items: Vec::new(),
                    span: Default::default(),
                },
                trivia: Vec::new(),
            },
        };
        let ctors = [
            (
                Concept::CoreSome,
                "(value: T) -> Option[T]",
                "The present case of `Option`.",
            ),
            (
                Concept::CoreNone,
                " -> Option[T]",
                "The absent case of `Option`.",
            ),
            (
                Concept::CoreOk,
                "(value: T) -> Result[T, E]",
                "The success case of `Result`.",
            ),
            (
                Concept::CoreErr,
                "(error: E) -> Result[T, E]",
                "The failure case of `Result`.",
            ),
        ];
        for (concept, sig, doc) in ctors {
            let sp = spellings(concept);
            let name = sp.en.clone();
            self.push(
                &prelude,
                name.clone(),
                "core_ctor",
                &name,
                None,
                None,
                format!("{name}{sig}"),
                Vec::new(),
                None,
                vec!["builtin".to_string()],
                Some(sp),
                doc.to_string(),
            );
        }
        for (name, doc) in MARKER_TRAITS {
            let generics = if *name == "Iterable" { "[T]" } else { "" };
            self.push(
                &prelude,
                name.to_string(),
                "trait",
                name,
                None,
                None,
                format!("trait {name}{generics}"),
                Vec::new(),
                None,
                vec!["builtin".to_string(), "marker".to_string()],
                None,
                doc.to_string(),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// Attributes and stubs
// ---------------------------------------------------------------------------------------

fn attr_name(a: &Attribute) -> String {
    a.path
        .iter()
        .map(|i| i.name.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn attr_names(attrs: &[Attribute]) -> Vec<String> {
    attrs.iter().map(attr_name).collect()
}

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| attr_name(a) == name)
}

fn is_stub_body(body: &FnBody) -> bool {
    let FnBody::Expr(e) = body else {
        return false;
    };
    let ExprKind::Call { callee, args } = &e.kind else {
        return false;
    };
    let ExprKind::Name(NameKind::Core(Concept::CorePanic)) = &callee.kind else {
        return false;
    };
    match args.as_slice() {
        [arg] if arg.label.is_none() => match &arg.value.kind {
            ExprKind::Str(s) => {
                matches!(s.parts.as_slice(), [StrPart::Text(t)] if t == STUB_MESSAGE)
            }
            _ => false,
        },
        _ => false,
    }
}

fn suspends(sig: &FnSig, attrs: &[Attribute]) -> &'static str {
    if has_attr(attrs, "suspends") {
        return "yes";
    }
    if has_attr(attrs, "stores_callbacks") {
        return "no";
    }
    let calls_back = sig.params.iter().any(|p| match p {
        Param::Named { ty, .. } => matches!(ty.kind, TypeKind::Fn { sync: false, .. }),
        Param::SelfParam(_) => false,
    });
    if calls_back {
        "if_callback"
    } else {
        "no"
    }
}

// ---------------------------------------------------------------------------------------
// Printing names, types and signatures (canonical English skin)
// ---------------------------------------------------------------------------------------

fn spellings(c: Concept) -> Spellings {
    let t = KeywordTable::builtin();
    Spellings {
        en: t.spelling(c, Skin::En).to_string(),
        ar: t.spelling(c, Skin::Ar).to_string(),
        franco: t.spelling(c, Skin::Franco).to_string(),
    }
}

fn core_spellings(n: &Name) -> Option<Spellings> {
    match n.kind {
        NameKind::Core(c) => Some(spellings(c)),
        NameKind::Ident(_) => None,
    }
}

fn name_text(n: &Name) -> String {
    match &n.kind {
        NameKind::Ident(s) => s.clone(),
        NameKind::Core(c) => KeywordTable::builtin().spelling(*c, Skin::En).to_string(),
    }
}

fn item_name(item: &Item) -> Option<String> {
    match &item.kind {
        ItemKind::Type(t) => Some(name_text(&t.name)),
        ItemKind::Enum(e) => Some(name_text(&e.name)),
        ItemKind::Extern(ExternDecl::Type { name, .. }) => Some(name_text(name)),
        _ => None,
    }
}

/// `List[T]` → `List`.
fn type_head(t: &Type) -> Option<String> {
    match &t.kind {
        TypeKind::Named { path, .. } => path.last().map(name_text),
        _ => None,
    }
}

/// A type in canonical English text.
pub fn type_text(t: &Type) -> String {
    match &t.kind {
        TypeKind::Named { path, args } => {
            let mut s = path.iter().map(name_text).collect::<Vec<_>>().join(".");
            if !args.is_empty() {
                let args: Vec<String> = args.iter().map(type_text).collect();
                let _ = write!(s, "[{}]", args.join(", "));
            }
            s
        }
        TypeKind::Unit => "()".to_string(),
        TypeKind::Tuple(items) => {
            let items: Vec<String> = items.iter().map(type_text).collect();
            format!("({})", items.join(", "))
        }
        TypeKind::Fn { sync, params, ret } => {
            let params: Vec<String> = params.iter().map(type_text).collect();
            let prefix = if *sync { "@sync " } else { "" };
            format!("{prefix}fn({}) -> {}", params.join(", "), type_text(ret))
        }
        TypeKind::Error => "?".to_string(),
    }
}

fn generics_text(g: &[GenericParam]) -> String {
    if g.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = g
        .iter()
        .map(|p| {
            if p.bounds.is_empty() {
                p.name.name.clone()
            } else {
                let bounds: Vec<String> = p.bounds.iter().map(type_text).collect();
                format!("{}: {}", p.name.name, bounds.join(" + "))
            }
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

/// `fn name[T: Ord](self, a: Int, b: T = …) -> R` on one line.
pub fn fn_signature(sig: &FnSig) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| match p {
            Param::SelfParam(_) => "self".to_string(),
            Param::Named {
                name, ty, default, ..
            } => {
                let mut s = format!("{}: {}", name.name, type_text(ty));
                if let Some(d) = default {
                    let _ = write!(s, " = {}", expr_text(&d.kind));
                }
                s
            }
        })
        .collect();
    let mut s = format!(
        "fn {}{}({})",
        name_text(&sig.name),
        generics_text(&sig.generics),
        params.join(", ")
    );
    if let Some(r) = &sig.ret {
        let _ = write!(s, " -> {}", type_text(r));
    }
    s
}

/// Default values: simple literals are printed, anything else is `…`.
fn expr_text(e: &ExprKind) -> String {
    match e {
        ExprKind::Int(i) => i.text.clone(),
        ExprKind::Float(f) => f.text.clone(),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Unit => "()".to_string(),
        ExprKind::ImplicitMember(i) => format!(".{}", i.name),
        ExprKind::List(xs) if xs.is_empty() => "[]".to_string(),
        ExprKind::Map(xs) if xs.is_empty() => "{}".to_string(),
        ExprKind::Str(s) if !s.triple => match s.parts.as_slice() {
            [] => "\"\"".to_string(),
            [StrPart::Text(t)] => format!("\"{t}\""),
            _ => "…".to_string(),
        },
        ExprKind::Call { callee, args } if args.is_empty() => match &callee.kind {
            ExprKind::Name(NameKind::Ident(n)) => format!("{n}()"),
            _ => "…".to_string(),
        },
        _ => "…".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(src: &str) -> ApiIndex {
        generate_from_sources(&[("src/demo.kl".to_string(), src.to_string())]).unwrap()
    }

    #[test]
    fn module_path_strips_src_and_extension() {
        assert_eq!(module_path("src/string.kl"), "std/string");
        assert_eq!(module_path("src/net/http.kl"), "std/net/http");
    }

    #[test]
    fn signatures_print_on_one_line_with_generics_and_defaults() {
        let idx = index_of(
            "## Doc one.\n## Doc two.\n@builtin\nexport fn pick[T: Eq + Hash](xs: List[T], n: Int = 3, f: @sync fn(T) -> Bool) -> Option[(T, Int)] => panic(\"builtin\")\n",
        );
        let e = idx.find("std/demo.pick").unwrap();
        assert_eq!(
            e.signature,
            "fn pick[T: Eq + Hash](xs: List[T], n: Int = 3, f: @sync fn(T) -> Bool) -> Option[(T, Int)]"
        );
        assert_eq!(e.doc, "Doc one. Doc two.");
        assert_eq!(e.suspends.as_deref(), Some("no"));
        assert_eq!(e.kind, "fn");
    }

    #[test]
    fn private_items_and_inherent_members_without_export_are_skipped() {
        let idx = index_of(
            "@builtin\nfn hidden() => panic(\"builtin\")\n\nexport enum E:\n    A\n\nimpl E:\n    @builtin\n    fn secret(self) -> Int => panic(\"builtin\")\n\n    @builtin\n    export fn shown(self) -> Int => panic(\"builtin\")\n",
        );
        assert!(idx.find("std/demo.hidden").is_none());
        assert!(idx.find("E.secret").is_none());
        let shown = idx.find("E.shown").unwrap();
        assert_eq!(shown.kind, "method");
        assert_eq!(shown.receiver.as_deref(), Some("impl E"));
        assert_eq!(idx.find("E.A").unwrap().kind, "variant");
    }

    #[test]
    fn builtin_and_stub_body_must_agree() {
        let missing_attr = "export fn f() -> Int => panic(\"builtin\")\n";
        let err = generate_from_sources(&[("src/a.kl".into(), missing_attr.into())]).unwrap_err();
        assert!(err.contains("no `@builtin`"), "{err}");
        let real_body = "@builtin\nexport fn f() -> Int => 1\n";
        let err = generate_from_sources(&[("src/a.kl".into(), real_body.into())]).unwrap_err();
        assert!(err.contains("is not `panic"), "{err}");
    }

    #[test]
    fn extern_functions_are_rejected() {
        let src = "export extern fn f() -> Int\n";
        let err = generate_from_sources(&[("src/a.kl".into(), src.into())]).unwrap_err();
        assert!(err.contains("extern fn f"), "{err}");
    }

    #[test]
    fn parse_errors_are_reported() {
        let err = generate_from_sources(&[("src/a.kl".into(), "export fn (\n".into())]);
        assert!(err.is_err());
    }

    #[test]
    fn capabilities_follow_marked_types_through_type_arguments() {
        let idx = index_of(
            "@builtin\n@capability\nexport extern type Cap\n\n@builtin\nexport fn f(x: List[Cap], g: fn(Cap) -> Int) => panic(\"builtin\")\n",
        );
        let f = idx.find("std/demo.f").unwrap();
        assert_eq!(f.capability, vec!["Cap"]);
        assert_eq!(f.suspends.as_deref(), Some("if_callback"));
        assert_eq!(idx.find("std/demo.Cap").unwrap().capability, vec!["Cap"]);
    }

    #[test]
    fn output_is_deterministic_json_with_a_final_newline() {
        let src = "@builtin\nexport fn b() => panic(\"builtin\")\n\n@builtin\nexport fn a() => panic(\"builtin\")\n";
        let one = index_of(src).to_json();
        assert_eq!(one, index_of(src).to_json());
        assert!(one.ends_with("}\n"));
        assert!(one.find("std/demo.a").unwrap() < one.find("std/demo.b").unwrap());
    }
}
