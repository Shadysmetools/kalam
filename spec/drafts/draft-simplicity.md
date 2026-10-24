# Kalam (كلام) v0.1 — Draft Language Specification

## Angle: simplicity and learnability first

| | |
|---|---|
| **Status** | Draft for comparison with other v0.1 drafts. Not normative until the founder approves it. |
| **Date** | 2026-10-24 |
| **Inputs** | `DECISIONS.md` (D1–D17, all respected) and research reports 01–06 |
| **Resolves** | **D7** (Arabic RTL and digits), in §2.7 |
| **Angle** | Smallest keyword set, fewest concepts, Elm-grade errors, and readable in all three skins. Where two designs are equally safe and expressive, this draft picks the one that needs fewer rules. |

---

## 0. Kalam on one page

```text
use web                                   # import a module

type User:                                # a record: fields are "name: Type"
    name: Text
    age: Int

type Shape:                               # an enum: variants have no colon after the name
    Circle(radius: Float)
    Square(side: Float)

func area(shape: Shape) -> Float:         # functions: parameter types required
    return match shape:                   # match is exhaustive, with no "case" keyword
        Circle(r): 3.14159 * r * r
        Square(s): s * s

func main(world: World):                  # capabilities arrive as plain values
    var total = 0.0                       # "var" = may be reassigned; plain "x = …" may not
    for shape in [Circle(1.0), Square(2.0)]:
        total = total + area(shape)
    print("Total area: {total}")          # string interpolation with {}
```

**Size of the language:**

| Item | Count | Report 05 D4 threshold |
|---|---|---|
| Reserved keywords per skin | **22** (21 translated + `extern`, fixed ASCII) | ≤ 40 (MVP), ≤ 35 (1.0) |
| Synonyms (more than one keyword per concept) | **0** | 0 |
| Grammar productions (§4) | **58** | ≤ 120 (MVP), ≤ 100 (1.0) |
| Distinct concepts needed for "hello web app" (§17.1) | **10** | ≤ 12 (MVP), ≤ 8 (1.0) |
| Ways to declare a type | 1 (`type`) | — |
| Ways to write a function | 2: named `func`, and a one-expression closure `func(x): …` | — |
| Ways to handle an expected error | 1: `Result` with `?` or `match` | — |

---

## 1. Design principles

Each principle is a tie-breaker for future design decisions, in priority order.

**P1. Fewer concepts beat fewer characters.** A professional should be able to read any Kalam file after one hour with the language tour. Every new feature must remove more confusion than it adds. *Test: can it be explained in one sentence without the word "except"?*

**P2. One obvious way, enforced by the formatter.** No synonyms, no optional filler words, no alternative spellings in canonical code. `kalam fmt` rewrites every accepted variant (Franco spelling aliases, Arabic hamza variants, `،` vs `,`) into one canonical form. (Report 01 §3.3, PEP 20, gofmt.)

**P3. Controlled English, not natural English.** Keywords are single, common words. Grammar is formal and LL(1)-friendly. Math is symbolic (`a + b`, `x >= 10`), not spelled out. Report 01 calls this "the uncanny valley of syntax": English-looking code makes users expect English to work. Kalam avoids that valley.

**P4. Structure in words, expressions in symbols.** Statement heads are words (`if`, `for`, `match`, `return`). Expressions use universal symbols, which are identical in all three skins.

**P5. Nothing implicit that can hurt you.** No null, no truthiness, no implicit number conversion, no exceptions, no ambient I/O, no string-to-code, and no default mutability. Each of these is a P0 security rule (report 04 §5) *and* a learnability rule: hidden rules are what beginners trip over.

**P6. The compiler is a teacher.** Every error has a plain-language title, the user's own code, a reason, a concrete fix, and a stable code. Errors are written in the reader's language (§14).

**P7. Three skins, one language.** English, Arabic (فصحى) and Egyptian Franco change only *words*: keywords, a small set of core names, digits and two punctuation marks. Grammar, word order, semantics and libraries are shared. Conversion is lossless (D6).

**P8. Borrow the ecosystem, not the quirks.** Kalam compiles to JS/TS and calls npm (D4). JS semantics (`==` coercion, `undefined` vs `null`, 2^53 surprises, prototype keys) never leak into Kalam semantics.

**P9. Case never carries meaning.** Arabic has no upper and lower case. Kalam therefore never uses letter case to tell types from values or constructors from variables. Capitalising type names in Latin-script skins is a *convention* that the formatter does not enforce. This single rule keeps the three skins truly equal.

---

## 2. Lexical structure

### 2.1 Source files and skins

- A source file is UTF-8 text. A leading BOM is ignored. Line endings may be LF or CRLF, and the formatter writes LF.
- **One file = one skin (D6).** The skin is declared by the file name, and there is no pragma:

| Skin | File name | Example |
|---|---|---|
| English | `name.kl` | `orders.kl` |
| Arabic (فصحى) | `name.ar.kl` | `orders.ar.kl` |
| Egyptian Franco | `name.fr.kl` | `orders.fr.kl` |

- The module name ignores the skin suffix, so `orders.kl` and `orders.ar.kl` both define module `orders`. Two skin files for the same module in one directory are an error (K0003). Converting a file between skins (`kalam convert --to ar orders.kl`) therefore never changes any `use` line anywhere.

### 2.2 Unicode normalization

1. The whole file must be in **NFC**. A non-NFC file is rejected (K0004), and `kalam fmt` fixes it automatically. (Report 04, U2.)
2. **Forbidden code points** anywhere in the file, including comments and string literals, as raw characters:
   - all bidi controls: U+202A–U+202E, U+2066–U+2069, **U+200E (LRM), U+200F (RLM), U+061C (ALM)**
   - zero-width and invisible characters: U+200B–U+200D, U+2060, U+FEFF (except a leading BOM), and the tag characters U+E0000–U+E007F

   Inside string literals these characters can be written only as escapes (`"\u{200F}"`). (Report 04, U1 and U9.) **This is stricter than report 02**, which allowed LRM/RLM/ALM as whitespace. This draft bans them because the formatter and editor handle display (§2.7). An invisible character that only changes display is attack surface without a semantic purpose.
3. **Tabs** are not allowed for indentation (K0010). Spaces only.

### 2.3 Identifiers

- An identifier starts with a letter (Unicode `XID_Start`) or `_`, followed by `XID_Continue` characters (UAX #31). **An identifier never starts with a digit, in any skin.** That includes Franco: write `omr` or `_3omr`, not `3omr`.
- Identifiers are **case-sensitive** for Latin-script names. Case has no meaning to the compiler (P9).
- **Single-script rule (report 04, U3):** each identifier uses one script, plus `Common` characters such as `_` and ASCII digits. `pаypal` with a Cyrillic `а` is rejected (K0021). Note that an Arabic file can still use a Latin identifier, such as the stdlib method `to_int`, and a Latin file can use an Arabic identifier. *Each* identifier must be single-script.
- **Not allowed inside identifiers:** tatweel U+0640, harakat U+064B–U+065F, U+0670, Quranic marks U+06D6–U+06ED, and ZWJ/ZWNJ (K0022). The formatter removes tatweel and harakat automatically, and the error message says so.
- **Arabic letter folding (comparison key).** Two identifiers are *the same name* if they are equal after:
  1. NFKC,
  2. folding `أ إ آ ٱ` → `ا`,
  3. folding `ى` → `ي`,
  4. folding `ة` → `ه`,
  5. folding Persian `ی` → `ي` and `ک` → `ك`.

  So `اكتب` and `أكتب` refer to the same variable, and `kalam fmt` rewrites every use to the spelling at the declaration site.
- **Collision rule:** if two *different declarations* in the same scope (or two exports of one module) have the same folding key or the same UTS #39 confusable skeleton, compilation fails (K0023), with both locations shown. Nothing is silently merged. (Report 02 §7.4, report 04 U4.)
- **One namespace.** Types, values, functions, variants and modules share a single namespace per scope. Arabic cannot tell `world: World` apart by case, so no skin may rely on that.
- **Raw identifiers.** A name in backticks, such as `` `law` ``, is always an identifier, even if it is a keyword. Humans rarely write this. It exists so that `kalam convert` can carry an English identifier named `law` into a Franco file, where `law` means *if* (§3.5).

### 2.4 Keywords and core names

There are two lists, and both are defined per skin in §3:

- **Keywords** (21 + `extern`) are reserved and cannot be used as identifiers in that skin.
- **Core names** (20) are the predeclared names nearly every program uses: `Int`, `Text`, `Some`, `Ok`, `print`, `main`, `World` and so on. They are *not* reserved words, but they **cannot be redefined or shadowed** (K0031). That guarantees that translating them between skins never changes meaning.

All other library names (`web.App`, `.to_int()`, `db.Conn`) are ordinary English identifiers in every skin (report 02 §7.3, "library APIs stay in English").

### 2.5 Literals

**Numbers.**

| Form | Example | Type |
|---|---|---|
| Decimal integer | `42`, `1_000_000` | `Int` |
| Hex / binary | `0xFF`, `0b1010` | `Int` |
| Float | `3.14`, `1.5e3`, `1e-5`, `2.0` | `Float` |
| Decimal (money) | `dec"19.99"` (a tagged literal, §11) | `Decimal` |

- **Lexing rule:** a digit *always* starts a number literal, in every skin. The lexer takes the longest run of letters, digits and `_` that begins with a digit, plus at most one `.` that is **immediately followed by a digit**, plus one `+` or `-` directly after an exponent `e`/`E` when a digit follows it, so `1..100` lexes as `1`, `..`, `100` and `3.14` and `1.5e-3` as one float each. That run must match the number grammar exactly. Otherwise it is error K0101, *"A number can't be glued to letters."* This makes `3adi` an error in every skin instead of a lexer ambiguity, and it removes the class of NashmiC bug described in report 02 §2. **No Franco keyword or alias starts with a digit** (checked by a CI test, report 05 P7).
- A float needs digits on both sides of the point: `1.0`, not `1.` or `.5`.
- **Digit systems:** ASCII `0–9` in all skins. Arabic-Indic `٠–٩` (U+0660–0669) are also accepted **in Arabic-skin files only** (§2.7). Extended Arabic-Indic (Persian) `۰–۹` are rejected everywhere with a fix-it (K0103), because they are confusable with Arabic-Indic. **One digit system per literal**: `1٢3` is K0104.
- Negative numbers are the unary minus operator applied to a literal.

**Text.**

- `"…"` is a single-line text literal, and `"""…"""` is a multi-line one. A multi-line literal strips the indentation of the closing `"""` from every line (Swift rule), so it can be indented with the code.
- Escapes: `\n \t \\ \" \{ \}` and `\u{XXXX}`. There is no other escape syntax.
- **Interpolation:** `"Hello {name}, you have {count + 1} messages"`. Any expression of a type with *Show* (§5.6) may appear inside `{}`. A literal brace is written `\{`.
- **Tagged literals:** a tag written directly before the opening quote, with no space: `sql"…"`, `html"…"`, `sh"…"`, `url"…"`, `dec"…"`. Tags are ASCII and **identical in every skin** (report 04 L13). See §11.

**Booleans:** the keywords `true` / `false` (per skin, §3).

### 2.6 Comments, indentation and lines

- `#` starts a comment that runs to the end of the line. `##` starts a doc comment attached to the next declaration. There are no block comments (one way).
- **Indentation (D15)** is 4 spaces per level, enforced by the formatter. The lexer emits `NEWLINE`, `INDENT` and `DEDENT` tokens with Python's algorithm.
- **Inside `( )`, `[ ]` and `{ }`, newlines and indentation are ignored**, so long calls and literals can span lines. Because a closure body is a single expression (§9.3), **a block can never appear inside brackets**, so this rule has no exceptions.
- There are no semicolons and no backslash line continuation.
- A block opens with `:`. **Every** construct that has a body uses `:` (functions, `if`, `else`, `for`, `while`, `match` and its arms, `type`, `test`). That gives one rule to learn.

### 2.7 Arabic text direction, digits and punctuation (resolves D7)

**Decision D7 (proposed):**

1. **Logical order on disk.** Source text is always stored in logical (typing) order, exactly as Unicode intends. There are no visual-order files and no direction markers in the file (they are banned, §2.2).
2. **Base direction is set by the skin.** Tools display `.ar.kl` files with an **RTL base paragraph direction** and `.kl` / `.fr.kl` files with LTR. Kalam's own tools (formatter preview, LSP and VS Code extension, playground, `kalam diff`) render each line following **UTS #55**:
   - each token is isolated, so operators and digits keep their logical neighbours;
   - mirrored brackets are shown correctly;
   - a "bidi skeleton" warning (`kalam check --bidi`) flags any line whose visual order could mislead a reader.

   GitHub and other generic viewers will show the text correctly in most lines. Review and CI tooling can always render the canonical **English view of the same AST** (`kalam diff --skin en`), which is never ambiguous.
3. **Digits.** ASCII digits are the default in every skin, because Egyptian developers already write prices, phone numbers and code with them (report 02 §6.3). Arabic-skin files *may* use Arabic-Indic digits `٠–٩` instead. This is a **printer setting**, not part of the program: `kalam.toml` → `[skin.ar] digits = "western" | "arabic"` (default `"western"`). The formatter writes every literal in an Arabic file with the chosen system. The AST stores only numeric values, so conversion stays lossless (report 05 P1–P3). Persian digits are never accepted.
4. **Decimal separator.** It is `.` in all skins. With `digits = "arabic"`, the Arabic decimal separator `٫` (U+066B) is accepted and printed instead (`٣٫١٤`). The Arabic comma `،` is **never** a decimal separator.
5. **Punctuation aliases (Arabic skin only).** `،` (U+060C) is accepted for `,` and `؟` (U+061F) for the `?` operator. The Arabic printer writes `،` and `؟`, because they read naturally in an RTL line. All other punctuation and all operators are ASCII in every skin. Text inside string literals is never altered.
6. **Terminal output.** Most terminals do no bidi or shaping (report 02 §6.4). The CLI therefore prints diagnostics in the *user's* message language (`KALAM_LANG=en|ar|fr`, default from the OS locale). When it prints Arabic code in a terminal that does not declare bidi support, it adds the same snippet in the English view underneath. The LSP always shows full Arabic diagnostics.

This is the smallest rule set that is both safe and comfortable. It needs one setting (digits) and two punctuation aliases. Everything else is a pure display concern handled by tools.

---

## 3. The three skins

### 3.1 Policy

1. **Same grammar, same order.** A skin changes only lexemes: keywords, core names, digit glyphs and the two Arabic punctuation aliases. No skin may reorder a production (report 02 §7.1).
2. **One canonical spelling per keyword per skin.** That spelling is what the formatter writes.
3. **Aliases policy:**
   - **English:** no aliases.
   - **Arabic:** no aliases. Instead, the orthographic folding of §2.3 (hamza and alef forms, ى/ي, ة/ه, Persian letters) applies to keywords too, and tatweel and harakat are tolerated inside *keywords* and stripped by `fmt`. So `اذا`, `إذا` and `إذا` with harakat all match.
   - **Franco:** each keyword has a **closed, versioned alias list** of common spelling variants (vowel changes, `kh`/`5`, `gh`/`8`, `sh`/`4` *inside* a word). The lexer accepts them, and `fmt` rewrites them to canonical spelling. **No alias starts with a digit.**
   - **No skin accepts another skin's keywords.** If an English keyword appears in an Arabic or Franco file, compilation fails with K0201, which includes an automatic fix. The LSP applies it as you type, and pasting English code into an Arabic file converts it on paste. This resolves the disagreement between report 02 ("accept English everywhere") and report 04 ("reject mixing"). D6 is locked as *one file = one skin*, so files on disk are never mixed, but copy-paste friction is removed by tooling.
4. **Fuzzy matching only in diagnostics.** "Did you mean `walla`?" uses Damerau-Levenshtein ≤ 2 after Franco folding (`5→kh`, `7→h`, `3→a`, `2→a`, `8→gh`, `4→sh`, vowels removed). It never silently accepts input.
5. **Fixed ASCII in every skin (report 04, L13):**
   - the keyword `extern`
   - tag prefixes `sql`, `html`, `sh`, `url`, `dec`
   - every stdlib name that starts with `unsafe_`

   These are deliberately identical across skins, so `grep` and audit tools find every escape hatch in every skin.

### 3.2 Keyword table (21 + `extern`)

Concepts are listed in the order they are taught in the tour.

| # | Concept | English | Arabic (canonical) | Franco (canonical) | Franco aliases (closed list) |
|---|---|---|---|---|---|
| 1 | define a function | `func` | `دالة` | `dala` | `dalla`, `dalah` |
| 2 | return a value | `return` | `أرجع` | `rag3` | `ragga3`, `raga3`, `rg3` |
| 3 | reassignable variable | `var` | `متغير` | `motaghayer` | `mot8ayar`, `motghayer`, `mota8ayer` |
| 4 | condition | `if` | `إذا` | `law` | `lw`, `lau` |
| 5 | otherwise | `else` | `وإلا` | `walla` | `wala`, `wla` |
| 6 | pattern match | `match` | `طابق` | `hasab` | `hasb`, `7asab` ✗ *(rejected: starts with digit)* |
| 7 | loop over items | `for` | `لكل` | `lekol` | `lkol`, `likol`, `le_kol` |
| 8 | "in" (only in `for`) | `in` | `في` | `fi` | `fe`, `fy` |
| 9 | loop while true | `while` | `طالما` | `tool_ma` | `tol_ma`, `tolma`, `toul_ma` |
| 10 | leave the loop | `break` | `اخرج` | `kfaya` | `kefaya`, `khalas` |
| 11 | next iteration | `continue` | `تابع` | `kammel` | `kamel`, `kml` |
| 12 | define a type | `type` | `نوع` | `no3` | `noo3`, `naw3` |
| 13 | import a module | `use` | `استخدم` | `hat` | `hatli`, `hatly` |
| 14 | make public | `export` | `صدّر` | `talla3` | `tala3`, `tl3` |
| 15 | logical and | `and` | `و` | `we` | `wi` |
| 16 | logical or | `or` | `أو` | `aw` | `ao` |
| 17 | logical not | `not` | `ليس` | `mesh` | `msh`, `mish` |
| 18 | pattern test | `is` | `هو` | `howa` | `hwa`, `huwa` |
| 19 | true | `true` | `صواب` | `sa7` | `sah` |
| 20 | false | `false` | `خطأ` | `ghalat` | `ghalt`, `8alat` ✗ *(rejected: starts with digit)* |
| 21 | test block | `test` | `اختبار` | `ekhtebar` | `ekhtbar`, `e5tebar` |
| 22 | foreign declaration | `extern` | `extern` | `extern` | none (fixed ASCII) |

That is **22 reserved words per skin: 21 translated keywords plus `extern`.** Python has 35 and Lua 22.

✗ marks spellings that look natural but are rejected by rule. They are listed so that the "did you mean" diagnostic knows them and can say: *"Names can't start with a digit. Did you mean `hasab`?"*

**Words deliberately *not* reserved:**
- `case` — arms of `match` need no keyword.
- `let` and `const` — a plain binding is already constant.
- `class`, `struct`, `enum`, `interface` — one `type` keyword covers all of these.
- `try`, `catch`, `throw` — errors are values.
- `async`, `await` — D12.
- `null`, `none` — use `None`, a core name.
- `self` — there are no classes.
- `as`, `from` — there are no import aliases.
- `print`, `main` — core names, not keywords.

### 3.3 Core names table (20)

These names are predeclared in every file, translated by skin, and cannot be redefined (K0031).

| # | Meaning | English | Arabic | Franco |
|---|---|---|---|---|
| 1 | whole number type | `Int` | `عدد` | `adad` |
| 2 | floating-point type | `Float` | `كسري` | `kasri` |
| 3 | exact decimal type | `Decimal` | `عشري` | `ashri` |
| 4 | text type | `Text` | `نص` | `nass` |
| 5 | boolean type | `Bool` | `منطقي` | `mantiqi` |
| 6 | the "no value" type and its value | `Nothing` | `فراغ` | `fady` |
| 7 | list type | `List` | `قائمة` | `qayma` |
| 8 | map type | `Map` | `قاموس` | `qamoos` |
| 9 | optional type | `Option` | `اختياري` | `ekhtiari` |
| 10 | Option: has a value | `Some` | `موجود` | `mawgood` |
| 11 | Option: no value | `None` | `عدم` | `mafeesh` |
| 12 | result type | `Result` | `نتيجة` | `natiga` |
| 13 | Result: success | `Ok` | `نجاح` | `nagah` |
| 14 | Result: failure | `Err` | `فشل` | `fashal` |
| 15 | general error type | `Error` | `مشكلة` | `moshkela` |
| 16 | print a line | `print` | `اطبع` | `ektb` |
| 17 | stop on a bug | `panic` | `أوقف` | `wa2af` |
| 18 | test assertion | `expect` | `يجب` | `lazem` |
| 19 | program entry point | `main` | `بداية` | `bedaya` |
| 20 | root capability type | `World` | `عالم` | `donia` |

Notes:
- `خطأ` ("wrong") is the Arabic **`false`**. The error *types* use different words (`فشل` = failure, `مشكلة` = problem), so that no two entries fold to the same key (checked by CI, report 05 P7).
- `عدد` (Int) and `عدم` (None) differ by one letter. They are distinct after folding, but the LSP highlights core names in a distinct colour to help readers.
- The stdlib types `Html`, `Sql`, `Command`, `Url`, `Secret` and `Request`, and all module names, stay English in every skin (§2.4).

### 3.4 Formatter-printed punctuation per skin

| Token | English | Arabic | Franco |
|---|---|---|---|
| argument separator | `,` | `،` (`,` accepted) | `,` |
| error propagation | `?` | `؟` (`?` accepted) | `?` |
| decimal point | `.` | `.` or `٫` (follows the digits setting) | `.` |
| everything else | ASCII | ASCII | ASCII |

### 3.5 Lossless conversion

`kalam convert --to <skin>` works on the **lossless CST** (report 02 §7.2):

- **Changes:** keywords, core names, digit glyphs (Arabic target only, per the setting), and `،`/`؟`.
- **Never changes:** user identifiers, string contents, comments, blank lines, module names or library names. An optional **glossary file** (`names.ar.toml`, `names.fr.toml`) can map project identifiers for teaching or for a localized codebase. Glossary translation is a separate, explicit step (`--glossary`).
- **Escaping:** if a user identifier equals a keyword **or a core name** of the target skin, the converter writes it as a raw identifier (`` `law` ``) and reports it. For example, an English variable `adad` becomes `` `adad` `` in Franco. Converting back removes the backticks if they are no longer needed.
- **Round-trip guarantee (report 05 P1–P3):** `convert(convert(fmt(A), S), skin(A)) == fmt(A)` byte-for-byte, and all three skins produce an identical canonical AST hash.

> **About the examples in this document.** Keywords, core names and punctuation in the Arabic and Franco versions are exactly what `kalam convert` produces. **User identifiers and string contents were translated by hand (as a glossary would)** so that each version reads naturally. Without a glossary, the converter would leave `area`, `room` and `"Kitchen"` in English.

---

## 4. Grammar (EBNF)

**Conventions:**
- Terminals in quotes are the **English-skin** lexemes. Other skins substitute their table entries, so the grammar itself is skin-free.
- `NEWLINE`, `INDENT` and `DEDENT` come from the indentation lexer (§2.6).
- `IDENT`, `INT`, `FLOAT`, `STRING` and `TAG` are lexer tokens. A `TAG` is one of `sql html sh url dec` and is immediately followed by a `STRING`.
- `{ x }` means zero or more, `[ x ]` means optional, and `|` means alternative.

```ebnf
(* ---------- files and declarations ---------- *)
 1 file          = { NEWLINE | top_decl } EOF ;
 2 top_decl      = use_decl | [ "export" ] decl | test_decl ;
 3 decl          = func_decl | type_decl | const_decl | extern_decl ;
 4 use_decl      = "use" module_path [ "." "{" IDENT { "," IDENT } "}" ] NEWLINE ;
 5 module_path   = IDENT { "/" IDENT } ;
 6 const_decl    = IDENT [ ":" type ] "=" expr NEWLINE ;
 7 func_decl     = "func" IDENT [ type_params ] "(" [ params ] ")" [ "->" type ] ":" func_body ;
 8 func_body     = expr NEWLINE | NEWLINE INDENT { stmt } DEDENT ;
 9 type_params   = "[" IDENT { "," IDENT } "]" ;
10 params        = param { "," param } [ "," ] ;
11 param         = IDENT ":" type ;
12 type_decl     = "type" IDENT [ type_params ] ":" NEWLINE INDENT ( fields | variants ) DEDENT ;
13 fields        = field { field } ;
14 field         = IDENT ":" type [ "=" expr ] NEWLINE ;
15 variants      = variant { variant } ;
16 variant       = IDENT [ "(" params ")" ] NEWLINE ;
17 test_decl     = "test" STRING ":" block ;
18 extern_decl   = "extern" STRING ( extern_func | extern_type ) NEWLINE ;
19 extern_func   = "func" IDENT [ type_params ] "(" [ params ] ")" [ "->" type ] ;
20 extern_type   = "type" IDENT [ type_params ] ;

(* ---------- types ---------- *)
21 type          = named_type | func_type ;
22 named_type    = IDENT { "." IDENT } [ "[" type { "," type } "]" ] ;
23 func_type     = "func" "(" [ type { "," type } ] ")" "->" type ;

(* ---------- statements ---------- *)
24 block         = simple_stmt NEWLINE | NEWLINE INDENT stmt { stmt } DEDENT ;
25 stmt          = simple_stmt NEWLINE | compound_stmt ;
26 simple_stmt   = binding | return_stmt | "break" | "continue" | expr ;
27 binding       = [ "var" ] IDENT [ ":" type ] "=" ( expr | match_expr ) ;
28 return_stmt   = "return" [ expr | match_expr ] ;
29 compound_stmt = if_stmt | match_stmt | for_stmt | while_stmt | func_decl ;
30 if_stmt       = "if" cond ":" block { "else" "if" cond ":" block } [ "else" ":" block ] ;
31 while_stmt    = "while" cond ":" block ;
32 for_stmt      = "for" pattern "in" expr ":" block ;
33 match_stmt    = "match" expr ":" NEWLINE INDENT arm { arm } DEDENT ;
34 match_expr    = "match" expr ":" NEWLINE INDENT expr_arm { expr_arm } DEDENT ;
35 arm           = pattern [ "if" expr ] ":" block ;
36 expr_arm      = pattern [ "if" expr ] ":" expr NEWLINE ;
37 cond          = expr ;                (* may contain "is" bindings; see §8.4 *)

(* ---------- expressions (lowest to highest precedence) ---------- *)
38 expr          = if_expr | lambda | or_expr ;
39 if_expr       = "if" expr ":" expr "else" ":" expr ;
40 lambda        = "func" "(" [ lparam { "," lparam } ] ")" ":" expr ;
41 lparam        = IDENT [ ":" type ] ;
42 or_expr       = and_expr { "or" and_expr } ;
43 and_expr      = not_expr { "and" not_expr } ;
44 not_expr      = "not" not_expr | is_expr ;
45 is_expr       = cmp_expr [ "is" pattern ] ;
46 cmp_expr      = range_expr [ cmp_op range_expr ] ;     (* non-associative *)
47 cmp_op        = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
48 range_expr    = add_expr [ ".." add_expr ] ;
49 add_expr      = mul_expr { ( "+" | "-" ) mul_expr } ;
50 mul_expr      = unary { ( "*" | "/" | "//" | "%" ) unary } ;
51 unary         = "-" unary | postfix ;
52 postfix       = primary { "." IDENT | [ "[" type { "," type } "]" ] call_args | "?" } ;
53 call_args     = "(" [ arg { "," arg } [ "," ] ] ")" ;
54 arg           = [ IDENT ":" ] expr ;
55 primary       = INT | FLOAT | STRING | TAG STRING | "true" | "false" | IDENT
                | "(" expr ")" | "[" [ expr { "," expr } [ "," ] ] "]"
                | "{" [ expr ":" expr { "," expr ":" expr } [ "," ] ] "}" ;

(* ---------- patterns ---------- *)
56 pattern       = prim_pattern { "|" prim_pattern } ;
57 prim_pattern  = "_" | INT | "-" INT | STRING | "true" | "false"
                | IDENT [ "(" [ pat_arg { "," pat_arg } ] ")" ]
                | "[" [ pattern { "," pattern } ] [ "," ".." [ IDENT ] ] "]" ;
58 pat_arg       = [ IDENT ":" ] pattern | ".." ;
```

**58 productions.** Report 05 sets ≤ 120 for MVP and ≤ 100 for 1.0.

**Disambiguation rules (the only ones):**
1. **Binding or assignment?** `x = e` *declares* `x` if no `x` is in scope. It *reassigns* `x` if `x` is a `var` in scope. If `x` is an immutable binding in scope, it is K2101: *"`x` is already defined and can't change. Did you mean `var x`?"* Shadowing in the same function is not allowed (one name, one meaning).
2. **Constructor or new variable in a pattern?** A bare `IDENT` in a pattern is a **variant** if a variant with that name is in scope, and otherwise a **new binding**. Case is never used (P9). The compiler protects against typos: if a binding arm catches everything and later arms become unreachable, that is K2402, *"This name catches every value, so the arms below can never run. Did you mean the variant `Circle`?"*
3. **Record or enum?** In a `type` body, lines of the form `name: Type` make a record, and lines of the form `Name` or `Name(…)` make an enum. Mixing the two is K2201.
4. `[T]` after an expression and before `(` is an explicit type argument (`decode[User](text)`). There is no indexing syntax (§6.4), so `[` after an expression has no other meaning.
5. `if … : … else : …` on one line is an **expression**. With a block on the next line, it is a **statement**. `match` is an expression only directly after `=` or `return`, and then every arm is a single expression.

---

## 5. Type system

Static, strong, nominal for user types, and inferred inside function bodies (D5). **No implicit conversions of any kind** (report 04, L3).

### 5.1 Primitive types

| Type | Values | JS representation | Notes |
|---|---|---|---|
| `Int` | whole numbers in **[−(2^53−1), 2^53−1]** | `number` | **Overflow panics on every backend** (L7). See §5.8. |
| `Float` | IEEE-754 binary64 | `number` | `NaN` and infinities exist. `==` on `Float` follows IEEE. |
| `Decimal` | exact base-10, 28 significant digits | runtime class | For money. Built from `dec"19.99"` or `Decimal(n)`. |
| `Bool` | `true`, `false` | `boolean` | The *only* type `if`/`while`/`and`/`or`/`not` accept (L4). |
| `Text` | immutable Unicode text | `string` | `.length` counts **grapheme clusters**, which is right for Arabic. `.bytes()` gives UTF-8. |
| `Nothing` | one value, `Nothing` | `undefined` | The return type of functions with no useful result. |

**Arithmetic rules:**
- `+ - *` need both sides to have the same numeric type. `1 + 2.0` is K2001 with a fix-it (`Float(1) + 2.0`).
- **`/` is defined only on `Float` and `Decimal`.** `//` (floor division) and `%` are defined only on `Int`. So `7 / 2` is K2002: *"`/` makes a Float. For whole-number division use `7 // 2` (= 3)."* This removes the classic "why is 7/2 = 3" surprise from both directions.
- Integer division or `%` by zero panics.
- `+` also joins `Text` with `Text` and `List[T]` with `List[T]`. Interpolation is usually clearer, and the linter suggests it.
- **Conversions are explicit functions:** `Float(i)`, `Int.from_float(f)` (returns `Option[Int]`, which is `None` for fractions, NaN or out of range), `f.round()`, `f.floor()`, `Text(x)`, `"42".to_int()` (returns `Option[Int]`).

### 5.2 Records

```text
type User:
    name: Text
    email: Text
    active: Bool = true        # a field may have a default
```

- Construction always names the fields: `User(name: "Mona", email: "m@x.io")`. Fields with defaults may be omitted. A record with **exactly one field** may be constructed positionally, as in `UserId(42)`. Field order in a call does not matter, and the formatter sorts fields into declaration order.
- Access is `user.name`. **Records are immutable.** A modified copy is written `user.with(active: false)`. `.with(...)` is provided for every record and is not a keyword.
- Equality is structural (§5.6). Records are **nominal**: two record types with the same fields are different types.

### 5.3 Enums (sum types)

```text
type Payment:
    Cash
    Card(last4: Text)
    Wallet(provider: Text, phone: Text)
```

- A variant is either bare (`Cash`) or carries named fields. Construction uses the same rules as records (`Card("4242")`, `Wallet(provider: "Paymob", phone: p)`).
- Variants are in scope wherever the type is in scope. If two imported types share a variant name, write it qualified (`Payment.Cash`).
- `match` must be exhaustive (§8, L8).

### 5.4 Generics

- Types and functions take type parameters in square brackets: `type Page[T]:`, `func first[T](items: List[T]) -> Option[T]:`.
- Type arguments are inferred at call sites. They can be given explicitly with `f[User](…)` when inference has nothing to go on, for example in decoding.
- **There are no user-defined traits, interfaces or type classes in v0.1** (§17). Instead, each type parameter may use the built-in abilities of §5.6, and the compiler checks that each use is valid:
  - `T` used with `==` requires **Eq** at the call site.
  - `T` used with `<` or `sort` requires **Ord**.
  - `T` used in `"{x}"` requires **Show**.
  - `T` used with `json.encode` / `decode` requires **Json**.

  The ability is inferred from the function body and shown in the signature by the LSP as a hint, such as `func max[T: Ord]`. In v0.1 it is never written by hand. Writing `[T: Ord]` is allowed and checked, but optional.
- `Map[K, V]` requires `K` to have **Eq** and **Hash**. Every type that has Eq has Hash.

### 5.5 Option and Result

```text
type Option[T]:
    Some(value: T)
    None

type Result[T, E]:
    Ok(value: T)
    Err(error: E)
```

- **There is no null (L5).** "Maybe absent" is always `Option[T]`.
- `Result[T, E]` is for **expected** failures (D16). Ignoring a `Result` is a compile error (L6): a call returning `Result` used as a statement is K2301, *"This can fail, and the failure is being ignored. Handle it with `?`, `match`, or write `_ = …` to discard it on purpose."*
- Both have a small, fixed set of helper methods in the prelude:
  - `.value_or(default)`, `.map(f)`, `.ok_or(err)` and `.is_some()` on Option
  - `.map(f)`, `.map_err(f)`, `.ok()` and `.is_ok()` on Result

### 5.6 Built-in abilities (instead of traits)

These are derived automatically by the compiler. Users cannot implement them by hand in v0.1.

| Ability | Provided for | Meaning |
|---|---|---|
| **Eq** | every type except function types and `extern type`s | Structural `==` / `!=`. Comparing two *different* types is a compile error (L4). |
| **Ord** | `Int`, `Float`, `Decimal`, `Text` (Unicode code-point order), and `List`/`Option` of Ord types | `< <= > >=`, `.sort()`, `max`, `min`. To sort records, use `.sort_by(func(u): u.age)`. |
| **Hash** | every Eq type | Usable as a `Map` key. Hashing is keyed SipHash (S14). |
| **Show** | every type | Used by `print` and `"{x}"`. Records show as `User(name: "Mona", …)`. **`Secret[T]` always shows `[REDACTED]`** (S5). |
| **Json** | records, enums, primitives, `List`, `Map[Text, _]`, `Option` | `json.encode(x)` and `json.decode[T](text) -> Result[T, json.Error]`. Decoding is **schema-first** (S4): unknown fields are rejected (S10), size, depth and count limits are on by default, and `Secret` encodes as `"[REDACTED]"` unless `.expose()` is called. |

### 5.7 Inference rules

Bidirectional, local type inference with a simple, predictable boundary:

1. **Named function parameters must be annotated.** Return types are inferred for private functions and **required for `export` functions**. Signatures are where the compiler explains errors, and where readers look first.
2. **Bindings are inferred** from their right-hand side (`total = 0.0` makes `total` a `Float`). An annotation (`total: Decimal = dec"0"`) is allowed and checked.
3. **Closures are inferred from the expected type** at the call site (`items.map(func(x): x * 2)`). Annotating closure parameters is optional.
4. **Literals have one type each:** `1` is `Int` and `1.0` is `Float`. There is no literal polymorphism, which keeps error messages simple.
5. **Empty collections** (`[]`, `{}`) take their element type from later use or from the expected type. If neither exists, it is K2010: *"I can't tell what this list will hold. Add a type, for example `names: List[Text] = []`."*
6. **Generalization** happens only at named `func` declarations, never at bindings, so a binding always has exactly one type.
7. **Top-level constants** (`limit = 100`) are inferred and must be computable without capabilities, so module setup can never do I/O (§15).

### 5.8 Why `Int` is 53-bit, and what follows from it

`Int` uses the same range on every backend: ±(2^53−1), the largest range JS `number` represents exactly. **This deliberately departs from report 03**, which suggested Int64 or BigInt. The reasons:
- One semantics on every backend, so the future C backend traps at the same bound (report 05, P5 behavioural parity).
- Full speed on JS, with no BigInt in hot loops.
- Every value that fits stays exact, and overflow is a **panic, never silent precision loss**.

Consequences, which are part of the spec:
- **Decoding** a JSON number, a Postgres `bigint`, or an npm value above 2^53−1 into `Int` produces a decode `Err`, never a rounded value.
- IDs that may exceed 2^53 (Twitter-style snowflakes, some payment IDs) should be modelled as `Text`. A stdlib `BigInt` type (arbitrary precision, slower) exists for arithmetic on them.
- Wrapping or saturating arithmetic is explicit (`a.wrapping_add(b)`, `a.saturating_add(b)`) and is meant for games and hashing.

---

## 6. Expressions

### 6.1 Operators and precedence (high to low)

| Level | Operators | Associativity |
|---|---|---|
| 1 | `.field`, `.method(…)`, call `f(…)`, `?` | left |
| 2 | unary `-` | right |
| 3 | `*` `/` `//` `%` | left |
| 4 | `+` `-` | left |
| 5 | `..` (inclusive range) | none |
| 6 | `==` `!=` `<` `<=` `>` `>=` | **none**: `a < b < c` is K2005 with a fix-it `a < b and b < c` |
| 7 | `is` (pattern test) | none |
| 8 | `not` | right |
| 9 | `and` | left |
| 10 | `or` | left |

`and`/`or` short-circuit and accept only `Bool`. Mixing `and` and `or` without parentheses is allowed, but the formatter adds parentheses to make the grouping visible.

### 6.2 Method-call syntax (uniform function call)

**`a.f(b, c)` means `f(a, b, c)`.** The compiler looks for `f` in this order:
1. a field of `a` holding a function,
2. the module that defines the type of `a`,
3. the current scope.

This gives readable chains such as `text.trim().to_int()` **without classes, methods or `self`**: a "method" is just a function whose first parameter is that type, defined in the same module.

### 6.3 Ranges

`a..b` is the inclusive range from `a` to `b`, so `1..100` is 1 through 100. There is one range form. For index loops, use `items.indices()` (0 to length−1). The linter flags `0..items.length` as a probable off-by-one and suggests `items.indices()`.

### 6.4 Collections

- `List[T]` literals: `[1, 2, 3]`. `Map[K, V]` literals: `{"cairo": 10, "giza": 4}`.
- **There is no `xs[i]` indexing syntax.** `xs.get(i)` and `m.get(k)` return `Option`. This keeps out-of-bounds panics out of normal code and frees `[ ]` to mean only "type arguments" and "list". Panicking access is still available, explicitly, as `xs.at(i)`.
- Collections are immutable values. `xs.add(x)`, `m.set(k, v)` and `xs.remove_at(i)` return new collections, and the runtime uses persistent or copy-on-write structures. Mutable buffers for performance-critical code (games) live in the stdlib (`buffer.Floats`) behind a clearly named API.

### 6.5 `if` and `match` as expressions

```text
label = if count == 1: "item" else: "items"
price = match tier:
    Free: dec"0"
    Pro: dec"9.99"
    Team(seats: n): dec"7.99" * Decimal(n)
```

In expression position, every branch is a single expression. For anything longer, use the statement form, or call a named function.

---

## 7. Statements

| Statement | Form |
|---|---|
| Immutable binding | `name = expr` |
| Reassignable binding | `var name = expr`, then `name = expr` |
| Expression statement | `print("hi")` (a `Result` here must be handled, §5.5) |
| Return | `return expr` (or bare `return` in a function returning `Nothing`) |
| If | `if cond:` … `else if cond:` … `else:` … |
| Match | `match value:` with indented arms (§8) |
| For | `for pattern in iterable:` |
| While | `while cond:` |
| Loop control | `break`, `continue` (innermost loop only, no labels) |
| Nested function | a `func` inside a block (for multi-line helpers, §9.3) |

- **Functions with a block body must use `return`** to produce a value. A one-line body (`func double(x: Int) -> Int: x * 2`) returns its expression. These are the only two forms.
- A block body that reaches its end without `return` in a function whose return type is not `Nothing` is K2303, which shows the path that falls through.
- Top-level `var` is not allowed (K2104). There is **no global mutable state**.

---

## 8. Pattern matching

### 8.1 Forms

| Pattern | Matches | Example |
|---|---|---|
| `_` | anything, binds nothing | `_: "other"` |
| literal | equal `Int`, `Text`, `Bool` | `0: "zero"` |
| name | anything, binds it (unless the name is a variant, §4 rule 2) | `n: n * 2` |
| variant | that variant, with nested patterns | `Some(user)`, `Card(last4: l)` |
| alternatives | any of several | `Sat \| Sun: "weekend"` |
| list | exact shape, with an optional rest | `[]`, `[x]`, `[first, ..rest]` |
| guard | pattern plus condition | `Some(n) if n > 0: …` |

Patterns for variants with several fields use field names (`Rect(width: w, height: h)`) and may skip fields with `..` (`Rect(width: w, ..)`). A variant with exactly one field may be matched positionally (`Some(x)`, `Circle(r)`).

### 8.2 Exhaustiveness (L8)

A `match` must cover every possible value. A missing case is an **error** (K2401), and the message lists the missing patterns **in the file's skin**:

```text
-- MISSING CASES ------------------------------------------- shapes.kl  K2401

This `match` does not handle every kind of Shape:

 9│     return match shape:
               ^^^^^^^^^^^
I found no arm for:

    Triangle(base: _, height: _)

Add an arm for it, or add `_: …` at the end if every other case
should be treated the same.
```

A `_` arm on an enum produces a *warning* (K2403) when it hides a single variant, because adding a variant later would silently fall into it. That is the "new role falls through to allow" bug from report 04, L8.

### 8.3 `match` as a statement and as an expression

As a statement, arms may have blocks. As an expression (after `=` or `return`), every arm is a single expression (§6.5).

### 8.4 `is`: one-arm pattern tests

`value is Pattern` is a `Bool` expression. When it appears directly in an `if` or `while` condition (possibly joined with `and`), the names it binds are available in that block:

```text
if user.plan is Team(seats: n) and n > 10:
    print("Large team: {n} seats")

while queue.pop() is Some(job):
    run(job)
```

This replaces `if let`, `instanceof`, type guards and `matches!` with one keyword. Names bound by `is` under `or` or `not` are not available (K2410), because they might not exist.

---

## 9. Functions and closures

### 9.1 Declaration

```text
func greet(name: Text) -> Text:
    return "Hello, {name}"

func double(x: Int) -> Int: x * 2          # one-line body = returned expression
```

- Parameters are positional. **There are no default arguments, no variadics and no overloading** (§17). For many optional settings, take a record with defaults.
- Arguments may be **named at the call site** for clarity (`send(to: user, subject: s)`). As with records, named arguments may come in any order; the formatter sorts them into declaration order. One rule for every call.
- Functions are values: `items.map(double)`.

### 9.2 Function types

`func(Int, Int) -> Int`. Functions have no Eq (§5.6).

### 9.3 Closures are one expression

```text
adults = users.filter(func(u): u.age >= 18)
names  = adults.map(func(u): u.name)
```

- A closure is `func(params): expression`. **Closures are a single expression by design.** Multi-line logic goes into a named nested `func`, which may also capture variables:

```text
func report(orders: List[Order]) -> Text:
    func line(o: Order) -> Text:
        tax = o.total * dec"0.14"
        return "{o.id}: {o.total} + {tax} VAT"
    return orders.map(line).join("\n")
```

  This keeps the indentation lexer free of special cases (§2.6), removes the "where does the lambda end?" problem, and gives every non-trivial piece of logic a name.
- **Capture is by value.** A closure or nested function sees the value a `var` had *when the closure was created*, and **cannot assign to outer variables** (K2501). This removes shared mutable state, so the concurrency model (§13) is free of data races by construction.

### 9.4 Entry point

A program's entry point is `func main():` or `func main(world: World):`. It may return `Nothing` or `Result[Nothing, E]`. If `main` returns `Err(e)`, the program prints `e` (redacted per Show) and exits with code 1.

---

## 10. Modules and visibility

- **One file = one module.** The module name is the file name without the skin suffix (§2.1). A package is a directory with `kalam.toml`.
- **Imports:** `use web` (stdlib), `use paymob` (dependency) and `use app/billing` (a module in this package). Names are then qualified, as in `billing.charge(...)`. To use names unqualified, list them: `use app/billing.{charge, Invoice}`.
- There are **no import aliases, wildcard imports or dynamic imports** (L9). Every module is resolved at compile time.
- **Visibility:** everything is private to its module unless marked `export`. Exported functions must have full signatures (§5.7). An exported type exports its fields and variants. *(Opaque types are an open question, §20.)*
- **Import cycles** between modules are a compile error (K0601). The message shows the cycle as a list.
- **Cross-skin modules are normal.** An Arabic module can export `حساب_الضريبة`, and an English module uses it as `billing.حساب_الضريبة`. A project glossary can give it an English alias for the English view (§3.5). The AST and the package interface are skin-free.
- **Module initialisation is pure.** Top-level constants cannot use capabilities (§15), so importing a module can never perform I/O. That rules out a whole class of supply-chain "runs on import" attacks.

---

## 11. Safe string kinds: tagged literals

Injection is a type error (report 04 §3.2, S1–S3). **A sink never accepts `Text`.** The only way to build a sink value is a tagged literal written in source code. The tag is literal-only and checked at compile time. Values inside `{ }` holes are bound or escaped by context, never spliced as raw text.

| Tag | Produces | Hole handling | Sink that requires it |
|---|---|---|---|
| `sql"…"` | `Sql` | Each hole becomes a **bound parameter** (`$1`, `$2`, …). | `db.Conn.query/execute/fetch_*` |
| `html"…"` | `Html` | **Contextual auto-escaping** (Go `html/template` rules) | `web` responses and views |
| `sh"…"` | `Command` | Split into an **argv list at compile time**. Each hole is exactly one argument. **No shell is ever invoked.** | `proc.run(cap, command)` |
| `url"…"` | `Url` | Holes are percent-encoded for their URL component | `http.get`, redirects, `href`/`src` |
| `dec"…"` | `Decimal` | No holes. The text must be a valid decimal at compile time. | arithmetic |

### 11.1 Rules shared by all tags

1. `Text` never converts to `Sql`, `Html`, `Command` or `Url`. `conn.execute("DELETE … " + id)` is K3101:

```text
-- TEXT IS NOT SQL ------------------------------------------ users.kl  K3101

`execute` needs a Sql query, but this is plain Text:

14│     conn.execute("DELETE FROM users WHERE id = " + Text(id))
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
Building SQL from Text is how SQL injection happens, so Kalam does not
allow it. Write a sql"…" literal and put values in { }:

    conn.execute(sql"DELETE FROM users WHERE id = {id}")

Values in { } are sent separately from the query and can never change it.
```

2. Two literals of the same tag can be joined with `+` (`Sql + Sql`). An `Html` value inside an `html` hole is inserted as-is, and a `List[Html]` is concatenated. Composition never goes through `Text`.
3. **Escape hatches** are ordinary stdlib functions whose names start with `unsafe_`: `Html.unsafe_from_text(t)`, `Sql.unsafe_from_text(t)`. They are never localized (L13), they are counted by `kalam audit` and shown on package pages (P10), and the linter flags each use.

### 11.2 `sql` specifics

- A hole is allowed **only in a value position**: after `=`, `<`, `IN (`, `VALUES (`, `LIMIT` and so on. A hole where an identifier, keyword or table name is expected is K3102. Dynamic column names go through `sql.column(name, allowed: [...])`, which checks against an allow-list.
- The SQL dialect is configured per connection (Postgres first). The compiler uses a lightweight SQL tokenizer to find hole contexts. Full schema checking (sqlx-style, S16) is a 1.x goal.

### 11.3 `html` specifics

- The HTML **context** of each hole is decided at compile time:
  - **text:** any Show value is escaped; `Html` is inserted as-is
  - **quoted attribute value:** escaped for the attribute
  - **URL attributes** (`href`, `src`, `action`): require `Url`, or `Text` that is checked against a safe-scheme list
- **Holes are forbidden** inside `<script>`, `<style>` and any `on*` attribute (K3103). Client behaviour is attached with `send-*` attributes (§11.4) or a JS island (§16.4).
- Literals must be well-formed: tags balanced and attributes quoted (K3104). That makes them safe to diff on the server.
- Output is served with the CSP, Trusted Types and other headers of report 04 S8.

### 11.4 Server-driven UI: `send-*` attributes (D11)

The web kit implements LiveView/htmx-style components **without any new syntax**. A live component is three ordinary things:

1. a **state** type,
2. an **update** function `func(State, Msg) -> State` (or `-> Result[State, E]`; it may use capabilities passed to it), and
3. a **view** function `func(State) -> Html`.

In a view, an attribute named `send-<event>` takes a hole whose value is a **message** (a value of the component's `Msg` enum):

```text
<button send-click={Increment}>+</button>
<form send-submit={Save}> … </form>
```

At runtime:
1. The browser loads the page and a small first-party client script.
2. An event sends the message (an opaque, signed reference, never code) to the server over a WebSocket or SSE connection.
3. The server runs `update`, then `view`, diffs the new `Html` against the previous one, and sends back a minimal patch.

Form inputs bound to a message decode into **declared types only** (S10). The checker verifies that every `send-*` hole has the component's `Msg` type (K3105). See example 18.7.

### 11.5 `sh` specifics

- The **program name must be literal** (`sh"git log {branch}"` is fine; `sh"{tool} --help"` is K3106).
- Running a `Command` requires the `proc` capability (§15). There is **no** API that takes a shell string (S2).

---

## 12. Error handling (D16)

Kalam has two kinds of failure and one way to handle each.

| Kind | When | Mechanism | Can it be caught? |
|---|---|---|---|
| **Expected failure** | The world said no: bad input, missing row, network down | Return `Result[T, E]` | Handled with `?` or `match` |
| **Bug** | The program is wrong: overflow, `xs.at(99)` out of range, division by zero, a broken invariant | `panic("message")` or an automatic panic | **No.** Only the runtime boundary sees it (§12.3) |

### 12.1 The `?` operator

`expr?` on a `Result[T, E]`:
- if it is `Ok(v)`, the value is `v`;
- if it is `Err(e)`, the current function returns `Err(e)` immediately.

On an `Option[T]`, `?` returns `None` early, and is allowed only in functions that return `Option`.

**Error type rule (one rule, no `From` trait):**
- If the function returns `Result[_, E]` and the inner error is also `E`, `?` passes it through.
- If the function returns `Result[_, Error]` (the general error core name), **any** error type converts automatically. It is wrapped with its Show text and the source location. This is the convenient mode for applications and `main`.
- Otherwise, the types must match. Use `.map_err(f)` to convert (K2302 suggests the exact call).

Libraries usually define their own error enums. Applications usually use `Error`.

### 12.2 Panics

- `panic("text")` stops the current **task** (§13) with a message and a stack trace.
- Automatic panics: `Int` overflow, integer division by zero, `.at(i)` out of range, and a failed `expect` in tests.
- There is no `catch` (§17). A panic is never part of normal control flow.

### 12.3 Where panics stop

| Context | What happens |
|---|---|
| A CLI program (`main`) | Message plus trace on stderr, exit code 101 |
| A web request | **Only that request** fails: the client gets `500` with a request ID, the trace goes to the log with `Secret`s redacted, and the server keeps serving. Production builds never render traces (S11). |
| A live component event | That event is dropped, the component reloads from its last good state, and the error is logged |
| A `test` | The test fails, showing the message |
| A task inside `task.all` (§13) | The sibling tasks are cancelled and the panic continues to the parent |

---

## 13. Concurrency: no `async` keyword (D12)

### 13.1 What the programmer sees

**Nothing special.** Code that waits for I/O is written exactly like code that doesn't:

```text
func load_profile(net: Net, id: Int) -> Result[Profile, Error]:
    user = http.get_json[User](net, url"https://api.example.com/users/{id}")?
    posts = http.get_json[List[Post]](net, url"https://api.example.com/users/{id}/posts")?
    return Ok(Profile(user: user, posts: posts))
```

To do several things **at the same time**, use the stdlib `task` module. These are functions, not keywords:

| Function | Meaning | JS lowering |
|---|---|---|
| `task.all(list_of_closures)` | run all, wait for all, return `List[T]`. If one panics, the others are cancelled. | `Promise.all` plus an `AbortController` |
| `task.both(f, g)` | run two (of different types), return `task.Pair[A, B]` | `Promise.all([f(), g()])` |
| `task.race(list_of_closures)` | first to finish wins, the rest are cancelled | `Promise.race` plus abort |
| `task.timeout(ms, f)` | `Result[T, task.TimedOut]` | `Promise.race` with a timer |
| `clock.sleep(ms)` | pause this task | `await new Promise(setTimeout)` |

```text
pair = task.both(func(): load_user(net, id), func(): load_orders(net, id))
print("{pair.first.name} has {pair.second.length} orders")
```

Concurrency is **structured**: every task started by `task.*` finishes, or is cancelled, before the call returns. There is no fire-and-forget `spawn` in v0.1, so no task can outlive its parent or leak.

### 13.2 Semantics

- **Single-threaded, cooperative.** A Kalam program runs on one event loop. A task gives up control only at a **suspension point**: a call that waits for I/O, a timer, or another task.
- **No data races, by construction.** Data is immutable, closures capture by value (§9.3), and there are no global variables (§7). Two tasks can never write to the same variable. Shared mutable state lives only in capabilities (a database or cache), where transactions are the tool.
- **Ordering:** code between two suspension points runs without interruption.

### 13.3 The hidden "suspends" bit

The compiler infers, for every function, whether it **suspends**:

1. Primitive suspending operations (stdlib I/O, `clock.sleep`, and `extern` functions returning `Promise[T]`, §16) suspend.
2. A function suspends if it calls any function that suspends.
3. **A function type carries the bit.** `func(Int) -> Text` is either sync or suspending, inferred and never written. A sync function may be used where a suspending one is expected (it is upgraded). The reverse is K3301.
4. **Higher-order functions are polymorphic in the bit.** `list.map(f)` suspends exactly when `f` does. The JS backend emits up to two versions of such functions (a plain one and an `async` one) and picks one statically at each call site.
5. **Escaping functions** are function values stored in a record, list, map or `var`, or passed to a stdlib API that stores them (such as route handlers in `web.App`). They are **always lowered as suspending.** This costs one microtask, so no inference across data structures is needed.
6. **Callbacks into JS:** a closure passed to an `extern` function whose JS signature expects a *synchronous* callback (for example a sort comparator) **must not suspend**. Violating that is K3302, *"This function waits for I/O, but `compare` must answer immediately. Load the data before sorting."*
7. Recursion: the bit is computed as a fixpoint over each group of mutually recursive functions.

The LSP shows a small ⏸ marker beside every suspending call (an inlay hint), so readers can see where interleaving can happen without the language having a keyword for it.

### 13.4 Lowering to JS

```text
# Kalam
func total_spent(conn: db.Conn, user_id: Int) -> Result[Decimal, db.Error]:
    rows = conn.fetch_all[Order](sql"SELECT * FROM orders WHERE user_id = {user_id}")?
    return Ok(rows.map(func(o): o.total).sum())
```

```js
// Emitted JS (simplified; source maps point back to the Kalam lines)
async function total_spent(conn, user_id) {
  const $r = await $db.fetch_all(conn, $sql(["SELECT * FROM orders WHERE user_id = ", ""], [user_id]), $decoder_Order);
  if ($r.$ === 1) return $r;                         // Err: early return for `?`
  const rows = $r.value;
  return $ok($list_sum_dec($list_map(rows, (o) => o.total)));   // sync map: no await
}
```

- A sync function lowers to a plain JS function, and a suspending one to an `async function`. Each call to a suspending function gets `await`.
- **`main` and route handlers** are invoked by the runtime, which awaits them.
- On the future **C/WASM backend**, the same bit selects stackless coroutines (C) or WASM stack switching. The source does not change.

---

## 14. Diagnostics: Elm-grade errors

Every diagnostic follows one template (report 01 §3.4, report 05 D5):

```text
-- TITLE IN PLAIN WORDS -------------------------------- file.kl  K<code>

One sentence: what I found, in the reader's words.

<line>│ the user's code exactly as written
              ^^^^^ underline of the exact span
<secondary label, if useful: "`total` was defined here as Text">

Why this matters, in one or two sentences, when it isn't obvious.

The fix, as code that will compile:

    total = count + price.round()

Learn more: kalam explain K<code>
```

**Rules:**
1. **Language of the message:** the user's `KALAM_LANG`, which can be English, Arabic or Franco. **Code in the message stays in the file's skin**, including the suggested fix.
2. **One root cause first.** Follow-on errors caused by the first one are hidden, with a count ("2 more errors may be caused by this one").
3. **Positive tone.** Never "illegal", "invalid", "fatal" or "bad". Speak in the first person ("I expected…"), as Elm does.
4. **A concrete fix in most errors.** Machine-applicable fixes are emitted as LSP code actions and in JSON (`--format json`) for AI agents (report 06 §7.3).
5. **Stable codes.**

| Range | Area |
|---|---|
| K0xxx | lexical, Unicode and skins |
| K1xxx | syntax |
| K2xxx | types and patterns |
| K3xxx | safety and security |
| K4xxx | modules and packages |

   `kalam explain K2001` prints a long explanation in the user's language.
6. **Cross-skin help.** Typing a keyword from another skin gives K0201 with the right word (*"`if` is English. In Arabic files, write `إذا`."*). An attached Arabic و gets a hint: *"`وس` is not defined. Did you mean `و س` (`and` followed by `س`)?"*
7. **Every diagnostic is golden-tested in all three message languages** (report 05 P6).

**Example: the same error in all three message languages.**

English (`KALAM_LANG=en`), English file:

```text
-- NUMBER TYPES DON'T MIX -------------------------------- cart.kl  K2001

I can't add an Int to a Float:

12│     total = count + price
                ^^^^^^^^^^^^^
`count` is an Int and `price` is a Float. Kalam never converts numbers
by itself, so you choose:

    total = Float(count) + price      # keep the fraction
    total = count + price.round()     # drop the fraction

Learn more: kalam explain K2001
```

Arabic (`KALAM_LANG=ar`), Arabic file:

```text
-- لا يمكن خلط أنواع الأرقام ----------------------------- سلة.ar.kl  K2001

لا أستطيع جمع عدد مع كسري:

12│     مجموع = كمية + سعر
                ^^^^^^^^^^^
`كمية` من النوع عدد و`سعر` من النوع كسري. كلام لا يحوّل الأرقام من تلقاء
نفسه، فاختر أنت:

    مجموع = كسري(كمية) + سعر        # احتفظ بالكسر
    مجموع = كمية + سعر.round()      # احذف الكسر

للمزيد: kalam explain K2001
```

Franco (`KALAM_LANG=fr`), Franco file:

```text
-- mynfa3sh ne5lot anwa3 el arqam ------------------------ sala.fr.kl  K2001

mesh 2ader agma3 adad 3ala kasri:

12│     magmoo3 = kammeya + se3r
                  ^^^^^^^^^^^^^^
`kammeya` no3o adad we `se3r` no3o kasri. Kalam mesh beyghayyar el arqam
lewa7do, e5tar enta:

    magmoo3 = kasri(kammeya) + se3r      # sib el kasr
    magmoo3 = kammeya + se3r.round()     # sheel el kasr

aktar: kalam explain K2001
```

*(Franco prose in messages may use digit-letters freely. The no-leading-digit rule applies only to code.)*

---

## 15. Effects and capabilities for I/O

**Capabilities are ordinary values (L12). There is no effect syntax.** To touch the outside world, a function needs a value that grants it. The only source of these values is the `World` passed to `main`, or the request context a framework passes to a handler.

```text
func main(world: World):
    world.net       # Net:   outbound network and listening sockets
    world.fs        # Fs:    files
    world.env       # Env:   environment variables (secrets come back as Secret[Text])
    world.clock     # Clock: current time and sleep
    world.random    # Random: CSPRNG (S7); random.seeded(n) is the explicit fast RNG for games
    world.proc      # Proc:  run Commands (§11.5)
    world.db        # Db:    database connections
    world.stdin     # Stdin: reading input
```

- **Attenuation:** capabilities can be narrowed before they are passed on: `world.fs.only("./uploads")`, `world.net.only(["api.paymob.com"])`. A narrowed capability can never be widened again.
- **What the compiler guarantees:** a function or package whose code never receives a `Net` value, and contains no `extern`, **cannot** use the network. The package manifest lists the capabilities a package's public API asks for, and `kalam add` / `kalam update` show any *new* capability as a diff that must be approved (report 04, P6).
- **The single ambient effect is `print`** (writing a line to stdout) plus `panic`. They are kept ambient so that "hello world" needs no explanation of capabilities. In web handlers, `print` goes to the structured log.
- **Why this is the simplest secure option:** it adds *zero* concepts beyond "functions take parameters". There is no effect-annotation language to learn, and data flow shows exactly which code can do what.
- **Ergonomics:** passing many capabilities is handled with a plain record, for example a type `Deps` with fields `db: db.Conn`, `net: Net` and `clock: Clock`, built once in `main` and passed down. The web kit does this for you: handlers receive `req` and the app's `Deps`.
- **Security defaults that apply inside capabilities** (report 04 §3.6):
  - `Net` blocks private, loopback, link-local and metadata IP ranges after DNS resolution unless attenuated to allow them (S12).
  - `Fs` paths refuse `..` and absolute escapes (S13).

---

## 16. Interop with JS and npm

### 16.1 `extern` declarations

```text
extern "npm:slugify@1.6.6" func slugify(text: Text) -> Text
extern "npm:@paymob/sdk@2.1.0" type PaymobClient
extern "npm:@paymob/sdk@2.1.0" func create_client(key: Secret[Text]) -> PaymobClient
extern "npm:@paymob/sdk@2.1.0" func create_intent(client: PaymobClient, amount_cents: Int) -> Promise[Result[Intent, Text]]
extern "./browser/confetti.js" func confetti(count: Int) -> Nothing
```

- The string names the source: `npm:package@exact-version` (pinned in the lockfile with a hash), or a relative `.js` / `.ts` file in the package.
- `extern type` declares an **opaque** JS value. Kalam code can only pass it around.
- **`Promise[T]` may appear only as the return type of an `extern func`.** Kalam callers see a plain `T` and become suspending (§13.3). This is the only place the word "promise" exists in Kalam.

### 16.2 Boundary checks

The JS side is untrusted. The compiler emits a **runtime check** on every value coming back from JS:

| Declared Kalam type | Accepted JS value |
|---|---|
| `Int` | a safe integer, or panic |
| `Float` | a number |
| `Text` | a string |
| `Bool` | a boolean |
| `Option[T]` | `null` or `undefined` (as `None`), else `T` |
| `List[T]` | an array, checked element by element |
| records | a plain object, decoded field by field with the Json rules (unknown fields ignored at this boundary, prototype keys rejected) |

A failed check **panics** with the extern's name and the bad value's shape. A wrong `.d.ts` can never corrupt Kalam's types. Values going *to* JS are converted the other way: records become frozen plain objects, `Option` becomes a value or `undefined`, and `Map` becomes a JS `Map` (never an object, so prototype pollution is impossible).

### 16.3 Safety accounting

- `extern` is an escape hatch (L13): fixed ASCII in every skin, counted by `kalam audit`, and allowed in a package only if its manifest declares `ffi = true`, which shows on its registry page.
- Extern code runs with the full authority of the JS host, so **a package with `ffi = true` is treated as holding every capability** in the capability diff (§15).
- `kalam bind npm:<pkg>` reads the package's `.d.ts` and **generates** `extern` declarations for review. It is a code generator, so the result is ordinary, readable Kalam.

### 16.4 Calling Kalam from JS, and client islands

- `export` functions of a module compiled with `kalam build --lib` become ES module exports with generated `.d.ts`, so TS projects can adopt Kalam one module at a time.
- For rich client-side widgets (editors, canvases), a module marked `[target] browser` in `kalam.toml` compiles to a JS island that a view can mount with `web.island("chart", data)`. Client islands follow the same rules (no ambient I/O, capability values from the browser host).

---

## 17. What is deliberately excluded

Each exclusion lowers the concept count. Most are also security rules (report 04).

| Excluded | Use instead | Why |
|---|---|---|
| `null` / `undefined` | `Option[T]` | L5; one absent value |
| Exceptions, `try`/`catch`/`throw` | `Result` + `?`; `panic` for bugs | L6; errors visible in types |
| `async` / `await` | inferred suspension (§13) | D12 |
| Classes, inheritance, `self`, `this` | records, enums, functions, `a.f()` syntax | fewer concepts; no fragile hierarchies |
| User-defined traits/interfaces/type classes | built-in abilities (§5.6) | v0.1 simplicity; revisit for 0.2 (§20) |
| Operator overloading | named functions | `+` always means what you think |
| Macros, decorators, annotations, metaprogramming | the stdlib and code generators (`kalam bind`) | readable code and readable errors |
| `eval`, dynamic `import`, reflection, call-by-name | none | L9 |
| Implicit conversions, truthiness, `==` across types | explicit conversion functions | L3, L4 |
| Default arguments, variadics, overloading | a record with defaults; a `List` parameter | one way to call a function |
| Tuples | records (`task.Pair` where a library needs one) | fields have names |
| Indexing syntax `xs[i]` | `.get(i)` (Option) and `.at(i)` (panics) | safe by default; `[` has one job |
| Type aliases | a one-field record (a type `UserId` with one field `value: Int`) | one way to name a type |
| Shadowing in the same function | a new name | one name, one meaning |
| Global mutable state, top-level `var` | capabilities (db, cache) or values passed down | no data races, testable code |
| Closures that mutate captured variables | return the new value | §9.3 |
| Multi-line lambdas | a named nested `func` | §9.3 |
| `++`, `+=`, ternary `?:`, `unless`, `do-while`, `goto`, labels | `x = x + 1`, `if` expressions, `while` | one way |
| Block comments | `#` per line | one way |
| Mixed skins in one file; per-skin word order | conversion tools (§3.5) | D6; parity |
| Shell strings, raw SQL/HTML from `Text` | tagged literals (§11) | S1–S3 |
| Install scripts in packages | none | P1 |
| Unstructured `spawn`, threads, shared memory | `task.*` (§13) | L11 |
| "Levels" or beginner sub-languages | the small core *is* the beginner language | one language for everyone |

### 17.1 Concept budget for "hello web app"

Example 18.5 (an HTTP handler with a JSON route) uses exactly these concepts. Report 05 sets ≤ 12 for MVP.

1. `use` of a module
2. `func` with typed parameters and a return type
3. `return`
4. binding (`name = …`)
5. text with `{}` interpolation
6. method-call syntax (`a.f(b)`)
7. `Option` with `.value_or(…)`
8. map literal (`{"ok": true}`)
9. `main(world: World)` and passing a capability
10. registering a route with an access rule (`web.public`)

That is **10 concepts**, with no `async`, no classes, no decorators and no build configuration.

---

## 18. Example programs in all three skins

Reminder (§3.5): keywords, core names, digits and `،`/`؟` are mechanical conversions. Identifiers and strings were translated by hand, as a glossary would. Library names (`web`, `db`, `.to_int()`, `.value_or()`) stay English in every skin.

### 18.1 Hello, world

**English** (`hello.kl`)
```text
func main():
    print("Hello, world!")
```

**Arabic** (`hello.ar.kl`)
```text
دالة بداية():
    اطبع("مرحبا يا عالم!")
```

**Franco** (`hello.fr.kl`)
```text
dala bedaya():
    ektb("ahlan ya donia!")
```

### 18.2 FizzBuzz, with a test

**English** (`fizzbuzz.kl`)
```text
func fizzbuzz(n: Int) -> Text:
    if n % 15 == 0:
        return "FizzBuzz"
    else if n % 3 == 0:
        return "Fizz"
    else if n % 5 == 0:
        return "Buzz"
    return "{n}"

func main():
    for n in 1..100:
        print(fizzbuzz(n))

test "multiples of 3 and 5":
    expect(fizzbuzz(15) == "FizzBuzz")
    expect(fizzbuzz(9) == "Fizz")
    expect(fizzbuzz(7) == "7")
```

**Arabic** (`fizzbuzz.ar.kl`)
```text
دالة فز_بز(ن: عدد) -> نص:
    إذا ن % 15 == 0:
        أرجع "فز بز"
    وإلا إذا ن % 3 == 0:
        أرجع "فز"
    وإلا إذا ن % 5 == 0:
        أرجع "بز"
    أرجع "{ن}"

دالة بداية():
    لكل ن في 1..100:
        اطبع(فز_بز(ن))

اختبار "مضاعفات 3 و 5":
    يجب(فز_بز(15) == "فز بز")
    يجب(فز_بز(9) == "فز")
    يجب(فز_بز(7) == "7")
```

With `[skin.ar] digits = "arabic"`, the formatter prints the same AST with Arabic-Indic digits, for example `لكل ن في ١..١٠٠:` and `إذا ن % ١٥ == ٠:`.

**Franco** (`fizzbuzz.fr.kl`)
```text
dala fizzbuzz(n: adad) -> nass:
    law n % 15 == 0:
        rag3 "FizzBuzz"
    walla law n % 3 == 0:
        rag3 "Fizz"
    walla law n % 5 == 0:
        rag3 "Buzz"
    rag3 "{n}"

dala bedaya():
    lekol n fi 1..100:
        ektb(fizzbuzz(n))

ekhtebar "modaa3afat 3 we 5":
    lazem(fizzbuzz(15) == "FizzBuzz")
    lazem(fizzbuzz(9) == "Fizz")
    lazem(fizzbuzz(7) == "7")
```

### 18.3 Records, enums and `match`

**English** (`rooms.kl`)
```text
type Shape:
    Circle(radius: Float)
    Rect(width: Float, height: Float)
    Triangle(base: Float, height: Float)

type Room:
    name: Text
    floor: Shape

func area(shape: Shape) -> Float:
    return match shape:
        Circle(r): 3.14159 * r * r
        Rect(width: w, height: h): w * h
        Triangle(base: b, height: h): b * h / 2.0

func main():
    rooms = [
        Room(name: "Kitchen", floor: Rect(width: 4.0, height: 3.0)),
        Room(name: "Tower", floor: Circle(2.5)),
    ]
    for room in rooms:
        print("{room.name}: {area(room.floor)} m²")
```

**Arabic** (`rooms.ar.kl`)
```text
نوع شكل:
    دائرة(نصف_قطر: كسري)
    مستطيل(عرض: كسري، ارتفاع: كسري)
    مثلث(قاعدة: كسري، ارتفاع: كسري)

نوع غرفة:
    اسم: نص
    أرضية: شكل

دالة مساحة(ش: شكل) -> كسري:
    أرجع طابق ش:
        دائرة(نق): 3.14159 * نق * نق
        مستطيل(عرض: ع، ارتفاع: ر): ع * ر
        مثلث(قاعدة: ق، ارتفاع: ر): ق * ر / 2.0

دالة بداية():
    غرف = [
        غرفة(اسم: "المطبخ"، أرضية: مستطيل(عرض: 4.0، ارتفاع: 3.0))،
        غرفة(اسم: "البرج"، أرضية: دائرة(2.5))،
    ]
    لكل غ في غرف:
        اطبع("{غ.اسم}: {مساحة(غ.أرضية)} م²")
```

**Franco** (`rooms.fr.kl`)
```text
no3 shakl:
    dayra(nos_qotr: kasri)
    mostateel(ard: kasri, ertefa3: kasri)
    mosallas(qa3da: kasri, ertefa3: kasri)

no3 ghorfa:
    esm: nass
    ardeya: shakl

dala masa7a(sh: shakl) -> kasri:
    rag3 hasab sh:
        dayra(r): 3.14159 * r * r
        mostateel(ard: a, ertefa3: e): a * e
        mosallas(qa3da: q, ertefa3: e): q * e / 2.0

dala bedaya():
    ghoraf = [
        ghorfa(esm: "el matbakh", ardeya: mostateel(ard: 4.0, ertefa3: 3.0)),
        ghorfa(esm: "el borg", ardeya: dayra(2.5)),
    ]
    lekol g fi ghoraf:
        ektb("{g.esm}: {masa7a(g.ardeya)} m²")
```

Deleting the `Triangle` arm in any skin produces K2401 (§8.2), naming the missing variant in that skin.

### 18.4 `Result` and `?`

**English** (`signup.kl`)
```text
type SignupError:
    NotANumber(text: Text)
    TooYoung(age: Int)

func parse_age(text: Text) -> Result[Int, SignupError]:
    age = text.trim().to_int().ok_or(NotANumber(text))?
    if age < 18:
        return Err(TooYoung(age))
    return Ok(age)

func describe(text: Text) -> Text:
    return match parse_age(text):
        Ok(age): "Welcome! You are {age}."
        Err(NotANumber(t)): "'{t}' is not a number."
        Err(TooYoung(age)): "Sorry, you must be 18 or older (you are {age})."

func main():
    for input in ["42", "abc", "15"]:
        print(describe(input))
```

**Arabic** (`signup.ar.kl`)
```text
نوع رفض_التسجيل:
    غير_رقمي(مدخل: نص)
    صغير_السن(عمر: عدد)

دالة حلل_العمر(مدخل: نص) -> نتيجة[عدد، رفض_التسجيل]:
    عمر = مدخل.trim().to_int().ok_or(غير_رقمي(مدخل))؟
    إذا عمر < 18:
        أرجع فشل(صغير_السن(عمر))
    أرجع نجاح(عمر)

دالة صف(مدخل: نص) -> نص:
    أرجع طابق حلل_العمر(مدخل):
        نجاح(عمر): "أهلا! عمرك {عمر}."
        فشل(غير_رقمي(م)): "'{م}' ليس رقما."
        فشل(صغير_السن(عمر)): "عذرا، يجب أن يكون عمرك 18 أو أكثر (عمرك {عمر})."

دالة بداية():
    لكل مدخل في ["42"، "abc"، "15"]:
        اطبع(صف(مدخل))
```

**Franco** (`signup.fr.kl`)
```text
no3 rafd:
    madkhal_ghareeb(madkhal: nass)
    soghayyar(omr: adad)

dala hallel_omr(madkhal: nass) -> natiga[adad, rafd]:
    omr = madkhal.trim().to_int().ok_or(madkhal_ghareeb(madkhal))?
    law omr < 18:
        rag3 fashal(soghayyar(omr))
    rag3 nagah(omr)

dala wasf(madkhal: nass) -> nass:
    rag3 hasab hallel_omr(madkhal):
        nagah(omr): "ahlan! 3omrak {omr}."
        fashal(madkhal_ghareeb(m)): "'{m}' mesh raqam."
        fashal(soghayyar(omr)): "ma3lesh, lazem 3omrak yekoon 18 aw aktar (3omrak {omr})."

dala bedaya():
    lekol madkhal fi ["42", "abc", "15"]:
        ektb(wasf(madkhal))
```

Output in every skin (modulo the strings): `Welcome! You are 42.` / `'abc' is not a number.` / `Sorry, you must be 18 or older (you are 15).`

### 18.5 HTTP handler

**English** (`server.kl`)
```text
use web

func hello(req: web.Request) -> web.Response:
    name = req.query("name").value_or("world")
    return web.text("Hello, {name}!")

func health(req: web.Request) -> web.Response:
    return web.json({"ok": true})

func main(world: World):
    app = web.App()
    app.get("/hello", web.public, hello)
    app.get("/health", web.public, health)
    app.serve(world.net, 8080)
```

**Arabic** (`server.ar.kl`)
```text
استخدم web

دالة رحب(طلب: web.Request) -> web.Response:
    اسم = طلب.query("name").value_or("يا عالم")
    أرجع web.text("أهلا {اسم}!")

دالة الصحة(طلب: web.Request) -> web.Response:
    أرجع web.json({"ok": صواب})

دالة بداية(كون: عالم):
    تطبيق = web.App()
    تطبيق.get("/hello"، web.public، رحب)
    تطبيق.get("/health"، web.public، الصحة)
    تطبيق.serve(كون.net، 8080)
```

**Franco** (`server.fr.kl`)
```text
hat web

dala ra7ab(talab: web.Request) -> web.Response:
    esm = talab.query("name").value_or("ya donia")
    rag3 web.text("ahlan {esm}!")

dala se7a(talab: web.Request) -> web.Response:
    rag3 web.json({"ok": sa7})

dala bedaya(kon: donia):
    app = web.App()
    app.get("/hello", web.public, ra7ab)
    app.get("/health", web.public, se7a)
    app.serve(kon.net, 8080)
```

Notes:
- **Deny by default (S9):** the second argument of `app.get` is a required access rule. It can be `web.public`, `web.signed_in` or `web.role("admin")`. There is no overload without it, so an unprotected route cannot be written by accident.
- In the Arabic file the `World` parameter is named `كون` ("universe"), because `عالم` is the core name for the type, and types and values share one namespace (§2.3). English can write `world: World` only because Latin script has case.
- `serve` suspends forever, so `main` is lowered as `async`. Neither handler suspends, so both lower to plain JS functions. The router stores them, so per §13.3 rule 5 they are wrapped as suspending.

### 18.6 SQL query

**English** (`users.kl`)
```text
use db

type User:
    id: Int
    name: Text
    email: Text

func find_user(conn: db.Conn, email: Text) -> Result[Option[User], db.Error]:
    return conn.fetch_optional(sql"SELECT id, name, email FROM users WHERE email = {email}")

func rename_user(conn: db.Conn, id: Int, name: Text) -> Result[Int, db.Error]:
    return conn.execute(sql"UPDATE users SET name = {name} WHERE id = {id}")

func main(world: World) -> Result[Nothing, Error]:
    conn = db.connect(world.db, world.env.secret("DATABASE_URL")?)?
    match find_user(conn, "mona@example.com")?:
        Some(user):
            print("Found {user.name} (#{user.id})")
            changed = rename_user(conn, user.id, "Mona A.")?
            print("{changed} row updated")
        None:
            print("No user with that email")
    return Ok(Nothing)
```

**Arabic** (`users.ar.kl`)
```text
استخدم db

نوع مستخدم:
    id: عدد
    name: نص
    email: نص

دالة ابحث_عن_مستخدم(اتصال: db.Conn، بريد: نص) -> نتيجة[اختياري[مستخدم]، db.Error]:
    أرجع اتصال.fetch_optional(sql"SELECT id, name, email FROM users WHERE email = {بريد}")

دالة غير_الاسم(اتصال: db.Conn، رقم: عدد، اسم: نص) -> نتيجة[عدد، db.Error]:
    أرجع اتصال.execute(sql"UPDATE users SET name = {اسم} WHERE id = {رقم}")

دالة بداية(كون: عالم) -> نتيجة[فراغ، مشكلة]:
    اتصال = db.connect(كون.db، كون.env.secret("DATABASE_URL")؟)؟
    طابق ابحث_عن_مستخدم(اتصال، "mona@example.com")؟:
        موجود(م):
            اطبع("وجدنا {م.name} (#{م.id})")
            عدد_الصفوف = غير_الاسم(اتصال، م.id، "منى أ.")؟
            اطبع("تم تحديث {عدد_الصفوف} صف")
        عدم:
            اطبع("لا يوجد مستخدم بهذا البريد")
    أرجع نجاح(فراغ)
```

**Franco** (`users.fr.kl`)
```text
hat db

no3 mostakhdem:
    id: adad
    name: nass
    email: nass

dala dawwar(conn: db.Conn, email: nass) -> natiga[ekhtiari[mostakhdem], db.Error]:
    rag3 conn.fetch_optional(sql"SELECT id, name, email FROM users WHERE email = {email}")

dala ghayyar_esm(conn: db.Conn, id: adad, esm: nass) -> natiga[adad, db.Error]:
    rag3 conn.execute(sql"UPDATE users SET name = {esm} WHERE id = {id}")

dala bedaya(kon: donia) -> natiga[fady, moshkela]:
    conn = db.connect(kon.db, kon.env.secret("DATABASE_URL")?)?
    hasab dawwar(conn, "mona@example.com")?:
        mawgood(m):
            ektb("la2eena {m.name} (#{m.id})")
            etghayyar = ghayyar_esm(conn, m.id, "Mona A.")?
            ektb("{etghayyar} saff etghayyar")
        mafeesh:
            ektb("mafeesh mostakhdem bel email da")
    rag3 nagah(fady)
```

Notes:
- **The record fields stay `id`, `name` and `email` in all three skins on purpose.** `fetch_optional` decodes rows into `User` by matching column names to field names (schema-first, S4). Arabic field names would need `AS` aliases in the SQL. Most teams keep the database schema in English, and this example shows that common, honest case.
- The SQL text inside `sql"…"` keeps its ASCII commas in the Arabic file, because string contents are never converted.
- `world.env.secret(...)` returns `Result[Secret[Text], env.Error]`. `db.connect` accepts the `Secret` directly, so the URL never appears in logs.
- Writing `conn.execute("UPDATE users SET name = '" + name + "'")` instead is K3101 (§11.1).

### 18.7 HTML view: a live, server-driven component (D11)

**English** (`counter.kl`)
```text
use web

type Model:
    count: Int

type Msg:
    Increment
    Decrement
    Reset

func update(model: Model, msg: Msg) -> Model:
    return match msg:
        Increment: model.with(count: model.count + 1)
        Decrement: model.with(count: model.count - 1)
        Reset: Model(count: 0)

func view(model: Model) -> Html:
    return html"""
        <section>
          <h1>Clicks: {model.count}</h1>
          <button send-click={Decrement}>−</button>
          <button send-click={Increment}>+</button>
          <button send-click={Reset}>Reset</button>
        </section>
        """

func main(world: World):
    app = web.App()
    app.live("/counter", web.public, Model(count: 0), update, view)
    app.serve(world.net, 8080)
```

**Arabic** (`counter.ar.kl`)
```text
استخدم web

نوع حالة:
    عداد: عدد

نوع رسالة:
    زد
    انقص
    صفر

دالة حدث(ح: حالة، ر: رسالة) -> حالة:
    أرجع طابق ر:
        زد: ح.with(عداد: ح.عداد + 1)
        انقص: ح.with(عداد: ح.عداد - 1)
        صفر: حالة(عداد: 0)

دالة اعرض(ح: حالة) -> Html:
    أرجع html"""
        <section dir="rtl">
          <h1>عدد النقرات: {ح.عداد}</h1>
          <button send-click={انقص}>−</button>
          <button send-click={زد}>+</button>
          <button send-click={صفر}>صفّر</button>
        </section>
        """

دالة بداية(كون: عالم):
    تطبيق = web.App()
    تطبيق.live("/counter"، web.public، حالة(عداد: 0)، حدث، اعرض)
    تطبيق.serve(كون.net، 8080)
```

**Franco** (`counter.fr.kl`)
```text
hat web

no3 hala:
    nakarat: adad

no3 resala:
    zawed
    na2es
    saffar

dala hadeth(ha: hala, r: resala) -> hala:
    rag3 hasab r:
        zawed: ha.with(nakarat: ha.nakarat + 1)
        na2es: ha.with(nakarat: ha.nakarat - 1)
        saffar: hala(nakarat: 0)

dala e3red(ha: hala) -> Html:
    rag3 html"""
        <section>
          <h1>nakarat: {ha.nakarat}</h1>
          <button send-click={na2es}>−</button>
          <button send-click={zawed}>+</button>
          <button send-click={saffar}>saffar</button>
        </section>
        """

dala bedaya(kon: donia):
    app = web.App()
    app.live("/counter", web.public, hala(nakarat: 0), hadeth, e3red)
    app.serve(kon.net, 8080)
```

Notes:
- `{model.count}` is in an HTML **text** context, so it is escaped. `send-click={Increment}` is checked to be a `Msg` (K3105). No JS is written by hand, and the browser runs only the first-party live client.
- The tag names and attributes inside `html"""…"""` are HTML, so they stay ASCII in every skin. Text content is whatever the author writes. In Arabic, `dir="rtl"` is explicit here. The web kit's page layout sets `dir` and `lang` automatically from the request locale.

### 18.8 A small game loop: guess the number

**English** (`guess.kl`)
```text
use random

func main(world: World):
    secret = random.int(world.random, 1, 100)
    var tries = 0
    print("I picked a number from 1 to 100. Guess it!")
    while world.stdin.read_line() is Some(line):
        match line.trim().to_int():
            None:
                print("Please type a whole number.")
            Some(guess):
                tries = tries + 1
                if guess < secret:
                    print("Higher!")
                else if guess > secret:
                    print("Lower!")
                else:
                    print("Correct in {tries} tries!")
                    break
```

**Arabic** (`guess.ar.kl`)
```text
استخدم random

دالة بداية(كون: عالم):
    سر = random.int(كون.random، 1، 100)
    متغير محاولات = 0
    اطبع("اخترت رقما من 1 إلى 100. خمّنه!")
    طالما كون.stdin.read_line() هو موجود(سطر):
        طابق سطر.trim().to_int():
            عدم:
                اطبع("اكتب رقما صحيحا من فضلك.")
            موجود(تخمين):
                محاولات = محاولات + 1
                إذا تخمين < سر:
                    اطبع("أكبر!")
                وإلا إذا تخمين > سر:
                    اطبع("أصغر!")
                وإلا:
                    اطبع("صح! في {محاولات} محاولات.")
                    اخرج
```

**Franco** (`guess.fr.kl`)
```text
hat random

dala bedaya(kon: donia):
    serr = random.int(kon.random, 1, 100)
    motaghayer mo7awlat = 0
    ektb("ana 5atart raqam men 1 le 100. 5ammen!")
    tool_ma kon.stdin.read_line() howa mawgood(satr):
        hasab satr.trim().to_int():
            mafeesh:
                ektb("ekteb raqam sa7 law sama7t.")
            mawgood(takhmeen):
                mo7awlat = mo7awlat + 1
                law takhmeen < serr:
                    ektb("akbar!")
                walla law takhmeen > serr:
                    ektb("asghar!")
                walla:
                    ektb("sa7! fi {mo7awlat} mo7awlat.")
                    kfaya
```

Notes:
- `read_line()` waits for the keyboard, so `main` suspends, with no `async` anywhere (§13). It returns `Option[Text]`, which is `None` at end of input, so `while … is Some(line)` ends cleanly.
- `var tries` is the only mutable binding in all eight examples. Everything else is immutable.
- `world.random` is a CSPRNG (S7). A real-time game uses `random.seeded(n)` for replayable runs and `world.clock` for frame timing. A frame loop has the same shape: `while running: state = update(state, world.clock.tick()); draw(screen, state)`.

---

## 19. Conformance and measurable simplicity

### 19.1 What CI must prove about this spec (report 05)

| Gate | Check for this draft |
|---|---|
| P1 canonical AST identity | The 8 examples in §18 parse to the same AST hash in all three skins, *after* glossary mapping of identifiers and strings. Without the glossary, `convert` output is checked instead. |
| P2/P3 round-trip | `convert` en→ar→fr→en is byte-identical after `fmt`, including with `[skin.ar] digits = "arabic"` |
| P7 keyword-table integrity | Five checks: (a) no Franco keyword or alias starts with a digit or matches the number grammar; (b) no two keywords or core names in one skin share a folding key (§2.3); (c) no Arabic keyword differs from another only by harakat; (d) every alias maps to exactly one keyword; (e) `extern` and the tag names are identical in all skins |
| L6/L8/S1 negative tests | Ignoring a `Result`, a non-exhaustive `match`, and `Text` passed to a `Sql`/`Html`/`Command` sink all fail to compile in every skin |
| U1/U3/U4 | The bidi, invisible-character, mixed-script and skeleton-collision corpora all fail with the documented K0xxx codes |

### 19.2 Simplicity scorecard (D4) for this draft

| Metric | This draft | MVP limit | 1.0 limit |
|---|---|---|---|
| Reserved keywords per skin | 22 | 40 | 35 |
| Synonyms | 0 | 0 | 0 |
| Grammar productions | 58 | 120 | 100 |
| Concepts before "hello web app" | 10 | 12 | 8 |
| Core names that differ by skin | 20 | (n/a) | (n/a) |
| Punctuation aliases | 2 (Arabic `،` `؟`) plus `٫` with Arabic digits | (n/a) | (n/a) |

The 1.0 target of ≤ 8 concepts can be reached by teaching `World` together with `main`, and the map literal together with JSON.

### 19.3 Toolchain hooks that this spec assumes (D10)

The single `kalam` binary provides:

| Command | Purpose |
|---|---|
| `kalam run` | build and run |
| `kalam build` | JS/TS output plus source maps; `--lib` for `.d.ts` |
| `kalam check` | type-check only; `--bidi` for the display-order check |
| `kalam fmt` | canonical formatting: normalizes aliases, hamza and digits; removes tatweel and harakat from identifiers |
| `kalam convert --to en\|ar\|fr [--glossary]` | lossless skin conversion |
| `kalam test` | runs `test` blocks |
| `kalam explain Kxxxx` | the long explanation of a diagnostic |
| `kalam bind npm:<pkg>` | generates `extern` declarations from `.d.ts` |
| `kalam audit` | `unsafe_*`, `extern` and capability counts, plus OSV checks |
| `kalam lsp` | the language server |
| `kalam diff --skin <s>` | review a change in any skin |

The playground is the same compiler built to WASM, with a skin toggle (report 03 §5).

---

## 20. Open questions

1. **`Int` range.** This draft makes `Int` ±(2^53−1) on every backend, with overflow panics and `BigInt` in the stdlib, which departs from report 03's Int64 suggestion. Is the "same on every backend, fast on JS" benefit worth not having native 64-bit integers when the C backend arrives? The alternative is Int64 everywhere, which means BigInt or a two-number representation on JS and slower SaaS code.
2. **The Arabic word for `is`.** `هو` ("it is") reads acceptably in `إذا الشكل هو دائرة(نق)`, but it is a pronoun. Alternatives are `يطابق` ("matches") and `كان`. This needs the native-speaker keyword survey from report 05 §1.1.
3. **Franco `var` = `motaghayer`** is 10 letters and over report 02's 3–8 letter guideline. The candidates are `khalli` (shorter, but it means "let", which loses the idea of *changes*) and `mot8ayar` (currently an alias). This needs the 200-developer survey proposed in report 02 §9.
4. **Escaping closures are always async** (§13.3 rule 5). That is simple, but it costs a microtask per call for stored callbacks. Should the compiler track the suspend bit through record and list types instead? That is more precise, but the bit would leak into type errors.
5. **No user-defined traits in v0.1.** Built-in Eq, Ord, Hash, Show and Json cover the examples and most SaaS code. Custom `Show` for domain types, or custom ordering for `sort()`, is impossible without a key function. When should interfaces arrive (0.2?), and in what minimal form (Go-style structural, or Gleam-style "pass a record of functions")?
6. **Closure capture by value** prevents a closure from updating outer state, which some UI and game patterns expect, such as an accumulator in a callback. The current answer is "return the new value". Is that acceptable to professional JS developers, or do they need a stdlib `Cell` (an explicit mutable box)?
7. **Opaque exported types.** `export type` currently exports its fields. Does v0.1 need a way to export a type while hiding its constructor and fields (for validated types such as `Email`)? A minimal option: fields marked by a leading `_` are module-private.
8. **Core names are translated.** This makes Arabic files read naturally (`نتيجة[عدد، مشكلة]`), but adds 20 per-skin names to learn and to keep in sync. The alternative, following report 02's "library names stay English", would show `Result[Int, Error]` inside Arabic code.
9. **No indexing syntax.** `xs.get(i)` and `xs.at(i)` are safe and unambiguous, but JS and Python developers will type `xs[i]` on day one. The plan is a K-code error with a fix-it. Should the formatter instead silently rewrite `xs[i]` to `xs.at(i)`?
10. **Live components and state storage.** `app.live` keeps state in server memory per connection. For multi-instance deployments, should state be re-sent from the client, signed, as with Hotwire, or kept in a store? This decision belongs to the web kit, not the language.
11. **The terminal fallback for Arabic diagnostics** (§2.7 item 6) relies on detecting bidi-capable terminals, and no reliable standard exists. Should the CLI default to English text plus Arabic code, with full Arabic only in the LSP?
