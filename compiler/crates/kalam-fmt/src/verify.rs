//! What "the formatter keeps the AST" means, as a comparable string.
//!
//! The formatter may change layout only. In the AST, layout shows up in exactly one place:
//! whether a statement-level block was written on its header line (`if x: return y`) or
//! indented. A too-long one-line block is indented by `fmt`, so that flag is reset before
//! comparing. Inline `if` expressions are never turned into `if` forms, so their flag (and
//! their arms' one-line blocks) stay as they are. Trivia is compared separately, as the list
//! of comment texts: their line numbers and the blank lines legitimately move.

use kalam_ast::*;

/// The span-free, skin-free shape of `ast` without trivia, with every statement-level block
/// marked as indented.
pub fn normalized_shape(ast: &Ast) -> String {
    let mut ast = ast.clone();
    ast.trivia.clear();
    for it in &mut ast.file.items {
        item(it);
    }
    shape(
        &ast,
        ShapeOptions {
            erase_idents: false,
            skip_trivia: true,
        },
    )
}

/// Comment and doc-comment texts in source order.
pub fn comment_texts(ast: &Ast) -> Vec<&str> {
    ast.trivia
        .iter()
        .filter(|t| t.kind != TriviaKind::BlankLine)
        .map(|t| t.text.as_str())
        .collect()
}

fn item(it: &mut Item) {
    match &mut it.kind {
        ItemKind::Fn(f) => fn_decl(f),
        ItemKind::Trait(t) => t.members.iter_mut().for_each(fn_decl),
        ItemKind::Impl(i) => i.members.iter_mut().for_each(|m| fn_decl(&mut m.func)),
        ItemKind::Test(t) => block(&mut t.body),
        _ => {}
    }
}

fn fn_decl(f: &mut FnDecl) {
    if let FnBody::Block(b) = &mut f.body {
        block(b);
    }
}

fn block(b: &mut Block) {
    b.inline = false;
    for s in &mut b.stmts {
        match &mut s.kind {
            StmtKind::Expr(e)
            | StmtKind::Let { value: e, .. }
            | StmtKind::Var { value: e, .. }
            | StmtKind::Assign { value: e, .. }
            | StmtKind::Return(Some(e)) => form(e),
            StmtKind::For { body, .. } | StmtKind::While { body, .. } => block(body),
            StmtKind::Fn(f) => fn_decl(f),
            _ => {}
        }
    }
}

/// Statement-level `if`/`match` forms hold statement-level blocks.
fn form(e: &mut Expr) {
    match &mut e.kind {
        ExprKind::If(i) if !i.inline => {
            for br in &mut i.branches {
                block(&mut br.body);
            }
            if let Some(b) = &mut i.else_body {
                block(b);
            }
        }
        ExprKind::Match(m) => {
            for arm in &mut m.arms {
                block(&mut arm.body);
            }
        }
        _ => {}
    }
}
