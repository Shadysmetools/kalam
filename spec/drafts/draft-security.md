# Kalam (كلام) Language Specification — v0.1 DRAFT (Security & Correctness angle)

> **Status:** draft for founder review. One of several competing drafts; this one is written *security and correctness first*.
> **Date:** 2026-10-24
> **Inputs:** `DECISIONS.md` (locked D1–D17), research reports 01–06. Charter rule IDs (L1…L14, U1…U9, S1…S17, P1…P14) refer to `research/04-security-by-design.md` §5.
> **Normative words:** MUST / MUST NOT / SHOULD / MAY as in RFC 2119. "Error" means compile-time error unless stated. "Panic" means the runtime abort defined in §13.4.
> **Diagnostic codes:** `K####` = language error, `SEC####` = security rule, `W####` = warning. Every code has a message in all three skins (research 05, P6).

---

## Table of contents

1. Design principles
2. Deviations from the research reports (read this first)
3. Decision D7 — Arabic RTL and digits (resolved)
4. Source text and lexical structure
5. Skins and the keyword table
6. Grammar (EBNF)
7. Type system
8. Soundness argument
9. Expressions and statements
10. Pattern matching
11. Functions and closures
12. Modules, packages and visibility
13. Error handling: `Result`, `?`, panic
14. Concurrency without an `async` keyword
15. Capabilities and effects for I/O
16. Safe string kinds: `Sql`, `Html`, `Shell` and friends
17. Secrets, randomness, decoding
18. Interop with JavaScript and npm
19. Lowering to JavaScript (summary)
20. Deliberately excluded
21. Example programs (8 programs × 3 skins)
22. Conformance tests this spec implies
23. Open questions

---

## 1. Design principles

Kalam is a statically typed, indentation-structured, general-purpose language for professional developers. It is aimed first at full-stack web/SaaS (D2, D3), compiles first to JavaScript (D4), and can be written in three keyword skins over one AST (D6). This draft orders the principles: when two of them conflict, the one listed earlier wins.

| # | Principle | Consequence in this spec |
|---|---|---|
| P1 | **No undefined behaviour in safe code, on any backend** (L1, L2) | Integer semantics are defined by the language (§7.2), not by JS `Number`. Every backend implements the same traps. |
| P2 | **Data never silently becomes code, a query, markup, a path or a command** (L9, S1) | No `eval`. Sinks take distinct types (`Sql`, `Html`, `Shell`, `Url`, `Path`). `String` never converts to them implicitly (§16). |
| P3 | **A well-typed program does not go wrong, except at a marked escape hatch** | Sound type system with no `any`, no unchecked casts, no subtyping and no implicit conversions (§7, §8). `unsafe` and `extern` are the only trust boundaries. |
| P4 | **Authority is a value** (L12) | I/O needs capability values, which originate in `main` (§15). A library that is never handed `Net` cannot reach the network. |
| P5 | **Failure is visible** (L6, L8) | Expected failures are `Result`. Ignoring a value is an error. `match` is exhaustive. Bugs panic and never fail open (§13). |
| P6 | **What you see is what compiles** (U1–U6) | No raw bidi controls or invisible characters anywhere in source, NFC-only files, single-script identifiers, confusable-skeleton collision errors, one skin per file (§4, §5). |
| P7 | **One way to write it** (research 01) | Controlled vocabulary of 29 keywords. The formatter is canonical. It is also the skin translator. Aliases exist only in Franco and are rewritten by `fmt` (§5.4). |
| P8 | **Escape hatches are loud, ASCII and countable** (L13) | `unsafe` and `extern` are spelled the same in all three skins. `kalam audit` counts them on the AST, not with grep (§5.3). |
| P9 | **Sync-looking code, no colour** (D12) | No `async`/`await` keywords. Suspension is inferred, and every suspension point is shown by tooling (§14). |
| P10 | **Small enough to hold in your head and in an LLM context window** (research 06 §7.3) | v0.1 has no macros, no inheritance, no operator overloading and no reflection (§20). |

---

## 2. Deviations from the research reports (read this first)

Where a research report contradicts a locked decision, the decision wins. Where reports contradict each other, this draft takes the **stricter** option and records it here.

| # | Research said | This spec says | Why |
|---|---|---|---|
| X1 | Report 02 §7.1: "English keywords are always accepted in every skin." | **Rejected.** An Arabic-skin file accepts only Arabic keywords. English keywords there are error `K0110`, with a fix-it to the Arabic keyword. The only exceptions are the escape-hatch keywords `unsafe` and `extern`, which are ASCII in every skin (§5.3). | D6 (one file = one skin). Report 04 §4.2 says mixed skins confuse reviewers. |
| X2 | Report 02 §8 has `await`/`انتظر`/`estanna`. Report 03 §4 recommends `async/await`. | **No `async` or `await` keyword in any skin.** | D12. Semantics are in §14. |
| X3 | Report 02 §8 has `try`, `catch`, `finally`, `throw`. | **No exceptions, so none of those keywords.** | D16: `Result` + `?` + panic. Report 04 L6: no unchecked exceptions crossing API boundaries. |
| X4 | Report 02 §8 examples use `{ }` braces. | Blocks are a header colon `:` followed by significant indentation. | D15. |
| X5 | Report 03 §4.1 rule 4: any digit-leading run containing a letter is an identifier (`3ala` is legal). | **User identifiers may never start with a digit, in any skin.** Digit-leading words are legal only as entries in the closed Franco alias table (§5.4). | Report 02 §7.6 and report 04 U5. This removes the number-vs-identifier spoofing surface entirely, instead of mitigating it with an edit-distance rule. |
| X6 | Report 02 §7.5 allows LRM/RLM/ALM (U+200E/U+200F/U+061C) as whitespace, which the formatter may insert. | **Rejected raw everywhere**, including comments and strings. Display isolation is done by the *renderer* (LSP, formatter display mode, diff viewer), never by bytes in the file. | Report 05 §1.6 (escape-only). It keeps the file free of invisible characters, so a byte-level review equals a visual review. |
| X7 | Report 02 §7.4 *folds* tatweel and harakat out of identifiers (silently). | **Rejected in identifiers and keywords**, with an auto-fix. They remain allowed in strings and comments. | Silent folding means two different byte sequences name one variable, which defeats grep and review. A reject plus fix-it gives the same ergonomics and keeps the file canonical. |
| X8 | Report 02 §7.4 folds hamza/alef variants, ى/ي, ة/ه in identifiers. | Identifiers are **not** folded (they stay distinct names). The fold is used to compute a **skeleton**, and two distinct identifiers with the same skeleton visible in one scope is error `SEC0104`. Keywords *are* matched after folding. | Folding identifiers changes meaning silently. Collision-as-error gives the safety without surprise (report 02 §7.4 "collision rule"). |
| X9 | Report 04 L12 lists Clock and Rand as passed capabilities. | `Clock` and `Random` **are** capabilities (§15). Writing to **stdout/stderr** through `print` is the one ambient effect. Libraries using it get a manifest flag (§15.6). | Deviation recorded: hello-world must stay one line. Stdout is the operator's channel, not an exfiltration channel. See §15.6 for the rationale and its limits. |
| X10 | Report 04 §3.2 sketch names the shell type `Command` with tag `cmd"…"`. | The type is **`Shell`** and the tag is **`shell"…"`**. The value is an argv vector. **No shell interpreter is ever involved**: the name means "the thing you would otherwise have used a shell for". | Task naming. Semantics are exactly report 04 S2. |
| X11 | Report 02 §8 includes `repeat`, `const`, `from`, `case`, `default`, `input`. | Dropped from v0.1. `repeat` → `for _ in 0..n`. `const` → module-level `let`. `case`/`default` → match arms and `_`. `input` is a library function that needs a capability. | Fewer keywords (P7). Each dropped word is one less cross-skin collision. |

---

## 3. Decision D7 — Arabic RTL and digits (resolved)

> **D7 RESOLUTION (proposed for founder sign-off)**
>
> 1. **Storage is always logical order.** A `.kla` (Arabic-skin) file is stored as UTF-8 in logical (typing) order like any other file. No bidi control characters are ever stored (§4.4).
> 2. **Display direction comes from the skin.** Tools render Arabic-skin files with an RTL base paragraph direction and English/Franco files with LTR. The LSP, formatter display mode, playground and `kalam diff` apply UTS #55 token isolation, so operators and operands never visually swap. That isolation lives in the *renderer*, never in the file.
> 3. **Western digits `0-9` are the default in every skin**, including Arabic. Egypt writes code, prices and phone numbers with them (report 02 §6.3).
> 4. **Arabic-Indic digits `٠-٩` (U+0660–0669) are accepted in numeric literals only in Arabic-skin files.** A project can set `[format] arabic_digits = true` so `fmt` renders Arabic-skin literals with them. The AST stores the *value*, so this never affects lossless conversion.
> 5. **Extended Arabic-Indic (Persian) digits `۰-۹` (U+06F0–06F9) are rejected in literals** (`SEC0121`, with a fix-it to `٠-٩`). They are near-identical to `٠-٩` and give no MENA benefit.
> 6. **One digit system per literal.** `1٢3` is `SEC0120`. With Arabic-Indic digits the decimal separator is `٫` (U+066B). With Western digits it is `.`. `،` is never a decimal separator. Hex, binary and exponent forms exist only with Western digits.
> 7. **Digits inside identifiers** (`total2`, `مرحلة٢`) are allowed after the first character. For skeleton comparison they fold to ASCII, so `مرحلة٢` and `مرحلة2` declared in one scope collide (`SEC0104`).
> 8. **Runtime formatting is not a language question.** `Int.to_string()` always produces Western digits (a stable wire format). Locale-aware display (`fmt.number(n, locale: "ar-EG")`) is a stdlib function. HTML output gets `dir="auto"` per text node from the UI kit (research 06 §1.2).
> 9. **Terminal diagnostics.** Terminals usually do no bidi or shaping, so `kalam` prints diagnostics for Arabic-skin files in Arabic only when stdout is the LSP, a browser or a terminal that advertises bidi support. Otherwise it prints the code snippet in its original skin and the prose in the user's fallback locale (default: English), flagged `--diag-lang`.

---

## 4. Source text and lexical structure

### 4.1 Encoding and file-level rules

| Rule | Detail | Code |
|---|---|---|
| Encoding | UTF-8 only. Invalid UTF-8 is an error. A BOM (U+FEFF) is rejected anywhere, including at offset 0; `fmt` strips it. | `SEC0100` |
| Normalization | The whole file MUST be in Unicode **NFC**. A non-NFC file is rejected; `fmt` normalizes it. Identifier comparison additionally uses NFKC (§4.5). | `SEC0101` |
| Line endings | `LF`. `CRLF` is accepted and treated as `LF`; `fmt` writes `LF`. A lone `CR` is an error. U+2028/U+2029 (line/paragraph separators) are rejected. | `K0102` |
| Tabs | Tab characters are rejected outside string literals (use `\t` inside strings). | `K0103` |
| Control characters | C0/C1 controls other than LF/CR are rejected outside escapes. | `SEC0102` |
| Skin | Determined by file extension: `.kl` English, `.kla` Arabic, `.klf` Franco. An optional first-line pragma `#skin en` / `#skin ar` / `#skin fr` MUST agree with the extension. | `K0104` |
| Max line length | None enforced. `fmt` wraps at 100 columns. | — |

### 4.2 Character classes

The lexer classifies every code point outside strings and comments as one of:

- **Whitespace:** U+0020 SPACE only. Newline is a token. All other Unicode spaces (NBSP U+00A0, U+2000–200A, U+3000, …) are rejected with a fix-it (`SEC0103`). This stops an "invisible space" from splitting or joining tokens.
- **Identifier characters:** the Kalam identifier profile (§4.5).
- **Digits:** `0-9` in all skins; `٠-٩` in Arabic-skin literals only (§3).
- **Punctuation and operators:** ASCII only, in every skin, plus exactly two Arabic-skin punctuation aliases: `،` (U+060C) for `,` and `؟` (U+061F) for `?`. Both are kept in the CST so conversion is lossless. The canonical Arabic-skin spelling (what `fmt` writes) is `،` and `؟`.
- **Everything else** is an error that names the code point (`K0105`).

### 4.3 Comments

- Line comments only: `#` to end of line. Doc comments: `##`. There are **no block comments**. A block comment can hide code across lines in a reviewer's view, and with no block comments, CST round-tripping stays line-local.
- Comments are trivia in the CST and are preserved byte-for-byte across skin conversion. They are never translated.
- Comment *content* follows §4.4. Raw bidi controls and invisible characters are rejected in comments too (U1).
- `#skin` on line 1 is a pragma, not a comment.

### 4.4 Bidi, invisible and confusable characters (the Trojan Source rules)

This is a P0 rule set. It applies to **all** source text: code, comments and string literals.

| Class | Code points | In code | In comments | In string literals |
|---|---|---|---|---|
| Bidi embeddings, overrides and isolates | U+202A–202E, U+2066–2069 | reject | reject | reject raw; allowed as `\u{…}` escape |
| Bidi marks | U+200E LRM, U+200F RLM, **U+061C ALM** | reject | reject | reject raw; escape only |
| Zero-width and invisible format | U+200B ZWSP, U+200C ZWNJ, U+200D ZWJ, U+2060 WJ, U+FEFF, U+180E, U+00AD SHY, U+034F CGJ, U+115F/1160/3164/FFA0 (Hangul fillers) | reject | reject | reject raw, with one exception: **ZWJ between two `Extended_Pictographic` characters** (emoji sequences) is allowed raw |
| Tag characters | U+E0000–E007F | reject | reject | reject raw (escape only) |
| Variation selectors | U+FE00–FE0F, U+E0100–E01EF | reject | allowed after an emoji only | allowed after an emoji only; otherwise escape |
| Private use, unassigned, noncharacters | — | reject | reject | reject raw (escape only) |

Error codes: `SEC0110` (bidi), `SEC0111` (invisible), `SEC0112` (tag/PUA/unassigned). Each message names the code point and its Unicode name, shows the line rendered with that code point made visible (`⟨U+202E RIGHT-TO-LEFT OVERRIDE⟩`), and offers the `\u{…}` escape as a fix-it when the character is inside a string.

**Why strings too.** Report 04 U1 allows raw controls in strings with tooling; report 05 §1.6 requires escape-only everywhere. We take report 05's rule. Legitimate Arabic text never *needs* these characters, because the Unicode Bidirectional Algorithm orders it implicitly. A program that really needs one writes `"\u{200F}"`, which is visible in every editor and in every diff.

### 4.5 Identifiers

An identifier is a maximal run matching:

```
Ident      = IdStart IdContinue*
IdStart    = "_" | XID_Start ∩ AllowedScripts
IdContinue = XID_Continue ∩ AllowedScripts ∩ ¬Forbidden
```

Rules (U3, U4):

1. **UAX #31 base.** `XID_Start` / `XID_Continue`, Unicode version pinned per compiler release (v0.1: Unicode 16.0).
2. **Never starts with a digit,** in any digit system, in any skin (X5). `3ala` is an error. Write `_3ala` or `ala`.
3. **Single script.** Each identifier must be **Latin + Common** or **Arabic + Common** (UTS #39 "Single Script"). Digits and `_` are Common. `pаypal` (Cyrillic `а`) is `SEC0105`. Other scripts (Greek, Cyrillic, CJK, …) are allowed when the whole identifier is in that one script. A project may restrict this further with `[lint] scripts = ["Latin", "Arabic"]`.
4. **Forbidden in identifiers** (`SEC0106`, fix-it removes them): tatweel U+0640, harakat and Quranic marks (U+064B–065F, U+0670, U+06D6–06ED), ZWJ/ZWNJ, and Arabic presentation forms (U+FB50–FDFF, U+FE70–FEFF). Presentation forms are reported rather than NFKC-folded, so the file shows what it means.
5. **Persian/Urdu letter variants** ی U+06CC, ک U+06A9, ە U+06D5, ھ U+06BE, ۀ U+06C0 are **rejected in identifiers in v0.1** (`SEC0107`), with a fix-it to the Arabic letter (ي, ك, ه). This is the Arabic confusable table that report 04 §7 open question 3 says must be curated (a linguist review is pending, §23).
6. **Case-sensitive.** No case or style folding (research 01: style-insensitivity hurts grep).
7. **Raw identifiers.** `r#name` lets a word that is a keyword in this skin be used as an identifier. `fmt` emits it automatically when skin conversion would turn an identifier into a keyword (§5.5). The `r#` prefix is ASCII in every skin and is not part of the name.

**Skeleton and collision rule** (`SEC0104`). For every identifier, the compiler computes
`skeleton(id) = UTS39_skeleton( arabic_fold( NFKC(id) ) )`, where `arabic_fold` is:

- أ إ آ ٱ → ا
- ى → ي
- ة → ه
- ؤ → و
- ئ → ي
- Arabic-Indic digits → ASCII digits

and the UTS #39 skeleton maps cross-script lookalikes (`١`/`l`/`1`/`ا`, `٠`/`o`/`0`, …). It is an error if **two distinct identifiers with the same skeleton are both visible in one scope**, or are both exported from one module. The message shows both declarations side by side with their code points. For example, `على` (Ali) and `علي` (on) declared in one function collide. Renaming one of them is the fix.

### 4.6 Keywords

Keywords are matched against the identifier run **after** `arabic_fold` (§4.5) for the Arabic skin, so `اذا` and `إذا` are the same keyword, and `fmt` writes the canonical spelling. Tatweel or harakat inside a keyword are rejected by the same rule as identifiers (`SEC0106`), with an auto-fix. Keywords are reserved **only in their own skin** (X1, report 04 §4.2 option b). A Franco keyword such as `law` is an ordinary identifier in an English file. §5.5 gives the cross-skin escape rule.

### 4.7 Numeric literals

```
IntLit     = DecInt | "0x" HexDigit ("_"? HexDigit)* | "0b" BinDigit ("_"? BinDigit)*
DecInt     = Digit ("_"? Digit)*
FloatLit   = DecInt "." DecInt Exponent? | DecInt Exponent
Exponent   = ("e" | "E") ("+" | "-")? DecInt
ArabicInt  = ADigit ("_"? ADigit)*                    -- Arabic skin only
ArabicFlt  = ArabicInt "٫" ArabicInt                  -- Arabic skin only; no exponent
Digit      = "0".."9"      ADigit = "٠".."٩"
```

- A literal ends at the first character that is not part of the grammar above. **If an identifier character directly follows a numeric literal, it is error `K0120`** ("numbers can't be glued to letters"). For example `0x1Fg`, `1e5x`, `7f`, `3ala` are all `K0120`. In a Franco file, the lexer first checks whether the *whole* word is in the closed digit-leading alias table (§5.4). If it is, the word is that keyword.
- Leading zeros (`007`) are rejected (`K0121`), so no one confuses them with octal.
- A literal outside the range of its type is a compile error, not a wrap (`K0122`).
- Literal typing is *bidirectional*. An `IntLit` has type `Int` unless the expected type is `Float` or `Decimal`, in which case it is that type. This is exact, because it happens at compile time. It is not a runtime conversion. A `FloatLit` is always `Float`. For `Decimal`, use `dec"19.99"` (§16.7). Kalam has no numeric suffixes, because a suffix would be letters glued to digits.

### 4.8 String literals

```
StrLit       = '"' StrPart* '"' | '"""' NEWLINE StrLine* INDENT? '"""'
TaggedLit    = TagIdent StrLit          -- no whitespace between tag and quote
StrPart      = StrChar | Escape | "{" Expr "}"
Escape       = "\n" | "\t" | "\r" | "\\" | '\"' | "\{" | "\}" | "\0" | "\u{" HexDigit{1,6} "}"
```

- Strings are immutable sequences of Unicode scalar values. A lone surrogate escape (`\u{D800}`) is `K0130`.
- Interpolation `{expr}` is allowed in plain strings and in tagged literals. A string literal inside an interpolation hole is not allowed (`K0131`): bind it to a `let` first. Interpolating a value into a **plain** string requires the value's type to implement `Show` (§7.8). `Secret[T]` shows as `[REDACTED]`.
- Triple-quoted strings are multi-line. The common leading indentation (up to the closing `"""`) is removed. The closing `"""` must be on its own line.
- §4.4 applies to string contents.
- **Tagged literals** (`sql"…"`, `html"…"`, `shell"…"`, `url"…"`, `path"…"`, `regex"…"`, `dec"…"`) are covered in §16. The tag is an ASCII library identifier, identical in every skin.

### 4.9 Indentation and layout

- Blocks begin after a header line ending in `:` (or `=>` in a match arm, §10). The next non-blank line must be indented more deeply than the header line. That increase opens the block (INDENT). A return to an enclosing level closes it (DEDENT).
- Indentation is spaces only (§4.1). Any consistent increase is accepted. `fmt` rewrites to 4 spaces. A dedent to a column that matches no enclosing level is `K0140`.
- **Inside `(…)`, `[…]` and `{…}` (in interpolations), newlines and indentation are ignored.** That is the only line-continuation mechanism. There is no backslash continuation.
- There are no semicolons and no `؛`. One statement per line.
- A file must end with a newline. Blank lines and comment-only lines do not affect indentation.

### 4.10 Operators and punctuation

```
+  -  *  /  //  %  **     ==  !=  <  <=  >  >=     =  +=  -=  *=
.  ,  :  ->  =>  ?  ..  ..=  |>  |  @  (  )  [  ]  {  }  _
```

`|` appears only between pattern alternatives. `@` appears only at the start of an attribute line (§12.5).

`and`, `or`, `not` are keywords, not symbols (`&&`, `||`, `!` are rejected with a fix-it, `K0150`). There are no bitwise operators. Bit operations are methods (`a.bit_and(b)`, `a.shift_left(n)`, which panics if `n` ∉ 0..63), so shift-width undefined behaviour cannot arise.

---

## 5. Skins and the keyword table

### 5.1 Model

- A **skin** is a lexeme table: keywords, translated prelude names (§5.2), and the two Arabic punctuation aliases. Grammar, operators, literal syntax, library names and type names are **identical across skins** (report 02 §7.1).
- **One file = one skin** (D6). The skin comes from the file extension (§4.1).
- The parser consumes **skin-neutral tokens**. A keyword token carries a *concept ID* such as `KW_IF`, and the CST also records the exact spelling that was used. The canonical AST contains only concept IDs. This is what makes conversion lossless (§5.6).
- **Library and type names are never translated** in v0.1 (`Int`, `String`, `List`, `http.App`, `.map(…)`). This follows report 02 §7.3. Arabic alias packs are an open question (§23).

### 5.2 The keyword table (29 keywords) and translated prelude names (6)

Canonical spellings are listed first. **Aliases** apply in the Franco skin only (§5.4). Arabic spellings are shown in canonical form. Hamza, alef-maqsura and taa-marbuta variants match by folding (§4.6); they are not aliases.

| # | Concept ID | English | Arabic (فصحى) | Franco (Egyptian), canonical | Franco aliases (closed list v0.1) |
|---|---|---|---|---|---|
| 1 | `KW_FN` | `fn` | `دالة` | `dala` | `dalla`, `dalah` |
| 2 | `KW_LET` | `let` | `ليكن` | `khali` | `khalli`, `5ali`†, `5alli`† |
| 3 | `KW_VAR` | `var` | `متغير` | `motaghayer` | `motghayer`, `mot8ayer` |
| 4 | `KW_TYPE` | `type` | `هيكل` | `haykal` | `haikal`, `heikal` |
| 5 | `KW_ENUM` | `enum` | `تعداد` | `ta3dad` | `t3dad` |
| 6 | `KW_TRAIT` | `trait` | `سمة` | `sema` | `sima` |
| 7 | `KW_IMPL` | `impl` | `حقق` | `tabba2` | `tabbe2`, `tab2` |
| 8 | `KW_IF` | `if` | `إذا` | `law` | `lw`, `lau`, `low` |
| 9 | `KW_ELSE` | `else` | `وإلا` | `walla` | `wala`, `wla` |
| 10 | `KW_MATCH` | `match` | `حسب` | `hasab` | `7asab`† |
| 11 | `KW_FOR` | `for` | `لكل` | `lekol` | `lkol`, `likol` |
| 12 | `KW_IN` | `in` | `في` | `fi` | `fe`, `fy` |
| 13 | `KW_WHILE` | `while` | `طالما` | `tool_ma` | `tol_ma`, `tolma`, `toul_ma` |
| 14 | `KW_BREAK` | `break` | `اخرج` | `kfaya` | `kefaya`, `khalas`, `5alas`† |
| 15 | `KW_CONTINUE` | `continue` | `استمر` | `kammel` | `kamel`, `kml` |
| 16 | `KW_RETURN` | `return` | `ارجع` | `rag3` | `ragga3`, `raga3`, `rg3` |
| 17 | `KW_TRUE` | `true` | `صح` | `sa7` | `sah`, `sa7e7` |
| 18 | `KW_FALSE` | `false` | `خطأ` | `ghalat` | `ghalt`, `8alat`† |
| 19 | `KW_NONE` | `none` | `عدم` | `mafeesh` | `mafish`, `mfesh` |
| 20 | `KW_AND` | `and` | `و` | `we` | `wi`, `wa` (bare `w` is rejected as too collision-prone) |
| 21 | `KW_OR` | `or` | `أو` | `aw` | `ao` |
| 22 | `KW_NOT` | `not` | `ليس` | `mesh` | `msh`, `mish` |
| 23 | `KW_USE` | `use` | `استخدم` | `hat` | `hatly`, `hatli` |
| 24 | `KW_PUB` | `pub` | `علني` | `talla3` | `tala3`, `tl3` |
| 25 | `KW_AS` | `as` | `باسم` | `besm` | `bism` |
| 26 | `KW_SELF` | `self` | `ذات` | `nafso` | `nafs`, `nafsu` |
| 27 | `KW_TEST` | `test` | `اختبار` | `ekhtebar` | `e5tebar`, `ekhtbar` |
| 28 | `KW_UNSAFE` | `unsafe` | `unsafe` | `unsafe` | — (ASCII in every skin, §5.3) |
| 29 | `KW_EXTERN` | `extern` | `extern` | `extern` | — (ASCII in every skin, §5.3) |

† digit-leading alias, accepted only under the rule in §5.4.

**Translated prelude names.** These are not keywords. They are reserved prelude identifiers whose spelling follows the skin, so the same AST prints in every skin. They cannot be shadowed or redefined (`K0202`).

| Concept ID | English | Arabic | Franco | Franco aliases |
|---|---|---|---|---|
| `PRE_MAIN` (entry point) | `main` | `الرئيسية` | `asasy` | `asasi` |
| `PRE_PRINT` | `print` | `اطبع` | `ektb` | `ekteb`, `etb3`, `2ktb`† |
| `PRE_PANIC` | `panic` | `انهيار` | `kharab` | `5arab`† |
| `PRE_SOME` | `some` | `موجود` | `feeh` | `fih`, `fi_h` |
| `PRE_OK` | `ok` | `نجاح` | `tamam` | `tmam` |
| `PRE_ERR` | `err` | `فشل` | `fashal` | `fashl` |

**Word-choice notes (security and usability).**

- The Arabic keywords avoid words that Arabic developers commonly want as identifiers. `هيكل` (structure) replaces `نوع` (kind/type, common in product data). `حسب` (according to) replaces `طابق` (also "floor", common in real-estate apps). `حقق` (fulfil) replaces `طبق` ("dish", common in food apps). `استمر` replaces `تابع` ("subsidiary"/"follower"). `علني` replaces `عام` ("year"/"general"). `موجود` replaces `قيمة` ("value"). The CI check in §22 keeps a list of the 2,000 most frequent Arabic identifier words and fails if a keyword appears in it without a recorded justification.
- `false` (`خطأ`) and `err` (`فشل`) are deliberately different words, so a boolean and an error are never confused in the Arabic skin.
- In Franco, `some`/`none` read as `feeh`/`mafeesh` ("there is"/"there isn't"), which is the most natural Egyptian pair for presence and absence.

### 5.3 Escape-hatch keywords are ASCII in every skin (L13)

`unsafe` and `extern` are spelled in ASCII in all three skins. The same applies to the `r#` raw-identifier prefix and to the `allow(...)` lint attribute names (§12.5). A reviewer reading an Arabic file sees the Latin word `unsafe` stand out from the Arabic text. `kalam audit` counts escape hatches on the **AST** (report 04 §4.2), so the count does not depend on spelling.

### 5.4 Franco aliases: the closed-list rule

Franco orthography is not standardized (report 02 §2), so a zero-alias policy would frustrate users. Open-ended fuzzy matching would be a spoofing surface. v0.1 therefore uses a **closed, versioned alias table**:

1. Aliases are accepted **only in Franco-skin files**, and only from the table in §5.2. The table is versioned with the language edition. An alias is never added in a patch release.
2. `kalam fmt` rewrites every alias to the canonical spelling. `kalam check --strict`, which is the default in CI templates, **rejects** non-canonical spellings (`W0170` is promoted to an error). So a repository holds only canonical spellings, and reviewers see one form.
3. **Digit-leading aliases (†).** The lexer reads the maximal run of `[0-9A-Za-z_]`. If the run starts with a digit, then:
   - (a) if the whole run is a †-alias, it is that keyword or prelude name;
   - (b) otherwise, if the whole run matches the numeric-literal grammar (§4.7), it is a number;
   - (c) otherwise it is `K0120`, with a "did you mean" suggestion.
4. **Build-time proof obligations.** CI checks these against the alias table, and they are part of the conformance suite:
   - (i) no alias, canonical spelling or prelude name matches the numeric-literal grammar;
   - (ii) **no †-alias is within Damerau–Levenshtein distance 1 of any string that matches the numeric-literal grammar** (report 04 U5). CI checks this by generating every distance-1 neighbour of each alias and running it through the number lexer. All v0.1 entries pass;
   - (iii) no two entries in the table, across concepts, fold to the same skeleton.
5. "Did you mean" for Franco uses folding (`5→kh, 7→h, 3→a, 2→a, 8→gh, 4→sh`, vowel stripping, distance ≤ 2) **in diagnostics only**. It never silently accepts input (report 02 §7.6).

Arabic and English have **no aliases**. Orthographic folding of hamza, ى and ة in Arabic keywords is not an alias: it is the identity of the word.

### 5.5 Cross-skin collisions and the `r#` escape

- Keywords are reserved only in their own skin (X1). `law` is a fine identifier in `.kl` files.
- When `kalam fmt --skin=fr` converts a file in which an identifier equals a Franco keyword, alias or prelude name (after folding), the printer emits `r#law`. The same applies to every target skin. Converting back removes the escape only when the target skin does not need it.
- The LSP raises `W0171` ("`law` is a keyword in the Franco skin; this file will need `r#law` if converted") when such an identifier is *declared*. This is a warning, not an error, because it would otherwise ban ordinary English words (report 04 §4.2).

### 5.6 Lossless conversion and the canonical form

- `kalam fmt --skin=ar|en|fr` re-prints the **CST**. Keywords, prelude names, the two punctuation aliases and (per project setting) Arabic-Indic digits change. Identifiers (unless a project glossary is applied, §5.7), strings, comments, blank lines and library names are byte-identical (report 02 §7.2).
- **Round-trip guarantee:** for every file `f` and skins `A`, `B`: `print_A(parse(print_B(parse(fmt_A(f))))) == fmt_A(f)` byte-for-byte. This is parity gates P1–P4 in research 05, and it is part of conformance.
- **Kalam Canonical Form (KCF)** is a skin-free serialization of the CST: keywords as concept IDs, identifiers as NFC code points, trivia retained. The KCF hash is the unit of **signing, lockfile hashing and registry storage** (U7). The registry renders any skin on demand. Code review tools SHOULD let a reviewer choose a skin, and they MUST show the KCF hash, so "the PR I reviewed is the PR that was merged" holds across skins.

### 5.7 Identifier glossaries (optional, per project)

Converting keywords alone gives Arabic keywords around English identifiers. That is lossless, but it is not what an Arabic-reading team wants to see. A project can therefore add `names.toml`. It is a bijective table from **canonical names** (the names stored in KCF) to **display names** per skin:

```toml
[names]                # canonical = ar             = fr
User     = { ar = "مستخدم",  fr = "mostakhdem" }
name     = { ar = "اسم",     fr = "esm" }
greeting = { ar = "تحية",    fr = "ta7eya" }
```

Rules:

1. **Runtime-visible names are always canonical names.** These are JSON keys, DB column mapping, exported JS symbols and `Show` output of field names. A program's behaviour therefore does not depend on the skin it is displayed in (parity P5).
2. The table must be **bijective per skin**. Every display name must pass the identifier rules (§4.5). After applying the table, no display name may collide with another identifier, keyword or prelude name by skeleton (`SEC0108`). Applying and reversing the table is checked by the round-trip gate.
3. The glossary is a reviewed source file. `kalam audit` and the review renderer can show canonical names on demand.
4. Without a glossary entry, an identifier is displayed as is.

The examples in §21 are shown **with** a glossary applied. Strings and comments are never translated (§5.6), which is why the English string literals stay English in the Arabic and Franco versions. Real applications put user-facing text in an i18n table.

---

## 6. Grammar (EBNF)

### 6.1 Lexical tokens assumed by the grammar

- `NEWLINE`, `INDENT` and `DEDENT` come from the layout algorithm (§4.9). Inside brackets, the lexer suppresses them.
- **`BLOCK_COLON`** is a `:` whose next significant token is `NEWLINE`. Any other `:` is `COLON`. Since newlines inside brackets are suppressed, a `BLOCK_COLON` can never occur inside brackets. **This is what keeps `:` unambiguous.** A `BLOCK_COLON` always starts a block. A `COLON` is always a type annotation, a named argument or field, or a bound.
- **`ARM_ARROW_BLOCK`** is a `=>` whose next significant token is `NEWLINE`, by the same rule.
- Keyword tokens are concept IDs (`"if"` below means `KW_IF` in any skin). `TAG_STR` is a tagged literal token (§4.8).

### 6.2 Productions

```ebnf
(* ---------- files and items ---------- *)
File        = { NEWLINE } { Item { NEWLINE } } EOF ;
Item        = { Attribute } ( UseDecl | FnDecl | TypeDecl | EnumDecl | TraitDecl
            | ImplDecl | ConstDecl | TestDecl | ExternDecl ) ;
Attribute   = "@" Ident [ "(" Args ")" ] NEWLINE ;   (* ASCII names in every skin, §12.5 *)

UseDecl     = "use" ModPath [ "." "{" UseItem { "," UseItem } [ "," ] "}" ]
              [ "as" Ident ] NEWLINE ;
UseItem     = Ident [ "as" Ident ] ;
ModPath     = Ident { "." Ident } ;

ConstDecl   = [ "pub" ] "let" Ident [ COLON Type ] "=" ConstExpr NEWLINE ;
              (* module-level: compile-time evaluable, no capabilities, §12.3 *)

FnDecl      = [ "pub" ] [ "unsafe" ] "fn" Ident [ TypeParams ]
              "(" [ Params ] ")" [ "->" Type ] BLOCK_COLON Block ;
TypeParams  = "[" TypeParam { "," TypeParam } [ "," ] "]" ;
TypeParam   = Ident [ COLON TypePath { "+" TypePath } ] ;
Params      = Param { "," Param } [ "," ] ;
Param       = "self" | Ident COLON Type ;          (* top-level: annotation required *)
LambdaParam = Ident [ COLON Type ] ;

TypeDecl    = [ "pub" ] "type" Ident [ TypeParams ]
              ( "=" Type NEWLINE                    (* alias *)
              | BLOCK_COLON INDENT Field { Field } DEDENT ) ;
Field       = [ "pub" ] Ident COLON Type [ "=" ConstExpr ] NEWLINE ;

EnumDecl    = [ "pub" ] "enum" Ident [ TypeParams ] BLOCK_COLON
              INDENT Variant { Variant } DEDENT ;
Variant     = Ident [ "(" VField { "," VField } [ "," ] ")" ] NEWLINE ;
VField      = Ident COLON Type ;

TraitDecl   = [ "pub" ] "trait" Ident [ TypeParams ] BLOCK_COLON
              INDENT TraitFn { TraitFn } DEDENT ;
TraitFn     = "fn" Ident [ TypeParams ] "(" [ Params ] ")" [ "->" Type ]
              ( NEWLINE | BLOCK_COLON Block ) ;     (* body = default method *)

ImplDecl    = "impl" [ TypeParams ] TypePath [ TypeArgs ] [ "for" Type ]
              BLOCK_COLON INDENT FnDecl { FnDecl } DEDENT ;
              (* "impl T:"            inherent methods
                 "impl Show for T:"   trait implementation *)

TestDecl    = "test" STRING BLOCK_COLON Block ;

ExternDecl  = "extern" Ident STRING BLOCK_COLON INDENT ExternFn { ExternFn } DEDENT ;
              (* extern js "npm:slugify@1.6.6": ...  §18 *)
ExternFn    = { Attribute } [ "pub" ] "fn" Ident [ TypeParams ] "(" [ Params ] ")" [ "->" Type ] NEWLINE ;

(* ---------- types ---------- *)
Type        = BaseType [ "?" ] ;                     (* T?  ==  Option[T] *)
BaseType    = TypePath [ TypeArgs ]
            | "fn" "(" [ Type { "," Type } ] ")" "->" Type
            | "(" ")"                                (* Unit *)
            | "(" Type "," Type { "," Type } [ "," ] ")" ;  (* tuple *)
TypePath    = Ident { "." Ident } ;
TypeArgs    = "[" Type { "," Type } [ "," ] "]" ;

(* ---------- statements ---------- *)
Block       = INDENT Stmt { Stmt } DEDENT ;
Stmt        = LetStmt | VarStmt | AssignStmt | IfStmt | MatchStmt
            | ForStmt | WhileStmt | ReturnStmt
            | "break" NEWLINE | "continue" NEWLINE
            | UnsafeBlock | LocalFn | ExprStmt ;

LetStmt     = "let" Pattern [ COLON Type ] "="
              ( Expr "else" BLOCK_COLON Block        (* let-else: refutable pattern; block must diverge *)
              | ValueRhs ) ;                         (* irrefutable pattern *)
VarStmt     = "var" Ident [ COLON Type ] "=" ValueRhs ;
ValueRhs    = Expr NEWLINE | IfStmt | MatchStmt ;    (* block-valued if/match *)
AssignStmt  = Ident ( "=" | "+=" | "-=" | "*=" ) Expr NEWLINE ;
              (* only a `var` binding may be assigned; there is no field assignment *)

IfStmt      = "if" Expr BLOCK_COLON Block
              { "else" "if" Expr BLOCK_COLON Block }
              [ "else" BLOCK_COLON Block ] ;
MatchStmt   = "match" Expr BLOCK_COLON INDENT Arm { Arm } DEDENT ;
Arm         = Pattern [ "if" Expr ]
              ( "=>" Expr NEWLINE | ARM_ARROW_BLOCK Block ) ;
ForStmt     = "for" Pattern "in" Expr BLOCK_COLON Block ;
WhileStmt   = "while" Expr BLOCK_COLON Block ;
ReturnStmt  = "return" [ Expr ] NEWLINE ;
UnsafeBlock = "unsafe" BLOCK_COLON Block ;
LocalFn     = "fn" Ident "(" [ Params ] ")" [ "->" Type ] BLOCK_COLON Block ;
ExprStmt    = Expr NEWLINE ;

(* ---------- expressions, lowest to highest precedence ---------- *)
Expr        = PipeExpr ;
PipeExpr    = OrExpr { "|>" OrExpr } ;                       (* a |> f(b)  ==  f(a, b) *)
OrExpr      = AndExpr { "or" AndExpr } ;
AndExpr     = NotExpr { "and" NotExpr } ;
NotExpr     = "not" NotExpr | CmpExpr ;
CmpExpr     = RangeExpr [ CmpOp RangeExpr ] ;                (* non-associative *)
CmpOp       = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
RangeExpr   = AddExpr [ ( ".." | "..=" ) AddExpr ] ;
AddExpr     = MulExpr { ( "+" | "-" ) MulExpr } ;
MulExpr     = UnaryExpr { ( "*" | "/" | "//" | "%" ) UnaryExpr } ;
UnaryExpr   = "-" UnaryExpr | "unsafe" UnaryExpr | PowExpr ;
PowExpr     = PostfixExpr [ "**" UnaryExpr ] ;               (* right-assoc; -x**2 == -(x**2) *)
PostfixExpr = Primary { "." Ident | TypeArgs | "(" [ Args ] ")" | "?" } ;
Args        = Arg { "," Arg } [ "," ] ;
Arg         = [ Ident COLON ] Expr ;
Primary     = INT | FLOAT | STRING | TAG_STR
            | "true" | "false" | "none"
            | Ident | "self" | VariantRef
            | "(" ")" | "(" Expr ")" | "(" Expr "," Expr { "," Expr } [ "," ] ")"
            | "[" [ Expr { "," Expr } [ "," ] ] "]"
            | Lambda ;
VariantRef  = "." Ident ;                                    (* .Admin: variant of the expected type *)
Lambda      = "fn" "(" [ LambdaParam { "," LambdaParam } ] ")" [ "->" Type ] "=>" Expr ;

(* ---------- patterns ---------- *)
Pattern     = AltPat ;
AltPat      = AtomPat { "|" AtomPat } ;
AtomPat     = "_" | Ident                                    (* fresh binding, always *)
            | INT | STRING | "true" | "false" | "none"
            | INT ( ".." | "..=" ) INT
            | "(" Pattern "," Pattern { "," Pattern } [ "," ] ")"
            | "[" [ ListPatItem { "," ListPatItem } ] "]"
            | CtorPat ;
CtorPat     = ( "." Ident | TypePath "." Ident | PreludeCtor )
              [ "(" PatField { "," PatField } [ "," ] ")" ]
            | TypePath "(" PatField { "," PatField } [ "," ] ")" ;   (* record pattern: User(name, ..) *)
              (* LL(2): an Ident directly followed by "(" in a pattern is a record pattern;
                 a bare Ident is always a fresh binding *)
PreludeCtor = "some" | "ok" | "err" ;                        (* skin-translated prelude names *)
PatField    = [ Ident COLON ] Pattern | ".." ;
ListPatItem = Pattern | ".." [ Ident ] ;
```

### 6.3 Ambiguities removed on purpose

| Potential ambiguity | Resolution |
|---|---|
| `a[b]`: indexing or type arguments? | **There is no index syntax.** Postfix `[…]` is always type arguments (`json.decode[User](body)`). Element access is `xs.at(i)` (panics when out of range) or `xs.get(i)` (returns `T?`). A prefix `[…]` is always a list literal. |
| `:` as block start, annotation or named argument | `BLOCK_COLON` vs `COLON` is decided by the lexer (§6.1). Block lambdas are not allowed inside brackets: write `fn(x) => expr`, or declare a local `fn`. |
| Variant vs. fresh binding in patterns (`Admin =>`) | A **bare identifier in a pattern is always a fresh binding.** Variants are written `.Admin` (resolved against the scrutinee's type) or `Role.Admin`. In Rust and Swift a misspelled variant silently becomes a catch-all. In Kalam that is impossible, and an unreachable arm after a catch-all is error `K0402`. Arabic has no letter case, so a case-based rule could never work in every skin. |
| `a < b < c` | Non-associative comparisons: `K0160`. |
| `else if` vs nested `if` | `else if` is two tokens handled by `IfStmt`. `fmt` never nests. |
| `-x ** 2` | `-(x ** 2)`, as in mathematics. |
| Dangling `else` | Impossible with indentation blocks. |

The grammar is LL(2) after layout and colon classification. The reference parser is a hand-written recursive-descent parser in Rust (D10), with the EBNF above as its normative spec and a tree-sitter grammar generated from the same keyword table for editors.

---

## 7. Type system

### 7.1 Overview

- **Static, strong, nominal for declared types, structural for tuples and function types.** Kalam has **no subtyping** (no class inheritance, no implicit widening, no `any`, no top type). The only relation between types is equality after resolving aliases and type variables. This one choice removes variance, covariant-array unsoundness (Java, TS), and most of what makes TS unsound.
- **No implicit conversions** of any kind (L3). Every conversion is a named function: `Float.from(n)`, `n.to_float()`, `Int.parse(s)`, `x.to_string()`.
- **Generics** are parametric with trait bounds (§7.8). Instantiation is by inference or explicit `TypeArgs`.
- **Inference**: local, bidirectional, Hindley–Milner-based (§7.9). Top-level function signatures are written down.

### 7.2 Primitive types

| Type | Values | Key rules |
|---|---|---|
| `Int` | Signed 64-bit integers, −2⁶³ … 2⁶³−1 | `+ - * **`, unary `-` and `abs` **panic on overflow on every backend** (L7). `//` is floor division, `%` is floor modulo, and both panic on a zero divisor and on `Int.min // -1`. **`/` is not defined on `Int`** (`K0301`, fix-it `//` or `.to_float()`). Wrapping, saturating and checked variants are explicit methods: `a.wrapping_add(b)`, `a.saturating_mul(b)`, `a.checked_add(b) -> Int?`. |
| `Float` | IEEE-754 binary64 | `/` gives ±∞ or NaN per IEEE. **`Float` implements `PartialEq` but not `Eq`, `Ord` or `Hash`** (§7.8). `==` on floats warns (`W0302`, use `a.near(b, eps)`). Floats cannot be `Map` keys or `Set` members. Sorting uses `sort_by(Float.total_cmp)`. |
| `Decimal` | Exact base-10, 34 significant digits (IEEE 754 decimal128 semantics) | For money (L14, pulled in to v0.1). Literal: `dec"19.99"` (§16.7). Arithmetic is exact or panics on inexact division. `a.div_round(b, scale: 2, mode: .HalfEven)` is the explicit rounding API. |
| `Bool` | `true`, `false` | The **only** type accepted by `if`, `while`, `and`, `or`, `not` and match guards (L4). Kalam has no truthiness. |
| `String` | Immutable sequence of Unicode scalar values | `+` concatenates two `String`s only. `.len()` is **not provided**. Use `.byte_len()`, `.scalar_count()` or `.grapheme_count()`, so the unit is always explicit (an Arabic word with harakat has more scalars than graphemes). Slicing is by grapheme range: `.graphemes()`, `.take(n)`. |
| `Byte`, `Bytes` | 0–255, immutable byte sequence | `Bytes` is not `String`. Converting uses `.decode_utf8() -> Result[String, Utf8Error]`. |
| `()` (Unit) | One value `()` | The type of statements and of functions with no `->` |
| `Never` | No values | The type of `return`, `break`, `continue`, `panic(…)`, and a diverging `let … else` block. `Never` unifies with every type in that one direction (the bottom-type rule for diverging expressions only). It is not a subtype relation, because no value of type `Never` ever exists. |

**Why `Int` is 64-bit and trapping on every backend.** Report 03 §4 says "never just number". Report 04 L2 says that the same program must not behave differently on JS and C. So the *language* defines `Int` as checked 64-bit, and the JS backend implements that rule (§19.2). It does not get to redefine it. A program that works on the JS target will behave identically on the future C target.

### 7.3 Records (`type`)

```
type User:
    id: Int
    name: String
    email: Email
    role: Role = .Member
```

- **Nominal.** Two record types with identical fields are different types.
- **Immutable.** There is no field assignment. Updating is functional: `user.with(name: "Mona")` returns a new record. `with` takes named arguments only and is checked field by field.
- **Construction is by named arguments only**, all fields required unless defaulted: `User(id: 1, name: "Ali", email: e)`. A positional record constructor is `K0310`. Swapped fields of the same type (`User(first: last, last: first)`) are a classic bug that named construction makes visible.
- Field defaults must be `ConstExpr` (compile-time constant, §12.3).
- **Field visibility.** Fields are private to the module unless marked `pub`.
- **Opaque types.** A record with **any private field** is *opaque*. Outside its module an opaque type cannot be constructed, cannot be updated with `with`, and gets **no derived `Decode`**. This is how invariants are enforced: a smart constructor such as `Email.parse(s) -> Result[Email, …]` is the only way to make an `Email`, so every `Email` in the program has been validated. This is the type-level answer to "validate input once, at the edge".
- **Decoding an opaque type** is possible only through its own validator: `@decode_with(Email.parse)` on the type declaration. The named function must be in the same module, must be non-suspending, must take no capabilities, and must return `Result[Self, E]`. The decoder parses the wire primitive (for example a JSON string) and then calls that function. The target type is still fixed statically, so this adds no gadget risk (§17.3). `Url`, `PathSegment` and `Email` in the stdlib are decodable this way. `Sql`, `Html` and `Shell` are not decodable at all.

### 7.4 Enums / sum types (`enum`)

```
enum Role:
    Admin
    Member(team: String)
    Guest
```

- Variants are constructed as `Role.Admin`, `Role.Member(team: "Payments")`, or `.Member(team: "Payments")` when the expected type is known.
- Variant payloads use named fields. A single-field variant may be constructed positionally: `.Member("Payments")`.
- `match` on an enum must be **exhaustive** (L8, `K0401`). A wildcard `_` arm on an enum from *another package* draws a warning (`W0403`, "new variants added upstream will silently go to this arm"). Security-sensitive code (authorization roles, states) should list every variant. `kalam check --strict` promotes `W0403` to an error for enums marked `@exhaustive_strict`.

### 7.5 `Option` and `Result`

```
enum Option[T]:   (prelude; written T?)
    some(value: T)
    none

enum Result[T, E]:
    ok(value: T)
    err(error: E)
```

- **No null.** `none` is the single absent value (L5, report 03 §4). `T?` is sugar for `Option[T]`. Nested `T??` is legal and distinct from `T?` (§19.3 handles lowering).
- Kalam has no `!` "force unwrap" operator. `opt.unwrap()` exists, panics on `none`, and is lint `W0501` in non-test code (`--strict` makes it an error). The preferred forms are `match`, `let some(x) = opt else: …`, `opt.or(default)` and `?`.
- `Result` is `must_use` by construction (§9.6).

### 7.6 Tuples, lists, maps, sets

- Tuples `(A, B, …)` (2–8 elements), structural, immutable.
- `List[T]`: immutable, persistent. `List.builder()` gives a local mutable builder, which a closure cannot capture (§11.3).
- `Map[K, V]` and `Set[T]` require `K: Eq + Hash`. They are immutable and persistent. Hashing is **keyed SipHash with a per-process random key** (S14). Iteration order is **insertion order**, so it is deterministic and never leaks the hash key.
- There is **no object-as-map**. Records have fixed fields, and maps are a distinct type. On JS, a `Map` lowers to a JS `Map`, never a plain object, so prototype pollution (`__proto__`) cannot occur (report 04 §4.3).

### 7.7 Function types

`fn(A, B) -> C` is a structural function type. Function types also carry an **effect set** that the programmer does not write and the compiler infers. It currently has one bit, *suspends* (§14). Function values are not `Eq`, `Hash`, `Show`, `Encode` or `Decode`.

### 7.8 Traits

```
trait Show:
    fn show(self) -> String
```

- Traits are interfaces with optional default methods. Generic bounds use them: `fn largest[T: Ord](xs: List[T]) -> T?`.
- **Coherence:** at most one `impl Trait for Type` exists in the whole program (no overlapping impls). An impl must live in the package that defines either the trait or the type (the orphan rule). This makes method resolution deterministic and independent of import order, so adding a dependency can never change which code runs.
- **No trait objects in v0.1.** Kalam has no dynamic dispatch through traits. Use an `enum` or a record of functions. This avoids object-safety rules and vtable lowering in the first version (§23).
- **Built-in traits and automatic derivation.**

| Trait | Meaning | Derived automatically for records/enums when… | Never implemented by |
|---|---|---|---|
| `Eq` | `==` / `!=` | all fields are `Eq` | `Float` (has `PartialEq`), functions, capabilities, `Secret[T]` |
| `PartialEq` | IEEE-style `==` | all fields are `PartialEq` | functions, capabilities, `Secret[T]` |
| `Ord` | `<`, `sort` | all fields are `Ord` (enum: declaration order) | `Float`, `Secret[T]` |
| `Hash` | map/set key | all fields are `Hash` | `Float`, `Secret[T]` |
| `Show` | `"{x}"`, `print`, logs | all fields are `Show` | — (`Secret[T]` shows `[REDACTED]`; capabilities show `<Net>`) |
| `Encode` | JSON/wire output | all fields are `Encode` | **`Secret[T]`** (compile error, use `.expose()` explicitly), capabilities, functions |
| `Decode` | schema-first input (§17.3) | all fields are `Decode` **and** the type is not opaque (§7.3). Opaque types decode only through `@decode_with(validator)` | capabilities, functions, `Secret[T]` (comes from config loaders only), `Sql`/`Html`/`Shell`/`Path` (`Url` and `PathSegment` decode through their validating parsers) |
| `Into[T]` | explicit conversion used by `?` (§13.2) | never derived. Write it yourself | — |

- **Equality is only between identical types** (L4). `1 == 1.0` is `K0302` (types `Int` and `Float`). `user == "ali"` is `K0302`.

### 7.9 Inference rules

1. **Top-level functions:** every parameter must be annotated. The return type is required on `pub` functions and is inferred for private ones. This keeps every module boundary, and every capability a function needs (§15), visible in the signature.
2. **Locals:** `let`/`var` types are inferred. `let` bindings are **monomorphic** (no local let-generalization, the "MonoLocalBinds" rule). Generic code goes in named `fn`s. This keeps inference predictable and error messages local.
3. **Bidirectional checking.** The expected type flows into literals (§4.7), `.Variant` references, empty lists `[]`, lambdas and `json.decode` calls. Where no expected type exists and one is needed, the error is `K0320` ("I can't tell what type this is; add an annotation"), with a suggested annotation.
4. **Generalization** happens only at top-level `fn` declarations (rank-1 polymorphism). Kalam has no higher-rank types and no type-level computation, so inference is decidable and runs in near-linear time in practice.
5. **Trait resolution** is a coherent, non-overlapping lookup (§7.8). Because Kalam has no subtyping, unification is first-order syntactic unification with an occurs check.
6. **Effect inference** for *suspends* is a fixed point over the call graph (§14.3). It never affects typing, only lowering and tooling.
7. **Every expression has exactly one type.** Kalam has no overloading by argument type (L3). Methods on different types may share a name; resolution is by the receiver's type.

### 7.10 Mutability

- `let` bindings are immutable (L10). `var` bindings can be reassigned within their function.
- All built-in values (strings, records, enums, lists, maps, sets) are **immutable**. The only mutable *storage* is (a) `var` locals, (b) builder objects that cannot escape their function, and (c) stdlib state cells (`Cell[T]`, actors, §14.6) that are explicitly shared.
- So there is **no aliasing of mutable data** in safe code. Two references to one value can never observe different contents. That gives data-race freedom (L11) without a borrow checker, and it keeps the semantics identical when a multi-threaded backend arrives.

---

## 8. Soundness argument

This section states what "well-typed programs don't go wrong" means for Kalam and how the design supports it. The proof is not finished. A mechanized proof of **Kalam Core** is a 1.0 deliverable (§23).

### 8.1 Statement

Let **Kalam Core** be the language after desugaring: records, enums, `match`, first-class functions, `let`/`var`, `while`, `Result`/`Option`, traits after monomorphization, capabilities as opaque values, and *suspends* as an annotation. For every closed, well-typed Kalam Core program that contains no `unsafe` and no call into an `extern` function:

- **Progress.** Evaluation either produces a value, diverges, or stops with a **defined panic** (overflow, `at` out of range, division by zero, explicit `panic`, `unwrap` on `none`, stack exhaustion, out of memory).
- **Preservation.** Every value produced by an expression has the expression's static type.
- **Capability safety.** A capability value of type `C` exists at run time only if it was derived, through calls and attenuation, from the `World` handed to `main`. Code with no path to a `C` value cannot perform `C`'s effect (§15.4).
- **Literal provenance** (§16.2). Every value of type `Sql`, `Html`, `Shell`, `Url`, `Path` or `Regex` was built from developer-written literal text plus values that went through that type's encoder or binder. It never came from runtime string data, except through an `unsafe` call.

### 8.2 Design choices that make this tractable

| Hazard in other languages | Why it cannot arise in Kalam |
|---|---|
| Variance holes (covariant arrays in Java, method parameter bivariance in TypeScript) | No subtyping at all (§7.1). |
| `any`, unchecked casts, `as unknown as T` | Kalam has none of them. The `as` keyword is only for import renaming. Dynamic JS values are the opaque `JsValue`, which can only be inspected through `Decode`-style validation (§18.3). |
| Null inhabiting every type | Kalam has no null. `Option` is an ordinary enum. |
| Uninitialized variables, definite-assignment holes | Every `let`/`var` has an initializer. Module-level `let` is a compile-time constant (§12.3), so there is no initialization order to get wrong. |
| Exceptions that escape typed signatures | Kalam has no exceptions. `panic` is not catchable in the language (§13.4). |
| Overlapping instances / import-dependent dispatch | Coherence plus the orphan rule (§7.8). |
| Pattern-binding/variant confusion | Bindings and variants are syntactically distinct (§6.3). |
| Mutable aliasing / data races | Immutable values. No `var` capture by closures (§11.3). |
| JS boundary returning wrong shapes | Every `extern` result is **validated at the boundary** against the declared type (§18.3). A failed validation is a panic at the call site, or `err(JsError)` if the declared type is a `Result`. A mis-typed value never enters Kalam code. |
| Generated code diverging from spec on JS | The integer, string and equality semantics are specified by the language and implemented by runtime helpers (§19). Differential testing against the reference interpreter is a conformance requirement (§22). |

### 8.3 Known soundness boundaries (explicit, countable)

1. **`unsafe` functions and blocks.** They are used for `Html.unsafe_raw`, `Sql.unsafe_raw`, `JsValue.unsafe_cast` and similar. Every use is counted by `kalam audit` and shown on registry pages (P10).
2. **`extern` declarations.** The declared signature is a *claim* about JS code. Validation catches shape mismatches, but it cannot catch side effects. Externs are therefore counted, require a manifest entry, and must name the capabilities they consume (§18.4).
3. **Resource exhaustion** (stack depth, heap, time) is a defined panic, not a type error.
4. **Floating-point** non-determinism across platforms (fused multiply-add, `Math.sin` precision) is out of scope for v0.1. `Float` results may differ in the last ulp between backends. `Int` and `Decimal` results never differ.

---

## 9. Expressions and statements

### 9.1 Evaluation order

Evaluation order is **strictly left to right** everywhere: operands, arguments, record fields in source order, interpolation holes, and tagged-literal holes. Short-circuit `and`/`or` evaluate the right operand only when needed. Each backend must preserve this (C has unsequenced evaluation; the C backend must emit temporaries).

### 9.2 Operators and their types

| Operator | Operand types | Result | Failure |
|---|---|---|---|
| `+ - *` | `Int,Int` / `Float,Float` / `Decimal,Decimal` | same | `Int` overflow panics. `Decimal` panics when a result exceeds 34 digits |
| `/` | `Float,Float` / `Decimal,Decimal` | same | `Decimal`: panics on inexact division (use `div_round`) |
| `//` `%` | `Int,Int` | `Int` | zero divisor panics. `Int.min // -1` panics |
| `**` | `Int,Int` (exponent ≥ 0) / `Float,Float` | same | negative `Int` exponent or overflow panics |
| `+` | `String,String` | `String` | — |
| `== !=` | `T,T` where `T: PartialEq` | `Bool` | — |
| `< <= > >=` | `T,T` where `T: Ord` (or `Float`, IEEE) | `Bool` | — |
| `and or not` | `Bool` | `Bool` | — |
| `..` `..=` | `Int,Int` | `Range` | — |
| `\|>` | `a \|> f(b)` desugars to `f(a, b)` | | — |
| `?` | `Result`/`Option` (§13.2) | inner | — |

Mixed operand types are always `K0302`. For example, `price * 2` where `price: Decimal` requires the literal to be typed `Decimal` by the expected type. That works because literals are bidirectional (§4.7). `price * qty`, where `qty: Int`, is `K0302`, with the fix-it `price * Decimal.from(qty)`.

### 9.3 Blocks as values

A block's value is the value of its **last expression statement**. `if`/`else` and `match` used as a `let`/`var` initializer or as the last statement of a function are values. In that case every branch must produce the same type, and `if` must have an `else` (`K0330`).

```
let label = match n:
    0 => "none"
    1 => "one"
    _ => "many"
```

### 9.4 `let`, `var`, assignment

- `let x = e` binds immutably. `var x = e` binds mutably. Only `var` names can be targets of `=`, `+=`, `-=` or `*=`.
- **No shadowing.** Declaring a name that is already visible from an enclosing scope in the same function, a parameter, or an import is `K0340`. (Shadowing a module-level item with a local is also `K0340`.) Shadowing plus Arabic folding plus skeleton collisions would make "which `x` is this?" a review hazard.
- `let` with a refutable pattern requires `else`, and the `else` block must diverge (have type `Never`): `let some(user) = users.find(id) else: return err(.NotFound)`. This is Kalam's guard clause. It keeps the happy path unindented, and the compiler checks that the failure path leaves.

### 9.5 Loops

- `for pattern in iterable:` works over any `Iterable[T]` (lists, ranges, map entries as `(k, v)`, sets, strings via `.graphemes()`). The loop variable is a fresh immutable binding per iteration.
- `while cond:` where `cond: Bool`.
- `break` and `continue` apply to the innermost loop. Labelled breaks are excluded from v0.1.
- Kalam has no C-style `for (;;)`, no `do-while` and no `repeat` (X11).

### 9.6 Must-use (no silently ignored values)

**An expression statement whose type is not `()` or `Never` is `K0350`**, unless it is the last statement of a value block. To discard deliberately, write `_ = expr`. When the discarded value is a `Result`, the lint `W0351` reports it and `kalam audit` counts it (L6, OWASP A10:2025). This one rule covers both "unused Result" and "called a pure function for nothing".

### 9.7 Strings and interpolation

`"Hello, {user.name}"` calls `Show.show` on each hole. A hole whose type is not `Show` (functions, capabilities without `Show`) is `K0360`. Interpolation into a plain `String` is **never** a way to reach a sink, because sinks don't take `String` (§16).

---

## 10. Pattern matching

### 10.1 Forms

| Pattern | Matches | Binds |
|---|---|---|
| `_` | anything | nothing |
| `name` | anything | `name` (always a fresh binding, §6.3) |
| `42`, `"admin"`, `true`, `none` | equal literal (only for `Eq` types; float literals are `K0403`) | nothing |
| `1..=9` | integer range | nothing |
| `(a, b)` | tuple | components |
| `.Member(team)` / `Role.Member(team: t)` | enum variant | fields. `team` alone is shorthand for `team: team` |
| `User(name, ..)` | record (the type name is required) | listed fields. `..` ignores the rest |
| `some(x)`, `ok(v)`, `err(e)` | prelude variants | inner |
| `[first, ..rest]`, `[]`, `[a, b]` | list shape | elements / rest list |
| `p1 \| p2` | either. Both sides must bind the same names with the same types | |
| `pattern if guard` | pattern **and** `guard: Bool` | |

### 10.2 Exhaustiveness and reachability

- `match` must be exhaustive (`K0401`). The checker is the standard usefulness algorithm (Maranget). The error lists **concrete missing cases** (`.Guest`, `some(.Member(_))`), not just "non-exhaustive".
- Guards do not count toward exhaustiveness.
- An unreachable arm is an error (`K0402`), not a warning. That catches the "catch-all placed first" bug.
- Matching on `String`, `Int` or `Decimal` requires a final `_` or binding arm.
- `W0403` (wildcard over a foreign enum) is described in §7.4.

### 10.3 Matching never runs user code

Patterns only inspect structure and compare literals using built-in equality. No user `Eq` impl, getter or "extractor" runs during matching. Matching is pure, total and predictable, and it cannot be used to hide side effects.

---

## 11. Functions and closures

### 11.1 Declarations

```
pub fn transfer(db: Db, from: AccountId, to: AccountId, amount: Decimal) -> Result[Receipt, TransferError]:
    ...
```

- Parameters are immutable. To mutate, copy into a `var`.
- **Named arguments** are allowed at every call site and **required** where two adjacent parameters have the same type and the function is `pub` (`K0370` at the call site; in the example above, `from:` and `to:` must be named). This prevents swapped-argument bugs such as transferring money in the wrong direction.
- A function whose argument order is symmetric or universally conventional can opt out with `@positional` on its declaration. `kalam audit` lists `@positional` functions that have adjacent same-type parameters. The stdlib marks `clamp`, `min`, `max`, `crypto.equal`, `tasks.both`, the drawing primitives `circle`/`rect`/`line` (`x, y` first) and `Range` constructors this way. `Path.join` and money-moving APIs are deliberately **not** positional.
- Default parameter values are excluded from v0.1, and so are varargs.
- Recursion is allowed. Stack exhaustion is a defined panic. Kalam does not guarantee tail calls in v0.1, because JS does not.
- Methods are functions in `impl` blocks whose first parameter is `self`. `x.f(a)` is sugar for `T.f(x, a)`.

### 11.2 Lambdas

`fn(x) => x * 2`, with types inferred from the expected function type. Multi-statement anonymous functions are not allowed inside brackets (§6.3). Declare a local `fn` instead. A local `fn` may capture like a lambda.

### 11.3 Capture rules (closures)

1. Closures capture **by value**. Since values are immutable, capture by value is indistinguishable from capture by reference.
2. **A closure may not capture a `var`** (`K0380`, fix-it: `let snapshot = counter`). This removes the classic loop-closure bug, makes closures safe to hand to concurrent tasks (§14.5), and means a closure's behaviour never changes after it is created.
3. Closures *may* capture capabilities. That is how a handler gets its `Db`: `let handler = fn(req) => list_notes(db, req)`. A closure's captured capabilities are part of its inferred effect set, and the LSP shows them on hover.
4. Builders (`List.builder()`, `StringBuilder`) cannot be captured at all (`K0381`). They are second-class local values.

### 11.4 Higher-order functions and effects

A function parameter of type `fn(A) -> B` accepts both suspending and non-suspending functions. The callee is **effect-polymorphic** (§14.3). Programmers never write effect annotations.

---

## 12. Modules, packages and visibility

### 12.1 Files and modules

- **One file = one module.** The module path is the file path relative to the package's `src/`, without the extension: `src/billing/invoice.kl` → `billing.invoice`. Module names are identifiers and are **never translated** between skins. So a package may mix `.kl`, `.kla` and `.klf` files, one skin each (D6). Callers do not care which skin a module is written in.
- `use std.http.{App, Route}`, `use billing.invoice as inv`. Imports are **static**: `use` takes a path, never a string or an expression. Dynamic import does not exist (L9).
- **Import cycles between modules are rejected** (`K0390`). This keeps initialization trivial and makes the dependency graph a DAG for incremental compilation and audit.

### 12.2 Visibility

| Marker | Visible to |
|---|---|
| (none) | the same module |
| `pub` | any module that can import it. A `pub` item of a package is part of its public API |
| `pub` on a record field | readable outside the module. Construction needs *all* fields to be visible (§7.3) |

A `pub` item may not expose a private type in its signature (`K0391`). Kalam has no `friend`, `protected` or `internal` in v0.1.

### 12.3 Module-level items

- A module-level `let` must be a **constant expression**: literals, constructors of constant values, other constants, and calls to stdlib functions marked `const` (pure, total). It cannot take or produce a capability, cannot suspend, and cannot panic. A constant that would panic is a compile error.
- **No module-level `var`** (`K0392`). Kalam has no global mutable state at all. Shared mutable state lives in an explicit `Cell`/actor value created in `main` and passed down (§14.6). This is how "a request handler mutated a global" bugs become type errors.
- Module initialization therefore has no observable effects and no order.

### 12.4 Packages and the manifest

`kalam.toml` (sketch):

```toml
[package]
name = "@acme/notes"           # namespaced (P7)
edition = "2026"
skin_default = "ar"            # for `kalam new` files; files still declare their own skin

[capabilities]                 # what this package's *externs* may touch (§18.4)
net  = ["api.paymob.com"]
fs   = []
env  = ["DATABASE_URL"]

[ffi]
npm = { "slugify" = "1.6.6" }  # npm modules this package binds with `extern`

[format]
arabic_digits = false
```

- Lockfile with content hashes. Versions are immutable. No install-time code runs on any path (P1, P2).
- On upgrade, the package manager shows a **capability diff** for the whole dependency tree. Any capability *added* by a dependency requires explicit approval (P6).
- `kalam audit` reports per package: `unsafe` count, `extern` count, npm modules bound, capabilities declared, and discarded `Result`s.

### 12.5 Attributes and lints

`@allow(W0403)` and similar attributes on an item suppress a warning locally. The attribute name is ASCII in every skin, and `kalam audit` counts it. **Security errors (`SEC####`) cannot be allowed.** They can only be fixed, or routed through `unsafe` where the rule permits that.

---

## 13. Error handling: `Result`, `?`, panic

### 13.1 Two channels, by intent (D16)

| Situation | Channel | Example |
|---|---|---|
| The caller can reasonably handle it: bad input, not found, network down, permission denied | `Result[T, E]` | `Int.parse(s)`, `db.query(q)`, `http.get(url)` |
| A bug: an invariant was broken, the program is wrong | `panic` | integer overflow, `xs.at(99)` on a 3-element list, `unwrap()` on `none`, failed `assert` |

### 13.2 The `?` operator

`expr?` where `expr: Result[T, E1]` inside a function returning `Result[U, E2]`:

- evaluates to `t` if `ok(t)`;
- on `err(e)`, returns `err(e)` immediately when `E1 == E2`, or `err(Into.into(e))` when **an explicit `impl Into[E2] for E1` exists**. Otherwise the error is `K0410`, with a fix-it to write `.map_err(…)` or the `impl`.

`expr?` where `expr: T?` inside a function returning `U?` propagates `none`. Mixing the two (`?` on an `Option` in a `Result` function) is `K0411`, with the fix-it `.or_err(e)?`.

`?` is the only implicit control flow in the language. It is a postfix token at the exact point of exit. The LSP highlights every `?` as an exit point.

### 13.3 Error types

Error types are ordinary enums. They should be closed and specific. For example, `TransferError` would have the variants `InsufficientFunds`, `AccountFrozen(id: AccountId)` and `Db(cause: DbError)`. Kalam has no universal "any error" type in v0.1 (that would be a dynamic type, §8.2). `std.error.Report` is an enum-carrying wrapper for top-level application errors that keeps a chain of causes. It implements `Show`, with secrets redacted.

### 13.4 Panic

- `panic(message: String) -> Never`. `assert(cond: Bool, message: String)` panics on `false`. `unreachable(message)` is equivalent to `panic` and documents intent.
- **A panic cannot be caught in Kalam code.** There is no `try`, `catch` or `recover`. A panic unwinds the current **task** (§14) and runs no user code on the way out (Kalam has no destructors or `finally`). Resources owned by the runtime (DB transactions, files, sockets) are released by the runtime: an open transaction is **rolled back** and a held lock is released.
- **Boundaries** that *contain* a panic are runtime constructs only:
  - an **HTTP request**: the request fails with 500 and a request ID, and the panic is logged. **The response body never contains the panic message, stack trace or source location in production builds** (S11). The server keeps serving.
  - a **supervised task** (`tasks.supervise`): the supervisor sees `err(TaskPanicked(id))`.
  - **`main`**: the process exits with code 101, after printing the message to stderr. A trace is printed in dev builds only.
- Production builds have no runtime switch that turns traces on (S11). A dev build is a different artifact.
- Panic messages are sanitized: `Secret` values inside them show as `[REDACTED]`.

### 13.5 `main` and exit codes

`main` may return `()` or `Result[(), E]` where `E: Show`. `err(e)` prints `e` to stderr and exits with code 1. A panic exits with 101.

---

## 14. Concurrency without an `async` keyword

### 14.1 Programmer's model

- Code is written sequentially. **There is no `async`, `await`, `Promise` or `Future` in the language** (D12). A call to `db.query(q)` returns the rows. The program waits for them.
- Concurrency happens only through **structured, explicit library constructs** (`tasks.both`, `tasks.all`, `tasks.race`, `tasks.group`, channels, actors). Code without them runs sequentially, even when it does I/O.
- The runtime is single-threaded per isolate on the JS target (event loop). The future C target may use threads. Kalam's immutability and no-`var`-capture rules (§7.10, §11.3) mean that a correct program behaves the same either way.

### 14.2 Suspension points

A **suspension point** is a call where the current task may pause and let other tasks run. In v0.1 these are:

1. calls to capability methods documented as suspending (`Net`, `Fs`, `Db`, `Clock.sleep`, `Screen.next_frame`, channel `send`/`receive`, `tasks.*` joins);
2. calls to `extern` functions whose declared result is `Js.Promise[T]` (§18.2);
3. calls to any function or function value whose inferred effect includes *suspends*.

**Tooling obligation (normative for the reference LSP and `fmt --show-suspends`).** Every suspension point is marked inline (an inlay hint `⏸` before the call). A function's hover shows "suspends". The inlay is not in the file, so it does not affect skins or hashes, but the LSP and the code-review renderer must show it. This replaces the visual cue that `await` would have provided.

### 14.3 Effect inference (the *suspends* bit)

- Each function body gets an effect set `{}` or `{suspends}`. The compiler computes the least fixed point over the call graph: a function suspends iff its body contains a suspension point (§14.2) after inlining nothing. Strongly connected components (mutual recursion) are solved together.
- **Function values.** A function type `fn(A) -> B` written by a programmer is **effect-polymorphic**. A higher-order function such as `List.map(self, f: fn(T) -> U) -> List[U]` gets the effect "suspends iff `f` suspends". The compiler represents this with an effect variable per function-typed parameter.
- **Function values stored in data** (record fields, list elements, map values) carry the effect of the value actually stored. A record field type `fn(Request) -> Response` gets one effect per *record type*: if any constructor site stores a suspending function, the field is treated as suspending everywhere (whole-program, monotone). This is conservative: it can only add `await`s, never remove a needed one.
- **Lowering consequence (JS).** A suspending function is emitted as an `async function`, and each suspending call as `await`. An effect-polymorphic function is **specialized**: the compiler emits a sync version and an async version on demand. If a function has more than 4 function-typed parameters, it gets only the async version, so code size stays bounded. Calls from a non-suspending context never see a Promise.

### 14.4 Interleaving and TOCTOU: what can and cannot happen

A hidden suspension point means other tasks may run "in the middle" of a function. Kalam limits what they can affect:

- **Locals can't change underneath you.** `var`s are function-local and cannot be captured (§11.3), and values are immutable. Another task can never modify any value the current function holds.
- **Shared state is explicit.** The only things another task can change are external resources (a database row, a file, a remote API) and explicit `Cell`/actor state (§14.6). The race risk is therefore *check external state → suspend → act on stale assumption*, the classic TOCTOU.
- Mitigations:
  - (a) `Cell.update(fn(v) => …)` is atomic because its closure **must not suspend** (`K0420` if it does). So read-modify-write on shared state can't interleave.
  - (b) `db.transaction(fn(tx) => …)` runs the closure in a DB transaction (serializable by default in the first-party Postgres driver).
  - (c) Suspension points are visible in tooling (§14.2).
  - (d) Lint `W0421`: "value read from a `Cell` before a suspension point and used after it". This is the in-process TOCTOU shape.

### 14.5 Structured concurrency API (stdlib, no keywords)

```
let (user, orders) = tasks.both(fn() => users.get(db, id), fn() => orders.for_user(db, id))
let pages = tasks.all(urls.map(fn(u) => fetch_page(net, u)))      # List[Result[Page, HttpError]]
let first = tasks.race([fn() => primary(net), fn() => mirror(net)])

fn notify_all(g: TaskGroup):                          # a local fn, since block lambdas
    g.spawn(fn() => send_email(mail, a))              # are not allowed inside brackets
    g.spawn(fn() => send_sms(sms, b))
tasks.group(notify_all)                               # returns after both children finish
```

- **Structured.** Every task is a child of a group or call that waits for it. **No task outlives its parent scope.** Kalam has no fire-and-forget `spawn`. A background job is `jobs.enqueue(…)` against a durable queue (research 06).
- **Cancellation.** When a parent is cancelled (request timeout, `race` loser, a sibling panic in a group), cancellation is delivered **at the child's next suspension point**: that suspending call returns `err(.Cancelled)` if its type is a `Result`, or unwinds the task like a panic otherwise. Cancellation can never interrupt non-suspending code, so invariants held between suspension points are preserved.
- **Sendability.** Values passed to tasks are immutable, and closures cannot capture `var` (§11.3). Every closure is therefore safe to run concurrently. Kalam needs no `Send` trait in v0.1, and adding threads later does not change the type system.
- Timeouts: `tasks.timeout(clock, 2.seconds(), fn() => …) -> Result[T, Timeout]`.

### 14.6 Shared state: `Cell` and actors

`Cell[T]` is a mutable reference whose operations are `get`, `set` and an atomic `update` with a non-suspending closure. `Actor[S, M]` processes messages one at a time. Both are values created in `main` (or by a component) and passed down explicitly. Neither can be stored in module-level `let` (§12.3).

### 14.7 Lowering summary for JS

| Kalam | Emitted JS |
|---|---|
| non-suspending `fn` | `function f(…) { … }` |
| suspending `fn` | `async function f(…) { … }` |
| suspending call | `await f(…)` |
| `tasks.both(a, b)` | `await __k.both(a, b)`, a runtime helper over `Promise.all` with an `AbortController`-based cancel tree |
| cancellation | `AbortSignal` threaded through the runtime's task context. Suspending stdlib calls check it on entry and on resume |
| `Screen.next_frame()` | `await new Promise(r => requestAnimationFrame(r))` |
| HTTP handler | an async handler registered with the runtime's server adapter (Node `http`, Bun or Deno) |

Debugging uses source maps (D4). Async stack traces in V8 map back to Kalam frames.

---

## 15. Capabilities and effects for I/O

### 15.1 The `World`

```
fn main(world: World):
```

`World` is a record of capability values, constructed only by the runtime and passed only to `main`:

| Field | Type | Grants | Attenuation examples |
|---|---|---|---|
| `world.net` | `Net` | outbound connections, listening sockets | `net.only(["api.paymob.com"])`, `net.listen_only(8080)` |
| `world.fs` | `Fs` | file system | `fs.subdir(path"./uploads")`, `fs.read_only()` |
| `world.env` | `Env` | environment variables | `env.only(["DATABASE_URL"])`. Values matching `*_KEY`, `*_SECRET`, `*_TOKEN`, `PASSWORD` come back as `Secret[String]` (S5) |
| `world.proc` | `Proc` | spawning processes (`Shell`, §16.5) | `proc.only([path"/usr/bin/git"])` |
| `world.clock` | `Clock` | wall time, monotonic time, sleep | — |
| `world.random` | `Random` | cryptographically secure random numbers (S7) | — |
| `world.screen` | `Screen` | browser canvas and input (JS browser target) | — |
| `world.stdin` | `Stdin` | reading standard input (`input()`) | — |

A program that does not need `World` declares `fn main():`. Writing to stdout with `print` needs no capability (§15.6).

### 15.2 Capability values

- Capability types have **no public constructor**, no `Decode`, no `Eq` and no `Hash`. They show as `<Net>` and cannot be serialized. You can only get one from `World` or by **attenuating** one you already hold. Attenuation methods return a strictly weaker capability.
- Capabilities are ordinary values. They can be passed as arguments, captured by closures and stored in records, which is how a request context carries `db`. They **cannot be stored in module-level constants** (§12.3), so ambient authority cannot be smuggled in through a global.
- Higher-level handles are built from capabilities: `Db.connect(world.net, url: config.database_url)` returns `Result[Db, DbError]`, and `Mailer.new(net, key)`. A library that needs a database takes `db: Db`. It never takes a connection string and opens the connection itself.

### 15.3 What this buys (worked threat)

A compromised dependency `@evil/leftpad` has the API `pad(s: String, n: Int) -> String`. It cannot read `~/.aws/credentials` or call home, because:

1. no capability reaches it through its signature;
2. it cannot construct one;
3. it cannot obtain one from a global;
4. its only route to ambient authority would be an `extern`. That needs an `[ffi]` entry in its manifest, which shows up in the capability diff and in `kalam audit` on upgrade (P6).

This is report 04 §3.3's "turn the exfiltration incident into a type error".

### 15.4 Formal rule

For any function `f`, the set of effects `f` can perform is bounded by the capability *types* reachable from its parameters and captured values, plus the effects of `extern`s its package declares. The compiler reports this set per public function (`kalam caps @acme/notes`), and the registry shows it.

### 15.5 Defence in depth on the JS target

The type rule constrains Kalam code. The npm code underneath is constrained by the runtime. The Node/Deno launcher generated by `kalam build` derives OS-level permission flags from the *root* manifest's capability list: Node `--permission` with `--allow-fs-read=…`, or Deno `--allow-net=…`. A bug in an extern binding then still hits a second wall.

### 15.6 Deviation: `print` is ambient (X9)

`print(x: Show)` writes to stdout and `eprint` writes to stderr, with no capability. Rationale: hello-world stays one line. Stdout and stderr go to the operator, not to an attacker-chosen destination. `Show` redacts `Secret`s. Limits on this deviation:

- (a) A **library** package that calls `print`/`eprint` must declare `[capabilities] console = true`, which appears in the capability diff.
- (b) Production web servers route stdout into the structured logger, where log-injection characters (CR/LF, ANSI escapes) are escaped (CWE-117).
- (c) `input()` (reading stdin) is **not** ambient. It needs `world.stdin`.

---

## 16. Safe string kinds: `Sql`, `Html`, `Shell` and friends

### 16.1 The rule

**A sink never accepts `String`** (S1). Every API that interprets text as code or as a structured command takes a distinct type. That type can be built only from (a) a **tagged literal**, whose literal parts the developer wrote and whose holes are encoded for their context, (b) a **validating parser** that returns `Result`, or (c) an `unsafe` function. Kalam has no implicit or explicit `String → Sql` cast.

| Tag | Type | Sink examples | Hole handling | Unsafe escape (counted) |
|---|---|---|---|---|
| `sql"…"` | `Sql` | `db.query`, `db.execute`, `tx.query` | **bound parameter** (`$1`…), never spliced | `Sql.unsafe_raw(s)` |
| `html"…"` | `Html` | HTTP responses, components, `live` updates | **contextual auto-escape** (text / attribute / URL / forbidden) | `Html.unsafe_raw(s)` |
| `shell"…"` | `Shell` (an argv vector) | `proc.run`, `proc.spawn` | **exactly one argv element**, never split, never globbed, no shell | — (no raw form exists) |
| `url"…"` | `Url` | `http.get`, `href`/`src` holes, `redirect` | percent-encoded per URL component | `Url.unsafe_raw(s)` |
| `path"…"` | `Path` | `fs.read`, `fs.write`, `fs.subdir` | hole must be `PathSegment` (§16.6) | — |
| `regex"…"` | `Regex` | `.matches`, `.replace` | hole is escaped as literal text | — |
| `dec"…"` | `Decimal` | arithmetic | no holes | — |

Tags are lowercase ASCII names, identical in every skin. They are resolved statically. A tag is **registered** by a `Template` impl annotated `@tag("sql")` (§16.8). The stdlib tags `sql`, `html`, `shell`, `url`, `path`, `regex` and `dec` are always in scope (the prelude). A third-party tag comes into scope when its defining module is imported. Two tags with the same name in scope are `K0607`. Writing `sql "…"` with a space is a call on a string and type-checks as such, so it is rejected because sinks never take `String`.

### 16.2 Literal provenance (the soundness property)

The compiler hands the tag's builder a value of type `StaticParts`. Only the compiler can create a `StaticParts`, and it is made **only from literal source text**. The holes arrive separately as typed values. Therefore:

- runtime strings cannot become the *structure* of a query, a page or a command;
- the structure of every `Sql`/`Html`/`Shell`/`Url` value in the program appears verbatim in the source code, so a reviewer (or `kalam audit --sinks`) can list every query the program can ever run.

For the stdlib tags, the literal text is **parsed at compile time**. Malformed SQL quoting, unbalanced HTML, or shell metacharacters are compile errors, not runtime surprises.

### 16.3 `Sql`

```
let q = sql"SELECT id, title FROM notes WHERE owner_id = {user.id} AND created_at > {since}"
let rows: List[Note] = db.query(q)?
```

- Every hole becomes a **bound parameter**. The value never enters the SQL text. Accepted hole types are those with `impl SqlParam`: `Int`, `Float`, `Decimal`, `String`, `Bool`, `Bytes`, date/time types, `T?` (as `NULL`), `List[T]` (as an array parameter, used with `= ANY({ids})`), and `Sql` (splices a fragment that is itself safe).
- **A hole inside an SQL string literal** (`'…{x}…'`) is `K0601` ("this hole is inside quotes; remove the quotes, the value is sent as a parameter").
- **A hole in an identifier position** (table or column name, `ORDER BY {col}`, `ASC/DESC`) is `K0602`. Dynamic identifiers come only from `SqlIdent`, which can be built only from a literal: `enum SortBy` with an `impl` that returns `sql.ident("created_at")` for each variant. User input picks an enum variant. It never names a column.
- **Multiple statements** (a `;` outside quotes) are `K0603`. Each `Sql` value is exactly one statement. This blocks stacked queries even through `unsafe_raw`, which is rejected at run time if it contains a top-level `;`.
- Comments `--` and `/* */` inside the literal are allowed. A hole inside a comment is `K0604`.
- `LIKE` wildcards: `sql.like_contains(search)` escapes `%`, `_` and `\` and returns a `SqlParam`. A raw `String` used in a `LIKE` hole draws `W0605`, because the user could inject wildcards (a logic bug, and a ReDoS-like cost on big tables).
- Results decode **schema-first** into the expected type (`List[Note]`), using the column names. A missing or extra column is `err(DbError.Shape)` (§17.3).
- Planned for P2: compile-time checking against the live schema (sqlx-style, S16).

### 16.4 `Html`

`html"…"` is parsed at compile time as an HTML5 fragment, and each hole gets a **context** (Go `html/template` semantics, S3):

| Hole position | Accepted types | Encoding / rule |
|---|---|---|
| Text content `<p>{x}</p>` | any `Show` type, `Html`, `List[Html]`, `Html?` | `Show` values are HTML-escaped. `Html` is inserted as is (it is already safe) |
| Quoted attribute value `class="{c}"` | `String`, `Int`, `Bool`, `AttrValue` | attribute-escaped |
| URL attribute `href`, `src`, `action`, `formaction`, `poster`, `srcset`, … | **`Url` only** (`K0611` for `String`) | `Url` cannot hold `javascript:`, `data:` or `vbscript:` unless built with `unsafe` |
| Event-handler attribute `on*` | **no holes allowed** (`K0612`) | use `live:` actions (§16.4.1) |
| `<script>` / `<style>` content, `style` attribute | **no holes** (`K0613`), except `{json(x)}` in `<script type="application/json">` | `json(x)` encodes for that context (escapes `<`, `>`, `&`, U+2028, U+2029) |
| Unquoted attribute `class={c}` | — | `K0614` (fix-it adds quotes) |
| Tag name or attribute name `<{t}>`, `<a {name}="x">` | — | `K0615`. Use `Attrs` for a dynamic attribute *set* with a closed set of names |
| Inside an HTML comment | — | `K0616` |

- The literal must be **well-formed** (balanced tags, valid nesting, void elements). A violation is a compile error, not a browser repair.
- Components are just functions returning `Html` (§21 example 7).
- **Arabic and RTL.** The first-party UI kit adds `dir="auto"` to text containers and `lang` from the request locale. Bidi controls inside text holes are not stripped (they are data), but the UI kit wraps every interpolated text hole in `<bdi>` when the surrounding element has a fixed `dir`. This stops user-supplied text from reordering the page around it (a display-level Trojan Source).

#### 16.4.1 Server-driven events (D11)

`<button live:click="{delete_note(note.id)}">` takes a hole of type **`Action`**. An `Action` is a server-side closure reference created by the framework. On the wire it is an opaque, signed, CSRF-bound token, never JavaScript. The live layer (SSE/WebSocket) sends `Html` patches. On the browser side, the runtime inserts them only through a Trusted Types policy that accepts nothing but framework-produced `Html` (§19.4).

### 16.5 `Shell` (argv only, no shell)

```
let cmd = shell"git log --oneline -n {count} -- {file}"
let out = proc.run(cmd, env: Env.empty())?
```

- The literal is split into argv words **at compile time**, at whitespace outside double quotes in the literal. Each hole becomes **exactly one argv element**, or part of one when it is glued to literal text (`--author={name}` is one element). Hole values are never split, globbed, tilde-expanded or variable-expanded.
- **The program (first word) must be literal text** (`K0621`).
- **Shell metacharacters in the literal** outside quotes (`| ; & && || > < >> $( ) \` * ? ~ !`) are `K0622` ("Kalam never runs a shell. Use `proc.pipe([…])` for pipelines, and pass files with `stdin:`/`stdout:`").
- **Argument injection.** If a whole-word hole *value* starts with `-` and no literal `--` precedes it, `proc.run` returns `err(.OptionLikeArgument(index))` without starting the process. This stops `git` `--upload-pack=` style attacks. The compiler warns (`W0623`) at such holes and suggests adding `--`.
- Hole types: `String`, `Int`, `Path`, `PathSegment`. A hole of type `List[String]` expands to several elements, one per item.
- The child process gets an **empty environment** unless `env:` is passed explicitly, and the `Proc` capability can restrict which programs may run (`proc.only([…])`).
- `Shell` has no `unsafe_raw`. A program that genuinely needs a shell must write `shell"/bin/sh -c {script}"`. The literal `-c` is visible, `script` is an ordinary hole, and `kalam audit` flags any `Shell` whose program is a known shell interpreter.

### 16.6 `Url`, `Path`, `PathSegment`, `Regex`

- `url"https://api.paymob.com/v1/orders/{order_id}?lang={lang}"`: the scheme and authority must be literal text. Holes are percent-encoded for their component (path segment, query key or value, fragment). A hole that would form the scheme or host is `K0631`. Runtime URLs come from `Url.parse(s) -> Result[Url, UrlError]`, which rejects `javascript:`, `data:` and `vbscript:`. `redirect(to: Url)` additionally requires a relative URL or a host on the app's allow-list (S17).
- `path"uploads/{name}"`: the hole type must be `PathSegment`, not `String` (`K0641`, fix-it `PathSegment.parse(name)?`). `PathSegment.parse` rejects empty strings, `.`, `..`, `/`, `\`, NUL, drive letters, and Windows reserved names (`CON`, `NUL`, …). `Path.join(base, rel)` returns `Result` and refuses to escape `base` (S13). `Fs` operations are always relative to the capability's root, and symlinks that leave the root are refused.
- `regex"…"` is compiled at compile time by a **linear-time engine** (RE2 semantics, no backreferences or lookaround), so ReDoS (CWE-1333) is impossible. `Regex.parse(s) -> Result` exists for runtime patterns and uses the same engine.

### 16.7 `dec"…"`

`dec"19.99"` is a `Decimal` constant, validated at compile time. Holes are not allowed.

### 16.8 User-defined tags

```
pub trait Template:
    fn build(parts: StaticParts, holes: List[TemplateHole]) -> Self

@tag("graphql")
impl Template for GraphQl:
    fn build(parts: StaticParts, holes: List[TemplateHole]) -> GraphQl:
        ...
```

A package can define its own safe-string type (for example `graphql"…"` or `ldap"…"`) by implementing `Template` and registering a tag. The compiler guarantees `StaticParts` provenance (§16.2). The package is responsible for its encoder, and it may validate the template structure at compile time by providing a `const fn check(parts: StaticParts) -> Result[(), String]`. `kalam audit` lists third-party `Template` impls as security-relevant code.

### 16.9 What tagged literals do not solve

Tagged literals prevent **injection**. They do not prevent:

- **authorization bugs** (IDOR). The HTTP router is deny-by-default (S9): every route must state `Access.public`, `Access.login` or `Access.role(…)`, or the app fails to build;
- **business-logic errors**;
- **second-order data problems**, such as storing HTML-looking text, which is safe because output is escaped again at its sink.

These limits are listed so no one oversells the guarantee.

---

## 17. Secrets, randomness, decoding

### 17.1 `Secret[T]`

- Created by config loaders (`env.get` on secret-like names, `config.load[AppConfig]` for fields typed `Secret[…]`) and by `Secret.new(x)`.
- `Show` prints `[REDACTED]`. It has no `Encode`: putting a secret in JSON is a compile error, not a redaction. It has no `Eq`, `Ord` or `Hash`: comparison is `crypto.equal(a, b)`, which is constant-time (S5).
- `.expose() -> T` returns the value. `kalam audit` counts every `.expose()` call. The method name is ASCII in every skin, because library names are never translated.
- The memory is zeroized on drop where the backend allows it. On JS this is best effort only (strings are immutable), so `Secret[Bytes]` is preferred for keys.

### 17.2 Randomness

- `world.random` is a CSPRNG (`crypto.getRandomValues` on JS). It provides `random.int(0..10)`, `random.token(bytes: 32) -> Secret[String]` and `random.uuid_v4()`.
- `FastRng.seeded(seed: Int)` is a deterministic PRNG (for games, simulations and tests) and needs **no capability**, because it is pure. It has **no** `token` or `uuid` methods. Using it for secrets is a type error by omission.

### 17.3 Decoding (schema-first, S4)

- `json.decode[T](input: Bytes, limits: DecodeLimits) -> Result[T, DecodeError]` (with `json.decode_str` for `String` input and `DecodeLimits.default()` as the usual limits) builds exactly a `T`. It never builds a type named by the payload. `Decode` is derived structurally and cannot be hand-written in v0.1. The only user code a decoder ever runs is an opaque type's `@decode_with` validator (§7.3). That validator is pure, non-suspending, takes no capabilities, and is chosen by the static target type, never by the payload.
- Defaults: size 1 MiB, depth 32, 10,000 elements per container, 64 KiB per string. **Unknown fields are rejected. Duplicate keys are rejected.** Integers outside `Int` range are rejected (no silent float). NaN and Infinity are rejected.
- The same rules apply to request binding (`req.json[T]()`, `req.form[T]()`, `req.query[T]()`) and to DB row decoding. That covers mass assignment, S10: only declared fields can be set. **Guidance, enforced by lint `W0710`:** decoding request data directly into a type that is also used as a DB row type is flagged. Define a separate input type, so a field such as `role` or `owner_id` cannot be bound from the request.

### 17.4 Cryptography (summary)

`std.crypto` exposes only high-level, misuse-resistant APIs:

- `password.hash`/`verify` (Argon2id);
- `aead.seal`/`open`;
- `sign`/`verify` (Ed25519);
- `hkdf`;
- `crypto.equal`.

MD5, SHA-1 and raw RSA live only under `std.crypto.legacy`. Importing it draws `W0720`, and `kalam audit` counts it (S6).

---

## 18. Interop with JavaScript and npm

### 18.1 Principles

1. Interop is required for adoption (research 03, 06). It is also the main trust boundary, so it is **explicit, typed, validated, declared in the manifest and counted**.
2. Calling *into* JS goes through `extern` declarations. Calling *from* JS uses generated ES modules plus `.d.ts` files.
3. A JS value never enters Kalam code without being checked against its declared Kalam type.

### 18.2 Declaring externs

```
extern js "npm:slugify":
    @js(name: "default")
    fn slugify(text: String, options: SlugOptions) -> String

extern js "npm:@paymob/sdk":
    fn create_intention(net: Net, key: Secret[String], order: IntentionInput) -> Js.Promise[Result[Intention, JsError]]
```

- The source string is `npm:<package>` (it must appear in `[ffi] npm` in the manifest, `K0801` otherwise), `node:<module>` (Node built-ins, which also need a manifest entry), or `./file.js`, a package-local JS file that is audited as part of the package.
- **Capability parameters in extern signatures are tokens.** The glue code does not pass them to JS. They make the authority visible to Kalam callers: a caller cannot invoke `create_intention` without holding a `Net`. Static analysis cannot prove that the JS respects this, so it is an audited claim. That is why the manifest's `[capabilities]` list and the runtime permission flags back it (§15.5).
- **`Js.Promise[T]` return type.** The call is a suspension point, and the Kalam-visible result type is `T`. No `await` is written (§14.2).
- **Thrown JS exceptions.** If the declared result type (after unwrapping a Promise) is `Result[T, JsError]`, a throw or a rejection becomes `err(JsError)`. Otherwise it becomes a **panic** at the call site. An exception never propagates as an exception through Kalam frames.
- `Secret[T]` parameters pass the raw value. The declaration is the explicit, counted exposure.
- Extern declarations are only allowed in a package whose manifest has an `[ffi]` section. Every `extern` block counts in `kalam audit`.

### 18.3 Boundary conversion and validation

| Kalam type | To JS | From JS (validated; failure → panic or `err(JsError.Shape)`) |
|---|---|---|
| `Int` | `number` if safe, else `bigint` | `number` that passes `Number.isSafeInteger`, or `bigint` within i64. Anything else fails. **No `parseInt`, no truncation** |
| `Float` | `number` | `typeof number` |
| `Decimal` | `string` (canonical decimal text) | `string` that parses as a Decimal |
| `Bool` | `boolean` | `typeof boolean` (no truthiness) |
| `String` | `string` | `typeof string` **and well-formed UTF-16** (a lone surrogate fails) |
| `Bytes` | fresh `Uint8Array` copy | `Uint8Array`/`ArrayBuffer`, copied |
| `T?` | `undefined` / value | `undefined` **or `null`** → `none` |
| `List[T]` | fresh frozen `Array` | `Array.isArray`, each element validated, copied |
| `Map[K, V]` | fresh `Map` | `Map` instance, entries validated, copied |
| record | fresh plain object, with a `null` prototype | object. Each declared field is read with `Object.getOwnPropertyDescriptor` and **must be a data property**: getters are never invoked. Extra properties are ignored (outgoing) or rejected (when `@js(strict)`). The prototype chain is never consulted |
| enum | `{ tag: "Member", team: "…" }` | same shape, tag checked against the variant list |
| `fn(A) -> B` | JS function wrapper. Its arguments are validated on the way in | JS function wrapped as a Kalam function. Its results are validated, and it is treated as *may suspend* if declared to return `Js.Promise` |
| `JsValue` | as is | **opaque, unvalidated**. It can only be inspected through `js.decode[T](v) -> Result[T, JsError]` or passed back to JS. `JsValue.unsafe_cast[T]` exists and is `unsafe` |
| capabilities | **cannot cross** (`K0802`), except as erased tokens (§18.2) | — |

Copying at the boundary means JS can never mutate a Kalam value, so Kalam's immutability (§7.10) holds even with buggy or hostile npm code. The cost of the copy is the price of soundness. `@js(borrow)` on a `Bytes` parameter skips it and counts as `unsafe`.

### 18.4 Calling Kalam from JS

- A `pub` function compiles to an ES module export, with a generated `.d.ts`.
- Suspending functions return `Promise<T>`.
- `Result[T, E]` becomes `{ ok: true, value } | { ok: false, error }`.
- `T?` becomes `T | undefined`.
- `Int` becomes `number | bigint` (documented). The generated wrapper validates JS-side arguments with the same table, so JS callers cannot break Kalam's invariants either.
- A panic surfaces to JS as a thrown `KalamPanic`, with no stack trace in production builds.

### 18.5 Generating bindings from `.d.ts` (planned, v0.2)

`kalam bind npm:stripe` will read the package's TypeScript declarations and emit `extern` blocks into a `bindings/` module. Generated externs count exactly like hand-written ones. TypeScript's `any` maps to `JsValue`, never to a Kalam type.

---

## 19. Lowering to JavaScript (summary)

### 19.1 General guarantees of generated code (P14)

- ES2022 modules, strict mode. **No `eval`, `new Function`, `with`, string `setTimeout`, or `import()` of computed strings** anywhere in generated code or in the runtime. A strict CSP without `'unsafe-eval'` must work.
- **Never** emitted: `==`/`!=`, `for…in`, `in` on user-keyed objects, `obj[userKey]` on plain objects, or `Object.assign` from user data. Kalam maps are JS `Map` or the runtime `HashMap`.
- Source maps (v3) for every output file. Browser bundles get SRI hashes on emitted `<script>` tags. Nothing is fetched from a CDN at run time by default.

### 19.2 `Int`: one semantics, a dual representation

- **Canonical representation invariant.** An `Int` value is a JS `number` iff its value is within ±(2⁵³−1). Otherwise it is a `bigint` within the i64 range. Every value obeys this, so `===` is correct equality and `Map` keys are canonical.
- Arithmetic is emitted as calls to runtime helpers (`__k.add`, `__k.mul`, …). The fast path is a number op plus `Number.isSafeInteger` on the result. The slow path is `bigint` with an explicit i64 range check, followed by normalization back to `number` when the value fits. **Overflow of i64 panics**, exactly as on the C backend.
- **Optimization.** When range analysis proves that both operands and the result fit in 2⁵³ (for example a loop counter bounded by a list length, or a value from `.at()`), the compiler emits a plain `+`. The optimizer may only *remove* checks it can prove redundant. It never changes semantics.
- `Int.to_string()` → decimal ASCII digits on both representations.

### 19.3 Data representation

| Kalam | JS |
|---|---|
| `Float` | `number` |
| `Decimal` | runtime `Dec128` class (no `number` anywhere in its arithmetic) |
| `String` | JS `string`, always well-formed (guaranteed by lexer, decoders and boundary validation). Grapheme APIs use `Intl.Segmenter` |
| `()` | `undefined` |
| record | an instance of a generated class with fixed fields. Fields are not frozen at run time (the compiler forbids mutation; boundary copies protect against JS) |
| enum | a generated class per variant, or a small integer for payload-free enums |
| `T?` | `none` → `undefined`. `some(v)` → `v`, **unless** `v` is itself `undefined` or a `Some` box, in which case it becomes `new __k.Some(v)`. This box is applied by a runtime check, so generic code and nested `T??` are correct without monomorphization |
| `Result` | `__k.Ok` / `__k.Err` instances |
| `List[T]` | a persistent vector (runtime). Small lists are frozen arrays |
| `Map[K, V]` | `String`/`Int`/`Bool` keys → JS `Map` (V8 seeds its string hash per process). Structural keys → runtime `HashMap` with keyed SipHash |
| `==` on structured types | generated `eq` functions (field by field), never reference equality |

### 19.4 `Html` on the client

The runtime installs one Trusted Types policy named `kalam`. The server's default CSP includes `require-trusted-types-for 'script'` and `trusted-types kalam`. Only `Html` values that came from server responses or from `html"…"` literals compiled into client islands can pass through the policy. A `String` can never reach `innerHTML`.

### 19.5 Panics and stack exhaustion

A panic is thrown as an internal `KalamPanic` object, which only runtime boundaries catch (§13.4). A JS `RangeError` from stack exhaustion is converted to a panic at the nearest boundary. Generated code never contains `try`/`catch` except at boundaries and in extern glue.

---

## 20. Deliberately excluded

| Excluded | Why (charter rule / principle) | What to use instead |
|---|---|---|
| `eval`, string-to-code, `new Function` | L9, CWE-94/95 | A sandboxed Wasm scripting VM (S15, a post-1.0 library) |
| Dynamic `import` / `require` of computed strings | L9, CWE-98 | Static `use` |
| Reflection, call-by-name, `getattr`-style access | L9 | `enum` + `match`, or a record of functions |
| `null` / `undefined` as values | L5 | `T?` |
| Exceptions: `try`/`catch`/`throw`/`finally` | L6, D16 | `Result` + `?`, and panic for bugs |
| Implicit conversions, truthiness, loose `==` | L3, L4 | named conversions, `Bool`-only conditions |
| Subtyping, class inheritance, `any`, unchecked casts | §8 soundness | traits, enums, composition, `JsValue` + decode |
| Global mutable state (module-level `var`) | L10, §12.3 | `Cell`/actors passed from `main` |
| Ambient I/O (except `print`, see X9) | L12 | capabilities |
| Shell-string process APIs (`system(s)`) | S2 | `shell"…"` argv |
| Native polymorphic deserialization, user decode hooks | S4, CWE-502 | derived `Decode`, schema-first |
| Raw bidi/invisible characters in source; mixed-script identifiers; mixed skins in a file | U1–U6 | escapes, `fmt` |
| Block comments | §4.3 (review integrity) | `#` line comments |
| Index syntax `a[i]` | §6.3 (grammar ambiguity, implicit panics) | `.at(i)`, `.get(i)` |
| Shadowing | §9.4 | new names |
| Operator overloading, custom operators | P7, readability across skins | methods |
| Macros, compile-time code execution beyond `const fn` | P10, supply-chain (P1: no build-time code) | `Template`, generics |
| `async`/`await` keywords, Promises as values | D12 | inferred suspension (§14) |
| Fire-and-forget tasks | §14.5 (structured concurrency) | `tasks.group`, durable `jobs` |
| Default parameters, varargs, labelled `break` | P7 (one way), deferred | named arguments, lists |
| Trait objects / dynamic dispatch | §7.8 (deferred) | enums, records of functions |
| Shared-memory threads | L11, research 03 | tasks + immutable data; threads later without type-system changes |
| Install scripts, build scripts running arbitrary code | P1 | sandboxed codegen (later) |
| English keywords in Arabic or Franco files | D6, X1 | `fmt --skin` |
| Unbounded fuzzy matching of Franco spellings | §5.4 | closed alias table |

---

## 21. Example programs (8 programs × 3 skins)

Each program is written once in English (`.kl`). The Arabic (`.kla`) and Franco (`.klf`) versions are `kalam fmt --skin=ar|fr` output with a project glossary (§5.7). All three have the same KCF hash, and the same output byte for byte (parity P1–P5). Strings and comments are identical in every skin by design (§5.6).

### 21.1 Hello

```kalam
# hello.kl
fn main():
    print("Hello, world!")
```

```kalam
# hello.kla
دالة الرئيسية():
    اطبع("Hello, world!")
```

```kalam
# hello.klf
dala asasy():
    ektb("Hello, world!")
```

### 21.2 FizzBuzz

```kalam
# fizzbuzz.kl
fn label(n: Int) -> String:
    if n % 15 == 0:
        return "FizzBuzz"
    else if n % 3 == 0:
        return "Fizz"
    else if n % 5 == 0:
        return "Buzz"
    "{n}"

fn main():
    for i in 1..=100:
        print(label(i))
```

```kalam
# fizzbuzz.kla
دالة وصف(رقم: Int) -> String:
    إذا رقم % 15 == 0:
        ارجع "FizzBuzz"
    وإلا إذا رقم % 3 == 0:
        ارجع "Fizz"
    وإلا إذا رقم % 5 == 0:
        ارجع "Buzz"
    "{رقم}"

دالة الرئيسية():
    لكل عدد في 1..=100:
        اطبع(وصف(عدد))
```

The same Arabic file with `[format] arabic_digits = true` (D7 point 4). The AST and KCF are unchanged:

```kalam
    إذا رقم % ١٥ == ٠:
    ...
    لكل عدد في ١..=١٠٠:
```

```kalam
# fizzbuzz.klf
dala wasf(rakam: Int) -> String:
    law rakam % 15 == 0:
        rag3 "FizzBuzz"
    walla law rakam % 3 == 0:
        rag3 "Fizz"
    walla law rakam % 5 == 0:
        rag3 "Buzz"
    "{rakam}"

dala asasy():
    lekol adad fi 1..=100:
        ektb(wasf(adad))
```

(The glossary maps `i` → `adad`, not `3adad`, because identifiers may not start with a digit, §4.5 rule 2.)

### 21.3 Record + match

```kalam
# roles.kl
enum Role:
    Admin
    Member(team: String)
    Guest

type User:
    name: String
    role: Role

fn greeting(user: User) -> String:
    match user.role:
        .Admin => "Welcome back, {user.name}. You have full access."
        .Member(team) => "Hi {user.name}, you are in {team}."
        .Guest => "Hello, guest."

fn main():
    let users = [
        User(name: "Mona", role: .Admin),
        User(name: "Karim", role: .Member(team: "Payments")),
        User(name: "Sara", role: .Guest),
    ]
    for user in users:
        print(greeting(user))
```

```kalam
# roles.kla
تعداد صلاحية:
    مدير
    عضو(فريق: String)
    ضيف

هيكل مستخدم:
    اسم: String
    دور: صلاحية

دالة تحية(فرد: مستخدم) -> String:
    حسب فرد.دور:
        .مدير => "Welcome back, {فرد.اسم}. You have full access."
        .عضو(فريق) => "Hi {فرد.اسم}, you are in {فريق}."
        .ضيف => "Hello, guest."

دالة الرئيسية():
    ليكن الأفراد = [
        مستخدم(اسم: "Mona"، دور: .مدير)،
        مستخدم(اسم: "Karim"، دور: .عضو(فريق: "Payments"))،
        مستخدم(اسم: "Sara"، دور: .ضيف)،
    ]
    لكل فرد في الأفراد:
        اطبع(تحية(فرد))
```

```kalam
# roles.klf
ta3dad salahiya:
    modeer
    mosharek(fare2: String)
    deif

haykal mostakhdem:
    esm: String
    dor: salahiya

dala ta7eya(fard: mostakhdem) -> String:
    hasab fard.dor:
        .modeer => "Welcome back, {fard.esm}. You have full access."
        .mosharek(fare2) => "Hi {fard.esm}, you are in {fare2}."
        .deif => "Hello, guest."

dala asasy():
    khali afrad = [
        mostakhdem(esm: "Mona", dor: .modeer),
        mostakhdem(esm: "Karim", dor: .mosharek(fare2: "Payments")),
        mostakhdem(esm: "Sara", dor: .deif),
    ]
    lekol fard fi afrad:
        ektb(ta7eya(fard))
```

Correctness notes. Adding a variant `Auditor` to `Role` makes `greeting` fail to compile (`K0401: missing case .Auditor`) instead of falling through. Writing `Admin =>` without the dot binds a fresh variable that matches everything, so the following arms become unreachable and the compiler reports `K0402`.

### 21.4 Result + `?`

```kalam
# ages.kl
enum AgeError:
    Empty
    NotANumber(text: String)
    OutOfRange(value: Int)

fn parse_age(text: String) -> Result[Int, AgeError]:
    if text.is_empty():
        return err(.Empty)
    let some(age) = Int.parse(text) else:
        return err(.NotANumber(text: text))
    if age < 0 or age > 150:
        return err(.OutOfRange(value: age))
    ok(age)

fn total_age(a: String, b: String) -> Result[Int, AgeError]:
    let x = parse_age(a)?
    let y = parse_age(b)?
    ok(x + y)

fn main():
    match total_age("31", "abc"):
        ok(sum) => print("Total: {sum}")
        err(.NotANumber(text)) => print("Not a number: {text}")
        err(problem) => print("Invalid age: {problem}")
```

```kalam
# ages.kla
تعداد مشكلة_العمر:
    فارغ
    غير_رقمي(نص: String)
    خارج_النطاق(قيمة: Int)

دالة حلل_العمر(نص: String) -> Result[Int، مشكلة_العمر]:
    إذا نص.is_empty():
        ارجع فشل(.فارغ)
    ليكن موجود(عمر) = Int.parse(نص) وإلا:
        ارجع فشل(.غير_رقمي(نص: نص))
    إذا عمر < 0 أو عمر > 150:
        ارجع فشل(.خارج_النطاق(قيمة: عمر))
    نجاح(عمر)

دالة مجموع_الأعمار(الأول: String، الثاني: String) -> Result[Int، مشكلة_العمر]:
    ليكن س = حلل_العمر(الأول)؟
    ليكن ص = حلل_العمر(الثاني)؟
    نجاح(س + ص)

دالة الرئيسية():
    حسب مجموع_الأعمار("31"، "abc"):
        نجاح(المجموع) => اطبع("Total: {المجموع}")
        فشل(.غير_رقمي(نص)) => اطبع("Not a number: {نص}")
        فشل(مشكلة) => اطبع("Invalid age: {مشكلة}")
```

```kalam
# ages.klf
ta3dad ghalta_omr:
    fady
    ghair_raqmy(nass: String)
    barra_elnetaq(qeema: Int)

dala hallel_omr(nass: String) -> Result[Int, ghalta_omr]:
    law nass.is_empty():
        rag3 fashal(.fady)
    khali feeh(omr) = Int.parse(nass) walla:
        rag3 fashal(.ghair_raqmy(nass: nass))
    law omr < 0 aw omr > 150:
        rag3 fashal(.barra_elnetaq(qeema: omr))
    tamam(omr)

dala magmoo3_omr(awwel: String, tany: String) -> Result[Int, ghalta_omr]:
    khali s = hallel_omr(awwel)?
    khali sad = hallel_omr(tany)?
    tamam(s + sad)

dala asasy():
    hasab magmoo3_omr("31", "abc"):
        tamam(magmoo3) => ektb("Total: {magmoo3}")
        fashal(.ghair_raqmy(nass)) => ektb("Not a number: {nass}")
        fashal(moshkela) => ektb("Invalid age: {moshkela}")
```

Correctness notes. `x + y` panics instead of wrapping if it overflows `Int`. Writing `parse_age(a)` as a bare statement is `K0350`, because a `Result` cannot be ignored.

### 21.5 HTTP handler

```kalam
# server.kl
use std.http.{Route, Access, Request, Response, HttpError, serve}

type Greeting:
    name: String

type Reply:
    message: String
    length: Int

fn greet(req: Request) -> Result[Response, HttpError]:
    let input: Greeting = req.json()?    # schema-first: unknown fields → 400
    let name = input.name.trim()
    if name.is_empty():
        return ok(Response.bad_request("name is required"))
    ok(Response.json(Reply(message: "Hello, {name}", length: name.grapheme_count())))

fn health(_req: Request) -> Result[Response, HttpError]:
    ok(Response.text("ok"))

fn main(world: World) -> Result[(), HttpError]:
    let routes = [
        Route.get("/health", Access.public, health),
        Route.post("/greet", Access.public, greet),
    ]
    serve(world.net, port: 8080, routes: routes)
```

```kalam
# server.kla
استخدم std.http.{Route، Access، Request، Response، HttpError، serve}

هيكل طلب_تحية:
    اسم: String

هيكل رد:
    رسالة: String
    الطول: Int

دالة رحب(طلب: Request) -> Result[Response، HttpError]:
    ليكن المدخل: طلب_تحية = طلب.json()؟    # schema-first: unknown fields → 400
    ليكن اسم = المدخل.اسم.trim()
    إذا اسم.is_empty():
        ارجع نجاح(Response.bad_request("name is required"))
    نجاح(Response.json(رد(رسالة: "Hello, {اسم}"، الطول: اسم.grapheme_count())))

دالة الصحة(_طلب: Request) -> Result[Response، HttpError]:
    نجاح(Response.text("ok"))

دالة الرئيسية(العالم: World) -> Result[()، HttpError]:
    ليكن المسارات = [
        Route.get("/health"، Access.public، الصحة)،
        Route.post("/greet"، Access.public، رحب)،
    ]
    serve(العالم.net، port: 8080، routes: المسارات)
```

```kalam
# server.klf
hat std.http.{Route, Access, Request, Response, HttpError, serve}

haykal talab_ta7eya:
    esm: String

haykal radd:
    resala: String
    tool: Int

dala ra7ab(talab: Request) -> Result[Response, HttpError]:
    khali madkhal: talab_ta7eya = talab.json()?    # schema-first: unknown fields → 400
    khali esm = madkhal.esm.trim()
    law esm.is_empty():
        rag3 tamam(Response.bad_request("name is required"))
    tamam(Response.json(radd(resala: "Hello, {esm}", tool: esm.grapheme_count())))

dala se7a(_talab: Request) -> Result[Response, HttpError]:
    tamam(Response.text("ok"))

dala asasy(el3alam: World) -> Result[(), HttpError]:
    khali masarat = [
        Route.get("/health", Access.public, se7a),
        Route.post("/greet", Access.public, ra7ab),
    ]
    serve(el3alam.net, port: 8080, routes: masarat)
```

Security notes:

- Each route must name its `Access`. Omitting it does not compile (S9).
- `req.json()` rejects unknown fields, duplicate keys and oversized bodies (§17.3). The JSON keys are the canonical names (`name`) in every skin (§5.7).
- The handlers suspend while reading the body, but no `async` appears in any skin (§14). The server sends CSRF, cookie and security-header defaults (S8).
- A panic inside `greet` produces a 500 with a request ID and no trace (§13.4).

### 21.6 SQL query

```kalam
# notes_db.kl
use std.db.{Db, DbError}
use std.sql

type Note:
    id: Int
    title: String

fn search_notes(db: Db, owner: Int, term: String) -> Result[List[Note], DbError]:
    let pattern = sql.like_contains(term)    # escapes % and _ in user input
    let query = sql"""
        SELECT id, title FROM notes
        WHERE owner_id = {owner} AND title ILIKE {pattern}
        ORDER BY id DESC
        LIMIT 50
        """
    db.query(query)

fn main(world: World) -> Result[(), DbError]:
    let db = Db.connect_env(world.net, world.env, "DATABASE_URL")?
    let notes = search_notes(db, owner: 7, term: "invoice")?
    for note in notes:
        print("{note.id}: {note.title}")
    ok(())
```

```kalam
# notes_db.kla
استخدم std.db.{Db، DbError}
استخدم std.sql

هيكل ملاحظة:
    المعرف: Int
    العنوان: String

دالة ابحث_الملاحظات(القاعدة: Db، المالك: Int، الكلمة: String) -> Result[List[ملاحظة]، DbError]:
    ليكن النمط = sql.like_contains(الكلمة)    # escapes % and _ in user input
    ليكن الاستعلام = sql"""
        SELECT id, title FROM notes
        WHERE owner_id = {المالك} AND title ILIKE {النمط}
        ORDER BY id DESC
        LIMIT 50
        """
    القاعدة.query(الاستعلام)

دالة الرئيسية(العالم: World) -> Result[()، DbError]:
    ليكن القاعدة = Db.connect_env(العالم.net، العالم.env، "DATABASE_URL")؟
    ليكن الملاحظات = ابحث_الملاحظات(القاعدة، المالك: 7، الكلمة: "invoice")؟
    لكل بند في الملاحظات:
        اطبع("{بند.المعرف}: {بند.العنوان}")
    نجاح(())
```

```kalam
# notes_db.klf
hat std.db.{Db, DbError}
hat std.sql

haykal molahza:
    mo3arraf: Int
    onwan: String

dala dawwar_molahzat(qa3eda: Db, malek: Int, kelma: String) -> Result[List[molahza], DbError]:
    khali namat = sql.like_contains(kelma)    # escapes % and _ in user input
    khali estelam = sql"""
        SELECT id, title FROM notes
        WHERE owner_id = {malek} AND title ILIKE {namat}
        ORDER BY id DESC
        LIMIT 50
        """
    qa3eda.query(estelam)

dala asasy(el3alam: World) -> Result[(), DbError]:
    khali qa3eda = Db.connect_env(el3alam.net, el3alam.env, "DATABASE_URL")?
    khali molahzat = dawwar_molahzat(qa3eda, malek: 7, kelma: "invoice")?
    lekol band fi molahzat:
        ektb("{band.mo3arraf}: {band.onwan}")
    tamam(())
```

The SQL text is a string, so it is identical in all skins, while the holes use each skin's display names. Rows decode by canonical field names (`id`, `title`) in every skin (§5.7 rule 1).

**What does not compile** (the negative twin of this example, research 05 NEG task 48):

```kalam
fn search_bad(db: Db, term: String) -> Result[List[Note], DbError]:
    let query = "SELECT id, title FROM notes WHERE title = '" + term + "'"
    db.query(query)
```

```
error[SEC0600]: `db.query` needs an `Sql` value, but `query` is a `String`
  --> notes_db.kl:3:14
   |
 2 |     let query = "SELECT id, title FROM notes WHERE title = '" + term + "'"
   |                 ------------------------------------------------------- this is a plain String built at runtime
 3 |     db.query(query)
   |              ^^^^^ expected `Sql`
   |
 = why: text built from runtime values could change the meaning of the query (SQL injection)
 = fix: write the query as a tagged literal; values in {…} are sent as parameters:
        db.query(sql"SELECT id, title FROM notes WHERE title = {term}")
```

The same diagnostic is rendered in Arabic for `.kla` files and in Franco for `.klf` files, with the same code `SEC0600` (P6).

### 21.7 HTML view

```kalam
# views.kl
use std.html.{Html}
use std.url.{Url}

type Note:
    title: String
    body: String
    link: Url

fn note_item(note: Note) -> Html:
    html"""
    <li>
      <a href="{note.link}">{note.title}</a>
      <p>{note.body}</p>
    </li>
    """

fn notes_page(owner: String, notes: List[Note]) -> Html:
    html"""
    <section>
      <h1>Notes for {owner}</h1>
      <ul>{notes.map(note_item)}</ul>
    </section>
    """
```

```kalam
# views.kla
استخدم std.html.{Html}
استخدم std.url.{Url}

هيكل ملاحظة:
    العنوان: String
    المحتوى: String
    الرابط: Url

دالة عنصر_ملاحظة(بند: ملاحظة) -> Html:
    html"""
    <li>
      <a href="{بند.الرابط}">{بند.العنوان}</a>
      <p>{بند.المحتوى}</p>
    </li>
    """

دالة صفحة_الملاحظات(المالك: String، الملاحظات: List[ملاحظة]) -> Html:
    html"""
    <section>
      <h1>Notes for {المالك}</h1>
      <ul>{الملاحظات.map(عنصر_ملاحظة)}</ul>
    </section>
    """
```

```kalam
# views.klf
hat std.html.{Html}
hat std.url.{Url}

haykal molahza:
    onwan: String
    mo7tawa: String
    rabet: Url

dala onsor_molahza(band: molahza) -> Html:
    html"""
    <li>
      <a href="{band.rabet}">{band.onwan}</a>
      <p>{band.mo7tawa}</p>
    </li>
    """

dala safhet_molahzat(malek: String, molahzat: List[molahza]) -> Html:
    html"""
    <section>
      <h1>Notes for {malek}</h1>
      <ul>{molahzat.map(onsor_molahza)}</ul>
    </section>
    """
```

Security notes:

- A title of `<script>alert(1)</script>` renders as text.
- `href="{note.title}"` is `K0611`, because a URL attribute needs a `Url`, and a `Url` can never be `javascript:…`.
- `<div onclick="{x}">` is `K0612`.
- Writing `<ul>{notes}</ul>` (a `List[Note]`) only prints the escaped `Show` text of the list, never markup. Lint `W0617` flags a non-`Html` list in a child-element position.
- The literal is parsed at compile time, so a missing `</li>` is a compile error.

### 21.8 Small game loop (browser canvas, JS target)

```kalam
# bounce.kl
use std.game.{Screen, Key, Color}

type Ball:
    x: Float
    y: Float
    dx: Float
    dy: Float

fn step(ball: Ball, dt: Float, width: Float, height: Float) -> Ball:
    var x = ball.x + ball.dx * dt
    var y = ball.y + ball.dy * dt
    var dx = ball.dx
    var dy = ball.dy
    if x < 0.0 or x > width:
        dx = -dx
        x = x.clamp(0.0, width)
    if y < 0.0 or y > height:
        dy = -dy
        y = y.clamp(0.0, height)
    Ball(x: x, y: y, dx: dx, dy: dy)

fn main(world: World):
    let screen = world.screen.open(width: 640, height: 480)
    let start = Ball(x: 320.0, y: 240.0, dx: 180.0, dy: 120.0)
    var ball = start
    while screen.is_open():
        let dt = screen.next_frame()    # suspends until the next animation frame
        if screen.pressed(Key.Space):
            ball = start
        ball = step(ball, dt, width: 640.0, height: 480.0)
        screen.clear(Color.black)
        screen.circle(ball.x, ball.y, radius: 10.0, color: Color.white)
```

```kalam
# bounce.kla
استخدم std.game.{Screen، Key، Color}

هيكل كرة:
    س: Float
    ص: Float
    سرعة_س: Float
    سرعة_ص: Float

دالة خطوة(الكرة: كرة، الزمن: Float، العرض: Float، الارتفاع: Float) -> كرة:
    متغير س = الكرة.س + الكرة.سرعة_س * الزمن
    متغير ص = الكرة.ص + الكرة.سرعة_ص * الزمن
    متغير سرعة_س = الكرة.سرعة_س
    متغير سرعة_ص = الكرة.سرعة_ص
    إذا س < 0.0 أو س > العرض:
        سرعة_س = -سرعة_س
        س = س.clamp(0.0، العرض)
    إذا ص < 0.0 أو ص > الارتفاع:
        سرعة_ص = -سرعة_ص
        ص = ص.clamp(0.0، الارتفاع)
    كرة(س: س، ص: ص، سرعة_س: سرعة_س، سرعة_ص: سرعة_ص)

دالة الرئيسية(العالم: World):
    ليكن الشاشة = العالم.screen.open(width: 640، height: 480)
    ليكن البداية = كرة(س: 320.0، ص: 240.0، سرعة_س: 180.0، سرعة_ص: 120.0)
    متغير الكرة = البداية
    طالما الشاشة.is_open():
        ليكن الزمن = الشاشة.next_frame()    # suspends until the next animation frame
        إذا الشاشة.pressed(Key.Space):
            الكرة = البداية
        الكرة = خطوة(الكرة، الزمن، العرض: 640.0، الارتفاع: 480.0)
        الشاشة.clear(Color.black)
        الشاشة.circle(الكرة.س، الكرة.ص، radius: 10.0، color: Color.white)
```

```kalam
# bounce.klf
hat std.game.{Screen, Key, Color}

haykal kora:
    x: Float
    y: Float
    dx: Float
    dy: Float

dala khatwa(el_kora: kora, zaman: Float, ard: Float, ertefa3: Float) -> kora:
    motaghayer x = el_kora.x + el_kora.dx * zaman
    motaghayer y = el_kora.y + el_kora.dy * zaman
    motaghayer dx = el_kora.dx
    motaghayer dy = el_kora.dy
    law x < 0.0 aw x > ard:
        dx = -dx
        x = x.clamp(0.0, ard)
    law y < 0.0 aw y > ertefa3:
        dy = -dy
        y = y.clamp(0.0, ertefa3)
    kora(x: x, y: y, dx: dx, dy: dy)

dala asasy(el3alam: World):
    khali shasha = el3alam.screen.open(width: 640, height: 480)
    khali bedaya = kora(x: 320.0, y: 240.0, dx: 180.0, dy: 120.0)
    motaghayer el_kora = bedaya
    tool_ma shasha.is_open():
        khali zaman = shasha.next_frame()    # suspends until the next animation frame
        law shasha.pressed(Key.Space):
            el_kora = bedaya
        el_kora = khatwa(el_kora, zaman, ard: 640.0, ertefa3: 480.0)
        shasha.clear(Color.black)
        shasha.circle(el_kora.x, el_kora.y, radius: 10.0, color: Color.white)
```

Concurrency notes:

- `screen.next_frame()` is a suspension point, which the LSP shows as `⏸`. `main` is inferred to suspend and is emitted as an `async function`, with `await new Promise(requestAnimationFrame)` in the loop (§14.7).
- The browser stays responsive with no `async` keyword written in any skin.
- `ball` is a `var`, so no closure can capture it. The frame loop cannot race with itself.
- Randomness in a game uses `FastRng.seeded(n)`, which needs no capability (§17.2).

---

## 22. Conformance tests this spec implies

These belong in CI from the first compiler commit (research 05: D1, D2 and the parity gate). Each item is a test directory in `conformance/`.

| Area | Tests |
|---|---|
| Trojan Source | Each code point class in §4.4, placed in code, comments, strings and identifiers: rejected with the right code. The escaped form compiles. Legitimate Arabic text in strings and comments compiles. |
| Identifiers | Mixed-script rejection; skeleton collisions (`على`/`علي`, `مرحلة٢`/`مرحلة2`, Latin `o` vs Arabic-Indic `٠`); tatweel/harakat/presentation-form rejection with fix-it; Persian letter rejection; digit-leading rejection in every skin. |
| Numerals | Mixed digit systems, Persian digits, `٫` with Western digits, `،` as a decimal, leading zeros, `0x1Fg`, `1e5x`, `7f`: all rejected with the right codes. Arabic-Indic literals accepted only in `.kla`. |
| Franco lexing | The proof obligations of §5.4 (i)–(iii) as generated tests over the alias table. Every alias formats to canonical. `--strict` rejects aliases. |
| Skins | P1–P5 round-trips for every corpus program, including `r#` escapes and glossaries. English keywords in `.kla` give `K0110`. |
| Types | No implicit conversions (a matrix of every primitive pair under `==`, `+`, call argument). `Float` is not `Eq`/`Hash`. `Secret` is not `Encode`/`Eq`. Capabilities have no `Decode`. |
| Integers | Overflow traps at exactly ±2⁶³ on JS and in the reference interpreter. The number↔bigint boundary at 2⁵³ is exercised by property tests with **differential execution** against the interpreter. |
| Patterns | Exhaustiveness, unreachable arms, bare identifier as binding, `.Variant` resolution. |
| Must-use | Discarded `Result`/value → `K0350`. `_ =` is accepted and counted. |
| Concurrency | Suspension inference over the call graph, higher-order specialization, stored-function conservatism, cancellation only at suspension points, `Cell.update` rejects suspending closures. |
| Sinks | Research 05 NEG tasks 48–50 in all three skins, plus each `K06xx` rule in §16 with a positive twin. The exploit-payload harness runs against the "good" twins. |
| JS boundary | Each row of §18.3 with hostile JS: getters, proxies, `__proto__` keys, lone surrogates, `NaN` as an `Int`, `null` in a non-optional field. Each yields a validation failure, never a Kalam-visible wrong value. |
| Generated JS | Static scan of all emitted JS: no `eval`, `Function(`, `==`, `for…in`. The test pages run under a strict CSP with Trusted Types enforced. |

---

## 23. Open questions

1. **Arabic keyword choices need a native-speaker survey** (research 05 §1.1 method). In particular: `هيكل` vs `نوع` for `type`, `حقق` for `impl`, `انهيار` for `panic`, `موجود` for `some`, and whether `و` as `and` reads well next to identifiers that start with `و`.
2. **Franco canonical spellings and the alias table** need a survey of 200+ Egyptian developers (report 02 §9). In particular `motaghayer` (long) for `var`, and `tabba2` for `impl`. Should v0.1 drop digit-leading aliases entirely for an even smaller lexing surface?
3. **Arabic confusable table** (report 04 §7.3): this draft rejects Persian/Urdu letters in identifiers. That excludes Persian-speaking developers. A linguist review should decide whether to fold them instead, and should finalize the `arabic_fold` list.
4. **Library and type names in Arabic** (`Int`, `String`, `.map`). This draft keeps them ASCII, which forces keyboard switching in the Arabic skin. Alias packs (report 02 §7.3) would fix that, but add a second translation layer to secure. Glossaries (§5.7) could cover user code only.
5. **`print` as an ambient effect** (X9, §15.6). Is the deviation from L12 acceptable, or should hello-world take `world` too?
6. **`Int` on JS: performance cost** of the checked dual representation (§19.2). This must be benchmarked before it is locked. The alternative is a 53-bit `Int` on every backend, which is uniform but unusual.
7. **No index syntax** (`xs.at(i)`) is safer and makes the grammar unambiguous, but professional developers may resist it. Measure it in the LLM-writability harness (research 05 D3).
8. **No shadowing** and **no `var` capture** are strict. Measure friction on the corpus, and keep them only if the cost is low.
9. **Effect specialization cap** (§14.3: 4 function-typed parameters). Is whole-program inference for stored functions acceptable for incremental compilation, or do record fields need an explicit marker?
10. **Trait objects / dynamic dispatch**: postponed. The server-driven UI component model may need them, or a record-of-functions pattern may be enough.
11. **Mechanized soundness proof** of Kalam Core (§8) in Lean or Coq before 1.0. Who owns it, and what is the budget?
12. **Canonical names under glossaries** (§5.7): should a project be allowed to make Arabic its canonical naming? That makes JSON keys and DB columns Arabic. This is fine for some teams and a problem for API consumers.
13. **Capability granularity** (report 04 §7.2): this draft uses values plus manifest-declared extern capabilities. Should public functions also get an inferred, *displayed* capability summary in docs (`kalam caps`), and should adding a capability to a function count as a semver-major change?
14. **Mass assignment** (§17.3, `W0710`): should decoding request data into a type used for DB rows be an error instead of a lint?
