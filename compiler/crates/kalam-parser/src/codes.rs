//! Parse-error diagnostic codes (see `PARSE-CODES.md` next to this crate).
//!
//! Spec §27 has no parse-error family (spec/CRITIQUE.md B4). These are **proposed** codes in
//! a reserved sub-range `K0250–K0269` of K02xx, pending the spec owner's decision. Codes the
//! spec already defines are used where they apply: K0120 (unexpected indent), K0413 (a core
//! name bound where the AST keeps an identifier), K0710 (assignment target is not a place),
//! K0720 (chained comparison), K0731 (value-position `if` without `else`), K1202
//! (module-level `var`), K1203 (glob import) and the §21.3 known-mistake codes K0901–K0926
//! ([`MISTAKES`]).

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
/// The file's own core name declared where the AST keeps a plain identifier (a `var` name,
/// a function or lambda parameter, a name after `as`, a list-rest name; spec §7.2). Core names
/// in other declaring positions are kept as `Name`s, and name resolution reports them.
pub const CORE_NAME_DECLARED: &str = "K0413";
/// A module-level `var` (spec §13.4): parsed as a module constant.
pub const MODULE_VAR: &str = "K1202";
/// A glob import `use a.*` (spec §13.1): parsed as `use a`.
pub const GLOB_IMPORT: &str = "K1203";

// Known LLM mistakes (spec §21.3). Each is reported *instead of* a generic K0250/K0255 at
// its site, with the fix the spec gives, and the parser recovers as if the canonical form
// had been written. K0904 (`;`), K0911 (`&&`, `||`, `!`) and K0924 (`'…'`) are the lexer's.
pub const TRIPLE_EQUALS: &str = "K0901";
pub const DEF_KEYWORD: &str = "K0902";
pub const ELIF: &str = "K0903";
pub const BRACE_BLOCK: &str = "K0905";
pub const NULL_WORD: &str = "K0906";
pub const ASYNC_AWAIT: &str = "K0907";
pub const TRY_CATCH: &str = "K0908";
pub const INCREMENT: &str = "K0909";
pub const TERNARY: &str = "K0910";
pub const CONST_OR_LET_MUT: &str = "K0912";
pub const F_STRING: &str = "K0913";
pub const ANGLE_GENERICS: &str = "K0914";
pub const UNWRAP: &str = "K0915";
pub const BARE_VARIANT: &str = "K0917";
pub const FREE_FUNCTION: &str = "K0918";
pub const IMPL_FOR: &str = "K0919";
pub const CLASS: &str = "K0920";
pub const POWER: &str = "K0923";
pub const SWITCH: &str = "K0925";
pub const C_FOR: &str = "K0926";

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
    CORE_NAME_DECLARED,
    MODULE_VAR,
    GLOB_IMPORT,
];

/// The §21.3 known-mistake codes this crate emits (K0921 is listed in [`ALL`] too). They are
/// spec codes, documented in spec §21.3 and the diagnostic catalog, not parse-code proposals.
/// Not here: K0904, K0911 and K0924 (the lexer's), and K0916 (`print(a, b)`) and K0922
/// (`f[T](x)`), which are well-formed calls the type checker reports.
pub const MISTAKES: &[&str] = &[
    TRIPLE_EQUALS,
    DEF_KEYWORD,
    ELIF,
    BRACE_BLOCK,
    NULL_WORD,
    ASYNC_AWAIT,
    TRY_CATCH,
    INCREMENT,
    TERNARY,
    CONST_OR_LET_MUT,
    F_STRING,
    ANGLE_GENERICS,
    UNWRAP,
    BARE_VARIANT,
    FREE_FUNCTION,
    IMPL_FOR,
    CLASS,
    MULTI_LINE_LAMBDA,
    POWER,
    SWITCH,
    C_FOR,
];
