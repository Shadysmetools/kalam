//! M1 metric D5.diag_completeness (eval/scorecard.json): every diagnostic code the compiler
//! can emit has a catalog entry with a title and a message in all three languages.
//!
//! kalam-diagnostics cannot depend on the crates that emit codes (they depend on it), so this
//! test scans their source: every quoted `SEC`/`K`/`W`/`H`/`L` + four-digit literal in
//! `crates/*/src/**/*.rs`, ignoring everything from the first `#[cfg(test)]` in a file
//! (test fixtures and mocks mention codes nobody emits).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use kalam_diagnostics::catalog;

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Quoted literals of the form `"PREFIX0000"` where PREFIX is one of the code families.
fn codes_in(text: &str, out: &mut BTreeSet<String>) {
    for (i, _) in text.match_indices('"') {
        let rest = &text[i + 1..];
        let Some(end) = rest.find('"') else { continue };
        let lit = &rest[..end];
        let prefix_len = lit.len().saturating_sub(4);
        if !lit.is_char_boundary(prefix_len) {
            continue; // a non-ASCII literal (`"… §27."`) is never a code
        }
        let (prefix, digits) = lit.split_at(prefix_len);
        if matches!(prefix, "SEC" | "K" | "W" | "H" | "L")
            && digits.bytes().all(|b| b.is_ascii_digit())
        {
            out.insert(lit.to_string());
        }
    }
}

fn emitted_codes() -> BTreeSet<String> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in fs::read_dir(&crates).expect("crates dir").flatten() {
        if entry.file_name() == "kalam-diagnostics" {
            continue; // the catalog itself
        }
        if entry.file_name() == "kalam-eval" {
            continue; // the eval harness names codes (retired ones too) but emits none
        }
        rust_files(&entry.path().join("src"), &mut files);
    }
    let mut codes = BTreeSet::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_default();
        let live = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => &text[..],
        };
        codes_in(live, &mut codes);
    }
    codes
}

#[test]
fn scanner_finds_codes_in_quotes_only() {
    let mut out = BTreeSet::new();
    codes_in(r#"x("K0250"); "SEC0111" "K12" "Kx123" "note""#, &mut out);
    assert_eq!(
        out.into_iter().collect::<Vec<_>>(),
        vec!["K0250", "SEC0111"]
    );
}

#[test]
fn every_emitted_code_has_a_catalog_entry_in_three_languages() {
    let codes = emitted_codes();
    // Sanity: the scan really sees the lexer and parser.
    for known in ["K0129", "K0250", "K0921"] {
        assert!(codes.contains(known), "scan missed {known}: {codes:?}");
    }
    let mut missing = Vec::new();
    for code in &codes {
        match catalog::lookup(code) {
            None => missing.push(format!("{code}: no entry")),
            Some(e) => {
                for (k, text) in e.title.iter().chain(e.message.iter()).enumerate() {
                    if text.trim().is_empty() {
                        missing.push(format!("{code}: empty text #{k}"));
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "catalog gaps: {missing:#?}");
}

#[test]
fn messages_have_no_unfilled_placeholders_for_known_args() {
    // Parser codes: K0250 gets `expected`/`found`, the rest get no args, so a stray `{x}`
    // would be printed raw.
    let no_args = [
        "K0251", "K0252", "K0253", "K0254", "K0255", "K0256", "K0259", "K0710", "K0720", "K0731",
        "K0921",
    ];
    for code in no_args {
        let e = catalog::lookup(code).unwrap_or_else(|| panic!("{code} missing"));
        for msg in e.message {
            let rendered = catalog::render_template(msg, &[]);
            assert!(
                !rendered.contains("{expected}") && !rendered.contains("{found}"),
                "{code}: {msg}"
            );
        }
    }
    let e = catalog::lookup("K0250").expect("K0250");
    let args = vec![
        ("expected", "`:`".to_string()),
        ("found", "end of line".to_string()),
    ];
    for msg in e.message {
        let rendered = catalog::render_template(msg, &args);
        assert!(
            rendered.contains("`:`") && rendered.contains("end of line"),
            "{rendered}"
        );
        assert!(!rendered.contains('{'), "{rendered}");
    }
}

#[test]
fn messages_keep_a_positive_tone() {
    // Spec §21.4 rule 3.
    for e in catalog::CATALOG {
        let en = e.message[0].to_lowercase();
        for word in ["illegal", "invalid", "fatal", " bad "] {
            assert!(!en.contains(word), "{}: {}", e.code, e.message[0]);
        }
    }
}
