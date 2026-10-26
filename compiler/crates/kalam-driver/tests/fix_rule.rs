//! `kalam fix` acceptance rule (FOUNDER-QUEUE Q42, Q49, Q50): a machine fix, or a chain of
//! them, is applied only when the file it gives has no error code, at any stage, that the
//! file did not have, and fewer errors (or, for fixes of warnings only, no more errors and
//! fewer diagnostics). Otherwise the fix is withheld and reported as a suggestion. The loop
//! runs to a fixpoint, so a second run changes nothing.
//!
//! The corpus is every `tests/fixp` case (the M2 iteration 4 verifier's repros) and every
//! diagnostic golden in `eval/goldens`. The check below recomputes "fewer errors, no new
//! code" from `check_bytes`, independently of the rule inside `fix_file`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kalam_diagnostics::Applicability;
use kalam_driver::{check_bytes, fix_file, FixedFile};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The error count, the error codes and the number of diagnostics of `text`.
fn error_codes(path: &str, text: &str) -> (usize, BTreeSet<&'static str>, usize) {
    let checked = check_bytes(path, text.as_bytes()).unwrap();
    let errors: Vec<_> = checked
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .collect();
    (
        errors.len(),
        errors.iter().map(|d| d.code).collect(),
        checked.diagnostics.len(),
    )
}

/// Fixes `text` (checked as `path`) and asserts the Q42 rule and the fixpoint.
fn check_rule(label: &str, path: &str, text: &str) -> FixedFile {
    let f = fix_file(path, text.as_bytes()).unwrap();
    let (before_n, before, before_all) = error_codes(path, text);
    let (after_n, after, after_all) = error_codes(path, &f.fixed);
    if f.changed() {
        assert!(
            after.is_subset(&before),
            "{label}: new error codes {:?} (before {before:?})\n{}",
            after.difference(&before).collect::<Vec<_>>(),
            f.fixed
        );
        assert!(
            after_n < before_n || (after_n == before_n && after_all < before_all),
            "{label}: {before_n} errors before, {after_n} after\n{}",
            f.fixed
        );
        assert!(!f.applied.is_empty(), "{label}");
    } else {
        assert!(f.applied.is_empty(), "{label}: {:?}", f.applied);
    }
    // Every withheld fix is shown as a suggestion in what is left.
    for w in &f.withheld {
        assert!(
            f.checked.diagnostics.iter().any(|d| d.code == w.fix.code
                && d.fixes.iter().any(
                    |x| x.title == w.fix.title && x.applicability == Applicability::Suggestion
                )),
            "{label}: withheld {w:?} is not shown as a suggestion"
        );
    }
    // Fixpoint: a second run is a no-op.
    let again = fix_file(path, f.fixed.as_bytes()).unwrap();
    assert!(
        !again.changed() && again.applied.is_empty(),
        "{label}: second run applied {:?}\n{}",
        again.applied,
        again.fixed
    );
    f
}

fn spec_name(file: &str) -> &'static str {
    if file.ends_with(".ar.kl") {
        "main.ar.kl"
    } else if file.ends_with(".franco.kl") {
        "main.franco.kl"
    } else {
        "main.kl"
    }
}

fn fixp(name: &str) -> FixedFile {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixp")
        .join(name);
    let text = std::fs::read_to_string(&p).unwrap();
    check_rule(name, spec_name(name), &text)
}

#[test]
fn every_fixp_case_follows_the_rule() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixp");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        fixp(&name);
        n += 1;
    }
    assert_eq!(n, 15);
}

#[test]
fn fixes_that_only_reveal_nothing_new_are_applied_and_leave_a_clean_file() {
    for name in [
        "f01_shadow_rename_collision.kl",
        "f02_var_capture_snapshot_collision.kl",
        "f03_field_call_chain.kl",
        "f04_missing_arms.kl",
        "f05_power_precedence.kl",
        "f06_len_builtin_expr.kl",
        "f08_ternary.kl",
        "f09_not_symbol.kl",
        "f10_label_positional.kl",
        "f11_shadow_ar.ar.kl",
        "f13_missing_arms.ar.kl",
        "f14_field_call.franco.kl",
    ] {
        let f = fixp(name);
        assert!(f.changed(), "{name}");
        assert!(f.withheld.is_empty(), "{name}: {:?}", f.withheld);
        assert!(
            !f.checked.has_errors(),
            "{name}: {:?}",
            f.checked.diagnostics
        );
    }
}

#[test]
fn a_fix_that_would_reveal_a_new_error_code_is_withheld() {
    // `s ** 2` → `s.pow(2)` gives K0420 (`String` has no `pow`): withheld. (`Decimal` has
    // `pow` since Q45, so `d ** 2` is now fixed.)
    let src = "fn main():\n    let s = \"ab\"\n    let r = s ** 2\n    print(\"{r}\")\n";
    let f = check_rule("string power", "main.kl", src);
    assert!(!f.changed() && f.applied.is_empty());
    assert_eq!(f.withheld.len(), 1, "{:?}", f.withheld);
    assert_eq!(f.withheld[0].fix.code, "K0923");
    assert_eq!(f.withheld[0].new_codes, ["K0420"]);
    let codes: Vec<&str> = f.checked.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["K0923"]);
}

#[test]
fn a_float_power_and_print_with_several_arguments_are_fixed_and_check_clean() {
    // f12: `Float` has `pow` (spec §5.9, §21.3), so `(2.0).pow(0.5)` checks clean.
    let f = fixp("f12_float_power.kl");
    assert!(f.fixed.contains("let r = (2.0).pow(0.5)"), "{}", f.fixed);
    assert!(f.withheld.is_empty(), "{:?}", f.withheld);
    assert!(!f.checked.has_errors(), "{:?}", f.checked.diagnostics);
    // f07: K0916's fix (§21.3) joins the values in one string.
    let f = fixp("f07_print_several.kl");
    assert!(f.fixed.contains("print(\"{a} {b}\")"), "{}", f.fixed);
    assert!(f.withheld.is_empty(), "{:?}", f.withheld);
    assert!(!f.checked.has_errors(), "{:?}", f.checked.diagnostics);
}

#[test]
fn of_two_fixes_the_one_that_adds_nothing_is_applied_and_the_other_withheld() {
    // `xs.len() ** 2` is fixed; `xs.first().unwrap_or(0) ** 3` would reveal K0420.
    let f = fixp("f15_power_method_receiver.kl");
    assert!(
        f.fixed
            .contains("xs.len().pow(2) + xs.first().unwrap_or(0) ** 3"),
        "{}",
        f.fixed
    );
    assert_eq!(f.applied.len(), 1);
    assert_eq!(f.withheld.len(), 1, "{:?}", f.withheld);
    assert_eq!(f.withheld[0].new_codes, ["K0420"]);
}

#[test]
fn a_diagnostic_without_a_machine_fix_leaves_the_file_unchanged() {
    // K0916 with a labelled argument (`end:` has no place in the joined string) carries no
    // machine fix.
    let src = "fn main():\n    print(1, end: \"\")\n";
    let f = check_rule("print with a label", "main.kl", src);
    assert!(!f.changed() && f.withheld.is_empty());
}

#[test]
fn every_golden_follows_the_rule() {
    let goldens = repo().join("eval/goldens");
    let mut files = 0;
    let mut changed = 0;
    for e in std::fs::read_dir(&goldens).unwrap().flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let code = e.file_name().to_string_lossy().into_owned();
        for (skin, name) in [
            ("en", "main.kl"),
            ("ar", "main.ar.kl"),
            ("franco", "main.franco.kl"),
        ] {
            let p = e.path().join(format!("{skin}.kl"));
            let Ok(bytes) = std::fs::read(&p) else {
                continue;
            };
            // A golden with invalid UTF-8 is never changed (checked in fix.rs).
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            let f = check_rule(&format!("{code}/{skin}"), name, &text);
            files += 1;
            changed += usize::from(f.changed());
        }
    }
    assert!(files >= 300, "{files} golden sources");
    assert!(changed >= 30, "only {changed} goldens changed");
}
