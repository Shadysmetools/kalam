//! Property (FOUNDER-QUEUE Q16): `kalam fmt` and `kalam convert` output always passes its
//! own lexer and parser. For every corpus program, every suite case and every spec §28
//! example that parses cleanly, and for every target skin (the source skin included), the
//! output of `convert` and of `fmt` has no K0201 (another skin's word read as a name), no
//! K0250 (parse error) and no other error.
//!
//! Generated programs are covered by `generated.rs`, whose check parses every conversion step
//! cleanly, half of them leaving reserved words undeclared (the case Q16 is about).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use kalam_base::{FileId, Skin};

/// The error codes of `text` read in `skin` (lexer and parser).
fn errors(text: &str, skin: Skin) -> Vec<&'static str> {
    let out = kalam_parser::parse_source(text, skin, FileId(0));
    out.diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code)
        .collect()
}

/// Every `.kl` file under `dir`, recursively, with its skin.
fn kl_files(dir: &Path, out: &mut Vec<(Skin, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            kl_files(&p, out);
        } else if let Some(skin) = p
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(Skin::from_path)
        {
            out.push((skin, p));
        }
    }
}

/// Corpus programs, suite cases (never `eval/holdout`) and spec §28 blocks.
fn inputs() -> Vec<(String, Skin, String)> {
    let mut all = Vec::new();
    for case in corpus_cases() {
        for (skin, path) in case.files {
            let text = std::fs::read_to_string(&path).unwrap();
            all.push((format!("corpus/{}/{}", case.name, skin.id()), skin, text));
        }
    }
    let suites = repo_root().join("eval").join("suites");
    let mut files = Vec::new();
    kl_files(&suites, &mut files);
    for (skin, path) in files {
        let bytes = std::fs::read(&path).unwrap();
        let name = path
            .strip_prefix(&suites)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        all.push((
            format!("suites/{name}"),
            skin,
            String::from_utf8_lossy(&bytes).into_owned(),
        ));
    }
    for (i, b) in spec_section_28().into_iter().enumerate() {
        all.push((format!("spec §28 #{i} {}", b.file), b.skin, b.body));
    }
    all
}

#[test]
fn convert_and_fmt_output_passes_its_own_lexer_and_parser() {
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for (name, from, text) in inputs() {
        if !errors(&text, from).is_empty() {
            continue; // seeded-error and hazard cases: neither tool prints them
        }
        checked += 1;
        for to in Skin::ALL {
            match kalam_fmt::convert(&text, from, to, &cfg()) {
                Ok(out) => {
                    let e = errors(&out, to);
                    if !e.is_empty() {
                        failures.push(format!("{name}: convert {from}->{to} gives {e:?}\n{out}"));
                    }
                }
                Err(d) => failures.push(format!(
                    "{name}: convert {from}->{to} refused a clean file: {:?}",
                    d.iter().map(|d| d.code).collect::<Vec<_>>()
                )),
            }
        }
        match kalam_fmt::format(&text, from, &cfg()) {
            Ok(out) => {
                let e = errors(&out, from);
                if !e.is_empty() {
                    failures.push(format!("{name}: fmt gives {e:?}\n{out}"));
                }
            }
            Err(_) => failures.push(format!("{name}: fmt refused a clean file")),
        }
    }
    eprintln!("{checked} clean inputs checked in every target skin");
    assert!(checked > 150, "too few clean inputs found: {checked}");
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}
