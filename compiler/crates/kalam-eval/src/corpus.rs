//! Corpus discovery (eval/corpus/INDEX.md §2) and the parts of `expected.txt`
//! (`kalam-expect/1`, INDEX.md §3) that the harness can check today.
//!
//! The corpus stores each case as `en.kalam` / `ar.kalam` / `franco.kalam`. Spec DR-18 and
//! DECISIONS D24 make the extension the only skin declaration, so the harness stages each
//! file under its spec name (`main.kl`, `main.ar.kl`, `main.franco.kl`) and derives the
//! skin from that name with [`Skin::from_path`]. Files already named `*.kl` are accepted
//! as they are.
//!
//! Note for the corpus-leak scan (G-INT): this file names no task id or corpus path.

use kalam_base::Skin;
use std::fs;
use std::path::{Path, PathBuf};

/// The staged (spec) file name for a corpus file name, or `None` if it is not a skin file.
pub fn staged_name(file_name: &str) -> Option<String> {
    let staged = match file_name {
        "en.kalam" => "main.kl".to_string(),
        "ar.kalam" => "main.ar.kl".to_string(),
        "franco.kalam" => "main.franco.kl".to_string(),
        other if other.ends_with(".kl") => other.to_string(),
        _ => return None,
    };
    Some(staged)
}

#[derive(Clone, Debug)]
pub struct SkinFile {
    pub skin: Skin,
    pub path: PathBuf,
    pub staged: String,
}

#[derive(Clone, Debug)]
pub struct Case {
    /// Directory name of the task, e.g. `tNN-slug`.
    pub task: String,
    /// `main` for the task root, otherwise the sub-directory path.
    pub case: String,
    pub dir: PathBuf,
    pub files: Vec<SkinFile>,
    pub expect: Expect,
}

impl Case {
    pub fn file(&self, skin: Skin) -> Option<&SkinFile> {
        self.files.iter().find(|f| f.skin == skin)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Expect {
    /// `run`, `compile-error` or `http`.
    pub kind: Option<String>,
    pub diagnostics: Vec<ExpectedDiag>,
    /// `others: none`
    pub others_none: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpectedDiag {
    pub code: String,
    /// 1-based alternatives (`line=5|6`).
    pub lines: Vec<usize>,
    /// `U+XXXX` strings that must be named in the message.
    pub codepoints: Vec<String>,
}

/// Parses the header of an `expected.txt`. Stops at the first `--- ` section marker.
pub fn parse_expected(text: &str) -> Expect {
    let mut out = Expect::default();
    for line in text.lines() {
        if line.starts_with("--- ") {
            break;
        }
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "kind" => out.kind = Some(value.to_string()),
            "others" => out.others_none = value == "none",
            "diagnostic" => {
                let mut diag = ExpectedDiag {
                    code: String::new(),
                    lines: Vec::new(),
                    codepoints: Vec::new(),
                };
                for part in value.split_whitespace() {
                    if let Some(c) = part.strip_prefix("code=") {
                        diag.code = c.to_string();
                    } else if let Some(l) = part.strip_prefix("line=") {
                        diag.lines = l.split('|').filter_map(|n| n.parse().ok()).collect();
                    } else if let Some(c) = part.strip_prefix("codepoints=") {
                        diag.codepoints = c.split(',').map(str::to_string).collect();
                    }
                }
                if !diag.code.is_empty() {
                    out.diagnostics.push(diag);
                }
            }
            _ => {}
        }
    }
    out
}

/// Finds every case under `corpus_dir`: a directory that holds at least one skin file.
/// Sorted by task, then case, so output is deterministic.
pub fn discover(corpus_dir: &Path) -> std::io::Result<Vec<Case>> {
    let mut cases = Vec::new();
    let mut tasks: Vec<PathBuf> = fs::read_dir(corpus_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    tasks.sort();
    for task_dir in tasks {
        let task = task_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        collect(&task, &task_dir, &task_dir, &mut cases)?;
    }
    Ok(cases)
}

fn collect(task: &str, task_dir: &Path, dir: &Path, out: &mut Vec<Case>) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    let mut files = Vec::new();
    let mut subdirs = Vec::new();
    for path in entries {
        if path.is_dir() {
            subdirs.push(path);
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if let Some(staged) = staged_name(&name) {
            if let Some(skin) = Skin::from_path(&staged) {
                files.push(SkinFile { skin, path, staged });
            }
        }
    }
    if !files.is_empty() {
        files.sort_by_key(|f| f.skin);
        let expect = fs::read_to_string(dir.join("expected.txt"))
            .map(|t| parse_expected(&t))
            .unwrap_or_default();
        let case = match dir.strip_prefix(task_dir) {
            Ok(rel) if rel.as_os_str().is_empty() => "main".to_string(),
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => dir.to_string_lossy().into_owned(),
        };
        out.push(Case {
            task: task.to_string(),
            case,
            dir: dir.to_path_buf(),
            files,
            expect,
        });
    }
    for sub in subdirs {
        collect(task, task_dir, &sub, out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_names_follow_the_spec_extensions() {
        assert_eq!(staged_name("en.kalam").as_deref(), Some("main.kl"));
        assert_eq!(staged_name("ar.kalam").as_deref(), Some("main.ar.kl"));
        assert_eq!(
            staged_name("franco.kalam").as_deref(),
            Some("main.franco.kl")
        );
        assert_eq!(
            staged_name("main.franco.kl").as_deref(),
            Some("main.franco.kl")
        );
        assert_eq!(staged_name("ref.py"), None);
        assert_eq!(
            Skin::from_path(&staged_name("franco.kalam").unwrap()),
            Some(Skin::Franco)
        );
        assert_eq!(
            Skin::from_path(&staged_name("ar.kalam").unwrap()),
            Some(Skin::Ar)
        );
    }

    #[test]
    fn parses_expected_header() {
        let text = "# kalam-expect/1\ntask: TX\ncase: main\nkind: compile-error\n\
                    diagnostic: code=SEC0110 line=5 codepoints=U+202E,U+2066\n\
                    diagnostic: code=SEC0104 line=7|8\nothers: none\n--- stdout\ndiagnostic: code=NOPE line=1\n";
        let e = parse_expected(text);
        assert_eq!(e.kind.as_deref(), Some("compile-error"));
        assert!(e.others_none);
        assert_eq!(e.diagnostics.len(), 2);
        assert_eq!(e.diagnostics[0].code, "SEC0110");
        assert_eq!(e.diagnostics[0].lines, vec![5]);
        assert_eq!(e.diagnostics[0].codepoints, vec!["U+202E", "U+2066"]);
        assert_eq!(e.diagnostics[1].lines, vec![7, 8]);
    }

    #[test]
    fn discovers_cases_in_a_temp_corpus() {
        let root = std::env::temp_dir().join(format!("kalam-eval-corpus-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let task = root.join("t99-demo");
        fs::create_dir_all(task.join("bad")).unwrap();
        fs::write(task.join("en.kalam"), "fn main():\n    print(\"hi\")\n").unwrap();
        fs::write(task.join("ar.kalam"), "x\n").unwrap();
        fs::write(
            task.join("expected.txt"),
            "# h\nkind: run\n--- stdout\nhi\n",
        )
        .unwrap();
        fs::write(task.join("bad").join("main.franco.kl"), "x\n").unwrap();
        fs::write(task.join("ref.py"), "print('hi')\n").unwrap();

        let cases = discover(&root).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].task, "t99-demo");
        assert_eq!(cases[0].case, "main");
        assert_eq!(cases[0].files.len(), 2);
        assert_eq!(cases[0].files[0].skin, Skin::En);
        assert_eq!(cases[0].expect.kind.as_deref(), Some("run"));
        assert_eq!(cases[1].case, "bad");
        assert_eq!(cases[1].files[0].skin, Skin::Franco);
    }
}
