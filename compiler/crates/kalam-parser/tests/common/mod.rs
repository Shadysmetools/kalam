//! Shared helpers for the parser integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use kalam_ast::{shape, ShapeOptions};
use kalam_base::{FileId, Skin};
use kalam_parser::{parse, ParseOutput};

pub fn repo_root() -> PathBuf {
    // compiler/crates/kalam-parser → new-language
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root")
}

/// Lexes and parses, returning the parser output and the *lexer* diagnostics separately.
pub fn parse_text(text: &str, skin: Skin) -> (ParseOutput, Vec<&'static str>) {
    let lexed = kalam_lexer::lex(text, skin, FileId(0));
    let lex_codes = lexed.diagnostics.iter().map(|d| d.code).collect();
    (parse(text, &lexed, skin, FileId(0)), lex_codes)
}

pub fn parser_codes(out: &ParseOutput) -> Vec<&'static str> {
    out.diagnostics.iter().map(|d| d.code).collect()
}

pub fn shape_of(out: &ParseOutput) -> String {
    shape(&out.ast, ShapeOptions::default())
}

/// A corpus case directory with its three skin files.
pub struct Case {
    pub name: String,
    pub files: Vec<(Skin, PathBuf)>,
}

/// Every `eval/corpus/<task>/` and `eval/corpus/<task>/<case>/` holding `{en,ar,franco}.kalam`.
/// The `.kalam` extension declares no skin (INDEX.md §2, E13), so the stem names it.
pub fn corpus_cases() -> Vec<Case> {
    let root = repo_root().join("eval").join("corpus");
    let mut dirs = Vec::new();
    for task in std::fs::read_dir(&root).expect("eval/corpus") {
        let task = task.unwrap().path();
        if !task.is_dir() {
            continue;
        }
        dirs.push(task.clone());
        for sub in std::fs::read_dir(&task).unwrap() {
            let sub = sub.unwrap().path();
            if sub.is_dir() {
                dirs.push(sub);
            }
        }
    }
    dirs.sort();
    let mut cases = Vec::new();
    for d in dirs {
        let mut files = Vec::new();
        for skin in Skin::ALL {
            let p = d.join(format!("{}.kalam", skin.id()));
            if p.exists() {
                files.push((skin, p));
            }
        }
        if !files.is_empty() {
            let name = d
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            cases.push(Case { name, files });
        }
    }
    cases
}

/// One ```kalam block of spec §28.
pub struct SpecBlock {
    /// e.g. `hello.ar.kl`
    pub file: String,
    pub skin: Skin,
    /// `[converter]` blocks are exact conversions; `[view: glossary]` blocks translate
    /// identifiers too (spec §4.5), so only their identifier-free shape can match.
    pub glossary: bool,
    /// The block without its first (`# file.kl [tag]`) header line.
    pub body: String,
    /// 1-based section, e.g. `28.3`.
    pub section: String,
}

pub fn spec_section_28() -> Vec<SpecBlock> {
    let spec = std::fs::read_to_string(repo_root().join("spec").join("KALAM-SPEC.md")).unwrap();
    let start = spec.find("## 28. Example programs").expect("§28");
    let end = spec[start..]
        .find("\n## 29")
        .map(|e| start + e)
        .unwrap_or(spec.len());
    let text = &spec[start..end];
    let mut blocks = Vec::new();
    let mut section = String::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("### ") {
            section = rest.split_whitespace().next().unwrap_or("").to_string();
        }
        if line.trim() != "```kalam" {
            continue;
        }
        let header = lines.next().unwrap_or("");
        let mut body = String::new();
        for l in lines.by_ref() {
            if l.trim() == "```" {
                break;
            }
            body.push_str(l);
            body.push('\n');
        }
        let file = header
            .trim_start_matches('#')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        let skin = Skin::from_path(&file).expect("header names a .kl file");
        blocks.push(SpecBlock {
            file,
            skin,
            glossary: header.contains("glossary"),
            body,
            section: section.clone(),
        });
    }
    blocks
}
