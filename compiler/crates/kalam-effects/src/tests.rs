//! End-to-end tests: the real parser, resolver and stdlib stubs, then typeck and this pass.

use kalam_base::{FileId, Skin};
use kalam_diagnostics::Diagnostic;
use kalam_resolve::{DefId, ModulePath, SourceModule};
use kalam_typeck::hir::{ItemKind, TypeKind};
use kalam_typeck::{Checked, SuspBit};

use crate::{analyze, base_attrs};

const STD: &[(&str, &str)] = &[
    ("bigint", include_str!("../../../../stdlib/src/bigint.kl")),
    ("decimal", include_str!("../../../../stdlib/src/decimal.kl")),
    ("env", include_str!("../../../../stdlib/src/env.kl")),
    ("fs", include_str!("../../../../stdlib/src/fs.kl")),
    ("http", include_str!("../../../../stdlib/src/http.kl")),
    ("json", include_str!("../../../../stdlib/src/json.kl")),
    ("list", include_str!("../../../../stdlib/src/list.kl")),
    ("log", include_str!("../../../../stdlib/src/log.kl")),
    ("map", include_str!("../../../../stdlib/src/map.kl")),
    ("math", include_str!("../../../../stdlib/src/math.kl")),
    ("option", include_str!("../../../../stdlib/src/option.kl")),
    ("path", include_str!("../../../../stdlib/src/path.kl")),
    ("prelude", include_str!("../../../../stdlib/src/prelude.kl")),
    ("proc", include_str!("../../../../stdlib/src/proc.kl")),
    ("random", include_str!("../../../../stdlib/src/random.kl")),
    ("result", include_str!("../../../../stdlib/src/result.kl")),
    ("set", include_str!("../../../../stdlib/src/set.kl")),
    ("sql", include_str!("../../../../stdlib/src/sql.kl")),
    ("string", include_str!("../../../../stdlib/src/string.kl")),
    ("time", include_str!("../../../../stdlib/src/time.kl")),
];

const API_INDEX: &str = include_str!("../../../../stdlib/api-index.json");

struct Run {
    checked: Checked,
    diags: Vec<Diagnostic>,
}

impl Run {
    /// Codes reported in the main file.
    fn codes(&self) -> Vec<&'static str> {
        self.diags
            .iter()
            .filter(|d| d.file == FileId(0))
            .map(|d| d.code)
            .collect()
    }

    /// A top-level function or method of the main module, by name.
    fn def(&self, name: &str) -> DefId {
        self.checked
            .hir
            .items()
            .iter()
            .find(|i| i.module.0 == "main" && i.name == name && matches!(i.kind, ItemKind::Fn(_)))
            .unwrap_or_else(|| panic!("no fn `{name}`"))
            .def
    }

    /// A method `name` whose impl is for the type `ty` (main module).
    fn method(&self, ty: &str, name: &str) -> DefId {
        let hir = &self.checked.hir;
        hir.items()
            .iter()
            .find(|i| {
                i.module.0 == "main"
                    && i.name == name
                    && matches!(&i.kind, ItemKind::Fn(f) if f.parent.and_then(|p| hir.item(p)).is_some_and(|p| matches!(&p.kind, ItemKind::Impl(imp) if matches!(&imp.self_ty.kind, TypeKind::Def(d, _) if hir.item(*d).is_some_and(|t| t.name == ty)))))
            })
            .unwrap_or_else(|| panic!("no method `{ty}.{name}`"))
            .def
    }

    fn bit(&self, name: &str) -> SuspBit {
        self.checked.results.suspension.of_fn(self.def(name))
    }

    /// The bits of the calls in `owner` whose source text (by span) is `text`.
    fn call_bits(&self, src: &str, owner: &str, text: &str) -> Vec<SuspBit> {
        let owner = self.def(owner);
        let mut out = Vec::new();
        visit_calls(&self.checked, owner, &mut |id, span| {
            if &src[span.start as usize..span.end as usize] == text {
                out.push(self.checked.results.suspension.of_call(owner, id));
            }
        });
        out
    }
}

/// Every call and method call of `owner`'s body: `(expr id, span)`.
fn visit_calls(
    checked: &Checked,
    owner: DefId,
    f: &mut dyn FnMut(kalam_typeck::hir::ExprId, kalam_base::Span),
) {
    use kalam_typeck::hir::{BodyValue, Expr, ExprKind, Stmt, StmtKind};
    fn expr(e: &Expr, f: &mut dyn FnMut(kalam_typeck::hir::ExprId, kalam_base::Span)) {
        match &e.kind {
            ExprKind::Call { callee, args } => {
                f(e.id, e.span);
                expr(callee, f);
                args.iter().for_each(|a| expr(&a.value, f));
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                f(e.id, e.span);
                expr(receiver, f);
                args.iter().for_each(|a| expr(&a.value, f));
            }
            ExprKind::Interp(parts) => {
                for p in parts {
                    if let kalam_typeck::hir::StrPart::Hole(h) = p {
                        expr(h, f);
                    }
                }
            }
            ExprKind::Lambda { body, .. } => expr(body, f),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, f);
                expr(rhs, f);
            }
            ExprKind::If {
                branches,
                else_body,
            } => {
                for b in branches {
                    expr(&b.cond, f);
                    stmts(&b.body.stmts, f);
                }
                if let Some(eb) = else_body {
                    stmts(&eb.stmts, f);
                }
            }
            _ => {}
        }
    }
    fn stmts(ss: &[Stmt], f: &mut dyn FnMut(kalam_typeck::hir::ExprId, kalam_base::Span)) {
        for s in ss {
            match &s.kind {
                StmtKind::Expr(e)
                | StmtKind::Let { value: e, .. }
                | StmtKind::Var { value: e, .. }
                | StmtKind::Return(Some(e)) => expr(e, f),
                StmtKind::LocalFn { decl, .. } => match decl.body.as_ref().map(|b| &b.value) {
                    Some(BodyValue::Block(b)) => stmts(&b.stmts, f),
                    Some(BodyValue::Expr(e)) => expr(e, f),
                    None => {}
                },
                _ => {}
            }
        }
    }
    let Some(ItemKind::Fn(decl)) = checked.hir.item(owner).map(|i| &i.kind) else {
        return;
    };
    match decl.body.as_ref().map(|b| &b.value) {
        Some(BodyValue::Block(b)) => stmts(&b.stmts, f),
        Some(BodyValue::Expr(e)) => expr(e, f),
        None => {}
    }
}

fn run_with(src: &str, std_bodies: bool) -> Run {
    let main = kalam_parser::parse_source(src, Skin::En, FileId(0));
    let errors: Vec<_> = main.diagnostics.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let std: Vec<_> = STD
        .iter()
        .enumerate()
        .map(|(i, (name, text))| {
            (
                ModulePath(format!("std/{name}")),
                kalam_parser::parse_source(text, Skin::En, FileId(1 + i as u32)),
            )
        })
        .collect();
    let mut sources = vec![SourceModule {
        path: ModulePath("main".into()),
        file: FileId(0),
        skin: Skin::En,
        ast: &main.ast,
        std: false,
    }];
    for (i, (path, out)) in std.iter().enumerate() {
        sources.push(SourceModule {
            path: path.clone(),
            file: FileId(1 + i as u32),
            skin: Skin::En,
            ast: &out.ast,
            std: true,
        });
    }
    let (program, resolved) =
        kalam_resolve::resolve_program(&sources, &kalam_resolve::Options::default());
    let errors: Vec<_> = resolved.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    let attrs = base_attrs(&program, &sources);
    let (checked, diags) = kalam_typeck::check_resolved_then(&program, &sources, std_bodies, |c| {
        let diags = analyze(&c.hir, &mut c.results, &attrs);
        let typed: Vec<_> = c
            .diagnostics()
            .into_iter()
            .filter(|d| d.file == FileId(0))
            .collect();
        assert!(typed.is_empty(), "type errors: {typed:?}");
        let checked = Checked {
            hir: std::mem::take(&mut c.hir),
            results: std::mem::take(&mut c.results),
        };
        (checked, diags)
    });
    Run { checked, diags }
}

fn run(src: &str) -> Run {
    run_with(src, false)
}

fn sync() -> SuspBit {
    SuspBit::sync()
}

fn susp() -> SuspBit {
    SuspBit::suspending()
}

// ---- §15.2 base set and propagation --------------------------------------------------

#[test]
fn sync_declaration_that_suspends_is_k1402_with_its_chain() {
    let r = run("@sync\n\
         fn present(fs: Fs) -> Bool => fs.exists(path\"notes.txt\")\n\
         \n\
         fn main(world: World):\n    print(\"{present(world.fs)}\")\n");
    assert_eq!(r.codes(), vec!["K1402"]);
    let label = &r.diags[0].labels[0].text;
    assert_eq!(label, "`present` calls `exists` (suspends)");
    assert_eq!(r.bit("present"), susp());
}

#[test]
fn suspension_reaches_callers_and_the_chain_names_each_step() {
    let r = run(
        "fn present(fs: Fs) -> Bool => fs.exists(path\"notes.txt\")\n\
         \n\
         @sync\n\
         fn check(fs: Fs) -> Bool => present(fs)\n\
         \n\
         fn main(world: World):\n    print(\"{check(world.fs)}\")\n",
    );
    assert_eq!(r.codes(), vec!["K1402"]);
    assert_eq!(
        r.diags[0].labels[0].text,
        "`check` calls `present`, which calls `exists` (suspends)"
    );
    assert_eq!(r.bit("present"), susp());
    assert_eq!(
        r.bit("main"),
        sync(),
        "`check` is `@sync`: its callers trust it"
    );
}

#[test]
fn pure_call_chains_stay_sync() {
    let r = run("fn double(x: Int) -> Int => x * 2\n\
         \n\
         @sync\n\
         fn quadruple(x: Int) -> Int => double(double(x))\n\
         \n\
         fn main():\n    print(\"{quadruple(2)}\")\n");
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(r.bit("double"), sync());
    assert_eq!(r.bit("main"), sync());
}

// ---- recursion (SCC fixpoint) ------------------------------------------------------------

#[test]
fn mutual_recursion_without_io_stays_sync() {
    let r = run(
        "fn even(n: Int) -> Bool:\n    if n == 0:\n        return true\n    return odd(n - 1)\n\
         \n\
         fn odd(n: Int) -> Bool:\n    if n == 0:\n        return false\n    return even(n - 1)\n\
         \n\
         @sync\n\
         fn main():\n    print(\"{even(10)}\")\n",
    );
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(r.bit("even"), sync());
    assert_eq!(r.bit("odd"), sync());
}

#[test]
fn one_suspending_member_makes_the_whole_cycle_suspend() {
    let r = run("fn ping(fs: Fs, n: Int) -> Int:\n    if n == 0:\n        return 0\n    return pong(fs, n - 1)\n\
         \n\
         fn pong(fs: Fs, n: Int) -> Int:\n    if fs.exists(path\"stop\"):\n        return n\n    return ping(fs, n - 1)\n\
         \n\
         fn count(n: Int) -> Int:\n    if n == 0:\n        return 0\n    return count(n - 1)\n\
         \n\
         fn main(world: World):\n    print(\"{ping(world.fs, 3)} {count(3)}\")\n");
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(r.bit("ping"), susp());
    assert_eq!(r.bit("pong"), susp());
    assert_eq!(r.bit("main"), susp());
    assert_eq!(
        r.bit("count"),
        sync(),
        "self-recursion alone never suspends"
    );
}

#[test]
fn sync_member_of_a_suspending_cycle_is_k1402() {
    let r = run("@sync\n\
         fn ping(fs: Fs, n: Int) -> Int:\n    if n == 0:\n        return 0\n    return pong(fs, n - 1)\n\
         \n\
         fn pong(fs: Fs, n: Int) -> Int:\n    if fs.exists(path\"stop\"):\n        return n\n    return ping(fs, n - 1)\n\
         \n\
         fn main(world: World):\n    print(\"{ping(world.fs, 3)}\")\n");
    assert_eq!(r.codes(), vec!["K1402"]);
    assert_eq!(
        r.diags[0].labels[0].text,
        "`ping` calls `pong`, which calls `exists` (suspends)"
    );
}

// ---- §15.3 higher-order functions (effect polymorphism) ---------------------------------

const TWICE: &str = "fn twice(f: fn(Int) -> Int, x: Int) -> Int => f(f(x))\n\
     \n\
     fn inc(x: Int) -> Int => x + 1\n";

#[test]
fn a_higher_order_function_suspends_exactly_when_its_callback_does() {
    let src = format!(
        "{TWICE}\n\
         fn main(world: World):\n    let fs = world.fs\n    fn slow(x: Int) -> Int:\n        if fs.exists(path\"x\"):\n            return x\n        return x + 1\n    print(\"{{twice(inc, 1)}}\")\n    print(\"{{twice(slow, 1)}}\")\n"
    );
    let r = run(&src);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    // `twice` is polymorphic over its first effect parameter.
    assert_eq!(r.bit("twice"), SuspBit::param(0));
    assert_eq!(r.bit("inc"), sync());
    assert_eq!(r.call_bits(&src, "main", "twice(inc, 1)"), vec![sync()]);
    assert_eq!(r.call_bits(&src, "main", "twice(slow, 1)"), vec![susp()]);
    assert_eq!(r.bit("main"), susp());
}

#[test]
fn sync_instances_of_higher_order_functions_are_fine_inside_sync_code() {
    let src = format!("{TWICE}\n@sync\nfn main():\n    print(\"{{twice(inc, 1)}}\")\n    print(\"{{twice(fn(n) => n * 2, 1)}}\")\n");
    let r = run(&src);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(r.bit("main"), sync());
}

#[test]
fn a_polymorphic_function_forwarding_its_callback_stays_polymorphic() {
    let src = format!(
        "{TWICE}\n\
         fn four_times(g: fn(Int) -> Int, x: Int) -> Int => twice(g, twice(g, x))\n\
         \n\
         @sync\n\
         fn main():\n    print(\"{{four_times(inc, 1)}}\")\n"
    );
    let r = run(&src);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(r.bit("four_times"), SuspBit::param(0));
}

#[test]
fn stdlib_higher_order_methods_take_their_bit_from_the_lambda() {
    let src = "fn main(world: World):\n    let fs = world.fs\n    let xs = [1, 2, 3]\n    let ys = xs.map(fn(x) => x + 1)\n    let zs = xs.map(fn(x) => fs.exists(path\"x\"))\n    print(\"{ys} {zs}\")\n";
    let r = run(src);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(
        r.call_bits(src, "main", "xs.map(fn(x) => x + 1)"),
        vec![sync()]
    );
    assert_eq!(
        r.call_bits(src, "main", "xs.map(fn(x) => fs.exists(path\"x\"))"),
        vec![susp()]
    );
}

#[test]
fn passing_a_suspending_function_where_sync_is_required_is_k1401() {
    let src = "fn apply(f: @sync fn(Int) -> Int, x: Int) -> Int => f(x)\n\
         \n\
         fn main(world: World):\n    let fs = world.fs\n    fn step(x: Int) -> Int:\n        if fs.exists(path\"flag.txt\"):\n            return x + 1\n        return x\n    print(\"{apply(step, 1)}\")\n";
    let r = run(src);
    assert_eq!(r.codes(), vec!["K1401"]);
    assert_eq!(
        r.diags[0].labels[0].text,
        "`step` calls `exists` (suspends)"
    );
    assert_eq!(r.bit("apply"), sync());
}

#[test]
fn a_callback_forwarded_into_a_sync_slot_is_checked_at_the_outer_call() {
    let src = "fn apply(f: @sync fn(Int) -> Int, x: Int) -> Int => f(x)\n\
         \n\
         fn relay(g: fn(Int) -> Int) -> Int => apply(g, 1)\n\
         \n\
         fn inc(x: Int) -> Int => x + 1\n\
         \n\
         fn main(world: World):\n    let fs = world.fs\n    fn step(x: Int) -> Int:\n        if fs.exists(path\"flag.txt\"):\n            return x + 1\n        return x\n    print(\"{relay(inc)}\")\n    print(\"{relay(step)}\")\n";
    let r = run(src);
    assert_eq!(r.codes(), vec!["K1401"], "{:?}", r.diags);
    // `relay` itself is fine: its parameter became sync-required.
    assert_eq!(
        r.checked
            .results
            .suspension
            .sync_params
            .get(&r.def("relay")),
        Some(&vec![0])
    );
    let at = r.diags[0].span;
    assert_eq!(&src[at.start as usize..at.end as usize], "relay");
}

#[test]
fn cell_update_requires_a_sync_function() {
    let ok = run("fn main():\n    let counter = Cell.new(0)\n    counter.update(fn(n) => n + 1)\n    print(\"{counter.get()}\")\n");
    assert!(ok.codes().is_empty(), "{:?}", ok.diags);
    let bad = run("fn main(world: World):\n    let counter = Cell.new(0)\n    let fs = world.fs\n    fn step(x: Int) -> Int:\n        if fs.exists(path\"flag.txt\"):\n            return x + 1\n        return x\n    counter.update(step)\n    print(\"{counter.get()}\")\n");
    assert_eq!(bad.codes(), vec!["K1401"]);
}

// ---- closures: lambdas and local fns get their bit from their body ------------------------

#[test]
fn a_closure_that_suspends_does_not_make_its_definer_suspend() {
    let src = "fn main(world: World):\n    let fs = world.fs\n    let probe = fn() => fs.exists(path\"x\")\n    fn local(x: Int) -> Bool => fs.exists(path\"y\")\n    print(\"ready\")\n";
    let r = run(src);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(
        r.bit("main"),
        sync(),
        "defining a closure is not calling it"
    );
    let s = &r.checked.results.suspension;
    assert_eq!(s.lambdas.len(), 1);
    assert!(s.lambdas.values().all(|b| *b == susp()));
    assert_eq!(s.local_fns.len(), 1);
    assert!(s.local_fns.values().all(|b| *b == susp()));
}

// ---- §15.3 functions stored in data (always suspending unless `@sync`) ----------------------

#[test]
fn a_sync_function_may_be_stored_in_a_default_field_and_calling_it_suspends() {
    let src = "type Job:\n    run: fn(Int) -> Int\n\
         \n\
         fn double(x: Int) -> Int => x * 2\n\
         \n\
         fn go(job: Job) -> Int:\n    let f = job.run\n    return f(2)\n\
         \n\
         fn main():\n    print(\"{go(Job(run: double))}\")\n";
    let r = run(src);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(
        r.bit("go"),
        susp(),
        "stored callbacks are always suspending"
    );
    assert_eq!(r.bit("main"), susp());
}

#[test]
fn calling_a_stored_callback_in_sync_code_is_k1402() {
    let src = "type Job:\n    run: fn(Int) -> Int\n\
         \n\
         @sync\n\
         fn go(job: Job) -> Int:\n    let f = job.run\n    return f(2)\n\
         \n\
         fn main():\n    print(\"ok\")\n";
    let r = run(src);
    assert_eq!(r.codes(), vec!["K1402"]);
    assert!(
        r.diags[0].labels[0].text.contains("stored in data"),
        "{}",
        r.diags[0].labels[0].text
    );
}

#[test]
fn sync_fields_keep_stored_callbacks_sync_and_reject_suspending_ones() {
    let ok = "type Job:\n    run: @sync fn(Int) -> Int\n\
         \n\
         fn double(x: Int) -> Int => x * 2\n\
         \n\
         @sync\n\
         fn go(job: Job) -> Int:\n    let f = job.run\n    return f(2)\n\
         \n\
         fn main():\n    print(\"{go(Job(run: double))}\")\n";
    let r = run(ok);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(r.bit("go"), sync());
    let bad = "type Job:\n    run: @sync fn(Int) -> Int\n\
         \n\
         fn main(world: World):\n    let fs = world.fs\n    fn step(x: Int) -> Int:\n        if fs.exists(path\"flag.txt\"):\n            return x + 1\n        return x\n    let job = Job(run: step)\n    let f = job.run\n    print(\"{f(1)}\")\n";
    assert_eq!(run(bad).codes(), vec!["K1401"]);
}

#[test]
fn sort_by_takes_a_sync_comparator() {
    let ok = "fn main():
    let xs = [3.0, 1.0, 2.0]
    print(\"{xs.sort_by(fn(a, b) => a.total_cmp(b))}\")
";
    let r = run(ok);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    let bad = "fn main(world: World):
    let fs = world.fs
    fn cmp(a: Float, b: Float) -> Ordering:
        if fs.exists(path\"x\"):
            return a.total_cmp(b)
        return b.total_cmp(a)
    print(\"{[3.0, 1.0].sort_by(cmp)}\")
";
    let r = run(bad);
    assert_eq!(r.codes(), vec!["K1401"]);
    assert_eq!(r.diags[0].labels[0].text, "`cmp` calls `exists` (suspends)");
}

// ---- trait methods ---------------------------------------------------------------------

const LOADERS: &str = "trait Loader:\n    fn load(self) -> Int\n\
     \n\
     type Pure:\n    n: Int\n\
     \n\
     type Disk:\n    fs: Fs\n\
     \n\
     impl Pure as Loader:\n    fn load(self) -> Int => self.n\n\
     \n\
     impl Disk as Loader:\n    fn load(self) -> Int:\n        if self.fs.exists(path\"n\"):\n            return 1\n        return 0\n\
     \n\
     fn generic[T: Loader](x: T) -> Int => x.load()\n";

#[test]
fn trait_methods_resolve_per_impl_and_generic_dispatch_joins_every_impl() {
    let src = format!(
        "{LOADERS}\n\
         fn main(world: World):\n    let p = Pure(n: 1)\n    let d = Disk(fs: world.fs)\n    print(\"{{p.load()}}\")\n    print(\"{{d.load()}}\")\n    print(\"{{generic(p)}}\")\n"
    );
    let r = run(&src);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(
        r.checked.results.suspension.of_fn(r.method("Pure", "load")),
        sync()
    );
    assert_eq!(
        r.checked.results.suspension.of_fn(r.method("Disk", "load")),
        susp()
    );
    assert_eq!(r.call_bits(&src, "main", "p.load()"), vec![sync()]);
    assert_eq!(r.call_bits(&src, "main", "d.load()"), vec![susp()]);
    // A generic receiver may be any impl in the program, one of which suspends.
    assert_eq!(r.bit("generic"), susp());
}

#[test]
fn generic_dispatch_over_pure_impls_is_sync() {
    let src = "trait Loader:\n    fn load(self) -> Int\n\
         \n\
         type Pure:\n    n: Int\n\
         \n\
         impl Pure as Loader:\n    fn load(self) -> Int => self.n\n\
         \n\
         fn generic[T: Loader](x: T) -> Int => x.load()\n\
         \n\
         @sync\n\
         fn main():\n    print(\"{generic(Pure(n: 1))}\")\n";
    let r = run(src);
    assert!(r.codes().is_empty(), "{:?}", r.diags);
    assert_eq!(r.bit("generic"), sync());
}

// ---- W0421 -------------------------------------------------------------------------------

#[test]
fn a_cell_value_used_across_a_wait_is_w0421() {
    let src = "fn main(world: World):\n    let counter = Cell.new(0)\n    let before = counter.get()\n    let found = world.fs.exists(path\"x\")\n    print(\"{before} {found}\")\n";
    let r = run(src);
    assert_eq!(r.codes(), vec!["W0421"]);
    let at = r.diags[0].span;
    assert_eq!(&src[at.start as usize..at.end as usize], "before");
    let fine = "fn main(world: World):\n    let counter = Cell.new(0)\n    let found = world.fs.exists(path\"x\")\n    let now = counter.get()\n    print(\"{now} {found}\")\n";
    assert!(run(fine).codes().is_empty());
}

// ---- stdlib stubs ------------------------------------------------------------------------

/// The stub rule agrees with the `suspends` field of `stdlib/api-index.json` for every
/// stdlib function and method, with and without the stdlib's placeholder bodies.
#[test]
fn stub_bits_agree_with_the_api_index() {
    let index = parse_index();
    for std_bodies in [false, true] {
        let r = run_with("fn main():\n    print(\"x\")\n", std_bodies);
        let hir = &r.checked.hir;
        let mut checked = 0;
        for item in hir.items() {
            let ItemKind::Fn(f) = &item.kind else {
                continue;
            };
            if !item.module.0.starts_with("std/") {
                continue;
            }
            // Trait methods dispatch over impls; they are not in the stub rule.
            let owner = match f.parent.and_then(|p| hir.item(p)).map(|p| &p.kind) {
                Some(ItemKind::Trait(_)) => continue,
                Some(ItemKind::Impl(imp)) => type_name(hir, &imp.self_ty),
                _ => None,
            };
            let path = match owner {
                Some(o) => format!("{o}.{}", item.name),
                None => format!("{}.{}", item.module.0, item.name),
            };
            let Some(expected) = index
                .iter()
                .find(|(p, m, _)| *p == path && *m == item.module.0)
                .map(|(_, _, s)| s.as_str())
            else {
                continue;
            };
            let bit = r.checked.results.suspension.of_fn(item.def);
            let got = if bit.always {
                "yes"
            } else if bit.params.is_empty() {
                "no"
            } else {
                "if_callback"
            };
            assert_eq!(got, expected, "{path} (std bodies: {std_bodies})");
            checked += 1;
        }
        assert!(
            checked > 150,
            "only {checked} stdlib functions matched the index"
        );
    }
}

fn type_name(hir: &kalam_typeck::HirProgram, t: &kalam_typeck::hir::Type) -> Option<String> {
    use kalam_typeck::hir::BuiltinType as B;
    Some(match &t.kind {
        TypeKind::Def(d, _) => hir.item(*d)?.name.clone(),
        TypeKind::Builtin(B::Capability(c), _) => c.name().to_string(),
        TypeKind::Builtin(B::Sink(s), _) => s.name().to_string(),
        TypeKind::Builtin(b, _) => format!("{b:?}"),
        _ => return None,
    })
}

/// `(path, module, suspends)` of every function in the index (a minimal reader: the file
/// is generated, one field per line).
fn parse_index() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let (mut path, mut module) = (String::new(), String::new());
    let field = |line: &str, key: &str| -> Option<String> {
        let rest = line.trim().strip_prefix(&format!("\"{key}\": \""))?;
        Some(rest.trim_end_matches(',').trim_end_matches('"').to_string())
    };
    for line in API_INDEX.lines() {
        if let Some(p) = field(line, "path") {
            path = p;
        } else if let Some(m) = field(line, "module") {
            module = m;
        } else if let Some(s) = field(line, "suspends") {
            out.push((path.clone(), module.clone(), s));
        }
    }
    out
}

// ---- overhead on the corpus ----------------------------------------------------------------

/// Measures the cost of this pass against type checking alone over `eval/corpus` (every
/// task, every skin), and reports any effect diagnostic there. Run with
/// `cargo test -p kalam-effects --release -- --ignored --nocapture corpus_overhead`.
#[test]
#[ignore]
fn corpus_overhead() {
    use std::time::{Duration, Instant};
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../eval/corpus");
    let mut files = Vec::new();
    for task in std::fs::read_dir(&corpus).expect("eval/corpus").flatten() {
        for (name, skin) in [
            ("en.kalam", Skin::En),
            ("ar.kalam", Skin::Ar),
            ("franco.kalam", Skin::Franco),
        ] {
            let p = task.path().join(name);
            if let Ok(text) = std::fs::read_to_string(&p) {
                files.push((p, text, skin));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let std: Vec<_> = STD
        .iter()
        .enumerate()
        .map(|(i, (name, text))| {
            (
                ModulePath(format!("std/{name}")),
                kalam_parser::parse_source(text, Skin::En, FileId(1 + i as u32)),
            )
        })
        .collect();
    const ROUNDS: u32 = 20;
    let (mut typeck, mut effects, mut analyze_only, mut attrs_only) = (
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
    );
    let (mut measured, mut skipped, mut fns, mut calls) = (0, 0, 0, 0);
    let mut found: Vec<String> = Vec::new();
    for (path, text, skin) in &files {
        let main = kalam_parser::parse_source(text, *skin, FileId(0));
        if main.diagnostics.iter().any(|d| d.is_error()) {
            skipped += 1;
            continue;
        }
        let mut sources = vec![SourceModule {
            path: ModulePath("main".into()),
            file: FileId(0),
            skin: *skin,
            ast: &main.ast,
            std: false,
        }];
        for (i, (p, out)) in std.iter().enumerate() {
            sources.push(SourceModule {
                path: p.clone(),
                file: FileId(1 + i as u32),
                skin: Skin::En,
                ast: &out.ast,
                std: true,
            });
        }
        let (program, resolved) =
            kalam_resolve::resolve_program(&sources, &kalam_resolve::Options::default());
        if resolved.iter().any(|d| d.is_error()) {
            skipped += 1;
            continue;
        }
        measured += 1;
        for round in 0..ROUNDS {
            let t = Instant::now();
            kalam_typeck::check_resolved_then(&program, &sources, false, |_| ());
            typeck += t.elapsed();
            let t = Instant::now();
            let ta = Instant::now();
            let attrs = base_attrs(&program, &sources);
            attrs_only += ta.elapsed();
            let (inner, diags, n_fns, n_calls) =
                kalam_typeck::check_resolved_then(&program, &sources, false, |c| {
                    let t = Instant::now();
                    let d = analyze(&c.hir, &mut c.results, &attrs);
                    let s = &c.results.suspension;
                    (t.elapsed(), d, s.fns.len(), s.calls.len())
                });
            effects += t.elapsed();
            analyze_only += inner;
            if round == 0 {
                fns += n_fns;
                calls += n_calls;
                for d in diags.iter().filter(|d| d.file == FileId(0)) {
                    found.push(format!("{}: {}", path.display(), d.code));
                }
            }
        }
    }
    let ms = |d: Duration| d.as_secs_f64() * 1000.0 / f64::from(ROUNDS);
    println!(
        "corpus: {} files measured, {} skipped (parse/resolve errors); {ROUNDS} rounds",
        measured, skipped
    );
    println!(
        "typeck alone: {:.2} ms/round; typeck + effects: {:.2} ms/round; analyze alone: {:.2} ms/round ({:.1}% of typeck)",
        ms(typeck),
        ms(effects),
        ms(analyze_only),
        100.0 * analyze_only.as_secs_f64() / typeck.as_secs_f64()
    );
    println!("base_attrs alone: {:.2} ms/round", ms(attrs_only));
    println!("functions with a bit: {fns}; suspending call sites: {calls}");
    println!("effect diagnostics in the corpus: {found:?}");
}
