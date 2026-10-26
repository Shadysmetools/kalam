//! FOUNDER-QUEUE Q34 (single-file and project modes see the same prelude) and the K0405
//! rename fix-it of spec §10.1 ("K0405, with a rename fix-it").

use kalam_ast::Ast;
use kalam_base::{FileId, Skin};
use kalam_diagnostics::{Applicability, Diagnostic, Fix};
use kalam_resolve::{prelude::PRELUDE, resolve_file};

fn parse(path: &str, src: &str) -> (Skin, Ast) {
    let skin = Skin::from_path(path).expect("a Kalam path");
    let out = kalam_parser::parse_source(src, skin, FileId(0));
    let errors: Vec<_> = out.diagnostics.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "{path} must parse: {errors:?}\n{src}");
    (skin, out.ast)
}

/// `kalam check FILE`'s resolution: the file alone, every import opaque.
fn diags(path: &str, src: &str) -> Vec<Diagnostic> {
    let (skin, ast) = parse(path, src);
    resolve_file(FileId(0), skin, &ast).1
}

fn codes(path: &str, src: &str) -> Vec<&'static str> {
    diags(path, src).iter().map(|d| d.code).collect()
}

fn apply(src: &str, fix: &Fix) -> String {
    let mut out = src.to_string();
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|e| std::cmp::Reverse((e.span.start, e.span.end)));
    for e in edits {
        out.replace_range(e.span.start as usize..e.span.end as usize, &e.insert);
    }
    out
}

// ---- Q34: the built-in prelude is the stdlib prelude -------------------------------------

/// The names `stdlib/src/prelude.kl` exports, other than the core names' stubs and the
/// re-exported prelude modules: with the stdlib loaded they are the prelude (project mode),
/// so the built-in prelude of a single-file check must hold every one of them.
fn std_prelude_exports() -> Vec<String> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../stdlib/src/prelude.kl");
    let text = std::fs::read_to_string(&path).expect("stdlib/src/prelude.kl");
    let out = kalam_parser::parse_source(&text, Skin::En, FileId(0));
    let mut names = Vec::new();
    for item in &out.ast.file.items {
        if item.vis == kalam_ast::Visibility::Private {
            continue;
        }
        let name = match &item.kind {
            kalam_ast::ItemKind::Type(t) => &t.name,
            kalam_ast::ItemKind::Enum(e) => &e.name,
            kalam_ast::ItemKind::Trait(t) => &t.name,
            kalam_ast::ItemKind::Extern(kalam_ast::ExternDecl::Type { name, .. }) => name,
            // Functions (`print`, `panic`, `assert`) are core names' stubs.
            _ => continue,
        };
        // Core names (`Int`, `World`, …) are prelude items by concept, in every skin.
        if let kalam_ast::NameKind::Ident(s) = &name.kind {
            names.push(s.clone());
        }
    }
    names
}

#[test]
fn every_std_prelude_export_is_a_builtin_prelude_name() {
    let exports = std_prelude_exports();
    assert!(exports.iter().any(|n| n == "Ordering"), "{exports:?}");
    let missing: Vec<_> = exports
        .iter()
        .filter(|n| !PRELUDE.contains(&n.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "std/prelude exports that a single-file check does not know: {missing:?}"
    );
}

#[test]
fn ordering_resolves_in_a_single_file() {
    // The iteration-3 verifier's r24: `Ordering` was K0401 in `kalam check FILE` only.
    let src = "fn rev(a: Float, b: Float) -> Ordering => b.total_cmp(a)\n\nfn main():\n    let o: Ordering = .Less\n    let p = Ordering.Greater\n    print(\"x\")\n";
    assert_eq!(codes("a.kl", src), Vec::<&str>::new());
    // The same names in every skin (they are fixed ASCII, spec §7.2).
    let ar = "دالة rev(a: كسري، b: كسري) -> Ordering => b.total_cmp(a)\n\nدالة رئيسي():\n    اطبع(\"x\")\n";
    assert_eq!(codes("a.ar.kl", ar), Vec::<&str>::new());
    // A declaration that reuses it is K0405, as in project mode.
    assert_eq!(codes("a.kl", "type Ordering:\n    n: Int\n"), vec!["K0405"]);
}

// ---- spec §10.1: K0405 comes with a rename fix-it -------------------------------------------

/// The single machine fix of the only finding, applied; the result must resolve cleanly.
fn renamed(path: &str, src: &str) -> String {
    let d = diags(path, src);
    assert_eq!(d.len(), 1, "{d:#?}");
    assert_eq!(d[0].code, "K0405");
    let fix = d[0]
        .fixes
        .first()
        .unwrap_or_else(|| panic!("no fix: {:#?}", d[0]));
    assert_eq!(fix.applicability, Applicability::Machine, "{fix:?}");
    let fixed = apply(src, fix);
    let after = diags(path, &fixed);
    assert!(after.is_empty(), "the fix leaves {after:#?}\n{fixed}");
    fixed
}

#[test]
fn a_shadowing_local_is_renamed_with_its_uses() {
    let src = "fn main():\n    let total = 1\n    if total > 0:\n        let total = 2\n        print(\"{total}\")\n    print(\"{total}\")\n";
    assert_eq!(
        renamed("a.kl", src),
        "fn main():\n    let total = 1\n    if total > 0:\n        let total_2 = 2\n        print(\"{total_2}\")\n    print(\"{total}\")\n"
    );
    // A parameter shadowed by a `let` in the body.
    assert_eq!(
        renamed(
            "a.kl",
            "fn f(x: Int) -> Int:\n    let x = 2\n    return x\n"
        ),
        "fn f(x: Int) -> Int:\n    let x_2 = 2\n    return x_2\n"
    );
    // The fresh name avoids every name of the file, so the fix cannot cause a new K0405.
    assert_eq!(
        renamed(
            "a.kl",
            "fn total_2() -> Int => 0\n\nfn main():\n    let total = 1\n    if total > 0:\n        let total = 2\n        print(\"{total}\")\n"
        ),
        "fn total_2() -> Int => 0\n\nfn main():\n    let total = 1\n    if total > 0:\n        let total_3 = 2\n        print(\"{total_3}\")\n"
    );
    // A local named like a module item, used in a lambda and as a label's value.
    assert_eq!(
        renamed(
            "a.kl",
            "fn area() -> Int => 1\n\nfn show(n: Int) -> Int => n\n\nfn main():\n    let area = 2\n    let f = fn(x: Int) => x + area\n    print(\"{show(n: area)} {f(1)}\")\n"
        ),
        "fn area() -> Int => 1\n\nfn show(n: Int) -> Int => n\n\nfn main():\n    let area_2 = 2\n    let f = fn(x: Int) => x + area_2\n    print(\"{show(n: area_2)} {f(1)}\")\n"
    );
    // A local named like a prelude name.
    assert_eq!(
        renamed(
            "a.kl",
            "fn main():\n    let json = 1\n    print(\"{json}\")\n"
        ),
        "fn main():\n    let json_2 = 1\n    print(\"{json_2}\")\n"
    );
}

#[test]
fn the_rename_uses_one_suffix_rule_in_every_skin() {
    // FOUNDER-QUEUE Q51: `_2`, `_3`, … in every skin, so a fixed file converts to the same
    // fixed file in another skin (no Arabic article, no Franco `el_`, no English lowercase).
    assert_eq!(
        renamed(
            "a.ar.kl",
            "هيكل شكل:\n    ضلع: عدد\n\nدالة رئيسي():\n    ليكن شكل = 1\n    اطبع(\"{شكل}\")\n"
        ),
        "هيكل شكل:\n    ضلع: عدد\n\nدالة رئيسي():\n    ليكن شكل_2 = 1\n    اطبع(\"{شكل_2}\")\n"
    );
    assert_eq!(
        renamed(
            "a.franco.kl",
            "dala ra2isi():\n    khali kotob = 1\n    law kotob > 0:\n        khali kotob = 2\n        ektb(\"{kotob}\")\n"
        ),
        "dala ra2isi():\n    khali kotob = 1\n    law kotob > 0:\n        khali kotob_2 = 2\n        ektb(\"{kotob_2}\")\n"
    );
    let fixed = renamed(
        "a.ar.kl",
        "دالة رئيسي():\n    ليكن total = 1\n    إذا total > 0:\n        ليكن total = 2\n        اطبع(\"{total}\")\n",
    );
    assert!(fixed.contains("ليكن total_2 = 2"), "{fixed}");
    // The suffix skips names the file already has, the same way in every skin.
    let fixed = renamed(
        "a.franco.kl",
        "dala ra2isi():\n    khali total = 1\n    law total > 0:\n        khali total = 2\n        ektb(\"{total}\")\n    khali total_2 = 5\n    ektb(\"{total} {total_2}\")\n",
    );
    assert!(fixed.contains("khali total_3 = 2"), "{fixed}");
}

#[test]
fn a_rename_that_cannot_be_mechanical_is_only_suggested_or_absent() {
    // A parameter's name is its label at call sites (§12.1): renaming it can break callers
    // that pass it by name, so the fix is a suggestion.
    let d = diags(
        "a.kl",
        "fn area() -> Int => 1\n\nfn f(area: Int) -> Int => area\n\nfn main():\n    print(\"{f(area: 2)}\")\n",
    );
    assert_eq!(d.len(), 1, "{d:#?}");
    assert_eq!(d[0].code, "K0405");
    assert!(d[0]
        .fixes
        .iter()
        .all(|f| f.applicability == Applicability::Suggestion));
    // Two items with one name: the later one is renamed (nothing refers to it).
    let fixed = renamed(
        "a.kl",
        "fn area() -> Int => 1\n\nfn area() -> Int => 2\n\nfn main():\n    print(\"{area()}\")\n",
    );
    assert!(fixed.contains("fn area_2() -> Int => 2"), "{fixed}");
}

#[test]
fn types_and_functions_get_a_number() {
    // One namespace (DR-11): a function named like a type, and a second type of one name,
    // get a number (Q51: one rule for every declaration in every skin).
    let fixed = renamed(
        "a.kl",
        "type Shape:\n    a: Int\n\nfn Shape() -> Int => 1\n\nfn main():\n    print(\"x\")\n",
    );
    assert!(fixed.contains("fn Shape_2() -> Int => 1"), "{fixed}");
    let fixed = renamed(
        "a.kl",
        "type Point:\n    x: Int\n\ntype Point:\n    y: Int\n\nfn main():\n    print(\"x\")\n",
    );
    assert!(fixed.contains("type Point_2:\n    y: Int"), "{fixed}");
    // Variants of one enum and fields of one record are named by the type checker's
    // members: no fix here.
    let d = diags("a.kl", "enum E:\n    A\n    A\n");
    assert_eq!(d.len(), 1);
    assert!(d[0].fixes.is_empty(), "{:#?}", d[0].fixes);
}
