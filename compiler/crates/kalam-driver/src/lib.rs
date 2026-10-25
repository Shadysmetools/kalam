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
    fn json_output_is_an_array() {
        let checked = check_source("a.kl", "x = 007\n").unwrap();
        let json = checked.render_json();
        assert!(json.starts_with('[') && json.contains("K0125"));
    }
}
