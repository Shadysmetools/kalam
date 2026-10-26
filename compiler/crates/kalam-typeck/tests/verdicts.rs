//! Regression tests for the wrong verdicts the M2 iteration-3 verifier found (its W1–W8 and
//! open item 1), and for the FOUNDER-QUEUE decisions Q33–Q37 behind them.
//!
//! Every program is checked the two ways `kalam check` checks a file:
//! - **single-file** (`kalam check FILE`, what conformance and `kalam run --interp` use):
//!   the file is resolved alone first, then again together with the stdlib signature stubs
//!   (default options: nothing else is loaded), and type-checked without the stubs' bodies;
//! - **project** (`kalam check DIR`): the file is `src/main.kl` of a package, resolved with
//!   the stdlib directory loaded (FOUNDER-QUEUE Q34: both modes give the same verdict).
//!
//! A clean program must also leave no type open (no inference variable and no poison in any
//! recorded type of the file), because those types become KIR, which the reference
//! interpreter runs (spec §9.10, ROADMAP M2 "well-typed programs don't go wrong").

use kalam_base::{FileId, Skin};
use kalam_diagnostics::{Applicability, Diagnostic, Fix};
use kalam_resolve::{resolve_file, resolve_program, ModulePath, Options, SourceModule};
use kalam_typeck::{Checked, Ty};

/// The stdlib stubs: `(file name, text)`, sorted.
fn stdlib() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../stdlib/src");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("stdlib/src") {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "kl") {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            out.push((name, std::fs::read_to_string(&p).unwrap()));
        }
    }
    out.sort();
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Single,
    Project,
}

/// What checking `main.kl` gave: the findings in the file (every stage) and, when the type
/// checker ran, its output.
struct Outcome {
    diags: Vec<Diagnostic>,
    checked: Option<Checked>,
}

fn run(mode: Mode, src: &str) -> Outcome {
    let std = stdlib();
    let main = kalam_parser::parse_source(src, Skin::En, FileId(0));
    let mut diags: Vec<Diagnostic> = main
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .cloned()
        .collect();
    if !diags.is_empty() {
        return Outcome {
            diags,
            checked: None,
        };
    }
    let std_parsed: Vec<_> = std
        .iter()
        .enumerate()
        .map(|(i, (_, text))| kalam_parser::parse_source(text, Skin::En, FileId(1 + i as u32)))
        .collect();
    let (path, options) = match mode {
        Mode::Single => {
            // First the file alone (every import opaque), as `kalam check FILE` does.
            let (_, alone) = resolve_file(FileId(0), Skin::En, &main.ast);
            diags.extend(alone.into_iter().filter(|d| d.is_error()));
            if !diags.is_empty() {
                return Outcome {
                    diags,
                    checked: None,
                };
            }
            (ModulePath("main".into()), Options::default())
        }
        Mode::Project => (
            ModulePath::from_file("app", "src/main.kl"),
            Options {
                package: "app".into(),
                whole_package: true,
                std_loaded: true,
            },
        ),
    };
    let mut sources = vec![SourceModule {
        path,
        file: FileId(0),
        skin: Skin::En,
        ast: &main.ast,
        std: false,
    }];
    for (i, ((name, _), parsed)) in std.iter().zip(&std_parsed).enumerate() {
        sources.push(SourceModule {
            path: ModulePath::from_file("std", &format!("src/{name}")),
            file: FileId(1 + i as u32),
            skin: Skin::En,
            ast: &parsed.ast,
            std: true,
        });
    }
    let (program, resolved) = resolve_program(&sources, &options);
    let resolved: Vec<Diagnostic> = resolved.into_iter().filter(|d| d.is_error()).collect();
    if !resolved.is_empty() {
        if mode == Mode::Single {
            // The driver skips the type check rather than guess (kalam-driver crate docs).
            return Outcome {
                diags,
                checked: None,
            };
        }
        return Outcome {
            diags: resolved,
            checked: None,
        };
    }
    let checked = match mode {
        Mode::Single => {
            let hir = kalam_typeck::lower::lower_program_with(&program, &sources, false);
            let results = kalam_typeck::check_program(&hir);
            Checked { hir, results }
        }
        Mode::Project => kalam_typeck::check_resolved(&program, &sources),
    };
    let found = checked.diagnostics();
    let stray: Vec<_> = found.iter().filter(|d| d.file != FileId(0)).collect();
    assert!(stray.is_empty(), "findings in the stdlib: {stray:#?}");
    diags.extend(found);
    Outcome {
        diags,
        checked: Some(checked),
    }
}

fn line_of(src: &str, at: u32) -> usize {
    src[..(at as usize).min(src.len())].matches('\n').count() + 1
}

fn codes_in(mode: Mode, src: &str) -> Vec<(&'static str, usize)> {
    run(mode, src)
        .diags
        .iter()
        .map(|d| (d.code, line_of(src, d.span.start)))
        .collect()
}

/// `(code, line)` of every finding, the same in both modes (Q34).
fn codes(src: &str) -> Vec<(&'static str, usize)> {
    let single = codes_in(Mode::Single, src);
    let project = codes_in(Mode::Project, src);
    assert_eq!(
        single, project,
        "single-file and project modes disagree on\n{src}"
    );
    single
}

/// The first finding, as single-file mode reports it (both modes agree, see [`codes`]).
fn first(src: &str) -> Diagnostic {
    codes(src);
    run(Mode::Single, src)
        .diags
        .into_iter()
        .next()
        .expect("a finding")
}

/// Every type the checker recorded for the file's own bodies is closed: no inference
/// variable and no poison is left for lowering to put into KIR.
fn assert_closed(checked: &Checked, src: &str) {
    let in_file = |owner| checked.hir.item(owner).is_some_and(|i| i.file == FileId(0));
    let open = |t: &Ty| t.any(&mut |t| matches!(t, Ty::Var(_) | Ty::Error));
    let r = &checked.results;
    for ((owner, e), t) in &r.expr_types {
        assert!(!(in_file(*owner) && open(t)), "expr {e:?}: {t}\n{src}");
    }
    for ((owner, l), t) in &r.local_types {
        assert!(!(in_file(*owner) && open(t)), "local {l:?}: {t}\n{src}");
    }
    for ((owner, e), ts) in &r.instantiations {
        for t in ts {
            assert!(
                !(in_file(*owner) && open(t)),
                "type argument at {e:?}: {t}\n{src}"
            );
        }
    }
    for (c, t) in &r.consts {
        assert!(!(in_file(*c) && open(t)), "constant {c:?}: {t}\n{src}");
    }
}

/// Clean in both modes, with every type closed.
fn clean(src: &str) {
    for mode in [Mode::Single, Mode::Project] {
        let out = run(mode, src);
        let found: Vec<_> = out
            .diags
            .iter()
            .map(|d| (d.code, line_of(src, d.span.start)))
            .collect();
        assert!(found.is_empty(), "{mode:?}: {found:?}\n{src}");
        assert_closed(out.checked.as_ref().expect("checked"), src);
    }
}

/// Applies every edit of `fix` to `src`.
fn apply(src: &str, fix: &Fix) -> String {
    let mut out = src.to_string();
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|e| std::cmp::Reverse((e.span.start, e.span.end)));
    for e in edits {
        out.replace_range(e.span.start as usize..e.span.end as usize, &e.insert);
    }
    out
}

/// The type the checker gave the `let`-bound local `name` in the file.
fn local_type(src: &str, name: &str) -> Ty {
    let out = run(Mode::Single, src);
    let checked = out.checked.expect("checked");
    let mut found = None;
    for item in checked.hir.items() {
        if item.file != FileId(0) {
            continue;
        }
        if let kalam_typeck::hir::ItemKind::Fn(f) = &item.kind {
            if let Some(body) = &f.body {
                for (i, l) in body.locals.iter().enumerate() {
                    if l.name == name {
                        let id = kalam_typeck::hir::LocalId(i as u32);
                        found = checked.results.local_types.get(&(item.def, id)).cloned();
                    }
                }
            }
        }
    }
    found.unwrap_or_else(|| panic!("no local `{name}` in\n{src}"))
}

// ---- (1) §9.9: a type argument that nothing fixes (open item 1, r15, conformance 13.1.3) --

#[test]
fn a_type_argument_nothing_fixes_is_never_and_no_type_stays_open() {
    for src in [
        // The verifier's r15 and the frozen conformance cases 13.1.3 module-import-binds-
        // module and renamed-module: accepted (must_reject: false), with every type closed.
        "fn main():\n    let b = Ok(1).is_ok()\n    print(\"{b}\")\n",
        "use std/http\n\nfn main():\n    let r = http.router()\n    print(\"x\")\n",
        "use std/http as web\n\nfn main():\n    let r = web.router()\n    print(\"x\")\n",
        "fn main():\n    let n = None\n    print(\"{n.is_none()}\")\n",
        "fn main():\n    let xs = [None, None]\n    print(\"{xs.len()}\")\n",
        "fn conjure[T]() -> T => panic(\"never\")\n\nfn main():\n    let x = conjure()\n    print(\"done\")\n",
        "fn main():\n    let pick = fn(o: Option[Int]) => o.is_some()\n    print(\"{pick(None)}\")\n",
    ] {
        clean(src);
    }
    // The open argument is `Never`: no value of it can exist, so the choice is invisible.
    assert_eq!(
        local_type(
            "fn main():\n    let b = Ok(1)\n    print(\"{b.is_ok()}\")\n",
            "b"
        ),
        Ty::Result(Box::new(Ty::Int), Box::new(Ty::Never))
    );
    // Empty collections keep their own rule (§9.9 rule 4).
    assert_eq!(
        codes("fn main():\n    let xs = []\n    print(\"{xs.len()}\")\n"),
        vec![("K0802", 2)]
    );
}

#[test]
fn a_type_argument_a_requirement_depends_on_is_k0804() {
    // §9.9 rule 5: `json.decode` takes its type from the expected type. With none, the
    // `Decode` the call needs depends on a type nothing fixes: guessing would change what
    // the program does.
    assert_eq!(
        codes("fn main():\n    let x = json.decode(\"1\")\n    print(\"x\")\n"),
        vec![("K0804", 2)]
    );
    // `Show` for `Option[_]` and `Eq` for `Option[_]` depend on the element type.
    assert_eq!(
        codes("fn main():\n    print(\"{None}\")\n"),
        vec![("K0804", 2)]
    );
    assert_eq!(
        codes("fn main():\n    let same = None == None\n    print(\"{same}\")\n"),
        vec![("K0804", 2)]
    );
    // An annotation fixes it.
    clean("fn main():\n    let x: Result[Int, json.DecodeError] = json.decode(\"1\")\n    print(\"x\")\n");
    clean("fn main():\n    let n: Option[Int] = None\n    print(\"{n}\")\n");
    // The message shows the open part as `_`; a label names the trait that depends on it.
    let d = first("fn main():\n    let x = json.decode(\"1\")\n    print(\"x\")\n");
    let arg = |k: &str| d.args.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
    assert_eq!(arg("type").as_deref(), Some("_"));
    assert!(
        d.labels.iter().any(|l| l.text.contains("`Decode`")),
        "{:?}",
        d.labels
    );
    // One report for one open type, however many requirements wait on it.
    assert_eq!(
        codes("fn main():\n    let n = None\n    print(\"{n}\")\n    print(\"{n}\")\n"),
        vec![("K0804", 3)]
    );
    // A requirement that fails whatever the open part is stays the trait error: a function
    // is never `Show` (§9.3).
    assert_eq!(
        codes("fn main():\n    let f = fn(x: Int) => None\n    print(\"{f}\")\n"),
        vec![("K0603", 3)]
    );
    // An earlier error in the body is the root cause: nothing more is said.
    assert_eq!(
        codes("fn main():\n    let y: Int = \"a\"\n    print(\"{None}\")\n"),
        vec![("K0600", 2)]
    );
}

#[test]
fn a_module_constant_takes_its_whole_type_from_its_value() {
    // §9.9 rule 1. Other bodies use a constant, so a part of its type that its value leaves
    // open cannot be defaulted the way a local's is: `Never` would surface at every use
    // (`expected Option[Int], found Option[Never]`). It is K0804 at the constant, once.
    for src in [
        "let NONE = None\n\nfn main():\n    let x: Option[Int] = NONE\n    print(\"{x.is_none()}\")\n",
        "let OK = Ok(1)\n\nfn f() -> Result[Int, String] => OK\n\nfn main():\n    print(\"{f().is_ok()}\")\n",
        "let NONE = None\n\nfn main():\n    print(\"{NONE.is_none()}\")\n",
        "let X = json.decode(\"1\")\n\nfn main():\n    print(\"x\")\n",
    ] {
        assert_eq!(codes(src), vec![("K0804", 1)], "{src}");
    }
    let d = first("let NONE = None\n\nfn main():\n    print(\"{NONE.is_none()}\")\n");
    let arg = |k: &str| d.args.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
    assert_eq!(arg("type").as_deref(), Some("Option[_]"));
    // Written on the constant, the type is closed everywhere.
    clean("let NONE: Option[Int] = None\nlet OK: Result[Int, String] = Ok(1)\n\nfn f() -> Result[Int, String] => OK\n\nfn main():\n    let x: Option[Int] = NONE\n    print(\"{x.is_none()} {f().is_ok()}\")\n");
    // An empty collection keeps its own code (§9.9 rule 4), reported once.
    assert_eq!(
        codes("let XS = []\n\nfn main():\n    print(\"{XS.len()}\")\n"),
        vec![("K0802", 1)]
    );
}

// ---- (2) Q33, W1, W8: module constants (§13.4, K1204) ---------------------------------------

#[test]
fn constant_initializers_are_constant_expressions() {
    clean(
        "type Point:\n    x: Int\n    y: Int = 0\n\nenum Shape:\n    Circle(r: Int)\n    Empty\n\nlet LIMIT = 3\nlet NAMES = [\"a\", \"b\"]\nlet ORIGIN = Point(x: 0)\nlet MOVED = ORIGIN.with(y: 2)\nlet SHAPE = Shape.Circle(r: LIMIT * 2)\nlet EMPTY: Shape = .Empty\nlet NONE: Option[Int] = None\nlet PAIR = (LIMIT, \"x\")\nlet TABLE = {\"a\": 1}\nlet MIN = -9223372036854775808\nlet BIG = 9223372036854775807 - LIMIT\nlet RATIO = 1.0 / 0.0\nlet HALF = 7 / 2\nlet MOD = -7 % 3\nlet ON = LIMIT > 2 and not false\nlet LABEL = \"n=\" + \"{LIMIT}\"\nlet DOUBLE = fn(x: Int) -> Int => x * 2\nlet OX = ORIGIN.x\nlet FIRST = PAIR.0\n\nfn main():\n    print(\"{LIMIT} {NAMES.len()} {MOVED.y} {MIN} {BIG} {HALF} {MOD} {ON} {LABEL} {DOUBLE(OX)} {FIRST}\")\n    print(\"{TABLE.len()} {RATIO}\")\n    match SHAPE:\n        case .Circle(r):\n            print(\"{r}\")\n        case .Empty:\n            print(\"empty\")\n    match EMPTY:\n        case .Circle(r):\n            print(\"{r}\")\n        case .Empty:\n            print(\"{NONE.is_none()}\")\n",
    );
    // W1 (e05): a method call is not a constant expression.
    assert_eq!(
        codes("let N = [1, 2, 3].len()\n\nfn main():\n    print(\"{N}\")\n"),
        vec![("K1204", 1)]
    );
    // A function call, `if`, `match`, indexing and `?` are not either.
    assert_eq!(
        codes("fn three() -> Int => 3\n\nlet N = three()\n\nfn main():\n    print(\"{N}\")\n"),
        vec![("K1204", 3)]
    );
    assert_eq!(
        codes("let N = if true: 1 else: 2\n\nfn main():\n    print(\"{N}\")\n"),
        vec![("K1204", 1)]
    );
    assert_eq!(
        codes("let XS = [1, 2]\nlet N = XS[0]\n\nfn main():\n    print(\"{N}\")\n"),
        vec![("K1204", 2)]
    );
    // The finding points at the part that is not constant.
    let d = first("let N = 1 + [1, 2, 3].len()\n\nfn main():\n    print(\"{N}\")\n");
    let src = "let N = 1 + [1, 2, 3].len()\n\nfn main():\n    print(\"{N}\")\n";
    assert_eq!(
        &src[d.span.start as usize..d.span.end as usize],
        "[1, 2, 3].len()"
    );
}

#[test]
fn a_constant_that_would_panic_is_k1204() {
    // W1 (e05, e06): §13.4 "a constant that would panic is a compile error".
    for (src, line) in [
        (
            "let BOOM = 1 // 0\n\nfn main():\n    print(\"before\")\n    print(\"{BOOM}\")\n",
            1,
        ),
        (
            "let N = 3\nlet BOOM = 1 // 0\n\nfn main():\n    print(\"before {N}\")\n",
            2,
        ),
        ("let R = 5 % 0\n\nfn main():\n    print(\"{R}\")\n", 1),
        // `/` on two `Int`s is a `Float`, and dividing by zero still panics (§9.2).
        ("let H = 1 / 0\n\nfn main():\n    print(\"{H}\")\n", 1),
        // D18: `Int` is checked at the 64-bit bounds.
        (
            "let BIG = 9223372036854775807 + 1\n\nfn main():\n    print(\"{BIG}\")\n",
            1,
        ),
        (
            "let M = -9223372036854775808 // -1\n\nfn main():\n    print(\"{M}\")\n",
            1,
        ),
        (
            "let N = -(-9223372036854775807 - 1)\n\nfn main():\n    print(\"{N}\")\n",
            1,
        ),
        (
            "let X = Some(2 * 4611686018427387904)\n\nfn main():\n    print(\"{X}\")\n",
            1,
        ),
        // Through other constants, in dependency order.
        (
            "let A = 9223372036854775807\nlet B = A + 1\n\nfn main():\n    print(\"{B}\")\n",
            2,
        ),
    ] {
        assert_eq!(codes(src), vec![("K1204", line)], "{src}");
    }
    // Reported once, at the constant that panics, not again at the ones that use it.
    assert_eq!(
        codes("let A = B + 1\nlet B = 1 // 0\n\nfn main():\n    print(\"before {A}\")\n"),
        vec![("K1204", 2)]
    );
    // Float division by zero is IEEE (no panic); `Int` arithmetic inside range is fine.
    clean("let INF = 1.0 / 0.0\nlet OK = 9223372036854775806 + 1\nlet Q = -7 // 2\n\nfn main():\n    print(\"{INF} {OK} {Q}\")\n");
}

#[test]
fn a_constant_that_depends_on_itself_is_k1204() {
    assert_eq!(
        codes("let A = A + 1\n\nfn main():\n    print(\"{A}\")\n"),
        vec![("K1204", 1)]
    );
    assert_eq!(
        codes("let A: Int = B + 1\nlet B: Int = A + 1\n\nfn main():\n    print(\"{A}\")\n"),
        vec![("K1204", 1)]
    );
}

#[test]
fn a_constant_cannot_hold_a_cell_or_a_capability() {
    // W1 (e03) and §15.6: shared state is a `Cell` made in `main`, never a constant.
    assert_eq!(
        codes("let COUNTER = Cell.new(0)\n\nfn main():\n    print(\"{COUNTER.get()}\")\n"),
        vec![("K1204", 1)]
    );
    // W8 (b19, e04): "it cannot take or produce a capability" (§13.4).
    assert_eq!(
        codes("let NOBODY: Option[Net] = None\n\nfn main():\n    print(\"x\")\n"),
        vec![("K1204", 1)]
    );
    assert_eq!(
        codes("let GRAB = fn(w: World) -> Net => w.net\n\nfn main(world: World):\n    let n = GRAB(world)\n    print(\"x\")\n"),
        vec![("K1204", 1)]
    );
    // A function constant that only uses ordinary values is fine.
    clean("let TWICE = fn(x: Int) -> Int => x * 2\n\nfn main():\n    print(\"{TWICE(2)}\")\n");
    // Constructing `World` is K1501, the root cause; no second report for the constant.
    assert_eq!(
        codes("let MAKE = fn() => World()\n\nfn main():\n    print(\"x\")\n"),
        vec![("K1501", 1)]
    );
}

#[test]
fn record_field_defaults_are_constant_expressions_too() {
    // §9.4: "field defaults must be constant expressions (§13.4)".
    assert_eq!(
        codes("type Job:\n    tries: Int = [1, 2].len()\n\nfn main():\n    print(\"{Job().tries}\")\n"),
        vec![("K1204", 2)]
    );
    assert_eq!(
        codes("type Job:\n    tries: Int = 1 // 0\n\nfn main():\n    print(\"{Job().tries}\")\n"),
        vec![("K1204", 2)]
    );
    // A record may hold a capability: only a module constant may not.
    clean("type Ctx:\n    net: Option[Net] = None\n    tries: Int = 3\n\nfn main():\n    print(\"{Ctx().tries}\")\n");
}

// ---- (3) Q37, W2: callers see the trait's signature ---------------------------------------

#[test]
fn callers_see_the_trait_signature_not_the_impls_extra_bound() {
    // The verifier's a02: K1107 at the impl, and nothing at the call, which the trait's
    // `pick[T]` (no bound) allows.
    assert_eq!(
        codes("trait Picker:\n    fn pick[T](self, xs: List[T]) -> Option[T]\n\ntype P:\n    n: Int\n\nimpl P as Picker:\n    fn pick[T: Ord](self, xs: List[T]) -> Option[T] => xs.get(0)\n\nfn main():\n    let p = P(n: 1)\n    let r = p.pick([fn() => 1])\n    print(\"done\")\n"),
        vec![("K1107", 8)]
    );
    // A different number of type parameters: callers still use the trait's.
    assert_eq!(
        codes("trait Tr:\n    fn m[A](self, a: A) -> A\n\ntype X:\n    n: Int\n\nimpl X as Tr:\n    fn m(self, a: Int) -> Int => a\n\nfn main():\n    let x = X(n: 1)\n    let s: String = x.m(\"s\")\n    print(s)\n"),
        vec![("K1107", 8)]
    );
    // `self` missing from the impl: callers still call it as the trait's method.
    assert_eq!(
        codes("trait Named:\n    fn name(self) -> String\n\ntype U:\n    id: Int\n\nimpl U as Named:\n    fn name() -> String => \"u\"\n\nfn main():\n    print(U(id: 1).name())\n"),
        vec![("K1107", 8)]
    );
}

// ---- (4) Q34, W3: the same prelude in both modes ------------------------------------------

#[test]
fn ordering_and_the_rest_of_the_prelude_resolve_in_both_modes() {
    // W3 (r24): a named `sort_by` comparator must be annotated (§9.9 rule 1) with `Ordering`.
    clean("fn rev(a: Float, b: Float) -> Ordering => b.total_cmp(a)\n\nfn main():\n    let ys = [3.0, 1.0].sort_by(rev)\n    print(\"{ys.len()}\")\n");
    clean("fn main():\n    let o = Ordering.Less\n    let p: Ordering = .Greater\n    let same = o == p\n    print(\"{same}\")\n");
    clean("fn f(e: UrlError) -> Int => 1\n\nfn g(e: RegexError) -> Int => 2\n\nfn main():\n    let c = Cell.new(1)\n    print(\"{c.get()}\")\n");
    // So a declaration reusing one is K0405 in both modes.
    assert_eq!(
        codes("type Ordering:\n    n: Int\n\nfn main():\n    print(\"x\")\n"),
        vec![("K0405", 1)]
    );
}

#[test]
fn an_implicit_member_no_enum_has_is_k0401_in_both_modes() {
    // Project mode: resolve knows the whole program and says K0401. Single-file mode used
    // to accept this program (the member became poison without a report).
    let src = "enum Color:\n    Red\n    Green\n\nfn main():\n    let c: Color = .Blue\n    print(\"x\")\n";
    assert_eq!(codes(src), vec![("K0401", 6)]);
    let pat = "enum Color:\n    Red\n    Green\n\nfn f(c: Color) -> Int:\n    match c:\n        case .Red:\n            return 1\n        case .Blue:\n            return 2\n        case _:\n            return 3\n\nfn main():\n    print(\"{f(.Red)}\")\n";
    assert_eq!(codes(pat), vec![("K0401", 9)]);
}

// ---- (5) Q35, W6: a function-typed field called with method syntax -----------------------

#[test]
fn a_field_called_like_a_method_is_k1108_with_a_parenthesizing_fix() {
    let src = "type Job:\n    run: fn(Int) -> Int\n\nfn main():\n    let job = Job(run: fn(x: Int) -> Int => x + 1)\n    let n: Int = job.run(1)\n    print(\"{n}\")\n";
    assert_eq!(codes(src), vec![("K1108", 6)]);
    let d = first(src);
    let arg = |k: &str| d.args.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
    assert_eq!(arg("name").as_deref(), Some("run"));
    assert_eq!(arg("type").as_deref(), Some("Job"));
    let fix = &d.fixes[0];
    assert_eq!(fix.applicability, Applicability::Machine);
    let fixed = apply(src, fix);
    assert!(fixed.contains("let n: Int = (job.run)(1)"), "{fixed}");
    clean(&fixed);
    // A field that holds no function stays the unknown-member error.
    assert_eq!(
        codes("type Job:\n    name: String\n\nfn main():\n    let job = Job(name: \"a\")\n    print(job.name())\n"),
        vec![("K0420", 6)]
    );
    // A real method of the type wins over a field (DR-27 looks at methods only).
    clean("type Job:\n    run: fn(Int) -> Int\n\nimpl Job:\n    fn run(self, x: Int) -> Int => x\n\nfn main():\n    let job = Job(run: fn(x: Int) -> Int => x)\n    print(\"{job.run(1)}\")\n");
}

// ---- W4: a pattern that does not fit the scrutinee ------------------------------------------

#[test]
fn exhaustiveness_waits_for_patterns_that_type_check() {
    // The verifier's r19: `Option` patterns over a `Result`. The pattern errors are the root
    // cause; K1001 (with a fix that would leave the program broken) is not reported.
    assert_eq!(
        codes("fn main():\n    match \"1\".to_int():\n        case Some(n):\n            print(\"{n}\")\n        case None:\n            print(\"none\")\n"),
        vec![("K0600", 3), ("K0600", 5)]
    );
}

#[test]
fn a_wrong_typed_pattern_hides_no_arm_behind_a_false_k1002() {
    // The verifier's r22: an arm whose pattern belongs to another enum is K0600, and the
    // `_` after it is no unreachable arm.
    let src = "enum A:\n    X(v: Int)\n    Y\n\nenum B:\n    X(v: String)\n    Z\n\nfn main():\n    let a: A = A.Y\n    match a:\n        case B.X(v):\n            print(v)\n        case _:\n            print(\"other\")\n";
    assert_eq!(codes(src), vec![("K0600", 12)]);
}

// ---- W5: one suspension flow per value passed to a `@sync` parameter --------------------

#[test]
fn a_suspending_result_passed_to_a_sync_parameter_is_one_flow_at_the_argument() {
    // The verifier's b02: `make` returns a function type, which is data (§15.3), so it may
    // suspend; passing it to `@sync` is K1401 (kalam-effects reads these flows). The hint
    // that types the call from its expected type no longer records a second flow with the
    // span of whatever was checked before it (it showed as an empty K1401 at 1:1).
    let src = "fn apply(f: @sync fn(Int) -> Int, x: Int) -> Int => f(x)\n\nfn make(fs: Fs) -> fn(Int) -> Int:\n    fn step(x: Int) -> Int:\n        if fs.exists(path\"flag.txt\"):\n            return x + 1\n        return x\n    return step\n\nfn main(world: World):\n    print(\"{apply(make(world.fs), 1)}\")\n";
    let checked = run(Mode::Single, src).checked.expect("checked");
    let to_sync: Vec<_> = checked
        .results
        .flows
        .iter()
        .filter(|(_, f)| f.to == kalam_typeck::Suspension::Sync)
        .map(|(_, f)| &src[f.span.start as usize..f.span.end as usize])
        .collect();
    assert_eq!(to_sync, vec!["make(world.fs)"]);
}
