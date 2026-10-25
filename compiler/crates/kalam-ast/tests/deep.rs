//! Stack safety: walking a tree at (and above) the parser's height cap (`MAX_HEIGHT` = 1024
//! in kalam-parser) must fit a 1 MiB stack, the Windows main-thread default. A stack
//! overflow aborts the process, so no `catch_unwind` could save a caller.

use kalam_ast::*;
use kalam_base::Span;
use kalam_skins::Concept;

/// Above the parser's `MAX_HEIGHT` (1024), so the cap itself is covered with margin.
const DEPTH: usize = 1500;

fn e(kind: ExprKind) -> Expr {
    Expr {
        kind,
        span: Span::default(),
    }
}

fn name(n: &str) -> Expr {
    e(ExprKind::Name(NameKind::Ident(n.into())))
}

/// A chain of `DEPTH` nested expressions that cycles through every one-child expression
/// form the parser can chain without hitting its nesting limit.
fn deep_expr(depth: usize, leaf: &str) -> Expr {
    let mut x = name(leaf);
    for i in 0..depth {
        let b = Box::new(x);
        x = e(match i % 8 {
            0 => ExprKind::Binary {
                op: BinOp::Add,
                lhs: b,
                rhs: Box::new(name("y")),
            },
            1 => ExprKind::Neg(b),
            2 => ExprKind::Call {
                callee: b,
                args: vec![],
            },
            3 => ExprKind::Member {
                base: b,
                member: Member::Name(Ident {
                    name: "m".into(),
                    span: Span::default(),
                }),
            },
            4 => ExprKind::Try(b),
            5 => ExprKind::Index {
                base: b,
                index: Box::new(name("i")),
            },
            6 => ExprKind::Not(b),
            _ => ExprKind::Binary {
                op: BinOp::Mul,
                lhs: Box::new(name("z")),
                rhs: b,
            },
        });
    }
    x
}

fn deep_ast_of(depth: usize, leaf: &str) -> Ast {
    let span = Span::default();
    let stmt = Stmt {
        kind: StmtKind::Let {
            pat: Pat {
                kind: PatKind::Bind(Ident {
                    name: "v".into(),
                    span,
                }),
                span,
            },
            ty: None,
            value: deep_expr(depth, leaf),
        },
        span,
    };
    let main = FnDecl {
        sig: FnSig {
            name: Name {
                kind: NameKind::Core(Concept::CoreMain),
                span,
            },
            generics: vec![],
            params: vec![],
            ret: None,
            span,
        },
        body: FnBody::Block(Block {
            stmts: vec![stmt],
            inline: false,
            span,
        }),
        span,
    };
    Ast {
        file: File {
            uses: vec![],
            items: vec![Item {
                attrs: vec![],
                vis: Visibility::Private,
                kind: ItemKind::Fn(main),
                span,
            }],
            span,
        },
        trivia: vec![],
    }
}

fn on_1mib<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .expect("fits a 1 MiB stack")
}

#[test]
fn shape_of_a_tree_above_max_height_fits_a_1mib_stack() {
    let s = on_1mib(|| {
        let ast = deep_ast();
        let s = shape(&ast, ShapeOptions::default());
        drop(ast);
        s
    });
    assert!(
        s.starts_with("(file\n (item (fn (sig CORE_MAIN"),
        "{}",
        &s[..80]
    );
    assert_eq!(
        s.matches("(neg").count(),
        DEPTH / 8 + usize::from(DEPTH % 8 > 1)
    );
}

#[test]
fn clone_eq_debug_and_drop_of_a_tree_above_max_height_fit_a_1mib_stack() {
    let (debug, pretty) = on_1mib(|| {
        let a = deep_ast();
        let b = a.clone();
        assert!(a == b, "a clone equals its original");
        // Differs only at the deepest leaf, so `==` has to walk the whole chain.
        let c = deep_ast_of(DEPTH, "w");
        assert!(a != c);
        let debug = format!("{a:?}");
        drop((a, b, c));
        // `{:#?}` output grows with depth squared (indentation), so a smaller tree that is
        // still deep enough to leave the caller's stack.
        let pretty = format!("{:#?}", deep_ast_of(300, "x"));
        (debug, pretty)
    });
    assert_eq!(
        debug.matches("Neg(").count(),
        DEPTH / 8 + usize::from(DEPTH % 8 > 1)
    );
    assert!(
        debug.ends_with("trivia: [] }"),
        "{}",
        &debug[debug.len() - 40..]
    );
    // Pretty output keeps its indentation through the part printed on the helper stack.
    let deepest = pretty.lines().map(|l| l.len() - l.trim_start().len()).max();
    assert!(deepest > Some(300 * 8), "{deepest:?}");
    assert!(
        pretty.contains("Name(\n"),
        "pretty form, not the compact one"
    );
}

#[test]
fn guarded_impls_match_the_derived_format() {
    let x = e(ExprKind::Neg(Box::new(name("x"))));
    assert_eq!(
        format!("{x:?}"),
        "Expr { kind: Neg(Expr { kind: Name(Ident(\"x\")), span: Span { start: 0, end: 0 } }), \
         span: Span { start: 0, end: 0 } }"
    );
    assert_eq!(x.clone(), x);
    assert_ne!(x, name("x"));
}

fn deep_ast() -> Ast {
    deep_ast_of(DEPTH, "x")
}
