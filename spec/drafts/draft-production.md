# Kalam (كلام) v0.1: Draft Language Specification (Production & LLM-Writability Angle)

> **Status:** Draft for founder review. **Date:** 2026-10-24. **Angle:** what a professional needs to ship a SaaS, and a canonical form that an LLM can write reliably.
> **Binding inputs:** `DECISIONS.md` (D1–D17, locked) and research reports 01–06. Where this draft cites a report rule it uses the report's own ID, for example `04 L7` (security charter, rule L7) or `05 P3` (parity gate P3).
> **Normative words:** MUST, MUST NOT, SHOULD and MAY have their RFC 2119 meanings. Text marked *Rationale* is not normative.

---

## Table of contents

0. Reading guide and summary of distinctive choices
1. Design principles
2. Resolved conflicts between research inputs
3. Source files, skins and the lossless pipeline
4. Lexical structure
5. D7 resolution: Arabic RTL, digits and punctuation
6. Keyword and core-name tables (all three skins) and alias policy
7. Grammar (EBNF)
8. Type system
9. Expressions and statements
10. Pattern matching
11. Functions and closures
12. Modules, packages and visibility
13. Error handling: `Result`, `?`, `panic`
14. Concurrency without an `async` keyword
15. Capabilities and effects for I/O
16. Safe string kinds: `sql`, `html`, `sh` (and `url`, `re`)
17. Interop with JavaScript and npm
18. Standard library shape (v0.1)
19. Canonical form for LLMs, diagnostics and tooling contract
20. Lowering to JavaScript (reference semantics)
21. Deliberately excluded features
22. Security charter traceability (report 04, P0 rules)
23. Example programs in three skins (8 programs × 3 skins)
24. Open questions

---

## 0. Reading guide and summary of distinctive choices

Kalam is a small, statically typed, indentation-based language. It compiles to ES2022 JavaScript (D4), and its compiler, formatter, LSP and package manager ship as one Rust binary, `kalam` (D10). Every file is written in exactly one **skin**: English (`en`), Arabic (`ar`) or Egyptian Franco (`fr`). All three skins parse to one canonical AST (D6).

The choices that come from this draft's production and LLM angle:

1. **The canonical form is the English skin, formatted.** It is close to Python and TypeScript on purpose. It is the form LLMs write, the form packages are published in, and the form that is hashed and signed (it settles `04 §7 Q5`).
2. **`Int` is the 53-bit safe-integer range on every backend.** Overflow traps. JS gets plain numbers at zero cost, the later C backend reproduces the same range with checks, and `BigInt` and `Decimal` are separate types (§8.2).
3. **Suspension is inferred from the call graph.** There is no `async` keyword. It costs one bit in each function type: suspend-polymorphic higher-order functions are specialized, and `@sync` callbacks are rejected at compile time if they suspend (§14).
4. **Enum variants are written with a leading dot** (`.Circle(r)`), and types and values live in separate namespaces. Arabic has no letter case, so Kalam never uses capitalization to mean anything (§2.6).
5. **Mutation has value semantics.** `var p = point` followed by `p.x = 3` rebinds `p`. It never mutates shared data. Closures cannot capture a `var`. With immutable data and only atomic `Cell` updates, data races cannot happen in single-isolate code (§9.4, §14.6).
6. **Injection is a type error.** `sql"…"`, `html"…"` and `sh"…"` are language-level tagged literals with compile-time parsing. Their tags are ASCII in every skin (§16).
7. **Routes are deny-by-default through the type system.** `access:` is a required parameter, not a decorator (§18.2).
8. **There is a table of known LLM mistakes.** Each mistake (`def`, `elif`, `null`, `await`, braces, `===` and so on) gets a stable diagnostic code and a machine-applicable fix (§19.3).

---

## 1. Design principles

| # | Principle | Consequence in this spec |
|---|---|---|
| P1 | **One obvious way, enforced by the formatter.** | No synonyms in the English skin. The formatter output is the only accepted style (`kalam fmt --check` in CI). Two spellings of a construct exist only where they mean different things. |
| P2 | **Familiar to the strongest LLM priors (Python, TypeScript, Rust, Swift).** | Colon-and-indent blocks, `match`/`case` from Python, `Result`/`?` from Rust, `.variant` from Swift, `export`/`use` close to TypeScript and Rust. No new punctuation. |
| P3 | **Controlled English, not natural English (report 01).** | About 30 single-word keywords. Each one translates to one Arabic word and one Franco word. Math stays symbolic. |
| P4 | **Explicit at boundaries, inferred inside.** | Top-level function signatures are mandatory. Local types are inferred. |
| P5 | **Secure by default, with greppable escape hatches (04 §1.3).** | No null, no exceptions, no coercion, no eval, capability-passing I/O, typed sinks. Every escape hatch uses the ASCII word `unsafe`. |
| P6 | **Ship real products in year 1 (reports 03 and 06).** | JS first, typed `extern` interop with npm, and a batteries-included `std/web` that is server-driven (D11). |
| P7 | **Skins are a lexer and printer concern only (02 §7.1).** | One grammar and one AST. No per-skin word order. Round-trips are byte-exact (`05 P1–P3`). |
| P8 | **Semantics are target-neutral (03 §4).** | Every rule is stated without reference to JS. §20 gives the JS lowering as a *reference implementation* of those semantics. |
| P9 | **Errors are a feature (01 §3.4).** | Stable codes, fix-its, JSON output, and messages in all three skins. |

---

## 2. Resolved conflicts between research inputs

### 2.1 English keywords inside an `ar` or `fr` file
- **Conflict:** `02 §7.1` says English keywords should be accepted in every skin. `D6` (locked) and `04 U6` say one file = one skin.
- **Resolution:** D6 wins. An English keyword in an `ar` or `fr` file is error **K0201 "keyword from another skin"**. The error carries a machine-applicable fix that replaces it with the file's canonical keyword. `kalam fix` and the LSP apply it automatically. Copying an English snippet into an Arabic file therefore costs one keystroke, but files on disk always stay single-skin. The same rule covers Arabic or Franco keywords that appear in an `en` file.

### 2.2 LRM, RLM and ALM marks
- **Conflict:** `02 §7.5` allows LRM (U+200E), RLM (U+200F) and ALM (U+061C) as whitespace. `04 U1` rejects them.
- **Resolution:** `04 U1` wins. All bidi formatting characters are rejected everywhere in source, including comments. Display order is the editor's job (UTS #55 token isolation in the LSP and the VS Code extension), never something written into the file. Details are in §5.

### 2.3 Identifiers that start with a digit
- **Conflict:** `02 §7.6` bans them. `03 §4.1` and `04 U5` allow them in Franco under restrictions.
- **Resolution:** banned in **every** skin. Only a closed list of Franco *keyword aliases* may start with a digit (`5ali`, `7asab`, `2ktb`). Identifiers stay portable to every skin and to the JS and C targets, and the number lexer stays trivially decidable (§4.6).

### 2.4 `async`/`await`
- **Conflict:** `03 §4` recommends `async/await`. `D12` says there is no async keyword.
- **Resolution:** D12 wins. Suspension is an inferred effect bit (§14). `async` and `await` are not keywords. Writing them triggers K0907, which has a deletion fix.

### 2.5 Integer representation on JS
- **Conflict:** `04 §4.3` suggests BigInt or a checked wrapper. `03 §4` warns that JS semantics must not leak into the language.
- **Resolution:** the language defines `Int` as the range [−(2⁵³−1), 2⁵³−1], and leaving that range traps. Every backend implements exactly this, so it is not a JS artifact. `BigInt` (arbitrary precision) and `Decimal` (exact base 10) are separate opt-in types. See §8.2.

### 2.6 No letter case in Arabic
Kalam never gives capitalization any meaning. Instead:
- **Types and values have separate namespaces.** `shape: Shape` translates to `شكل: شكل` and is legal.
- **Enum variants are always written with a leading dot** (`.Circle`) or qualified (`Shape.Circle`). In patterns, a bare identifier is *always* a binding, except for the core constructors `Some`, `None`, `Ok` and `Err`, which are reserved words.
- **Record construction** `N(field: …)` resolves `N` in the type namespace. A local value binding may not share a name with a record type in scope (K0412). Without this rule, `let مستخدم = مستخدم(...)` would be ambiguous. The style guide recommends the definite article for values (`المستخدم`).

### 2.7 Canonical skin for signing, hashing and diffs
- **Open question in `04 §7 Q5`.**
- **Resolution:** the canonical serialization is the **English skin with project-independent formatter settings** (ASCII digits and ASCII punctuation). Registry packages are stored in this form (the Excel model, `02 §7.2`). Signatures and content hashes cover it.

---

## 3. Source files, skins and the lossless pipeline

### 3.1 Files and skin declaration
- A source file is UTF-8 without a BOM and in Unicode NFC (`04 U2`). A file with a BOM or in non-NFC form is rejected with K0101, and `kalam fmt` fixes it. Line endings are LF. CRLF is accepted and rewritten by `fmt`.
- **The extension declares the skin.** There is no in-file pragma, because there should be one way:

| Skin | Extension | Example |
|---|---|---|
| English | `.kl` | `users.kl` |
| Arabic (فصحى) | `.ar.kl` | `users.ar.kl` |
| Egyptian Franco | `.fr.kl` | `users.fr.kl` |

- The module name does not include the skin suffix. If `users.kl` and `users.ar.kl` exist in the same directory, that is error K0301.

### 3.2 Pipeline

```
bytes ─► skin lexer (keyword/alias/punctuation tables) ─► lossless CST (trivia, comments, spellings)
                                                         │
                         kalam fmt --skin=X ◄────────────┤ (printer: CST → text in any skin)
                                                         ▼
                                    canonical AST (keywords = concept IDs)
                                                         ▼
                           name resolution ► type/effect inference ► capability check
                                                         ▼
                                     typed IR ► JS emitter (+ source maps, .d.ts)
```

### 3.3 Conversion guarantees
- **G1, AST identity:** `ast(parse_X(s)) == ast(parse_Y(print_Y(parse_X(s))))` for all skins X and Y (`05 P1`, `05 P2`).
- **G2, byte fixpoint:** for a fixed project configuration, `print_X(parse_Y(print_Y(parse_X(s)))) == print_X(parse_X(s))` (`05 P3`). Every skin setting that changes output (Arabic digit system, Arabic punctuation) is a **project-level** setting in `kalam.toml`, never a per-file one, so G2 holds.
- **Code inside interpolation holes (`{expr}`) is code**, and it is converted like any other code. The literal text around the holes is not.
- **Never translated by conversion:** identifiers, string and tagged-literal text, comments, attribute names, module paths and stdlib API names.
- **Translated:** keywords, core names (§6.2), punctuation aliases (§5.4) and digit glyphs (§5.2).
- **Optional glossary:** the file `names.ar.toml` / `names.fr.toml` maps project identifiers between skins, for teaching and for Arabic-first teams. Glossary conversion is still deterministic. It is a *separate* mode (`kalam fmt --skin=ar --glossary`), and the AST hash it produces changes, because identifiers change.
- **Raw identifiers:** an identifier that is a reserved word in the target skin is printed as `` `law` `` (backticks). Backticks are valid in every skin and can quote any identifier. `fmt` removes them when they are not needed.

---

## 4. Lexical structure

### 4.1 Character classes

| Class | Definition |
|---|---|
| `LETTER_LATIN` | Unicode `XID_Start` ∩ Script=Latin |
| `LETTER_ARABIC` | Unicode `XID_Start` ∩ Script=Arabic, **excluding** U+0640 (tatweel), U+064B–U+065F and U+0670 (harakat), U+06D6–U+06ED (Quranic marks), U+06CC (Farsi yeh), U+06A9 (keheh) and presentation forms U+FB50–U+FDFF and U+FE70–U+FEFF |
| `ASCII_DIGIT` | `0`–`9` |
| `ARABIC_DIGIT` | `٠`–`٩` (U+0660–U+0669) |
| `PERSIAN_DIGIT` | `۰`–`۹` (U+06F0–U+06F9): **always rejected** (K0105, with a fix to ASCII or Arabic-Indic digits) |
| `WS` | U+0020 only. Tabs are rejected outside string literals (K0103). |
| `FORBIDDEN` | U+202A–U+202E, U+2066–U+2069, U+200E, U+200F, U+061C, U+200B–U+200D, U+2060–U+2064, U+FEFF, U+00AD, and all `Cc` controls except LF and CR. Rejected **anywhere** in the file, including strings and comments (K0104). In strings, write them as escapes (`"\u{200F}"`). |

### 4.2 Identifiers
```
IDENT        = ID_START { ID_CONTINUE } | "`" ID_START { ID_CONTINUE } "`" ;
ID_START     = LETTER_LATIN | LETTER_ARABIC | "_" ;
ID_CONTINUE  = ID_START | ASCII_DIGIT | XID_Continue-marks-of-same-script ;
```
- **Single-script rule (`04 U3`):** an identifier's letters are all Latin or all Arabic. `_` and ASCII digits are allowed with either script. `user_اسم` is K0106.
- **No digit start** in any skin (§2.3). **No Arabic-Indic digits** inside identifiers (K0107), because `١` is confusable with `ا`, `l` and `1`.
- **Case-sensitive** in Latin. No style-insensitivity (report 01, "features that hurt").
- **Arabic fold key** (`02 §7.4`, `04 U4`). Identifiers are *compared* by their fold key:
  1. NFKC.
  2. Fold أ إ آ ٱ → ا.
  3. Fold ى → ي.
  4. Fold ة → ه.
  5. Fold ؤ → و and ئ → ي.

  The source text is never rewritten except by `fmt`.
  - Two **distinct declarations** in the same scope, or two exports of one module, whose fold keys are equal: error K0108 ("ambiguous after normalization").
  - A **use** whose spelling differs from the declaration's but has the same fold key resolves to that declaration and gets warning W0109. `fmt` rewrites the use to the declaration's spelling.
- **Confusable skeletons (`04 U4`):** UTS #39 `skeleton()` plus Kalam's Arabic table. Two different identifiers in one scope with the same skeleton are error K0110.
- **Underscore:** `_` alone is the wildcard and cannot be read as a value. A leading `_x` silences unused-binding warnings.

### 4.3 Keywords and contextual words
- The reserved words of the **file's skin** are reserved. These are keywords (§6.1) plus core names (§6.2). Reserved words of the other skins are ordinary identifiers. The LSP shows a hint (H0111) when an identifier equals another skin's reserved word, and conversion escapes it automatically (§3.3).
- **Contextual freedom:** after `.` (field or method access), as a named-argument label, and as a record field name, any word is an identifier. `contract.law`, `x.type` and `f(match: 1)` are legal in every skin.
- The words `unsafe`, `sql`, `html`, `sh`, `url` and `re` (tags) and all `@attribute` names are **ASCII in every skin and never translated** (`04 L13`).

### 4.4 Comments
- `#` starts a line comment that runs to the end of the line.
- `##` starts a doc comment, which attaches to the following item. Doc comments are Markdown, and `kalam doc` renders them.
- There are no block comments.
- Comments are kept byte-for-byte in the CST and never translated.

### 4.5 Indentation and line structure
Kalam uses Python-style significant indentation, enforced by the formatter (D15).

1. **Indent unit:** exactly 4 spaces per level (K0120 otherwise). Tabs are K0103.
2. **Logical lines.** The lexer emits `NEWLINE` at the end of every non-blank, non-comment-only line, except in these cases:
   - **Bracket suppression:** inside `(`, `[` or `{`, newlines and indentation are ignored. The exception is block islands (rule 4).
   - **Leading-dot continuation:** a line whose first token is `.` *and* that is indented more than the logical line it continues is joined to that line. This is how method chains are written:
     ```
     let names = users
         .filter(fn(u) => u.active)
         .map(fn(u) => u.name)
     ```
     A line starting with `.` at the same or lower indentation is K0121.
   - There is no backslash continuation and no trailing-operator continuation. Use parentheses for long arithmetic.
3. **INDENT/DEDENT:** the lexer keeps an indentation stack. A line indented more than the top of the stack emits `INDENT`. Only one level at a time is allowed, else K0120. A line indented less emits one `DEDENT` per popped level, and it must land exactly on a stack entry (K0122). At EOF, all levels are popped.
4. **Block islands (multi-line lambdas).** Inside brackets, the sequence `fn ( … ) [-> T] :` followed by a newline opens an *island*:
   - The island's base column is the indentation of the line holding the `fn`.
   - Normal INDENT/DEDENT processing resumes for the following lines, which must be indented more than the base.
   - The island closes at the first line indented ≤ the base. That line must start with `)`, `]`, `}` or `,`.
   - Islands nest.

   Example:
   ```
   let result = db.transaction(fn(tx):
       tx.exec(sql"UPDATE accounts SET balance = balance - {amount} WHERE id = {from}")?
       tx.exec(sql"UPDATE accounts SET balance = balance + {amount} WHERE id = {to}")?
       return Ok(())
   )
   ```
5. **One-line blocks:** a block header may be followed by one single-line statement on the same line (an expression, `return`, `break`, `continue`, `pass`, `let`, `var` or an assignment, for example `if x < 0: return Err(.Negative)`) (`case .Circle(r): r * r`). `fmt` keeps it on one line only if the line fits in 100 columns.
6. **No semicolons.** `;` is K0904, with a fix that splits the line.

### 4.6 Numeric literals
```
INT_LIT    = DEC | HEX | BIN | OCT ;
DEC        = "0" | NZ_DIGIT { ["_"] ASCII_DIGIT }
           | NZ_AR_DIGIT { ["_"] ARABIC_DIGIT } | "٠" ;
HEX        = "0x" HEXDIG { ["_"] HEXDIG } ;        (* ASCII only; lowercase x only *)
BIN        = "0b" ("0"|"1") { ["_"] ("0"|"1") } ;
OCT        = "0o" OCTDIG { ["_"] OCTDIG } ;
FLOAT_LIT  = DEC_ASCII "." ASCII_DIGIT { ["_"] ASCII_DIGIT } [ EXP ]
           | DEC_ARABIC "٫" ARABIC_DIGIT { ["_"] ARABIC_DIGIT } [ EXP_AR ] ;
EXP        = ("e"|"E") ["+"|"-"] ASCII_DIGIT { ASCII_DIGIT } ;
EXP_AR     = ("e"|"E") ["+"|"-"] ARABIC_DIGIT { ARABIC_DIGIT } ;
```
- **A float literal always contains a decimal point.** `1e5` is K0124, with fix `1.0e5`. Together with the Int/Float split, this means a literal's kind is visible at a glance.
- Leading zeros (`007`) are K0125. There are no numeric suffixes in v0.1.
- **One digit system per literal** (`04 U5`). `1٢3` is K0126. Arabic-Indic floats use the Arabic decimal separator `٫` (U+066B), and ASCII floats use `.`. Mixing them is K0126.
- **The digit-run rule (decidable lexing of Franco).** When the lexer sees an ASCII digit, it reads the maximal run W = `[0-9A-Za-z_]+`. Then:
  0. If W has the form digits + `e`/`E` + digits (for example `1e5`), it is error K0124, with fix `1.0e5`.
  1. If W matches `INT_LIT`, it is a number. If it is followed by `.` and an ASCII digit, it continues as `FLOAT_LIT`.
  2. Otherwise, if the file is `fr` and W is in the closed Franco alias table (§6.3), it is that keyword.
  3. Otherwise it is error K0127: "numbers can't be glued to letters". The error lists the closest alias or identifier, using `02 §7.6` fuzzy folding, as a *suggestion only*.

  A build-time test in the compiler's CI proves that no alias matches `INT_LIT` (`02 §7.6`, `04 U5c`). This is easy to see. `INT_LIT` can contain letters only after a leading `0x`, `0b` or `0o`, and exponents exist only in `FLOAT_LIT`, after a `.`. No alias starts with `0`, and no alias contains `.`.
- A literal with no `.` has type `Int`. As a **literal-only** convenience, it may also be checked against an expected `Float`, `Decimal` or `BigInt` (`let r: Float = 2`). A float literal may be checked against `Decimal`, and it is then exact (`let p: Decimal = 19.99` is exactly 19.99). Variables never convert implicitly.

### 4.7 String literals
- `"…"` is a single-line string. `"""…"""` is a multi-line string: the common leading indentation of its content lines is stripped, and the first newline after the opening `"""` is dropped. A `"""…"""` literal may also sit on a single line, which is useful when the content contains `"`.
- **Escapes:** `\n \t \r \\ \" \{ \}` and `\u{XXXX}`. Unknown escapes are K0130.
- **Interpolation:** `{expr}` inside any string. `{{` and `}}` are literal braces. The expression's type must implement `Show` (§8.7). Interpolated expressions may not contain string literals with unescaped `"` in single-line strings. Use a `let` instead.
- **Tagged literals:** `TAG"…"` or `TAG"""…"""`, where `TAG ∈ {sql, html, sh, url, re}`, with no space before the quote. See §16.
- There are no raw strings in v0.1. Use `\\`.
- **String content lint (`04 U9`):** variation selectors, tag characters and unpaired surrogates in literals produce W0131.

### 4.8 Operators and punctuation (ASCII in all skins, except the aliases in §5.4)
```
+  -  *  /  //  %  ==  !=  <  <=  >  >=  =  +=  -=  *=  /=
->  =>  ..  ..=  ?  .  ,  :  (  )  [  ]  {  }  @  `
```
`and`, `or` and `not` are keywords (§6.1), not symbols. There are no bitwise operators (use `Int` methods `bit_and` etc.). There is no `**`, no `++`, no `===` and no `?:`. See the LLM-mistake table in §19.3.

---

## 5. D7 resolution: Arabic RTL, digits and punctuation

D7 is decided here from reports 02 and 04.

### 5.1 Direction
1. **Base direction per file follows its skin.** `.ar.kl` files are displayed RTL, and `.kl` / `.fr.kl` files LTR. This is a *display* rule for first-party tools (LSP semantic tokens, the VS Code extension, the playground and `kalam diff`). It is not stored in the file.
2. **No bidi controls in source, ever** (§4.1 `FORBIDDEN`). This includes LRM, RLM and ALM, which settles 02 vs 04 in favour of `04 U1`.
3. First-party tools render each token as an isolated run (UTS #55 §5) and mirror brackets, so `x + عمر == 1` cannot *look* like a different expression. `kalam diff` and code-review views can show any file in any skin, including a "logical order" English view (`04 U7`, `04 U8`).
4. **Terminal diagnostics:** terminals do not shape or reorder Arabic text. For `ar` files, `kalam` therefore prints the diagnostic message in the user's locale and the code excerpt twice: once as written, and once converted to the English skin with the same span highlighted. `--excerpt=ar|en|both` (default `both` on non-bidi terminals) controls this.

### 5.2 Digits
| Skin | Accepted in literals | Formatter emits |
|---|---|---|
| `en` | ASCII. Arabic-Indic is accepted and rewritten to ASCII | ASCII |
| `fr` | ASCII. Arabic-Indic is accepted and rewritten to ASCII | ASCII |
| `ar` | ASCII or Arabic-Indic, one system per literal | the project setting `[skin.ar] digits = "ascii"` (default) or `"arabic-indic"` |

- Persian/Extended digits are always rejected (K0105), because they are confusable with Arabic-Indic.
- *Rationale for the ASCII default:* report 02 §6.3 notes that Egyptians use Western digits for prices, phone numbers and code, and ASCII avoids a keyboard-layout switch for every number. Teams that prefer Arabic-Indic digits flip one project setting, and round-trips stay byte-exact (§3.3 G2).
- Digits **inside strings** are data and never converted. Runtime parsing (`"١٢٣".to_int()`) accepts both systems and returns `Ok(123)`.

### 5.3 Decimal separator
`.` with ASCII digits, and `٫` (U+066B) with Arabic-Indic digits. The Arabic comma `،` is **never** a decimal separator (`02 §7.5`).

### 5.4 Punctuation aliases (Arabic skin only)
| ASCII | Arabic | Accepted in `ar` input | `ar` formatter emits |
|---|---|---|---|
| `,` | `،` (U+060C) | both | project setting `[skin.ar] punctuation = "arabic"` (default) emits `،`; `"ascii"` emits `,` |
| `?` (try operator) | `؟` (U+061F) | both | same setting: `؟` or `?` |
| `;` | `؛` | rejected like `;` (K0904) | n/a |

All other operators and brackets stay ASCII in all skins (`02 §8`). `en` and `fr` files accept only ASCII punctuation, and `،` there is K0201.

### 5.5 Strings and output
- String contents are never normalized beyond NFC and never reordered.
- `std/web` emits `dir="auto"` on text containers by default and offers `dir` on layout components (report 06 §1.2).
- `print` writes code points in logical order. Rendering them is the terminal's job.

---

## 6. Keyword and core-name tables and alias policy

### 6.1 Keywords (31 concepts, plus `unsafe`)

The Arabic spellings are canonical, the way `fmt` prints them. Matching uses the fold key (§4.2), so `اذا` and `إذا` are the same keyword. The **Aliases** columns list the *closed* lists the lexer also accepts, which `fmt` rewrites to the canonical form. Franco spellings follow `02 §7.6`: ASCII, never starting with a digit, Cairene spelling, and avoiding the regional digits 6, 8 and 9 in canonical forms.

| # | Concept | English (canonical) | Arabic canonical | Arabic aliases | Franco canonical | Franco aliases |
|---|---|---|---|---|---|---|
| 1 | immutable binding | `let` | `ليكن` | — | `khali` | `khalli`, `5ali`, `5alli` |
| 2 | mutable binding | `var` | `متغير` | — | `motghayer` | `mot8ayer`, `motghayyer` |
| 3 | function | `fn` | `دالة` | `داله` (fold) | `dala` | `dalla`, `dalah` |
| 4 | return | `return` | `أرجع` | `ارجع` (fold) | `rag3` | `ragga3`, `raga3`, `rg3` |
| 5 | if | `if` | `إذا` | `لو` | `law` | `lw`, `lau` |
| 6 | else | `else` | `وإلا` | `والا` (fold) | `walla` | `wala`, `wla` |
| 7 | match | `match` | `طابق` | — | `hasab` | `7asab` |
| 8 | case | `case` | `حالة` | `حاله` (fold) | `hala` | `7ala` |
| 9 | for | `for` | `لكل` | — | `lekol` | `lkol`, `likol`, `le_kol` |
| 10 | in | `in` | `في` | — | `fi` | `fe`, `fy` |
| 11 | while | `while` | `طالما` | `بينما` | `tool_ma` | `tol_ma`, `tolma`, `toul_ma` |
| 12 | break | `break` | `اخرج` | `توقف` | `kfaya` | `kefaya`, `khalas`, `5alas` |
| 13 | continue | `continue` | `استمر` | — | `kammel` | `kamel`, `kml` |
| 14 | pass (empty statement) | `pass` | `تخطى` | `تخطي` (fold) | `fot` | `foot` |
| 15 | and | `and` | `و` | — | `we` | `wi` (`w` is rejected because it collides too easily) |
| 16 | or | `or` | `أو` | `او` (fold) | `aw` | `ao` |
| 17 | not | `not` | `ليس` | — | `mesh` | `msh`, `mish` |
| 18 | true | `true` | `صح` | — | `sa7` | `sah` |
| 19 | false | `false` | `خطأ` | `خطا` (fold) | `ghalat` | `8alat`, `ghalt` |
| 20 | record type | `record` | `سجل` | — | `sagel` | `sigel` |
| 21 | enum / sum type | `enum` | `تعداد` | — | `ta3dad` | — |
| 22 | type alias | `type` | `نوع` | — | `no3` | `noo3`, `naw3` |
| 23 | trait | `trait` | `سمة` | `سمه` (fold) | `sefa` | `sifa` |
| 24 | impl | `impl` | `نفذ` | — | `naffez` | `nafez` |
| 25 | import | `use` | `استخدم` | — | `hat` | `hatly`, `hatli` |
| 26 | rename | `as` | `باسم` | — | `besm` | `bism`, `b_esm` |
| 27 | export / public | `export` | `صدر` | — | `talla3` | `tala3`, `tl3` |
| 28 | opaque | `opaque` | `مغلق` | — | `ma2fool` | `ma2fol`, `mafool` |
| 29 | extern (FFI) | `extern` | `خارجي` | — | `kharegi` | `5aregi` |
| 30 | test block | `test` | `اختبار` | — | `ekhtebar` | `e5tebar` |
| 31 | receiver | `self` | `ذات` | — | `nafso` | `nafs`, `nafsu` |
| — | escape hatch | `unsafe` | `unsafe` (ASCII, identical) | none | `unsafe` (ASCII, identical) | none |

`else if` is written as two keywords: `وإلا إذا` in Arabic and `walla law` in Franco.

*Not keywords:* `async`, `await`, `null`, `nil`, `undefined`, `class`, `def`, `elif`, `try`, `catch`, `throw`, `const`, `function`, `interface`, `new`. They are ordinary identifiers, but using one in a place where the mistake is obvious triggers the targeted diagnostics in §19.3.

### 6.2 Core names (reserved, translated, cannot be shadowed)

Core names are prelude items whose *spelling* belongs to the skin table. They are reserved in their skin (K0413 if declared or shadowed), so translating them is unambiguous.

| Concept | English | Arabic | Franco |
|---|---|---|---|
| integer type | `Int` | `عدد` | `adad` |
| float type | `Float` | `كسري` | `kasri` |
| decimal type | `Decimal` | `عشري` | `ashri` |
| boolean type | `Bool` | `منطقي` | `mante2i` |
| text type | `String` | `نص` | `nass` |
| list type | `List` | `قائمة` | `lista` |
| map type | `Map` | `قاموس` | `qamoos` |
| set type | `Set` | `مجموعة` | `magmoo3a` |
| option type | `Option` | `اختياري` | `ekhtiyari` |
| result type | `Result` | `نتيجة` | `natiga` |
| present | `Some` | `موجود` | `mawgood` |
| absent | `None` | `لا_شيء` | `mafeesh` |
| success | `Ok` | `نجاح` | `naga7` |
| failure | `Err` | `فشل` | `fashal` |
| print line | `print` | `اطبع` | `ektb` |
| abort (bug) | `panic` | `انهيار` | `enhyar` |
| assertion | `assert` | `تحقق` | `et2aked` |
| entry point | `main` | `رئيسي` | `ra2isi` |
| capability root | `World` | `عالم` | `donya` |

Franco core-name aliases (closed list): `2ktb`, `ektb`, `ekteb`, `etb3` → `ektb`; `mafish`, `mfesh` → `mafeesh`; `nag7` → `naga7`; `fashl` → `fashal`. There are no Arabic core-name aliases other than fold variants.

**Never translated, ASCII in every skin:** `unsafe`; the tags `sql`, `html`, `sh`, `url`, `re`; the types `Sql`, `Html`, `Command`, `Url`, `Regex`, `Secret`, `Never`, `Self`; every `@attribute` name; and every stdlib module and member name (`std/http`, `.map`, `.with`, `db.all`). *Rationale:* `04 L13` greppability and `02 §7.3` ("library APIs stay English"). Arabic and Franco alias packs for stdlib names are an open question (§24).

### 6.3 Alias policy (per skin)

| Skin | Policy |
|---|---|
| **English** | **No aliases at all.** The English skin is the LLM-canonical form. Common foreign spellings (`def`, `elif`, `null` …) are *errors with fixes* (§19.3), never aliases. |
| **Arabic** | Orthographic variants are absorbed by the fold key (hamza/alef, ى/ي, ة/ه), and are not counted as aliases. Real lexical aliases form a closed, versioned list (v0.1: `لو`, `بينما`, `توقف`). Tatweel and harakat inside keywords are K0112, auto-fixed by `fmt`. |
| **Franco** | A closed, versioned alias table (`skins/fr.toml`, versioned with the compiler edition). It covers vowel variants and the digit-letter forms `5`=kh, `7`=h, `2`=ʔ/q, `3`=ʕ, `8`=gh. Digit-leading aliases are allowed only here, and lexed by the §4.6 digit-run rule. Fuzzy matching is used **only** in "did you mean" diagnostics and never accepts input silently. |
| **All** | The compiler accepts aliases with no warning. `kalam fmt` rewrites them to canonical spellings, and `kalam fmt --check` (CI) fails on any non-canonical spelling. Adding an alias is a minor-version change. Removing one is an edition change. |

### 6.4 Collision rules (enforced by a CI test over the tables, `05 P7`)
1. No two reserved words in one skin share a fold key.
2. No canonical Franco word starts with a digit, and no Franco alias matches `INT_LIT` (§4.6).
3. Within a skin, two reserved words at edit distance ≤ 1 after folding are allowed only if they can never appear in the same grammatical position. Example: English `or` (infix) and `for` (statement head). **Known exception E1:** Arabic `و` (and) and `أو` (or) are both infix and differ by one letter. They are kept because they are the standard Arabic conjunctions, and changing them would be unnatural. Linter rule L0114 flags a mixed `و`/`أو` chain written without parentheses. See open question Q4.
4. `صح` (true) was chosen over `صحيح`, and `Int` is `عدد`, not `صحيح`. This avoids the true/integer near-collision found in report 02's draft table. `None` is `لا_شيء`, not `عدم`, because `عدم` is one letter away from `عدد` (Int).
5. `تابع` was rejected for *continue* because some Arabic CS curricula use it to mean "function". `طبق` was rejected for *impl* because it looks like `طابق` (match).

---

## 7. Grammar (EBNF)

**Notation:** ISO-style EBNF. `{ x }` means zero or more, `[ x ]` means optional, and `"kw"` is the concept token that every skin's spelling maps to. Lexical tokens: `IDENT`, `CORE` (a core name from §6.2), `INT_LIT`, `FLOAT_LIT`, `STRING`, `TAGGED_STRING`, `NEWLINE`, `INDENT`, `DEDENT`, `EOF`. The lexer rules for `NEWLINE`/`INDENT`/`DEDENT`, bracket suppression, leading-dot continuation and block islands are in §4.5.

```ebnf
(* ---------- File and items ---------- *)
file          = { NEWLINE } { use_decl } { item } EOF ;

use_decl      = [ "export" ] "use" module_path [ use_tail ] NEWLINE ;
module_path   = IDENT { "/" IDENT } ;                    (* std/http, app/models/user *)
use_tail      = "." "{" use_item { "," use_item } [ "," ] "}"
              | "as" IDENT ;
use_item      = IDENT [ "as" IDENT ] | "self" ;        (* self also binds the module name *)

item          = { attribute } [ visibility ] item_body ;
attribute     = "@" IDENT { "." IDENT } [ "(" [ arg_list ] ")" ] NEWLINE ;
visibility    = "export" [ "opaque" ] ;                  (* opaque: record/enum only, K0302 *)
item_body     = fn_decl | record_decl | enum_decl | alias_decl | trait_decl
              | impl_decl | const_decl | extern_decl | test_decl ;

fn_decl       = fn_sig ( block | "=>" expr NEWLINE ) ;
fn_sig        = "fn" IDENT [ generics ] "(" [ param_list ] ")" [ "->" type ] ;
generics      = "[" gparam { "," gparam } [ "," ] "]" ;
gparam        = IDENT [ ":" type_path { "+" type_path } ] ;
param_list    = param { "," param } [ "," ] ;
param         = "self" | IDENT ":" type [ "=" expr ] ;

record_decl   = "record" IDENT [ generics ] ":" NEWLINE INDENT field_decl { field_decl } DEDENT ;
field_decl    = { attribute } IDENT ":" type [ "=" expr ] NEWLINE ;

enum_decl     = "enum" IDENT [ generics ] ":" NEWLINE INDENT variant { variant } DEDENT ;
variant       = IDENT [ "(" variant_field { "," variant_field } [ "," ] ")" ] NEWLINE ;
variant_field = [ IDENT ":" ] type ;

alias_decl    = "type" IDENT [ generics ] "=" type NEWLINE ;

trait_decl    = "trait" IDENT [ generics ] ":" NEWLINE INDENT trait_member { trait_member } DEDENT ;
trait_member  = fn_sig ( block | "=>" expr NEWLINE | NEWLINE ) ;   (* NEWLINE = required method *)

impl_decl     = "impl" [ generics ] [ type_path [ type_args ] "for" ] type ":"
                NEWLINE INDENT impl_member { impl_member } DEDENT ;
impl_member   = { attribute } [ "export" ] fn_decl ;

const_decl    = "let" IDENT [ ":" type ] "=" expr NEWLINE ;  (* module-level, pure, non-suspending *)

extern_decl   = "extern" ( "fn" IDENT [ generics ] "(" [ param_list ] ")" [ "->" type ]
                         | "type" IDENT [ generics ] ) NEWLINE ;

test_decl     = "test" STRING block ;

(* ---------- Blocks and statements ---------- *)
block         = ":" ( NEWLINE INDENT stmt { stmt } DEDENT
                    | line_stmt NEWLINE ) ;
line_stmt     = simple_stmt | "return" [ expr ]                (* one-line block body *)
              | "let" pattern [ ":" type ] "=" expr
              | "var" IDENT [ ":" type ] "=" expr
              | place ( "=" | "+=" | "-=" | "*=" | "/=" ) expr ;
stmt          = simple_stmt NEWLINE
              | let_stmt | var_stmt | assign_stmt | return_stmt
              | if_form | match_form | for_stmt | while_stmt ;
simple_stmt   = expr | "break" | "continue" | "pass" ;

let_stmt      = "let" pattern [ ":" type ] "=" rhs ;
var_stmt      = "var" IDENT [ ":" type ] "=" rhs ;
assign_stmt   = place ( "=" | "+=" | "-=" | "*=" | "/=" ) rhs ;
place         = IDENT { "." IDENT | "[" expr "]" } ;       (* root must be a var, K0710 *)
return_stmt   = "return" ( [ expr ] NEWLINE | if_form | match_form ) ;
rhs           = expr NEWLINE | if_form | match_form ;      (* if/match as multi-line values *)

if_form       = "if" expr block { "else" "if" expr block } [ "else" block ] ;
match_form    = "match" expr ":" NEWLINE INDENT case_arm { case_arm } DEDENT ;
case_arm      = "case" pattern [ "if" expr ] block ;
for_stmt      = "for" pattern "in" expr block ;
while_stmt    = "while" expr block ;

(* ---------- Expressions (lowest to highest precedence) ---------- *)
expr          = lambda | if_inline | or_expr ;
lambda        = "fn" "(" [ lparam { "," lparam } [ "," ] ] ")" [ "->" type ]
                ( "=>" expr | ":" island_block ) ;
lparam        = ( IDENT | "_" ) [ ":" type ] ;
island_block  = NEWLINE INDENT stmt { stmt } DEDENT ;      (* §4.5 rule 4 *)
if_inline     = "if" or_expr ":" or_expr { "else" "if" or_expr ":" or_expr }
                "else" ":" or_expr ;                        (* single logical line *)

or_expr       = and_expr { "or" and_expr } ;
and_expr      = not_expr { "and" not_expr } ;
not_expr      = "not" not_expr | cmp_expr ;
cmp_expr      = range_expr [ cmp_op range_expr ] ;          (* non-associative: a < b < c is K0720 *)
cmp_op        = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
range_expr    = add_expr [ ( ".." | "..=" ) add_expr ] ;
add_expr      = mul_expr { ( "+" | "-" ) mul_expr } ;
mul_expr      = unary { ( "*" | "/" | "//" | "%" ) unary } ;
unary         = "-" unary | postfix ;
postfix       = primary { "(" [ arg_list ] ")" | "[" expr { "," expr } "]" | "." member | "?" } ;
arg_list      = arg { "," arg } [ "," ] ;
arg           = [ IDENT ":" ] expr ;                         (* positional args before named *)
member        = IDENT | INT_LIT ;                             (* t.0 for tuples *)

primary       = literal | IDENT | CORE | "self"
              | "." IDENT                                     (* implicit member: .Circle, .public *)
              | "(" ")" | "(" expr ")" | "(" expr "," [ expr { "," expr } [ "," ] ] ")"
              | "[" [ expr { "," expr } [ "," ] ] "]"
              | "{" [ expr ":" expr { "," expr ":" expr } [ "," ] ] "}"
              | "unsafe" "." IDENT ;
literal       = INT_LIT | FLOAT_LIT | STRING | TAGGED_STRING | "true" | "false" ;

(* ---------- Patterns ---------- *)
pattern       = bind_pat { "|" bind_pat } ;
bind_pat      = prim_pat [ "as" IDENT ] ;
prim_pat      = "_" | IDENT                                    (* always a binding *)
              | [ "-" ] INT_LIT | STRING | "true" | "false"
              | INT_LIT ( ".." | "..=" ) INT_LIT
              | ( "." IDENT | type_path "." IDENT ) [ "(" pat_fields ")" ]   (* enum variant *)
              | ( "Some" | "Ok" | "Err" ) "(" pattern ")" | "None"           (* CORE ctors *)
              | type_path "(" pat_fields ")"                                  (* record *)
              | "(" pattern "," [ pattern { "," pattern } ] ")"
              | "[" [ list_elem { "," list_elem } [ "," ] ] "]" ;
list_elem     = pattern | ".." [ IDENT ] ;
pat_fields    = pat_field { "," pat_field } [ "," ".." ] ;
pat_field     = [ IDENT ":" ] pattern ;

(* ---------- Types ---------- *)
type          = fn_type | "(" ")" | "(" type "," [ type { "," type } [ "," ] ] ")" | named_type ;
named_type    = type_path [ type_args ] ;
type_args     = "[" type { "," type } [ "," ] "]" ;
type_path     = ( IDENT | CORE ) { "." IDENT } ;
fn_type       = [ "@sync" ] "fn" "(" [ type { "," type } [ "," ] ] ")" "->" type ;
```

**Parsing notes**
- **`[ ]` after an expression** is parsed as indexing. The type checker reinterprets `f[T1, T2](args)` as explicit type application when `f` resolves to a generic function or method, and the bracket contents parse as types (K0415 otherwise). *Rationale:* Go-style square-bracket generics. Explicit type arguments are rarely needed, because return-type inference covers most calls (`let u: User = req.decode_json()?`).
- **Statement vs inline `if`:** at statement start and in `rhs` position, `if` is `if_form`. Anywhere else inside an expression, it is `if_inline`, which requires `else`.
- **The `"(" [ arg_list ] ")"` suffix on a record type name** is construction. All arguments must be named (K0416), and fields with defaults may be omitted.
- **Ambiguity check:** the grammar is LL(2) except for `primary "("`, which the parser disambiguates between tuple and group by the presence of a comma. CI includes a tree-sitter grammar generated from this EBNF for all three skins (`03 §5`).

---

## 8. Type system

### 8.1 Overview
- **Static, nominal for records, enums and traits, structural for tuples and functions.** There is no subtyping except: (a) `Never <: T`, and (b) a non-suspending function type is a subtype of the same function type with suspension (§14.3).
- **No implicit conversions** (`04 L3`). No truthiness: conditions must be `Bool` (`04 L4`).
- **All values are immutable.** Mutation exists only as *rebinding* of `var` locals (§9.4) and through the explicit stdlib type `Cell[T]` (§14.6).

### 8.2 Primitive types

| Type | Values | Operations and semantics |
|---|---|---|
| `Int` | integers in **[−(2⁵³−1), 2⁵³−1]** | `+ - *` trap on leaving the range (`04 L7`). `//` floors and `%` takes the sign of the divisor (Python semantics). `/` returns `Float`. Division or modulo by 0 panics. Explicit `wrapping_add`, `checked_add` (→ `Option[Int]`) and `saturating_add` exist. |
| `BigInt` | arbitrary precision | Same operators. No overflow. Built with `BigInt.from(i)` or a literal in `BigInt` context. |
| `Float` | IEEE-754 binary64 | `+ - * /`, IEEE `==` and `<`. **Not `Hash` and not `Ord`**, so it cannot be a `Map` key or be sorted without `.total_cmp()`. `to_int()` returns `Option[Int]`. |
| `Decimal` | exact base 10, up to 34 significant digits (IEEE 754 decimal128 semantics) | `+ - *` are exact, and leaving the range traps. **No `/`.** Use `a.div(b, scale: 2, rounding: .HalfEven)`. For money (`04 L14`, report 01 COBOL lesson). |
| `Bool` | `true`, `false` | `and`, `or`, `not`, which short-circuit |
| `String` | immutable Unicode text | `+` concatenates. Interpolation. **No `s[i]`** (K0650). Use `.graphemes()`, `.chars()`, `.byte_len()`, `.slice(from, to)` (grapheme indices). Equality compares code points after NFC. |
| `Bytes` | immutable byte arrays | `.len()`, `.slice()`, base64/hex codecs |
| `()` | the unit value `()` | Returned by functions with no `->` |
| `Never` | no values | The type of `panic(...)`, `return`, `break` and `continue` expressions |

*Rationale for the `Int` range:* JS represents it as a plain `number` with an overflow check after `+ - *`, at almost no cost. JSON and npm numbers fit it naturally. The future C backend implements the same range with `int64_t` plus range checks, so the two backends agree exactly (`03 §4`, `04 L2`). Values that need more go in `BigInt`.

### 8.3 Built-in composite types
| Type | Notes |
|---|---|
| `(A, B, …)` | Tuples, accessed with `.0`, `.1`. Unit is `()`. |
| `List[T]` | Ordered and immutable. Literal `[a, b]`. `xs[i]` panics when out of bounds, and `xs.get(i)` returns `Option[T]`. |
| `Map[K, V]` | Immutable and insertion-ordered. `K: Hash + Eq`. Literal `{k: v}`, and `{}` is the empty map when the expected type is known. Hashing is keyed SipHash (`04 S14`). |
| `Set[T]` | Immutable. `Set.of([…])`. |
| `Option[T]` | `Some(x)` or `None`. **The only absence value** (`04 L5`). |
| `Result[T, E]` | `Ok(x)` or `Err(e)`. Marked must-use (§13.2). |
| `fn(A, B) -> R` | Function types. Each carries an inferred suspension bit, and `@sync fn(...)` forbids suspension (§14). |

*Performance note (not semantics):* the compiler updates `List`, `Map` and `Set` in place when it can prove the old value is unreferenced (Roc/Koka style, "functional but in place"). Semantically they are always values.

### 8.4 Records
```
export record User:
    id: Int
    name: String
    email: String
    role: Role = .Member
```
- Nominal and immutable. Construction uses **named arguments only**: `User(id: 1, name: "Sara", email: e)`. Fields with defaults may be omitted.
- **Update:** `u.with(name: "Omar")` returns a copy. `with` is a reserved method name on every record (a record field may not be called `with`, K0417).
- **Visibility:**
  - `export record` exposes the type, its fields (read-only) and its constructor.
  - `export opaque record` exposes only the type. Fields, construction and `.with` stay private to the defining module, so validated types such as `Email` can only be built through the module's own functions.
  - A non-exported record is module-private.
- Generic records: `record Page[T]:` with fields `items: List[T]`, `next: Option[String]`.

### 8.5 Enums (sum types)
```
enum Shape:
    Circle(radius: Float)
    Rect(width: Float, height: Float)
    Empty
```
- Variants are constructed with a **leading dot** when the expected type is known (`.Circle(radius: 2.0)`), or qualified (`Shape.Circle(radius: 2.0)`). A bare `Circle` is never a variant (§2.6).
- Variant fields may be named or positional, but not mixed. Named variant fields are constructed with named arguments. In patterns, all fields can be bound positionally in declaration order.
- Generic enums are allowed: `enum Tree[T]:`.
- `export opaque enum` hides the variants.

### 8.6 Generics
- Type parameters can appear on functions, records, enums, aliases and impls: `fn first[T](xs: List[T]) -> Option[T]`.
- Bounds use traits: `fn largest[T: Ord](xs: List[T]) -> Option[T]`, and several bounds are written `T: Eq + Hash`.
- There are no higher-kinded types, no variance annotations (everything is invariant, which is sound because values are immutable and there is no subtyping), no const generics and no specialization.
- **Implementation freedom:** the JS backend erases types and passes trait dictionaries. Effect specialization is the one exception (§14.3).

### 8.7 Traits
```
trait Show:
    fn show(self) -> String

impl Show for Money:
    fn show(self) -> String => "{self.amount} {self.currency}"
```
- Methods are declared in `impl` blocks. Inherent methods use `impl Type:` and trait methods use `impl Trait for Type:`.
- **Coherence (orphan rule):** an `impl Trait for Type` must live in the module that defines the trait or the one that defines the type.
- **Built-in traits:**

| Trait | Purpose | Derivation |
|---|---|---|
| `Eq` | `==`, `!=` | Automatic for records, enums and tuples whose components are all `Eq` |
| `Hash` | Map and Set keys | Automatic when all components are `Hash` |
| `Show` | interpolation and `print` | Automatic. `Secret[T]` shows `[REDACTED]` (`04 S5`) |
| `Ord` | `< <= > >=`, sorting | `@derive(Ord)`: explicit, lexicographic by declaration order |
| `Json` | `json.encode` / `json.decode[T]` | `@derive(Json)`: explicit, so nothing crosses the wire by accident |
| `Row` | decoding SQL rows by column name | `@derive(Row)` |
| (wire names) | `@name("title")` on a field sets its JSON key and SQL column name. Glossary conversion (§3.3) inserts it automatically on `Json`/`Row` records, so wire formats never change when identifiers are translated | — |
| `Error` | `.message()`, used by `AnyError` and logs | `impl Error for E:` |
| `From[E]` | conversion applied by `?` | written by hand |

- **No trait objects in v0.1.** Heterogeneity is expressed with enums or function values. The one built-in existential type is `AnyError` (§13.3).

### 8.8 Inference rules
1. **Mandatory annotations:** every top-level and `impl` function declares its parameter types, and its return type unless it returns `()`. Module constants infer their type from the initializer. Extern declarations are fully annotated.
2. **Local inference** is bidirectional with Hindley–Milner unification inside a function body. Local `let` bindings are **not generalized** (monomorphic locals), which keeps errors local and predictable.
3. **Lambdas** take parameter types from the expected function type. Without one, annotations are required (K0801).
4. **Literals:** an integer literal is `Int` unless the expected type is `Float`, `Decimal` or `BigInt`. A float literal is `Float` unless `Decimal` is expected. Empty `[]` and `{}` need an expected type (K0802).
5. **Return-type inference for generic calls:** a call such as `json.decode(text)` with type parameter `T` only in its return position takes `T` from the expected type (`let u: User = json.decode(text)?`).
6. **Implicit members** (`.Circle`, `.public`) resolve against the expected type. Without one, they are K0803.
7. Type errors report the expected type and the found type with their origins (*"`price` was inferred as `Float` here …"*), following `01 §3.4` rule 7.

### 8.9 Equality and comparison
`==` is defined only between two values of the *same* type that implements `Eq` (K0602 otherwise, `04 L4`). Equality on records and enums is structural. `Secret`, `Mac` and `PasswordHash` deliberately do not implement `Eq`. They offer `.equals_ct(other)` (constant time, `04 S5`).

---

## 9. Expressions and statements

### 9.1 Scoping
- Scoping is lexical and block-scoped.
- **Locals may not be shadowed:** declaring a name that is already bound in an enclosing local scope of the same function is K0705. Locals may shadow module items, which produces warning W0706. *Rationale:* shadowing is a common source of LLM and human bugs, and it would combine badly with Arabic fold keys (§4.2).
- Every binding must be initialized at its declaration.

### 9.2 `let` and `var`
- `let pattern [: T] = expr` binds immutably. The pattern must be irrefutable (tuples, records), or the binding is K1003: `let (lat, lng) = point.coords()`.
- `var name [: T] = expr` declares a re-bindable local.
- `var` is **allowed only inside function bodies.** There are no module-level variables (K1202).

### 9.3 `if` and `match` as values
- Used as a statement, `if` without `else` has type `()`.
- In `rhs` position (after `let x =`, `var x =`, an assignment or `return`), `if_form` and `match_form` are expressions. Each arm yields its **last expression**, and all arms must have the same type. `if` without `else` is not allowed there (K0731).
  ```
  let label = if n > 0:
      "positive"
  else:
      "not positive"
  ```
- `if_inline` (`if c: a else: b`) must fit on one logical line.

### 9.4 Assignment has value semantics
Assignment re-binds a `var`. It never mutates a value that someone else can see.

| Written | Means |
|---|---|
| `x = e` | rebind `x` |
| `p.x = e` | `p = p.with(x: e)` (nested: `a.b.c = e` → `a = a.with(b: a.b.with(c: e))`) |
| `xs[i] = e` | `xs = xs.set(i, e)` (panics when out of bounds) |
| `m[k] = e` | `m = m.insert(k, e)` |
| `x += e` | `x = x + e` (likewise `-=`, `*=`, `/=`) |

The root of a place must be a `var` (K0710 otherwise). Because values are never shared mutably, aliasing bugs cannot happen. The compiler performs the copy in place when the old value is unreferenced (§8.3).

### 9.5 Loops
- `for pattern in expr:` iterates any type that implements `Iterable[T]`:
  - `List`, `Set`, and `Map`, which yields `(key, value)` tuples in insertion order;
  - ranges (`a..b` excludes `b`, `a..=b` includes it, both are `Range[Int]`);
  - `String.chars()` and `.graphemes()`;
  - `.enumerate()`, which yields `(index, item)`.
- `while cond:` loops while `cond` is true.
- `break` and `continue` apply to the innermost loop. There are no labels and no `for … else`.
- Loops are statements and produce no value.

### 9.6 Operator precedence (high to low)

| Level | Operators | Associativity |
|---|---|---|
| 1 | postfix: call `()`, index `[]`, member `.`, try `?` | left |
| 2 | unary `-` | prefix |
| 3 | `*` `/` `//` `%` | left |
| 4 | `+` `-` | left |
| 5 | `..` `..=` | none |
| 6 | `==` `!=` `<` `<=` `>` `>=` | none (chaining is K0720) |
| 7 | `not` | prefix |
| 8 | `and` | left, short-circuit |
| 9 | `or` | left, short-circuit |

`+` is defined on `Int`, `BigInt`, `Float`, `Decimal`, `String`, `List` (concatenation) and `Sql`/`Html` (safe concatenation). Users cannot overload operators. Mixing numeric types (`Int + Float`) is K0601, with a fix to `.to_float()`.

### 9.7 Discarded values
An expression statement whose type is not `()` is K0730 ("value is unused"). Write `let _ = expr` to discard on purpose. For `Result` the code is K1303 (§13.2).

### 9.8 Indexing
- `xs[i]` on a `List` panics when out of bounds, and `xs.get(i)` returns `Option[T]`.
- `m[k]` on a `Map` panics when the key is absent, and `m.get(k)` returns `Option[V]`.
- Negative indices are a panic. They never count from the end. Use `xs.last()`.
- Strings cannot be indexed (K0650).

---

## 10. Pattern matching

```
match event:
    case .Paid(order_id, amount) if amount > 0:
        fulfil(order_id)
    case .Paid(_, _):
        log.warn("zero payment")
    case .Refunded(order_id: id, ..):
        refund(id)
    case .Cancelled:
        pass
```

1. **Exhaustiveness is mandatory** (`04 L8`). A non-exhaustive `match` is K1001, and the diagnostic lists the missing cases as ready-to-paste `case` arms. That is a machine-applicable fix that inserts `panic("TODO")` bodies.
2. Arms with guards do not count toward exhaustiveness. Unreachable arms are W1002.
3. **Bindings:** a bare identifier always binds. Variants need `.V` or `Type.V`. Core constructors (`Some`, `None`, `Ok`, `Err`) are reserved, so `None` is never a binding.
4. **Or-patterns** (`case .Card | .Wallet:`) must bind the same names with the same types in every alternative (K1004).
5. **Literal patterns:** `Int`, `String` and `Bool` literals, and `Int` ranges (`case 1..=9:`). `Float` patterns are not allowed (K1005).
6. **List patterns:** `[]`, `[x]`, `[first, ..rest]` and `[.., last]`.
7. **Record patterns:** `User(role: .Admin, name: n, ..)`. Without `..`, every field must be listed.
8. `as` binds the whole value: `case .Circle(_) as c:`.
9. Nested patterns are fully supported, for example `case Err(.NotFound(id)):`.
10. There is no `if let`. Use `match`, `?`, or `Option` methods (`.map`, `.or`, `.ok_or`). *Rationale:* one way to do it.

---

## 11. Functions and closures

### 11.1 Declarations
```
fn total(items: List[Item], tax: Decimal = 0.14) -> Decimal:
    var sum: Decimal = 0
    for item in items:
        sum += item.price * Decimal.from(item.qty)
    return sum + sum * tax

fn double(x: Int) -> Int => x * 2
```
- **Block-bodied functions with a non-`()` return type need an explicit `return`** on every path. A missing one is K1101. If the final statement is an expression of the return type, the fix inserts `return`. Block bodies have no implicit last-expression return. *Rationale:* LLMs and humans read `return` reliably. The value-of-last-expression rule is limited to `if`/`match` arms in value position (§9.3).
- **Expression-bodied** functions use `=> expr`.
- **Arguments:** positional arguments come first, then named ones (`f(a, b, tax: 0.1)`). Any parameter may be passed by name. Parameters with defaults are optional. A default expression may not refer to other parameters, and it is evaluated on each call.
- There is no overloading, no variadics (use `List[T]`) and no implicit `this`.

### 11.2 Methods
- Methods are declared in `impl` blocks with `self` as the first parameter and called as `x.m(args)`.
- Associated functions have no `self` and are called as `Type.f(args)`, for example `User.new(...)`.
- `Type.method` without a call is a function value whose first parameter is the receiver.
- Method resolution: inherent methods first, then methods of traits imported into the module. Ambiguity is K1103.

### 11.3 Closures
- `fn(x) => x + 1` is a single-expression closure. `fn(x):` followed by an indented block is a block closure, allowed inside brackets (§4.5 rule 4).
- **Capture is by value, of `let` bindings only.** Capturing a `var` is K1110, with the fix `let snapshot = v` inserted before the closure. *Rationale:* together with immutable values, this means closures never observe mutation. That is what makes concurrency race-free (§14.6).
- Closures are ordinary function values with an inferred suspension bit (§14).
- Tail calls are not guaranteed, because of the JS target. Deep recursion beyond about 10k frames panics with a stack-overflow message.

---

## 12. Modules, packages and visibility

### 12.1 Packages and modules
```
notes/                     # package "notes"
  kalam.toml
  kalam.lock
  src/
    main.kl                # module notes/main
    db.kl                  # module notes/db
    web/views.ar.kl        # module notes/web/views (Arabic skin)
    internal/money.kl      # module notes/internal/money (package-private)
  test/
    views_test.kl
```
- **One file is one module.** Its path is `<package>/<path without src/ and without the skin suffix>`.
- **Imports always use absolute paths:** `use notes/db.{App}`, `use std/http`. There are no relative imports and no glob imports (`*` is K1203, because globs feed hallucination and hide where names come from).
- `use std/http` binds `http`, so items are referenced as `http.Router`. `use std/http.{Router, Response}` binds names directly. `as` renames.
- **Import cycles are K1201.** Module constants are evaluated once, in dependency order, and must be pure and non-suspending.
- The prelude is imported implicitly: the core names (§6.2), `Html`, `Sql`, `Command`, `Url`, `Regex`, `Secret`, `World`, `AnyError`, `Never` and `Cell`, plus the capability types `Net`, `Fs`, `Env`, `Clock`, `Rand`, `Proc`, `Stdin` and `Screen` (ASCII in every skin, like all stdlib names).

### 12.2 Visibility
| Marker | Visible to |
|---|---|
| none | the defining module only |
| `export` | every module of the package. Also dependents, unless the module is under `src/internal/` |
| `export opaque` (record/enum) | as `export`, but fields, variants and the constructor stay private |
| `export use path.{X}` | re-export |

The public API of a package is every `export`ed item outside `internal/`. `kalam pkg api` prints it, and `kalam pkg diff` classifies changes for semver. Changes to the suspension bit count as breaking (§14.5).

### 12.3 Manifest (`kalam.toml`)
```toml
[package]
name = "notes"                 # ASCII [a-z0-9_-], namespaced on the registry: @team/notes
version = "0.1.0"
edition = "2027"
targets = ["js-node", "js-browser"]

[skin.ar]
digits = "ascii"               # or "arabic-indic"  (§5.2)
punctuation = "arabic"         # or "ascii"         (§5.4)

[dependencies]
"@kalam/web" = "0.3"

[npm]                          # JS deps used only through `extern` (§17)
nanoid = { version = "5.0.7", caps = [] }
stripe = { version = "17.5.0", caps = ["net:api.stripe.com"] }
```
- Package names are ASCII. Publish-time checks include cross-script typosquat detection (`04 P7`).
- `kalam.lock` stores content hashes, and CI installs are frozen (`04 P2`).
- **No install-time scripts, on any fetch path, for Kalam or npm dependencies** (`04 P1`). npm packages that need lifecycle scripts or native builds are unsupported in v0.1.

---

## 13. Error handling: `Result`, `?`, `panic`

### 13.1 Two channels (D16)
| Situation | Mechanism |
|---|---|
| Expected failure: bad input, a missing row, a network timeout | `Result[T, E]`, propagated with `?` |
| A bug: a broken invariant, out-of-bounds index, `Int` overflow, division by zero | `panic`, which aborts the current task |

There are no exceptions, no `try`/`catch`/`throw` and no way to catch a panic inside Kalam code (`04 L6`).

### 13.2 `Result` is must-use
An unused `Result` value is **K1303** (an error, not a warning). Write `let _ = r` to discard it explicitly. `kalam audit` counts these discards in non-test code.

### 13.3 The `?` operator
`e?` where `e: Result[T, E]`, inside a function returning `Result[U, F]`:
1. If `E == F`, it propagates `Err(e)` unchanged.
2. Otherwise, if `impl From[E] for F` exists, it propagates `Err(F.from(e))`.
3. Otherwise, if `F == AnyError` and `E: Error`, it boxes the error.
4. Otherwise it is K1301, with fix-its `.map_err(fn(e) => …)` and "add `impl From[E] for F`".

`e?` where `e: Option[T]`, inside a function returning `Option[U]`, propagates `None`. Using an `Option` with `?` in a function that returns `Result` is K1302, with fix `.ok_or(<error>)?`.

`AnyError` is the application-level catch-all (comparable to Rust's `anyhow`):
- `.message()`, `.chain()`, and `.is[E]() -> Option[E]`, which tests the underlying concrete type.
- `result.context("loading config")` wraps any `Result[T, E: Error]` into `Result[T, AnyError]` and adds a message.

`main` may return `()` or `Result[(), AnyError]`. If it returns `Err`, the message chain goes to stderr and the exit code is 1.

**Stdlib conversions for web apps:** `impl From[DbError] for HttpError` (→ 500, details logged and not shown), `From[JsonError]` (→ 400) and `From[AnyError]` (→ 500). This lets a handler returning `Result[Response, HttpError]` use `?` on database and decoding calls directly.

### 13.4 Panics
- They are raised by `panic(msg)`, `assert(cond, msg)`, `opt.expect(msg)`, `res.expect(msg)`, out-of-bounds indexing, `Int`/`Decimal` overflow, division by zero, and calling a suspending function after its task has been cancelled.
- There is no bare `.unwrap()`. `.expect("reason")` requires a reason, so every forced unwrap documents its invariant.
- **Unwinding stops at the task boundary** (§14):
  - **HTTP handler:** the response is 500 with a request ID. The panic is logged with `Secret` values redacted. The process keeps running. Production builds never render stack traces (`04 S11`).
  - **Main task:** the message and a Kalam-source stack trace (via source maps) go to stderr, and the exit code is 101.
  - **Child task in `task.scope`:** its siblings are cancelled and the panic re-raises in the parent.
  - **Tests:** `test.expect_panic(fn() => …)` asserts that a panic happens.

---

## 14. Concurrency without an `async` keyword (D12)

### 14.1 Model
- A program runs in an **isolate**: one heap and one event loop. v0.1 has exactly one isolate per process. Multi-isolate message passing is planned (§24).
- Inside an isolate, **tasks** are scheduled cooperatively. A task can be suspended **only during a call to a suspending function**.
- Source code looks synchronous. There are no `async` or `await` keywords, no promises and no callbacks in user code.

### 14.2 Suspension inference
- The **base suspending functions** are stdlib I/O primitives (network, filesystem, database, timers, `task.*`) and externs declared `@js(async: true)`.
- A function is **suspending** if its body calls a suspending function, or calls a function-typed value whose suspension bit is set. Inference runs to a fixpoint per strongly connected component of the call graph, so mutual recursion is handled.
- The bit is never written in source. The LSP shows it as an inlay hint (`⏸`) and in hover, and it appears in `kalam doc` output.

### 14.3 Function types, higher-order functions and stored functions
- Every function type has a hidden bit `s ∈ {sync, susp}`, with **sync <: susp**. A sync function may be used wherever a suspending one is expected.
- **Function-typed parameters are effect-polymorphic.** In `fn map[T, U](xs: List[T], f: fn(T) -> U) -> List[U]`, the bit of `f` is an implicit effect variable. `map` is suspending exactly when it is instantiated with a suspending `f`. The compiler emits at most two versions per such function, sync and suspending, and only those that are used (§14.8).
- **Function types inside data** (record fields, enum payloads, list/map elements, extern parameters) are **suspending by default**. Storing a sync function there is allowed by subtyping, and calling it is treated as a possible suspension. Write `@sync fn(...) -> R` in the field type to require and preserve a sync function. *Rationale:* sound, simple, with no effect parameters on data types.
- Lambdas get their bit from their body.

### 14.4 Requiring a non-suspending callback
- `@sync` on a function type (`update: @sync fn(State, Input) -> State`) means the argument must not suspend. Passing a suspending function is **K1401**, and the diagnostic shows the call chain that makes it suspend (*"`update` calls `save_score`, which calls `db.exec` (suspends)"*).
- `@sync` on a declaration (`@sync fn price(...)`) asserts that the function does not suspend (K1402 if it does). Use it to freeze an API promise.
- **Stdlib uses `@sync` for:** game `update`/`view` callbacks, `Cell.update`, comparators in `sort_by`, and template helpers called during rendering.

### 14.5 Suspension is part of the API
The suspension bit of every exported function is stored in the module interface (`.kli`). A change from sync to suspending is a **breaking change**: `kalam pkg diff` reports it, and the registry requires a major version bump. The other direction is compatible. Library authors should put `@sync` on functions that are meant to stay synchronous.

### 14.6 Shared state and atomicity
- **Between two suspension points, a task runs without interruption.**
- Values are immutable (§8), closures cannot capture `var` (§11.3) and there is no module-level `var` (§9.2). So the **only shared mutable state** is `Cell[T]` plus capability-backed external stores (databases, caches).
- `Cell[T]`: `Cell.new(v)`, `.get()`, `.set(v)`, `.update(@sync fn(T) -> T)`. Because the update function cannot suspend, **`update` is atomic**.
- Result: data races are impossible in v0.1 (`04 L11`). Race *conditions* against external stores remain visible in the code as separate calls, and are handled with DB transactions.

### 14.7 Structured concurrency (`std/task`)
| Function | Meaning |
|---|---|
| `task.all(fs: List[fn() -> T]) -> List[T]` | run concurrently, results in order. The first panic cancels the rest |
| `task.both(f: fn() -> A, g: fn() -> B) -> (A, B)` | typed pair version |
| `task.race(fs: List[fn() -> T]) -> T` | first to finish wins, the others are cancelled |
| `task.timeout(clock: Clock, ms: Int, f: fn() -> T) -> Result[T, TimedOut]` | deadline |
| `task.sleep(clock: Clock, ms: Int)` | suspend |
| `task.scope(body: fn(Scope) -> T) -> T` | `scope.spawn(f)` starts a child. The scope waits for all children before returning |

- There is **no unscoped fire-and-forget**. Work that should outlive a request goes into the DB-backed job queue (`std/jobs`).
- **Cancellation is cooperative.** A cancelled task stops at its next suspension point and unwinds as an unobservable "cancelled" panic. The scope's caller sees a normal result, or the sibling's panic.

### 14.8 Lowering to JavaScript (reference)
Kalam source:
```
fn load_dashboard(app: App, user_id: Int) -> Result[Dashboard, AnyError]:
    let (user, invoices) = task.both(
        fn() => users.find(app.db, user_id),
        fn() => invoices.recent(app.db, user_id),
    )
    return Ok(Dashboard(user: user?, invoices: invoices?))

fn names(users: List[User]) -> List[String] => users.map(fn(u) => u.name)
```
Emitted ES2022 (simplified):
```js
// suspending: calls task.both with suspending closures
export async function load_dashboard(app, user_id) {
  const [user, invoices] = await $task.both(
    async () => await users.find(app.db, user_id),
    async () => await invoices.recent(app.db, user_id),
  );
  if (!user.ok) return $Err($box(user.error));         // ? rule 3: DbError boxed into AnyError
  if (!invoices.ok) return $Err($box(invoices.error));
  return $Ok(new Dashboard(user.value, invoices.value));
}
// sync instance of List.map: no Promise, no await
export function names(users) { return $List.map(users, (u) => u.name); }
// List.map is emitted in two variants: $List.map (sync) and $List.map$s (async, awaits f)
```
`main`, if it suspends, is invoked as `await main(world)` by the generated entry module.

---

## 15. Capabilities and effects for I/O (`04 L12`)

### 15.1 Rules
1. Only `main` receives the root capability: `fn main(world: World)`. `World` cannot be constructed or decoded (K1501).
2. **Ambient (no capability needed):** pure computation; `print` and `log.*` (stdout/stderr); `panic`; `Cell`; JSON, strings, math; `Duration` arithmetic; hashing; and crypto that needs no randomness (`mac.verify`, `aead.open`).
3. **Capability-gated**, and only obtainable by being passed in:

| Capability | From `World` | Grants | Attenuation |
|---|---|---|---|
| `Net` | `world.net` | outbound HTTP, listening sockets | `.only(hosts: [...])`, `.allow_private()` (needed for SSRF-blocked ranges, `04 S12`) |
| `Fs` | `world.fs` | files | `.dir(path) -> Result[Fs, FsError]` (rooted, `..`-safe, `04 S13`), `.read_only()` |
| `Env` | `world.env` | environment variables | `.only([...])`. Keys matching `*_KEY`, `*_SECRET`, `*_TOKEN` or `PASSWORD` are returned as `Secret[String]` |
| `Clock` | `world.clock` | current time, timers | — |
| `Rand` | `world.rand` | CSPRNG (`04 S7`) | `Rand.seeded(n)` is a deterministic PRNG for games and tests and needs no capability |
| `Proc` | `world.proc` | running `Command`s (no shell) | `.only(programs: ["git"])` |
| `Stdin` | `world.stdin` | reading stdin | — |
| `Screen` | `world.screen(width:, height:)` | canvas and input (target `js-browser` only, K1502 elsewhere) | — |

4. **Capabilities are ordinary values.** Read a signature to know what a function can do: `fn charge(net: Net, order: Order) -> …` can reach the network, and `fn total(order: Order) -> Decimal` provably cannot.
5. **Packages:** Kalam-only packages need no capability declaration, because the type system already proves that they can only use what callers pass in. Packages with `extern` code must declare `ffi = true`, and each npm dependency in `[npm]` declares the capabilities its JS code uses. `kalam add` and `kalam update` show capability *diffs*, and additions need explicit approval (`04 P6`).
6. **Testing:** `std/test` provides fakes: `test.clock(at:)`, `test.net(routes)`, `test.fs(files)`, `test.env(map)`, `Rand.seeded(n)`.
7. There is no reflection and no call-by-name in v0.1, so there is no `reflect` capability (`04 L9`).

### 15.2 Web apps
Applications collect their capabilities into their own record and hand it to the server once:
```
record App:
    db: Db
    clock: Clock
    payments: Net
```
Handlers receive `Request[App]` and reach capabilities through `req.app`. Each handler's power is still visible from the `App` type, which is ordinary code.

---

## 16. Safe string kinds (`04 §3.2`, `04 S1–S3`)

### 16.1 Common rules
- `TAG"…"` and `TAG"""…"""` are parsed **at compile time** by a tag-specific grammar. Interpolations are typed holes, not text splices.
- Each tag produces a distinct type: `sql`→`Sql`, `html`→`Html`, `sh`→`Command`, `url`→`Url`, `re`→`Regex`.
- **A `String` never converts to these types** (K1601). For example: *"`db.all` expects `Sql`, but this is `String`. Write the query as `sql"…"` and put values in `{…}`."* The fix rewrites `"…" + x` concatenations into a tagged literal when it can do so mechanically.
- Values of the same safe type may be concatenated with `+` or `Sql.join(parts, sep)`.
- **Escape hatches** are `unsafe.raw_sql(s)`, `unsafe.raw_html(s)` and `unsafe.raw_url(s)`. They use the ASCII keyword `unsafe` in every skin, are flagged by the linter and counted by `kalam audit` and the registry (`04 L13`, `04 P10`). There is **no** escape hatch that runs a shell string.

### 16.2 `sql"…"` → `Sql`
- Each `{expr}` becomes a **bound parameter**, with placeholders generated per driver (`$1` for Postgres, `?` for SQLite).
- Interpolating an `Sql` value splices it as SQL, which is how queries are composed safely.
- Interpolating a `List[T]` expands to a parenthesized parameter list for `IN {ids}`. An empty list is a runtime `DbError.EmptyList`, not invalid SQL.
- A hole inside SQL quotes (`'{name}'`) is K1602 ("parameters must not be quoted").
- Identifiers such as column names cannot be parameters. Use `sql.ident(name, allowed: [...]) -> Result[Sql, DbError]`, which validates against an allow-list.
- `Db` API (`std/sql`):
  - `db.all(q) -> Result[List[T], DbError]` with `T: Row`, inferred from the expected type
  - `db.one(q) -> Result[Option[T], DbError]`
  - `db.exec(q) -> Result[Int, DbError]` (rows affected)
  - `db.transaction(fn(tx: Db) -> Result[T, E]) -> Result[T, E]` with `E: From[DbError]`. It commits on `Ok` and rolls back on `Err` or panic.
- `kalam db check` (P2, `04 S16`) optionally verifies every `sql` literal against a live or migration-derived schema.

### 16.3 `html"…"` → `Html` (contextual auto-escaping, `04 S3`)
The literal is tokenized as an HTML5 fragment at compile time. Unbalanced tags are K1610. Each hole is classified by its context:

| Context | Allowed hole types | Treatment |
|---|---|---|
| text | `Html`, `List[Html]`, `Option[Html]` | inserted as is (already safe). `None` renders nothing |
| text | any other `T: Show` | `.show()`, then HTML-escaped |
| quoted attribute value | `T: Show` | attribute-escaped. **Unquoted attribute holes are K1611** |
| URL attribute (`href`, `src`, `action`, `formaction`) | `Url`, or `T: Show` | `Url` is inserted. Other types are checked at runtime: only relative, `http`, `https` and `mailto` survive, and anything else becomes `about:invalid#kalam` |
| `<script>`, `<style>`, `on*=` attributes, attribute names, tag names | — | **holes forbidden** (K1612) |

- `std/web` treats attributes starting with `k-` as server-driven interaction directives (`k-post`, `k-get`, `k-target`, `k-swap`, `k-live`), which a small fixed runtime processes (D11).
- Forms with `method="post"` get a CSRF token injected automatically (`04 S8`).
- Generated pages carry a nonce-based CSP and Trusted Types (`04 §4.3`).

### 16.4 `sh"…"` → `Command` (no shell, ever: `04 S2`)
- The literal text is split into argv words at compile time. Single and double quotes group words. **Shell metacharacters outside quotes (`| ; & > < $ ( ) *` and backtick) are K1620.**
- Each `{expr}` hole is exactly **one** argv element (`T: Show`). A `List[String]` hole expands to several elements. A hole can never introduce a word boundary or an option on its own, and interpolated values are never re-parsed.
- Run it with `proc.run(cmd) -> Result[Output, ProcError]`, which requires `Proc`.
- Lint W1621 suggests `--` before holes that could start with `-` (`sh"git log -- {path}"`).

### 16.5 `url"…"` and `re"…"`
- `url"https://api.example.com/users/{id}?q={term}"` percent-encodes each hole for its URL component (path segment or query value). The scheme and host must be literal.
- `re"^[a-z0-9_]{3,20}$"` is compiled and syntax-checked at compile time, with RE2 semantics: linear time, no backreferences, safe from ReDoS. Holes are not allowed (K1630).

---

## 17. Interop with JavaScript and npm

### 17.1 `extern` declarations
```
@js(module: "nanoid", name: "nanoid")
extern fn nanoid(size: Int) -> String

@js(module: "stripe", name: "default")
extern type StripeClient

@js(module: "stripe", name: "default", new: true)
extern fn stripe_client(key: String) -> StripeClient

@js(member: "checkout.sessions.create", async: true, throws: true)
extern fn create_session(client: StripeClient, params: SessionParams) -> Result[Session, JsError]
```
| `@js` option | Meaning |
|---|---|
| `module`, `name` | ESM import specifier and export name (`"default"` allowed) |
| `new: true` | call as a constructor |
| `member: "a.b.c"` | call a member path on the first argument (method call) |
| `async: true` | the JS function returns a Promise, so this extern **suspends** (§14.2) |
| `throws: true` | JS throws are caught, and the return type must be `Result[T, JsError]`. Without it, a JS throw becomes a Kalam **panic** |
| `global: "fetch"` | a global instead of a module import (gated: the package must declare the matching capability) |

### 17.2 Boundary type mapping (checked at runtime on every crossing)
| Kalam | JS | Check on values coming in from JS |
|---|---|---|
| `Int` | `number` | `Number.isSafeInteger` |
| `Float` | `number` | `typeof === "number"` |
| `Bool` / `String` | `boolean` / `string` | `typeof` |
| `()` | `undefined` | ignored |
| `Option[T]` | `T \| undefined \| null` | both `null` and `undefined` become `None` |
| `List[T]` | `Array` | copied and frozen, elements checked |
| `Bytes` | `Uint8Array` | copied |
| record or enum with `Json` | plain object | **schema decode** (`04 S4`): unknown keys rejected, `__proto__`, `constructor` and `prototype` keys rejected, depth and size limits |
| `extern type X` | any object | opaque reference. Only externs can use it |
| `fn(A) -> R` | function | wrapped both ways. JS callbacks are assumed to suspend if they return a Promise |

### 17.3 Security and supply chain
- Any module containing `extern` makes its package an **FFI package** (`ffi = true` in the manifest, shown on the registry page, counted by `kalam audit`).
- npm dependencies are fetched by `kalam` from the npm registry using `kalam.lock` integrity hashes. **Lifecycle scripts are never run** (`04 P1`). A release-age cooldown of 48 h is the default (`04 P9`).
- Generated JS contains no `eval` and no `new Function`. Kalam maps compile to `Map`, and decoded objects are built with `Object.create(null)` and copied into class instances (`04 P14`).

### 17.4 Binding generation from TypeScript
`kalam bind npm:stripe@17.5.0 --only Stripe,checkout.sessions` reads `.d.ts` files and generates an `extern` module:
- interfaces become `@derive(Json)` records;
- classes become `extern type` plus `member:` externs;
- `Promise<T>` becomes `async: true`.

Unsupported TS features (overloads, conditional and mapped types) become `# TODO(bind)` comments. Curated bindings are published as ordinary Kalam packages (`@kalam-bind/stripe`) so applications avoid writing `extern` themselves (report 06 §5.3).

### 17.5 Calling Kalam from JS/TS
`@js.export` on an exported function emits it in an ESM entry with a `.d.ts`:

| Kalam | TypeScript |
|---|---|
| record | `readonly` interface |
| enum | discriminated union `{ tag: "Circle"; radius: number }` |
| `Option[T]` | `T \| undefined` |
| `Result[T, E]` | `{ ok: true; value: T } \| { ok: false; error: E }` |
| suspending function | returns `Promise<T>` |

Arguments are validated at the boundary with the same checks as §17.2.

---

## 18. Standard library shape (v0.1)

The rule from report 06 §5.3 applies: **the beginner path to a working SaaS must be first-party.** npm is for advanced users. All stdlib names are English in every skin (§6.2). Every function listed here has a runnable doc example, and those examples are also tests (`05 D3`).

### 18.1 Module map

| Module | Contents (v0.1) | Capability |
|---|---|---|
| prelude | core names, `Html` (`.render() -> String`), `Sql`, `Command`, `Url`, `Regex`, `Secret`, `World`, `AnyError`, `Never`, `Cell`, capability types | — |
| `std/string` | `String` methods: `len_chars`, `byte_len`, `graphemes`, `chars`, `slice`, `split`, `join`, `trim`, `is_empty`, `starts_with`, `contains`, `replace`, `to_upper`, `to_lower`, `to_int → Result[Int, ParseError]`, `to_float`, `to_decimal`, `normalize_arabic` (fold key) | — |
| `std/list`, `std/map`, `std/set` | `len`, `is_empty`, `get`, `set`, `append`, `insert`, `remove`, `map`, `filter`, `fold`, `find`, `any`, `all`, `sort_by(@sync cmp)`, `sorted` (`T: Ord`), `enumerate`, `zip`, `take`, `skip`, `first`, `last`, `group_by`, `sum` | — |
| `std/option`, `std/result` | `map`, `or`, `ok_or`, `expect`, `is_some`, `is_none` / `map`, `map_err`, `ok`, `expect`, `context`, `is_ok` | — |
| `std/math` | `PI`, `abs`, `min`, `max`, `clamp`, `sqrt`, `floor`, `round`, `pow` | — |
| `std/decimal` | `Decimal.from(Int)`, `Decimal.parse`, `div(b, scale:, rounding:)`, `round(scale:)`, `Rounding` | — |
| `std/bigint` | `BigInt.from`, the arithmetic operators | — |
| `std/json` | `json.encode(T: Json) → String`, `json.decode(String) → Result[T, JsonError]` with size and depth limits (`04 S4`) | — |
| `std/time` | `Instant`, `Date`, `Duration`, `Zone`, `clock.now()`, `clock.today(zone)`, Hijri formatting | `Clock` to read the time |
| `std/log` | `log.info`, `log.warn`, `log.error`, structured fields. `Secret` is redacted | — (stderr) |
| `std/env` | `env.get(key) → Option[String]`, `env.secret(key) → Option[Secret[String]]`. Calling `env.get` with a literal key that looks secret is K1503 | `Env` |
| `std/fs`, `std/path` | `fs.read_text`, `fs.write_text`, `fs.list`, `fs.dir`. `path.join` returns a `Result` and rejects escapes (`04 S13`) | `Fs` |
| `std/http` | `http.router()`, `Router[A]` with `.get/.post/.put/.delete(path, handler, access:)`, `http.serve(net, port:, router:, app:)`, `Request[A]` (`.app`, `.path_int`, `.path`, `.query`, `.json`, `.form`, `.session`), `Response` (`.json`, `.html`, `.text`, `.redirect`, `.status`), `HttpError`, `Access`, and the client `http.get(net, url)` etc. (SSRF-blocking, `04 S12`) | `Net` |
| `std/web` | layouts, `form` binding into declared records (unknown fields rejected, `04 S10`), the `k-*` live runtime, the RTL-first component kit, flash messages, assets | — (via `std/http`) |
| `std/auth` | cookie sessions (`__Host-`, `Secure`, `HttpOnly`, `SameSite=Lax`), `password.hash` / `password.verify` (Argon2id), login rate limiting | `Rand`, `Clock` |
| `std/sql` | `sql.sqlite(fs, path)`, `sql.postgres(net, url: Secret[String])`, `Db` (`.one`, `.all`, `.exec`, `.transaction`), `sql.ident`, `DbError`, migrations (`kalam db migrate`) | `Fs` / `Net` |
| `std/jobs` | DB-backed job queue with retries | `Db` |
| `std/task` | §14.7, plus `Cell` | `Clock` for timeouts and sleep |
| `std/proc` | `proc.run(Command) → Result[Output, ProcError]` | `Proc` |
| `std/crypto` | `aead.seal` / `aead.open`, `mac`, `sign` (Ed25519), `hkdf`, `random.token(rand, bytes:)`, and `legacy/` for MD5/SHA-1 | `Rand` where random |
| `std/test` | `test` blocks, `expect_eq`, `expect_panic`, snapshot tests, fake capabilities | — |
| `std/i18n` | message tables for ar/en/fr, plural rules, number formatting with Arabic-Indic digits | — |
| `std/game` | `game.run(screen, init:, update: @sync fn(S, Input, Float) -> S, view: @sync fn(S) -> Picture)`, `Input.is_down(Key)`, `Key`, `Picture`, `draw.rect/circle/text/group/image`, `Color` | `Screen` (browser only) |
| `std/js` | `JsError`, interop helpers | `ffi` packages only |
| `std/unsafe` | reachable only as `unsafe.x`: `raw_html`, `raw_sql`, `raw_url` | counted by audit |

**Planned for 0.2, not part of v0.1:** Postgres pooling options, OAuth, Paymob/Fawry/Stripe payment adapters (report 06 §5.4), S3 storage, websockets, and the `k-live` push channel.

### 18.2 HTTP essentials (enough to type-check the examples)
```
enum Access:
    Public
    LoggedIn
    Role(name: String)

enum HttpError:
    BadRequest(message: String)
    Unauthorized
    Forbidden
    NotFound
    Conflict(message: String)
    Internal

# documentation notation: these are methods declared in `impl Router[A]:` / `impl Request[A]:`
# (A = the application record; `self: T` shows the receiver type)
fn router[A]() -> Router[A]
fn get[A](self: Router[A], path: String, handler: fn(Request[A]) -> Result[Response, HttpError],
          access: Access) -> Router[A]
fn serve[A](net: Net, port: Int, router: Router[A], app: A) -> Result[(), ServeError]
fn path_int[A](self: Request[A], name: String) -> Result[Int, HttpError]      # 400 on bad input
fn json[A, T: Json](self: Request[A]) -> Result[T, HttpError]                   # 400 on bad input
```
- `access:` has no default. **Forgetting it is a compile error** (missing argument), and that is how routes are deny-by-default (`04 S9`).
- `.LoggedIn` and `.Role` use the session middleware from `std/auth`, which `http.router()` installs. A failing check returns 401 or 403 before the handler runs.
- The stdlib provides `impl From[DbError] for HttpError`, `From[JsonError]` and `From[AnyError]` (§13.3).

---

## 19. Canonical form for LLMs, diagnostics and the tooling contract

### 19.1 What "canonical Kalam" is
Canonical Kalam is **English-skin source exactly as `kalam fmt` prints it.** It is what docs show first, what `llms.txt` teaches, what packages are published as, and what agents are asked to write. Humans can read and edit it in any skin, because conversion is deterministic (report 06 §7.2).

**Formatter rules (non-configurable except the two `[skin.ar]` settings):**
1. 4-space indentation and 100 columns. Lines are broken inside brackets with one item per line and a trailing comma.
2. One statement per line. A one-line block is allowed only for a single simple statement that fits.
3. `use` declarations come first, sorted: `std/` first, then dependencies, then the current package. Names inside `{…}` are sorted.
4. Items are separated by one blank line, and blank lines are capped at one.
5. Double-quoted strings. Multi-line text uses `"""`.
6. Aliases are rewritten to canonical keywords, fold variants to declaration spellings, and Arabic-Indic digits per §5.2.
7. Redundant backticks, parentheses and `pass` are removed.
8. Attributes go one per line above the item.

### 19.2 Rules that exist for writability
| Rule | Why it helps an LLM (and a reviewer) |
|---|---|
| Top-level signatures are fully annotated | Errors stay local, and the model can generate a body against a fixed contract |
| Explicit `return` in block bodies | Removes the Rust/Ruby/Python ambiguity about implicit returns |
| No shadowing, no globals, no `var` capture | Removes classes of silent bugs that models introduce often |
| No glob imports, absolute module paths | Every name's origin is visible in the file, which lowers the hallucinated-API rate |
| One spelling per construct, a closed keyword set | Fewer valid outputs means more probability mass on the right one |
| `.Variant`, `Some/None/Ok/Err`, `match/case` | Directly borrowed from Swift, Rust and Python |
| Named record construction | Field-order mistakes become impossible |
| Spec budget | `llms.txt` + compact spec ≤ 20k tokens at MVP and ≤ 12k at 1.0 (`05 D3`). `llms-full.txt` ≤ 40k, generated from this spec and the stdlib docs |

### 19.3 Known LLM mistakes: diagnostics with machine-applicable fixes
Each row is an **error** (never a silent alias, so canonical code stays canonical). Each has a fix that `kalam fix` applies automatically. They are detected only in the obvious syntactic position, so `def` as a variable name stays legal.

| Code | Written | Fix |
|---|---|---|
| K0901 | `a === b`, `a !== b` | `==`, `!=` |
| K0902 | `def f(x):`, `function f(x)`, `func f` | `fn f(x: …):` (types left as `_` holes for the model) |
| K0903 | `elif c:` / `else if (c) {` | `else if c:` |
| K0904 | trailing `;` | remove it, splitting statements onto separate lines |
| K0905 | `{ … }` used as a block | colon + indentation |
| K0906 | `null`, `nil`, `undefined` | `None`, plus a hint about `Option[T]` |
| K0907 | `async fn`, `await x` | delete it: suspension is inferred (§14) |
| K0908 | `try:` / `catch` / `throw` / `raise` | `?` on a `Result`, or `return Err(…)` |
| K0909 | `x++`, `x--` | `x += 1` |
| K0910 | `c ? a : b` | `if c: a else: b` |
| K0911 | `&&`, `\|\|`, `!x` | `and`, `or`, `not x` |
| K0912 | `let mut x` / `const x` | `var x` / `let x` |
| K0913 | `f"…{x}"`, `` `…${x}` `` | `"…{x}"` |
| K0914 | `List<T>`, `Vec<T>`, `Array<T>` | `List[T]` |
| K0915 | `x.unwrap()` | `x.expect("…")` (explain the invariant) |
| K0916 | `print(a, b)` with several arguments | `print("{a} {b}")` |
| K0917 | `Circle(r)` pattern for a variant | `.Circle(r)` |
| K0918 | `len(xs)`, `str(x)`, `int(s)` | `xs.len()`, `"{x}"`, `s.to_int()?` |
| K0124 | `1e5` | `1.0e5` |
| K1601 | string-built SQL, HTML or shell | tagged literal with holes |

The **hallucinated-API guard:** an unknown method gives K0420 with the three closest stdlib names by edit distance *and* by a synonym table (`push`→`append`, `length`→`len`, `includes`→`contains`).

### 19.4 Diagnostic format
```json
{
  "code": "K1301",
  "severity": "error",
  "message": {"en": "…", "ar": "…", "fr": "…"},
  "file": "src/notes.kl", "skin": "en",
  "span": {"start": {"line": 12, "col": 9}, "end": {"line": 12, "col": 27}},
  "labels": [{"span": {...}, "text": "this is `DbError`"},
             {"span": {...}, "text": "function returns `Result[Note, ParseError]`"}],
  "fixes": [{"title": "add .map_err", "applicability": "machine",
             "edits": [{"span": {...}, "insert": ".map_err(fn(e) => …)"}]}],
  "explain": "https://kalam.dev/errors/K1301"
}
```
- `kalam check --json` and the LSP emit this format. Fixes marked `"applicability": "machine"` are safe for `kalam fix` and agents to apply without review.
- Messages follow `01 §3.4`: plain title, why, fix, no blame. Every code has message text and an `explain` page in all three skins (`05 P6`).

### 19.5 Agent tooling (ships with the compiler, report 06 §7.3)
- `kalam mcp` is an MCP server with the tools `check`, `fix`, `fmt`, `convert_skin`, `run_tests`, `doc_search` and `explain`.
- `kalam new` writes an `AGENTS.md` that tells agents to write canonical English skin, run `kalam check --json` after every edit and apply machine fixes.
- `kalam convert --to ar|fr|en` is the deterministic skin translator. It never uses an LLM (02, APL anti-pattern).

---

## 20. Lowering to JavaScript (reference semantics)

The output is ES2022 ESM for Node ≥ 22, Bun, Deno and modern browsers, with Source Map v3 and `.d.ts` for `@js.export`. The emitted code passes a CSP without `unsafe-eval`.

| Kalam | JS lowering |
|---|---|
| module | one ESM file. Module constants become `const`s evaluated in import order |
| record | frozen class instance with fixed fields (`new User(id, name, email)`). `.with` copies |
| enum | one frozen class per variant with a numeric `$tag`. Payload-free variants are singletons |
| `Option[T]` | `undefined` for `None` and the value itself for `Some(v)`, **unless** `T` can itself be `undefined` (a nested `Option`, `()` or a type variable), in which case `$Some(v)` boxes it |
| `Result[T, E]` | `{ok: true, value}` / `{ok: false, error}` (shared shapes for V8) |
| `Int` | JS `number`. `+ - *` emit `$i(a + b)`, which checks `Number.isSafeInteger` and panics otherwise. Range analysis elides proven-safe checks. `//` and `%` use `$floordiv`/`$mod` (floored, panic on 0) |
| `Float` | `number` |
| `Decimal`, `BigInt` | a small runtime decimal128 library, and native `bigint` |
| `String` | JS string. APIs work in code points and graphemes (`Intl.Segmenter`) and never expose UTF-16 units |
| `List[T]` | frozen `Array`. In-place updates happen when the compiler proves uniqueness |
| `Map`, `Set` | `$Map`/`$Set`: JS `Map` for primitive keys, and structural hashing (keyed SipHash) for compound keys. **Never plain objects** (`04 §4.3`) |
| `==` | `===` for primitives, generated `$eq` for structural types |
| `match` | `switch` on `$tag` plus nested tests. Exhaustiveness is checked statically, and a `default` that panics is emitted only for defense in depth |
| `panic` | `throw new $Panic(msg, kalamTrace)`. The runtime catches it at task and request boundaries (§13.4) |
| suspending function | `async function`. Each call site gets `await`. Polymorphic higher-order functions get two variants (`f` / `f$s`) (§14.8) |
| `sql`/`html`/`sh` literals | precompiled templates: `$Sql([…parts], [args])`, a precomputed HTML template with per-hole escapers, an argv array |
| capabilities | runtime objects created only by the entry module. Kalam code cannot forge one (the classes are not exported to user scope) |
| `extern` | a direct import plus boundary checks (§17.2) |

---

## 21. Deliberately excluded (v0.1)

| Excluded | Reason |
|---|---|
| Classes, inheritance, `this` | Records + enums + traits cover SaaS modeling. Inheritance is a frequent source of LLM and human design bugs |
| `null`/`undefined`/`nil` | `Option` only (`04 L5`) |
| Exceptions, `try`/`catch`/`throw` | `Result` + `?` + panic (D16, `04 L6`) |
| `async`/`await` keywords, promises in user code | D12, §14 |
| `eval`, dynamic `import`, reflection, call-by-name | `04 L9` |
| Macros, compile-time metaprogramming (beyond `@derive`) | Keeps code greppable and the language small. Derives are a closed, compiler-known set |
| User-defined operators, operator overloading | One meaning per symbol |
| Implicit conversions, truthiness, loose equality | `04 L3`, `04 L4` |
| Function overloading, variadics | One way to call |
| Module-level mutable state, mutable shared references | §14.6 race freedom |
| Shadowing of locals | §9.1 |
| Bitwise operators, `**`, `++`, `?:`, `===` | Methods or the canonical spelling (§19.3) |
| Semicolons, braces for blocks, block comments | D15 and one way to write things |
| Glob imports, relative imports, import cycles | §12.1 |
| Shell-string process APIs | `04 S2` |
| Trait objects, higher-kinded types, variance, specialization | Complexity a SaaS team does not need in v0.1 |
| Shared-memory threads | §14.1. Isolates with message passing come later |
| Native-binary backend features (pointers, `unsafe` blocks of raw memory) | C/WASM backend later (D4). Only `unsafe.*` functions exist |
| Install scripts in any dependency | `04 P1` |
| Per-file skin mixing and per-skin syntax | D6, P7 |

---

## 22. Security charter traceability (report 04, P0 rules)

| Rule | Where satisfied |
|---|---|
| L1 memory safety | §8 (GC semantics, no pointers, bounds-checked indexing §9.8) |
| L2 no UB on any backend | §8.2 (defined `Int`/`Decimal` overflow, division by zero), §20 |
| L3 strong static typing, no implicit conversions | §8.1, §8.8, §9.6 |
| L4 one strict equality, no truthiness | §8.9, §8.1 |
| L5 no null | §8.3 `Option`, §21 |
| L6 errors as values, unused `Result` is an error | §13.2 (K1303) |
| L7 overflow traps | §8.2 |
| L8 exhaustive match | §10 (K1001) |
| L9 no eval / dynamic import / call-by-name | §12.1, §15.1 rule 7, §21 |
| L10 immutable by default | §8.1, §9.2, §9.4 |
| L12 capability-passing I/O | §15 |
| L13 escape hatches ASCII and identical in every skin | §4.3, §6.1, §16.1 |
| U1 bidi controls rejected | §4.1 `FORBIDDEN`, §5.1 |
| U2 NFC source | §3.1 (K0101) |
| U3 UAX #31, single script, no tatweel/harakat/ZWJ | §4.1, §4.2 |
| U4 confusable skeleton collisions | §4.2 (K0108, K0110) |
| U5 one digit system per literal, keywords never numbers | §4.6, §6.3 |
| U6 one skin per file | §2.1, §3.1 (K0201) |
| S1 typed sinks | §16 |
| S2 no shell strings | §16.4 |
| S3 contextual auto-escaping | §16.3 |
| S4 schema-first decoding with limits | §17.2, §18.1 `std/json` |
| S7 CSPRNG by default | §15.1 `Rand` |
| S14 DoS-resistant hashing | §8.3, §20 |
| P1 no install-time code | §12.3, §17.3 |
| P2 lockfile with hashes | §12.3 |
| 4.3 prototype-safe JS, no eval in output | §17.2, §17.3, §20 |

---

## 23. Example programs in three skins

**How to read these:**
- The **English** block is canonical (§19.1).
- **[converter]** marks blocks that are exactly what `kalam convert` prints. Only keywords, core names, punctuation and digits change, so identifiers stay as written.
- **[converter + glossary]** marks blocks that also apply a project glossary (`names.ar.toml` / `names.fr.toml`, §3.3), so identifiers are translated too. Glossary output has a different AST hash from the English original, and wire names are kept with `@name`.
- In both modes, **string literal text is never translated**, which is why the output strings are identical in all three skins. User-facing text belongs in `std/i18n` tables.
- Arabic blocks use the default settings `digits = "ascii"` and `punctuation = "arabic"` (`،` and `؟`).
- Markdown viewers do not apply Kalam's per-token bidi isolation (§5.1), so Arabic lines may look reordered here. The first-party tools display them correctly.

### 23.1 Hello world

```kalam
# hello.kl
fn main():
    print("Hello, world!")
```
```kalam
# hello.ar.kl  [converter]
دالة رئيسي():
    اطبع("Hello, world!")
```
```kalam
# hello.fr.kl  [converter]
dala ra2isi():
    ektb("Hello, world!")
```

### 23.2 FizzBuzz

```kalam
# fizzbuzz.kl
fn main():
    for n in 1..=100:
        if n % 15 == 0:
            print("FizzBuzz")
        else if n % 3 == 0:
            print("Fizz")
        else if n % 5 == 0:
            print("Buzz")
        else:
            print("{n}")
```
```kalam
# fizzbuzz.ar.kl  [converter: the identifier n is kept as written]
دالة رئيسي():
    لكل n في 1..=100:
        إذا n % 15 == 0:
            اطبع("FizzBuzz")
        وإلا إذا n % 3 == 0:
            اطبع("Fizz")
        وإلا إذا n % 5 == 0:
            اطبع("Buzz")
        وإلا:
            اطبع("{n}")
```
```kalam
# fizzbuzz.fr.kl  [converter]
dala ra2isi():
    lekol n fi 1..=100:
        law n % 15 == 0:
            ektb("FizzBuzz")
        walla law n % 3 == 0:
            ektb("Fizz")
        walla law n % 5 == 0:
            ektb("Buzz")
        walla:
            ektb("{n}")
```
With `[skin.ar] digits = "arabic-indic"`, the second line of the Arabic file prints as `لكل n في ١..=١٠٠:` and `n % 15` prints as `n % ١٥`. Both forms parse to the same AST.

### 23.3 Records, enums and `match`

```kalam
# shapes.kl
enum Shape:
    Circle(radius: Float)
    Rect(width: Float, height: Float)

record Item:
    label: String
    shape: Shape

fn area(shape: Shape) -> Float:
    return match shape:
        case .Circle(r): 3.14159 * r * r
        case .Rect(w, h): w * h

fn main():
    let items = [
        Item(label: "coin", shape: .Circle(radius: 1.5)),
        Item(label: "card", shape: .Rect(width: 8.5, height: 5.4)),
    ]
    for item in items:
        print("{item.label}: {area(item.shape)}")
```
```kalam
# shapes.ar.kl  [converter + glossary]
تعداد شكل:
    دائرة(نصف_القطر: كسري)
    مستطيل(العرض: كسري، الارتفاع: كسري)

سجل عنصر:
    الاسم: نص
    الشكل: شكل

دالة المساحة(الشكل: شكل) -> كسري:
    أرجع طابق الشكل:
        حالة .دائرة(ن): 3.14159 * ن * ن
        حالة .مستطيل(ع، ط): ع * ط

دالة رئيسي():
    ليكن العناصر = [
        عنصر(الاسم: "coin"، الشكل: .دائرة(نصف_القطر: 1.5))،
        عنصر(الاسم: "card"، الشكل: .مستطيل(العرض: 8.5، الارتفاع: 5.4))،
    ]
    لكل العنصر في العناصر:
        اطبع("{العنصر.الاسم}: {المساحة(العنصر.الشكل)}")
```
```kalam
# shapes.fr.kl  [converter + glossary]
ta3dad shakl:
    dayra(nos_qotr: kasri)
    mostateel(el_ard: kasri, el_ertefa3: kasri)

sagel onsor:
    el_esm: nass
    el_shakl: shakl

dala el_mesa7a(el_shakl: shakl) -> kasri:
    rag3 hasab el_shakl:
        hala .dayra(n): 3.14159 * n * n
        hala .mostateel(a, t): a * t

dala ra2isi():
    khali el_anaser = [
        onsor(el_esm: "coin", el_shakl: .dayra(nos_qotr: 1.5)),
        onsor(el_esm: "card", el_shakl: .mostateel(el_ard: 8.5, el_ertefa3: 5.4)),
    ]
    lekol el_onsor fi el_anaser:
        ektb("{el_onsor.el_esm}: {el_mesa7a(el_onsor.el_shakl)}")
```
*Notes:*
- The type `شكل` and the field `الشكل` sit in separate namespaces (§2.6).
- Values take the `ال` / `el_` prefix, so they never collide with record names (K0412).
- The variants `.دائرة` and `.dayra` work without capital letters because of the leading dot.

### 23.4 `Result` and `?`

```kalam
# config.kl
enum ConfigError:
    Missing(key: String)
    NotANumber(key: String, value: String)

fn read_port(env: Env) -> Result[Int, ConfigError]:
    let raw = env.get("PORT").ok_or(ConfigError.Missing(key: "PORT"))?
    return match raw.to_int():
        case Ok(port): Ok(port)
        case Err(_): Err(.NotANumber(key: "PORT", value: raw))

fn main(world: World):
    match read_port(world.env):
        case Ok(port):
            print("Listening on port {port}")
        case Err(.Missing(key)):
            print("Please set {key}")
        case Err(.NotANumber(key, value)):
            print("{key}={value} is not a number")
```
```kalam
# config.ar.kl  [converter + glossary]
تعداد مشكلة_الإعداد:
    مفقود(المفتاح: نص)
    غير_رقمي(المفتاح: نص، القيمة: نص)

دالة اقرأ_المنفذ(البيئة: Env) -> نتيجة[عدد، مشكلة_الإعداد]:
    ليكن الخام = البيئة.get("PORT").ok_or(مشكلة_الإعداد.مفقود(المفتاح: "PORT"))؟
    أرجع طابق الخام.to_int():
        حالة نجاح(المنفذ): نجاح(المنفذ)
        حالة فشل(_): فشل(.غير_رقمي(المفتاح: "PORT"، القيمة: الخام))

دالة رئيسي(العالم: عالم):
    طابق اقرأ_المنفذ(العالم.env):
        حالة نجاح(المنفذ):
            اطبع("Listening on port {المنفذ}")
        حالة فشل(.مفقود(المفتاح)):
            اطبع("Please set {المفتاح}")
        حالة فشل(.غير_رقمي(المفتاح، القيمة)):
            اطبع("{المفتاح}={القيمة} is not a number")
```
```kalam
# config.fr.kl  [converter + glossary]
ta3dad moshkelet_e3dad:
    na2es(el_moftah: nass)
    ghair_raqami(el_moftah: nass, el_qeema: nass)

dala e2ra_el_port(el_bee2a: Env) -> natiga[adad, moshkelet_e3dad]:
    khali el_khaam = el_bee2a.get("PORT").ok_or(moshkelet_e3dad.na2es(el_moftah: "PORT"))?
    rag3 hasab el_khaam.to_int():
        hala naga7(el_port): naga7(el_port)
        hala fashal(_): fashal(.ghair_raqami(el_moftah: "PORT", el_qeema: el_khaam))

dala ra2isi(el_donya: donya):
    hasab e2ra_el_port(el_donya.env):
        hala naga7(el_port):
            ektb("Listening on port {el_port}")
        hala fashal(.na2es(el_moftah)):
            ektb("Please set {el_moftah}")
        hala fashal(.ghair_raqami(el_moftah, el_qeema)):
            ektb("{el_moftah}={el_qeema} is not a number")
```
*Notes:*
- `ok_or` is generic in its error type, so the argument is qualified (`ConfigError.Missing`), because nothing gives an expected type for `.Missing`.
- In `Err(.NotANumber …)`, the return type supplies the expected type, so the leading-dot form works.
- `Env` and `.get` are stdlib names and stay ASCII (§6.2).

### 23.5 HTTP handler (JSON API, deny-by-default routes)

```kalam
# notes.kl
use std/http.{self, HttpError, Request, Response, Router}
use std/sql.{self, Db}

record App:
    db: Db

@derive(Json, Row)
record Note:
    id: Int
    title: String
    body: String

@derive(Json)
record NewNote:
    title: String
    body: String

fn show_note(req: Request[App]) -> Result[Response, HttpError]:
    let id = req.path_int("id")?
    let query = sql"SELECT id, title, body FROM notes WHERE id = {id}"
    let found: Option[Note] = req.app.db.one(query)?
    return match found:
        case Some(note): Ok(Response.json(note))
        case None: Err(.NotFound)

fn create_note(req: Request[App]) -> Result[Response, HttpError]:
    let input: NewNote = req.json()?
    if input.title.is_empty():
        return Err(.BadRequest(message: "title is required"))
    let query = sql"""
        INSERT INTO notes (title, body) VALUES ({input.title}, {input.body})
        RETURNING id, title, body
    """
    let created: Option[Note] = req.app.db.one(query)?
    return Ok(Response.json(created.expect("RETURNING yields one row")).status(201))

fn routes() -> Router[App]:
    return http.router()
        .get("/notes/{id}", show_note, access: .Public)
        .post("/notes", create_note, access: .LoggedIn)

fn main(world: World) -> Result[(), AnyError]:
    let db = sql.sqlite(world.fs, "notes.db")?
    http.serve(world.net, port: 8080, router: routes(), app: App(db: db))?
    return Ok(())
```
```kalam
# notes.ar.kl  [converter + glossary]
استخدم std/http.{ذات، HttpError، Request، Response، Router}
استخدم std/sql.{ذات، Db}

سجل تطبيق:
    القاعدة: Db

@derive(Json، Row)
سجل ملاحظة:
    @name("id")
    المعرف: عدد
    @name("title")
    العنوان: نص
    @name("body")
    المحتوى: نص

@derive(Json)
سجل ملاحظة_جديدة:
    @name("title")
    العنوان: نص
    @name("body")
    المحتوى: نص

دالة اعرض_ملاحظة(الطلب: Request[تطبيق]) -> نتيجة[Response، HttpError]:
    ليكن المعرف = الطلب.path_int("id")؟
    ليكن الاستعلام = sql"SELECT id, title, body FROM notes WHERE id = {المعرف}"
    ليكن المطلوبة: اختياري[ملاحظة] = الطلب.app.القاعدة.one(الاستعلام)؟
    أرجع طابق المطلوبة:
        حالة موجود(الملاحظة): نجاح(Response.json(الملاحظة))
        حالة لا_شيء: فشل(.NotFound)

دالة أنشئ_ملاحظة(الطلب: Request[تطبيق]) -> نتيجة[Response، HttpError]:
    ليكن المدخل: ملاحظة_جديدة = الطلب.json()؟
    إذا المدخل.العنوان.is_empty():
        أرجع فشل(.BadRequest(message: "title is required"))
    ليكن الاستعلام = sql"""
        INSERT INTO notes (title, body) VALUES ({المدخل.العنوان}, {المدخل.المحتوى})
        RETURNING id, title, body
    """
    ليكن الجديدة: اختياري[ملاحظة] = الطلب.app.القاعدة.one(الاستعلام)؟
    أرجع نجاح(Response.json(الجديدة.expect("RETURNING yields one row")).status(201))

دالة المسارات() -> Router[تطبيق]:
    أرجع http.router()
        .get("/notes/{id}"، اعرض_ملاحظة، access: .Public)
        .post("/notes"، أنشئ_ملاحظة، access: .LoggedIn)

دالة رئيسي(العالم: عالم) -> نتيجة[()، AnyError]:
    ليكن القاعدة = sql.sqlite(العالم.fs، "notes.db")؟
    http.serve(العالم.net، port: 8080، router: المسارات()، app: تطبيق(القاعدة: القاعدة))؟
    أرجع نجاح(())
```
```kalam
# notes.fr.kl  [converter + glossary]
hat std/http.{nafso, HttpError, Request, Response, Router}
hat std/sql.{nafso, Db}

sagel tatbee2:
    el_qa3da: Db

@derive(Json, Row)
sagel molahza:
    @name("id")
    el_raqam: adad
    @name("title")
    el_3enwan: nass
    @name("body")
    el_mo7tawa: nass

@derive(Json)
sagel molahza_gdeda:
    @name("title")
    el_3enwan: nass
    @name("body")
    el_mo7tawa: nass

dala e3rad_molahza(el_talab: Request[tatbee2]) -> natiga[Response, HttpError]:
    khali el_raqam = el_talab.path_int("id")?
    khali el_estelam = sql"SELECT id, title, body FROM notes WHERE id = {el_raqam}"
    khali el_matlooba: ekhtiyari[molahza] = el_talab.app.el_qa3da.one(el_estelam)?
    rag3 hasab el_matlooba:
        hala mawgood(el_molahza): naga7(Response.json(el_molahza))
        hala mafeesh: fashal(.NotFound)

dala ensha2_molahza(el_talab: Request[tatbee2]) -> natiga[Response, HttpError]:
    khali el_madkhal: molahza_gdeda = el_talab.json()?
    law el_madkhal.el_3enwan.is_empty():
        rag3 fashal(.BadRequest(message: "title is required"))
    khali el_estelam = sql"""
        INSERT INTO notes (title, body) VALUES ({el_madkhal.el_3enwan}, {el_madkhal.el_mo7tawa})
        RETURNING id, title, body
    """
    khali el_gdeda: ekhtiyari[molahza] = el_talab.app.el_qa3da.one(el_estelam)?
    rag3 naga7(Response.json(el_gdeda.expect("RETURNING yields one row")).status(201))

dala el_masarat() -> Router[tatbee2]:
    rag3 http.router()
        .get("/notes/{id}", e3rad_molahza, access: .Public)
        .post("/notes", ensha2_molahza, access: .LoggedIn)

dala ra2isi(el_donya: donya) -> natiga[(), AnyError]:
    khali el_qa3da = sql.sqlite(el_donya.fs, "notes.db")?
    http.serve(el_donya.net, port: 8080, router: el_masarat(), app: tatbee2(el_qa3da: el_qa3da))?
    rag3 naga7(())
```
*Notes:*
- `?` on `DbError` inside a handler returning `HttpError` uses the stdlib `From[DbError]` (§13.3). In `main`, `?` boxes into `AnyError`.
- Removing `access:` from a route is a compile error (§18.2).
- The commas *inside* `sql"…"` stay ASCII in the Arabic file, because they are literal text. The holes are code and are converted.
- In the Arabic `main`, the local `القاعدة` and the field `القاعدة` share a spelling. That is legal: field labels are not bindings.

### 23.6 SQL query (and the injection that does not compile)

```kalam
# debtors.kl
use std/sql.{self, Db, DbError}

@derive(Row)
record Customer:
    id: Int
    name: String
    email: String
    balance: Decimal

fn top_debtors(db: Db, city: String, limit: Int) -> Result[List[Customer], DbError]:
    return db.all(sql"""
        SELECT id, name, email, balance
        FROM customers
        WHERE city = {city} AND balance > 0
        ORDER BY balance DESC
        LIMIT {limit}
    """)

# This does not compile:
#   let q = "SELECT * FROM customers WHERE city = '" + city + "'"
#   db.all(q)    # K1601: `db.all` expects `Sql`, found `String`

fn main(world: World) -> Result[(), AnyError]:
    let db = sql.sqlite(world.fs, "shop.db")?
    for c in top_debtors(db, "Cairo", 10)?:
        print("{c.name} <{c.email}> owes {c.balance} EGP")
    return Ok(())
```
```kalam
# debtors.ar.kl  [converter + glossary]
استخدم std/sql.{ذات، Db، DbError}

@derive(Row)
سجل عميل:
    @name("id")
    المعرف: عدد
    @name("name")
    الاسم: نص
    @name("email")
    البريد: نص
    @name("balance")
    الرصيد: عشري

دالة أكبر_المدينين(القاعدة: Db، المدينة: نص، الحد: عدد) -> نتيجة[قائمة[عميل]، DbError]:
    أرجع القاعدة.all(sql"""
        SELECT id, name, email, balance
        FROM customers
        WHERE city = {المدينة} AND balance > 0
        ORDER BY balance DESC
        LIMIT {الحد}
    """)

# This does not compile:
#   let q = "SELECT * FROM customers WHERE city = '" + city + "'"
#   db.all(q)    # K1601: `db.all` expects `Sql`, found `String`

دالة رئيسي(العالم: عالم) -> نتيجة[()، AnyError]:
    ليكن القاعدة = sql.sqlite(العالم.fs، "shop.db")؟
    لكل ع في أكبر_المدينين(القاعدة، "Cairo"، 10)؟:
        اطبع("{ع.الاسم} <{ع.البريد}> owes {ع.الرصيد} EGP")
    أرجع نجاح(())
```
Glossary note: the natural Franco spelling `3amil` (customer) would be rejected, because identifiers cannot start with a digit (§2.3, K0127). The glossary therefore uses `ameel`.
```kalam
# debtors.fr.kl  [converter + glossary]
hat std/sql.{nafso, Db, DbError}

@derive(Row)
sagel ameel:
    @name("id")
    el_raqam: adad
    @name("name")
    el_esm: nass
    @name("email")
    el_bareed: nass
    @name("balance")
    el_raseed: ashri

dala akbar_el_madyoneen(el_qa3da: Db, el_madina: nass, el_7ad: adad) -> natiga[lista[ameel], DbError]:
    rag3 el_qa3da.all(sql"""
        SELECT id, name, email, balance
        FROM customers
        WHERE city = {el_madina} AND balance > 0
        ORDER BY balance DESC
        LIMIT {el_7ad}
    """)

# This does not compile:
#   let q = "SELECT * FROM customers WHERE city = '" + city + "'"
#   db.all(q)    # K1601: `db.all` expects `Sql`, found `String`

dala ra2isi(el_donya: donya) -> natiga[(), AnyError]:
    khali el_qa3da = sql.sqlite(el_donya.fs, "shop.db")?
    lekol a fi akbar_el_madyoneen(el_qa3da, "Cairo", 10)?:
        ektb("{a.el_esm} <{a.el_bareed}> owes {a.el_raseed} EGP")
    rag3 naga7(())
```
(Comments are never translated, which is why the English comment survives in every skin.)

### 23.7 HTML view (server-rendered, auto-escaped, live form)

```kalam
# todos.kl
record Todo:
    title: String
    done: Bool

fn todo_item(todo: Todo) -> Html:
    let state = if todo.done: "done" else: "open"
    return html"""<li class="{state}">{todo.title}</li>"""

fn todo_page(owner: String, todos: List[Todo]) -> Html:
    return html"""
        <section dir="auto">
          <h1>{owner}'s tasks</h1>
          <ul id="todos">{todos.map(todo_item)}</ul>
          <form method="post" action="/todos" k-post="/todos" k-target="#todos" k-swap="append">
            <input name="title" required>
            <button>Add</button>
          </form>
        </section>
    """

fn main():
    let todos = [
        Todo(title: "Pay rent", done: true),
        Todo(title: "<script>alert(1)</script>", done: false),
    ]
    print(todo_page("Sara", todos).render())
```
```kalam
# todos.ar.kl  [converter + glossary]
سجل مهمة:
    العنوان: نص
    منجزة: منطقي

دالة عنصر_مهمة(المهمة: مهمة) -> Html:
    ليكن الوضع = إذا المهمة.منجزة: "done" وإلا: "open"
    أرجع html"""<li class="{الوضع}">{المهمة.العنوان}</li>"""

دالة صفحة_المهام(المالك: نص، المهام: قائمة[مهمة]) -> Html:
    أرجع html"""
        <section dir="auto">
          <h1>{المالك}'s tasks</h1>
          <ul id="todos">{المهام.map(عنصر_مهمة)}</ul>
          <form method="post" action="/todos" k-post="/todos" k-target="#todos" k-swap="append">
            <input name="title" required>
            <button>Add</button>
          </form>
        </section>
    """

دالة رئيسي():
    ليكن المهام = [
        مهمة(العنوان: "Pay rent"، منجزة: صح)،
        مهمة(العنوان: "<script>alert(1)</script>"، منجزة: خطأ)،
    ]
    اطبع(صفحة_المهام("Sara"، المهام).render())
```
```kalam
# todos.fr.kl  [converter + glossary]
sagel mohemma:
    el_3enwan: nass
    khalset: mante2i

dala bond_mohemma(el_mohemma: mohemma) -> Html:
    khali el_wad3 = law el_mohemma.khalset: "done" walla: "open"
    rag3 html"""<li class="{el_wad3}">{el_mohemma.el_3enwan}</li>"""

dala saf7et_el_maham(el_saheb: nass, el_maham: lista[mohemma]) -> Html:
    rag3 html"""
        <section dir="auto">
          <h1>{el_saheb}'s tasks</h1>
          <ul id="todos">{el_maham.map(bond_mohemma)}</ul>
          <form method="post" action="/todos" k-post="/todos" k-target="#todos" k-swap="append">
            <input name="title" required>
            <button>Add</button>
          </form>
        </section>
    """

dala ra2isi():
    khali el_maham = [
        mohemma(el_3enwan: "Pay rent", khalset: sa7),
        mohemma(el_3enwan: "<script>alert(1)</script>", khalset: ghalat),
    ]
    ektb(saf7et_el_maham("Sara", el_maham).render())
```
*Notes:*
- The second todo's title renders as `&lt;script&gt;…`.
- `{todos.map(todo_item)}` is a `List[Html]` hole in text context, so it is inserted as is.
- The `class="{state}"` hole is attribute-escaped.
- Inside a handler, the page is returned with `Ok(Response.html(todo_page(...)))`, and the CSRF token is injected into the form automatically.

### 23.8 Small game loop (browser target)

```kalam
# bounce.kl
use std/game.{self, Input, Picture, draw}

record Ball:
    x: Float
    y: Float
    dx: Float
    dy: Float

record State:
    ball: Ball
    paddle: Float
    score: Int

let WIDTH: Float = 640.0
let HEIGHT: Float = 480.0

fn init() -> State:
    let ball = Ball(x: 320.0, y: 100.0, dx: 180.0, dy: 220.0)
    return State(ball: ball, paddle: 280.0, score: 0)

fn update(state: State, input: Input, dt: Float) -> State:
    var s = state
    if input.is_down(.Left):
        s.paddle -= 400.0 * dt
    if input.is_down(.Right):
        s.paddle += 400.0 * dt
    s.ball.x += s.ball.dx * dt
    s.ball.y += s.ball.dy * dt
    if s.ball.x < 0.0 or s.ball.x > WIDTH:
        s.ball.dx = -s.ball.dx
    if s.ball.y < 0.0:
        s.ball.dy = -s.ball.dy
    let near_bottom = s.ball.y > HEIGHT - 20.0
    let over_paddle = s.ball.x > s.paddle and s.ball.x < s.paddle + 80.0
    if near_bottom and over_paddle and s.ball.dy > 0.0:
        s.ball.dy = -s.ball.dy
        s.score += 1
    if s.ball.y > HEIGHT:
        return init()
    return s

fn view(state: State) -> Picture:
    return draw.group([
        draw.rect(x: 0.0, y: 0.0, width: WIDTH, height: HEIGHT, color: .Black),
        draw.circle(x: state.ball.x, y: state.ball.y, radius: 8.0, color: .White),
        draw.rect(x: state.paddle, y: HEIGHT - 12.0, width: 80.0, height: 12.0, color: .White),
        draw.text("Score: {state.score}", x: 10.0, y: 24.0, color: .White),
    ])

fn main(world: World):
    game.run(world.screen(width: 640, height: 480), init: init(), update: update, view: view)
```
```kalam
# bounce.ar.kl  [converter + glossary]
استخدم std/game.{ذات، Input، Picture، draw}

سجل كرة:
    س: كسري
    ص: كسري
    سرعة_س: كسري
    سرعة_ص: كسري

سجل وضع_اللعبة:
    الكرة: كرة
    المضرب: كسري
    النقاط: عدد

ليكن العرض: كسري = 640.0
ليكن الارتفاع: كسري = 480.0

دالة البداية() -> وضع_اللعبة:
    ليكن الكرة = كرة(س: 320.0، ص: 100.0، سرعة_س: 180.0، سرعة_ص: 220.0)
    أرجع وضع_اللعبة(الكرة: الكرة، المضرب: 280.0، النقاط: 0)

دالة حدث(الوضع: وضع_اللعبة، المدخلات: Input، الزمن: كسري) -> وضع_اللعبة:
    متغير ح = الوضع
    إذا المدخلات.is_down(.Left):
        ح.المضرب -= 400.0 * الزمن
    إذا المدخلات.is_down(.Right):
        ح.المضرب += 400.0 * الزمن
    ح.الكرة.س += ح.الكرة.سرعة_س * الزمن
    ح.الكرة.ص += ح.الكرة.سرعة_ص * الزمن
    إذا ح.الكرة.س < 0.0 أو ح.الكرة.س > العرض:
        ح.الكرة.سرعة_س = -ح.الكرة.سرعة_س
    إذا ح.الكرة.ص < 0.0:
        ح.الكرة.سرعة_ص = -ح.الكرة.سرعة_ص
    ليكن قرب_القاع = ح.الكرة.ص > الارتفاع - 20.0
    ليكن فوق_المضرب = ح.الكرة.س > ح.المضرب و ح.الكرة.س < ح.المضرب + 80.0
    إذا قرب_القاع و فوق_المضرب و ح.الكرة.سرعة_ص > 0.0:
        ح.الكرة.سرعة_ص = -ح.الكرة.سرعة_ص
        ح.النقاط += 1
    إذا ح.الكرة.ص > الارتفاع:
        أرجع البداية()
    أرجع ح

دالة ارسم(الوضع: وضع_اللعبة) -> Picture:
    أرجع draw.group([
        draw.rect(x: 0.0، y: 0.0، width: العرض، height: الارتفاع، color: .Black)،
        draw.circle(x: الوضع.الكرة.س، y: الوضع.الكرة.ص، radius: 8.0، color: .White)،
        draw.rect(x: الوضع.المضرب، y: الارتفاع - 12.0، width: 80.0، height: 12.0، color: .White)،
        draw.text("Score: {الوضع.النقاط}"، x: 10.0، y: 24.0، color: .White)،
    ])

دالة رئيسي(العالم: عالم):
    game.run(العالم.screen(width: 640، height: 480)، init: البداية()، update: حدث، view: ارسم)
```
```kalam
# bounce.fr.kl  [converter + glossary]
hat std/game.{nafso, Input, Picture, draw}

sagel kora:
    x: kasri
    y: kasri
    sor3et_x: kasri
    sor3et_y: kasri

sagel halet_el_le3ba:
    el_kora: kora
    el_madrab: kasri
    el_nekat: adad

khali el_ard: kasri = 640.0
khali el_ertefa3: kasri = 480.0

dala el_bedaya() -> halet_el_le3ba:
    khali el_kora = kora(x: 320.0, y: 100.0, sor3et_x: 180.0, sor3et_y: 220.0)
    rag3 halet_el_le3ba(el_kora: el_kora, el_madrab: 280.0, el_nekat: 0)

dala haddes(el_hala: halet_el_le3ba, el_madkhal: Input, el_zaman: kasri) -> halet_el_le3ba:
    motghayer h = el_hala
    law el_madkhal.is_down(.Left):
        h.el_madrab -= 400.0 * el_zaman
    law el_madkhal.is_down(.Right):
        h.el_madrab += 400.0 * el_zaman
    h.el_kora.x += h.el_kora.sor3et_x * el_zaman
    h.el_kora.y += h.el_kora.sor3et_y * el_zaman
    law h.el_kora.x < 0.0 aw h.el_kora.x > el_ard:
        h.el_kora.sor3et_x = -h.el_kora.sor3et_x
    law h.el_kora.y < 0.0:
        h.el_kora.sor3et_y = -h.el_kora.sor3et_y
    khali orayeb_mel_ard = h.el_kora.y > el_ertefa3 - 20.0
    khali fo2_el_madrab = h.el_kora.x > h.el_madrab we h.el_kora.x < h.el_madrab + 80.0
    law orayeb_mel_ard we fo2_el_madrab we h.el_kora.sor3et_y > 0.0:
        h.el_kora.sor3et_y = -h.el_kora.sor3et_y
        h.el_nekat += 1
    law h.el_kora.y > el_ertefa3:
        rag3 el_bedaya()
    rag3 h

dala ersem(el_hala: halet_el_le3ba) -> Picture:
    rag3 draw.group([
        draw.rect(x: 0.0, y: 0.0, width: el_ard, height: el_ertefa3, color: .Black),
        draw.circle(x: el_hala.el_kora.x, y: el_hala.el_kora.y, radius: 8.0, color: .White),
        draw.rect(x: el_hala.el_madrab, y: el_ertefa3 - 12.0, width: 80.0, height: 12.0, color: .White),
        draw.text("Score: {el_hala.el_nekat}", x: 10.0, y: 24.0, color: .White),
    ])

dala ra2isi(el_donya: donya):
    game.run(el_donya.screen(width: 640, height: 480), init: el_bedaya(), update: haddes, view: ersem)
```
*Notes:*
- `update` and `view` are passed to `@sync` parameters. If either one called a database or a timer, K1401 would name the offending call chain.
- `s.ball.x += …` is value-semantic rebinding (§9.4). The caller's `state` is never mutated.
- The glossary output above may exceed 100 columns on some lines. `kalam fmt` would wrap those argument lists, and the wrapping is deterministic per skin.

---

## 24. Open questions

1. **Franco and Arabic vocabulary validation.** The canonical spellings in §6 (for example `motghayer`, `naffez`, `sefa`, `ma2fool`, `ashri`, `donya`) need the survey of 200+ Egyptian developers that report 02 §9 recommends, plus a linguist review of the Arabic confusable table (`04 §7 Q3`).
2. **`Int` = 53-bit safe range.** This is cheap on JS and portable, but it is unusual. Should the C backend also offer a native `Int64`, and should `Int` widen at 1.0? The decision must be final before 1.0.
3. **Glossary mode in teams.** Should Arabic-first teams commit glossary-translated source (AST hash differs from the English original), or always commit canonical English and view other skins only? This affects code review and `05 P1`.
4. **Exception E1 (`و`/`أو`).** Accept it with lint L0114, or pick a less confusable word for *or*? This needs a small user study.
5. **Stdlib alias packs.** Should `Request`, `.map`, `Env` and others get official Arabic and Franco names (`02 §9 Q2`)? This draft says no for v0.1, to keep APIs greppable and docs single-sourced.
6. **Effect polymorphism scope.** Is "function types in data are suspending by default" too pessimistic for performance-sensitive code? Would an explicit effect variable on generic records be worth its complexity?
7. **Capability ergonomics.** Is an `App` record passed through `Request[A]` enough, or do larger apps need a dependency-injection convention? Per-function capability manifests (Austral-style, `04 §7 Q2`) were rejected as too heavy for v0.1.
8. **Multi-line lambda "islands" (§4.5 rule 4).** This is the most complex lexer rule. A trailing-block syntax is the alternative. A prototype and LLM-eval measurement should decide.
9. **Record field privacy granularity.** Only whole-record `opaque` exists. Is per-field `export` needed?
10. **Client islands and `k-*` directives.** Should the server-driven directive set be language-blessed (checked inside `html"…"`) or library-defined?
11. **Arabic punctuation default.** `،` and `؟` (default) versus ASCII. Measure keyboard-switch cost against readability with real users.
12. **Must the English skin reject `def`/`elif` etc. as errors, or accept them as hidden aliases** to raise first-try LLM pass rates? This draft chooses errors with fixes, to keep canonical code canonical. Re-measure with `05 D3` self-repair rates.
