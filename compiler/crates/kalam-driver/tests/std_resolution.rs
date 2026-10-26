//! FOUNDER-QUEUE Q34: `kalam check FILE` resolves `std/…` against the stdlib signature stubs
//! exactly as `kalam check DIR` does with the stdlib directory: a missing stdlib module, or
//! a member a stub module does not have, is K0401 in both (the M2 iteration-4 verifier's
//! p04–p06). `kalam run --interp` checks the file the same way first, so it reports that
//! finding instead of a reason that blamed the file for declaring a stub's name.

use std::path::{Path, PathBuf};

use kalam_driver::{
    check_project, check_source, run_interp, MemoryVfs, ProjectOptions, RunOutcome,
};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn line_of(src: &str, at: u32) -> usize {
    src[..(at as usize).min(src.len())].matches('\n').count() + 1
}

/// `(code, line)` of every finding of `kalam check main.kl`.
fn single(src: &str) -> Vec<(&'static str, usize)> {
    check_source("main.kl", src)
        .unwrap()
        .diagnostics
        .iter()
        .map(|d| (d.code, line_of(src, d.span.start)))
        .collect()
}

/// `(code, line)` of every finding in `src/main.kl` of a package checked with the
/// repository's stdlib directory, as `kalam check DIR` does.
fn project(src: &str) -> Vec<(&'static str, usize)> {
    let mut vfs = MemoryVfs::default();
    vfs.files.insert("app/src/main.kl".into(), src.into());
    let stubs = repo().join("stdlib/src");
    for e in std::fs::read_dir(&stubs).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".kl") {
            let text = std::fs::read_to_string(e.path()).unwrap();
            vfs.files.insert(format!("std/src/{name}"), text);
        }
    }
    let options = ProjectOptions {
        package: "app".into(),
        std_dir: Some("std".into()),
    };
    let files = check_project(&vfs, "app", &options);
    let stray: Vec<_> = files
        .iter()
        .filter(|f| f.path != "app/src/main.kl" && f.has_errors())
        .map(|f| f.path.clone())
        .collect();
    assert!(stray.is_empty(), "errors in the stdlib: {stray:?}");
    files
        .iter()
        .filter(|f| f.path == "app/src/main.kl")
        .flat_map(|f| f.diagnostics.iter())
        .map(|d| (d.code, line_of(src, d.span.start)))
        .collect()
}

/// Both modes agree, and say `want`.
fn both(src: &str, want: &[(&str, usize)]) {
    let (s, p) = (single(src), project(src));
    assert_eq!(s, p, "single-file and project modes disagree on\n{src}");
    assert_eq!(s, want, "{src}");
}

/// FOUNDER-QUEUE Q45: `pow` of `Int`, `Float`, `BigInt` and `Decimal` is evaluated in a
/// module constant in project mode too, where the stdlib's `@builtin` stubs keep their
/// `panic("builtin")` bodies (the M2 iteration-6 verifier's a14/a15).
#[test]
fn pow_in_a_constant_is_evaluated_in_both_modes() {
    both(
        "let D: Decimal = 1.5\nlet B: BigInt = 3\nlet DP = D.pow(3)\nlet BP = B.pow(100)\nlet FP = 2.0.pow(0.5)\nlet IP = 2.pow(62)\n\nfn main():\n    print(\"{DP} {BP} {FP} {IP}\")\n",
        &[],
    );
    both(
        "let A = 2.pow(63)\n\nfn main():\n    print(\"{A}\")\n",
        &[("K1204", 1)],
    );
    both(
        "let D: Decimal = 10\nlet X = D.pow(7000)\n\nfn main():\n    print(\"{X}\")\n",
        &[("K1204", 2)],
    );
}

#[test]
fn a_missing_std_member_or_module_is_k0401_in_both_modes() {
    // p04: `std/http` has no `nope`.
    both(
        "use std/http\n\nfn main():\n    let r = http.nope()\n    print(\"x\")\n",
        &[("K0401", 4)],
    );
    // p05 (FOUNDER-QUEUE Q46): `std/task` re-exports the prelude's `Cell`.
    both(
        "use std/task.{Cell}\n\nfn main():\n    let c = Cell.new(0)\n    print(\"x\")\n",
        &[],
    );
    // `std/task` has nothing else yet.
    both(
        "use std/task.{nope}\n\nfn main():\n    print(\"x\")\n",
        &[("K0401", 1)],
    );
    // p06: no such module.
    both(
        "use std/nonexistent\n\nfn main():\n    print(\"x\")\n",
        &[("K0401", 1)],
    );
    // An item a stub module does not export.
    both(
        "use std/http.{Nope}\n\nfn main():\n    print(\"x\")\n",
        &[("K0401", 1)],
    );
    // With another name error in the file, the stdlib's K0401 is still reported, in both
    // modes (the M2 iteration-6 verifier's p08: single-file mode dropped it).
    both(
        "use std/nonexistent

fn main():
    print(zzz)
",
        &[("K0401", 1), ("K0401", 4)],
    );
    both(
        "use std.{nope}

fn main():
    print(zzz)
",
        &[("K0401", 1), ("K0401", 4)],
    );
    both(
        "use std/http

fn main():
    let r = http.nope()
    print(zzz)
",
        &[("K0401", 4), ("K0401", 5)],
    );
    // What the stubs have still resolves and type-checks.
    both(
        "use std/http\nuse std/math.{sqrt}\n\nfn main():\n    let r = http.router()\n    print(\"{sqrt(2.0)}\")\n",
        &[],
    );
}

#[test]
fn a_missing_std_member_is_a_type_error_found_by_check_not_by_the_run() {
    // Before Q34, single-file mode skipped the type check here and `kalam run --interp`
    // refused the file because it "declares a name the stdlib stubs also declare".
    let src = "use std/http\n\nfn main():\n    let r = http.nope()\n    print(\"x\")\n";
    let mut out = Vec::new();
    match run_interp("main.kl", src.as_bytes(), &mut out).unwrap() {
        RunOutcome::Rejected(checked) => {
            let codes: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
            assert_eq!(codes, ["K0401"]);
        }
        other => panic!(
            "expected the K0401 of `kalam check`, got exit {}: {}",
            other.exit_code(),
            other.stderr_text("main.kl")
        ),
    }
    assert!(out.is_empty());
    // A missing module no longer runs as if it were opaque.
    let src = "use std/nonexistent\n\nfn main():\n    print(\"x\")\n";
    let mut out = Vec::new();
    let outcome = run_interp("main.kl", src.as_bytes(), &mut out).unwrap();
    assert!(
        matches!(outcome, RunOutcome::Rejected(_)),
        "exit {}",
        outcome.exit_code()
    );
    assert!(out.is_empty());
}

#[test]
fn imports_of_the_files_own_package_stay_opaque_in_single_file_mode() {
    // Only the stdlib is loaded with the file: its package's other modules are not, so a
    // `use` of one stays unchecked (no K0401).
    let src = "use app/db.{Store}\n\nfn main():\n    print(\"x\")\n";
    assert_eq!(single(src), vec![]);
}

/// The M2 iteration-6 verifier's fuzz crash: a call that leaves a stdlib default argument
/// out (`json.decode(text)` takes `limits: DecodeLimits = DecodeLimits()`) cannot run in
/// M2, and the reason pointed into the stdlib stub's text. Located in the user's file, that
/// offset panicked inside an Arabic letter, or named a wrong line. It is the call's line.
#[test]
fn a_stdlib_default_argument_that_cannot_run_is_reported_at_the_call() {
    let call = "    let r: Result[P, DecodeError] = json.decode(\"x\")\n";
    for pad in [0usize, 1, 600, 1199, 1200, 1201] {
        // Arabic letters (two bytes each) before the call put the stub's offsets inside
        // characters of this file.
        let filler = format!("# {}\n", "ا".repeat(pad));
        let src = format!(
            "use std/json.{{DecodeError}}\n\n@derive(Decode)\ntype P:\n    x: Int\n\n{filler}fn main():\n{call}    print(\"x\")\n"
        );
        let line = src[..src.find(call).unwrap()].matches('\n').count() + 1;
        let mut out = Vec::new();
        match run_interp("main.kl", src.as_bytes(), &mut out).unwrap() {
            RunOutcome::Unsupported(why) => {
                assert!(why.starts_with(&format!("line {line}:")), "{why}\n{src}");
            }
            other => panic!(
                "expected `not supported`, got exit {}: {}",
                other.exit_code(),
                other.stderr_text("main.kl")
            ),
        }
    }
}
