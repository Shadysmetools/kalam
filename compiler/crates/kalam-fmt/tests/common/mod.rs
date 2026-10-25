//! Shared helpers for the formatter and converter integration tests (copied from the
//! parser's test helpers, which another crate cannot import).
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use kalam_ast::{shape, Ast, ShapeOptions, TriviaKind};
use kalam_base::{FileId, Skin};
use kalam_fmt::FmtConfig;

pub fn repo_root() -> PathBuf {
    // compiler/crates/kalam-fmt → new-language
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root")
}

pub fn cfg() -> FmtConfig {
    FmtConfig::default()
}

/// Parses `text`; `None` if the lexer or the parser reports an error.
pub fn parse_clean(text: &str, skin: Skin) -> Option<Ast> {
    let out = kalam_parser::parse_source(text, skin, FileId(0));
    if out.diagnostics.iter().any(|d| d.is_error()) {
        None
    } else {
        Some(out.ast)
    }
}

/// The skin-free tree without trivia and without one-line/indented block flags.
pub fn tree(ast: &Ast) -> String {
    kalam_fmt::normalized_shape(ast)
}

/// Comment and doc-comment texts in source order.
pub fn comments(ast: &Ast) -> Vec<String> {
    ast.trivia
        .iter()
        .filter(|t| t.kind != TriviaKind::BlankLine)
        .map(|t| t.text.clone())
        .collect()
}

/// The full shape, trivia line numbers included.
pub fn full_shape(ast: &Ast) -> String {
    shape(ast, ShapeOptions::default())
}

/// A corpus case directory with its three skin files.
pub struct Case {
    pub name: String,
    pub files: Vec<(Skin, PathBuf)>,
}

/// Every `eval/corpus/<task>/` and `eval/corpus/<task>/<case>/` holding `{en,ar,franco}.kalam`.
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
    pub file: String,
    pub skin: Skin,
    pub glossary: bool,
    pub body: String,
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

/// The two 3-hop cycles and every ordered pair, as skin paths starting at `from`.
pub fn round_trip_paths(from: Skin) -> Vec<Vec<Skin>> {
    let mut paths = Vec::new();
    for to in Skin::ALL {
        if to != from {
            paths.push(vec![from, to, from]);
        }
    }
    let others: Vec<Skin> = Skin::ALL.into_iter().filter(|s| *s != from).collect();
    paths.push(vec![from, others[0], others[1], from]);
    paths.push(vec![from, others[1], others[0], from]);
    paths
}

/// Converts along `path`, returning every intermediate text (the last is back in `path[0]`).
pub fn convert_path(text: &str, path: &[Skin]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = text.to_string();
    for w in path.windows(2) {
        cur = kalam_fmt::convert(&cur, w[0], w[1], &cfg()).map_err(|d| {
            format!(
                "{}->{}: {:?}",
                w[0],
                w[1],
                d.iter().map(|d| d.code).collect::<Vec<_>>()
            )
        })?;
        out.push(cur.clone());
    }
    Ok(out)
}
