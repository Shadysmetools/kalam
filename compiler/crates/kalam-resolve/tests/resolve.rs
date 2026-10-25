//! Name resolution tests: single files (`resolve_file`, what `kalam check FILE` runs) and
//! multi-module programs (`resolve_program`).

use kalam_ast::Ast;
use kalam_base::{FileId, Skin, Span};
use kalam_diagnostics::{Applicability, Diagnostic};
use kalam_resolve::{
    resolve_file, resolve_program, DefKind, ModulePath, Options, Program, Res, SourceModule,
};

fn parse(path: &str, src: &str, file: FileId) -> (Skin, Ast) {
    let skin = Skin::from_path(path).expect("a Kalam path");
    let out = kalam_parser::parse_source(src, skin, file);
    let errors: Vec<_> = out.diagnostics.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "{path} must parse: {errors:?}");
    (skin, out.ast)
}

fn check(path: &str, src: &str) -> (Program, Vec<Diagnostic>) {
    let (skin, ast) = parse(path, src, FileId(0));
    resolve_file(FileId(0), skin, &ast)
}

fn codes(path: &str, src: &str) -> Vec<&'static str> {
    check(path, src).1.iter().map(|d| d.code).collect()
}

/// 1-based line of a diagnostic.
fn line(src: &str, d: &Diagnostic) -> usize {
    src[..d.span.start as usize].matches('\n').count() + 1
}

fn clean(path: &str, src: &str) {
    let (_, diags) = check(path, src);
    assert!(diags.is_empty(), "{path}: {diags:#?}");
}

// ---- eval/suites [M2] cases (written from the spec, not read from the suite files) ----

#[test]
fn post_fold_collisions_are_sec0104() {
    let pairs = [
        ("على", "علي"),
        ("كتاب", "کتاب"),
        ("كرسي", "کرسی"),
        ("مدرسة", "مدرسه"),
        ("مدرسه", "مدرسە"),
        ("أحمد", "احمد"),
    ];
    for (a, b) in pairs {
        let src = format!(
            "دالة رئيسي():\n    ليكن {a} = 1\n    ليكن {b} = 2\n    اطبع(\"{{{a}}} {{{b}}}\")\n"
        );
        let (_, diags) = check("case.ar.kl", &src);
        let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
        assert_eq!(codes, vec!["SEC0104"], "{a} / {b}: {diags:#?}");
        // Reported once, at the later declaration, with both spellings and code points.
        assert_eq!(line(&src, &diags[0]), 3);
        let args: Vec<_> = diags[0].args.iter().map(|(_, v)| v.as_str()).collect();
        assert!(args[0].contains(b) && args[0].contains("U+"), "{args:?}");
        assert!(args[1].contains(a), "{args:?}");
        assert_eq!(diags[0].labels.len(), 1, "a label at the first declaration");
    }
    for (a, b) in [("al", "a1"), ("ax", "\u{0430}\u{0445}")] {
        let src = format!(
            "fn main():\n    let {a} = 1\n    let {b} = 2\n    print(\"{{{a}}} {{{b}}}\")\n"
        );
        assert_eq!(codes("case.kl", &src), vec!["SEC0104"], "{a} / {b}");
    }
}

#[test]
fn t50_homoglyph_in_all_skins() {
    let en = "fn main():\n    let pay = 100\n    let \u{0440}\u{0430}\u{0443} = 0\n    let على = 1\n    let علي = 2\n    print(\"{pay} {\u{0440}\u{0430}\u{0443}} {على} {علي}\")\n";
    assert_eq!(codes("t50.kl", en), vec!["SEC0104", "SEC0104"]);
    let franco = en
        .replace("fn main", "dala ra2isi")
        .replace("let ", "khali ")
        .replace("print", "ektb");
    assert_eq!(codes("t50.franco.kl", &franco), vec!["SEC0104", "SEC0104"]);
}

#[test]
fn persian_spelled_use_is_k0401_with_a_machine_fix() {
    // FOUNDER-QUEUE Q7: the use is never folded; the fix writes the declared spelling.
    let src = "دالة رئيسي():\n    ليكن كتاب = 1\n    اطبع(\"{کتاب}\")\n";
    let (_, diags) = check("case.ar.kl", src);
    assert_eq!(diags.len(), 1, "{diags:#?}");
    let d = &diags[0];
    assert_eq!((d.code, line(src, d)), ("K0401", 3));
    assert_eq!(d.fixes[0].applicability, Applicability::Machine);
    assert_eq!(d.fixes[0].edits[0].insert, "كتاب");
}

#[test]
fn glued_waw_is_k0401_with_the_split_suggestion() {
    let src = "دالة رئيسي():\n    ليكن م = صح\n    ليكن ن = وصح\n    اطبع(\"{م} {ن}\")\n";
    let (_, diags) = check("case.ar.kl", src);
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!((diags[0].code, line(src, &diags[0])), ("K0401", 3));
    assert_eq!(diags[0].fixes[0].edits[0].insert, "و صح");
}

// ---- clean programs ----

#[test]
fn ordinary_programs_resolve_cleanly() {
    clean(
        "shapes.kl",
        "enum Shape:\n    Rect(width: Int, height: Int)\n    Square(side: Int)\n    Empty\n\n\
         type Item:\n    label: String\n    shape: Shape\n    count: Int = 1\n\n\
         fn area(shape: Shape) -> Int:\n    return match shape:\n        case .Rect(w, h): w * h\n        case .Square(s): s * s\n        case .Empty: 0\n\n\
         fn describe(shape: Shape) -> String:\n    return match shape:\n        case .Rect(w, h) if w == h: \"square\"\n        case .Rect(_, _) | .Square(_): \"four\"\n        case .Empty: \"none\"\n\n\
         fn main():\n    let items = [Item(label: \"door\", shape: .Rect(width: 1, height: 2))]\n    var total = 0\n    for item in items:\n        total += area(item.shape) * item.count\n        print(\"{item.label}: {describe(item.shape)}\")\n    let bigger = items[0].with(count: 2)\n    let names = items.map(fn(it) => it.label)\n    print(\"{total} {bigger.count} {names}\")\n",
    );
    // Opaque std imports, capabilities, prelude types and `Type.assoc` calls.
    clean(
        "app.kl",
        "use std/http.{self, Request, Response, HttpError}\nuse std/json\n\n\
         type App:\n    store: Cell[Int]\n\n\
         fn show(req: Request[App]) -> Result[Response, HttpError]:\n    let id = req.path_int(\"id\")?\n    return Ok(Response.json(id))\n\n\
         fn main(world: World) -> Result[(), AnyError]:\n    let app = App(store: Cell.new(0))\n    http.serve(world.net, port: 8080, router: http.router().get(\"/\", show, access: .Public), app: app)?\n    log.info(json.encode(1))\n    return Ok(())\n",
    );
    // Recursion, local fns, generics, traits and impls.
    clean(
        "misc.kl",
        "trait Describe:\n    fn describe(self) -> String\n\n\
         type Money:\n    amount: Int\n\n\
         impl Money:\n    fn zero() -> Money => Money(amount: 0)\n\n\
         impl Money as Describe:\n    fn describe(self) -> String => \"{self.amount}\"\n\n\
         fn first[T](xs: List[T]) -> Option[T]:\n    return xs.get(0)\n\n\
         fn fact(n: Int) -> Int:\n    fn step(k: Int) -> Int:\n        return if k <= 1: 1 else: k * step(k - 1)\n    return step(n)\n\n\
         fn main():\n    let m = Money.zero()\n    print(m.describe())\n    print(\"{fact(5)} {first([1])}\")\n",
    );
    // The same names in sibling scopes are fine.
    clean(
        "siblings.kl",
        "fn main():\n    for x in [1]:\n        print(\"{x}\")\n    for x in [2]:\n        print(\"{x}\")\n    if true:\n        let y = 1\n        print(\"{y}\")\n    else:\n        let y = 2\n        print(\"{y}\")\n",
    );
}

#[test]
fn every_skin_resolves_the_same_program() {
    clean("hello.ar.kl", "دالة رئيسي():\n    اطبع(\"x\")\n");
    clean("hello.franco.kl", "dala ra2isi():\n    ektb(\"x\")\n");
    // Arabic convention: the value takes the article, the type does not (spec §13.3).
    clean("w.ar.kl", "دالة رئيسي(العالم: عالم):\n    اطبع(\"x\")\n");
}

// ---- K0405, K0413 ----

#[test]
fn shadowing_is_k0405() {
    assert_eq!(
        codes(
            "a.kl",
            "fn f(x: Int) -> Int:\n    let x = 2\n    return x\n"
        ),
        vec!["K0405"]
    );
    assert_eq!(
        codes(
            "a.kl",
            "fn f() -> Int:\n    let y = 1\n    if true:\n        let y = 2\n    return y\n"
        ),
        vec!["K0405"]
    );
    assert_eq!(
        codes(
            "a.kl",
            "fn area() -> Int => 1\nfn main():\n    let area = 2\n    print(\"{area}\")\n"
        ),
        vec!["K0405"]
    );
    // Two items with one name (one namespace, DR-11), and an item named like an import.
    assert_eq!(
        codes("a.kl", "type Shape:\n    a: Int\nfn Shape() -> Int => 1\n"),
        vec!["K0405"]
    );
    assert_eq!(
        codes("a.kl", "use std/http.{Router}\ntype Router:\n    a: Int\n"),
        vec!["K0405"]
    );
    // A prelude name is visible everywhere.
    assert_eq!(
        codes(
            "a.kl",
            "fn main():\n    let json = 1\n    print(\"{json}\")\n"
        ),
        vec!["K0405"]
    );
    // Variants of one enum and fields of one record.
    assert_eq!(codes("a.kl", "enum E:\n    A\n    A\n"), vec!["K0405"]);
    // A label points at the first declaration.
    let (_, d) = check("a.kl", "fn f(x: Int):\n    let x = 2\n");
    assert_eq!(d[0].labels.len(), 1);
}

#[test]
fn importing_the_same_module_twice_is_not_shadowing() {
    clean("a.kl", "use std/http\nuse std/http.{self, Router}\nuse std/json\nfn main():\n    let r: Router = http.router()\n    print(json.encode(1))\n");
    // `use std/task.{Cell}` repeats a prelude name on purpose.
    clean(
        "a.kl",
        "use std/task.{Cell}\nfn main():\n    let c = Cell.new(1)\n",
    );
}

#[test]
fn core_names_cannot_be_declared() {
    assert_eq!(codes("a.kl", "fn print() -> Int => 1\n"), vec!["K0413"]);
    assert_eq!(codes("a.ar.kl", "دالة اطبع() -> عدد => 1\n"), vec!["K0413"]);
    // A backticked word is an identifier, not the core name: the same program converted to
    // Arabic has `print` without backticks, and both skins must agree (P1).
    clean(
        "a.kl",
        "fn main():\n    let `print` = 1\n    print(\"{`print`}\")\n",
    );
    clean(
        "a.ar.kl",
        "دالة رئيسي():\n    ليكن print = 1\n    اطبع(\"{print}\")\n",
    );
    // The other skins' spellings are ordinary names in an English file (spec §5.4).
    clean(
        "a.kl",
        "fn main():\n    let `law` = 1\n    print(\"{`law`}\")\n",
    );
    // A lookalike of a core name collides in every skin (`lnt` / `Int`).
    assert_eq!(codes("a.kl", "type lnt = String\n"), vec!["SEC0104"]);
    assert_eq!(codes("a.ar.kl", "هيكل lnt = نص\n"), vec!["SEC0104"]);
}

#[test]
fn opaque_is_for_types_and_with_is_reserved() {
    assert_eq!(
        codes(
            "a.kl",
            "export opaque fn answer() -> Int => 42
"
        ),
        vec!["K0302"]
    );
    clean(
        "a.kl",
        "export opaque type Email:
    value: String
",
    );
    clean(
        "a.kl",
        "export opaque enum E:
    A
",
    );
    let src = "type Box:
    with: Int
";
    let (_, d) = check("a.kl", src);
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].code, line(src, &d[0])), ("K0417", 2));
    // `with` as a variant label or a local is fine.
    clean(
        "a.kl",
        "enum E:
    A(with: Int)
fn main():
    let with = 1
    print(\"{with}\")
",
    );
}

// ---- K0401, K0201 ----

#[test]
fn undefined_names_get_suggestions() {
    let (_, d) = check(
        "a.kl",
        "fn main():\n    let total = 1\n    print(\"{totl}\")\n",
    );
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].code, "K0401");
    assert_eq!(d[0].args[0].1, "`totl`");
    assert_eq!(d[0].fixes[0].applicability, Applicability::Suggestion);
    assert_eq!(d[0].fixes[0].edits[0].insert, "total");
    // Undefined types and calls too; locals are visible only after their declaration.
    assert_eq!(codes("a.kl", "fn f(x: Strin) -> Int => 1\n"), vec!["K0401"]);
    assert_eq!(
        codes("a.kl", "fn main():\n    print(\"{y}\")\n    let y = 1\n"),
        vec!["K0401"]
    );
    // A use refers to the declaration's symbol.
    let src = "fn main():\n    let y = 1\n    print(\"{y}\")\n";
    let (p, d) = check("a.kl", src);
    assert!(d.is_empty());
    let decl = src.find("y =").unwrap() as u32;
    let use_ = src.rfind('y').unwrap() as u32;
    let def = p.decl_at(FileId(0), Span::new(decl, decl + 1)).unwrap();
    assert_eq!(
        p.use_at(FileId(0), Span::new(use_, use_ + 1)),
        Some(Res::Def(def))
    );
    assert_eq!(p.def(def).kind, DefKind::Local);
}

#[test]
fn another_skins_word_in_a_hole_is_k0201() {
    // The lexer cannot see declarations from inside a hole (FOUNDER-QUEUE Q9); resolve can.
    let (_, d) = check("a.ar.kl", "دالة رئيسي():\n    اطبع(\"{print}\")\n");
    assert_eq!(d.len(), 1, "{d:#?}");
    assert_eq!(d[0].code, "K0201");
    assert_eq!(d[0].fixes[0].edits[0].insert, "اطبع");
    // Declared, it is an ordinary name (spec §5.4 bullet 2).
    clean(
        "a.ar.kl",
        "دالة رئيسي():\n    ليكن print = 1\n    اطبع(\"{print}\")\n",
    );
}

// ---- multi-module programs ----

struct Files(Vec<(String, String)>);

impl Files {
    fn resolve(&self, opts: &Options) -> (Program, Vec<(String, Diagnostic)>) {
        let parsed: Vec<(Skin, Ast)> = self
            .0
            .iter()
            .enumerate()
            .map(|(i, (path, src))| parse(path, src, FileId(i as u32)))
            .collect();
        let sources: Vec<SourceModule<'_>> = self
            .0
            .iter()
            .zip(&parsed)
            .enumerate()
            .map(|(i, ((path, _), (skin, ast)))| {
                let std = path.starts_with("std/");
                let module = if std {
                    ModulePath::from_file("std", path.strip_prefix("std/").unwrap())
                } else {
                    ModulePath::from_file(&opts.package, path)
                };
                SourceModule {
                    path: module,
                    file: FileId(i as u32),
                    skin: *skin,
                    ast,
                    std,
                }
            })
            .collect();
        let (p, d) = resolve_program(&sources, opts);
        let named = d
            .into_iter()
            .map(|d| (self.0[d.file.0 as usize].0.clone(), d))
            .collect();
        (p, named)
    }

    fn codes(&self, opts: &Options) -> Vec<(String, &'static str)> {
        self.resolve(opts)
            .1
            .into_iter()
            .map(|(f, d)| (f, d.code))
            .collect()
    }
}

fn files(list: &[(&str, &str)]) -> Files {
    Files(
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
    )
}

fn whole(std_loaded: bool) -> Options {
    Options {
        package: "app".into(),
        whole_package: true,
        std_loaded,
    }
}

#[test]
fn imports_reach_exported_items_only() {
    let fs = files(&[
        (
            "src/shapes.kl",
            "export enum Shape:\n    Circle(radius: Float)\n    Empty\n\nexport fn area(s: Shape) -> Float => 0.0\n\nfn helper() -> Int => 1\n",
        ),
        (
            "src/main.ar.kl",
            "استخدم app/shapes.{Shape, area}\nاستخدم app/shapes\n\nدالة رئيسي():\n    ليكن ش = Shape.Circle(radius: 1.0)\n    اطبع(\"{area(ش)} {shapes.area(.Empty)}\")\n",
        ),
    ]);
    let (p, d) = fs.resolve(&whole(false));
    assert!(d.is_empty(), "{d:#?}");
    let shapes = p.module_by_path("app/shapes").unwrap();
    let area = shapes.exports["area"];
    assert_eq!(p.def(area).kind, DefKind::Fn);
    assert!(!shapes.exports.contains_key("helper"));
    let shape = shapes.exports["Shape"];
    let variants: Vec<_> = p
        .variants_of(shape)
        .iter()
        .map(|&v| p.def(v).name.clone())
        .collect();
    assert_eq!(variants, vec!["Circle", "Empty"]);
    // `.Empty` has its candidates; `Shape.Circle` resolves to the variant.
    let main = &fs.0[1].1;
    let empty = main.find("Empty").unwrap() as u32;
    let c = p
        .implicit_candidates(FileId(1), Span::new(empty, empty + 5))
        .unwrap();
    assert_eq!(c.len(), 1);
    let circle = main.find("Circle").unwrap() as u32;
    let r = p.use_at(FileId(1), Span::new(circle, circle + 6)).unwrap();
    assert_eq!(r, Res::Def(p.variants_of(shape)[0]));

    let private = files(&[
        ("src/a.kl", "fn hidden() -> Int => 1\n"),
        (
            "src/b.kl",
            "use app/a.{hidden}\nfn main():\n    print(\"{hidden()}\")\n",
        ),
    ]);
    assert_eq!(
        private.codes(&whole(false)),
        vec![("src/b.kl".to_string(), "K0401")]
    );
}

#[test]
fn missing_modules_cycles_and_duplicate_skins() {
    let missing = files(&[(
        "src/main.kl",
        "use app/nope\nuse std/http\nfn main():\n    pass\n",
    )]);
    assert_eq!(
        missing.codes(&whole(false)),
        vec![("src/main.kl".to_string(), "K0401")]
    );
    // With the stdlib loaded, a missing std module is an error too.
    assert_eq!(missing.codes(&whole(true)).len(), 2);
    // A single-file check never complains about modules it cannot see.
    let single = Options {
        package: "app".into(),
        ..Options::default()
    };
    assert!(missing.codes(&single).is_empty());

    let cycle = files(&[
        ("src/a.kl", "use app/b\nexport fn f() -> Int => 1\n"),
        ("src/b.kl", "use app/a\nexport fn g() -> Int => 1\n"),
    ]);
    let (_, d) = cycle.resolve(&whole(false));
    assert_eq!(d.len(), 1, "{d:#?}");
    assert_eq!(d[0].1.code, "K1201");
    assert_eq!(d[0].1.args[0].1, "app/a → app/b → app/a");

    let dup = files(&[
        ("src/users.kl", "export fn f() -> Int => 1\n"),
        ("src/users.ar.kl", "علني دالة ف() -> عدد => 1\n"),
    ]);
    let codes = dup.codes(&whole(false));
    assert_eq!(codes.len(), 1);
    assert_eq!(codes[0].1, "K0301");
}

#[test]
fn bare_variant_from_another_module_is_k0917() {
    let fs = files(&[
        (
            "src/shapes.kl",
            "export enum Shape:\n    Circle(radius: Float)\n    Empty\n",
        ),
        (
            "src/main.kl",
            "use app/shapes.{Shape}\nfn main():\n    let s: Shape = Circle(radius: 1.0)\n",
        ),
    ]);
    let (_, d) = fs.resolve(&whole(false));
    assert_eq!(d.len(), 1, "{d:#?}");
    assert_eq!(d[0].1.code, "K0917");
    assert_eq!(d[0].1.fixes[0].edits[0].insert, ".");
}

#[test]
fn exported_signatures_cannot_name_private_types() {
    let fs = files(&[(
        "src/a.kl",
        "type Secretish:\n    a: Int\n\nexport fn leak() -> Secretish => Secretish(a: 1)\n\nexport type Box:\n    inner: Secretish\n\nexport opaque type Fine:\n    inner: Secretish\n",
    )]);
    let codes = fs.codes(&whole(false));
    assert_eq!(
        codes.iter().filter(|(_, c)| *c == "K0303").count(),
        2,
        "{codes:?}"
    );
}

#[test]
fn re_exports_and_stdlib_modules() {
    let fs = files(&[
        ("std/http.kl", "export type Router:\n    a: Int\n\nexport enum Access:\n    Public\n    Private\n\nexport fn router() -> Router => Router(a: 1)\n"),
        ("std/log.kl", "export fn info(s: String):\n    pass\n"),
        ("std/json.kl", "export fn encode(x: Int) -> String => \"\"\n"),
        ("src/web.kl", "export use std/http.{Router}\n"),
        ("src/main.kl", "use app/web.{Router}\nuse std/http\nfn main():\n    let r: Router = http.router()\n    let a: http.Access = .Public\n    log.info(json.encode(1))\n"),
    ]);
    let (p, d) = fs.resolve(&whole(true));
    assert!(d.is_empty(), "{d:#?}");
    assert!(p.module_by_path("std/http").unwrap().loaded);
    // With the whole program known, an implicit member nobody declares is K0401.
    let bad = files(&[
        ("std/log.kl", "export fn info(s: String):\n    pass\n"),
        (
            "std/json.kl",
            "export fn encode(x: Int) -> String => \"\"\n",
        ),
        (
            "src/main.kl",
            "enum E:\n    Yes\nfn main():\n    let e: E = .Yse\n",
        ),
    ]);
    let (_, d) = bad.resolve(&whole(true));
    assert_eq!(d.len(), 1, "{d:#?}");
    assert_eq!(d[0].1.code, "K0401");
    assert_eq!(d[0].1.fixes[0].edits[0].insert, "Yes");
    // And an unknown member of a loaded module.
    let member = files(&[
        ("std/log.kl", "export fn info(s: String):\n    pass\n"),
        (
            "std/json.kl",
            "export fn encode(x: Int) -> String => \"\"\n",
        ),
        ("src/main.kl", "fn main():\n    log.inf(\"x\")\n"),
    ]);
    let (_, d) = member.resolve(&whole(true));
    assert_eq!(d.len(), 1, "{d:#?}");
    assert_eq!(d[0].1.fixes[0].edits[0].insert, "info");
}

#[test]
fn std_prelude_is_the_prelude_when_loaded() {
    let fs = files(&[
        (
            "std/prelude.kl",
            "export use std/json
export use std/log

@builtin
export extern type Int

export enum Ordering:
    Less
    Equal
    Greater

export fn print(value: Int) => panic(\"builtin\")
",
        ),
        (
            "std/json.kl",
            "export fn encode(x: Int) -> String => \"\"
",
        ),
        (
            "std/log.kl",
            "export fn info(s: String):
    pass
",
        ),
        // A stdlib parameter may be named like a module item: it is an API label.
        (
            "std/http.kl",
            "export type Router:
    a: Int

export fn router() -> Router => Router(a: 1)

export fn serve(router: Router) -> Int => 1
",
        ),
        (
            "src/main.kl",
            "fn cmp(a: Int, b: Int) -> Ordering => .Less

fn main():
    log.info(json.encode(1))
",
        ),
    ]);
    let (p, d) = fs.resolve(&whole(true));
    assert!(d.is_empty(), "{d:#?}");
    let int = p
        .core_stub(kalam_skins::Concept::CoreInt)
        .expect("Int stub");
    assert_eq!(p.def(int).kind, DefKind::ExternType);
    let ordering = p.prelude("Ordering").expect("Ordering is a prelude name");
    assert_eq!(p.module_of(ordering).unwrap().path.as_str(), "std/prelude");
    // The same parameter in user code is still K0405 (spec §10.1, literal reading).
    let user = files(&[(
        "src/http.kl",
        "export fn router() -> Int => 1

export fn serve(router: Int) -> Int => router
",
    )]);
    assert_eq!(
        user.codes(&whole(false)),
        vec![("src/http.kl".to_string(), "K0405")]
    );
}

// ---- determinism and robustness ----

#[test]
fn symbol_ids_are_deterministic() {
    let src = "type A:\n    x: Int\nenum B:\n    C\nfn main():\n    let a = A(x: 1)\n    print(\"{a.x}\")\n";
    let (p1, _) = check("a.kl", src);
    let (p2, _) = check("a.kl", src);
    let names = |p: &Program| {
        p.defs
            .iter()
            .map(|d| (d.name.clone(), d.kind))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&p1), names(&p2));
    assert_eq!(p1.uses, p2.uses);
    assert_eq!(p1.decls, p2.decls);
}

#[test]
fn deep_expression_chains_do_not_overflow_the_stack() {
    let mut expr = String::from("x");
    for _ in 0..1000 {
        expr.push_str(" + x");
    }
    let src = format!("fn main():\n    let x = 1\n    let y = {expr}\n    print(\"{{y}}\")\n");
    let handle = std::thread::Builder::new()
        .stack_size(1 << 20) // the Windows main-thread default
        .spawn(move || codes("deep.kl", &src))
        .unwrap();
    assert!(handle.join().unwrap().is_empty());
}
