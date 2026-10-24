//! Recursive-descent parser for the grammar of spec §8.2, producing the lossless CST.
//!
//! - **LL(1) after left-factoring at the four DR-34 points**, with no backtracking:
//!   `if` at statement or rhs start (§8.3 rule 1), `(` unit/group/tuple, expression
//!   statement vs. assignment, and named vs. positional arguments.
//! - **Skin-free:** the parser matches `TokenKind::Keyword(Concept)`, never spellings.
//!   A `Keyword`/`Core` token is accepted as an identifier where §5.3 rule 8 allows it
//!   (named-argument labels, record field names).
//! - **Error recovery:** a missing token is reported once and inserted virtually. On an
//!   unexpected token the parser skips to the next NEWLINE or DEDENT at the current block
//!   depth and wraps the skipped tokens in an `Error` node, so `fmt` never drops code.
//! - **Known LLM mistakes** (K0901–K0926) are recognized only in the obvious syntactic
//!   position, each with a machine fix (spec §21.3).
//!
//! Status: stub (ARCHITECTURE.md §22, Phase 0 step 3).

use kalam_base::FileId;
use kalam_diagnostics::Diagnostic;
use kalam_lexer::LexOutput;
use kalam_syntax::Cst;

pub struct ParseOutput {
    pub cst: Cst,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses one file's tokens (from `kalam_lexer::lex`) into a CST.
pub fn parse(_text: &str, _tokens: &LexOutput, _file: FileId) -> ParseOutput {
    todo!("parser (Phase 0, step 3)")
}
