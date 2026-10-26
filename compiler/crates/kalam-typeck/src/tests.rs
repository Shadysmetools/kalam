//! End-to-end checker tests on hand-built programs (spec §28.1–§28.2 and the M2 subset).

use kalam_base::Span;
use kalam_resolve::DefId;

use crate::builder::{fixtures, Builder};
use crate::check::{check_program, TypeckResults};
use crate::hir::*;
use crate::traits::{BuiltinTrait, TraitId};
use crate::ty::{SinkType, Suspension, Ty};

fn check(b: Builder) -> TypeckResults {
    check_program(&b.finish())
}

fn ok(b: Builder) -> TypeckResults {
    let r = check(b);
    assert!(r.errors.is_empty(), "unexpected errors: {:#?}", r.errors);
    r
}

fn codes(b: Builder) -> Vec<&'static str> {
    check(b).codes()
}

fn int_ty(b: &Builder) -> Type {
    b.builtin(BuiltinType::Int)
}

/// `fn main(): <stmts>` where `stmts` is built by `f`.
fn main_with(f: impl FnOnce(&mut Builder) -> Vec<Stmt>) -> Builder {
    let mut b = Builder::new("app/main");
    let stmts = f(&mut b);
    b.main(stmts);
    b
}

fn let_stmt(b: &mut Builder, ty: Option<Type>, value: Expr) -> Stmt {
    b.let_("x", ty, value).0
}

// ---------------------------------------------------------------------------------------
// Corpus tasks
// ---------------------------------------------------------------------------------------

/// Spec §28.1: `print` calls with string literals.
#[test]
fn hello_world_type_checks() {
    let prog = fixtures::hello_world();
    let r = check_program(&prog);
    assert!(r.errors.is_empty(), "{:#?}", r.errors);
    let main = prog.item_ids()[0];
    let ItemKind::Fn(f) = &prog.item(main).unwrap().kind else {
        panic!()
    };
    let BodyValue::Block(block) = &f.body.as_ref().unwrap().value else {
        panic!()
    };
    let StmtKind::Expr(call) = &block.stmts[0].kind else {
        panic!()
    };
    assert_eq!(r.expr_ty(main, call), Some(&Ty::Unit));
    assert_eq!(
        r.signatures[&main].ty,
        Ty::func(vec![], Ty::Unit, Suspension::Param(0))
    );
}

/// Spec §28.2: `for n in 1..=100` with an `if`/`else if` chain on `n % k == 0` and an
/// interpolated `"{n}"`.
#[test]
fn fizzbuzz_type_checks() {
    let prog = fixtures::fizzbuzz();
    let r = check_program(&prog);
    assert!(r.errors.is_empty(), "{:#?}", r.errors);
    let main = prog.item_ids()[0];
    assert_eq!(r.local_types[&(main, LocalId(0))], Ty::Int);
}

// ---------------------------------------------------------------------------------------
// Literals and operators
// ---------------------------------------------------------------------------------------

#[test]
fn int_literal_range_is_64_bit_and_unary_minus_admits_min() {
    // 2^63 as an Int is K0126; -2^63 is legal; 2^63 as a BigInt is fine.
    let b = main_with(|b| {
        let v = b.int(1 << 63);
        vec![let_stmt(b, None, v)]
    });
    assert_eq!(codes(b), vec!["K0126"]);
    ok(main_with(|b| {
        let v = b.int(1 << 63);
        let v = b.neg(v);
        vec![let_stmt(b, None, v)]
    }));
    ok(main_with(|b| {
        let v = b.int(1 << 63);
        let t = b.builtin(BuiltinType::BigInt);
        vec![let_stmt(b, Some(t), v)]
    }));
    let b = main_with(|b| {
        let v = b.int((1 << 63) + 1);
        let v = b.neg(v);
        vec![let_stmt(b, None, v)]
    });
    assert_eq!(codes(b), vec!["K0126"]);
    let b = main_with(|b| {
        let v = b.e(ExprKind::Int(IntLit::Large(format!("1{}", "0".repeat(40)))));
        vec![let_stmt(b, None, v)]
    });
    assert_eq!(codes(b), vec!["K0126"]);
}

#[test]
fn literals_take_the_expected_type() {
    let mut b = Builder::new("app/main");
    let v = b.float("19.99");
    let dec = b.builtin(BuiltinType::Decimal);
    let (s1, d) = b.let_("price", Some(dec), v);
    let v = b.int(3);
    let fl = b.builtin(BuiltinType::Float);
    let (s2, f) = b.let_("f", Some(fl), v);
    let main = b.main(vec![s1, s2]);
    let r = ok(b);
    assert_eq!(r.local_types[&(main, d)], Ty::Decimal);
    assert_eq!(r.local_types[&(main, f)], Ty::Float);
}

#[test]
fn mixing_int_and_float_is_k0601_but_literals_adapt() {
    let b = main_with(|b| {
        let one = b.int(1);
        let (s1, x) = b.let_("x", None, one);
        let xr = b.local_ref(x);
        let h = b.float("1.5");
        let sum = b.bin(BinOp::Add, xr, h);
        let (s2, _) = b.let_("y", None, sum);
        vec![s1, s2]
    });
    assert_eq!(codes(b), vec!["K0601"]);
    // `2 * a` with `a: Float`: the literal takes `Float` from the other operand.
    let mut b = Builder::new("app/main");
    let two = b.float("2.0");
    let (s1, a) = b.let_("a", None, two);
    let two = b.int(2);
    let ar = b.local_ref(a);
    let prod = b.bin(BinOp::Mul, two, ar);
    let (s2, p) = b.let_("p", None, prod);
    let main = b.main(vec![s1, s2]);
    let r = ok(b);
    assert_eq!(r.local_types[&(main, p)], Ty::Float);
}

#[test]
fn int_division_gives_float_and_decimal_has_no_slash() {
    let mut b = Builder::new("app/main");
    let (x, y) = (b.int(7), b.int(2));
    let q = b.bin(BinOp::Div, x, y);
    let (s1, q) = b.let_("q", None, q);
    let main = b.main(vec![s1]);
    let r = ok(b);
    assert_eq!(r.local_types[&(main, q)], Ty::Float);
    let b = main_with(|b| {
        let dec = b.builtin(BuiltinType::Decimal);
        let one = b.int(1);
        let (s1, d) = b.let_("d", Some(dec), one);
        let dr = b.local_ref(d);
        let two = b.int(2);
        let q = b.bin(BinOp::Div, dr, two);
        vec![s1, let_stmt(b, None, q)]
    });
    assert_eq!(codes(b), vec!["K0603"]);
}

#[test]
fn equality_needs_same_eq_type_but_float_has_ieee_equality() {
    // `1 == "a"`
    let b = main_with(|b| {
        let (l, r) = (b.int(1), b.str("a"));
        let eq = b.bin(BinOp::Eq, l, r);
        vec![let_stmt(b, None, eq)]
    });
    assert_eq!(codes(b), vec!["K0602"]);
    // `i == 1.0` with `i: Int` compares two types: §9.8 says K0602 ("`1 == 1.0` is
    // K0602"), not the arithmetic code K0601.
    let b = main_with(|b| {
        let one = b.int(1);
        let (s1, x) = b.let_("i", None, one);
        let xr = b.local_ref(x);
        let f = b.float("1.0");
        let eq = b.bin(BinOp::Eq, xr, f);
        vec![s1, let_stmt(b, None, eq)]
    });
    assert_eq!(codes(b), vec!["K0602"]);
    // Float == Float is fine.
    ok(main_with(|b| {
        let (l, r) = (b.float("0.1"), b.float("0.2"));
        let eq = b.bin(BinOp::Eq, l, r);
        vec![let_stmt(b, None, eq)]
    }));
    // A list of Floats is not `Eq`.
    let b = main_with(|b| {
        let x = b.float("1.0");
        let l = b.list(vec![x]);
        let y = b.float("1.0");
        let r = b.list(vec![y]);
        let eq = b.bin(BinOp::Eq, l, r);
        vec![let_stmt(b, None, eq)]
    });
    assert_eq!(codes(b), vec!["K0602"]);
}

#[test]
fn conditions_must_be_bool() {
    let b = main_with(|b| {
        let c = b.int(1);
        let s = b.str("x");
        let p = b.print(s);
        let body = b.block(vec![p]);
        let i = b.if_expr(vec![(c, body)], None);
        vec![b.expr_stmt(i)]
    });
    let r = check(b);
    assert_eq!(r.codes(), vec!["K0600"]);
    assert!(matches!(
        &r.errors[0].kind,
        crate::TypeErrorKind::Mismatch {
            expected: Ty::Bool,
            found: Ty::Int
        }
    ));
}

#[test]
fn string_indexing_is_k0650() {
    let b = main_with(|b| {
        let s = b.str("abc");
        let i = b.int(0);
        let ix = b.index(s, i);
        vec![let_stmt(b, None, ix)]
    });
    assert_eq!(codes(b), vec!["K0650"]);
}

// ---------------------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------------------

#[test]
fn let_is_immutable_and_compound_assignment_keeps_the_type() {
    let b = main_with(|b| {
        let one = b.int(1);
        let (s1, x) = b.let_("x", None, one);
        let xr = b.local_ref(x);
        let two = b.int(2);
        vec![s1, b.assign(xr, AssignOp::Assign, two)]
    });
    assert_eq!(codes(b), vec!["K0710"]);
    ok(main_with(|b| {
        let zero = b.int(0);
        let (s1, t) = b.var("total", None, zero);
        let tr = b.local_ref(t);
        let one = b.int(1);
        vec![s1, b.assign(tr, AssignOp::Add, one)]
    }));
    // `total /= 2` would make `total` a Float: a mismatch, expected `Int`, found `Float`
    // (FOUNDER-QUEUE Q31), not a mix of two operands.
    let b = main_with(|b| {
        let zero = b.int(0);
        let (s1, t) = b.var("total", None, zero);
        let tr = b.local_ref(t);
        let two = b.int(2);
        vec![s1, b.assign(tr, AssignOp::Div, two)]
    });
    assert_eq!(codes(b), vec!["K0600"]);
}

#[test]
fn unused_values_and_results() {
    let b = main_with(|b| {
        let (l, r) = (b.int(1), b.int(2));
        let sum = b.bin(BinOp::Add, l, r);
        vec![b.expr_stmt(sum)]
    });
    assert_eq!(codes(b), vec!["K0730"]);
    let b = main_with(|b| {
        let ok_ = b.lang(LangItem::Ok);
        let one = b.int(1);
        let call = b.call(ok_, vec![one]);
        vec![b.expr_stmt(call)]
    });
    assert_eq!(codes(b), vec!["K1303"]);
}

#[test]
fn value_if_needs_else_and_never_coerces() {
    let b = main_with(|b| {
        let c = b.bool(true);
        let one = b.int(1);
        let s = b.expr_stmt(one);
        let body = b.block(vec![s]);
        let i = b.if_expr(vec![(c, body)], None);
        vec![let_stmt(b, None, i)]
    });
    assert_eq!(codes(b), vec!["K0731"]);
    // `let x: Int = if c: panic("no") else: 3`
    ok(main_with(|b| {
        let c = b.bool(true);
        let panic = b.lang(LangItem::Panic);
        let msg = b.str("no");
        let call = b.call(panic, vec![msg]);
        let s = b.expr_stmt(call);
        let then = b.block(vec![s]);
        let three = b.int(3);
        let s = b.expr_stmt(three);
        let els = b.block(vec![s]);
        let i = b.if_expr(vec![(c, then)], Some(els));
        let t = int_ty(b);
        vec![let_stmt(b, Some(t), i)]
    }));
}

#[test]
fn match_on_int_needs_a_final_catch_all() {
    let build = |with_wild: bool| {
        main_with(|b| {
            let one = b.int(1);
            let (s1, x) = b.let_("x", None, one);
            let xr = b.local_ref(x);
            let mut arms = Vec::new();
            let p = b.pat_int(1);
            let v = b.str("one");
            let s = b.expr_stmt(v);
            arms.push((p, b.block(vec![s])));
            if with_wild {
                let p = b.pat_wild();
                let v = b.str("many");
                let s = b.expr_stmt(v);
                arms.push((p, b.block(vec![s])));
            }
            let m = b.match_expr(xr, arms);
            let (s2, _) = b.let_("label", None, m);
            vec![s1, s2]
        })
    };
    assert_eq!(codes(build(false)), vec!["K1001"]);
    ok(build(true));
}

#[test]
fn missing_return_is_k1101() {
    let mut b = Builder::new("app/main");
    let f = b.def();
    let p = int_ty(&b);
    let x = b.param("x", p);
    let xr = b.local_ref(x.local);
    let s = b.expr_stmt(xr);
    let _ = s;
    let one = b.int(1);
    let unit_call = b.print(one);
    let body = b.block(vec![unit_call]);
    let ret = int_ty(&b);
    b.func(f, "f", vec![x], Some(ret), BodyValue::Block(body));
    assert_eq!(codes(b), vec!["K1101"]);
}

// ---------------------------------------------------------------------------------------
// Functions, records, enums
// ---------------------------------------------------------------------------------------

/// `fn double(x: Int) -> Int => x * 2`
fn add_double(b: &mut Builder) -> DefId {
    let d = b.def();
    let t = int_ty(b);
    let x = b.param("x", t);
    let xr = b.local_ref(x.local);
    let two = b.int(2);
    let body = b.bin(BinOp::Mul, xr, two);
    let ret = int_ty(b);
    b.func(d, "double", vec![x], Some(ret), BodyValue::Expr(body))
}

#[test]
fn calls_check_arguments_and_labels() {
    let mut b = Builder::new("app/main");
    let double = add_double(&mut b);
    let callee = b.def_ref(double);
    let arg = b.int(21);
    let call = b.call(callee, vec![arg]);
    let interp = b.interp(vec![StrPart::Text("x".into()), StrPart::Hole(call)]);
    let p = b.print(interp);
    let callee = b.def_ref(double);
    let arg = b.int(4);
    let named = b.call_named(callee, vec![("x", arg)]);
    let (s, _) = b.let_("y", None, named);
    b.main(vec![p, s]);
    ok(b);

    let mut b = Builder::new("app/main");
    let double = add_double(&mut b);
    let callee = b.def_ref(double);
    let arg = b.str("21");
    let call = b.call(callee, vec![arg]);
    let (s1, _) = b.let_("a", None, call);
    let callee = b.def_ref(double);
    let call = b.call(callee, vec![]);
    let (s2, _) = b.let_("b", None, call);
    let callee = b.def_ref(double);
    let arg = b.int(1);
    let call = b.call_named(callee, vec![("y", arg)]);
    let (s3, _) = b.let_("c", None, call);
    b.main(vec![s1, s2, s3]);
    assert_eq!(codes(b), vec!["K0600", "K1105", "K1105", "K1105"]);
}

#[test]
fn print_takes_one_show_value() {
    let b = main_with(|b| {
        let (x, y) = (b.int(1), b.int(2));
        let p = b.lang(LangItem::Print);
        let call = b.call(p, vec![x, y]);
        vec![b.expr_stmt(call)]
    });
    assert_eq!(codes(b), vec!["K0916"]);
    ok(main_with(|b| {
        let x = b.int(1);
        vec![b.print(x)]
    }));
    // A function value is not `Show`.
    let mut b = Builder::new("app/main");
    let double = add_double(&mut b);
    let f = b.def_ref(double);
    let p = b.print(f);
    b.main(vec![p]);
    assert_eq!(codes(b), vec!["K0603"]);
}

fn add_user(b: &mut Builder) -> DefId {
    let d = b.def();
    let fields = vec![
        FieldDecl {
            name: "name".into(),
            ty: b.builtin(BuiltinType::String),
            default: None,
            span: Span::default(),
        },
        FieldDecl {
            name: "age".into(),
            ty: int_ty(b),
            default: Some(b.int(0)),
            span: Span::default(),
        },
    ];
    b.item(
        d,
        "User",
        ItemKind::Record(RecordDecl {
            generics: vec![],
            fields,
            opaque: false,
            derives: vec![],
            derive_spans: vec![],
        }),
    )
}

#[test]
fn records_construct_with_named_arguments_and_have_typed_fields() {
    let mut b = Builder::new("app/main");
    let user = add_user(&mut b);
    let ctor = b.def_ref(user);
    let name = b.str("Sara");
    let make = b.call_named(ctor, vec![("name", name)]);
    let (s1, u) = b.let_("u", None, make);
    let ur = b.local_ref(u);
    let age = b.field(ur, "age");
    let (s2, a) = b.let_("a", None, age);
    let main = b.main(vec![s1, s2]);
    let r = ok(b);
    assert_eq!(r.local_types[&(main, u)], Ty::Adt(user, vec![]));
    assert_eq!(r.local_types[&(main, a)], Ty::Int);

    let mut b = Builder::new("app/main");
    let user = add_user(&mut b);
    let ctor = b.def_ref(user);
    let name = b.str("Sara");
    let positional = b.call(ctor, vec![name]);
    let (s1, _) = b.let_("u", None, positional);
    let ctor = b.def_ref(user);
    let age = b.int(3);
    let missing = b.call_named(ctor, vec![("age", age)]);
    let (s2, u2) = b.let_("v", None, missing);
    let ur = b.local_ref(u2);
    let bad = b.field(ur, "email");
    let (s3, _) = b.let_("e", None, bad);
    b.main(vec![s1, s2, s3]);
    assert_eq!(codes(b), vec!["K0416", "K1105", "K0420"]);
}

fn add_role(b: &mut Builder) -> (DefId, DefId, DefId) {
    let d = b.def();
    let (m, a) = (b.def(), b.def());
    let variants = vec![
        VariantDecl {
            def: m,
            name: "Member".into(),
            fields: vec![],
            span: Span::default(),
        },
        VariantDecl {
            def: a,
            name: "Admin".into(),
            fields: vec![],
            span: Span::default(),
        },
    ];
    b.item(
        d,
        "Role",
        ItemKind::Enum(EnumDecl {
            generics: vec![],
            variants,
            opaque: false,
            derives: vec![],
            derive_spans: vec![],
        }),
    );
    (d, m, a)
}

#[test]
fn implicit_members_need_an_expected_type_and_match_is_exhaustive() {
    let mut b = Builder::new("app/main");
    let (role, _, _) = add_role(&mut b);
    let admin = b.implicit("Admin");
    let t = b.ty(TypeKind::Def(role, vec![]));
    let (s1, r) = b.let_("r", Some(t), admin);
    let rr = b.local_ref(r);
    let mut arms = Vec::new();
    for (v, text) in [("Member", "member"), ("Admin", "admin")] {
        let p = b.pat(PatKind::Variant {
            def: None,
            name: v.into(),
            fields: vec![],
            rest: false,
        });
        let s = b.str(text);
        let s = b.expr_stmt(s);
        arms.push((p, b.block(vec![s])));
    }
    let m = b.match_expr(rr, arms);
    let (s2, label) = b.let_("label", None, m);
    let main = b.main(vec![s1, s2]);
    let res = ok(b);
    assert_eq!(res.local_types[&(main, label)], Ty::String);

    let mut b = Builder::new("app/main");
    add_role(&mut b);
    let admin = b.implicit("Admin");
    let (s1, r) = b.let_("r", None, admin);
    let _ = r;
    b.main(vec![s1]);
    assert_eq!(codes(b), vec!["K0803"]);

    let mut b = Builder::new("app/main");
    let (role, _, _) = add_role(&mut b);
    let admin = b.implicit("Admin");
    let t = b.ty(TypeKind::Def(role, vec![]));
    let (s1, r) = b.let_("r", Some(t), admin);
    let rr = b.local_ref(r);
    let p = b.pat(PatKind::Variant {
        def: None,
        name: "Member".into(),
        fields: vec![],
        rest: false,
    });
    let s = b.pass_stmt();
    let body = b.block(vec![s]);
    let m = b.match_expr(rr, vec![(p, body)]);
    let s2 = b.expr_stmt(m);
    b.main(vec![s1, s2]);
    let res = check(b);
    assert_eq!(res.codes(), vec!["K1001"]);
    assert!(
        matches!(&res.errors[0].kind, crate::TypeErrorKind::NonExhaustive { missing } if missing == &vec![".Admin".to_string()])
    );
}

/// `fn first[T](xs: List[T]) -> Option[T]` as a signature stub (M2 stdlib stubs).
fn add_first(b: &mut Builder) -> DefId {
    let d = b.def();
    let t = b.ty(TypeKind::Param(0));
    let list = b.ty(TypeKind::Builtin(BuiltinType::List, vec![t.clone()]));
    let xs = b.param("xs", list);
    let ret = b.ty(TypeKind::Builtin(BuiltinType::Option, vec![t]));
    b.item(
        d,
        "first",
        ItemKind::Fn(FnDecl {
            generics: vec![GenericParamDecl {
                name: "T".into(),
                bounds: vec![],
            }],
            has_self: false,
            params: vec![xs],
            ret: Some(ret),
            sync_attr: false,
            positional: false,
            parent: None,
            body: None,
        }),
    )
}

#[test]
fn generic_calls_instantiate_per_use() {
    let mut b = Builder::new("app/main");
    let first = add_first(&mut b);
    let callee = b.def_ref(first);
    let callee_id = callee.id;
    let (x, y) = (b.int(1), b.int(2));
    let xs = b.list(vec![x, y]);
    let call = b.call(callee, vec![xs]);
    let (s1, a) = b.let_("a", None, call);
    let callee = b.def_ref(first);
    let s = b.str("s");
    let xs = b.list(vec![s]);
    let call = b.call(callee, vec![xs]);
    let (s2, c) = b.let_("c", None, call);
    let main = b.main(vec![s1, s2]);
    let r = ok(b);
    assert_eq!(r.local_types[&(main, a)], Ty::option(Ty::Int));
    assert_eq!(r.local_types[&(main, c)], Ty::option(Ty::String));
    assert_eq!(r.instantiations[&(main, callee_id)], vec![Ty::Int]);
}

#[test]
fn fn_typed_parameters_are_effect_polymorphic() {
    // fn apply(f: fn(Int) -> Int, x: Int) -> Int   (stub)
    let mut b = Builder::new("app/main");
    let double = add_double(&mut b);
    let apply = b.def();
    let i = int_ty(&b);
    let fty = b.ty(TypeKind::Fn {
        sync: false,
        params: vec![i.clone()],
        ret: Box::new(i.clone()),
    });
    let f = b.param("f", fty);
    let x = b.param("x", i.clone());
    b.item(
        apply,
        "apply",
        ItemKind::Fn(FnDecl {
            generics: vec![],
            has_self: false,
            params: vec![f, x],
            ret: Some(i),
            sync_attr: false,
            positional: false,
            parent: None,
            body: None,
        }),
    );
    let callee = b.def_ref(apply);
    let dr = b.def_ref(double);
    let one = b.int(1);
    let call = b.call(callee, vec![dr, one]);
    let (s, _) = b.let_("r", None, call);
    let main = b.main(vec![s]);
    let r = ok(b);
    let scheme = &r.signatures[&apply];
    assert_eq!(scheme.effect_params, 2, "f's bit and apply's own bit");
    let Ty::Fn { params, .. } = &scheme.ty else {
        panic!()
    };
    assert!(matches!(
        params[0],
        Ty::Fn {
            suspension: Suspension::Param(0),
            ..
        }
    ));
    // Passing `double` records a flow from double's bit to f's bit for kalam-effects.
    assert!(r.flows.iter().any(|(owner, f)| *owner == main
        && matches!(f.from, Suspension::Var(_))
        && matches!(f.to, Suspension::Var(_))));
}

#[test]
fn function_types_in_data_are_suspending_unless_sync() {
    let mut b = Builder::new("app/main");
    let d = b.def();
    let unit = b.ty(TypeKind::Unit);
    let f = b.ty(TypeKind::Fn {
        sync: false,
        params: vec![],
        ret: Box::new(unit.clone()),
    });
    let g = b.ty(TypeKind::Fn {
        sync: true,
        params: vec![],
        ret: Box::new(unit),
    });
    let fields = vec![
        FieldDecl {
            name: "on_click".into(),
            ty: f,
            default: None,
            span: Span::default(),
        },
        FieldDecl {
            name: "update".into(),
            ty: g,
            default: None,
            span: Span::default(),
        },
    ];
    b.item(
        d,
        "Handlers",
        ItemKind::Record(RecordDecl {
            generics: vec![],
            fields,
            opaque: false,
            derives: vec![],
            derive_spans: vec![],
        }),
    );
    let r = ok(b);
    let tys: Vec<Ty> = r.adts[&d].component_types(&[]);
    assert!(matches!(
        tys[0],
        Ty::Fn {
            suspension: Suspension::Suspending,
            ..
        }
    ));
    assert!(matches!(
        tys[1],
        Ty::Fn {
            suspension: Suspension::Sync,
            ..
        }
    ));
}

#[test]
fn lambdas_need_types_from_context() {
    let b = main_with(|b| {
        let p = b.local("v", false);
        let body = b.local_ref(p);
        let lam = b.e(ExprKind::Lambda {
            params: vec![LambdaParam { local: p, ty: None }],
            ret: None,
            body: Box::new(body),
            closure: 1,
        });
        vec![let_stmt(b, None, lam)]
    });
    assert_eq!(codes(b), vec!["K0801"]);
    let mut b = Builder::new("app/main");
    let p = b.local("v", false);
    let pr = b.local_ref(p);
    let one = b.int(1);
    let body = b.bin(BinOp::Add, pr, one);
    let lam = b.e(ExprKind::Lambda {
        params: vec![LambdaParam { local: p, ty: None }],
        ret: None,
        body: Box::new(body),
        closure: 1,
    });
    let i = int_ty(&b);
    let ft = b.ty(TypeKind::Fn {
        sync: false,
        params: vec![i.clone()],
        ret: Box::new(i),
    });
    let (s, _) = b.let_("inc", Some(ft), lam);
    let main = b.main(vec![s]);
    let r = ok(b);
    assert_eq!(r.local_types[&(main, p)], Ty::Int);
}

#[test]
fn empty_list_needs_an_expected_type() {
    let b = main_with(|b| {
        let l = b.list(vec![]);
        vec![let_stmt(b, None, l)]
    });
    assert_eq!(codes(b), vec!["K0802"]);
    ok(main_with(|b| {
        let l = b.list(vec![]);
        let i = int_ty(b);
        let t = b.ty(TypeKind::Builtin(BuiltinType::List, vec![i]));
        vec![let_stmt(b, Some(t), l)]
    }));
}

// ---------------------------------------------------------------------------------------
// Sinks, secrets, traits
// ---------------------------------------------------------------------------------------

#[test]
fn sinks_never_accept_string() {
    let b = main_with(|b| {
        let s = b.str("SELECT 1");
        let t = b.builtin(BuiltinType::Sink(SinkType::Sql));
        vec![let_stmt(b, Some(t), s)]
    });
    assert_eq!(codes(b), vec!["K1601"]);
    let mut b = Builder::new("app/main");
    let q = b.e(ExprKind::Tagged {
        tag: kalam_sinks::Tag::Sql,
        parts: vec!["SELECT 1".into()],
        holes: vec![],
    });
    let (s, q) = b.let_("q", None, q);
    let main = b.main(vec![s]);
    let r = ok(b);
    assert_eq!(r.local_types[&(main, q)], Ty::Sink(SinkType::Sql));
}

#[test]
fn deriving_encode_with_a_secret_field_is_k0612() {
    let mut b = Builder::new("app/main");
    let d = b.def();
    let s = b.builtin(BuiltinType::String);
    let secret = b.ty(TypeKind::Builtin(BuiltinType::Secret, vec![s]));
    b.item(
        d,
        "Login",
        ItemKind::Record(RecordDecl {
            generics: vec![],
            fields: vec![FieldDecl {
                name: "password".into(),
                ty: secret,
                default: None,
                span: Span::default(),
            }],
            opaque: false,
            derives: vec![BuiltinTrait::Encode],
            derive_spans: vec![],
        }),
    );
    assert_eq!(codes(b), vec!["K0612"]);
}

fn impl_item(
    b: &mut Builder,
    self_ty: Type,
    tr: Option<BuiltinTrait>,
    methods: Vec<DefId>,
) -> DefId {
    let d = b.def();
    b.item(
        d,
        "impl",
        ItemKind::Impl(ImplItem {
            generics: vec![],
            self_ty,
            as_trait: tr.map(|t| TraitBound {
                id: TraitId::Builtin(t),
                args: vec![],
            }),
            methods,
        }),
    )
}

#[test]
fn impls_obey_the_orphan_rule_and_methods_resolve() {
    // impl Int as Show (in a user module): orphan.
    let mut b = Builder::new("app/main");
    let i = int_ty(&b);
    impl_item(&mut b, i, Some(BuiltinTrait::Show), vec![]);
    assert_eq!(codes(b), vec!["K1106"]);

    // impl User: fn initial(self) -> String => "S"
    let mut b = Builder::new("app/main");
    let user = add_user(&mut b);
    let m = b.def();
    let impl_def = b.def();
    let self_t = b.ty(TypeKind::SelfType);
    let this = b.param("self", self_t);
    let body = b.str("S");
    let ret = b.builtin(BuiltinType::String);
    b.method_fn(
        m,
        "initial",
        impl_def,
        vec![this],
        Some(ret),
        BodyValue::Expr(body),
    );
    let ut = b.ty(TypeKind::Def(user, vec![]));
    b.item(
        impl_def,
        "impl",
        ItemKind::Impl(ImplItem {
            generics: vec![],
            self_ty: ut,
            as_trait: None,
            methods: vec![m],
        }),
    );
    let ctor = b.def_ref(user);
    let name = b.str("Sara");
    let make = b.call_named(ctor, vec![("name", name)]);
    let (s1, u) = b.let_("u", None, make);
    let ur = b.local_ref(u);
    let call = b.method_call(ur, "initial", vec![]);
    let call_id = call.id;
    let (s2, i) = b.let_("i", None, call);
    let ur = b.local_ref(u);
    let bad = b.method_call(ur, "nope", vec![]);
    let (s3, _) = b.let_("n", None, bad);
    let main = b.main(vec![s1, s2, s3]);
    let r = check(b);
    assert_eq!(r.codes(), vec!["K0420"]);
    assert_eq!(r.local_types[&(main, i)], Ty::String);
    assert_eq!(r.method_picks[&(main, call_id)], m);
}

/// Malformed input from the adapter must never panic (the type checker is a fuzz target):
/// a type naming a `DefId` that is not an item, a call to a non-item, and a method declared
/// with `has_self` but no parameters.
#[test]
fn unknown_defs_and_malformed_methods_do_not_panic() {
    let mut b = Builder::new("app/main");
    let ghost = DefId(9_999);
    let t = b.ty(TypeKind::Def(ghost, vec![]));
    let one = b.int(1);
    let (s1, x) = b.let_("x", Some(t), one);
    let callee = b.def_ref(ghost);
    let call = b.call(callee, vec![]);
    let (s2, _) = b.let_("y", None, call);
    let main = b.main(vec![s1, s2]);
    let r = check(b);
    assert_eq!(r.local_types[&(main, x)], Ty::Error);

    let mut b = Builder::new("app/main");
    let user = add_user(&mut b);
    let (m, imp) = (b.def(), b.def());
    let body = b.int(1);
    let ret = int_ty(&b);
    b.method_fn(m, "broken", imp, vec![], Some(ret), BodyValue::Expr(body));
    let ut = b.ty(TypeKind::Def(user, vec![]));
    b.item(
        imp,
        "impl",
        ItemKind::Impl(ImplItem {
            generics: vec![],
            self_ty: ut,
            as_trait: None,
            methods: vec![m],
        }),
    );
    let ctor = b.def_ref(user);
    let name = b.str("S");
    let make = b.call_named(ctor, vec![("name", name)]);
    let (s1, u) = b.let_("u", None, make);
    let ur = b.local_ref(u);
    let call = b.method_call(ur, "broken", vec![]);
    let (s2, _) = b.let_("v", None, call);
    b.main(vec![s1, s2]);
    assert_eq!(codes(b), vec!["K1105"]);
}
