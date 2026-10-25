//! The M2 type-checking acceptance on the dev corpus, through `check_project` with the
//! stdlib signature stubs (ROADMAP.md M2: "All MVP-tier corpus tasks 1–15 type-check in all
//! three skins against the stdlib signature stubs"; "The [NEG] tasks 48–50 fail with the
//! exact expected codes"). Integration test: it reads `eval/corpus` and `stdlib/src`.

use std::path::{Path, PathBuf};

use kalam_driver::{check_project, MemoryVfs, ProjectOptions};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn stdlib_vfs() -> MemoryVfs {
    let mut vfs = MemoryVfs::default();
    for e in std::fs::read_dir(repo().join("stdlib/src"))
        .unwrap()
        .flatten()
    {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "kl") {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            vfs.files
                .insert(format!("std/{name}"), std::fs::read_to_string(&p).unwrap());
        }
    }
    vfs
}

/// The spec file name for a corpus skin file (`ar.kalam` → `main.ar.kl`).
fn staged_name(corpus_file: &str) -> &'static str {
    match corpus_file {
        "en.kalam" => "main.kl",
        "ar.kalam" => "main.ar.kl",
        "franco.kalam" => "main.franco.kl",
        other => panic!("unknown skin file {other}"),
    }
}

/// `(file, code, line)` of every error `check_project` reports for one staged file.
fn check_case(dir: &Path, skin_file: &str) -> Vec<(String, &'static str, usize)> {
    let text = std::fs::read_to_string(dir.join(skin_file)).unwrap();
    let mut vfs = stdlib_vfs();
    let staged = format!("proj/src/{}", staged_name(skin_file));
    vfs.files.insert(staged, text);
    let opts = ProjectOptions {
        package: "app".into(),
        std_dir: Some("std".into()),
    };
    let mut out = Vec::new();
    for f in check_project(&vfs, "proj", &opts) {
        for d in f.diagnostics.iter().filter(|d| d.is_error()) {
            let line = f.lines.line_of(d.span.start) + 1;
            out.push((f.path.clone(), d.code, line));
        }
    }
    out
}

/// Task directories of the dev corpus with a number in `numbers`.
fn tasks(numbers: &[u32]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(repo().join("eval/corpus"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            name.strip_prefix('t')
                .and_then(|r| r.get(..2))
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| numbers.contains(&n))
        })
        .collect();
    out.sort();
    out
}

const SKINS: [&str; 3] = ["en.kalam", "ar.kalam", "franco.kalam"];

#[test]
fn mvp_tasks_type_check_in_every_skin() {
    let numbers: Vec<u32> = (1..=15).chain(31..=33).collect();
    let dirs = tasks(&numbers);
    assert!(dirs.len() >= 12, "dev corpus: {dirs:?}");
    let mut failures = Vec::new();
    let mut checked = 0;
    for dir in &dirs {
        for skin in SKINS {
            let found = check_case(dir, skin);
            checked += 1;
            if !found.is_empty() {
                failures.push(format!("{} {skin}: {found:?}", dir.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{checked} files; failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn positive_twins_of_the_negative_tasks_type_check() {
    let mut failures = Vec::new();
    for dir in tasks(&[45, 48, 49, 50]) {
        for case in [dir.join("good"), dir.clone()] {
            let expected = std::fs::read_to_string(case.join("expected.txt")).unwrap_or_default();
            if !expected.contains("kind: run") || !case.join("en.kalam").exists() {
                continue;
            }
            for skin in SKINS {
                let found = check_case(&case, skin);
                if !found.is_empty() {
                    failures.push(format!("{} {skin}: {found:?}", case.display()));
                }
            }
        }
    }
    assert!(failures.is_empty(), "failures:\n{}", failures.join("\n"));
}

/// The `diagnostic:` lines of a compile-error `expected.txt`: `(code, line)`.
fn expected_codes(case: &Path) -> Vec<(String, usize)> {
    let text = std::fs::read_to_string(case.join("expected.txt")).unwrap();
    let mut out = Vec::new();
    for l in text.lines() {
        let Some(rest) = l.strip_prefix("diagnostic:") else {
            continue;
        };
        let mut code = String::new();
        let mut line = 0;
        for part in rest.split_whitespace() {
            if let Some(c) = part.strip_prefix("code=") {
                code = c.to_string();
            } else if let Some(n) = part.strip_prefix("line=") {
                line = n.split('|').next().unwrap().parse().unwrap();
            }
        }
        out.push((code, line));
    }
    out.sort();
    out
}

#[test]
fn negative_tasks_report_exactly_their_expected_codes() {
    let mut compared = 0;
    let mut failures = Vec::new();
    for dir in tasks(&[48, 49]) {
        for case in [dir.clone(), dir.join("bad")] {
            let Ok(expected_text) = std::fs::read_to_string(case.join("expected.txt")) else {
                continue;
            };
            if !expected_text.contains("kind: compile-error") {
                continue;
            }
            let want = expected_codes(&case);
            for skin in SKINS {
                let mut got: Vec<(String, usize)> = check_case(&case, skin)
                    .into_iter()
                    .map(|(_, c, l)| (c.to_string(), l))
                    .collect();
                got.sort();
                compared += 1;
                if got != want {
                    failures.push(format!(
                        "{} {skin}: got {got:?}, want {want:?}",
                        case.display()
                    ));
                }
            }
        }
    }
    assert!(compared >= 6, "compared {compared}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_k1601_in_the_negative_tasks_carries_a_fix() {
    for dir in tasks(&[48, 49]) {
        for case in [dir.clone(), dir.join("bad")] {
            let Ok(expected_text) = std::fs::read_to_string(case.join("expected.txt")) else {
                continue;
            };
            if !expected_text.contains("fix: required") {
                continue;
            }
            let text = std::fs::read_to_string(case.join("en.kalam")).unwrap();
            let mut vfs = stdlib_vfs();
            vfs.files.insert("proj/src/main.kl".into(), text);
            let opts = ProjectOptions {
                package: "app".into(),
                std_dir: Some("std".into()),
            };
            for f in check_project(&vfs, "proj", &opts) {
                for d in &f.diagnostics {
                    assert!(!d.fixes.is_empty(), "{} {}: no fix", case.display(), d.code);
                }
            }
        }
    }
}
