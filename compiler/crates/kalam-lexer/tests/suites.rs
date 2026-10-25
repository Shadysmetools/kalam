//! K0201 against the shared eval suites (`eval/suites/<suite>/<case>/`, eval P8 and
//! `D5.error_recovery_precision`): the lexer reports K0201 exactly where a case expects it
//! (at the expected line), and nowhere else, in every suite. The corpus (valid programs in
//! all three skins) never gets K0201.
//!
//! Cases whose notes start with `[M2]` need name resolution and are skipped.

use std::path::{Path, PathBuf};

use kalam_base::{FileId, Skin};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root")
}

/// 1-based line of a byte offset (LF only, like the suite format).
fn line_of(text: &str, offset: u32) -> usize {
    text.as_bytes()[..(offset as usize).min(text.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

struct Case {
    id: String,
    expects_k0201_at: Option<Option<usize>>,
    files: Vec<(String, Skin, Vec<u8>)>,
}

fn cases(suite: &str) -> Vec<Case> {
    let root = repo_root().join("eval").join("suites").join(suite);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(expect) = std::fs::read_to_string(dir.join("expect.json")) else {
            continue;
        };
        let v: serde_json::Value = serde_json::from_str(&expect).expect("expect.json");
        if v["notes"].as_str().unwrap_or("").starts_with("[M2]") {
            continue;
        }
        let codes: Vec<&str> = v["codes"]
            .as_array()
            .map(|a| a.iter().filter_map(|c| c.as_str()).collect())
            .unwrap_or_default();
        let expects_k0201_at = codes.iter().position(|&c| c == "K0201").map(|k| {
            v["lines"]
                .as_array()
                .and_then(|l| l.get(k))
                .and_then(|n| n.as_u64())
                .map(|n| n as usize)
        });
        let mut files = Vec::new();
        for f in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            if let Some(skin) = Skin::from_path(&name) {
                files.push((name, skin, std::fs::read(f.path()).unwrap()));
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        out.push(Case {
            id: format!("{suite}/{}", dir.file_name().unwrap().to_string_lossy()),
            expects_k0201_at,
            files,
        });
    }
    out
}

#[test]
fn k0201_is_reported_exactly_where_the_suites_expect_it() {
    let mut checked = 0;
    let mut expected_hits = 0;
    let mut failures = Vec::new();
    for suite in [
        "mixed-skin",
        "seeded-errors",
        "lexing-hazards",
        "trojan-source",
        "conformance",
    ] {
        for case in cases(suite) {
            for (name, skin, bytes) in &case.files {
                // Invalid UTF-8 is a SEC0100 case: nothing for K0201 to say.
                let Ok(text) = std::str::from_utf8(bytes) else {
                    continue;
                };
                let out = kalam_lexer::lex(text, *skin, FileId(0));
                let first = out.diagnostics.iter().find(|d| d.code == "K0201");
                checked += 1;
                match (case.expects_k0201_at, first) {
                    (Some(_), None) => failures.push(format!("{}/{name}: no K0201", case.id)),
                    (Some(Some(line)), Some(d)) if line_of(text, d.span.start) != line => failures
                        .push(format!(
                            "{}/{name}: K0201 at line {}, expected {line}",
                            case.id,
                            line_of(text, d.span.start)
                        )),
                    (Some(_), Some(_)) => expected_hits += 1,
                    (None, Some(d)) => failures.push(format!(
                        "{}/{name}: unexpected K0201 at line {} ({:?})",
                        case.id,
                        line_of(text, d.span.start),
                        d.args
                    )),
                    (None, None) => {}
                }
            }
        }
    }
    eprintln!("K0201: {checked} suite files checked, {expected_hits} expected reports found");
    if checked == 0 {
        return; // the suites are not checked out
    }
    assert!(
        expected_hits >= 27,
        "only {expected_hits} expected K0201 reports"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_corpus_never_gets_k0201() {
    let root = repo_root().join("eval").join("corpus");
    let Ok(tasks) = std::fs::read_dir(&root) else {
        return;
    };
    let mut dirs = Vec::new();
    for t in tasks.flatten() {
        if t.path().is_dir() {
            dirs.push(t.path());
            for s in std::fs::read_dir(t.path()).unwrap().flatten() {
                if s.path().is_dir() {
                    dirs.push(s.path());
                }
            }
        }
    }
    let mut checked = 0;
    for d in dirs {
        for skin in Skin::ALL {
            let Ok(text) = std::fs::read_to_string(d.join(format!("{}.kalam", skin.id()))) else {
                continue;
            };
            let out = kalam_lexer::lex(&text, skin, FileId(0));
            let k: Vec<_> = out
                .diagnostics
                .iter()
                .filter(|d| d.code == "K0201")
                .map(|d| (line_of(&text, d.span.start), d.args.clone()))
                .collect();
            assert!(k.is_empty(), "{}/{skin:?}: {k:?}", d.display());
            checked += 1;
        }
    }
    assert!(checked >= 42, "only {checked} corpus files");
}
