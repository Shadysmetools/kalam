//! Q33 (FOUNDER-QUEUE): module constants are evaluated **eagerly, at program start, in
//! dependency order** (spec §13.4), not on first use. These tests build KIR by hand, so
//! they pin the interpreter's start-up order whatever the type checker later rejects at
//! compile time (K1204 rejects a constant that would panic; the interpreter must still
//! never run `main` before every constant has a value).

use kalam_base::{FileId, FileSpan, Span};
use kalam_interp::{run, Exit, InterpError, Options};
use kalam_ir::{
    Block, Callee, ConstDecl, Expr, ExprKind, Function, Instance, PreludeFn, PrimOp, Program, Stmt,
};
use kalam_resolve::DefId;
use kalam_typeck::Ty;

fn at(start: u32) -> Span {
    Span {
        start,
        end: start + 1,
    }
}

fn int(i: i64, start: u32) -> Expr {
    Expr::new(ExprKind::Int(i), Ty::Int, at(start))
}

fn prim(op: PrimOp, args: Vec<Expr>, ty: Ty, start: u32) -> Expr {
    Expr::new(ExprKind::Prim { op, args }, ty, at(start))
}

fn constant(def: u32, start: u32) -> Expr {
    Expr::new(ExprKind::Const(DefId(def)), Ty::Int, at(start))
}

fn print(text: Expr) -> Stmt {
    Stmt::Expr(Expr::new(
        ExprKind::Call {
            callee: Callee::Prelude(PreludeFn::Print),
            args: vec![text],
            type_args: Vec::new(),
            suspends: false,
        },
        Ty::Unit,
        at(0),
    ))
}

fn show(e: Expr) -> Expr {
    prim(PrimOp::IntShow, vec![e], Ty::String, 0)
}

fn decl(def: u32, name: &str, value: Expr) -> ConstDecl {
    ConstDecl {
        def: DefId(def),
        name: name.to_string(),
        file: FileId(0),
        ty: Ty::Int,
        value,
    }
}

/// A program whose `main` prints "before", then each listed constant.
fn program(consts: Vec<ConstDecl>, printed: &[u32]) -> Program {
    let mut stmts = vec![print(Expr::new(
        ExprKind::Str("before".into()),
        Ty::String,
        at(0),
    ))];
    for d in printed {
        stmts.push(print(show(constant(*d, 0))));
    }
    Program {
        package: "main".into(),
        functions: vec![Function {
            id: 0,
            def: DefId(1),
            name: "main".into(),
            generics: 0,
            params: Vec::new(),
            locals: Vec::new(),
            ret: Ty::Unit,
            body: Block { stmts },
            suspends: false,
            instance: Instance::Sync,
            exported: false,
            span: FileSpan::new(FileId(0), at(0)),
        }],
        consts,
        entry: Some(0),
        ..Program::default()
    }
}

fn exec(p: &Program) -> (Result<Exit, InterpError>, String) {
    let mut out = Vec::new();
    let r = run(p, &mut out, &Options::default());
    (r, String::from_utf8(out).expect("UTF-8"))
}

fn div_by_zero(start: u32) -> Expr {
    prim(
        PrimOp::IntFloorDiv,
        vec![int(1, start), int(0, start + 4)],
        Ty::Int,
        start,
    )
}

#[test]
fn a_panicking_constant_stops_the_program_before_main_runs() {
    // let BOOM = 1 // 0   (used by main, after a print)
    let p = program(vec![decl(10, "BOOM", div_by_zero(11))], &[10]);
    let (r, out) = exec(&p);
    assert_eq!(out, "", "nothing in `main` runs before the constants");
    match r {
        Err(InterpError::Panic { message, span, .. }) => {
            assert_eq!(message, "division by zero");
            assert_eq!(span, at(11));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unused_panicking_constant_still_panics_at_start() {
    let p = program(vec![decl(10, "BOOM", div_by_zero(11))], &[]);
    let (r, out) = exec(&p);
    assert_eq!(out, "");
    assert!(
        matches!(&r, Err(InterpError::Panic { message, .. }) if message == "division by zero"),
        "{r:?}"
    );
}

#[test]
fn constants_run_in_dependency_order_not_declaration_order() {
    // let A = B + 1   (declared first, needs B)
    // let B = 1 // 0  (its panic is reported at B's span)
    let a = prim(
        PrimOp::IntAdd,
        vec![constant(11, 40), int(1, 44)],
        Ty::Int,
        40,
    );
    let p = program(vec![decl(10, "A", a), decl(11, "B", div_by_zero(60))], &[]);
    let (r, out) = exec(&p);
    assert_eq!(out, "");
    match r {
        Err(InterpError::Panic { span, .. }) => assert_eq!(span, at(60)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn of_two_independent_panicking_constants_the_first_declared_panics() {
    let overflow = prim(
        PrimOp::IntAdd,
        vec![int(i64::MAX, 31), int(1, 35)],
        Ty::Int,
        30,
    );
    let p = program(
        vec![decl(10, "X", overflow), decl(11, "Y", div_by_zero(50))],
        &[],
    );
    let (r, _) = exec(&p);
    assert!(
        matches!(&r, Err(InterpError::Panic { message, .. }) if message == "integer overflow"),
        "{r:?}"
    );
}

#[test]
fn each_constant_is_evaluated_once_and_main_sees_the_values() {
    let a = prim(PrimOp::IntMul, vec![int(6, 0), int(7, 0)], Ty::Int, 0);
    let b = prim(PrimOp::IntAdd, vec![constant(10, 0), int(1, 0)], Ty::Int, 0);
    let p = program(vec![decl(10, "A", a), decl(11, "B", b)], &[10, 11, 10]);
    let (r, out) = exec(&p);
    assert_eq!(r, Ok(Exit::Done));
    assert_eq!(out, "before\n42\n43\n42\n");
}

#[test]
fn a_constant_that_needs_itself_is_refused_not_a_stack_overflow() {
    // let A = A + 1: K0405-style cycles are rejected by the checker; the interpreter
    // still fails closed if one reaches it.
    let a = prim(PrimOp::IntAdd, vec![constant(10, 0), int(1, 0)], Ty::Int, 0);
    let p = program(vec![decl(10, "A", a)], &[]);
    let (r, out) = exec(&p);
    assert_eq!(out, "");
    assert!(
        matches!(&r, Err(InterpError::Unsupported(m)) if m.contains("`A`")),
        "{r:?}"
    );
}

#[test]
fn a_program_the_interpreter_refuses_runs_no_constant_first() {
    // The refusal (an unsupported intrinsic) wins over a constant that would panic.
    let mut p = program(vec![decl(10, "BOOM", div_by_zero(11))], &[]);
    p.functions[0].body.stmts.push(Stmt::Expr(Expr::new(
        ExprKind::Call {
            callee: Callee::Intrinsic("std/time.now".into()),
            args: Vec::new(),
            type_args: Vec::new(),
            suspends: false,
        },
        Ty::Unit,
        at(0),
    )));
    let (r, out) = exec(&p);
    assert_eq!(out, "");
    assert!(matches!(r, Err(InterpError::Unsupported(_))), "{r:?}");
}
