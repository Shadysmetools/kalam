//! Lowering tests: the spec §28.1–§28.2 programs go HIR → typeck → KIR → verifier, and a
//! tiny test-only evaluator runs the KIR and compares stdout with the expected output
//! (computed here, so no compiler source depends on the eval corpus).
//! (The real reference interpreter is a separate crate; this evaluator covers only the
//! constructs these tests lower, to pin down lowering semantics.)

use std::collections::HashMap;

use kalam_typeck::builder::{fixtures, Builder};
use kalam_typeck::check_program;
use kalam_typeck::hir::{BinOp, BuiltinType, HirProgram, LangItem, PatKind, StmtKind};

use super::*;
use crate::verify::verify;

fn lower_ok(prog: &HirProgram) -> Program {
    let tc = check_program(prog);
    assert!(tc.errors.is_empty(), "{:#?}", tc.errors);
    let kir = lower_program(prog, &tc, "test").expect("lowers");
    let problems = verify(&kir);
    assert!(problems.is_empty(), "{problems:#?}");
    kir
}

/// FizzBuzz from 1 to 100, one line each (spec §28.2).
fn fizzbuzz_stdout() -> String {
    (1..=100)
        .map(|n| match (n % 3, n % 5) {
            (0, 0) => "FizzBuzz\n".to_string(),
            (0, _) => "Fizz\n".to_string(),
            (_, 0) => "Buzz\n".to_string(),
            _ => format!("{n}\n"),
        })
        .collect()
}

// -- a minimal evaluator for the lowered subset ------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum V {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Unit,
    Tuple(Vec<V>),
    Variant(u32, Vec<V>),
    Record(Vec<V>),
    List(Vec<V>),
}

enum Flow {
    Next,
    Break,
    Continue,
    Return(V),
}

struct Eval<'a> {
    program: &'a Program,
    out: String,
}

type Frame = HashMap<LocalId, V>;

impl Eval<'_> {
    fn run(program: &Program) -> String {
        let mut e = Eval {
            program,
            out: String::new(),
        };
        let entry = program.entry.expect("entry");
        e.call(entry, Vec::new());
        e.out
    }

    fn call(&mut self, id: FnId, args: Vec<V>) -> V {
        let f = self.program.function(id).expect("fn");
        let mut frame: Frame = f.params.iter().map(|(l, _)| *l).zip(args).collect();
        match self.block(&f.body, &mut frame) {
            Flow::Return(v) => v,
            _ => V::Unit,
        }
    }

    fn block(&mut self, b: &Block, fr: &mut Frame) -> Flow {
        for s in &b.stmts {
            match self.stmt(s, fr) {
                Flow::Next => {}
                other => return other,
            }
        }
        Flow::Next
    }

    fn stmt(&mut self, s: &Stmt, fr: &mut Frame) -> Flow {
        match s {
            Stmt::Let { local, value, .. } | Stmt::Assign { local, value } => {
                let v = self.expr(value, fr);
                fr.insert(*local, v);
            }
            Stmt::Expr(e) => {
                self.expr(e, fr);
            }
            Stmt::If {
                cond,
                then,
                otherwise,
            } => {
                let b = self.expr(cond, fr) == V::Bool(true);
                return self.block(if b { then } else { otherwise }, fr);
            }
            Stmt::Loop { body } => loop {
                match self.block(body, fr) {
                    Flow::Break => break,
                    Flow::Return(v) => return Flow::Return(v),
                    _ => {}
                }
            },
            Stmt::ForRange {
                local,
                lo,
                hi,
                inclusive,
                body,
            } => {
                let (V::Int(lo), V::Int(hi)) = (self.expr(lo, fr), self.expr(hi, fr)) else {
                    panic!("range bounds")
                };
                let end = if *inclusive {
                    hi as i128 + 1
                } else {
                    hi as i128
                };
                for i in lo as i128..end {
                    fr.insert(*local, V::Int(i as i64));
                    match self.block(body, fr) {
                        Flow::Break => break,
                        Flow::Return(v) => return Flow::Return(v),
                        _ => {}
                    }
                }
            }
            Stmt::ForEach { local, iter, body } => {
                let V::List(items) = self.expr(iter, fr) else {
                    panic!("list")
                };
                for item in items {
                    fr.insert(*local, item);
                    match self.block(body, fr) {
                        Flow::Break => break,
                        Flow::Return(v) => return Flow::Return(v),
                        _ => {}
                    }
                }
            }
            Stmt::Break => return Flow::Break,
            Stmt::Continue => return Flow::Continue,
            Stmt::Return(e) => return Flow::Return(self.expr(e, fr)),
            Stmt::Switch { .. } => panic!("not produced by the M2 skeleton"),
        }
        Flow::Next
    }

    fn expr(&mut self, e: &Expr, fr: &mut Frame) -> V {
        match &e.kind {
            ExprKind::Local(l) => fr[l].clone(),
            ExprKind::Int(i) => V::Int(*i),
            ExprKind::Float(f) => V::Float(*f),
            ExprKind::Str(s) => V::Str(s.clone()),
            ExprKind::Bool(b) => V::Bool(*b),
            ExprKind::Unit => V::Unit,
            ExprKind::Tuple(es) => V::Tuple(es.iter().map(|x| self.expr(x, fr)).collect()),
            ExprKind::List(es) => V::List(es.iter().map(|x| self.expr(x, fr)).collect()),
            ExprKind::Record { fields, .. } => {
                V::Record(fields.iter().map(|x| self.expr(x, fr)).collect())
            }
            ExprKind::Variant { index, fields, .. } => {
                V::Variant(*index, fields.iter().map(|x| self.expr(x, fr)).collect())
            }
            ExprKind::Field { base, index } | ExprKind::TupleField { base, index } => {
                match self.expr(base, fr) {
                    V::Record(vs) | V::Tuple(vs) => vs[*index as usize].clone(),
                    other => panic!("field of {other:?}"),
                }
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                if self.expr(cond, fr) == V::Bool(true) {
                    self.expr(then, fr)
                } else {
                    self.expr(otherwise, fr)
                }
            }
            ExprKind::Block { stmts, value } => {
                for s in stmts {
                    self.stmt(s, fr);
                }
                self.expr(value, fr)
            }
            ExprKind::Call { callee, args, .. } => {
                let args: Vec<V> = args.iter().map(|a| self.expr(a, fr)).collect();
                match callee {
                    Callee::Prelude(PreludeFn::Print) => {
                        let V::Str(s) = &args[0] else { panic!() };
                        self.out.push_str(s);
                        self.out.push('\n');
                        V::Unit
                    }
                    Callee::Fn(id) => self.call(*id, args),
                    other => panic!("callee {other:?}"),
                }
            }
            ExprKind::Prim { op, args } => {
                let a: Vec<V> = args.iter().map(|x| self.expr(x, fr)).collect();
                prim_eval(*op, &a)
            }
            ExprKind::Panic(msg) => panic!("kalam panic: {:?}", self.expr(msg, fr)),
            other => panic!("not in the test evaluator: {other:?}"),
        }
    }
}

fn prim_eval(op: PrimOp, a: &[V]) -> V {
    use PrimOp::*;
    let int = |v: &V| match v {
        V::Int(i) => *i,
        _ => panic!("int"),
    };
    let checked = |r: Option<i64>| V::Int(r.expect("kalam panic: Int overflow"));
    match op {
        IntAdd => checked(int(&a[0]).checked_add(int(&a[1]))),
        IntSub => checked(int(&a[0]).checked_sub(int(&a[1]))),
        IntMul => checked(int(&a[0]).checked_mul(int(&a[1]))),
        IntFloorDiv => {
            let (x, y) = (int(&a[0]), int(&a[1]));
            assert!(y != 0, "kalam panic: division by zero");
            let q = x.checked_div(y).expect("kalam panic: Int overflow");
            V::Int(if (x % y != 0) && ((x < 0) != (y < 0)) {
                q - 1
            } else {
                q
            })
        }
        IntMod => {
            let (x, y) = (int(&a[0]), int(&a[1]));
            assert!(y != 0, "kalam panic: division by zero");
            let r = x.wrapping_rem(y);
            V::Int(if r != 0 && ((r < 0) != (y < 0)) {
                r + y
            } else {
                r
            })
        }
        IntNeg => checked(int(&a[0]).checked_neg()),
        IntEq => V::Bool(int(&a[0]) == int(&a[1])),
        IntLt => V::Bool(int(&a[0]) < int(&a[1])),
        IntLe => V::Bool(int(&a[0]) <= int(&a[1])),
        IntGt => V::Bool(int(&a[0]) > int(&a[1])),
        IntGe => V::Bool(int(&a[0]) >= int(&a[1])),
        IntShow => V::Str(int(&a[0]).to_string()),
        StrConcat => match (&a[0], &a[1]) {
            (V::Str(x), V::Str(y)) => V::Str(format!("{x}{y}")),
            _ => panic!("str"),
        },
        StrEq => V::Bool(a[0] == a[1]),
        BoolShow => V::Str(format!("{}", a[0] == V::Bool(true))),
        Not => V::Bool(a[0] == V::Bool(false)),
        other => panic!("prim {other:?} not in the test evaluator"),
    }
}

// -- tests ---------------------------------------------------------------------------------

#[test]
fn hello_world_lowers_and_prints_the_expected_output() {
    let kir = lower_ok(&fixtures::hello_world());
    assert_eq!(Eval::run(&kir), "Hello, world!\nمرحبا بالعالم\n");
}

#[test]
fn fizzbuzz_lowers_to_a_range_loop_and_prints_the_expected_output() {
    let kir = lower_ok(&fixtures::fizzbuzz());
    let main = kir.function(kir.entry.unwrap()).unwrap();
    assert!(matches!(
        main.body.stmts[0],
        Stmt::ForRange {
            inclusive: true,
            ..
        }
    ));
    assert_eq!(Eval::run(&kir), fizzbuzz_stdout());
}

#[test]
fn int_semantics_follow_the_spec() {
    // Floor division and modulo follow the divisor's sign (§9.2).
    assert_eq!(
        prim_eval(PrimOp::IntFloorDiv, &[V::Int(-7), V::Int(2)]),
        V::Int(-4)
    );
    assert_eq!(
        prim_eval(PrimOp::IntMod, &[V::Int(-7), V::Int(2)]),
        V::Int(1)
    );
    assert_eq!(
        prim_eval(PrimOp::IntMod, &[V::Int(7), V::Int(-2)]),
        V::Int(-1)
    );
}

/// `fn add(a: Int, b: Int) -> Int => a - b` called as `add(b: 1, a: 10)`: arguments are
/// evaluated in source order into temporaries, then passed by position.
#[test]
fn named_arguments_become_positional_through_temporaries() {
    let mut b = Builder::new("app/main");
    let add = b.def();
    let i = b.builtin(BuiltinType::Int);
    let pa = b.param("a", i.clone());
    let pb = b.param("b", i.clone());
    let (ra, rb) = (b.local_ref(pa.local), b.local_ref(pb.local));
    let body = b.bin(BinOp::Sub, ra, rb);
    b.func(
        add,
        "add",
        vec![pa, pb],
        Some(i),
        kalam_typeck::hir::BodyValue::Expr(body),
    );
    let callee = b.def_ref(add);
    let (one, ten) = (b.int(1), b.int(10));
    let call = b.call_named(callee, vec![("b", one), ("a", ten)]);
    let hole = kalam_typeck::hir::StrPart::Hole(call);
    let s = b.interp(vec![hole]);
    let p = b.print(s);
    b.main(vec![p]);
    let kir = lower_ok(&b.finish());
    assert_eq!(Eval::run(&kir), "9\n");
}

/// `match n: case 1: "one" case 2..=9: "small" case _: "big"` in value position, with
/// `and`/`or` short-circuiting and a `while` loop.
#[test]
fn literal_match_and_while_lower_to_if_chains_and_loops() {
    let mut b = Builder::new("app/main");
    let zero = b.int(0);
    let (s1, n) = b.var("n", None, zero);
    // while n < 12:
    let nr = b.local_ref(n);
    let twelve = b.int(12);
    let cond = b.bin(BinOp::Lt, nr, twelve);
    let mut arms = Vec::new();
    let p = b.pat_int(1);
    let v = b.str("one");
    let v = b.expr_stmt(v);
    arms.push((p, b.block(vec![v])));
    let p = b.pat(PatKind::Range {
        lo: 2,
        hi: 9,
        inclusive: true,
    });
    let v = b.str("small");
    let v = b.expr_stmt(v);
    arms.push((p, b.block(vec![v])));
    let p = b.pat_wild();
    let v = b.str("big");
    let v = b.expr_stmt(v);
    arms.push((p, b.block(vec![v])));
    let nr = b.local_ref(n);
    let m = b.match_expr(nr, arms);
    let (s_label, label) = b.let_("label", None, m);
    let lr = b.local_ref(label);
    let print = b.print(lr);
    let nr = b.local_ref(n);
    let three = b.int(3);
    let step = b.assign(nr, kalam_typeck::hir::AssignOp::Add, three);
    let body = b.block(vec![s_label, print, step]);
    let w = b.stmt(StmtKind::While { cond, body });
    b.main(vec![s1, w]);
    let kir = lower_ok(&b.finish());
    // n = 0, 3, 6, 9
    assert_eq!(Eval::run(&kir), "big\nsmall\nsmall\nsmall\n");
}

#[test]
fn lowering_refuses_programs_with_type_errors_and_unsupported_constructs() {
    let mut b = Builder::new("app/main");
    let x = b.int(1);
    let y = b.str("a");
    let bad = b.bin(BinOp::Add, x, y);
    let s = b.expr_stmt(bad);
    b.main(vec![s]);
    let prog = b.finish();
    let tc = check_program(&prog);
    assert!(matches!(
        lower_program(&prog, &tc, "t"),
        Err(LowerError::TypeErrors(_))
    ));

    // `print(Some(1))`: `Show` for compound types is not lowered yet.
    let mut b = Builder::new("app/main");
    let some = b.lang(LangItem::Some);
    let one = b.int(1);
    let call = b.call(some, vec![one]);
    let p = b.print(call);
    b.main(vec![p]);
    let prog = b.finish();
    let tc = check_program(&prog);
    assert!(tc.errors.is_empty(), "{:#?}", tc.errors);
    assert!(matches!(
        lower_program(&prog, &tc, "t"),
        Err(LowerError::Unsupported { .. })
    ));
}
