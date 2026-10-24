//! The lossless concrete syntax tree (CST), typed AST views over it, and the Kalam
//! Canonical Form (KCF).
//!
//! Design (ARCHITECTURE.md §6, spec §4.4):
//! - The parser emits events into [`TreeBuilder`], which produces an immutable green tree.
//!   Every token keeps its exact source text (spelling, digits, `،`), and comments are kept
//!   as trivia, so printing the tree in its own skin reproduces the file byte for byte.
//! - Keyword and core-name tokens also carry their [`Concept`], so the grammar and every
//!   later stage are skin-free.
//! - [`kcf`] serializes the tree with concepts instead of spellings; its SHA-256 is the unit
//!   of identity (lockfile, registry, signing, parity gate P1).
//!
//! Status: data types only. Open item A1 (in-house tree vs. the `rowan` crate).

use std::sync::Arc;

use kalam_base::Span;
use kalam_skins::Concept;

/// Node and token kinds. Token kinds mirror `kalam_lexer::TokenKind`; node kinds mirror the
/// productions of spec §8.2 (75 productions; only the top-level ones are listed so far).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SyntaxKind {
    // Tokens
    Ident,
    Keyword(Concept),
    Core(Concept),
    IntLit,
    FloatLit,
    StrLit,
    TaggedStrLit,
    Punct,
    Newline,
    Indent,
    Dedent,
    Comment,
    DocComment,
    Whitespace,
    // Nodes
    File,
    UseDecl,
    Attribute,
    FnDecl,
    TypeDecl,
    EnumDecl,
    TraitDecl,
    ImplDecl,
    ConstDecl,
    ExternDecl,
    TestDecl,
    Block,
    Stmt,
    IfForm,
    MatchForm,
    CaseArm,
    ForStmt,
    WhileStmt,
    Expr,
    Pattern,
    Type,
    /// Tokens the parser skipped during error recovery. `fmt` prints them unchanged.
    Error,
}

/// A leaf of the green tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenToken {
    pub kind: SyntaxKind,
    /// Exact source text, including the original spelling.
    pub text: Arc<str>,
}

/// An interior node of the green tree. Green nodes have no positions and no parents, so
/// identical subtrees can be shared; positions are computed by red (cursor) nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenNode {
    pub kind: SyntaxKind,
    pub children: Vec<GreenElement>,
    /// Total text length in bytes.
    pub len: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GreenElement {
    Node(Arc<GreenNode>),
    Token(Arc<GreenToken>),
}

impl GreenElement {
    pub fn len(&self) -> u32 {
        match self {
            GreenElement::Node(n) => n.len,
            GreenElement::Token(t) => t.text.len() as u32,
        }
    }
}

/// Builds a green tree from parser events (`start_node`, `token`, `finish_node`).
#[derive(Default)]
pub struct TreeBuilder {
    stack: Vec<(SyntaxKind, Vec<GreenElement>)>,
}

impl TreeBuilder {
    pub fn start_node(&mut self, kind: SyntaxKind) {
        self.stack.push((kind, Vec::new()));
    }

    pub fn token(&mut self, kind: SyntaxKind, text: &str) {
        let token = GreenElement::Token(Arc::new(GreenToken {
            kind,
            text: Arc::from(text),
        }));
        self.stack
            .last_mut()
            .expect("token() outside of any node")
            .1
            .push(token);
    }

    pub fn finish_node(&mut self) {
        let (kind, children) = self.stack.pop().expect("unbalanced finish_node()");
        let len = children.iter().map(GreenElement::len).sum();
        let node = GreenElement::Node(Arc::new(GreenNode { kind, children, len }));
        match self.stack.last_mut() {
            Some(parent) => parent.1.push(node),
            None => self.stack.push((SyntaxKind::File, vec![node])),
        }
    }

    /// Finishes the tree. The root is the last finished top-level node.
    pub fn finish(mut self) -> Arc<GreenNode> {
        let (_, mut children) = self.stack.pop().expect("empty tree");
        match children.pop() {
            Some(GreenElement::Node(root)) => root,
            _ => panic!("the root must be a node"),
        }
    }
}

/// A parsed file: the green tree plus the span bookkeeping the AST views need.
#[derive(Clone, Debug)]
pub struct Cst {
    pub root: Arc<GreenNode>,
}

impl Cst {
    /// Reconstructs the source text (lossless round trip in the file's own skin).
    pub fn text(&self) -> String {
        fn walk(node: &GreenNode, out: &mut String) {
            for child in &node.children {
                match child {
                    GreenElement::Node(n) => walk(n, out),
                    GreenElement::Token(t) => out.push_str(&t.text),
                }
            }
        }
        let mut out = String::new();
        walk(&self.root, &mut out);
        out
    }
}

/// Typed AST views: zero-cost wrappers over CST nodes with accessor methods (Phase 0).
pub mod ast {
    use super::GreenNode;
    use std::sync::Arc;

    /// `fn name[generics](params) -> ret: body` (spec §8.2 `fn_decl`).
    pub struct FnDecl(pub Arc<GreenNode>);
}

/// Kalam Canonical Form (spec §4.4).
pub mod kcf {
    use super::Cst;

    /// SHA-256 of the KCF bytes. The `sha2` dependency is added with the parser.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct KcfHash(pub [u8; 32]);

    /// Serializes a CST skin-free: keywords and core names as concept IDs, `،`/`؟` as their
    /// ASCII concepts, numbers as kind plus exact value, identifiers as NFC code points,
    /// strings, comments and blank lines byte for byte.
    pub fn serialize(_cst: &Cst) -> Vec<u8> {
        todo!("KCF serialization (Phase 0, step 3)")
    }
}

/// Spans are byte ranges; this re-export keeps downstream crates on one definition.
pub type TextRange = Span;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_round_trips_text() {
        let mut b = TreeBuilder::default();
        b.start_node(SyntaxKind::File);
        b.token(SyntaxKind::Keyword(Concept::KwFn), "fn");
        b.token(SyntaxKind::Whitespace, " ");
        b.token(SyntaxKind::Core(Concept::CoreMain), "main");
        b.finish_node();
        let cst = Cst { root: b.finish() };
        assert_eq!(cst.text(), "fn main");
        assert_eq!(cst.root.len, 7);
    }
}
