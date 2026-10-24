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
//! Status: [`check_source`] runs the lexer (the only finished stage). Each later stage is
//! added to [`check_source`] as it lands, in the order of spec §4.2.

use std::collections::BTreeMap;

use kalam_base::{FileId, LineIndex, Skin};
use kalam_diagnostics::{render_human, render_json, Diagnostic, Lang, SourceView};

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
        let values: Vec<_> = self.diagnostics.iter().map(|d| render_json(d, &view)).collect();
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
    let lexed = kalam_lexer::lex(text, skin, FileId(0));
    // Next stages, in spec §4.2 order: parse → resolve → typeck → effects → sinks/caps.
    Ok(CheckedFile {
        path: path.to_string(),
        skin,
        text: text.to_string(),
        lines: LineIndex::new(text),
        diagnostics: lexed.diagnostics,
    })
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
    fn json_output_is_an_array() {
        let checked = check_source("a.kl", "x = 007\n").unwrap();
        let json = checked.render_json();
        assert!(json.starts_with('[') && json.contains("K0125"));
    }
}
