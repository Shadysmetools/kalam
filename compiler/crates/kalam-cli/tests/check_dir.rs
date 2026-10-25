//! `kalam check DIR`: directory (project) mode over `DIR/src` plus the stdlib (`--std`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kalam-cli-dir-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (rel, text) in files {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    dir
}

fn kalam(args: &[&str], dir: &Path) -> (Option<i32>, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_kalam"))
        .args(args)
        .arg(dir)
        .env("KALAM_LANG", "en")
        .output()
        .unwrap();
    (
        o.status.code(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

fn std_dir() -> String {
    format!("{}/../../../stdlib", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn two_skin_files_of_one_module_are_k0301() {
    let dir = project(
        "k0301",
        &[
            ("src/users.kl", "fn main():\n    print(\"x\")\n"),
            ("src/users.ar.kl", "دالة رئيسي():\n    اطبع(\"x\")\n"),
        ],
    );
    let (code, out, err) = kalam(&["check", "--json"], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, Some(1), "{out}\n{err}");
    assert!(out.trim_start().starts_with('['), "{out}");
    assert!(out.contains("\"K0301\""), "{out}");
    // the diagnostic names the file it belongs to
    assert!(
        out.contains("users.ar.kl") || out.contains("users.kl"),
        "{out}"
    );
}

#[test]
fn a_clean_project_resolves_against_the_stdlib() {
    let dir = project(
        "clean",
        &[
            (
                "src/main.kl",
                "use app/shapes.{area}\nuse std/math\n\nfn main():\n    print(\"{area(2)}\")\n",
            ),
            ("src/shapes.kl", "export fn area(r: Int) -> Int => r * r\n"),
        ],
    );
    let std = std_dir();
    let (code, out, err) = kalam(&["check", "--json", "--std", &std], &dir);
    let (code_default, out_default, _) = kalam(&["check", "--json"], &dir);
    // a module the stdlib does not have is K0401 once the stdlib is loaded
    fs::write(
        dir.join("src/main.kl"),
        "use std/definitely_not_a_module\n\nfn main():\n    print(\"x\")\n",
    )
    .unwrap();
    let (code_missing, out_missing, _) = kalam(&["check", "--json", "--std", &std], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, Some(0), "{out}\n{err}");
    assert_eq!(out.trim(), "[]");
    // the default --std is the repository stdlib
    assert_eq!((code_default, out_default.trim()), (Some(0), "[]"));
    assert_eq!(code_missing, Some(1), "{out_missing}");
    assert!(out_missing.contains("\"K0401\""), "{out_missing}");
}

#[test]
fn the_package_name_comes_from_kalam_toml() {
    let dir = project(
        "manifest",
        &[
            (
                "kalam.toml",
                "[package]\nname = \"@team/notes\"\nversion = \"0.1.0\"\n",
            ),
            ("src/db.kl", "export fn open() -> Int => 1\n"),
            (
                "src/main.kl",
                "use notes/db\n\nfn main():\n    print(\"{db.open()}\")\n",
            ),
        ],
    );
    let (code, out, err) = kalam(&["check", "--json"], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!((code, out.trim()), (Some(0), "[]"), "{err}");
}

#[test]
fn an_empty_project_or_a_missing_stdlib_is_a_usage_error() {
    let dir = project("empty", &[("README.md", "no sources\n")]);
    let (code, _, err) = kalam(&["check", "--json"], &dir);
    assert_eq!(code, Some(2), "{err}");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/main.kl"), "fn main():\n    print(\"x\")\n").unwrap();
    let (code, _, err) = kalam(&["check", "--std", "definitely/not/a/dir"], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, Some(2), "{err}");
}

#[test]
fn human_output_reports_errors_per_file() {
    let dir = project("human", &[("src/main.kl", "use std/nope\n")]);
    let (code, out, err) = kalam(&["check"], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, Some(1));
    assert!(out.is_empty(), "{out}");
    assert!(err.contains("K0401"), "{err}");
}
