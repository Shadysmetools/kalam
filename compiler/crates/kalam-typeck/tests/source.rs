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
