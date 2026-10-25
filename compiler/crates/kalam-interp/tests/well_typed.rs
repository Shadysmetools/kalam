//! Q19 (FOUNDER-QUEUE): until a soundness proof exists, property tests on the reference
//! interpreter stand in for it — **well-typed programs don't go wrong**.
//!
//! A type-directed generator writes random prelude programs (records, enums, `Option`,
//! `Result` with `?`, lists, strings, closures, loops, `match`, `var` and value-semantic
//! assignment). Each program goes through the real pipeline (`kalam_driver::run_interp`).
//! Programs the checker rejects are discarded; for every accepted one, the run must end in
//! one of the spec's outcomes (§9.10: a value, or a *defined* panic such as overflow, an
//! index out of bounds or division by zero), never in an interpreter shape error
//! ([`RunOutcome::Internal`]) and never in "unsupported" (the generator only uses
//! constructs the M2 interpreter runs). Every 8th program is also converted to the Arabic
//! and Franco skins, which must behave identically (parity P5).
//!
//! `KALAM_INTERP_PROPS_CASES` (default 300) and `KALAM_INTERP_PROPS_SEED` configure a run;
//! a failure prints the seed and the program.

use kalam_base::Skin;
use kalam_driver::{convert_source, run_interp, FmtConfig, RunOutcome};

// -- a seeded generator (no proptest dependency, as in kalam-props) -------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum T {
    Int,
    Bool,
    Str,
    List,
    Opt,
    Res,
    Pair,
    Shape,
}

const ALL: [T; 8] = [
    T::Int,
    T::Bool,
    T::Str,
    T::List,
    T::Opt,
    T::Res,
    T::Pair,
    T::Shape,
];

fn name(t: T) -> &'static str {
    match t {
        T::Int => "Int",
        T::Bool => "Bool",
        T::Str => "String",
        T::List => "List[Int]",
        T::Opt => "Option[Int]",
        T::Res => "Result[Int, String]",
        T::Pair => "Pair",
        T::Shape => "Shape",
    }
}

/// Types with `==` (§9.8) and a printable `Show` (primitives only: derived `Show` text is
/// unspecified, spec gaps G3/G4).
fn has_eq(t: T) -> bool {
    matches!(t, T::Int | T::Bool | T::Str | T::List | T::Pair | T::Shape)
}

struct Var {
    name: String,
    ty: T,
    mutable: bool,
}

struct Sig {
    name: String,
    params: Vec<T>,
    ret: T,
}

/// `ok`/`err` build fully typed `Result`s: the checker currently accepts `Ok(1).is_ok()`
/// with the error type never inferred (a typeck finding reported with this test), so the
/// generator never writes a bare `Ok`/`Err` where nothing fixes `E`.
const PREAMBLE: &str = "type Pair:\n    a: Int\n    b: String = \"d\"\n\nenum Shape:\n    Circle(r: Int)\n    Square(side: Int)\n    Dot\n\nfn ok(n: Int) -> Result[Int, String] => Ok(n)\n\nfn err(s: String) -> Result[Int, String] => Err(s)\n\n";

const WORDS: &[&str] = &[
    "", "a", "kalam", "كلام", " pad ", "x y", "Zed", "٤٢", "-17", "99",
];

struct Gen {
    rng: Rng,
    next: u32,
    fns: Vec<Sig>,
    scopes: Vec<Vec<Var>>,
    /// Return type of the function being written (for `?`).
    ret: T,
}

impl Gen {
    fn fresh(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    fn vars(&self, t: T) -> Vec<&Var> {
        self.scopes.iter().flatten().filter(|v| v.ty == t).collect()
    }

    fn var_of(&mut self, t: T) -> Option<String> {
        let names: Vec<String> = self.vars(t).iter().map(|v| v.name.clone()).collect();
        if names.is_empty() {
            None
        } else {
            Some(self.rng.pick(&names).clone())
        }
    }

    fn declare(&mut self, name: &str, ty: T, mutable: bool) {
        self.scopes.last_mut().expect("a scope").push(Var {
            name: name.to_string(),
            ty,
            mutable,
        });
    }

    fn int_lit(&mut self) -> String {
        match self.rng.below(30) {
            0 => "9223372036854775807".into(),
            1 => "(-9223372036854775807 - 1)".into(),
            2 => "3037000499".into(),
            3 => format!("(-{})", self.rng.below(5)),
            _ => self.rng.below(12).to_string(),
        }
    }

    fn str_lit(&mut self) -> String {
        format!("\"{}\"", self.rng.pick(WORDS))
    }

    /// An expression of type `t`. `lit`: string literals allowed (not inside an
    /// interpolation hole, K0131).
    fn expr(&mut self, t: T, depth: u32, lit: bool) -> String {
        if depth == 0 || self.rng.chance(25) {
            if let Some(v) = self.var_of(t) {
                return v;
            }
        }
        let d = depth.saturating_sub(1);
        if depth == 0 {
            return self.leaf(t, lit);
        }
        // A call to an earlier function returning `t`.
        if self.rng.chance(15) {
            let fs: Vec<usize> = (0..self.fns.len())
                .filter(|&i| self.fns[i].ret == t)
                .collect();
            if !fs.is_empty() {
                let i = *self.rng.pick(&fs);
                let (fname, params) = (self.fns[i].name.clone(), self.fns[i].params.clone());
                let args: Vec<String> = params.iter().map(|p| self.expr(*p, d, lit)).collect();
                return format!("{fname}({})", args.join(", "));
            }
        }
        if self.rng.chance(8) {
            let c = self.expr(T::Bool, d, lit);
            let a = self.expr(t, d, lit);
            let b = self.expr(t, d, lit);
            return format!("(if {c}: {a} else: {b})");
        }
        match t {
            T::Int => match self.rng.below(16) {
                0..=4 => {
                    let op = *self.rng.pick(&["+", "-", "*", "//", "%"]);
                    let a = self.expr(T::Int, d, lit);
                    let b = self.expr(T::Int, d, lit);
                    format!("({a} {op} {b})")
                }
                5 => format!("(-{})", self.expr(T::Int, d, lit)),
                6 => format!("{}.len()", self.expr(T::List, d, lit)),
                7 if lit => format!("{}.len_chars()", self.expr(T::Str, d, lit)),
                8 => format!(
                    "{}.or({})",
                    self.expr(T::Opt, d, lit),
                    self.expr(T::Int, d, lit)
                ),
                9 => format!("{}.a", self.expr(T::Pair, d, lit)),
                10 => format!("{}.sum()", self.expr(T::List, d, lit)),
                11 => {
                    let p = self.fresh("q");
                    let q = self.fresh("q");
                    format!(
                        "{}.fold(0, fn({p}, {q}) => {p} + {q})",
                        self.expr(T::List, d, lit)
                    )
                }
                12 => format!("{}[{}]", self.expr(T::List, d, lit), self.rng.below(2)),
                13 => format!(
                    "{}.get({}).or(0)",
                    self.expr(T::List, d, lit),
                    self.expr(T::Int, d, lit)
                ),
                _ => self.leaf(t, lit),
            },
            T::Bool => match self.rng.below(9) {
                0 => {
                    let op = *self.rng.pick(&["<", "<=", ">", ">="]);
                    format!(
                        "({} {op} {})",
                        self.expr(T::Int, d, lit),
                        self.expr(T::Int, d, lit)
                    )
                }
                1 => {
                    let candidates: Vec<T> = ALL.iter().copied().filter(|t| has_eq(*t)).collect();
                    let u = *self.rng.pick(&candidates);
                    let op = *self.rng.pick(&["==", "!="]);
                    format!("({} {op} {})", self.expr(u, d, lit), self.expr(u, d, lit))
                }
                2 => format!("(not {})", self.expr(T::Bool, d, lit)),
                3 => {
                    let op = *self.rng.pick(&["and", "or"]);
                    format!(
                        "({} {op} {})",
                        self.expr(T::Bool, d, lit),
                        self.expr(T::Bool, d, lit)
                    )
                }
                4 => format!("{}.is_empty()", self.expr(T::List, d, lit)),
                5 => format!("{}.is_some()", self.expr(T::Opt, d, lit)),
                6 if lit => format!(
                    "{}.contains({})",
                    self.expr(T::Str, d, lit),
                    self.expr(T::Str, d, lit)
                ),
                7 => format!("{}.is_ok()", self.expr(T::Res, d, lit)),
                _ => self.leaf(t, lit),
            },
            T::Str => match self.rng.below(7) {
                0 => format!(
                    "({} + {})",
                    self.expr(T::Str, d, lit),
                    self.expr(T::Str, d, lit)
                ),
                1 => format!("{}.to_string()", self.expr(T::Int, d, lit)),
                2 => format!("{}.trim()", self.expr(T::Str, d, lit)),
                3 => format!("{}.to_upper()", self.expr(T::Str, d, lit)),
                4 if lit => {
                    // An interpolation whose holes hold no string literal.
                    let a = self.expr(T::Int, d, false);
                    let b = self.expr(T::Str, d, false);
                    format!("\"<{{{a}}}|{{{b}}}>\"")
                }
                5 => format!("{}.b", self.expr(T::Pair, d, lit)),
                _ => self.leaf(t, lit),
            },
            T::List => match self.rng.below(8) {
                0 => {
                    let n = 1 + self.rng.below(3);
                    let items: Vec<String> = (0..n).map(|_| self.expr(T::Int, d, lit)).collect();
                    format!("[{}]", items.join(", "))
                }
                1 => format!(
                    "({} + {})",
                    self.expr(T::List, d, lit),
                    self.expr(T::List, d, lit)
                ),
                2 => format!(
                    "{}.append({})",
                    self.expr(T::List, d, lit),
                    self.expr(T::Int, d, lit)
                ),
                3 => {
                    let p = self.fresh("q");
                    format!("{}.map(fn({p}) => {p} * 2)", self.expr(T::List, d, lit))
                }
                4 => {
                    let p = self.fresh("q");
                    format!(
                        "{}.filter(fn({p}) => {p} % 2 == 0)",
                        self.expr(T::List, d, lit)
                    )
                }
                5 => format!("{}.take({})", self.expr(T::List, d, lit), self.rng.below(4)),
                6 => format!("{}.sorted()", self.expr(T::List, d, lit)),
                _ => self.leaf(t, lit),
            },
            T::Opt => match self.rng.below(5) {
                0 => format!("Some({})", self.expr(T::Int, d, lit)),
                1 => format!("{}.first()", self.expr(T::List, d, lit)),
                2 => format!("{}.ok()", self.expr(T::Res, d, lit)),
                3 => {
                    let p = self.fresh("q");
                    format!("{}.map(fn({p}) => {p} + 1)", self.expr(T::Opt, d, lit))
                }
                _ => self.leaf(t, lit),
            },
            T::Res => match self.rng.below(4) {
                0 => format!("ok({})", self.expr(T::Int, d, lit)),
                1 => format!("err({})", self.expr(T::Str, d, lit)),
                2 if lit => format!(
                    "{}.to_int().map_err(fn(e) => \"bad\")",
                    self.expr(T::Str, d, lit)
                ),
                _ => self.leaf(t, lit),
            },
            T::Pair => match self.rng.below(4) {
                0 if lit => format!(
                    "Pair(a: {}, b: {})",
                    self.expr(T::Int, d, lit),
                    self.expr(T::Str, d, lit)
                ),
                1 => format!(
                    "{}.with(a: {})",
                    self.expr(T::Pair, d, lit),
                    self.expr(T::Int, d, lit)
                ),
                _ => self.leaf(t, lit),
            },
            T::Shape => match self.rng.below(4) {
                0 => format!("Shape.Circle(r: {})", self.expr(T::Int, d, lit)),
                1 => format!("Shape.Square(side: {})", self.expr(T::Int, d, lit)),
                _ => self.leaf(t, lit),
            },
        }
    }

    /// A small closed expression of type `t`.
    fn leaf(&mut self, t: T, lit: bool) -> String {
        if let Some(v) = self.var_of(t) {
            if self.rng.chance(50) {
                return v;
            }
        }
        match t {
            T::Int => self.int_lit(),
            T::Bool => (if self.rng.chance(50) { "true" } else { "false" }).into(),
            T::Str if lit => self.str_lit(),
            T::Str => format!("{}.to_string()", self.int_lit()),
            T::List => format!("[{}, {}]", self.int_lit(), self.int_lit()),
            T::Opt => {
                if self.rng.chance(50) {
                    format!("Some({})", self.int_lit())
                } else {
                    format!("[{}].get({})", self.int_lit(), self.rng.below(2))
                }
            }
            T::Res => format!("ok({})", self.int_lit()),
            T::Pair => format!("Pair(a: {})", self.int_lit()),
            T::Shape => "Shape.Dot".into(),
        }
    }

    // -- statements -------------------------------------------------------------------------

    fn line(out: &mut String, indent: usize, text: &str) {
        out.push_str(&" ".repeat(indent * 4));
        out.push_str(text);
        out.push('\n');
    }

    fn block(&mut self, out: &mut String, indent: usize, n: u64, depth: u32) {
        self.scopes.push(Vec::new());
        let before = out.len();
        let count = 1 + self.rng.below(n);
        for _ in 0..count {
            self.stmt(out, indent, depth);
        }
        if out.len() == before {
            Gen::line(out, indent, "pass");
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, out: &mut String, indent: usize, depth: u32) {
        let t = *self.rng.pick(&ALL);
        match self.rng.below(12) {
            0..=3 => {
                let v = self.fresh("v");
                let e = self.expr(t, 3, true);
                let kw = if self.rng.chance(30) { "var" } else { "let" };
                Gen::line(out, indent, &format!("{kw} {v}: {} = {e}", name(t)));
                self.declare(&v, t, kw == "var");
            }
            4 => {
                // Rebind a `var` (value semantics, §10.4).
                let vars: Vec<(String, T)> = self
                    .scopes
                    .iter()
                    .flatten()
                    .filter(|v| v.mutable)
                    .map(|v| (v.name.clone(), v.ty))
                    .collect();
                if vars.is_empty() {
                    return;
                }
                let (v, vt) = self.rng.pick(&vars).clone();
                let text = match (vt, self.rng.below(3)) {
                    (T::Int, 0) => format!("{v} += {}", self.expr(T::Int, 2, true)),
                    (T::List, 0) => format!(
                        "{v}[{}] = {}",
                        self.rng.below(3),
                        self.expr(T::Int, 2, true)
                    ),
                    (T::Pair, 0) => format!("{v}.a = {}", self.expr(T::Int, 2, true)),
                    _ => format!("{v} = {}", self.expr(vt, 2, true)),
                };
                Gen::line(out, indent, &text);
            }
            5 => {
                let pt = *self.rng.pick(&[T::Int, T::Str, T::Bool]);
                let v = self.var_of(pt).unwrap_or_else(|| "\"none\"".into());
                Gen::line(out, indent, &format!("print({v})"));
            }
            6 if depth > 0 => {
                let c = self.expr(T::Bool, 2, true);
                Gen::line(out, indent, &format!("if {c}:"));
                self.block(out, indent + 1, 3, depth - 1);
                Gen::line(out, indent, "else:");
                self.block(out, indent + 1, 3, depth - 1);
            }
            7 if depth > 0 => {
                let i = self.fresh("i");
                Gen::line(
                    out,
                    indent,
                    &format!("for {i} in 0..{}:", self.rng.below(4)),
                );
                self.scopes.push(vec![Var {
                    name: i,
                    ty: T::Int,
                    mutable: false,
                }]);
                self.block(out, indent + 1, 3, depth - 1);
                self.scopes.pop();
            }
            8 => self.let_match(out, indent),
            9 if matches!(self.ret, T::Res | T::Opt) => {
                // `?` (§14.3) on the function's own carrier type.
                let v = self.fresh("v");
                let e = self.expr(self.ret, 2, true);
                Gen::line(out, indent, &format!("let {v}: Int = {e}?"));
                self.declare(&v, T::Int, false);
            }
            _ => {
                let pt = *self.rng.pick(&[T::Int, T::Str, T::Bool]);
                if let Some(v) = self.var_of(pt) {
                    Gen::line(out, indent, &format!("print(\"[{{{v}}}]\")"));
                }
            }
        }
    }

    /// `let v: T = match scrutinee:` over `Option`, `Result` or `Shape`.
    fn let_match(&mut self, out: &mut String, indent: usize) {
        let t = *self.rng.pick(&[T::Int, T::Str, T::Bool]);
        let v = self.fresh("v");
        let st = *self.rng.pick(&[T::Opt, T::Res, T::Shape]);
        let s = self.expr(st, 2, true);
        Gen::line(out, indent, &format!("let {v}: {} = match {s}:", name(t)));
        let arms: Vec<(String, Option<String>)> = match st {
            T::Opt => {
                let x = self.fresh("x");
                vec![(format!("Some({x})"), Some(x)), ("None".into(), None)]
            }
            T::Res => {
                let x = self.fresh("x");
                vec![(format!("Ok({x})"), Some(x)), ("Err(_)".into(), None)]
            }
            _ => {
                let (r, s) = (self.fresh("x"), self.fresh("x"));
                vec![
                    (format!(".Circle({r})"), Some(r)),
                    (format!(".Square({s})"), Some(s)),
                    (".Dot".into(), None),
                ]
            }
        };
        for (pat, bound) in arms {
            self.scopes.push(Vec::new());
            if let Some(b) = &bound {
                self.declare(b, T::Int, false);
            }
            let guard = if bound.is_some() && self.rng.chance(20) {
                format!(" if {}", self.expr(T::Bool, 1, true))
            } else {
                String::new()
            };
            let e = self.expr(t, 2, true);
            Gen::line(out, indent + 1, &format!("case {pat}{guard}: {e}"));
            if !guard.is_empty() {
                // A guarded arm does not count toward exhaustiveness (§11 rule 2).
                let e = self.expr(t, 1, true);
                Gen::line(out, indent + 1, &format!("case {pat}: {e}"));
            }
            self.scopes.pop();
        }
        self.declare(&v, t, false);
    }

    fn function(&mut self, out: &mut String) {
        let fname = self.fresh("f");
        let n = self.rng.below(3);
        let params: Vec<(String, T)> = (0..n)
            .map(|_| (self.fresh("p"), *self.rng.pick(&ALL)))
            .collect();
        let ret = *self.rng.pick(&ALL);
        let list: Vec<String> = params
            .iter()
            .map(|(p, t)| format!("{p}: {}", name(*t)))
            .collect();
        Gen::line(
            out,
            0,
            &format!("fn {fname}({}) -> {}:", list.join(", "), name(ret)),
        );
        self.ret = ret;
        self.scopes.push(
            params
                .iter()
                .map(|(p, t)| Var {
                    name: p.clone(),
                    ty: *t,
                    mutable: false,
                })
                .collect(),
        );
        self.block(out, 1, 4, 2);
        let e = self.expr(ret, 3, true);
        Gen::line(out, 1, &format!("return {e}"));
        self.scopes.pop();
        out.push('\n');
        self.fns.push(Sig {
            name: fname,
            params: params.into_iter().map(|(_, t)| t).collect(),
            ret,
        });
    }

    fn program(seed: u64) -> String {
        let mut g = Gen {
            rng: Rng(seed),
            next: 0,
            fns: Vec::new(),
            scopes: Vec::new(),
            ret: T::Int,
        };
        let mut out = PREAMBLE.to_string();
        for _ in 0..g.rng.below(4) {
            g.function(&mut out);
        }
        Gen::line(&mut out, 0, "fn main():");
        g.ret = T::Int;
        g.scopes.push(Vec::new());
        for _ in 0..(3 + g.rng.below(8)) {
            g.stmt(&mut out, 1, 2);
        }
        // Call every function once, through a local (no string literal in a hole).
        for i in 0..g.fns.len() {
            let (fname, params, ret) =
                (g.fns[i].name.clone(), g.fns[i].params.clone(), g.fns[i].ret);
            let args: Vec<String> = params.iter().map(|p| g.expr(*p, 2, true)).collect();
            let v = g.fresh("v");
            Gen::line(
                &mut out,
                1,
                &format!("let {v}: {} = {fname}({})", name(ret), args.join(", ")),
            );
            if matches!(ret, T::Int | T::Str | T::Bool) {
                Gen::line(&mut out, 1, &format!("print({v})"));
            }
        }
        Gen::line(&mut out, 1, "print(\"end\")");
        out
    }
}

// -- the property ----------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Seen {
    /// The checker's error codes and first message.
    Rejected(Vec<&'static str>, String),
    Unsupported(String),
    Ran {
        exit: u8,
        stdout: Vec<u8>,
    },
    Internal(String),
}

fn observe(path: &str, src: &str) -> Seen {
    let mut out = Vec::new();
    match run_interp(path, src.as_bytes(), &mut out).expect("a Kalam path") {
        RunOutcome::Rejected(c) => Seen::Rejected(
            c.diagnostics
                .iter()
                .filter(|d| d.is_error())
                .map(|d| d.code)
                .collect(),
            c.render_human(Skin::En)
                .lines()
                .take(12)
                .collect::<Vec<_>>()
                .join(
                    "
",
                ),
        ),
        RunOutcome::Unsupported(why) => Seen::Unsupported(why),
        RunOutcome::Internal(why) => Seen::Internal(why),
        other => Seen::Ran {
            exit: other.exit_code(),
            stdout: out,
        },
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
fn well_typed_programs_do_not_go_wrong() {
    let cases = env_u64("KALAM_INTERP_PROPS_CASES", 300);
    let base = env_u64("KALAM_INTERP_PROPS_SEED", 0x006b_616c_616d);
    let (mut ran, mut panicked, mut rejected, mut parity) = (0, 0, 0, 0);
    let mut rejected_example = None;
    let mut reject_codes: std::collections::BTreeMap<&str, u32> = Default::default();
    for i in 0..cases {
        let seed = base.wrapping_add(i);
        let src = Gen::program(seed);
        match observe("main.kl", &src) {
            Seen::Rejected(codes, first) => {
                rejected += 1;
                for c in codes {
                    *reject_codes.entry(c).or_insert(0) += 1;
                }
                rejected_example.get_or_insert((seed, first, src));
                continue;
            }
            Seen::Internal(why) => {
                panic!("seed {seed}: a well-typed program went wrong: {why}\n{src}")
            }
            Seen::Unsupported(why) => {
                panic!("seed {seed}: the generator used something the interpreter refuses: {why}\n{src}")
            }
            Seen::Ran { exit, stdout } => {
                assert!(exit == 0 || exit == 101, "seed {seed}: exit {exit}\n{src}");
                ran += 1;
                if exit == 101 {
                    panicked += 1;
                }
                // P5: the same program in the other skins behaves identically.
                if i % 8 == 0 {
                    for (skin, path) in [(Skin::Ar, "main.ar.kl"), (Skin::Franco, "main.franco.kl")]
                    {
                        let converted =
                            convert_source("main.kl", &src, skin, &FmtConfig::default())
                                .unwrap_or_else(|e| panic!("seed {seed}: convert failed: {e:?}"));
                        let again = observe(path, &converted.text);
                        assert_eq!(
                            again,
                            Seen::Ran {
                                exit,
                                stdout: stdout.clone()
                            },
                            "seed {seed}: {path} behaves differently\n{src}"
                        );
                    }
                    parity += 1;
                }
            }
        }
    }
    eprintln!(
        "well-typed property: {cases} programs, {ran} ran ({panicked} with a defined panic), {rejected} rejected by the checker {reject_codes:?}, {parity} checked in all three skins"
    );
    // The generator must mostly produce well-typed programs, or the property is vacuous.
    assert!(
        ran * 10 >= cases * 8,
        "only {ran}/{cases} generated programs type-checked; first rejected: {rejected_example:?}"
    );
}
