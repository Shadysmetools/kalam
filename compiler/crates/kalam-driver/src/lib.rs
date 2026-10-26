//! The compilation driver: session, virtual file system, module graph, query cache and
//! pipeline. CLI, LSP, MCP, the test runner and the WASM playground all call into this
//! crate, so they produce identical results (ARCHITECTURE.md §2, §3).
//!
//! Frontend crates never touch the OS; all file access goes through [`Vfs`], which is an
//! in-memory map in the playground and the real file system in the CLI.
//!
//! Incrementality: Phase 0 caches per file by content hash; Phase 1 moves to `salsa`
//! queries. Dependency packages are never re-type-checked: their `.kli` interfaces are used.
//!
//! Status: [`check_source`] runs the lexer, the parser, name resolution and the type
//! checker on one file; [`check_project`] does the same for a whole package, optionally
//! with a stdlib directory; [`run_interp`] runs one file on the reference interpreter
//! (`kalam run --interp`, module `run`). Each stage runs only when the stages before it found no error
//! (root cause first, ARCHITECTURE.md §12.4). Each later stage is added as it lands, in the
//! order of spec §4.2.
//!
//! **Type checking one file.** `kalam check FILE` resolves the file on its own (every
//! import opaque), exactly as before, and reports what that finds. When that is clean, the
//! file is type-checked against the stdlib signature stubs, which this crate embeds
//! (`stdlib/src`, the same text `kalam check --std` reads): the file is resolved a second
//! time together with them, with the stdlib loaded as project mode loads it, so `xs.len()`
//! and `use std/http` have their real types, and a `std/…` module or member the stubs do
//! not have is K0401 in both modes (FOUNDER-QUEUE Q34). What that second resolution finds
//! in the file is reported, and the type check then waits for it. Imports of the file's
//! other package modules stay opaque.

use std::collections::BTreeMap;
use std::sync::OnceLock;

mod fix;
mod run;
pub use fix::{fix_file, unified_diff, AppliedFix, FixedFile, WithheldFix, MAX_FIX_ROUNDS};
pub use run::{run_interp, RunOutcome, MAX_CALL_DEPTH};

use kalam_base::{FileId, LineIndex, Skin};
use kalam_diagnostics::{render_human, render_json, Diagnostic, Lang, SourceView};
pub use kalam_fmt::{ArDigits, FmtConfig};

/// Read-only file access for the frontend.
pub trait Vfs {
    fn read(&self, path: &str) -> Option<String>;
    fn list(&self, dir: &str) -> Vec<String>;
}

/// An in-memory VFS (playground, tests, LSP overlays).
#[derive(Clone, Debug, Default)]
pub struct MemoryVfs {
    pub files: BTreeMap<String, String>,
}

impl Vfs for MemoryVfs {
    fn read(&self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        self.files
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect()
    }
}

/// The result of checking one file.
pub struct CheckedFile {
    pub path: String,
    pub skin: Skin,
    pub text: String,
    pub lines: LineIndex,
    pub diagnostics: Vec<Diagnostic>,
}

impl CheckedFile {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }

    pub fn view(&self) -> SourceView<'_> {
        SourceView::new(&self.path, &self.text, self.skin, &self.lines)
    }

    /// Terminal rendering of every diagnostic in `lang` (`KALAM_LANG`).
    pub fn render_human(&self, lang: Lang) -> String {
        let view = self.view();
        self.diagnostics
            .iter()
            .map(|d| render_human(d, &view, lang, None))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The `kalam check --json` array (spec §21.4).
    pub fn render_json(&self) -> String {
        let view = self.view();
        let values: Vec<_> = self
            .diagnostics
            .iter()
            .map(|d| render_json(d, &view))
            .collect();
        serde_json_array(&values)
    }
}

fn serde_json_array(values: &[impl std::fmt::Display]) -> String {
    let items: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    format!("[{}]", items.join(","))
}

/// Error for paths that do not name a Kalam source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotKalamFile(pub String);

/// Checks one file's text. The skin comes from the file name (DR-18).
pub fn check_source(path: &str, text: &str) -> Result<CheckedFile, NotKalamFile> {
    let skin = Skin::from_path(path).ok_or_else(|| NotKalamFile(path.to_string()))?;
    // Lexer and parser (FOUNDER-QUEUE Q17): every K01xx, K02xx, K07xx, K09xx and SEC
    // finding they make, in source order. Then name resolution. Next stages, in spec §4.2
    // order: typeck → effects → sinks/caps.
    let mut parsed = kalam_parser::parse_source(text, skin, FileId(0));
    let diagnostics = std::mem::take(&mut parsed.diagnostics);
    let parse = diagnostics.clone();
    let diagnostics = resolve_single(skin, &parsed, diagnostics);
    let diagnostics = typecheck_single(skin, &parsed, &parse, diagnostics);
    Ok(CheckedFile {
        path: path.to_string(),
        skin,
        text: text.to_string(),
        lines: LineIndex::new(text),
        diagnostics,
    })
}

/// Name resolution of one file (`kalam_resolve::resolve_file`: every `use` is opaque),
/// after the lexer and parser. It runs only when they reported no error: names declared
/// inside unparsable code would otherwise come back as follow-on K0401s (root cause first,
/// ARCHITECTURE.md §12.4). Its findings come after theirs (phase order), in source order.
fn resolve_single(
    skin: Skin,
    parsed: &kalam_parser::ParseOutput,
    diagnostics: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    if diagnostics.iter().any(Diagnostic::is_error) {
        return diagnostics;
    }
    let (_, resolved) = kalam_resolve::resolve_file(FileId(0), skin, &parsed.ast);
    append_resolved(diagnostics, resolved)
}

/// The stdlib signature stubs (`stdlib/src`), embedded so a single-file check can type
/// against them. Module `std/<name>` for `<name>.kl`.
const STDLIB: &[(&str, &str)] = &[
    ("bigint", include_str!("../../../../stdlib/src/bigint.kl")),
    ("decimal", include_str!("../../../../stdlib/src/decimal.kl")),
    ("env", include_str!("../../../../stdlib/src/env.kl")),
    ("fs", include_str!("../../../../stdlib/src/fs.kl")),
    ("http", include_str!("../../../../stdlib/src/http.kl")),
    ("json", include_str!("../../../../stdlib/src/json.kl")),
    ("list", include_str!("../../../../stdlib/src/list.kl")),
    ("log", include_str!("../../../../stdlib/src/log.kl")),
    ("map", include_str!("../../../../stdlib/src/map.kl")),
    ("math", include_str!("../../../../stdlib/src/math.kl")),
    ("option", include_str!("../../../../stdlib/src/option.kl")),
    ("path", include_str!("../../../../stdlib/src/path.kl")),
    ("prelude", include_str!("../../../../stdlib/src/prelude.kl")),
    ("proc", include_str!("../../../../stdlib/src/proc.kl")),
    ("random", include_str!("../../../../stdlib/src/random.kl")),
    ("result", include_str!("../../../../stdlib/src/result.kl")),
    ("set", include_str!("../../../../stdlib/src/set.kl")),
    ("sql", include_str!("../../../../stdlib/src/sql.kl")),
    ("string", include_str!("../../../../stdlib/src/string.kl")),
    ("task", include_str!("../../../../stdlib/src/task.kl")),
    ("time", include_str!("../../../../stdlib/src/time.kl")),
];

/// The embedded stubs, parsed once: module path and parse. `None` if one of them does
/// not parse (then single-file checks stop after name resolution).
type StdModules = Vec<(kalam_resolve::ModulePath, kalam_parser::ParseOutput)>;

fn embedded_stdlib() -> Option<&'static StdModules> {
    static PARSED: OnceLock<Option<StdModules>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            let mut out = Vec::new();
            for (i, (name, text)) in STDLIB.iter().enumerate() {
                let parsed = kalam_parser::parse_source(text, Skin::En, FileId(1 + i as u32));
                if parsed.diagnostics.iter().any(Diagnostic::is_error) {
                    return None;
                }
                out.push((kalam_resolve::ModulePath(format!("std/{name}")), parsed));
            }
            Some(out)
        })
        .as_ref()
}

/// The type checker on one file (see the crate docs), after a clean lex and parse
/// (`parse`: their findings) and resolve. Its findings come after the earlier ones, in
/// source order.
fn typecheck_single(
    skin: Skin,
    parsed: &kalam_parser::ParseOutput,
    parse: &[Diagnostic],
    diagnostics: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    if parse.iter().any(Diagnostic::is_error) {
        return diagnostics;
    }
    let Some(std) = embedded_stdlib() else {
        return diagnostics;
    };
    let sources = with_std(skin, &parsed.ast, std);
    let (program, resolved) = resolve_with_std(&sources);
    if resolved.iter().any(Diagnostic::is_error) {
        // What resolving against the stubs finds in the file is reported, as project mode
        // reports it (FOUNDER-QUEUE Q34), and the type check waits for it. An error in a
        // stub itself would be a broken stdlib: then the type check is skipped silently.
        let (own, stubs): (Vec<Diagnostic>, Vec<Diagnostic>) =
            resolved.into_iter().partition(|d| d.file == FileId(0));
        if stubs.iter().any(Diagnostic::is_error) {
            return diagnostics;
        }
        // In project mode's order: the lexer's and parser's findings, then resolve's, then
        // anything only the opaque pass found. So a missing stdlib module or member is
        // reported even when the file has other name errors.
        return append_resolved(append_resolved(parse.to_vec(), own), diagnostics);
    }
    if diagnostics.iter().any(Diagnostic::is_error) {
        return diagnostics;
    }
    // Type checking plus suspension inference (K1401, K1402, W0421; spec §15).
    let typed = kalam_effects::diagnose_against_std(&program, &sources)
        .into_iter()
        .filter(|d| d.file == FileId(0))
        .collect();
    append_resolved(diagnostics, typed)
}

/// The single-file program: the file as module `main` (file 0), then the embedded stdlib
/// stubs (files 1, 2, …).
pub(crate) fn with_std<'a>(
    skin: Skin,
    ast: &'a kalam_ast::Ast,
    std: &'a StdModules,
) -> Vec<kalam_resolve::SourceModule<'a>> {
    let mut sources = vec![kalam_resolve::SourceModule {
        path: kalam_resolve::ModulePath("main".to_string()),
        file: FileId(0),
        skin,
        ast,
        std: false,
    }];
    for (i, (path, out)) in std.iter().enumerate() {
        sources.push(kalam_resolve::SourceModule {
            path: path.clone(),
            file: FileId(1 + i as u32),
            skin: Skin::En,
            ast: &out.ast,
            std: true,
        });
    }
    sources
}

/// Resolves [`with_std`]'s program the way project mode resolves a package with the stdlib
/// directory (FOUNDER-QUEUE Q34): the stdlib is loaded, so a `std/…` module the stubs do
/// not have, or a member a stub module lacks, is K0401. The file's own package is not
/// loaded, so its imports of other package modules stay opaque.
pub(crate) fn resolve_with_std(
    sources: &[kalam_resolve::SourceModule<'_>],
) -> (kalam_resolve::Program, Vec<Diagnostic>) {
    let options = kalam_resolve::Options {
        std_loaded: true,
        ..kalam_resolve::Options::default()
    };
    kalam_resolve::resolve_program(sources, &options)
}

/// `earlier` followed by the resolver's findings, minus any that repeat an earlier finding
/// at the same place with the same code.
fn append_resolved(mut earlier: Vec<Diagnostic>, resolved: Vec<Diagnostic>) -> Vec<Diagnostic> {
    for d in resolved {
        if !earlier.iter().any(|e| e.code == d.code && e.span == d.span) {
            earlier.push(d);
        }
    }
    earlier
}

/// `path` relative to `dir` (`dir/a/b.kl` → `a/b.kl`); `path` itself if it is not under it.
fn relative(path: &str, dir: &str) -> String {
    path.strip_prefix(dir)
        .and_then(|r| r.strip_prefix('/'))
        .unwrap_or(path)
        .to_string()
}

/// What [`check_project`] needs besides the files.
#[derive(Clone, Debug, Default)]
pub struct ProjectOptions {
    /// The package's short name: the first segment of its module paths (`notes` for the
    /// package `@team/notes`, whose modules are imported as `use notes/db`).
    pub package: String,
    /// The stdlib directory: every `.kl` file under it is the module `std/<path>` (spec
    /// §20.1). `None` leaves `std/…` imports opaque (unchecked).
    pub std_dir: Option<String>,
}

/// `kalam check` over a whole package: every Kalam file under `<root>/src` is one module
/// (spec §13.1), resolved together with the stdlib modules. Returns one [`CheckedFile`] per
/// package file (in path order), then one per stdlib file that has diagnostics. `vfs.list`
/// must list files recursively.
pub fn check_project(vfs: &dyn Vfs, root: &str, options: &ProjectOptions) -> Vec<CheckedFile> {
    let root = root.trim_end_matches('/');
    let kalam_files = |dir: &str| -> Vec<String> {
        let mut files: Vec<String> = vfs
            .list(dir)
            .into_iter()
            .filter(|p| Skin::from_path(p).is_some())
            .collect();
        files.sort();
        files
    };
    let src_dir = format!("{root}/src");
    let mut entries: Vec<(String, kalam_resolve::ModulePath, bool)> = kalam_files(&src_dir)
        .into_iter()
        .map(|p| {
            let rel = relative(&p, root);
            let module = kalam_resolve::ModulePath::from_file(&options.package, &rel);
            (p, module, false)
        })
        .collect();
    if let Some(std_dir) = &options.std_dir {
        let std_dir = std_dir.trim_end_matches('/');
        for p in kalam_files(std_dir) {
            let rel = relative(&p, std_dir);
            let module = kalam_resolve::ModulePath::from_file("std", &rel);
            entries.push((p, module, true));
        }
    }
    let mut files = Vec::new();
    let mut parsed = Vec::new();
    for (i, (path, _, _)) in entries.iter().enumerate() {
        let skin = Skin::from_path(path).unwrap_or(Skin::En);
        let text = vfs.read(path).unwrap_or_default();
        let mut out = kalam_parser::parse_source(&text, skin, FileId(i as u32));
        let diagnostics = std::mem::take(&mut out.diagnostics);
        files.push(CheckedFile {
            path: path.clone(),
            skin,
            lines: LineIndex::new(&text),
            text,
            diagnostics,
        });
        parsed.push(out);
    }
    let sources: Vec<kalam_resolve::SourceModule<'_>> = entries
        .iter()
        .zip(&parsed)
        .enumerate()
        .map(|(i, ((_, module, std), out))| kalam_resolve::SourceModule {
            path: module.clone(),
            file: FileId(i as u32),
            skin: files[i].skin,
            ast: &out.ast,
            std: *std,
        })
        .collect();
    let resolve_options = kalam_resolve::Options {
        package: options.package.clone(),
        whole_package: true,
        std_loaded: options.std_dir.is_some(),
    };
    let (program, resolved) = kalam_resolve::resolve_program(&sources, &resolve_options);
    let mut per_file: Vec<Vec<Diagnostic>> = vec![Vec::new(); files.len()];
    for d in resolved {
        if let Some(list) = per_file.get_mut(d.file.0 as usize) {
            list.push(d);
        }
    }
    for (file, resolved) in files.iter_mut().zip(per_file) {
        // Same gate as `check_source`: a file with syntax errors keeps only those.
        if !file.has_errors() {
            let earlier = std::mem::take(&mut file.diagnostics);
            file.diagnostics = append_resolved(earlier, resolved);
        }
    }
    // The type checker runs on the whole program, and only when every file (stdlib
    // included) lexed, parsed and resolved without error.
    if !files.iter().any(CheckedFile::has_errors) {
        // Type checking plus suspension inference (spec §15).
        let typed = kalam_effects::diagnose(&program, &sources);
        let mut per_file: Vec<Vec<Diagnostic>> = vec![Vec::new(); files.len()];
        for d in typed {
            if let Some(list) = per_file.get_mut(d.file.0 as usize) {
                list.push(d);
            }
        }
        for (file, typed) in files.iter_mut().zip(per_file) {
            let earlier = std::mem::take(&mut file.diagnostics);
            file.diagnostics = append_resolved(earlier, typed);
        }
    }
    files
        .into_iter()
        .zip(&entries)
        .filter(|(f, (_, _, std))| !*std || !f.diagnostics.is_empty())
        .map(|(f, _)| f)
        .collect()
}

/// Checks one file as read from disk (`kalam check`): like [`check_source`], but invalid
/// UTF-8 is reported (SEC0100, one per invalid run, spec §5.1) instead of making the file
/// unreadable. The checked text has U+FFFD in place of each invalid run, so every span and
/// line points into it.
pub fn check_bytes(path: &str, bytes: &[u8]) -> Result<CheckedFile, NotKalamFile> {
    let skin = Skin::from_path(path).ok_or_else(|| NotKalamFile(path.to_string()))?;
    let lexed = kalam_lexer::lex_bytes(bytes, skin);
    let text = lexed.text.as_str();
    let mut parsed = kalam_parser::parse(text, &lexed.lexed, skin, FileId(0));
    let parser = std::mem::take(&mut parsed.diagnostics);
    let diagnostics =
        kalam_parser::merge_diagnostics(text, lexed.lexed.diagnostics.clone(), parser);
    let parse = diagnostics.clone();
    let diagnostics = resolve_single(skin, &parsed, diagnostics);
    let diagnostics = typecheck_single(skin, &parsed, &parse, diagnostics);
    Ok(CheckedFile {
        path: path.to_string(),
        skin,
        text: text.to_string(),
        lines: LineIndex::new(text),
        diagnostics,
    })
}

/// Why `kalam fmt` or `kalam convert` produced no text for a file.
pub enum SourceError {
    /// The path does not name a `.kl`, `.ar.kl` or `.franco.kl` file.
    NotKalam(NotKalamFile),
    /// The file has lexer or parser errors; nothing is printed (ARCHITECTURE.md §13).
    Diagnostics(CheckedFile),
}

impl std::fmt::Debug for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::NotKalam(n) => write!(f, "NotKalam({:?})", n.0),
            SourceError::Diagnostics(c) => {
                let codes: Vec<&str> = c.diagnostics.iter().map(|d| d.code).collect();
                write!(f, "Diagnostics({:?}: {codes:?})", c.path)
            }
        }
    }
}

fn source_error(path: &str, skin: Skin, text: &str, diagnostics: Vec<Diagnostic>) -> SourceError {
    SourceError::Diagnostics(CheckedFile {
        path: path.to_string(),
        skin,
        text: text.to_string(),
        lines: LineIndex::new(text),
        diagnostics,
    })
}

/// The output of `kalam fmt` for one file.
pub struct Formatted {
    pub text: String,
    /// Set when the layout could not be normalized (a comment inside a block header or
    /// between an item's parts): the text is canonically spelled but keeps its layout.
    pub layout_kept: Option<String>,
}

/// `kalam fmt`: the file's text in canonical layout, in its own skin (spec §21.1).
pub fn format_source(path: &str, text: &str, config: &FmtConfig) -> Result<Formatted, SourceError> {
    let skin = Skin::from_path(path)
        .ok_or_else(|| SourceError::NotKalam(NotKalamFile(path.to_string())))?;
    match kalam_fmt::format_strict(text, skin, config) {
        Ok(text) => Ok(Formatted {
            text,
            layout_kept: None,
        }),
        Err(kalam_fmt::FmtError::Source(d)) => Err(source_error(path, skin, text, d)),
        Err(kalam_fmt::FmtError::Unformattable(why)) => {
            let respelled = kalam_fmt::convert(text, skin, skin, config)
                .map_err(|d| source_error(path, skin, text, d))?;
            Ok(Formatted {
                text: respelled,
                layout_kept: Some(why),
            })
        }
    }
}

/// A file re-printed in another skin, with its new name.
pub struct Converted {
    /// The path with its skin suffix replaced (`users.kl` → `users.ar.kl`); the module name,
    /// and so every `use` line, stays the same (spec §4.1).
    pub path: String,
    pub text: String,
}

/// `kalam convert --to SKIN`: lossless conversion, layout kept (spec §4.3).
pub fn convert_source(
    path: &str,
    text: &str,
    to: Skin,
    config: &FmtConfig,
) -> Result<Converted, SourceError> {
    let from = Skin::from_path(path)
        .ok_or_else(|| SourceError::NotKalam(NotKalamFile(path.to_string())))?;
    let converted = kalam_fmt::convert(text, from, to, config)
        .map_err(|d| source_error(path, from, text, d))?;
    let stem = &path[..path.len() - from.extension().len()];
    Ok(Converted {
        path: format!("{stem}{}", to.extension()),
        text: converted,
    })
}

/// `kalam explain CODE`: the explain page in `lang` (`KALAM_LANG`), with every message
/// placeholder described rather than printed raw (spec §21.4 rule 5). `None` for a code
/// with no catalog entry. The code is matched case-insensitively.
pub fn explain(code: &str, lang: Lang) -> Option<String> {
    kalam_diagnostics::explain::page(code, lang)
}

/// `kalam explain --catalog`: every catalog code and its title in `lang`.
pub fn explain_catalog(lang: Lang) -> String {
    kalam_diagnostics::explain::catalog_list(lang)
}

/// `kalam explain --catalog --json`: every code × en/ar/franco (title, message template,
/// placeholder descriptions, explain page; `help` is `null` until the catalog has it).
pub fn explain_catalog_json() -> String {
    kalam_diagnostics::explain::catalog_json()
}

/// Build targets accepted by `kalam build --target`.
pub fn parse_target(name: &str) -> Option<kalam_ir::Target> {
    match name {
        "js-node" => Some(kalam_ir::Target::JsNode),
        "js-browser" => Some(kalam_ir::Target::JsBrowser),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_reads_skin_from_extension() {
        let ok = check_source("hello.ar.kl", "دالة رئيسي():\n    اطبع(\"x\")\n").unwrap();
        assert_eq!(ok.skin, Skin::Ar);
        assert!(!ok.has_errors());
        assert!(check_source("hello.txt", "").is_err());
    }

    #[test]
    fn format_source_formats_in_the_files_own_skin() {
        let cfg = FmtConfig::default();
        let out = format_source(
            "a.franco.kl",
            "dala ra2eesy( ):\n  \n    etb3(\"x\")\n",
            &cfg,
        );
        let out = out.unwrap();
        assert_eq!(out.text, "dala ra2isi():\n    ektb(\"x\")\n");
        assert!(out.layout_kept.is_none());
        // A comment inside a block header: re-spelled only, and said so.
        let kept = format_source(
            "b.franco.kl",
            "dala ra2eesy():\n    tool_ma f(  # why\n      b):\n        fot\n",
            &cfg,
        )
        .unwrap();
        assert_eq!(
            kept.text,
            "dala ra2isi():\n    tool_ma f(  # why\n      b):\n        fot\n"
        );
        assert!(kept.layout_kept.is_some());
        match format_source("a.kl", "fn main(:\n", &cfg) {
            Err(SourceError::Diagnostics(checked)) => assert!(checked.has_errors()),
            _ => panic!("a broken file is not formatted"),
        }
        assert!(matches!(
            format_source("a.txt", "", &cfg),
            Err(SourceError::NotKalam(_))
        ));
    }

    #[test]
    fn convert_source_renames_the_skin_suffix() {
        let cfg = FmtConfig::default();
        let c = convert_source(
            "src/hello.kl",
            "fn main():\n    print(\"x\")\n",
            Skin::Ar,
            &cfg,
        )
        .unwrap();
        assert_eq!(c.path, "src/hello.ar.kl");
        assert_eq!(c.text, "دالة رئيسي():\n    اطبع(\"x\")\n");
        let back = convert_source(&c.path, &c.text, Skin::Franco, &cfg).unwrap();
        assert_eq!(back.path, "src/hello.franco.kl");
        assert_eq!(back.text, "dala ra2isi():\n    ektb(\"x\")\n");
        assert!(matches!(
            convert_source("x.kl", "law = 1\n", Skin::Ar, &cfg),
            Err(SourceError::Diagnostics(_))
        ));
    }

    #[test]
    fn explain_describes_placeholders_and_lists_the_catalog() {
        let page = explain("k0201", Skin::En).expect("K0201 page");
        assert!(page.starts_with("K0201  WORD FROM ANOTHER SKIN"), "{page}");
        assert!(
            !page.contains("{found}") && !page.contains("{expected}"),
            "{page}"
        );
        let ar = explain("K0201", Skin::Ar).unwrap();
        assert!(ar.contains("كلمة من واجهة أخرى"), "{ar}");
        assert!(explain("K9999", Skin::En).is_none());
        assert!(explain_catalog(Skin::En).contains("K0201"));
        let json = explain_catalog_json();
        assert!(json.contains("\"K0201\"") && json.contains("\"franco\""));
    }

    #[test]
    fn check_runs_the_parser_too() {
        // FOUNDER-QUEUE Q17: `kalam check` reports lexer and parser diagnostics together.
        let codes = |path: &str, src: &str| -> Vec<&'static str> {
            check_source(path, src)
                .unwrap()
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect()
        };
        assert_eq!(codes("a.kl", "fn main():\n    let = 5\n"), vec!["K0250"]);
        assert_eq!(
            codes("a.kl", "print(\"x\")\nfn main():\n    pass\n"),
            vec!["K0255"]
        );
        // Known LLM mistakes (K09xx) come from the parser too.
        assert_eq!(codes("a.kl", "def main():\n    pass\n"), vec!["K0902"]);
        // Lexer and parser findings together, in source order.
        assert_eq!(
            codes("a.kl", "fn main():\n    f() = 1\n    let x = 007\n"),
            vec!["K0710", "K0125"]
        );
        // A clean file stays clean in every skin.
        assert!(codes("a.franco.kl", "dala ra2isi():\n    ektb(\"x\")\n").is_empty());
    }

    #[test]
    fn check_bytes_reports_invalid_utf8_and_parses_the_rest() {
        let codes = |path: &str, src: &[u8]| -> Vec<&'static str> {
            check_bytes(path, src)
                .unwrap()
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect()
        };
        assert_eq!(
            codes("a.kl", b"fn main():\n    print(\"a\xFFb\")\n"),
            vec!["SEC0100"]
        );
        assert_eq!(
            codes("a.kl", b"fn main():\n    print(\"a\xFFb\")\n    x === 1\n"),
            vec!["SEC0100", "K0901"]
        );
        // Valid UTF-8 gives exactly what `check_source` gives.
        let src = "def main():\n    let x = 007\n";
        let a = check_bytes("a.kl", src.as_bytes()).unwrap();
        let b = check_source("a.kl", src).unwrap();
        assert_eq!(a.diagnostics, b.diagnostics);
        assert!(check_bytes("a.txt", b"").is_err());
    }

    #[test]
    fn check_runs_name_resolution_after_a_clean_parse() {
        let codes = |path: &str, src: &str| -> Vec<&'static str> {
            check_source(path, src)
                .unwrap()
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect()
        };
        // SEC0104 (D19 fold) and K0401 with the declared spelling (FOUNDER-QUEUE Q7).
        assert_eq!(
            codes(
                "a.ar.kl",
                "دالة رئيسي():
    ليكن كتاب = 1
    ليكن کتاب = 2
"
            ),
            vec!["SEC0104"]
        );
        let used = check_source(
            "a.ar.kl",
            "دالة رئيسي():
    ليكن كتاب = 1
    اطبع(\"{کتاب}\")
",
        )
        .unwrap();
        assert_eq!(used.diagnostics.len(), 1);
        assert_eq!(used.diagnostics[0].code, "K0401");
        assert_eq!(used.diagnostics[0].fixes[0].edits[0].insert, "كتاب");
        // `check_bytes` resolves too, identically.
        let src = "fn main():
    let al = 1
    let a1 = 2
";
        let a = check_bytes("a.kl", src.as_bytes()).unwrap();
        assert_eq!(
            a.diagnostics,
            check_source("a.kl", src).unwrap().diagnostics
        );
        assert_eq!(codes("a.kl", src), vec!["SEC0104"]);
        // A syntax error suppresses resolution (no follow-on K0401 for `y`).
        assert_eq!(
            codes(
                "a.kl",
                "fn main():
    let = 5
    print(\"{y}\")
"
            ),
            vec!["K0250"]
        );
        // Imports of other modules are opaque in a single-file check (the stdlib stubs are
        // known to the type checker, so `Router` takes its type argument).
        assert!(codes(
            "a.kl",
            "use std/http.{Router}
use app/db
fn main():
    let r: Router[Int] = db.open()
"
        )
        .is_empty());
    }

    #[test]
    fn check_runs_the_type_checker_after_a_clean_resolve() {
        let codes = |path: &str, src: &str| -> Vec<&'static str> {
            check_source(path, src)
                .unwrap()
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect()
        };
        // Spec §9.8: `1 == 1.0` compares an `Int` with a `Float`.
        let probe = "fn main():\n    let same = 1 == 1.0\n    print(\"{same}\")\n";
        assert_eq!(codes("a.kl", probe), vec!["K0602"]);
        assert_eq!(
            codes(
                "a.ar.kl",
                "دالة رئيسي():\n    ليكن same = 1 == 1.0\n    اطبع(\"{same}\")\n"
            ),
            vec!["K0602"]
        );
        // Stdlib members are typed from the embedded stubs: `List.get` is an `Option`.
        assert_eq!(
            codes(
                "a.kl",
                "fn head(xs: List[Int]) -> Result[Int, String]:\n    let n = xs.get(0)?\n    return Ok(n)\n\nfn main():\n    let _ = head([2])\n"
            ),
            vec!["K1302"]
        );
        assert!(codes(
            "a.kl",
            "use std/http\n\nfn main():\n    let r = http.router()\n    print(\"x\")\n"
        )
        .is_empty());
        // A name error stops before the type checker: one root cause.
        assert_eq!(
            codes(
                "a.kl",
                "fn main():\n    let x: Int = nope\n    print(\"{x}\")\n"
            ),
            vec!["K0401"]
        );
        // `check_bytes` runs it too.
        let a = check_bytes("a.kl", probe.as_bytes()).unwrap();
        assert_eq!(a.diagnostics.len(), 1);
        assert_eq!(a.diagnostics[0].code, "K0602");
    }

    #[test]
    fn the_embedded_stdlib_is_the_stdlib_directory() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../stdlib/src");
        let mut on_disk: Vec<(String, String)> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "kl"))
            .map(|p| {
                let name = p.file_stem().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read_to_string(&p).unwrap())
            })
            .collect();
        on_disk.sort();
        let embedded: Vec<(String, String)> = STDLIB
            .iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect();
        assert_eq!(embedded, on_disk, "update STDLIB when stdlib/src changes");
        assert!(embedded_stdlib().is_some(), "every stub parses");
    }

    #[test]
    fn check_project_resolves_modules_and_the_stdlib() {
        let mut vfs = MemoryVfs::default();
        let mut add = |path: &str, text: &str| {
            vfs.files.insert(path.to_string(), text.to_string());
        };
        add(
            "proj/src/shapes.kl",
            "export enum Shape:
    Circle(r: Int)
    Empty
",
        );
        add(
            "proj/src/main.ar.kl",
            "استخدم app/shapes.{Shape}
استخدم std/http

دالة رئيسي():
    ليكن ش: Shape = .Empty
    ليكن ر = http.router()
    اطبع(\"x\")
",
        );
        add(
            "proj/src/broken.kl",
            "use app/nope
",
        );
        add(
            "std/http.kl",
            "export type Router:
    a: Int

export fn router() -> Router => Router(a: 1)
",
        );
        add(
            "std/log.kl",
            "export fn info(s: String):
    pass
",
        );
        add(
            "std/json.kl",
            "export fn encode(x: Int) -> String => \"\"
",
        );
        let opts = ProjectOptions {
            package: "app".into(),
            std_dir: Some("std".into()),
        };
        let checked = check_project(&vfs, "proj", &opts);
        let paths: Vec<&str> = checked.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "proj/src/broken.kl",
                "proj/src/main.ar.kl",
                "proj/src/shapes.kl"
            ]
        );
        let codes: Vec<Vec<&str>> = checked
            .iter()
            .map(|c| c.diagnostics.iter().map(|d| d.code).collect())
            .collect();
        assert_eq!(codes, vec![vec!["K0401"], vec![], vec![]]);
        // Without the stdlib, `std/…` is opaque, so nothing changes for this program.
        let opaque = check_project(
            &vfs,
            "proj",
            &ProjectOptions {
                std_dir: None,
                ..opts
            },
        );
        assert!(opaque[1].diagnostics.is_empty());
    }

    #[test]
    fn json_output_is_an_array() {
        let checked = check_source("a.kl", "x = 007\n").unwrap();
        let json = checked.render_json();
        assert!(json.starts_with('[') && json.contains("K0125"));
    }
}
