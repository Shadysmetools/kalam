//! Resolution never panics, whatever the parser recovered: every eval suite file and every
//! dev corpus file is resolved, including files with syntax errors (whose ASTs hold
//! `Error` nodes), which the driver itself never resolves.

use std::fs;
use std::path::{Path, PathBuf};

use kalam_base::{FileId, Skin};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
fn every_suite_and_corpus_file_resolves_without_panicking() {
    let eval = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../eval");
    let mut all = Vec::new();
    for sub in ["suites", "corpus", "conformance"] {
        files(&eval.join(sub), &mut all);
    }
    let mut seen = 0;
    for path in all {
        let name = path.to_string_lossy().replace('\\', "/");
        let skin = if name.ends_with(".kalam") {
            match name.rsplit('/').next() {
                Some("ar.kalam") => Skin::Ar,
                Some("franco.kalam") => Skin::Franco,
                _ => Skin::En,
            }
        } else {
            match Skin::from_path(&name) {
                Some(s) => s,
                None => continue,
            }
        };
        let Ok(bytes) = fs::read(&path) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let parsed = kalam_parser::parse_source(&text, skin, FileId(0));
        let _ = kalam_resolve::resolve_file(FileId(0), skin, &parsed.ast);
        seen += 1;
    }
    assert!(seen > 300, "only {seen} files found");
}
