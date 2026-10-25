//! End-to-end semantics of the reference interpreter: Kalam source → `kalam_driver::run_interp`
//! (check, lower, run) → stdout, exit code and panic message. Each case pins one spec rule.

use kalam_driver::{run_interp, RunOutcome};

struct Ran {
    stdout: String,
    exit: u8,
    stderr: String,
}

fn run_as(path: &str, src: &str) -> Ran {
    let mut out = Vec::new();
    let outcome = run_interp(path, src.as_bytes(), &mut out).expect("a Kalam path");
    let stderr = match &outcome {
        RunOutcome::Rejected(c) => c.render_human(kalam_base::Skin::En),
        other => other.stderr_text(path),
    };
    Ran {
        stdout: String::from_utf8(out).expect("UTF-8 output"),
        exit: outcome.exit_code(),
        stderr,
    }
}

fn run(src: &str) -> Ran {
    run_as("main.kl", src)
}

/// Runs `src` and requires exit 0 with exactly `want` on stdout.
#[track_caller]
fn ok(src: &str, want: &str) {
    let r = run(src);
    assert_eq!(
        (r.exit, r.stdout.as_str()),
        (0, want),
        "stderr: {}",
        r.stderr
    );
}

/// Runs `src` and requires a panic (exit 101) whose message contains `msg`, after `want`.
#[track_caller]
fn panics(src: &str, want: &str, msg: &str) {
    let r = run(src);
    assert_eq!(r.exit, 101, "stdout {:?} stderr {}", r.stdout, r.stderr);
    assert_eq!(r.stdout, want);
    assert!(r.stderr.starts_with("panic: "), "{}", r.stderr);
    assert!(r.stderr.contains(msg), "{}", r.stderr);
}

#[test]
fn hello_in_every_skin() {
    ok("fn main():\n    print(\"hi\")\n", "hi\n");
    let r = run_as("main.ar.kl", "دالة رئيسي():\n    اطبع(\"مرحبا\")\n");
    assert_eq!((r.exit, r.stdout.as_str()), (0, "مرحبا\n"), "{}", r.stderr);
    let r = run_as("main.franco.kl", "dala ra2isi():\n    ektb(\"ahlan\")\n");
    assert_eq!((r.exit, r.stdout.as_str()), (0, "ahlan\n"), "{}", r.stderr);
}

#[test]
fn int_is_checked_64_bit_and_traps_exactly_at_the_bounds() {
    ok(
        "fn main():\n    let big = 9223372036854775806\n    print(\"{big + 1}\")\n    let small = -9223372036854775807\n    print(\"{small - 1}\")\n    print(\"{9007199254740991 + 1}\")\n    print(\"{3037000499 * 3037000499}\")\n",
        "9223372036854775807\n-9223372036854775808\n9007199254740992\n9223372030926249001\n",
    );
    panics(
        "fn main():\n    let big = 9223372036854775807\n    print(\"before\")\n    print(\"{big + 1}\")\n",
        "before\n",
        "integer overflow",
    );
    panics(
        "fn main():\n    let small = -9223372036854775808\n    print(\"{small - 1}\")\n",
        "",
        "integer overflow",
    );
    panics(
        "fn main():\n    let small = -9223372036854775808\n    print(\"{-small}\")\n",
        "",
        "integer overflow",
    );
    panics(
        "fn main():\n    let x = 3037000500\n    print(\"{x * x}\")\n",
        "",
        "integer overflow",
    );
    panics(
        "fn main():\n    let small = -9223372036854775808\n    let m = -1\n    print(\"{small // m}\")\n",
        "",
        "integer overflow",
    );
}

#[test]
fn division_floors_and_modulo_takes_the_divisor_sign() {
    ok(
        "fn main():\n    print(\"{-7 // 2} {-7 % 2} {7 % -2} {7 // -2}\")\n    let small = -9223372036854775808\n    let m = -1\n    print(\"{small % m}\")\n",
        "-4 1 -1 -4\n0\n",
    );
    panics(
        "fn main():\n    let z = 0\n    print(\"{1 // z}\")\n",
        "",
        "division by zero",
    );
    panics(
        "fn main():\n    let z = 0\n    print(\"{1 % z}\")\n",
        "",
        "modulo by zero",
    );
}

#[test]
fn a_panic_reports_its_message_and_location() {
    let r = run("fn main():\n    print(\"a\")\n    panic(\"boom\")\n");
    assert_eq!(r.exit, 101);
    assert_eq!(r.stdout, "a\n");
    assert_eq!(r.stderr, "panic: boom\n  --> main.kl:3:5\n");
    panics(
        "fn main():\n    assert(1 > 2, \"one is not more than two\")\n",
        "",
        "one is not more than two",
    );
    panics(
        "fn main():\n    let xs = [1, 2]\n    print(\"{xs[2]}\")\n",
        "",
        "index 2 is out of bounds for a list of length 2",
    );
    panics(
        "fn main():\n    let xs = [1, 2]\n    print(\"{xs[-1]}\")\n",
        "",
        "out of bounds",
    );
    panics(
        "fn main():\n    let o: Option[Int] = None\n    let v = o.expect(\"needed a value\")\n    print(\"{v}\")\n",
        "",
        "needed a value",
    );
}

#[test]
fn deep_recursion_is_a_defined_panic() {
    ok(
        "fn depth(n: Int) -> Int:\n    if n == 0:\n        return 0\n    return 1 + depth(n - 1)\n\nfn main():\n    print(\"{depth(5000)}\")\n",
        "5000\n",
    );
    panics(
        "fn forever(n: Int) -> Int => forever(n + 1)\n\nfn main():\n    print(\"{forever(0)}\")\n",
        "",
        "stack overflow",
    );
}

#[test]
fn records_defaults_with_and_structural_equality() {
    ok(
        "type Point:\n    x: Int\n    y: Int = 7\n\nfn main():\n    let p = Point(x: 1)\n    let q = p.with(x: 5)\n    print(\"{p.x} {p.y} {q.x} {q.y}\")\n    print(if p == Point(y: 7, x: 1): \"same\" else: \"different\")\n    print(if p == q: \"same\" else: \"different\")\n",
        "1 7 5 7\nsame\ndifferent\n",
    );
}

#[test]
fn named_arguments_run_in_source_order_and_defaults_fill_in() {
    ok(
        "fn note(s: String) -> Int:\n    print(s)\n    return 1\n\nfn sub(a: Int, b: Int = 100) -> Int => a - b\n\nfn main():\n    let r = sub(b: note(\"b\"), a: note(\"a\") + 9)\n    print(\"{r}\")\n    print(\"{sub(1)}\")\n",
        "b\na\n9\n-99\n",
    );
}

#[test]
fn assignment_has_value_semantics() {
    ok(
        "type Inner:\n    n: Int\n\ntype Outer:\n    inner: Inner\n    tag: String\n\nfn main():\n    var o = Outer(inner: Inner(n: 1), tag: \"t\")\n    let before = o\n    o.inner.n = 5\n    o.inner.n += 1\n    print(\"{before.inner.n} {o.inner.n} {o.tag}\")\n    var xs = [1, 2, 3]\n    let ys = xs\n    xs[0] = 10\n    xs[2] *= 3\n    print(\"{xs[0]} {xs[2]} {ys[0]}\")\n    var m: Map[String, Int] = {}\n    m[\"a\"] = 1\n    m[\"b\"] = 2\n    m[\"a\"] += 10\n    for (k, v) in m:\n        print(\"{k}={v}\")\n",
        "1 6 t\n10 9 1\na=11\nb=2\n",
    );
}

#[test]
fn enums_and_nested_patterns() {
    ok(
        "enum Shape:\n    Circle(r: Int)\n    Rect(w: Int, h: Int)\n    Empty\n\nfn describe(s: Option[Shape]) -> String:\n    return match s:\n        case Some(.Circle(r)) if r > 10: \"big circle\"\n        case Some(.Circle(_)): \"circle\"\n        case Some(.Rect(w: 1, ..)) | Some(.Empty): \"thin or empty\"\n        case Some(.Rect(w, h)): \"rect {w}x{h}\"\n        case None: \"nothing\"\n\nfn main():\n    print(describe(Some(.Circle(r: 20))))\n    print(describe(Some(.Circle(r: 2))))\n    print(describe(Some(.Rect(w: 1, h: 9))))\n    print(describe(Some(.Empty)))\n    print(describe(Some(.Rect(w: 3, h: 4))))\n    print(describe(None))\n",
        "big circle\ncircle\nthin or empty\nthin or empty\nrect 3x4\nnothing\n",
    );
}

#[test]
fn or_patterns_bind_from_the_alternative_that_matched() {
    ok(
        "enum Pay:\n    Card(amount: Int)\n    Cash(amount: Int)\n    Free\n\nfn amount(p: Pay) -> Int:\n    return match p:\n        case .Card(a) | .Cash(a): a\n        case .Free: 0\n\nfn main():\n    print(\"{amount(.Card(amount: 3))} {amount(.Cash(amount: 4))} {amount(.Free)}\")\n",
        "3 4 0\n",
    );
}

#[test]
fn list_tuple_literal_and_as_patterns() {
    ok(
        "fn shape(xs: List[Int]) -> String:\n    return match xs:\n        case []: \"empty\"\n        case [x]: \"one {x}\"\n        case [first, ..rest]: \"first {first}, {rest.len()} more\"\n\nfn main():\n    print(shape([]))\n    print(shape([7]))\n    print(shape([1, 2, 3]))\n    let pair = (1, \"one\")\n    match pair:\n        case (1, name) as whole:\n            print(\"{name} {whole.0}\")\n        case _:\n            print(\"other\")\n    match \"b\":\n        case \"a\": print(\"A\")\n        case \"b\": print(\"B\")\n        case _: print(\"?\")\n    match 42:\n        case 1..=9: print(\"small\")\n        case 10..100: print(\"medium\")\n        case _: print(\"large\")\n",
        "empty\none 7\nfirst 1, 2 more\none 1\nB\nmedium\n",
    );
}

#[test]
fn try_propagates_converts_and_returns_from_lambdas() {
    ok(
        "enum Low:\n    Bad(n: Int)\n\nenum High:\n    Wrapped(inner: Low)\n    Other\n\nimpl High as From[Low]:\n    fn from(value: Low) -> High => .Wrapped(inner: value)\n\nfn low(n: Int) -> Result[Int, Low]:\n    if n < 0:\n        return Err(.Bad(n: n))\n    return Ok(n * 2)\n\nfn high(n: Int) -> Result[Int, High]:\n    let v = low(n)?\n    return Ok(v + 1)\n\nfn first_even(xs: List[Int]) -> Option[Int]:\n    let x = xs.find(fn(x) => x % 2 == 0)?\n    return Some(x * 10)\n\nfn main():\n    match high(5):\n        case Ok(v): print(\"ok {v}\")\n        case Err(_): print(\"err\")\n    match high(-3):\n        case Ok(_): print(\"ok\")\n        case Err(.Wrapped(.Bad(n))): print(\"wrapped {n}\")\n        case Err(.Other): print(\"other\")\n    print(\"{first_even([1, 4, 6]).or(0)} {first_even([1, 3]).or(-1)}\")\n    let f = fn(x: Option[Int]) => Some(x? + 1)\n    print(\"{f(Some(1)).or(0)} {f(None).or(0)}\")\n",
        "ok 11\nwrapped -3\n40 -1\n2 0\n",
    );
}

#[test]
fn closures_capture_by_value_and_local_functions_recurse() {
    ok(
        "fn apply(f: fn(Int) -> Int, x: Int) -> Int => f(x)\n\nfn double(x: Int) -> Int => x * 2\n\nfn main():\n    let base = 10\n    let add = fn(x: Int) => x + base\n    print(\"{apply(add, 1)} {apply(double, 4)}\")\n    fn fact(n: Int) -> Int:\n        if n <= 1:\n            return 1\n        return n * fact(n - 1)\n    print(\"{fact(10)}\")\n    let xs = [3, 1, 2].map(fn(x) => x * base)\n    print(\"{xs[0]} {xs[1]} {xs[2]}\")\n    let total = [1, 2, 3, 4].fold(0, fn(acc, x) => acc + x)\n    print(\"{total}\")\n",
        "11 8\n3628800\n30 10 20\n10\n",
    );
}

#[test]
fn user_show_impls_drive_interpolation_and_print() {
    ok(
        "type Money:\n    cents: Int\n\nimpl Money as Show:\n    fn show(self) -> String => \"{self.cents // 100}.{self.cents % 100} EGP\"\n\nfn main():\n    let m = Money(cents: 1250)\n    print(m)\n    print(\"total: {m}\")\n",
        "12.50 EGP\ntotal: 12.50 EGP\n",
    );
}

#[test]
fn methods_and_associated_functions_of_user_types() {
    ok(
        "type Counter:\n    n: Int\n\nimpl Counter:\n    fn zero() -> Counter => Counter(n: 0)\n    fn bump(self, by: Int = 1) -> Counter => self.with(n: self.n + by)\n\nfn main():\n    let c = Counter.zero().bump().bump(by: 5)\n    print(\"{c.n}\")\n",
        "6\n",
    );
}

#[test]
fn strings_lists_maps_and_sets_follow_the_stdlib() {
    ok(
        "fn main():\n    let s = \"  Kalam كلام  \".trim()\n    print(\"[{s}] {s.len_chars()} {s.byte_len()} {s.to_upper()}\")\n    let parts = \"a,b,,c\".split(\",\")\n    let joined = String.join(parts, \"|\")\n    print(\"{parts.len()} {joined}\")\n    let (abc, bc, b) = (\"abc\", \"bc\", \"b\")\n    print(\"{abc.contains(bc)} {abc.starts_with(b)}\")\n    let n = \"٤٢\".to_int().ok().or(0)\n    print(\"{n + 1}\")\n    let xs = [5, 3, 9, 1]\n    print(\"{xs.sorted()[0]} {xs.sum()} {xs.take(2).len()} {xs.skip(3)[0]}\")\n    var counts: Map[String, Int] = {}\n    for w in [\"b\", \"a\", \"b\"]:\n        counts[w] = counts.get(w).or(0) + 1\n    for (k, v) in counts:\n        print(\"{k}:{v}\")\n    let set = Set.of([3, 1, 3, 2])\n    print(\"{set.len()} {set.contains(2)} {set.to_list()[0]}\")\n    for (i, x) in [\"x\", \"y\"].enumerate():\n        print(\"{i}{x}\")\n",
        "[Kalam كلام] 10 14 KALAM كلام\n4 a|b||c\ntrue false\n43\n1 18 2 1\nb:2\na:1\n3 true 3\n0x\n1y\n",
    );
}

#[test]
fn decimal_is_exact_and_bigint_is_unbounded() {
    ok(
        "use std/decimal.{Rounding}\n\nfn main():\n    let a: Decimal = 0.1\n    let b: Decimal = 0.2\n    print(\"{a + b} {a * b}\")\n    print(if a + b == 0.3: \"exact\" else: \"inexact\")\n    let price: Decimal = 19.99\n    print(\"{price * Decimal.from(3)}\")\n    print(\"{Decimal.from(10).div(Decimal.from(3), 2, .HalfEven)}\")\n    let big: BigInt = 9223372036854775807\n    print(\"{big * big}\")\n    print(\"{BigInt.from(-7) // 2} {BigInt.from(-7) % 2}\")\n",
        "0.3 0.02\nexact\n59.97\n3.33\n85070591730234615847396907784232501249\n-4 1\n",
    );
}

#[test]
fn floats_compare_with_ieee_semantics() {
    ok(
        "use std/math\n\nfn main():\n    let x = 0.1 + 0.2\n    print(if x == 0.3: \"equal\" else: \"not equal\")\n    print(\"{math.round(2.5).to_int().or(0)} {math.round(-2.5).to_int().or(0)} {(7 / 2).to_int().or(0)}\")\n    let nan = 0.0 / 0.0\n    print(if nan == nan: \"nan equal\" else: \"nan not equal\")\n    print(\"{nan.to_int().is_none()}\")\n",
        "not equal\n3 -3 3\nnan not equal\ntrue\n",
    );
}

#[test]
fn module_constants_are_evaluated_once() {
    ok(
        "let LIMIT: Int = 3\nlet NAMES = [\"a\", \"b\"]\n\nfn main():\n    for i in 0..LIMIT:\n        print(\"{i} {NAMES.len()}\")\n",
        "0 2\n1 2\n2 2\n",
    );
}

#[test]
fn loops_break_continue_and_while() {
    ok(
        "fn main():\n    var i = 0\n    while true:\n        i += 1\n        if i % 2 == 0:\n            continue\n        if i > 7:\n            break\n        print(\"{i}\")\n    for j in 9223372036854775806..=9223372036854775807:\n        print(\"{j}\")\n",
        "1\n3\n5\n7\n9223372036854775806\n9223372036854775807\n",
    );
}

#[test]
fn main_returning_err_exits_1_with_the_message() {
    let r = run(
        "enum Oops:\n    Broken\n\nimpl Oops as Error:\n    fn message(self) -> String => \"it broke\"\n\nfn main() -> Result[(), Oops]:\n    print(\"start\")\n    return Err(.Broken)\n",
    );
    assert_eq!((r.exit, r.stdout.as_str()), (1, "start\n"), "{}", r.stderr);
    assert_eq!(r.stderr, "error: it broke\n");
}

#[test]
fn programs_with_errors_or_outside_the_prelude_do_not_run() {
    let r = run("fn main():\n    let x: Int = \"no\"\n    print(\"never\")\n");
    assert_eq!((r.exit, r.stdout.as_str()), (1, ""));
    assert!(r.stderr.contains("K06"), "{}", r.stderr);
    // A stdlib call that only exists as a signature stub in M2: refused before it starts.
    let r = run(
        "fn main(world: World):\n    print(\"never\")\n    let line = world.stdin.read_line()\n",
    );
    assert_eq!((r.exit, r.stdout.as_str()), (2, ""), "{}", r.stderr);
    assert!(r.stderr.starts_with("cannot run `main.kl`"), "{}", r.stderr);
}

#[test]
fn main_may_take_the_world() {
    ok("fn main(world: World):\n    print(\"hi\")\n", "hi\n");
}

#[test]
fn nested_with_updates() {
    ok(
        "type A:\n    x: Int\n\ntype B:\n    a: A\n\nfn main():\n    let b = B(a: A(x: 1))\n    let c = b.with(a: b.a.with(x: 2))\n    print(\"{b.a.x} {c.a.x}\")\n",
        "1 2\n",
    );
}

#[test]
fn secrets_and_capabilities_show_as_the_spec_says() {
    ok(
        "fn main(world: World):\n    let password = Secret.new(\"hunter2\")\n    print(\"password: {password}\")\n    print(\"{password.expose().len_chars()}\")\n    print(\"{world}\")\n",
        "password: [REDACTED]\n7\n<World>\n",
    );
}

#[test]
fn tagged_literals_are_refused_before_anything_runs() {
    let r = run("fn main():\n    print(\"before\")\n    let q = sql\"SELECT 1\"\n");
    assert_eq!((r.exit, r.stdout.as_str()), (2, ""), "{}", r.stderr);
    assert!(r.stderr.contains("`sql\"…\"` literals"), "{}", r.stderr);
}

/// Q19 counterexample (typeck finding, M2 iter 3): the checker accepts `Ok(1).is_ok()`
/// although nothing fixes the error type `E`, so `Result[Int, ?]` reaches KIR and the run
/// stops with an internal error instead of either a compile error (K0802-like "needs an
/// expected type") or a defaulted type. Un-ignore once typeck rejects or defaults it.
#[test]
#[ignore = "typeck lets an uninferred type argument through (reported to the typeck owner)"]
fn an_uninferred_type_argument_never_reaches_the_interpreter() {
    let r = run("fn main():\n    let b = Ok(1).is_ok()\n    print(\"{b}\")\n");
    assert!(r.exit == 0 || r.exit == 1, "exit {}: {}", r.exit, r.stderr);
}
