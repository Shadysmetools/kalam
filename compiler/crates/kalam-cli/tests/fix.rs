//! `kalam fix [--dry-run] FILE|DIR...`: machine fixes applied to a fixpoint (spec §21.3).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn files(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kalam-cli-fix-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (rel, text) in files {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    dir
}

fn kalam(args: &[&str], path: &Path) -> (Option<i32>, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_kalam"))
        .arg("fix")
        .args(args)
        .arg(path)
        .env("KALAM_LANG", "en")
        .output()
        .unwrap();
    (
        o.status.code(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

const SEMI: &str = "fn main():\n    let a = 1;\n    let b = a === 1;\n    print(\"{b}\")\n";
const SEMI_FIXED: &str = "fn main():\n    let a = 1\n    let b = a == 1\n    print(\"{b}\")\n";

#[test]
fn fixes_a_file_in_place() {
    let dir = files("file", &[("main.kl", SEMI)]);
    let main = dir.join("main.kl");
    let (code, out, err) = kalam(&[], &main);
    let text = fs::read_to_string(&main).unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, Some(0), "{out}\n{err}");
    assert_eq!(text, SEMI_FIXED);
    assert!(
        out.contains("3 fixes (K0904 line 2, K0901 line 3, K0904 line 3)"),
        "{out}"
    );
    assert!(err.is_empty(), "{err}");
}

#[test]
fn dry_run_prints_a_diff_and_writes_nothing() {
    let dir = files("dry", &[("main.kl", SEMI)]);
    let main = dir.join("main.kl");
    let (code, out, err) = kalam(&["--dry-run"], &main);
    let text = fs::read_to_string(&main).unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, Some(0), "{out}\n{err}");
    assert_eq!(text, SEMI);
    assert!(
        out.contains("@@ -1,4 +1,4 @@\n fn main():\n-    let a = 1;\n-    let b = a === 1;\n+    let a = 1\n+    let b = a == 1\n"),
        "{out}"
    );
}

#[test]
fn a_directory_is_fixed_file_by_file_and_what_is_left_is_reported() {
    // `b.ar.kl` has only a suggestion-only problem (K1301 is not machine-applicable): it
    // stays as it is, is reported, and the exit status says errors are left.
    let k1301 = "enum ParseFail:\n    Empty\n\nenum LoadFail:\n    Missing\n\nfn parse(text: String) -> Result[Int, ParseFail]:\n    return Err(.Empty)\n\nfn load(text: String) -> Result[Int, LoadFail]:\n    let n = parse(text)?\n    return Ok(n)\n\nfn main():\n    let _ = load(\"1\")\n";
    let dir = files(
        "dir",
        &[
            ("src/a.kl", SEMI),
            ("src/nested/b.kl", k1301),
            ("target/skip.kl", SEMI),
            ("notes.txt", "not Kalam"),
        ],
    );
    let (code, out, err) = kalam(&[], &dir);
    let a = fs::read_to_string(dir.join("src/a.kl")).unwrap();
    let b = fs::read_to_string(dir.join("src/nested/b.kl")).unwrap();
    let skipped = fs::read_to_string(dir.join("target/skip.kl")).unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, Some(1), "{out}\n{err}");
    assert_eq!(a, SEMI_FIXED);
    assert_eq!(b, k1301);
    assert_eq!(skipped, SEMI, "target/ is not walked");
    assert!(err.contains("K1301"), "{err}");
}

#[test]
fn usage_errors() {
    let dir = files("usage", &[("notes.txt", "x")]);
    let (code, _, err) = kalam(&[], &dir.join("notes.txt"));
    assert_eq!(code, Some(2), "{err}");
    let (code, _, err) = kalam(&[], &dir);
    assert_eq!(code, Some(2), "{err}");
    assert!(err.contains("holds no Kalam file"), "{err}");
    let (code, _, err) = kalam(&["--write"], &dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, Some(2), "{err}");
}
