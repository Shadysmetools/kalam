//! Parse-error diagnostic codes (see `PARSE-CODES.md` next to this crate).
//!
//! Spec §27 has no parse-error family (spec/CRITIQUE.md B4). These are **proposed** codes in
//! a reserved sub-range `K0250–K0269` of K02xx, pending the spec owner's decision. Codes the
//! spec already defines are used where they apply: K0120 (unexpected indent), K0710
//! (assignment target is not a place), K0720 (chained comparison), K0731 (value-position
//! `if` without `else`) and K0921 (multi-line lambda).

/// Unexpected token: something else was expected (args: `expected`, `found`).
pub const UNEXPECTED_TOKEN: &str = "K0250";
/// A `(`, `[` or `{` is not closed before the end of the line or file.
pub const UNCLOSED_DELIMITER: &str = "K0251";
/// A block header ends with `:` and a newline, but no indented block follows.
pub const EXPECTED_BLOCK: &str = "K0252";
/// A bad interpolation hole: empty `{}`, unclosed `{`, or not exactly one expression.
pub const BAD_HOLE: &str = "K0253";
/// A positional argument after a named one (spec §8.2 `arg`: positional before named).
pub const POSITIONAL_AFTER_NAMED: &str = "K0254";
/// A statement or other non-item at module level (spec §8.2 `file`/`item`).
pub const EXPECTED_ITEM: &str = "K0255";
/// A `use` after the first item (spec §8.2 `file`: uses come first).
pub const USE_AFTER_ITEM: &str = "K0256";
/// Nesting deeper than the parser's limit (protects the stack; not a spec rule).
pub const TOO_DEEP: &str = "K0259";

/// Spec-defined codes the parser also emits.
pub const UNEXPECTED_INDENT: &str = "K0120";
pub const NOT_A_PLACE: &str = "K0710";
pub const CHAINED_COMPARISON: &str = "K0720";
pub const IF_WITHOUT_ELSE: &str = "K0731";
pub const MULTI_LINE_LAMBDA: &str = "K0921";

/// Every code this crate can emit.
pub const ALL: &[&str] = &[
    UNEXPECTED_TOKEN,
    UNCLOSED_DELIMITER,
    EXPECTED_BLOCK,
    BAD_HOLE,
    POSITIONAL_AFTER_NAMED,
    EXPECTED_ITEM,
    USE_AFTER_ITEM,
    TOO_DEEP,
    UNEXPECTED_INDENT,
    NOT_A_PLACE,
    CHAINED_COMPARISON,
    IF_WITHOUT_ELSE,
    MULTI_LINE_LAMBDA,
];
