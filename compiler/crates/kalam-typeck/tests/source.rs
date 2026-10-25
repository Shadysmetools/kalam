//! The type checker from source text: parse → resolve (with the stdlib signature stubs of
//! `stdlib/src`) → lower → check. Each test states the spec rule it checks; the programs are
//! written from the spec, in the English skin unless the test is about skins.

use kalam_base::{FileId, Skin};
use kalam_resolve::{resolve_program, ModulePath, Options, SourceModule};

/// The stdlib stubs: `(relative path, text)`, sorted.
fn stdlib() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../stdlib/src");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("stdlib/src") {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "kl") {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            out.push((format!("src/{name}"), std::fs::read_to_string(&p).unwrap()));
        }
    }
    out.sort();
    out
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct Found {
    /// Index into the program's files (user files first).
    file: usize,
    code: &'static str,
    line: usize,
}

fn line_of(text: &str, at: u32) -> usize {
    text[..(at as usize).min(text.len())].matches('\n').count() + 1
}

/// Checks `files` (`(path under src/, text)`) as the package `app` with the stdlib.
/// Panics on a lexer, parser or resolver error: these tests are about types.
fn check_full(files: &[(&str, &str)]) -> (Vec<Found>, Vec<kalam_diagnostics::Diagnostic>) {
    let std = stdlib();
    let mut texts: Vec<(String, bool, String)> = files
        .iter()
        .map(|(p, t)| (format!("src/{p}"), false, t.to_string()))
        .collect();
    for (p, t) in &std {
        texts.push((p.clone(), true, t.clone()));
    }
    let mut parsed = Vec::new();
    for (i, (path, _, text)) in texts.iter().enumerate() {
        let skin = Skin::from_path(path).expect("a Kalam path");
        let out = kalam_parser::parse_source(text, skin, FileId(i as u32));
        let errors: Vec<_> = out.diagnostics.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "{path} must parse: {errors:?}");
        parsed.push((skin, out));
    }
    let sources: Vec<SourceModule<'_>> = texts
        .iter()
        .zip(&parsed)
        .enumerate()
        .map(|(i, ((path, std, _), (skin, out)))| SourceModule {
            path: ModulePath::from_file(if *std { "std" } else { "app" }, path),
            file: FileId(i as u32),
            skin: *skin,
            ast: &out.ast,
            std: *std,
        })
        .collect();
    let options = Options {
        package: "app".into(),
        whole_package: true,
        std_loaded: true,
    };
    let (program, diags) = resolve_program(&sources, &options);
    let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "resolve must be clean: {errors:#?}");
    let checked = kalam_typeck::check_resolved(&program, &sources);
    let diags = checked.diagnostics();
    let found = diags
        .iter()
        .map(|d| Found {
            file: d.file.0 as usize,
            code: d.code,
            line: line_of(&texts[d.file.0 as usize].2, d.span.start),
        })
        .collect();
    (found, diags)
}

fn check(files: &[(&str, &str)]) -> Vec<Found> {
    check_full(files).0
}

/// `(code, line)` of every finding in the one-file program `main.kl`.
fn codes(src: &str) -> Vec<(&'static str, usize)> {
    check(&[("main.kl", src)])
        .into_iter()
        .map(|f| {
            assert_eq!(f.file, 0, "a finding in the stdlib: {f:?}");
            (f.code, f.line)
        })
        .collect()
}

fn clean(src: &str) {
    assert_eq!(codes(src), vec![], "{src}");
}

// ---- the stdlib itself ------------------------------------------------------------------

#[test]
fn the_stdlib_stubs_type_check() {
    let found = check(&[("main.kl", "fn main():\n    pass\n")]);
    assert!(found.is_empty(), "{found:#?}");
}

// ---- §5.6, §9.1, §9.8 and FOUNDER-QUEUE Q25: numbers -------------------------------------

#[test]
fn literals_follow_their_context_but_variables_never_convert() {
    clean("fn main():\n    let ratio = 2.5\n    let total = 1 + ratio\n    let mix = 1 + 2.5\n    let one = 1\n    let f = one.to_float() + ratio\n    print(\"{total} {mix} {f}\")\n");
    assert_eq!(
        codes("fn main():\n    let count = 1\n    let ratio = 2.5\n    let total = count + ratio\n    print(\"{total}\")\n"),
        vec![("K0601", 4)]
    );
    assert_eq!(
        codes("fn main():\n    let same = 1 == 1.0\n    print(\"{same}\")\n"),
        vec![("K0602", 2)]
    );
    assert_eq!(
        codes("fn main():\n    let x = 9223372036854775808\n    print(\"{x}\")\n"),
        vec![("K0126", 2)]
    );
    clean("fn main():\n    let x = -9223372036854775808\n    let d: Decimal = 19.99 + 1\n    print(\"{x} {d}\")\n");
}

#[test]
fn int_to_string_is_in_the_stubs() {
    // §6.2: "`Int.to_string()` always produces ASCII digits". Also the must-accept
    // lexing-hazards case num-ranges-and-member-index-valid (`5.to_string()`).
    clean("fn main():\n    let n = 42\n    let s: String = n.to_string()\n    print(5.to_string() + s)\n");
}

#[test]
fn k0601_has_a_to_float_fix() {
    let (_, diags) = check_full(&[(
        "main.kl",
        "fn main():\n    let count = 1\n    let ratio = 2.5\n    let total = count + ratio\n    print(\"{total}\")\n",
    )]);
    let fix = &diags[0].fixes[0];
    assert_eq!(fix.edits[0].insert, ".to_float()");
}

// ---- §9.4 records ---------------------------------------------------------------------------

#[test]
fn records_construct_by_name_update_with_with_and_compare_structurally() {
    clean("type Point:\n    x: Int\n    y: Int = 0\n\nfn main():\n    let p = Point(x: 1)\n    let q = p.with(y: 3)\n    print(\"{q.x} {q.y}\")\n    let same = p == Point(x: 1, y: 0)\n    print(\"{same}\")\n");
    assert_eq!(
        codes("type Point:\n    x: Int\n    y: Int\n\nfn main():\n    let p = Point(1, 2)\n    print(\"{p.x}\")\n"),
        vec![("K0416", 6)]
    );
    assert_eq!(
        codes("type Point:\n    x: Int\n\nfn main():\n    let p = Point(x: 1)\n    print(\"{p.z}\")\n"),
        vec![("K0420", 6)]
    );
}

#[test]
fn opaque_records_are_private_outside_their_module() {
    let found = check(&[
        (
            "email.kl",
            "export opaque type Email:\n    address: String\n\nimpl Email:\n    export fn parse(text: String) -> Email => Email(address: text)\n\n    export fn text(self) -> String => self.address\n",
        ),
        (
            "main.kl",
            "use app/email.{Email}\n\nfn main():\n    let e = Email.parse(\"a@b.c\")\n    print(e.text())\n    print(e.address)\n",
        ),
    ]);
    let codes: Vec<_> = found.iter().map(|f| (f.file, f.code, f.line)).collect();
    assert_eq!(codes, vec![(1, "K0418", 6)]);
}

#[test]
fn k0418_from_the_checker_names_the_items_module_like_resolve_does() {
    // The K0418 template uses `{name}` and `{module}`; resolve supplies both in backticks,
    // so the checker's K0418 must too, or the message shows a raw `{module}`.
    let (_, diags) = check_full(&[
        (
            "email.kl",
            "export opaque type Email:\n    address: String\n\nimpl Email:\n    export fn parse(text: String) -> Email => Email(address: text)\n",
        ),
        (
            "main.kl",
            "use app/email.{Email}\n\nfn main():\n    let e = Email.parse(\"a@b.c\")\n    print(e.address)\n",
        ),
    ]);
    let d = diags.iter().find(|d| d.code == "K0418").expect("a K0418");
    let arg = |k: &str| d.args.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
    assert_eq!(arg("name").as_deref(), Some("`address`"));
    assert_eq!(arg("module").as_deref(), Some("`app/email`"));
}

// ---- §9.5 enums and §11 matching ---------------------------------------------------------------

const LIGHT: &str = "enum Light:\n    Red\n    Amber\n    Green\n\n";

#[test]
fn match_exhaustiveness_and_usefulness() {
    clean(&format!(
        "{LIGHT}fn main():\n    let light: Light = .Red\n    match light:\n        case .Red:\n            print(\"stop\")\n        case .Amber | .Green:\n            print(\"go\")\n"
    ));
    assert_eq!(
        codes(&format!(
            "{LIGHT}fn main():\n    let light: Light = .Red\n    match light:\n        case .Red:\n            print(\"stop\")\n        case .Green:\n            print(\"go\")\n"
        )),
        vec![("K1001", 8)]
    );
    assert_eq!(
        codes(&format!(
            "{LIGHT}fn main():\n    let light: Light = .Red\n    match light:\n        case _:\n            print(\"any\")\n        case .Red:\n            print(\"stop\")\n"
        )),
        // The unreachable arm is `case .Red:`.
        vec![("K1002", 11)]
    );
    // Matching on `Int` needs a final catch-all (§11 rule 6).
    assert_eq!(
        codes("fn main():\n    let n = 7\n    match n:\n        case 0:\n            print(\"zero\")\n        case 1..=9:\n            print(\"digit\")\n"),
        vec![("K1001", 3)]
    );
}

#[test]
fn missing_cases_are_listed() {
    let (_, diags) = check_full(&[(
        "main.kl",
        "enum Shape:\n    Circle(r: Int)\n    Square(s: Int)\n    Empty\n\nfn area(s: Shape) -> Int:\n    return match s:\n        case .Circle(r): r * r\n\nfn main():\n    print(\"{area(.Empty)}\")\n",
    )]);
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].code, "K1001");
    let missing = &diags[0]
        .args
        .iter()
        .find(|(n, _)| *n == "missing")
        .unwrap()
        .1;
    assert_eq!(missing, ".Square(_), .Empty");
}

#[test]
fn or_patterns_bind_the_same_names() {
    let payment = "enum Payment:\n    Card(last4: String)\n    Wallet(id: String)\n    Cash\n\n";
    clean(&format!("{payment}fn main():\n    let p: Payment = .Cash\n    match p:\n        case .Card(code) | .Wallet(code):\n            print(code)\n        case .Cash:\n            print(\"cash\")\n"));
    assert_eq!(
        codes(&format!("{payment}fn main():\n    let p: Payment = .Cash\n    match p:\n        case .Card(last4) | .Wallet(id):\n            print(\"electronic\")\n        case .Cash:\n            print(\"cash\")\n")),
        vec![("K1004", 9)]
    );
}

#[test]
fn non_regular_recursive_types_derive_without_looping() {
    // `Tree[Int]: Eq` needs `Tree[List[Int]]: Eq`, which needs `Tree[List[List[Int]]]: Eq`…
    clean("enum Tree[T]:\n    Leaf(value: T)\n    Node(next: Tree[List[T]])\n\nfn main():\n    let t: Tree[Int] = .Leaf(value: 1)\n    let same = t == t\n    print(\"{same} {t}\")\n");
}

#[test]
fn let_patterns_must_be_irrefutable() {
    clean("fn main():\n    let point = (3, 4)\n    let (x, y) = point\n    print(\"{x} {y}\")\n");
    assert_eq!(
        codes("fn main():\n    let found: Option[Int] = Some(1)\n    let Some(n) = found\n    print(\"{n}\")\n"),
        vec![("K1003", 3)]
    );
}

#[test]
fn nested_and_list_patterns() {
    clean("fn first(xs: List[Int]) -> Int:\n    return match xs:\n        case []: 0\n        case [x, ..rest]: x + rest.len()\n\nfn main():\n    print(\"{first([1, 2])}\")\n");
    clean("enum E:\n    NotFound(id: Int)\n    Other\n\nfn f(r: Result[Int, E]) -> Int:\n    return match r:\n        case Ok(v): v\n        case Err(.NotFound(id)): id\n        case Err(.Other): 0\n\nfn main():\n    print(\"{f(Ok(1))}\")\n");
}

// ---- §10 statements ---------------------------------------------------------------------------

#[test]
fn assignment_needs_a_var_place_and_values_must_be_used() {
    clean("type Point:\n    x: Int\n\nfn main():\n    var p = Point(x: 1)\n    p.x = 5\n    var xs = [1, 2, 3]\n    xs[0] = 9\n    var m: Map[String, Int] = {}\n    m[\"a\"] = 1\n    let a = m[\"a\"]\n    print(\"{p.x} {xs[0]} {a}\")\n");
    assert_eq!(
        codes("fn main():\n    let total = 1\n    total = 2\n    print(\"{total}\")\n"),
        vec![("K0710", 3)]
    );
    assert_eq!(
        codes("fn main():\n    let n = 2\n    n + 1\n    print(\"{n}\")\n"),
        vec![("K0730", 3)]
    );
    assert_eq!(
        codes("fn main():\n    let name = \"Sara\"\n    let first = name[0]\n    print(first)\n"),
        vec![("K0650", 3)]
    );
}

#[test]
fn empty_collections_and_implicit_members_need_an_expected_type() {
    assert_eq!(
        codes("fn main():\n    let xs = []\n    print(\"x\")\n"),
        vec![("K0802", 2)]
    );
    assert_eq!(
        codes("fn main():\n    let m = {}\n    print(\"x\")\n"),
        vec![("K0802", 2)]
    );
    assert_eq!(
        codes(&format!(
            "{LIGHT}fn main():\n    let c = .Red\n    print(\"x\")\n"
        )),
        vec![("K0803", 7)]
    );
}

// ---- §12 functions ------------------------------------------------------------------------------

#[test]
fn missing_return_and_adjacent_same_type_parameters() {
    assert_eq!(
        codes("fn double(x: Int) -> Int:\n    x * 2\n\nfn main():\n    print(\"{double(2)}\")\n"),
        vec![("K1101", 2)]
    );
    clean("export fn transfer(from: Int, to: Int, amount: Int) -> Int => from + to + amount\n\nfn main():\n    let r = transfer(1, to: 2, amount: 3)\n    print(\"{r}\")\n");
    assert_eq!(
        codes("export fn transfer(from: Int, to: Int, amount: Int) -> Int => from + to + amount\n\nfn main():\n    let r = transfer(1, 2, 3)\n    print(\"{r}\")\n"),
        vec![("K1104", 4)]
    );
}

#[test]
fn k1104_fix_names_the_arguments() {
    let src = "export fn transfer(from: Int, to: Int, amount: Int) -> Int => from + to + amount\n\nfn main():\n    let r = transfer(1, 2, 3)\n    print(\"{r}\")\n";
    let (_, diags) = check_full(&[("main.kl", src)]);
    let fix = &diags[0].fixes[0];
    let mut text = src.to_string();
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|e| std::cmp::Reverse(e.span.start));
    for e in edits {
        text.replace_range(e.span.start as usize..e.span.end as usize, &e.insert);
    }
    assert!(text.contains("transfer(1, to: 2, amount: 3)"), "{text}");
    clean(&text);
}

#[test]
fn call_sites_never_write_type_arguments() {
    // §9.6 / DR-25 and §21.3 K0922: `f[T](x)`; the fix deletes `[T]`.
    let src = "fn first[T](xs: List[T]) -> Option[T] => xs.get(0)\n\nfn main():\n    let xs = [1, 2, 3]\n    let head = first[Int](xs)\n    print(\"{head.is_some()}\")\n";
    let (found, diags) = check_full(&[("main.kl", src)]);
    let codes: Vec<_> = found.iter().map(|f| (f.code, f.line)).collect();
    assert_eq!(codes, vec![("K0922", 5)]);
    let edit = &diags[0].fixes[0].edits[0];
    let mut text = src.to_string();
    text.replace_range(
        edit.span.start as usize..edit.span.end as usize,
        &edit.insert,
    );
    assert!(text.contains("let head = first(xs)"), "{text}");
    clean(&text);
}

/// `src` with every edit of `fix` applied.
fn apply(src: &str, fix: &kalam_diagnostics::Fix) -> String {
    let mut text = src.to_string();
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|e| std::cmp::Reverse(e.span.start));
    for e in edits {
        text.replace_range(e.span.start as usize..e.span.end as usize, &e.insert);
    }
    text
}

#[test]
fn machine_fixes_are_spelled_in_the_files_skin() {
    // K1101: the fix inserts `return` as the file's skin spells it (§12.1, §21.4 rule 1).
    for (path, src) in [
        (
            "main.ar.kl",
            "دالة double(x: عدد) -> عدد:\n    x * 2\n\nدالة رئيسي():\n    اطبع(\"{double(2)}\")\n",
        ),
        (
            "main.franco.kl",
            "dala double(x: adad) -> adad:\n    x * 2\n\ndala ra2isi():\n    ektb(\"{double(2)}\")\n",
        ),
    ] {
        let (found, diags) = check_full(&[(path, src)]);
        let codes: Vec<_> = found.iter().map(|f| (f.code, f.line)).collect();
        assert_eq!(codes, vec![("K1101", 2)], "{path}");
        let fix = &diags[0].fixes[0];
        assert_eq!(fix.applicability, kalam_diagnostics::Applicability::Machine);
        let fixed = apply(src, fix);
        let after: Vec<_> = check(&[(path, &fixed)]).iter().map(|f| f.code).collect();
        assert!(after.is_empty(), "{path}: {fixed}\n{after:?}");
    }
    // K1104: the labels are the parameters' own names, in any skin.
    let src = "talla3 dala transfer(from: adad, to: adad, amount: adad) -> adad => from + to + amount\n\ndala ra2isi():\n    khali r = transfer(1, 2, 3)\n    ektb(\"{r}\")\n";
    let (found, diags) = check_full(&[("main.franco.kl", src)]);
    assert_eq!(
        found.iter().map(|f| f.code).collect::<Vec<_>>(),
        vec!["K1104"]
    );
    let fixed = apply(src, &diags[0].fixes[0]);
    assert!(check(&[("main.franco.kl", &fixed)]).is_empty(), "{fixed}");
}

#[test]
fn methods_resolve_to_exactly_one_candidate() {
    let traits = "trait Named:\n    fn label(self) -> String\n\ntrait Tagged:\n    fn label(self) -> String\n\ntype Item:\n    id: Int\n\nimpl Item as Named:\n    fn label(self) -> String => \"named\"\n\nimpl Item as Tagged:\n    fn label(self) -> String => \"tagged\"\n\n";
    assert_eq!(
        codes(&format!(
            "{traits}fn main():\n    let item = Item(id: 1)\n    print(item.label())\n"
        )),
        vec![("K1103", 18)]
    );
    clean(&format!(
        "{traits}fn main():\n    let item = Item(id: 1)\n    print(Named.label(item))\n    print(Tagged.label(item))\n"
    ));
    // Stdlib methods resolve on built-in types without any import (DR-27).
    clean("fn main():\n    let words = \" a b \".trim().split(\" \")\n    print(\"{words.len()} {words.is_empty()}\")\n");
    assert_eq!(
        codes("fn main():\n    let xs = [1, 2]\n    let n = xs.length()\n    print(\"{n}\")\n"),
        vec![("K0420", 3)]
    );
}

#[test]
fn closures_capture_let_bindings_only() {
    assert_eq!(
        codes("fn main():\n    var count = 1\n    let add = fn(x: Int) => x + count\n    print(\"{add(1)}\")\n"),
        vec![("K1110", 3)]
    );
    clean("fn main():\n    var count = 1\n    count += 1\n    let snapshot = count\n    let add = fn(x: Int) => x + snapshot\n    fn twice(x: Int) -> Int:\n        let once = add(x)\n        return add(once)\n    print(\"{twice(1)}\")\n");
    assert_eq!(
        codes("fn main():\n    let inc = fn(x) => x + 1\n    print(\"{inc(1)}\")\n"),
        vec![("K0801", 2)]
    );
    clean("fn apply(f: fn(Int) -> Int, v: Int) -> Int => f(v)\n\nfn main():\n    print(\"{apply(fn(x) => x + 1, 1)}\")\n");
}

// ---- §14 errors ---------------------------------------------------------------------------------

const FAILS: &str = "enum ParseFail:\n    Empty\n\nenum LoadFail:\n    Missing\n\nfn parse(text: String) -> Result[Int, ParseFail]:\n    if text == \"\":\n        return Err(.Empty)\n    return Ok(1)\n\n";

#[test]
fn question_mark_follows_the_four_rules() {
    // Rule 1: same error type.
    clean(&format!("{FAILS}fn again(text: String) -> Result[Int, ParseFail]:\n    let n = parse(text)?\n    return Ok(n + 1)\n\nfn main():\n    let _ = again(\"1\")\n"));
    // Rule 2: `impl F as From[E]`.
    clean(&format!("{FAILS}impl LoadFail as From[ParseFail]:\n    fn from(value: ParseFail) -> LoadFail => LoadFail.Missing\n\nfn load(text: String) -> Result[Int, LoadFail]:\n    let n = parse(text)?\n    return Ok(n)\n\nfn main():\n    let _ = load(\"1\")\n"));
    // Rule 3: boxing into `AnyError` needs `E: Error`.
    clean(&format!("{FAILS}impl ParseFail as Error:\n    fn message(self) -> String => \"empty\"\n\nfn load(text: String) -> Result[Int, AnyError]:\n    let n = parse(text)?\n    return Ok(n)\n\nfn main():\n    let _ = load(\"1\")\n"));
    // Rule 4: K1301.
    assert_eq!(
        codes(&format!("{FAILS}fn load(text: String) -> Result[Int, LoadFail]:\n    let n = parse(text)?\n    return Ok(n)\n\nfn main():\n    let _ = load(\"1\")\n")),
        vec![("K1301", 13)]
    );
}

#[test]
fn question_mark_on_option() {
    clean("fn head_plus_one(xs: List[Int]) -> Option[Int]:\n    let n = xs.get(0)?\n    return Some(n + 1)\n\nfn head(xs: List[Int]) -> Result[Int, String]:\n    let n = xs.get(0).ok_or(\"empty\")?\n    return Ok(n)\n\nfn main():\n    let a = head_plus_one([2])\n    let _ = head([2])\n    print(\"{a.is_some()}\")\n");
    assert_eq!(
        codes("fn head(xs: List[Int]) -> Result[Int, String]:\n    let n = xs.get(0)?\n    return Ok(n)\n\nfn main():\n    let _ = head([2])\n"),
        vec![("K1302", 2)]
    );
}

#[test]
fn results_are_must_use() {
    assert_eq!(
        codes("fn save(n: Int) -> Result[(), String]:\n    if n < 0:\n        return Err(\"negative\")\n    return Ok(())\n\nfn main():\n    save(1)\n"),
        vec![("K1303", 7)]
    );
    clean("fn save(n: Int) -> Result[(), String]:\n    return Ok(())\n\nfn main():\n    let _ = save(1)\n");
}

#[test]
fn return_type_inference_from_the_expected_type() {
    // §9.9 rule 5: `json.decode` takes `T` from the expected type, through `?`.
    clean("@derive(Decode, Encode)\ntype Point:\n    x: Int\n\nfn main() -> Result[(), AnyError]:\n    let p: Point = json.decode(\"\")?\n    let back: Point = json.decode(json.encode(p))?\n    print(\"{p.x} {back.x}\")\n    return Ok(())\n");
    // A type that does not derive `Decode` cannot be decoded.
    assert_eq!(
        codes("type Point:\n    x: Int\n\nfn main() -> Result[(), AnyError]:\n    let p: Point = json.decode(\"\")?\n    print(\"{p.x}\")\n    return Ok(())\n"),
        vec![("K0603", 5)]
    );
}

// ---- §17 sinks ----------------------------------------------------------------------------------

#[test]
fn sinks_never_take_string_and_the_fix_builds_a_tagged_literal() {
    let src = "use std/sql.{Db, DbError}\n\nfn count(db: Db, city: String) -> Result[Int, DbError]:\n    return db.exec(\"DELETE FROM t WHERE city = '\" + city + \"'\")\n\nfn main():\n    pass\n";
    let (found, diags) = check_full(&[("main.kl", src)]);
    assert_eq!(found.len(), 1, "{diags:#?}");
    assert_eq!((found[0].code, found[0].line), ("K1601", 4));
    let fix = &diags[0].fixes[0];
    assert_eq!(fix.applicability, kalam_diagnostics::Applicability::Machine);
    let mut text = src.to_string();
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|e| std::cmp::Reverse(e.span.start));
    for e in edits {
        text.replace_range(e.span.start as usize..e.span.end as usize, &e.insert);
    }
    assert!(
        text.contains("db.exec(sql\"DELETE FROM t WHERE city = {city}\")"),
        "{text}"
    );
    clean(&text);
}

#[test]
fn tagged_literals_record_their_hole_types_for_the_sink_checks() {
    // The typed hook for §17.3–§17.6 (next iteration): each literal's parts and hole types.
    let src = "use std/sql.{Db, DbError}\n\nfn f(db: Db, id: Int) -> Result[Int, DbError]:\n    return db.exec(sql\"DELETE FROM t WHERE id = {id}\")\n\nfn main():\n    pass\n";
    let std = stdlib();
    let mut texts = vec![("src/main.kl".to_string(), false, src.to_string())];
    texts.extend(std.into_iter().map(|(p, t)| (p, true, t)));
    let parsed: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(i, (p, _, t))| {
            kalam_parser::parse_source(t, Skin::from_path(p).unwrap(), FileId(i as u32))
        })
        .collect();
    let sources: Vec<SourceModule<'_>> = texts
        .iter()
        .zip(&parsed)
        .enumerate()
        .map(|(i, ((p, std, _), out))| SourceModule {
            path: ModulePath::from_file(if *std { "std" } else { "app" }, p),
            file: FileId(i as u32),
            skin: Skin::En,
            ast: &out.ast,
            std: *std,
        })
        .collect();
    let options = Options {
        package: "app".into(),
        whole_package: true,
        std_loaded: true,
    };
    let (program, _) = resolve_program(&sources, &options);
    let checked = kalam_typeck::check_resolved(&program, &sources);
    let lit = checked
        .results
        .tagged
        .iter()
        .find(|t| t.file == FileId(0))
        .expect("the sql literal");
    assert_eq!(lit.tag, kalam_sinks::Tag::Sql);
    assert_eq!(
        lit.parts,
        vec!["DELETE FROM t WHERE id = ".to_string(), String::new()]
    );
    assert_eq!(lit.holes.len(), 1);
    assert_eq!(lit.holes[0].1, kalam_typeck::Ty::Int);
}

#[test]
fn path_holes_must_be_path_segments() {
    assert_eq!(
        codes("use std/fs.{FsError}\n\nfn read(fs: Fs, name: String) -> Result[String, FsError]:\n    return fs.read_text(path\"uploads/{name}\")\n\nfn main():\n    pass\n"),
        vec![("K1641", 4)]
    );
    clean("use std/fs.{FsError}\n\nfn read(fs: Fs, name: PathSegment) -> Result[String, FsError]:\n    return fs.read_text(path\"uploads/{name}\")\n\nfn main():\n    pass\n");
}

// ---- skins --------------------------------------------------------------------------------------

#[test]
fn every_skin_checks_the_same_way() {
    let en = "fn main():\n    let total = 1 == 1.0\n    print(\"{total}\")\n";
    let ar = "دالة رئيسي():\n    ليكن total = 1 == 1.0\n    اطبع(\"{total}\")\n";
    let franco = "dala ra2isi():\n    khali total = 1 == 1.0\n    ektb(\"{total}\")\n";
    for (path, src) in [
        ("main.kl", en),
        ("main.ar.kl", ar),
        ("main.franco.kl", franco),
    ] {
        let found: Vec<_> = check(&[(path, src)])
            .into_iter()
            .map(|f| (f.code, f.line))
            .collect();
        assert_eq!(found, vec![("K0602", 2)], "{path}");
    }
}

// ---- M2 iteration 3: soundness holes the iteration-2 verifier found ------------------------

/// The fixes of the diagnostics with `code`, from a one-file program.
fn fixes_of(src: &str, code: &str) -> Vec<kalam_diagnostics::Fix> {
    let (_, diags) = check_full(&[("main.kl", src)]);
    diags
        .into_iter()
        .filter(|d| d.code == code)
        .flat_map(|d| d.fixes)
        .collect()
}

/// The machine fix of the only diagnostic, applied; the result must check clean.
fn machine_fix_makes_it_clean(path: &str, src: &str, code: &str) -> String {
    let (found, diags) = check_full(&[(path, src)]);
    let codes: Vec<_> = found.iter().map(|f| f.code).collect();
    assert_eq!(codes, vec![code], "{path}: {diags:#?}");
    let fix = diags[0]
        .fixes
        .iter()
        .find(|f| f.applicability == kalam_diagnostics::Applicability::Machine)
        .unwrap_or_else(|| panic!("{path}: no machine fix: {:#?}", diags[0]));
    let fixed = apply(src, fix);
    let after: Vec<_> = check(&[(path, &fixed)]).iter().map(|f| f.code).collect();
    assert!(after.is_empty(), "{path}:\n{fixed}\n{after:?}");
    fixed
}

fn label_texts(src: &str, code: &str) -> Vec<String> {
    let (_, diags) = check_full(&[("main.kl", src)]);
    diags
        .into_iter()
        .filter(|d| d.code == code)
        .flat_map(|d| d.labels.into_iter().map(|l| l.text))
        .collect()
}

const NAMED: &str = "trait Named:\n    fn name(self) -> String\n\ntype Box:\n    v: Int\n\n";
const SHOW_NAME: &str =
    "\nfn show_name[T: Named](x: T) -> String => x.name()\n\nfn main():\n    print(show_name(Box(v: 1)))\n";

#[test]
fn impls_must_match_their_trait_k1107() {
    // FOUNDER-QUEUE Q27 (spec §27 erratum): an impl writes exactly the trait's methods, with
    // the trait's signatures after substitution. Each case is reported once, at the impl.
    let wrong_return =
        format!("{NAMED}impl Box as Named:\n    fn name(self) -> Int => self.v\n{SHOW_NAME}");
    assert_eq!(codes(&wrong_return), vec![("K1107", 8)]);
    let missing_and_extra =
        format!("{NAMED}impl Box as Named:\n    fn other(self) -> Int => self.v\n{SHOW_NAME}");
    assert_eq!(codes(&missing_and_extra), vec![("K1107", 7), ("K1107", 8)]);
    let extra_param = format!(
        "{NAMED}impl Box as Named:\n    fn name(self, n: Int) -> String => \"x\"\n{SHOW_NAME}"
    );
    assert_eq!(codes(&extra_param), vec![("K1107", 8)]);
    let no_self =
        format!("{NAMED}impl Box as Named:\n    fn name() -> String => \"x\"\n{SHOW_NAME}");
    assert_eq!(codes(&no_self), vec![("K1107", 8)]);
    // `From[E1]` fixes `from`'s parameter to `E1`.
    let errors = "enum E1:\n    Bad\n\nenum E2:\n    Worse\n\n";
    let from_wrong_param = format!("{errors}impl E2 as From[E1]:\n    fn from(value: Int) -> E2 => .Worse\n\nfn a() -> Result[Int, E1] => Err(.Bad)\n\nfn b() -> Result[Int, E2]:\n    let v = a()?\n    return Ok(v)\n\nfn main():\n    let _ = b()\n");
    assert_eq!(codes(&from_wrong_param), vec![("K1107", 8)]);
    clean(&from_wrong_param.replace("value: Int", "value: E1"));
    // A static trait method reached through the type: callers see the trait's signature, so
    // the mismatch is reported at the impl only, not again at `E2.from(…)`.
    let static_bypass = "enum E2:\n    Worse\n\nimpl E2 as From[String]:\n    fn from(value: String) -> Int => 1\n\nfn main():\n    let e: E2 = E2.from(\"x\")\n    let _ = e\n";
    assert_eq!(codes(static_bypass), vec![("K1107", 5)]);
    // Method type parameters: their number, and no bound the trait does not require.
    let generic_trait = "trait Describe:\n    fn describe[T: Show](self, x: T) -> String\n\ntype Box:\n    v: Int\n\n";
    clean(&format!("{generic_trait}impl Box as Describe:\n    fn describe[U: Show](self, x: U) -> String => \"{{x}}\"\n\nfn main():\n    print(Box(v: 1).describe(2))\n"));
    assert_eq!(
        codes(&format!("{generic_trait}impl Box as Describe:\n    fn describe[U: Show + Eq](self, x: U) -> String => \"{{x}}\"\n\nfn main():\n    print(Box(v: 1).describe(2))\n")),
        vec![("K1107", 8)]
    );
    assert_eq!(
        codes(&format!("{generic_trait}impl Box as Describe:\n    fn describe(self, x: Int) -> String => \"{{x}}\"\n\nfn main():\n    print(Box(v: 1).describe(2))\n")),
        vec![("K1107", 8)]
    );
    // The trait's own parameters' bounds hold for the impl's trait arguments.
    let shown = "trait Render[T: Show]:
    fn render(self, x: T) -> String

type Box:
    v: Int

";
    clean(&format!(
        "{shown}impl Box as Render[Int]:
    fn render(self, x: Int) -> String => \"{{x}}\"

fn main():
    print(Box(v: 1).render(2))
"
    ));
    assert_eq!(
        codes(&format!(
            "{shown}impl Box as Render[fn() -> Int]:
    fn render(self, x: fn() -> Int) -> String => \"f\"

fn main():
    pass
"
        )),
        vec![("K0603", 7)]
    );
    // A trait method with a body is a default the impl may leave out.
    clean("trait Greet:\n    fn name(self) -> String\n    fn greet(self) -> String => \"hi \" + self.name()\n\ntype P:\n    n: String\n\nimpl P as Greet:\n    fn name(self) -> String => self.n\n\nfn main():\n    print(P(n: \"x\").greet())\n");
    // The message names the trait as written in the impl head, and the difference.
    let (_, diags) = check_full(&[("main.kl", &from_wrong_param)]);
    let arg = |k: &str| {
        diags[0]
            .args
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(arg("trait").as_deref(), Some("From[E1]"));
    assert_eq!(
        arg("detail").as_deref(),
        Some("parameter `value` of `from` is `Int` here, but `E1` in the trait")
    );
}

#[test]
fn a_rejected_overlapping_impl_causes_no_follow_on_ambiguity() {
    // The second `impl Box as Named` is K1106; the call then resolves to the first impl
    // instead of reporting K1103 between the two.
    let src = format!("{NAMED}impl Box as Named:\n    fn name(self) -> String => \"one\"\n\nimpl Box as Named:\n    fn name(self) -> String => \"two\"\n\nfn main():\n    print(Box(v: 1).name())\n");
    assert_eq!(codes(&src), vec![("K1106", 10)]);
}

#[test]
fn map_keys_and_set_elements_must_be_eq_and_hash() {
    // §9.3: `Map[K, V]` and `Set[T]` need `K`/`T: Eq + Hash`, wherever the type is formed.
    // Each root cause is reported once: the value's later uses stay quiet.
    assert_eq!(
        codes("fn main():\n    let m: Map[Float, Int] = {1.5: 1}\n    print(m.len())\n"),
        vec![("K0603", 2)]
    );
    assert_eq!(
        codes("fn main():\n    let m = {1.5: 1}\n    print(m.len())\n"),
        vec![("K0603", 2)]
    );
    assert_eq!(
        codes("fn main():\n    let s = Set.of([1.5, 2.5])\n    print(s.len())\n"),
        vec![("K0603", 2)]
    );
    assert_eq!(
        codes("type Point:\n    x: Float\n\nfn main():\n    let m: Map[Point, Int] = {Point(x: 1.0): 1}\n    let _ = m\n"),
        vec![("K0603", 5)]
    );
    // A record field, and a generic parameter that lacks the bounds (reported once, at the
    // declaration; the body and the callers then assume them).
    assert_eq!(
        codes("type Index[T]:\n    entries: Map[T, Int]\n\nfn main():\n    let i = Index(entries: {})\n    let _ = i\n"),
        vec![("K0603", 2)]
    );
    assert_eq!(
        codes(
            "fn wrap[T](x: T) -> Set[T] => Set.of([x])\n\nfn main():\n    print(wrap(1).len())\n"
        ),
        vec![("K0603", 1)]
    );
    clean("fn wrap[T: Eq + Hash](x: T) -> Set[T] => Set.of([x])\n\nfn count[K: Eq + Hash](m: Map[K, Int]) -> Int => m.len()\n\nfn main():\n    let m: Map[String, Int] = {\"a\": 1}\n    print(\"{wrap(1).len()} {count(m)}\")\n");
    // The bounds of a generic record or enum hold where it is written and where it is built.
    let boxed = "type Box[T: Eq]:\n    v: T\n\n";
    clean(&format!(
        "{boxed}fn main():\n    let b: Box[Int] = Box(v: 1)\n    print(b.v)\n"
    ));
    assert_eq!(
        codes(&format!(
            "{boxed}fn main():\n    let b = Box(v: 1.5)\n    print(b.v)\n"
        )),
        vec![("K0603", 5)]
    );
    assert_eq!(
        codes(&format!(
            "{boxed}fn f(b: Box[Float]) -> Float => b.v\n\nfn main():\n    pass\n"
        )),
        vec![("K0603", 4)]
    );
    // The label names the rule.
    assert_eq!(
        label_texts(
            "fn main():\n    let m: Map[Float, Int] = {}\n    let _ = m\n",
            "K0603"
        ),
        vec!["a `Map` key must be `Eq` and `Hash`".to_string()]
    );
}

#[test]
fn main_has_the_signature_of_section_14_3() {
    // "`main` may return `()` or `Result[(), E]` with `E: Error`"; §16.1: it takes nothing,
    // or the `World`.
    clean("fn main() -> Result[(), AnyError]:\n    return Ok(())\n");
    clean("fn main(world: World):\n    let _ = world\n");
    assert_eq!(
        codes("fn main() -> Result[(), String]:\n    return Ok(())\n"),
        vec![("K0603", 1)]
    );
    assert_eq!(
        codes("fn main() -> Result[Int, AnyError]:\n    return Ok(1)\n"),
        vec![("K0600", 1)]
    );
    assert_eq!(
        codes("fn main() -> Int:\n    return 1\n"),
        vec![("K0600", 1)]
    );
    assert_eq!(
        codes("fn main(n: Int):\n    print(n)\n"),
        vec![("K0600", 1)]
    );
    // `main` is a core name: the rule follows it into every skin.
    let found: Vec<_> = check(&[(
        "main.ar.kl",
        "دالة رئيسي() -> نتيجة[()، نص]:\n    أرجع نجاح(())\n",
    )])
    .into_iter()
    .map(|f| (f.code, f.line))
    .collect();
    assert_eq!(found, vec![("K0603", 1)]);
}

#[test]
fn derives_are_checked_where_they_are_written() {
    // §9.7: `@derive(Ord)` on a record with a `Float` field is reported at the derive, and
    // the comparison that uses it is then quiet (one root cause).
    assert_eq!(
        codes("@derive(Ord)\ntype Reading:\n    value: Float\n\nfn main():\n    let r = Reading(value: 1.0)\n    print(r < r)\n"),
        vec![("K0603", 1)]
    );
    assert_eq!(
        codes("@derive(Eq, Hash)\nenum Shape:\n    Circle(r: Float)\n    Empty\n\nfn main():\n    let s: Shape = .Empty\n    print(s == s)\n"),
        vec![("K0603", 1)]
    );
    // A derive is conditional on the type's own parameters.
    clean("@derive(Eq, Ord)\ntype Box[T]:\n    v: T\n\nfn main():\n    let a = Box(v: 1)\n    print(a < a)\n");
    // `Encode` with a `Secret` field stays K0612 alone.
    assert_eq!(
        codes(
            "@derive(Encode)\ntype Login:\n    password: Secret[String]\n\nfn main():\n    pass\n"
        ),
        vec![("K0612", 2)]
    );
    assert_eq!(
        label_texts(
            "@derive(Ord)\ntype Reading:\n    value: Float\n\nfn main():\n    pass\n",
            "K0603"
        ),
        vec!["`@derive(Ord)` needs every field to be `Ord`".to_string()]
    );
}

#[test]
fn question_mark_in_a_lambda_returns_from_the_lambda_q28() {
    // FOUNDER-QUEUE Q28: `?` returns from the lambda, whose return type becomes a `Result`
    // with the operand's error type (its success type is the body's own) or an `Option`.
    clean(&format!("{FAILS}fn main():\n    let positive = fn(text: String) => Ok(parse(text)? > 0)\n    match positive(\"1\"):\n        case Ok(b):\n            print(\"{{b}}\")\n        case Err(_):\n            print(\"no\")\n"));
    clean("fn main():\n    let next = fn(xs: List[Int]) => Some(xs.get(0)? + 1)\n    match next([1]):\n        case Some(n):\n            print(\"{n}\")\n        case None:\n            print(\"none\")\n");
    // The lambda returns an `Option`, so its body must be one too.
    assert_eq!(
        codes("fn f(xs: List[List[Int]]) -> Result[List[Int], String]:\n    let firsts = xs.map(fn(ys: List[Int]) => ys.get(0)?)\n    let _ = firsts\n    return Ok([])\n\nfn main():\n    let _ = f([])\n"),
        vec![("K0600", 2)]
    );
}

#[test]
fn int_division_where_an_int_is_expected_is_a_mismatch_q31() {
    // FOUNDER-QUEUE Q31: `Int / Int` is a `Float`; `let x: Int = 7 / 2` is K0600, expected
    // `Int`, found `Float`, with `//` suggested.
    let src = "fn main():\n    let half: Int = 7 / 2\n    print(half)\n";
    assert_eq!(codes(src), vec![("K0600", 2)]);
    assert_eq!(
        label_texts(src, "K0600"),
        vec!["expected `Int`, found `Float`".to_string()]
    );
    let fix = &fixes_of(src, "K0600")[0];
    assert_eq!(
        fix.applicability,
        kalam_diagnostics::Applicability::Suggestion
    );
    let fixed = apply(src, fix);
    assert!(fixed.contains("let half: Int = 7 // 2"), "{fixed}");
    clean(&fixed);
    // `x /= n` on an `Int` likewise.
    let src = "fn main():\n    var total = 9\n    total /= 2\n    print(total)\n";
    assert_eq!(codes(src), vec![("K0600", 3)]);
    let fixed = apply(src, &fixes_of(src, "K0600")[0]);
    assert!(fixed.contains("total = total // 2"), "{fixed}");
    clean(&fixed);
    // A value of one number type where another is expected is a mismatch too, with the
    // conversion as a machine fix; mixing two operands stays K0601.
    machine_fix_makes_it_clean(
        "main.kl",
        "fn half(x: Float) -> Float => x / 2.0\n\nfn main():\n    let n = 3\n    print(half(n))\n",
        "K0600",
    );
    assert_eq!(
        codes("fn main():\n    let count = 1\n    let ratio = 2.5\n    print(count + ratio)\n"),
        vec![("K0601", 4)]
    );
}

#[test]
fn k1001_fix_adds_the_missing_arms_in_the_files_skin() {
    // §11 rule 1, FOUNDER-QUEUE Q32: the missing cases as `case` arms with `panic("TODO")`
    // bodies, at the arms' column.
    let en = format!("{LIGHT}fn main():\n    let light: Light = .Red\n    match light:\n        case .Red:\n            print(\"stop\")\n        case .Green:\n            print(\"go\")\n");
    let fixed = machine_fix_makes_it_clean("main.kl", &en, "K1001");
    assert!(
        fixed.ends_with("print(\"go\")\n        case .Amber: panic(\"TODO\")\n"),
        "{fixed}"
    );
    // A value-form match, nested patterns and the core constructors.
    machine_fix_makes_it_clean(
        "main.kl",
        "enum E:\n    A\n    B\n\nfn describe(r: Result[E, String]) -> String:\n    return match r:\n        case Ok(.A): \"a\"\n        case Err(_): \"err\"\n\nfn main():\n    print(describe(Ok(.B)))\n",
        "K1001",
    );
    machine_fix_makes_it_clean(
        "main.kl",
        "fn f(p: (Bool, Bool)) -> Int:\n    return match p:\n        case (true, _): 1\n        case (false, true): 2\n\nfn main():\n    print(f((true, true)))\n",
        "K1001",
    );
    // The other skins: `case`, `panic`, `Some` and the comma are spelled as the file does.
    machine_fix_makes_it_clean(
        "main.ar.kl",
        "دالة f(x: اختياري[عدد]) -> عدد:\n    أرجع حسب x:\n        عندما لا_شيء: 0\n\nدالة رئيسي():\n    اطبع(f(لا_شيء))\n",
        "K1001",
    );
    machine_fix_makes_it_clean(
        "main.franco.kl",
        "dala f(x: ekhtiyari[adad]) -> adad:\n    rag3 hasab x:\n        lama mafeesh: 0\n\ndala ra2isi():\n    ektb(f(mafeesh))\n",
        "K1001",
    );
}

#[test]
fn k1110_fix_copies_the_var_before_the_closure() {
    // §12.3: "Capturing a `var` is K1110, with the fix `let snapshot = v` inserted before
    // the closure." One error per `var` and closure, covering every use.
    let fixed = machine_fix_makes_it_clean(
        "main.kl",
        "fn main():\n    var count = 1\n    count += 1\n    let add = fn(x: Int) => x + count\n    print(\"{add(1)}\")\n",
        "K1110",
    );
    assert!(
        fixed.contains("    let snapshot = count\n    let add = fn(x: Int) => x + snapshot\n"),
        "{fixed}"
    );
    machine_fix_makes_it_clean(
        "main.kl",
        "fn main():\n    var total = 0\n    total += 2\n    fn twice(x: Int) -> Int:\n        return x + total + total\n    print(\"{twice(1)}\")\n",
        "K1110",
    );
}

#[test]
fn question_mark_fixes_follow_section_14_3() {
    // K1302: `.ok_or(…)` before the `?` (a `String` error gets a text: a machine fix).
    machine_fix_makes_it_clean(
        "main.kl",
        "fn head(xs: List[Int]) -> Result[Int, String]:\n    let n = xs.get(0)?\n    return Ok(n)\n\nfn main():\n    let _ = head([2])\n",
        "K1302",
    );
    // K1301: `.map_err(…)` (into a `String`: a machine fix) and `impl F as From[E]`.
    machine_fix_makes_it_clean(
        "main.kl",
        &format!("{FAILS}fn load(text: String) -> Result[Int, String]:\n    let n = parse(text)?\n    return Ok(n)\n\nfn main():\n    let _ = load(\"1\")\n"),
        "K1301",
    );
    let src = format!("{FAILS}fn load(text: String) -> Result[Int, LoadFail]:\n    let n = parse(text)?\n    return Ok(n)\n\nfn main():\n    let _ = load(\"1\")\n");
    let fixes = fixes_of(&src, "K1301");
    assert_eq!(fixes.len(), 2, "{fixes:#?}");
    assert!(fixes
        .iter()
        .all(|f| f.applicability == kalam_diagnostics::Applicability::Suggestion));
    let with_impl = apply(&src, &fixes[1]);
    assert!(
        with_impl.contains("impl LoadFail as From[ParseFail]:\n    fn from(value: ParseFail) -> LoadFail => panic(\"TODO\")"),
        "{with_impl}"
    );
    clean(&with_impl);
}

#[test]
fn diagnostics_name_generics_and_types_and_report_one_root_cause() {
    // Generic parameters are shown by name, never as `T0`.
    let (_, diags) = check_full(&[(
        "main.kl",
        "fn describe[T](x: T) -> String => \"value {x}\"\n\nfn main():\n    print(describe(1))\n",
    )]);
    let ty = diags[0]
        .args
        .iter()
        .find(|(n, _)| *n == "type")
        .map(|(_, v)| v.clone());
    assert_eq!(ty.as_deref(), Some("T"));
    // Every K0600 names both types in a label.
    assert_eq!(
        label_texts(
            "fn main():\n    let n: Int = \"a\"\n    print(n)\n",
            "K0600"
        ),
        vec!["expected `Int`, found `String`".to_string()]
    );
    // `?` in a function that returns `()` is not K1302 (whose text is about a `Result`
    // function): K1301, with a label naming the return type.
    let src = "fn main():\n    let x = [1].get(0)?\n    print(x)\n";
    assert_eq!(codes(src), vec![("K1301", 2)]);
    let labels = label_texts(src, "K1301");
    assert!(
        labels[0].contains("this function returns `()`"),
        "{labels:?}"
    );
}
