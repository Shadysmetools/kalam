# Parse-error codes (proposal for the spec owner)

Spec §27 has no parse-error family (spec/CRITIQUE.md **B4**). The parser still needs stable
codes, so this crate uses the **provisional** sub-range **K0250–K0269** inside K02xx. Today
§27 uses only K0201 in K02xx, so nothing collides. CRITIQUE B4 suggests K0700–K0709 instead.
If the spec owner picks another range, change `src/codes.rs` (every emission site
uses the constants there) and this table. The test
`recovery.rs::every_parse_code_is_documented_for_the_spec_owner` checks that every code in
`codes::ALL` has a row here.

## Proposed codes (K0250–K0269, not yet in §27)

| Code | Constant | Meaning | Recovery |
|---|---|---|---|
| K0250 | `UNEXPECTED_TOKEN` | Unexpected token: the grammar expected something else (args `expected`, `found`). A missing token is reported once and inserted virtually. | Skip to the end of the logical line, plus any block that line opens, or to the closing DEDENT. The skipped tokens go into an `Error` node (CST) and `(error)` (AST). |
| K0251 | `UNCLOSED_DELIMITER` | `(`, `[` or `{` not closed before the end of the line or file. Covers "unclosed bracket at EOF" from B4. | Close it virtually at the line or file end. |
| K0252 | `EXPECTED_BLOCK` | A block header ends with `:` and a newline, but no indented block follows. | Use an empty block and continue at the same indent. |
| K0253 | `BAD_HOLE` | A bad interpolation hole: empty `{}`, or a hole that is not exactly one expression. An unclosed `{` is reported by the lexer (K0129/K0131), not here. | The hole becomes an error expression. The string is kept. |
| K0254 | `POSITIONAL_AFTER_NAMED` | A positional argument after a named one (§8.2 `arg`). | The argument is kept in the AST. |
| K0255 | `EXPECTED_ITEM` | A statement or other non-item at module level (§8.2 `file`/`item`). | Skip that line and its block. |
| K0256 | `USE_AFTER_ITEM` | A `use` after the first item (§8.2 `file`: uses come first). | The `use` is kept in the AST. |
| K0257–K0258 | — | Unused, reserved. | |
| K0259 | `TOO_DEEP` | Nesting deeper than the parser's limit. This protects the stack and is not a language rule. | Reported once per file. The parser stops descending (`MAX_NEST`/`MAX_HEIGHT` in `src/lib.rs`), cuts the over-deep construct short, and goes on parsing. |
| K0261–K0269 | — | Reserved, unused. The rest of B4 today: an unterminated `"""` is the lexer's K0129. A `"""` (any quote) inside a hole is the lexer's K0131. An unknown tag before `"…"` is K0260, a spec code (below). | |

## Spec-defined codes the parser also emits

| Code | Constant | Spec meaning used by the parser |
|---|---|---|
| K0260 | `UNKNOWN_TAG` | Unknown string tag (spec §27 erratum 2026-10-26, FOUNDER-QUEUE Q53; arg `tag`). Any lowercase ASCII word directly before a quote lexes as a tag (§5.6); one that is not a built-in tag (`BUILTIN_TAGS` in `src/mistakes.rs`) is reported at the tag. `f"…"` is K0913 instead. The literal is kept in the AST. |
| K0120 | `UNEXPECTED_INDENT` | An indented line where no block is open. It is also used for an indented line after a leading-dot continuation (§5.8 rule 2) that does not start with `.`. |
| K0710 | `NOT_A_PLACE` | The left side of `=` or `op=` is not a place. |
| K0720 | `CHAINED_COMPARISON` | `a < b < c`. |
| K0731 | `IF_WITHOUT_ELSE` | An `if` used as a value without `else`. |
| K0921 | `MULTI_LINE_LAMBDA` | A lambda body after `:` (multi-line lambda form). |
| K0413 | `CORE_NAME_DECLARED` | The file's own core name declared where the AST keeps a plain identifier: a `var` name, a function or lambda parameter, a name after `as`, a list-rest name (§7.2). It is recovered as an identifier. Core names in item names and in `let`/`for`/`case` patterns are kept as names, and name resolution reports them. Another skin's core name is an ordinary identifier (FOUNDER-QUEUE Q21). |
| K1202 | `MODULE_VAR` | A module-level `var` (§13.4). It is parsed as the constant `let`, with a suggested fix to `let`. |
| K1203 | `GLOB_IMPORT` | A glob import `use a.*` (§13.1). It is parsed as `use a`. No fix. |
| K1005 | `FLOAT_PATTERN` | A `Float` literal pattern, `case 1.5:` or `case 0.0..=1.0:` (§11 rule 6). The literal (or range) is skipped and recovered as an error pattern, so the arm's body still parses. No fix: the help suggests a guard with `.near(…)` or `Decimal`. |

## Questions for the spec owner

1. Keep K0250–K0269, or move the family (for example to K0700–K0709 as CRITIQUE B4 suggests)?
   K07xx is "statements", and it already holds K0710/K0720/K0731, which are also syntax errors.
2. Should K0259 (too deep) be a spec code or an implementation limit (like a K00xx file
   limit)? The current limit is in `src/lib.rs`.
3. Leading-dot continuation (§5.8 rule 2) is applied in the parser, because the lexer does
   not implement it yet. Its one error already uses K0120. If the lexer takes it over, the
   parser emits that error no longer.
