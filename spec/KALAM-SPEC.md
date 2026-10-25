# Kalam (كلام) Language Specification v0.1

> **Status:** canonical v0.1 specification. It supersedes the three drafts in `spec/drafts/`. **Date:** 2026-10-24.
> **Lineage:** based on `draft-production.md` (the judge's winner), with ideas grafted from `draft-security.md` and `draft-simplicity.md`. Every conflict the judge listed is resolved in §2.
> **Binding inputs:** `DECISIONS.md` (D1–D25, locked; D18–D25 and resolved D7 were applied on 2026-10-25, see the errata log) and research reports 01–06. Citations use the report's own rule IDs: `04 L7` is security charter rule L7, `05 P3` is parity gate P3, `02 §7.6` is a report section.
> **Companion files:** `spec/keywords.json` is the machine-readable keyword table the lexer loads. Where it and §7 disagree, `keywords.json` wins and §7 is a bug. `spec/llms.txt` is the compact summary for LLMs.
> **Normative words:** MUST, MUST NOT, SHOULD and MAY have their RFC 2119 meanings. Text marked *Rationale* is not normative.

---

## Table of contents

0. Reading guide and summary
1. Design principles
2. Design decisions and rationale (every judge conflict, resolved)
3. Deviations from the research reports
4. Source files, skins, canonical form and conversion
5. Lexical structure
6. D7 resolved: Arabic RTL, digits and punctuation
7. Keywords, core names and alias policy
8. Grammar (EBNF) and parser class
9. Type system
10. Expressions and statements
11. Pattern matching
12. Functions and closures
13. Modules, packages and visibility
14. Error handling: `Result`, `?`, `panic`
15. Concurrency without an `async` keyword
16. Capabilities and effects for I/O
17. Safe string kinds (typed sinks)
18. Secrets, randomness and decoding
19. Interop with JavaScript and npm
20. Standard library and the web kit (server-driven UI)
21. Canonical form for LLMs, diagnostics and tooling
22. Lowering to JavaScript (reference semantics)
23. Deliberately excluded
24. Security charter traceability
25. Conformance tests this spec implies
26. Scorecard self-report and concept budget
27. Diagnostic index
28. Example programs (8 programs × 3 skins)
29. Open questions (design team)
30. Founder decisions (resolved)

---

## 0. Reading guide and summary

Kalam is a small, statically typed, indentation-based language for professional developers building full-stack web and SaaS products (D2, D3). It compiles to ES2022 JavaScript first (D4). The compiler, formatter, LSP, package manager and MCP server ship as one Rust binary, `kalam` (D10). Every file is written in exactly one **skin**: English (`en`), Arabic (`ar`, فصحى) or Egyptian Franco (`franco`). All three skins parse to one skin-free canonical form, and conversion between them is lossless (D6).

**Kalam on one screen** (canonical English skin):

```kalam
use std/http.{self, HttpError, Request, Response}

@derive(Encode, Row)
type Note:                                   # a record: named fields
    id: Int
    title: String

enum Shape:                                  # a sum type: variants
    Circle(radius: Float)
    Rect(width: Float, height: Float)

fn area(shape: Shape) -> Float:              # top-level signatures are annotated
    return match shape:
        case .Circle(r): 3.14159 * r * r     # variants always have a leading dot
        case .Rect(w, h): w * h

fn show_note(req: Request[App]) -> Result[Response, HttpError]:
    let id = req.path_int("id")?             # ? propagates expected errors
    let found: Option[Note] = req.app.db.one(sql"SELECT id, title FROM notes WHERE id = {id}")?
    let label = if found.is_some(): "found" else: "missing"   # inline if expression
    log.info("note {id}: {label}")
    return match found:
        case Some(note): Ok(Response.json(note))
        case None: Err(.NotFound)
```

**Distinctive choices, in one list:**

1. **Canonical text is the English skin, formatted. Canonical identity is KCF.** LLMs, docs and `llms.txt` use the formatted English skin. Hashing, signing, the lockfile and the registry use the Kalam Canonical Form (KCF), a skin-free serialization (§4.4). An optional glossary is display-only, so it never changes a hash or a wire name.
2. **"A digit always starts a number."** No keyword, alias or core name in any skin starts with a digit. `3ala` is error K0127 in every skin. Lexing is trivially decidable (§5.6).
3. **Trojan-Source-proof source text.** There is a per-class table for code, comments and strings. The only whitespace is U+0020. Bidi controls are never allowed raw. Identifiers are never folded: two distinct names with the same confusable skeleton in one scope are an error (SEC0104) (§5.1–§5.3).
4. **`Int` is a true 64-bit checked integer on every backend.** Overflow traps (§9.2). On JS, an `Int` is a `number` in the safe range and a `bigint` outside it (§22). Locked by the founder (D18).
5. **Suspension is inferred.** There is no `async` keyword. Function types inside data suspend by default, and `@sync` opts out (§15).
6. **Variants use a leading dot** (`.Circle`). Kalam has **one namespace** per scope. Arabic has no letter case, so capitalization never carries meaning (§2, DR-05, DR-11).
7. **Injection is a type error.** `sql"…"`, `html"…"`, `cmd"…"`, `url"…"`, `path"…"` and `regex"…"` are compile-time-parsed tagged literals with typed holes. Literal provenance is guaranteed (§17).
8. **Routes are deny-by-default.** `access:` is a required argument (§20.2).
9. **Server-driven UI.** A live component is three ordinary things: state, `update(State, Msg) -> State` and `view(State) -> Html`. `k-click="{Msg.Increment}"` holes are type-checked and travel as signed, CSRF-bound opaque tokens (§20.3).
10. **A table of known LLM mistakes.** Each mistake has a stable code and a machine-applicable fix (§21.3). `kalam mcp` exposes the compiler to agents.

**Size of the language** (details in §26): 31 keywords per skin (29 translated, plus `unsafe` and `extern`, which are fixed ASCII). There are also 19 translated core names that cannot be shadowed, for **50 reserved spellings per skin**. There are 0 synonyms in English and Arabic, and Franco has a closed list of spelling variants. The grammar has 75 productions.

---

## 1. Design principles

When two principles conflict, the one listed earlier wins. The exception is P2: it never overrides P1, P3, P5 or P6.

| # | Principle | Consequence in this spec |
|---|---|---|
| P1 | **No undefined behaviour, no silent failure** (04 L1, L2, L6) | Integer semantics are defined by the language, not by JS. `Result` is must-use. `match` is exhaustive. Bugs panic and never fail open. |
| P2 | **Familiar to the strongest LLM priors** (Python, TypeScript, Rust, Swift; 05 D3) | Colon-and-indent blocks, `match`/`case`, `Result`/`?`, `.Variant`, `fn`, `let`/`var`, `Some`/`None`/`Ok`/`Err`, `..` and `..=` with Rust meaning, `xs[i]`. No new punctuation. |
| P3 | **Data never silently becomes code, query, markup, path or command** (04 S1–S3, L9) | Typed sinks. No `eval`. `String` never converts to a sink type. |
| P4 | **One obvious way, enforced by the formatter** (01 §3.3) | No synonyms. `kalam fmt --check` is the only accepted style. Two spellings exist only where they mean different things. |
| P5 | **What you see is what compiles** (04 U1–U7) | NFC files, no raw invisible or bidi characters, single-script identifiers, skeleton-collision errors, one skin per file. |
| P6 | **Authority is a value** (04 L12) | I/O needs capability values that originate in `main`. `print` is the one documented ambient effect (X9 in §3). |
| P7 | **Skins are a lexer and printer concern only** (02 §7.1) | One grammar, one canonical form, no per-skin word order. Round-trips are byte-exact (05 P1–P3). |
| P8 | **Semantics are target-neutral** (03 §4) | Every rule is stated without reference to JS. §22 is a *reference* lowering. |
| P9 | **Errors are a feature** (01 §3.4) | Stable codes, plain-language messages in three languages, fix-its, JSON output. |
| P10 | **Escape hatches are loud, ASCII and countable** (04 L13) | `unsafe` and `extern` are spelled the same in every skin. `kalam audit` counts them on the AST, not with grep. |

---

## 2. Design decisions and rationale

This section resolves every conflict that the judge listed, in the judge's order. Each row gives the decision, the options it rejects, and the reasons, with research citations. The relevant normative text lives in the section named in the last column.

### 2.1 Lexical, Unicode and skins

| ID | Conflict | Decision | Rejected | Rationale (citations) | Spec |
|---|---|---|---|---|---|
| DR-01 | **D7: Arabic RTL and digits** | **Locked as the consensus of all three drafts, and ratified by the founder (D7 resolved, 2026-10-25).** Files are stored in logical order. Bidi controls (including LRM, RLM and ALM) are rejected raw everywhere, strings included, and are written as `\u{…}` escapes. Base display direction comes from the skin, and UTS #55 token isolation happens only in first-party renderers. ASCII digits are the default in every skin. Arabic-Indic `٠–٩` are allowed in Arabic-skin literals, and the Arabic printer emits them only under a project setting. Persian `۰–۹` are rejected. One digit system per literal. `٫` is the decimal separator only with Arabic-Indic digits. `،` is never a decimal separator. `،` and `؟` are the only punctuation aliases, Arabic skin only. | 02 §7.5 (LRM/RLM/ALM as whitespace) | 04 U1 and 05 §1.6 overrule 02 §7.5: an invisible character that only changes display is attack surface with no semantic purpose, and UTS #55 puts display fixes in the renderer (02 §6.1). ASCII default: 02 §6.3 (Egyptians write prices, phones and code with ASCII digits). | §6 |
| DR-01a | D7 open item (a): Arabic-Indic digits in `en`/`franco` files | **Rejected** (SEC0122), with a machine-applicable fix to ASCII. | Production: accept and rewrite | One file = one skin (D6) also covers digit glyphs. The fix gives the same ergonomics, and the lexer surface stays smaller (04 U5). | §6.2 |
| DR-01b | D7 (b): exponents with Arabic-Indic digits | **Not allowed.** Hex, binary, octal and exponent forms exist only with ASCII digits (K0128). | Production: Latin `e` inside an Arabic-Indic literal | A Latin letter inside an Arabic-Indic literal is a mixed-script token, which is what 04 U3/U5 forbid. Scientific notation is rare in SaaS code. | §5.6 |
| DR-01c | D7 (c): printing of `،` and `؟` | **Fixed.** The Arabic printer always emits `،` and `؟`. Input accepts both `,`/`،` and `?`/`؟` in Arabic files. | Production: project setting | One fewer setting, and the output is the same in every Arabic project (P4). The digit glyph setting remains, because 02 §6.3 shows a real split in practice. | §6.4 |
| DR-01d | D7 (d): U+066C ARABIC THOUSANDS SEPARATOR | **Rejected** (SEC0123), with a fix-it to `_`. | unspecified in all drafts | It is a visible separator that looks like `,` and `٫`. The digit separator is `_` in every skin. | §6.3 |
| DR-03 | **Franco digit-leading aliases** | **None, anywhere.** No keyword, alias or core name starts with a digit. The natural digit-leading spellings (`5ali`, `7asab`, `2ktb`, `5alas`, `8alat`, `5arab`, `5aregi`) are kept only in the `did_you_mean` lists of `keywords.json`, so K0127 can say "Names can't start with a digit. Did you mean `hasab`?". | Security and Production: closed list with whole-run lookup | 05 P7 forbids prefix-ambiguous numbers. With no digit-leading spellings, P7 holds trivially and no edit-distance proof is needed (04 U5's mitigation becomes unnecessary). Security's own open question 2 and its strict CI already reject these aliases. | §5.6, §7 |
| DR-04 | **Identifier folding** | **No folding of identifiers** (Security). For each identifier the compiler computes `skeleton = UTS39_skeleton(arabic_fold(NFKC(id)))`, with ASCII digits only in identifiers. Two *distinct* identifiers with the same skeleton visible in one scope, or exported from one module, are **SEC0104**. A *use* that matches no declaration but has the same skeleton as exactly one visible declaration is K0401 ("not defined") with a machine fix to the declared spelling. Keywords **are** matched after `arabic_fold`. **Persian/Urdu letters ی ک ە ھ ۀ are accepted and folded** by `arabic_fold` to ي ك ه ه ه (D19), so `کتاب` and `كتاب` have one skeleton: declaring both in one scope is SEC0104, and a use spelled `کتاب` of a declared `كتاب` is K0401 with a machine fix, exactly like the hamza/alef forms. (SEC0107 is retired.) Presentation forms are rejected (SEC0106), not NFKC-folded. | Simplicity (fold = same name); Production (fold resolves, W0109); Persian/Urdu letters rejected (SEC0107, superseded by D19) | Silent folding makes two byte sequences name one variable, which defeats grep and review (04 U4, 02 §7.4 collision rule). Reject-plus-fix gives the same comfort and keeps files canonical. Folding Persian/Urdu letters into the skeleton (D19) admits Persian- and Urdu-keyboard users without adding a homoglyph hole, because the collision rule still applies after folding. | §5.3 |
| DR-17 | **Raw identifier spelling** | **Backticks:** `` `law` ``. | Security: `r#name` | `r#` collides with the `#` comment character (`x = r#note`). Backticks have no other meaning in Kalam. | §5.3 |
| DR-18 | **Skin declaration and IDs** | **File extension only:** `.kl` (en), `.ar.kl` (ar), `.franco.kl` (franco). There is no pragma. Skin IDs are `en`, `ar` and `franco` everywhere: `KALAM_LANG`, `kalam.toml`, `keywords.json` and CLI flags. Locked by the founder (D24). | Security: `.kla`/`.klf` plus an optional `#skin` pragma; `fr` as an ID | One way (P4). A pragma is a special-cased comment. `fr` is ISO 639-1 for French. | §4.1 |
| DR-20 | **Arabic synonyms; tatweel and harakat in keywords** | **No lexical synonyms in Arabic** (Production's `لو`, `بينما`, `توقف` are removed). Orthographic folding (hamza/alef, ى/ي, ة/ه) is not an alias. **Tatweel and harakat inside a keyword are rejected** (SEC0106), with an auto-fix by `fmt`. Franco keeps a closed, versioned list of *spelling variants*, all non-digit-leading, which `fmt` rewrites and `fmt --check` rejects. | Production (real aliases); Simplicity §3.1 (tolerate and strip) | 05 D4 requires zero synonyms. Franco orthography is not standardized (02 §2, §7.6), so spelling variants are the Franco analogue of Arabic hamza folding. Repositories still converge on one spelling. | §7.3 |
| DR-30 | **BOM; Arabic-Indic digits inside identifiers** | **A BOM is rejected everywhere** (SEC0100), including at offset 0, and `fmt` strips it. **Arabic-Indic digits inside identifiers are rejected** (SEC0124), with a fix to ASCII digits. | Simplicity: BOM ignored; Security: digits allowed with skeleton folding | One canonical byte form. `١` is confusable with `ا`, `l` and `1` (04 §4.1 table). Rejecting is simpler than folding and has the same safety. | §5.1, §5.3 |
| DR-31 | **Franco `walla`/`aw` semantics** | `walla` = `else` and `aw` = `or` stay **provisionally**. In Egyptian speech `walla` often means "or", so this pair goes into the 200-developer survey (02 §9), together with the و/أو pair (exception E1, §7.4). | — | The judge found a hazard that none of the drafts flagged. Changing a keyword after 1.0 is an edition change, so the survey must come before beta. | §7.4, D22 |

### 2.2 Grammar and syntax

| ID | Conflict | Decision | Rejected | Rationale | Spec |
|---|---|---|---|---|---|
| DR-05 | **Enum variant syntax in patterns and expressions** | **Leading dot `.Variant`** when an expected type exists, or qualified `Type.Variant`. A bare identifier in a pattern is *always* a binding, except for the core constructors `Some`, `None`, `Ok` and `Err`, which are reserved core names. **Leading-dot continuation is kept but restricted:** a physical line that begins with `.` joins the previous logical line only if (1) it is indented strictly deeper than the first line of that logical line, **and** (2) that logical line does not end with a block `:`. A `.X` line at the same indentation is an ordinary new statement (Production's K0121 is dropped). The first line of a block never joins its header. | Simplicity: bare identifier resolved by scope | A bare-identifier variant test turns into a binding when an import changes, which is the Swift/Rust hazard. Arabic has no case to separate them (P9 of Simplicity). The restricted continuation rule fixes Production's two defects, a value block's final `.None` and a block whose first line starts with `.X`. Both become conformance tests (§25). | §5.8, §11 |
| DR-06 | **Match arm syntax** | **`case pattern:`** (Production). `:` is the only block opener in the language. | Simplicity `pattern:`; Security `pattern =>` | Python's `match`/`case` is the strongest prior (05 D3). An arm keyword makes arm boundaries obvious when one-line and multi-line arms are mixed, and costs one keyword. D15 and P4 call for one block opener. | §8, §11 |
| DR-07 | **Inline `if` in rhs** (Production bug) | **Adopted from Simplicity, with a precise rule.** `if c: a else: b` is an expression wherever an expression is allowed, including after `let x =`, after `return`, and in arguments. At statement start and in rhs position, the parser reads the common prefix `if cond :`. If NEWLINE follows, the construct is the multi-line `if` form. Otherwise it reads one `line_stmt`. If that line_stmt is a bare expression **and the next token on the same logical line is `else`**, the construct is the inline-`if` expression. Otherwise it is an `if` statement with a one-line block. So `if x < 0: return Err(.Neg)` still parses as a statement, and the K0910 fix-it output parses. | Production: drop the inline form or the fix-it | LLMs write ternaries constantly. A Python-like one-line conditional is the strongest prior, and Production's K0910 fix already emits it. | §8.3, §10.3 |
| DR-08 | **Implicit vs explicit return** | **Explicit `return`** in block-bodied functions. A last-expression value applies only to `if`/`match` arms in value position. Expression-bodied functions use `=> expr`. | Security: implicit last-expression return | LLMs and reviewers read `return` reliably, and it removes Ruby/Rust/Python ambiguity (Production §19.2). | §12.1 |
| DR-09 | **Core vocabulary** | `fn` (not `func`). `let` + `var` (not declaration-by-assignment). **`type`** declares records (`type User:` + fields) and transparent aliases (`type UserId = Int`), and **`enum`** declares sum types. `export` (Arabic `علني`). `String`. `()` is the unit type and value. | `func`; implicit declaration; `record` keyword; `pub`; `Text`; `Nothing` | `fn` and `let` are the Rust/TS/Swift priors. With declaration-by-assignment, a typo such as `totl = …` silently declares a new variable. `type` covers records and aliases with two shapes marked by `:` and `=`, which saves a keyword. It matches the TypeScript prior, and its Arabic word `هيكل` ("structure") passes the 2,000-word identifier rule (DR-36). `record`'s Arabic `سجل` is a very common identifier ("log/record"). | §7, §9 |
| DR-10 | **Traits and `impl`; the KW_FOR collision** | **Traits are kept** (coherent, orphan rule, no trait objects). **A trait implementation is written `impl Type as Trait:`**, for example `impl Money as Show:`. `for` is never reused. Inherent methods use `impl Type:`. | Simplicity: no traits; Security/Production: `impl Trait for Type` (reuses KW_FOR) | Custom `Show`, `Ord`, `From` and user-defined `Template` tags need traits (Production 8.7, Security 16.8). `impl Show for Money` would print `نفذ Show لكل Money` ("for each Money") in Arabic, which breaks the 1:1 concept map (02 §7.2). `as` already means "in the role of" (imports, pattern binding). Arabic `بصفة` ("in the capacity of") and Franco `ka` ("as") read naturally in all three uses: `نفذ مال بصفة Show`, `naffez flous ka Show`. K0919 rewrites the Rust form automatically. | §9.7 |
| DR-11 | **Namespaces** | **One namespace per scope** for every kind of name: types, enums, aliases, traits, functions, values and module aliases. Constructors are the type's name. In Arabic, `world: World` is written **`العالم: عالم`**, because the definite article is the natural way to name a value. Without a glossary, the converter keeps the identifier: `رئيسي(world: عالم)`. | Production: separate namespaces plus K0412 (self-contradictory) | One rule, with no exception for records versus enums. Arabic grammar already marks values with `ال` (Production's own style guide). `fmt` and the glossary checker enforce that no display name collides (SEC0108). | §13.3, §28 |
| DR-15 | **Closures and multi-line lambdas** | Closures **may not capture a `var`** (K1110, fix: `let snapshot = v`). **There are no multi-line lambdas and no block islands.** A lambda body is one expression. For a multi-statement callback, declare a **local `fn`** inside the function body. It can capture `let` bindings: `fn move(tx: Db) -> Result[(), DbError]:` … then `db.transaction(move)`. | Simplicity: snapshot capture; Production: block islands | Capturing a mutable variable surprises JS and Python developers either way, so forbidding it is clearest (04 L10/L11). Islands were Production's most complex lexer rule (its own open question 8). Python has no multi-line lambdas and LLMs handle nested `def` well. Bracket suppression of newlines now has no exceptions. | §5.8, §12.3 |
| DR-24 | **Ranges** | `a..b` excludes `b`. `a..=b` includes it. | Simplicity: inclusive `..` only | Rust and Swift priors read `0..n` as exclusive (05 D3). An inclusive `..` would be an LLM trap. | §10.5 |
| DR-25 | **Indexing and explicit type arguments** | **`xs[i]` is kept** (panics when out of bounds; `xs.get(i)` returns `Option`). `xs[i] = e` is value-semantic assignment. **Postfix `[…]` is always indexing.** v0.1 has **no explicit type-argument syntax in expressions**. Type arguments come from inference, from the expected type (`let u: User = json.decode(s)?`) or from a typed `let`. Generic arguments are written only in type positions. | Production: reparse `f[T](x)` as type application; Simplicity/Security: no indexing | LLMs and JS/Python developers type `xs[i]` on day one (Simplicity's own open question 9). Dropping call-site type arguments removes Production's reparse defect (`f[fn(Int) -> Int](x)` was unparsable) at almost no cost, because return-type inference covers the cases. | §10.8 |
| DR-26 | **Tuples and `t.0.1`** | **Tuples are kept**, with positional access `t.0`. **Lexer rule:** immediately after a `.` token, a digit run lexes as a decimal `INT_LIT` member index only, never as a float and never with an exponent. So `t.0.1` lexes as `t . 0 . 1` (Rust's fix). | Simplicity: no tuples | `enumerate()`, `zip()` and map iteration produce pairs, and destructuring `let (a, b) = …` is a strong prior. | §5.6 |
| DR-33 | **One-line blocks** | **Allowed**, for one simple statement on the header line (`if x < 0: return Err(.Negative)`, `case .Circle(r): r * r`). The parser classifies colons: a `:` at bracket depth 0 that ends a line is always a block colon. | Security: forbidden | Production and Simplicity both rely on it, and it is a Python prior. `fmt` keeps a one-line block only if the line fits in 100 columns. | §5.8, §8 |
| DR-34 | **Parser-class claims** | The grammar is **LL(1) after left-factoring at four listed points, with no backtracking**: (1) `if` at statement or rhs start (decided after the common prefix `if cond :`, by NEWLINE vs. not, then by `else` vs. NEWLINE); (2) `(`: unit vs. group vs. tuple (decided after the first expression by `)` vs. `,`); (3) a statement that starts with an expression vs. an assignment (parse the postfix expression, then the next token decides, and the target must be a place, K0710); (4) a named vs. positional argument (parse an expression; a bare `IDENT` followed by `:` becomes a label). | Simplicity "LL(1)-friendly", Security "LL(2)" (both inaccurate) | The claim is stated so the tree-sitter grammar and the Rust parser can be checked against it (03 §5). | §8.4 |
| DR-35 | **Default parameters, map literal, `pass`** | **Default parameter values are allowed** (evaluated per call, and they may not refer to other parameters). **Map literal `{k: v}`**, where `{}` needs an expected type. **`pass`** is the keyword for an empty body or arm. | Simplicity/Security: no defaults; Security: no map literal | `access:`-style named options and SaaS configuration objects need defaults. The map literal is a Python/JS prior. An empty arm needs a spelling, and `pass` is Python's. | §8, §12.1 |

### 2.3 Types and semantics

| ID | Conflict | Decision | Rejected | Rationale | Spec |
|---|---|---|---|---|---|
| DR-02 | **Int semantics** | **`Int` = [−2⁶³, 2⁶³−1] (a true 64-bit signed integer) on every backend, trapping on overflow** (locked by the founder, D18). `BigInt` is the arbitrary-precision opt-in. **JS representation:** a canonical dual representation, where an `Int` is a JS `number` exactly when its value is in the safe range ±(2⁵³−1), and a `bigint` otherwise (§22). **Decode rules:** a Postgres `bigint` always fits; a JSON integer outside the i64 range decoding into `Int` is a decode error (`DecodeError.OutOfRange`), never a silent float. JSON integers are decoded exactly, never through a lossy JS `number` (§18.3). Declare the field `BigInt` to accept larger values. An npm value crossing into `Int` must be a safe-integer `number` or an in-range `bigint` (§19.2). The C backend uses `int64_t` with checked arithmetic, so results are identical. | 53-bit safe range with a plain JS `number` (the provisional v0.1 default, superseded by D18) | One integer type that means the same thing on every backend and matches Postgres `bigint`, Rust/Go/Java/C# `i64`/`long`, and the future C/WASM backend with no range checks (03 §4, 04 L2). 03 §4's "never just number" is met. The cost is confined to the JS backend's slow path: arithmetic on two `number`s stays a plain double operation plus one `Number.isSafeInteger` check (§22). **Performance target** (not a decision gate): the dual representation costs ≤ 10% on the 05 D6 corpus, and a regression is a JS-backend bug, not a reason to change `Int`. | §9.2, §22 |
| DR-12 | **Error conversion in `?`** | **`From` + `AnyError` + `.context()`** (Production). `?` converts with an explicit `impl F as From[E]`. `AnyError` boxes any `E: Error` and is not dynamically typed: it can only be inspected with `.downcast() -> Option[E]` against an expected type. | Simplicity: auto-wrap into core `Error`; Security: explicit `Into` only, no any-error type | SaaS handlers need ergonomic `?` across DB, JSON and HTTP errors (06 §2). `AnyError` is an opaque existential with a typed downcast, so it does not reintroduce `any` (Security §8.2's concern). | §14.3 |
| DR-13 | **Suspension of stored functions** | **Suspend by default, with `@sync` opt-out** (Production). Function-typed parameters are effect-polymorphic. Function types inside data are suspending unless written `@sync fn(…)`. The suspension bit of exported functions is part of the API: sync → suspending is a semver-major change. | Simplicity: escaping functions always suspend; Security: whole-program per-record inference | Modular and sound. Separate compilation stays possible, and a library's lowering never depends on how downstream code uses it. | §15.3 |
| DR-14 | **Cancellation semantics** | **An unobservable "cancelled" unwind at the next suspension point** (Production). It runs no user code. The runtime rolls back an open DB transaction and releases locks, as for a panic. | Security: suspending call returns `Err(.Cancelled)` | With an `Err` value, user code can swallow cancellation and keep working after it was cancelled. | §15.7 |
| DR-16 | **Escape hatches** | `unsafe` is a fixed-ASCII keyword usable only as a pseudo-module prefix: `unsafe.raw_html(s)`, `unsafe.raw_sql(s)`, `unsafe.raw_url(s)`, `unsafe.js_cast(v)`. **`extern` is fixed ASCII in every skin** (Production's `خارجي`/`kharegi`/`5aregi` are removed). `Secret.expose()` is an ASCII stdlib method counted as an escape hatch. `kalam audit` counts all of them on the AST. | Simplicity: `unsafe_` name prefix; Security: `unsafe` blocks and functions | 04 L13: every escape hatch uses one ASCII spelling identical in all skins and is countable. v0.1 has no raw-memory code, so `unsafe` blocks are unnecessary. A pseudo-module keeps every escape hatch a single greppable token followed by a named operation. | §17.1, §19 |
| DR-21 | **Canonical form and glossary** | **KCF** (Security) is the unit of identity: a skin-free CST serialization (keywords as concept IDs, identifiers as NFC code points, numbers as values, trivia kept). P1 compares KCF hashes. **The glossary is a bijective, display-only map** over canonical names. LSP display, review views and docs can apply it, and runtime names (JSON keys, DB columns, JS exports, `Show` of field names) always use canonical names. **"Canonical text" is the formatted English skin**, the LLM-facing form. Writing glossary output to disk is a *rename refactoring* (`kalam rename --glossary`), not a skin conversion. It produces a new KCF, and `@name` keeps wire names. | Simplicity: AST hash; Production: glossary conversion changes the AST and needs `@name` | 05 P1 requires identical hashes for converted programs. 04 U7 requires a canonical serialization for signing. Production's "canonical = English print" is kept for LLMs (06 §7.2). | §4.4, §4.5 |
| DR-22 | **Numeric literal rules** | `1e5` is a **legal Float**. Leading zeros are **rejected** (K0125). An integer literal may be checked against an expected `Float`, `Decimal` or `BigInt`. **A float literal may be checked exactly against an expected `Decimal`** (`let p: Decimal = 19.99` is exactly 19.99, a compile-time conversion). There is no `dec"…"` tag. `Decimal` is **34 significant digits** (IEEE 754 decimal128). **`Decimal` has no `/`.** Use `a.div(b, scale:, rounding:)`. Exponent signs are consumed only in decimal runs, so `0x1e+5` is `0x1e` `+` `5`. | Production: `1e5` is K0124; Simplicity: `dec"…"` only, 28 digits; Security: `/` panics when inexact | `1e5` is a float in Python, JS and Rust (05 D3). Exact literal checking needs no second literal syntax (P4). Rounding must be explicit for money (04 L14, 01 COBOL lesson). | §5.6, §9.2 |
| DR-23 | **Float equality and order** | **`Float` is not `Eq`, `Ord` or `Hash`.** `==`, `!=`, `<`, `<=`, `>`, `>=` on two `Float` values are built-in IEEE operators specific to `Float`. Generic code bounded by `Eq`/`Ord`/`Hash` never accepts `Float`, and records with `Float` fields do not derive them. Use `.total_cmp()` to sort and `.near(other, tol:)` to compare. `Float` patterns are not allowed. | Simplicity: Float is Eq + Hash (NaN keys); Production: "not Ord, yet has `<`" (contradictory) | NaN is not reflexive. Making the IEEE operators built-in and specific to `Float` removes Production's contradiction without introducing a `PartialEq`/`PartialOrd` trait pair. | §9.2, §9.8 |
| DR-27 | **UFCS and method resolution** | **No UFCS.** `x.m(…)` considers the inherent methods of `x`'s type **and** the methods of every trait implemented for that type anywhere in the program (coherence makes this a fixed set). **Exactly one candidate must exist, or it is K1103.** Disambiguate with `Trait.m(x)`. Imports never affect method resolution. | Simplicity: fallback to a function in the current scope; Production: trait methods only if the trait is imported | Import-independent dispatch. A dependency upgrade can turn a resolved call into an ambiguity *error*, but it can never silently re-route the call (Security 7.8). | §12.2 |
| DR-28 | **Shadowing** | **No shadowing at all.** A new binding may not reuse any name visible at that point: enclosing locals, parameters, module items, imports or core names (K0405). | Simplicity: same function only; Production: locals may shadow module items (W0706) | With one namespace (DR-11), a single rule is simplest. It also rules out the Arabic fold/skeleton surprises (04 U4). Arabic values use `ال`, English values use lowercase names, so collisions are rare and the fix-it suggests a rename. | §10.1 |

### 2.4 Security, sinks and web

| ID | Conflict | Decision | Rejected | Rationale | Spec |
|---|---|---|---|---|---|
| DR-19 | **Core names translated per skin** | **All 19 core names are translated in every skin** (locked by the founder, D21), including the primitive types (Production's set), so Arabic lines do not mix scripts for everyday types. Word choices: None = `لا_شيء` (not `عدم`, one letter from `عدد` = Int). true = `صح`. type = `هيكل`. match = `حسب` (`طابق` also means "floor"). continue = `استمر` (`تابع` also means "subsidiary"). case = `عندما` ("when"). Rejected: `حالة` ("state") and `عند`, which is one letter from `عدد`. main = `رئيسي` / `ra2isi`. Option and Result constructors are capitalized `Some`/`None`/`Ok`/`Err`. | Security: 6 names, types ASCII, lowercase `some`/`ok`/`err` | Lowercase constructors would ban the locals `ok` and `err` and make a bare `ok` in a pattern ambiguous. Mixed-script Arabic lines force a keyboard switch (02 §6.4). | §7.2 |
| DR-29 | **Tag names and the tag set** | `sql"…"`→`Sql`, `html"…"`→`Html`, `cmd"…"`→`Command` (argv only, never a shell), `url"…"`→`Url`, `path"…"`→`Path` (holes must be `PathSegment`), `regex"…"`→`Regex` (RE2). User-defined tags come from `impl T as Template` plus `@tag("name")`. Escape hatches: `unsafe.raw_sql`, `unsafe.raw_html`, `unsafe.raw_url`. **There is no raw `Command`.** | `sh`/`shell`, `re` | `cmd`/`Command` is report 04 §3.2's own naming, and unlike `sh` it does not suggest a shell is involved (Security X10). `regex` matches its type name. | §17 |
| DR-32 | **Server-driven UI directives (D11)** | **Both layers, compiler-checked inside `html"…"`.** (1) **Live components** (typed, TEA-style, from Simplicity): `state`, `update(State, Msg) -> State` and `view(State) -> Html`. Event attributes `k-click`, `k-submit`, `k-input` and `k-change` take a hole whose type must derive `Message` (K1616). The value crosses the wire as an **opaque, signed, CSRF-bound token** (Security §16.4.1), never code. (2) **Hypermedia directives** for plain actions (Production): `k-get`, `k-post`, `k-target` and `k-swap`. Their URL holes must be `Url`. The `k-` attribute namespace is reserved, and unknown `k-*` attributes are K1617. | library-defined directives | Checking in the compiler keeps the security property local and testable (04 S3). The typed message model gives the strongest LLM prior (Elm/TEA) without new syntax (06 §1.2). | §20.3 |
| DR-36 | **Arabic keyword choice rule** (graft) | CI keeps a list of the 2,000 most frequent Arabic identifier words. A keyword or core name that appears in it fails CI unless a written justification is recorded in `skins/ar-justifications.toml`. v0.1 justifications: `نص`, `قائمة` and `نتيجة` are core names that are also common nouns. Arabic code names values with `ال` (`النص`), so the collision is mostly avoided. | — | Security §5.2. This rule is why the table uses `هيكل` over `نوع`, `حسب` over `طابق`, `استمر` over `تابع`, and `عندما` over `حالة`. | §7.4 |

---

## 3. Deviations from the research reports

A locked decision beats a research report. Where reports disagree with each other, this spec records which one it follows.

| # | Research said | This spec says | Why |
|---|---|---|---|
| X1 | 02 §7.1: English keywords are always accepted in every skin. | **Rejected.** An English keyword in an `ar` or `franco` file is K0201, with a machine fix to the file's keyword. The LSP converts on paste. | D6 (one file = one skin); 04 §4.2. |
| X2 | 03 §4 and 02 §8: `async`/`await`. | No `async` or `await` in any skin. Suspension is inferred (§15). | D12. |
| X3 | 02 §8: `try`/`catch`/`finally`/`throw`. | No exceptions. `Result` + `?`, and panic for bugs. | D16; 04 L6. |
| X4 | 02 §8 examples use braces. | Colon + significant indentation. | D15. |
| X5 | 03 §4.1 rule 4 and 04 U5: digit-leading identifiers are legal in Franco under restrictions. | **No identifier, keyword, alias or core name starts with a digit, in any skin.** | 02 §7.6; 05 P7. This removes the spoofing surface instead of mitigating it. |
| X6 | 02 §7.5: LRM/RLM/ALM allowed as whitespace. | Rejected raw everywhere, including strings and comments. | 04 U1; 05 §1.6. |
| X7 | 02 §7.4: fold tatweel/harakat and hamza/ى/ة in identifiers. | Identifiers are never folded. Folding is used to compute skeletons (SEC0104). Tatweel and harakat are rejected in identifiers and keywords. | Silent folding defeats grep and review (DR-04). |
| X8 | 02 §7.4 step 5: fold Persian ی/ک. | **Followed, as a comparison fold** (D19). ی ک ە ھ ۀ are legal in identifiers and keywords. `arabic_fold` maps them to ي ك ه ه ه for keyword matching and identifier skeletons. Identifiers are still never rewritten, and a collision after folding is SEC0104 (DR-04). | Founder decision D19 (Persian/Urdu-keyboard users are in scope). The linguist review of the fold list (Q14) continues. |
| X9 | 04 L12: no ambient authority. | `print` (stdout) and `log.*` (stderr) are ambient. A **library** package that calls them must declare `console = true` in its manifest, which shows in the capability diff. | Hello-world stays one line. Stdout goes to the operator. Locked by the founder (D20). |
| X10 | 04 §3.2: shell type `Command`, tag `cmd"…"`. | Followed exactly (`cmd"…"` → `Command`). Security draft's `shell"…"` is not used. | Report naming. |
| X11 | 02 §8: `repeat`, `const`, `from`, `default`, `input`, `try`, `await`. | Dropped. `repeat` → `for _ in 0..n`. `const` → module-level `let`. `default` → `case _:`. `input` is a capability method. | Fewer keywords, fewer cross-skin collisions (05 D4). |
| X12 | 01 §3.3: identifiers normalized with NFC. 02 §9 Q1b: NFKC. | The **file** must be NFC. Identifiers are **compared** through NFKC only inside the skeleton function. They are never rewritten. | 04 U2 permits both. Keeping spelling visible is safer (DR-04). |
| X13 | 02 §7.6: Franco alias lists include digit-leading forms. | Kept only as "did you mean" hints (`keywords.json` `did_you_mean`). | DR-03. |
| X14 | 06 §1.2: client islands with keyword reactivity (`state`/`derived`/`effect`). | Not in v0.1. Server-driven live components cover v0.1 (§20.3). Client islands are a later web-kit feature, with no new keywords. | D11; keyword budget (05 D4). |

---

## 4. Source files, skins, canonical form and conversion

### 4.1 Files and skin declaration

- A source file is UTF-8 in Unicode **NFC**. It has no BOM and uses LF line endings. CRLF is accepted and `fmt` rewrites it. A lone CR, U+2028 or U+2029 is an error (SEC0113). Encoding errors are listed in §5.1.
- **The file extension declares the skin** (DR-18). There is no pragma.

| Skin ID | Skin | Extension | Example |
|---|---|---|---|
| `en` | English | `.kl` | `users.kl` |
| `ar` | Arabic (فصحى) | `.ar.kl` | `users.ar.kl` |
| `franco` | Egyptian Franco | `.franco.kl` | `users.franco.kl` |

- The module name never includes the skin suffix. If `users.kl` and `users.ar.kl` both exist in one directory, that is K0301. Converting a file therefore never changes any `use` line anywhere.
- A package may mix skins file by file. Callers never know or care which skin a module is written in.

### 4.2 Pipeline

```
bytes ─► source-text checks (§5.1) ─► skin lexer (keywords.json) ─► lossless CST (trivia, spellings)
                                                                     │
                       kalam fmt / kalam convert --to X ◄────────────┤  printer: CST → text in any skin
                                                                     ▼
                                       KCF (skin-free; keywords = concept IDs) ──► hash, sign, registry
                                                                     ▼
                   name resolution ► type and suspension inference ► capability and sink checks
                                                                     ▼
                                   typed IR ► JS emitter (+ source maps, .d.ts)
```

### 4.3 What conversion changes

`kalam convert --to en|ar|franco FILE` re-prints the CST in another skin.

- **Translated:** keywords and core names (§7), the two Arabic punctuation aliases (§6.4), and digit glyphs (Arabic target only, per project setting, §6.2).
- **Never translated:** identifiers, string and tagged-literal text, comments, blank lines, attribute names, module paths, stdlib names and `unsafe`/`extern`.
- **Code inside interpolation holes (`{expr}`) is code**, and it converts like any other code. The literal text around the holes does not.
- **Escaping:** if an identifier would lex as a keyword or core name of the target skin, the printer writes it in backticks (`` `law` ``). "Would lex as" uses the target skin's own lookup (§5.4): for `franco` it includes every spelling variant (`low`, `foot`, `wala` …), and for `ar` it compares after `arabic_fold` (an identifier `اذا` or `فى` in an English-skin file is escaped). Positions where a reserved word is already an identifier (§5.3 rule 8) need no backticks. The reverse conversion removes the backticks when they are no longer needed. The LSP shows hint H0202 when a declared identifier equals another skin's reserved word.
- **Guarantees** (05 P1–P3):
  - **G1 (identity):** `kcf(parse_X(s)) == kcf(parse_Y(print_Y(parse_X(s))))` for all skins X, Y.
  - **G2 (fixpoint):** for a fixed project configuration, `print_X(parse_Y(print_Y(parse_X(s)))) == print_X(parse_X(s))` byte for byte. Every setting that affects output (only `[skin.ar] digits`) is project-level in `kalam.toml`, never per file.

### 4.4 Kalam Canonical Form (KCF)

KCF is a skin-free serialization of the CST (04 U7, DR-21):

- keywords and core names → concept IDs (`KW_IF`, `CORE_INT`, from `keywords.json`);
- punctuation aliases → their ASCII concept (`،` → `PUNCT_COMMA`);
- numeric literals → their kind plus exact value (digit glyphs are not recorded);
- identifiers → their exact NFC code points (no folding);
- strings and comments → kept byte for byte. For string and tagged literals this applies to the literal text **outside** interpolation holes. The code inside each `{…}` hole is serialized like any other code (keywords as concept IDs, `،` as `PUNCT_COMMA`, numbers as values), because conversion translates it (§4.3). Otherwise `"{a and b}"` and `"{a و b}"` would hash differently and P1 would fail.

**KCF records no layout that `kalam fmt` may change** (format `KCF2`, provisional: FOUNDER-QUEUE Q8). So `kcf(fmt(x)) == kcf(x)` holds for every file that parses (D7 formatter idempotence), and reformatting never changes a program's identity:

- **Blank lines** are recorded only as "at least one blank line here": a run of blank lines counts as one. They are recorded only where `fmt` keeps them, which is between two statements or members of one block, and between comments there. They are not recorded at the start of a block (before its first comment or statement), after an attribute line, before `else`, inside brackets, at the top level of the file (`fmt` puts exactly one blank line between items itself, §21.1 rule 4) or at the end of the file.
- **A trailing comma** before a closing bracket is not recorded (`f(a, b,)` = `f(a, b)`, `[1, 2,]` = `[1, 2]`). The exception is a one-element tuple `(x,)`, where the comma is what makes it a tuple.
- **A one-line block** (`if x: stmt`, `fn f() -> Int: return 1`, `case 1: f()`) serializes identically to its indented form. An inline `if` *expression* (`if c: a else: b`) is not a block and keeps its form.
- Spaces, indentation bytes (INDENT/DEDENT carry the structure), CR, line breaks inside brackets and a missing final newline are not recorded either.

The serialization starts with the header `KCF2`. A KCF hash is only comparable with hashes of the same header version.

**The KCF hash (SHA-256) is the unit of identity.** It is what gets signed, what `kalam.lock` records, what the registry stores, and what parity gate P1 compares. The registry renders any skin on demand. Code-review tools SHOULD let a reviewer pick a skin and MUST show the KCF hash, so the PR that was reviewed is provably the PR that was merged.

**Canonical text** is a separate idea. It is the formatted English skin: what LLMs are asked to write, what `llms.txt` teaches, and what docs show first (§21.1).

### 4.5 Identifier glossaries (display only)

Converting keywords alone shows Arabic keywords around English identifiers. That is lossless, but it is not what an Arabic-reading team wants to look at. A project can therefore add `names.toml`, a **bijective display map from canonical names to display names**:

```toml
[names]                 # canonical = { ar = …, franco = … }
User     = { ar = "مستخدم", franco = "mostakhdem" }
name     = { ar = "الاسم",  franco = "el_esm" }
world    = { ar = "العالم", franco = "el_donya" }
```

1. **The glossary is applied only by display surfaces:** the LSP's display mode, `kalam view --skin ar --glossary`, review renderers and generated docs. It never changes files on disk, the KCF, or behaviour.
2. **Runtime-visible names are always canonical names.** These are JSON keys, DB column mapping, exported JS symbols and the field names that `Show` prints (05 P5). **Arabic canonical names are valid wire names** (D23): a project that writes Arabic identifiers natively gets Arabic JSON keys and DB columns, and that is allowed freely. There is **no default lint** requiring ASCII wire names or `@name` on `Encode`/`Decode`/`Row` types. A team that wants ASCII keys for outside API consumers writes `@name("…")` on those fields.
3. The map must be bijective per skin. Every display name must pass the identifier rules (§5.3). No display name may have the same skeleton as another identifier, keyword or core name in scope (SEC0108).
4. **Writing glossary output to disk is a rename refactoring**, not a conversion: `kalam rename --glossary names.toml`. It produces a new KCF, and it inserts `@name("…")` on fields of `Encode`/`Decode`/`Row` types so that wire names do not change.

Most examples in §28 are shown in glossary display mode.

---

## 5. Lexical structure

### 5.1 Source-text rules (the Trojan Source rules)

These rules apply to **all** source text: code, comments and string literals (04 U1, U9; 05 §1.6). They are errors that no attribute can suppress. Every SEC message names the code point and its Unicode name, shows the line with the character made visible (`⟨U+202E RIGHT-TO-LEFT OVERRIDE⟩`), and offers the `\u{…}` escape as a fix-it when the character is inside a string.

| Class | Code points | In code | In comments | In string literals | Code |
|---|---|---|---|---|---|
| Invalid UTF-8; BOM | — ; U+FEFF anywhere, including offset 0 | reject | reject | reject (escape only) | SEC0100 |
| Not NFC | — | reject (`fmt` fixes) | reject | reject | SEC0101 |
| C0/C1 controls other than LF, CR | U+0000–001F, U+007F–009F | reject | reject | escape only | SEC0102 |
| Non-ASCII whitespace | NBSP U+00A0, U+1680, U+2000–200A, U+202F, U+205F, U+3000 | reject (fix-it: space) | reject | allowed raw (it is text), lint W0114 | SEC0103 |
| Tab | U+0009 | reject (K0103) | allowed | escape `\t` only | K0103 |
| Bidi embeddings, overrides, isolates | U+202A–202E, U+2066–2069 | reject | reject | escape only | SEC0110 |
| Bidi marks | U+200E LRM, U+200F RLM, **U+061C ALM** | reject | reject | escape only | SEC0110 |
| Zero-width and invisible format | U+200B, U+200C, U+200D, U+2060–2064, U+180E, U+00AD, U+034F, U+115F, U+1160, U+3164, U+FFA0 | reject | reject | escape only, **except ZWJ U+200D directly between two `Extended_Pictographic` characters** (emoji sequences), which is allowed raw | SEC0111 |
| Variation selectors | U+FE00–FE0F, U+E0100–E01EF | reject | allowed only directly after an emoji | allowed only directly after an emoji; otherwise escape | SEC0111 |
| Tag characters, private use, unassigned, noncharacters | U+E0000–E007F, PUA, Cn | reject | reject | escape only | SEC0112 |
| Line/paragraph separators, lone CR | U+2028, U+2029, CR not followed by LF | reject | reject | escape only | SEC0113 |

- **The only whitespace is U+0020 SPACE.** Newline (LF) is a token. This stops an invisible space from splitting or joining tokens.
- **Unicode version is pinned per compiler release** (v0.1: Unicode 16.0). Unassigned code points in that version are SEC0112.
- *Rationale for strings:* legitimate Arabic text never needs a raw bidi control, because the Unicode Bidirectional Algorithm orders it implicitly. A program that really needs one writes `"\u{200F}"`, which is visible in every editor and diff.

### 5.2 Character classes

| Class | Definition |
|---|---|
| `LETTER_LATIN` | `XID_Start` ∩ Script=Latin |
| `LETTER_ARABIC` | `XID_Start` ∩ Script=Arabic, **excluding**: U+0640 tatweel; harakat U+064B–065F, U+0670; Quranic marks U+06D6–06ED; presentation forms U+FB50–FDFF, U+FE70–FEFF. The Persian/Urdu letters U+06CC ی, U+06A9 ک, U+06D5 ە, U+06BE ھ and U+06C0 ۀ **are** included; `arabic_fold` folds them for comparison (§5.3, D19) |
| `LETTER_OTHER` | `XID_Start` in any other single script (Greek, Cyrillic, CJK, …). Allowed, but a project may restrict scripts with `[lint] scripts = ["Latin", "Arabic"]` |
| `ASCII_DIGIT` | `0`–`9` |
| `ARABIC_DIGIT` | `٠`–`٩` (U+0660–0669). Valid only in numeric literals of `ar` files (§6.2) |
| `PERSIAN_DIGIT` | `۰`–`۹` (U+06F0–06F9). Always rejected (SEC0121, fix-it to `٠–٩` or ASCII) |
| `PUNCT` | the ASCII operators of §5.9, plus `،` and `؟` in `ar` files only |
| anything else outside strings and comments | K0105, naming the code point |

### 5.3 Identifiers

```
IDENT       = ID_START { ID_CONTINUE } | "`" ID_START { ID_CONTINUE } "`" ;
ID_START    = LETTER_LATIN | LETTER_ARABIC | LETTER_OTHER | "_" ;
ID_CONTINUE = ID_START | ASCII_DIGIT | XID_Continue marks of the identifier's own script ;
```

1. **UAX #31 base** (`XID_Start`/`XID_Continue`) in the pinned Unicode version.
2. **Never starts with a digit,** in any digit system and any skin (X5). `3ala` is K0127. Write `ala` or `_3ala`.
3. **Single script** (UTS #39 "Single Script", 04 U3). Each identifier is one script plus Common (`_`, ASCII digits). `pаypal` with a Cyrillic `а` is SEC0105. `user_اسم` is SEC0105.
4. **Forbidden inside identifiers** (SEC0106, auto-fix removes or replaces them): tatweel, harakat, Quranic marks, ZWJ/ZWNJ, presentation forms. **Persian/Urdu letters** (ی ک ە ھ ۀ) are allowed (D19). They are never rewritten, but they fold to `ي`, `ك`, `ه` for every comparison (keyword lookup and skeletons, below), so a collision with the Arabic spelling is SEC0104.
5. **ASCII digits only** after the first character. Arabic-Indic digits in identifiers are SEC0124 (fix: ASCII), because `١` is confusable with `ا`, `l` and `1` (DR-30).
6. **Case-sensitive.** No case or style folding (01 §3.2). Case never carries *meaning* in Kalam (DR-11).
7. **Raw identifiers:** backticks make any word an identifier (`` `law` ``). The backticks are not part of the name. `fmt` removes unneeded backticks.
8. **Contextual freedom:** a reserved word is an ordinary identifier after `.` (field or method access), as a named-argument label, and as a record field name in a declaration. So `contract.law`, `x.type` and `f(match: 1)` are legal in every skin, which cuts down on backtick escapes after conversion.
9. `_` alone is the wildcard and can never be read. A leading `_x` silences unused-binding warnings.

**Skeleton and collision rule (SEC0104, DR-04).** For every identifier the compiler computes

`skeleton(id) = UTS39_skeleton( arabic_fold( NFKC(id) ) )`

where `arabic_fold` maps أ إ آ ٱ → ا, ى → ي, ة → ه, ؤ → و, ئ → ي, and the Persian/Urdu letters ی (U+06CC) → ي, ک (U+06A9) → ك, ە (U+06D5) → ه, ھ (U+06BE) → ه, ۀ (U+06C0) → ه (D19). The UTS #39 skeleton maps cross-script lookalikes (`ا`/`l`/`1`, `٥`/`o`, Latin/Cyrillic). It is an error if two **distinct** identifiers with the same skeleton are both visible in one scope or are both exported from one module. The message shows both declarations side by side with their code points. For example, `على` and `علي` declared in one function collide, and so do `كتاب` and `کتاب` (Arabic kaf vs. Persian keheh).

**Uses are never folded.** If a use matches no visible declaration exactly but has the same skeleton as exactly one, the error is K0401 ("`اكتب` is not defined"), with a machine fix to the declared spelling (`أكتب`).

### 5.4 Keywords and core names in the lexer

- The lexer loads `keywords.json`. A word run is checked against the **file's skin only**: keywords and core names are matched exactly in `en`, after `arabic_fold` in `ar` (so `اذا` and `إذا` are one keyword, `لکل` with Persian keheh is `لكل`, and `fmt` writes the canonical spelling), and against the canonical spelling plus closed alias list in `franco`.
- Reserved words of other skins are ordinary identifiers. They become backtick-escaped only if the file is converted (§4.3).
- An English keyword or core name in an `ar` or `franco` file is K0201, with a machine fix to the file's spelling (X1). The same applies in the other direction. **K0201 replaces the error the word would otherwise cause**: it is reported only where that word, read as an identifier, would be a syntax error (for example `if x:` in a Franco file) or an undefined name (for example `print(…)` in an Arabic file). A declared identifier that happens to equal another skin's reserved word is legal (the bullet above), and the LSP shows H0202.
- Tatweel or harakat inside an Arabic keyword are SEC0106, with an auto-fix (DR-20).
- **Cross-skin hint for Arabic `و`:** an undefined identifier that begins with `و` and whose remainder is a visible identifier, a keyword or a core name (`وليس`, `وصح`) gets the hint *"`وس` is not defined. Did you mean `و س` (`and` followed by `س`)?"*, with a machine fix that inserts the space.

### 5.5 Comments

- `#` starts a line comment that runs to end of line. `##` starts a doc comment (Markdown), which attaches to the next item. There are no block comments, so a comment can never hide code across lines.
- Comments are CST trivia, preserved byte for byte across conversion, and never translated. §5.1 applies to them.

### 5.6 Numeric literals

```
INT_LIT    = DEC_INT | HEX | BIN | OCT ;
DEC_INT    = "0" | NZ_DIGIT { [ "_" ] ASCII_DIGIT }
           | "٠" | NZ_AR_DIGIT { [ "_" ] ARABIC_DIGIT } ;          (* ar files only *)
HEX        = "0x" HEXDIG { [ "_" ] HEXDIG } ;                       (* ASCII only; lowercase prefix *)
BIN        = "0b" BINDIG { [ "_" ] BINDIG } ;
OCT        = "0o" OCTDIG { [ "_" ] OCTDIG } ;
FLOAT_LIT  = DEC_ASCII "." ASCII_DIGIT { [ "_" ] ASCII_DIGIT } [ EXP ]
           | DEC_ASCII EXP
           | DEC_ARABIC "٫" ARABIC_DIGIT { [ "_" ] ARABIC_DIGIT } ;  (* ar only; no exponent *)
EXP        = ( "e" | "E" ) [ "+" | "-" ] ASCII_DIGIT { ASCII_DIGIT } ;
```

**The lexing rule (DR-03, from Simplicity): a digit always starts a number.** When the lexer meets a digit (an ASCII digit, or an Arabic-Indic digit in an `ar` file), it reads the maximal run of letters, digits and `_`, with two extensions:

- it absorbs one `.` (or `٫`) only if a digit immediately follows it and the run so far is a plain decimal integer;
- it absorbs one `+` or `-` only directly after an `e`/`E` in a **decimal** run (never after a `0x`/`0b`/`0o` prefix) and only when a digit follows.

The whole run must then match `INT_LIT` or `FLOAT_LIT`. Otherwise it is **K0127**, *"A number can't be glued to letters."* The message includes the `did_you_mean` spelling from `keywords.json` when one matches ("Names can't start with a digit. Did you mean `hasab`?"), or a fuzzy suggestion (§7.3).

Consequences:
- `1..10` lexes as `1` `..` `10`. `1.5e-3` is one float. `0x1e+5` is `0x1e` `+` `5`. `3ala`, `7f`, `1e5x` and `0x1Fg` are K0127 in every skin.
- **After a member-access `.`** (the previous token is `.`), a digit run lexes as a decimal `INT_LIT` member index only. It never absorbs a `.` or an exponent. So `t.0.1` lexes as `t` `.` `0` `.` `1` (DR-26).
- Leading zeros (`007`) are K0125. `0`, `0.5` and `0x0F` are fine.
- **One digit system per literal** (SEC0120). `1٢3` is rejected. Arabic-Indic literals use `٫` as the decimal separator. ASCII literals use `.`. Hex, binary, octal and exponents are ASCII only (K0128).
- A literal outside the range of its type is a compile error (K0126), not a wrap. For `Int` the range is [−2⁶³, 2⁶³−1] = [−9223372036854775808, 9223372036854775807] (D18). The lexer computes the exact value but never range-checks, because an integer literal may be a `BigInt` of any size and `-9223372036854775808` reaches it without the sign. The **type checker** reports K0126 when an integer literal is checked against `Int` and its value does not fit, treating a literal that is the direct operand of unary `-` as negative (so `-9223372036854775808` is legal and `9223372036854775808` is not).

**Literal typing** (bidirectional, compile time, exact):
- An integer literal is `Int`, unless the expected type is `Float`, `Decimal` or `BigInt`, in which case it has that type.
- A float literal is `Float`, unless the expected type is `Decimal`. In that case it is the **exact** decimal value written (`let p: Decimal = 19.99` is exactly 19.99). There is no runtime conversion.
- Variables never convert implicitly (04 L3).

### 5.7 String literals

- `"…"` is a single-line string. `"""…"""` is multi-line: the first newline after the opening quotes is dropped, and the common indentation of the content lines (up to the closing `"""`) is stripped.
- **Escapes:** `\n \t \r \\ \" \{ \} \0` and `\u{X…}` (1–6 hex digits). An unknown escape is K0130. A lone surrogate escape (`\u{D800}`) is K0132.
- **Interpolation:** `{expr}`. A literal brace is `\{` or `\}`. That is the only way to write one. The expression's type must implement `Show` (§9.7). A string literal inside a hole is K0131: bind it to a `let` first.
- **Tagged literals:** `TAG"…"` or `TAG"""…"""` with no space before the quote, where `TAG` is a tag in scope (§17). Tags are ASCII and identical in every skin.
- There are no raw strings in v0.1.
- §5.1 applies to string contents. Lint W0133 flags remaining unusual code points in strings (04 U9).

### 5.8 Indentation and line structure (D15)

1. **Indent unit:** 4 spaces per level (K0120 otherwise). Tabs are K0103.
2. **Logical lines.** The lexer emits `NEWLINE` at the end of each non-blank, non-comment-only line, with these exceptions:
   - **Bracket suppression:** inside `(`, `[` or `{`, newlines and indentation are ignored. There are no exceptions, because Kalam has no multi-line lambdas (DR-15).
   - **Leading-dot continuation (DR-05):** a physical line whose first token is `.` is joined to the preceding logical line if, and only if, (a) it is indented strictly deeper than the first line of that logical line, **and** (b) the preceding logical line does not end with a block colon. Otherwise the line follows the normal rules. At the same indentation it is a new statement (for example a value block's final `.None`). A deeper indentation after a non-header line is K0120.
     ```kalam
     let names = users
         .filter(fn(u) => u.active)
         .map(fn(u) => u.name)
     ```
   - There is no backslash continuation and no trailing-operator continuation. Use parentheses.
3. **INDENT/DEDENT:** Python's algorithm. An indent may only grow by one level at a time (K0120). A dedent must land on a previous level (K0122). EOF pops all levels.
4. **Block colon:** a `:` at bracket depth 0 followed by NEWLINE is always a block opener. A `:` at bracket depth 0 followed by more tokens on the same line opens a **one-line block** (DR-33). The single statement after it is a `line_stmt` (§8). `fmt` keeps it on one line only if it fits in 100 columns.
5. **No semicolons.** `;` and `؛` are K0904, with a fix that splits the line.
6. A file ends with a newline (`fmt` adds it).

### 5.9 Operators and punctuation

```
+  -  *  /  //  %  ==  !=  <  <=  >  >=  =  +=  -=  *=  /=
->  =>  ..  ..=  ?  .  ,  :  (  )  [  ]  {  }  @  |  `
```

- ASCII in every skin. The exceptions are `،` for `,` and `؟` for `?`, which are accepted in `ar` files only (§6.4).
- `|` appears only between pattern alternatives. `@` appears only at the start of an attribute.
- `and`, `or` and `not` are keywords. `&&`, `||` and `!` are K0911, with fixes.
- There are no bitwise operators (use `Int` methods such as `bit_and`, `shift_left`; a shift amount outside `0..=63` panics), and no `**` (use `.pow()`), `++`, `===`, `?:` or `|>`. §21.3 has the fixes.

---

## 6. D7 resolved: Arabic RTL, digits and punctuation

D7 was deferred to research. This section locks it, as the consensus of all three drafts plus the four open items (DR-01, DR-01a–d). The founder ratified it as written (D7 resolved, 2026-10-25).

### 6.1 Direction

1. **Storage is always logical order.** No file ever contains a bidi control character (§5.1).
2. **Base direction comes from the skin.** First-party tools (LSP semantic tokens, the VS Code extension, the playground and `kalam diff`) display `.ar.kl` files with an RTL base paragraph direction, and `.kl`/`.franco.kl` files with LTR.
3. **First-party renderers isolate each token** (UTS #55 §5) and mirror brackets correctly, so `x + عمر == 1` cannot *look* like a different expression (02 §6.1). `kalam check --bidi` flags lines whose visual order could still mislead a generic viewer (04 U8).
4. **Reviewers can always see an unambiguous view.** `kalam diff --skin en` renders any change in the English skin, which is LTR (04 U7).
5. **Terminal diagnostics.** Terminals do not shape or reorder Arabic text (02 §6.4). For `ar` files, the CLI prints the message in the user's `KALAM_LANG` (`en`, `ar` or `franco`) and the code excerpt **twice**: once as written, and once in the English skin with the same span highlighted. `--excerpt=ar|en|both` controls this. The default is `both`, unless the terminal advertises bidi support. The LSP always shows full Arabic diagnostics.

### 6.2 Digits

| Skin | Accepted in numeric literals | Formatter emits |
|---|---|---|
| `en` | ASCII only. Arabic-Indic digits are SEC0122, with a machine fix to ASCII | ASCII |
| `franco` | ASCII only. Arabic-Indic digits are SEC0122, with a machine fix to ASCII | ASCII |
| `ar` | ASCII or Arabic-Indic, one system per literal | per project setting `[skin.ar] digits = "ascii"` (default) or `"arabic-indic"` |

- Persian/Extended Arabic-Indic digits are always rejected (SEC0121). D19 does not change this: only the Persian/Urdu **letters** ی ک ە ھ ۀ are accepted and folded (§5.3). Persian digits `۰–۹` stay rejected everywhere, because `۴ ۵ ۶` differ in shape from `٤ ٥ ٦` and the two systems are confusable.
- *Rationale for the ASCII default:* Egyptians write prices, phone numbers and code with ASCII digits, and ASCII avoids a keyboard-layout switch for every number (02 §6.3). Teams that prefer Arabic-Indic flip one project setting. KCF stores values, so G1/G2 hold.
- Digits **inside strings** are data and are never converted. Runtime parsing (`"١٢٣".to_int()`) accepts ASCII and Arabic-Indic digits and returns `Ok(123)`. `Int.to_string()` always produces ASCII digits, which is a stable wire format. Locale-aware display is `i18n.number(n, locale: "ar-EG")`.

### 6.3 Separators

- **Decimal separator:** `.` with ASCII digits and `٫` (U+066B) with Arabic-Indic digits. **`،` is never a decimal separator** (02 §6.3).
- **Digit group separator:** `_` in every skin. **U+066C ARABIC THOUSANDS SEPARATOR is rejected** (SEC0123), with a fix-it to `_` (DR-01d).

### 6.4 Punctuation aliases (Arabic skin only)

| ASCII | Arabic | Accepted in `ar` input | `ar` printer emits |
|---|---|---|---|
| `,` | `،` U+060C | both | `،` (fixed, DR-01c) |
| `?` (try) | `؟` U+061F | both | `؟` (fixed) |
| `;` | `؛` | rejected like `;` (K0904) | — |

All other punctuation and operators are ASCII in every skin. In `en` and `franco` files, `،` and `؟` are K0201, with a fix.

### 6.5 Strings and output

- String contents are never normalized and never reordered. String equality is code-point equality (§9.2).
- `std/web` emits `dir="auto"` on text containers by default, sets `lang`/`dir` from the request locale, and wraps interpolated text holes in `<bdi>` when the surrounding element has a fixed `dir`. This stops user text from reordering the page around it (06 §1.2, a display-level Trojan Source).
- `print` writes code points in logical order. Rendering them is the terminal's job.

---

## 7. Keywords, core names and alias policy

The authoritative table is `spec/keywords.json`, and the lexer loads it. The tables below are generated from that file. Each concept has exactly one canonical spelling per skin, and no spelling serves two concepts (02 §7.2).

**`keywords.json` schema.** The file is a JSON array of objects. Every object has `concept` (a stable concept ID such as `KW_IF`, which is also the KCF token), `kind`, and the three spellings `en`, `ar` and `franco`. `kind` is one of:
- `keyword`: a translated reserved word;
- `ascii_fixed`: a reserved word spelled identically in every skin (`unsafe`, `extern`);
- `core_name`: a translated, unshadowable prelude name;
- `punctuation`: a punctuation alias. For these rows, `ar_also_accepts` lists the ASCII spellings that Arabic files also accept, and `note` explains usage. Punctuation rows have no aliases.

The optional fields are `franco_aliases` (the closed list of Franco spelling variants, accepted by the lexer and rewritten by `fmt`) and `did_you_mean` (spellings that are **never accepted**, used only to improve K0127/K0401 messages, for example digit-leading Franco forms). The lexer filters rows by `kind` and loads only the file's skin column.

### 7.1 Keywords (31 per skin: 29 translated + 2 fixed ASCII)

Arabic spellings are canonical, the way `fmt` prints them. Arabic matching uses `arabic_fold` (§5.3), so `اذا` and `إذا` are one keyword. Franco spellings follow 02 §7.6: ASCII lowercase, **never starting with a digit**, Cairene spelling, and no regional digits 6, 8 or 9 in canonical forms.

| # | Concept | English (canonical) | Arabic | Franco (canonical) | Franco spelling variants (closed list) |
|---|---|---|---|---|---|
| 1 | `KW_LET` | `let` | `ليكن` | `khali` | `khalli`, `khaly` |
| 2 | `KW_VAR` | `var` | `متغير` | `motghayer` | `motaghayer`, `motghayyer`, `mot8ayer` |
| 3 | `KW_FN` | `fn` | `دالة` | `dala` | `dalla`, `dalah` |
| 4 | `KW_RETURN` | `return` | `أرجع` | `rag3` | `ragga3`, `raga3`, `rg3` |
| 5 | `KW_IF` | `if` | `إذا` | `law` | `lw`, `lau`, `low` |
| 6 | `KW_ELSE` | `else` | `وإلا` | `walla` | `wala`, `wla` |
| 7 | `KW_MATCH` | `match` | `حسب` | `hasab` | `hasb` |
| 8 | `KW_CASE` | `case` | `عندما` | `lama` | `lamma` |
| 9 | `KW_FOR` | `for` | `لكل` | `lekol` | `lkol`, `likol`, `le_kol` |
| 10 | `KW_IN` | `in` | `في` | `fi` | `fe`, `fy` |
| 11 | `KW_WHILE` | `while` | `طالما` | `tool_ma` | `tol_ma`, `tolma`, `toul_ma` |
| 12 | `KW_BREAK` | `break` | `اخرج` | `kfaya` | `kefaya`, `khalas` |
| 13 | `KW_CONTINUE` | `continue` | `استمر` | `kammel` | `kamel`, `kml` |
| 14 | `KW_PASS` | `pass` | `تخطى` | `fot` | `foot` |
| 15 | `KW_AND` | `and` | `و` | `we` | `wi` |
| 16 | `KW_OR` | `or` | `أو` | `aw` | `ao` |
| 17 | `KW_NOT` | `not` | `ليس` | `mesh` | `msh`, `mish` |
| 18 | `KW_TRUE` | `true` | `صح` | `sa7` | `sah`, `sa7e7` |
| 19 | `KW_FALSE` | `false` | `خطأ` | `ghalat` | `ghalt` |
| 20 | `KW_TYPE` | `type` | `هيكل` | `haykal` | `haikal`, `heikal` |
| 21 | `KW_ENUM` | `enum` | `تعداد` | `ta3dad` | `t3dad` |
| 22 | `KW_TRAIT` | `trait` | `سمة` | `sefa` | `sifa` |
| 23 | `KW_IMPL` | `impl` | `نفذ` | `naffez` | `nafez`, `naffiz` |
| 24 | `KW_USE` | `use` | `استخدم` | `hat` | `hatly`, `hatli` |
| 25 | `KW_AS` | `as` | `بصفة` | `ka` | — |
| 26 | `KW_EXPORT` | `export` | `علني` | `talla3` | `tala3`, `tl3` |
| 27 | `KW_OPAQUE` | `opaque` | `مغلق` | `ma2fool` | `ma2fol`, `mafool` |
| 28 | `KW_TEST` | `test` | `اختبار` | `ekhtebar` | `ekhtbar`, `e5tebar` |
| 29 | `KW_SELF` | `self` | `ذات` | `nafso` | `nafs`, `nafsu` |
| 30 | `KW_UNSAFE` | `unsafe` | `unsafe` | `unsafe` | — |
| 31 | `KW_EXTERN` | `extern` | `extern` | `extern` | — |

- `else if` is two keywords: `وإلا إذا` in Arabic and `walla law` in Franco.
- **Not keywords:** `async`, `await`, `null`, `nil`, `undefined`, `class`, `struct`, `record`, `def`, `func`, `function`, `elif`, `try`, `catch`, `throw`, `raise`, `const`, `interface`, `new`, `switch`, `lambda`. They are ordinary identifiers. Writing one where the mistake is obvious triggers the targeted diagnostics of §21.3.

### 7.2 Core names (19 per skin: translated, reserved, cannot be shadowed)

Core names are prelude items whose *spelling* comes from the skin table. They cannot be declared, redefined or shadowed (K0413), so translating them never changes meaning.

| Concept | English | Arabic | Franco | Franco spelling variants |
|---|---|---|---|---|
| `CORE_INT` | `Int` | `عدد` | `adad` | — |
| `CORE_FLOAT` | `Float` | `كسري` | `kasri` | — |
| `CORE_DECIMAL` | `Decimal` | `عشري` | `ashri` | — |
| `CORE_BOOL` | `Bool` | `منطقي` | `mante2i` | `mantiqi` |
| `CORE_STRING` | `String` | `نص` | `nass` | — |
| `CORE_LIST` | `List` | `قائمة` | `qayma` | `qaima` |
| `CORE_MAP` | `Map` | `قاموس` | `qamoos` | `qamous` |
| `CORE_SET` | `Set` | `مجموعة` | `magmoo3a` | `magmo3a` |
| `CORE_OPTION` | `Option` | `اختياري` | `ekhtiyari` | `ekhtiari` |
| `CORE_RESULT` | `Result` | `نتيجة` | `natiga` | `nateega` |
| `CORE_SOME` | `Some` | `موجود` | `mawgood` | `mawgod` |
| `CORE_NONE` | `None` | `لا_شيء` | `mafeesh` | `mafish`, `mfesh` |
| `CORE_OK` | `Ok` | `نجاح` | `naga7` | `nagah` |
| `CORE_ERR` | `Err` | `فشل` | `fashal` | `fashl` |
| `CORE_PRINT` | `print` | `اطبع` | `ektb` | `ekteb`, `etb3` |
| `CORE_PANIC` | `panic` | `انهيار` | `enhyar` | `enhiar` |
| `CORE_ASSERT` | `assert` | `تحقق` | `et2aked` | `etakked` |
| `CORE_MAIN` | `main` | `رئيسي` | `ra2isi` | `ra2eesy` |
| `CORE_WORLD` | `World` | `عالم` | `donya` | `donia`, `dunya` |

**Fixed ASCII, never translated, in every skin:** `unsafe`, `extern`; the tags `sql`, `html`, `cmd`, `url`, `path`, `regex`; the types `BigInt`, `Bytes`, `Never`, `Self`, `Sql`, `Html`, `Command`, `Url`, `Path`, `PathSegment`, `Regex`, `Secret`, `AnyError`, `Cell` and the capability types; every `@attribute` name; every module path and stdlib member name (`std/http`, `.map`, `db.all`). *Rationale:* 04 L13 greppability, and 02 §7.3 ("library APIs stay English"). Arabic and Franco alias packs for stdlib names are open question Q5 (§29).

**Effective reserved spellings per skin:** 31 keywords + 19 core names = **50**. §26 reports both counts against 05 D4.

### 7.3 Alias policy

| Skin | Policy |
|---|---|
| **English** | **No aliases.** The English skin is the canonical text. Foreign spellings (`def`, `elif`, `null`, …) are *errors with machine fixes* (§21.3), never aliases. |
| **Arabic** | **No lexical aliases** (DR-20). Hamza/alef forms, ى/ي and ة/ه are matched by `arabic_fold` for keywords and core names. That is orthography, not synonymy. Tatweel or harakat inside a keyword is SEC0106, auto-fixed by `fmt`. |
| **Franco** | A **closed, versioned list of spelling variants** per keyword and core name (the `franco_aliases` field), covering vowel variants and in-word digit letters (`3`=ع, `7`=ح, `2`=ء/ق, `5`=خ, `8`=غ). **No variant starts with a digit.** Natural digit-leading spellings appear only in `did_you_mean`, so the error message can suggest the right word. |
| **All** | The compiler accepts variants silently. `kalam fmt` rewrites them to the canonical spelling. `kalam fmt --check` (CI) fails on any non-canonical spelling, so repositories hold one form. Adding a variant is a minor-version change. Removing one is an edition change. |

**"Did you mean" for Franco** uses folding (`5→kh`, `7→h`, `3→a`, `2→a`, `8→gh`, `4→sh`, vowels removed) and Damerau–Levenshtein distance ≤ 2, **in diagnostics only**. It never accepts input silently (02 §7.6).

### 7.4 Table integrity rules (CI test over `keywords.json`, 05 P7)

1. No two reserved spellings (keywords, core names or Franco variants) in one skin share a **lookup key**. The lookup key is the one the lexer matches on (§5.4): the exact spelling in `en` and `franco`, and `arabic_fold(spelling)` in `ar` (including the Persian/Urdu folds of D19, §5.3). (The vowel-stripping Franco fold of §7.3 is used only for "did you mean" and is not a lookup key. Under it, `we`/`wi` (and) and `aw` (or) share the key `w`, and `ao` folds to the empty string, so those hints can be ambiguous.)
2. No Franco canonical spelling or variant starts with a digit or matches the numeric-literal grammar (§5.6). This makes "no prefix-ambiguous number" hold trivially.
3. No Arabic canonical spelling contains tatweel or harakat.
4. `KW_UNSAFE` and `KW_EXTERN` are identical ASCII in all skins.
5. Within a skin, two reserved words at edit distance ≤ 1 after folding are allowed only if they can never appear in the same grammatical position. **Known exception E1:** Arabic `و` (and) and `أو` (or) are both infix and differ by one letter. They are kept because they are the standard Arabic conjunctions. Lint L0114 flags a mixed `و`/`أو` chain written without parentheses. Survey item: D22 (vocabulary survey before beta).
6. **2,000-word rule (DR-36):** an Arabic keyword or core name that appears in the CI list of the 2,000 most frequent Arabic identifier words needs a recorded justification.
7. Word-choice notes that are enforced by tests:
   - `صح` (true) and `عدد` (Int) avoid the `صحيح` true/integer near-collision.
   - `لا_شيء` (None) avoids `عدم`, which is one letter from `عدد`.
   - `خطأ` (false) and `فشل` (Err) are deliberately different words, so a boolean and an error are never confused.
   - `عندما` (case) avoids `حالة` ("state", a top-50 identifier) and `عند` (one letter from `عدد`).

CI runs these checks as a script under `conformance/keywords/`. The v0.1 table was checked against rules 1–4 by script when this spec was written (31 keywords, 19 core names, 0 conflicts). Rules 5–7 need the confusable table and the 2,000-word list, which do not exist yet.

---

## 8. Grammar (EBNF) and parser class

### 8.1 Notation and tokens

ISO-style EBNF. `{ x }` means zero or more, `[ x ]` means optional, and `"kw"` is the **concept token** that every skin's spelling maps to. The grammar is skin-free.

Lexical tokens: `IDENT`, `CORE` (a core name), `CORE_CTOR` (`Some`, `None`, `Ok`, `Err` in the file's skin), `INT_LIT`, `FLOAT_LIT`, `STRING`, `TAGGED_STRING`, `NEWLINE`, `INDENT`, `DEDENT` and `EOF`. Line structure (bracket suppression, leading-dot continuation, one-line blocks) is defined in §5.8.

### 8.2 Productions

```ebnf
(* ---------- File and items ---------- *)
file          = { NEWLINE } { use_decl } { item } EOF ;
use_decl      = [ "export" ] "use" module_path [ use_tail ] NEWLINE ;
module_path   = IDENT { "/" IDENT } ;                         (* std/http, notes/db *)
use_tail      = "." "{" use_item { "," use_item } [ "," ] "}" | "as" IDENT ;
use_item      = IDENT [ "as" IDENT ] | "self" ;               (* self binds the module name *)

item          = { attribute } [ visibility ] item_body ;
attribute     = "@" IDENT { "." IDENT } [ "(" [ arg_list ] ")" ] NEWLINE ;
visibility    = "export" [ "opaque" ] ;                       (* opaque: type/enum only, K0302 *)
item_body     = fn_decl | type_decl | enum_decl | trait_decl | impl_decl
              | const_decl | extern_decl | test_decl ;

fn_decl       = fn_sig ( block | "=>" expr NEWLINE ) ;
fn_sig        = "fn" IDENT [ generics ] "(" [ param_list ] ")" [ "->" type ] ;
generics      = "[" gparam { "," gparam } [ "," ] "]" ;
gparam        = IDENT [ ":" bound { "+" bound } ] ;
bound         = type_path [ type_args ] ;
param_list    = param { "," param } [ "," ] ;
param         = "self" | IDENT ":" type [ "=" expr ] ;

type_decl     = "type" IDENT [ generics ]
                ( ":" NEWLINE INDENT field_decl { field_decl } DEDENT     (* record *)
                | "=" type NEWLINE ) ;                                    (* transparent alias *)
field_decl    = { attribute } IDENT ":" type [ "=" expr ] NEWLINE ;
enum_decl     = "enum" IDENT [ generics ] ":" NEWLINE INDENT variant { variant } DEDENT ;
variant       = { attribute } IDENT [ "(" variant_field { "," variant_field } [ "," ] ")" ] NEWLINE ;
variant_field = [ IDENT ":" ] type ;
trait_decl    = "trait" IDENT [ generics ] ":" NEWLINE INDENT trait_member { trait_member } DEDENT ;
trait_member  = fn_sig ( block | "=>" expr NEWLINE | NEWLINE ) ;  (* NEWLINE alone = required *)
impl_decl     = "impl" [ generics ] type [ "as" bound ] ":"
                NEWLINE INDENT impl_member { impl_member } DEDENT ;
impl_member   = { attribute } [ "export" ] fn_decl ;
const_decl    = "let" IDENT [ ":" type ] "=" expr NEWLINE ;    (* module level: pure, non-suspending *)
extern_decl   = "extern" ( fn_sig | "type" IDENT [ generics ] ) NEWLINE ;
test_decl     = "test" STRING block ;

(* ---------- Blocks and statements ---------- *)
block         = ":" ( NEWLINE INDENT stmt { stmt } DEDENT | simple_line NEWLINE ) ;
stmt          = simple_line NEWLINE
              | binding_head ( if_form | match_form )          (* multi-line value forms *)
              | if_form | match_form | for_stmt | while_stmt | fn_decl ;  (* fn_decl = local fn *)
simple_line   = expr | "break" | "continue" | "pass"
              | "return" [ expr ]
              | "let" pattern [ ":" type ] "=" expr
              | "var" IDENT [ ":" type ] "=" expr
              | place assign_op expr ;
binding_head  = "let" pattern [ ":" type ] "=" | "var" IDENT [ ":" type ] "="
              | place assign_op | "return" ;
assign_op     = "=" | "+=" | "-=" | "*=" | "/=" ;
place         = IDENT { "." member | "[" expr "]" } ;         (* root must be a var, K0710 *)
if_form       = "if" or_expr block { "else" "if" or_expr block } [ "else" block ] ;
match_form    = "match" expr ":" NEWLINE INDENT case_arm { case_arm } DEDENT ;
case_arm      = "case" pattern [ "if" or_expr ] block ;
for_stmt      = "for" pattern "in" expr block ;
while_stmt    = "while" or_expr block ;

(* ---------- Expressions, lowest to highest precedence ---------- *)
expr          = lambda | if_inline | or_expr ;
lambda        = "fn" "(" [ lparam { "," lparam } [ "," ] ] ")" [ "->" type ] "=>" expr ;
lparam        = ( IDENT | "_" ) [ ":" type ] ;
if_inline     = "if" or_expr ":" or_expr { "else" "if" or_expr ":" or_expr } "else" ":" or_expr ;
or_expr       = and_expr { "or" and_expr } ;
and_expr      = not_expr { "and" not_expr } ;
not_expr      = "not" not_expr | cmp_expr ;
cmp_expr      = range_expr [ cmp_op range_expr ] ;              (* non-associative: K0720 *)
cmp_op        = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
range_expr    = add_expr [ ( ".." | "..=" ) add_expr ] ;
add_expr      = mul_expr { ( "+" | "-" ) mul_expr } ;
mul_expr      = unary { ( "*" | "/" | "//" | "%" ) unary } ;
unary         = "-" unary | postfix ;
postfix       = primary { call | index | "." member | "?" } ;
call          = "(" [ arg_list ] ")" ;
index         = "[" expr "]" ;                                 (* always indexing, DR-25 *)
arg_list      = arg { "," arg } [ "," ] ;
arg           = [ IDENT ":" ] expr ;                           (* positional before named *)
member        = IDENT | INT_LIT ;                              (* t.0: §5.6 member rule *)
primary       = literal | IDENT | CORE | CORE_CTOR | "self"
              | "." IDENT                                      (* implicit member: .Circle *)
              | paren | list_lit | map_lit
              | "unsafe" "." IDENT ;
paren         = "(" [ expr [ "," [ expr { "," expr } [ "," ] ] ] ] ")" ;  (* unit, group, tuple *)
list_lit      = "[" [ expr { "," expr } [ "," ] ] "]" ;
map_lit       = "{" [ expr ":" expr { "," expr ":" expr } [ "," ] ] "}" ;
literal       = INT_LIT | FLOAT_LIT | STRING | TAGGED_STRING | "true" | "false" ;

(* ---------- Patterns ---------- *)
pattern       = bind_pat { "|" bind_pat } ;
bind_pat      = prim_pat [ "as" IDENT ] ;
prim_pat      = "_" | IDENT                                     (* a bare IDENT always binds *)
              | int_pat | STRING | "true" | "false"
              | ( "." IDENT | type_path "." IDENT ) [ "(" pat_fields ")" ]  (* enum variant *)
              | CORE_CTOR [ "(" pattern ")" ]                    (* Some(x), None, Ok(v), Err(e) *)
              | type_path "(" pat_fields ")"                     (* record *)
              | "(" pattern "," pattern { "," pattern } [ "," ] ")"
              | "[" [ list_elem { "," list_elem } [ "," ] ] "]" ;
int_pat       = [ "-" ] INT_LIT [ ( ".." | "..=" ) [ "-" ] INT_LIT ] ;
list_elem     = pattern | ".." [ IDENT ] ;
pat_fields    = pat_field { "," pat_field } [ "," ".." ] | ".." ;
pat_field     = [ IDENT ":" ] pattern ;

(* ---------- Types ---------- *)
type          = fn_type | "(" ")" | "(" type "," type { "," type } [ "," ] ")" | named_type ;
named_type    = type_path [ type_args ] ;
type_args     = "[" type { "," type } [ "," ] "]" ;
type_path     = ( IDENT | CORE ) { "." IDENT } ;
fn_type       = [ "@sync" ] "fn" "(" [ type { "," type } [ "," ] ] ")" "->" type ;
```

### 8.3 Disambiguation rules (normative)

1. **`if` at statement start and in rhs position** (after `binding_head`) (DR-07). The parser reads `"if" or_expr ":"`. Then:
   - if the next token is `NEWLINE`, the construct is `if_form` with multi-line blocks;
   - otherwise it reads a `simple_line`. If that line is a bare expression **and** the next token is `else` (same logical line), the construct is `if_inline` (an expression; at statement start it becomes an expression statement and K0730 applies if its type is not `()`). Otherwise it is `if_form` whose first block is a one-line block, and further `else` clauses may follow on later lines.
   - In rhs position, `if_form` is a *value form*. It needs an `else` (K0731), and each arm yields its last expression (§10.3).
   
   Examples: `if x < 0: return Err(.Neg)` is a statement. `let s = if done: "done" else: "open"` is an inline expression. `return if a > b: a else: b` is an inline expression.
2. **Inside any other expression** (arguments, list elements, lambda bodies), `if` is always `if_inline` and requires `else`.
3. **`fn` at statement start:** `fn IDENT` begins a local function declaration. `fn (` begins a lambda expression.
4. **Record construction vs. call:** `N(args)` has one syntax. Name resolution decides. A record constructor requires every argument to be named (K0416).
5. **Implicit members** (`.Circle`, `.LoggedIn`) resolve against the expected type, or are K0803 without one.

### 8.4 Parser class

The grammar is **LL(1) after left-factoring at four points, and needs no backtracking** (DR-34):

1. `if` at statement or rhs start: the shared prefix `if or_expr :` is parsed once, then one token (NEWLINE vs. other) decides, and later one token (`else` vs. NEWLINE) decides.
2. `(`: unit vs. group vs. tuple. After the first expression, `)` vs. `,` decides. The same applies to tuple patterns and types.
3. A statement that starts with an expression vs. an assignment: the parser reads a `postfix`, then `assign_op` vs. other decides, and the expression is then checked to be a `place` (K0710).
4. A named vs. positional argument (and `IDENT ":"` in map literals and record patterns): the parser reads an `expr`, and if it is a bare `IDENT` followed by `:`, it becomes a label.

There is no explicit type-argument syntax in expressions (DR-25), so postfix `[` is never ambiguous. CI generates a tree-sitter grammar from this EBNF for all three skins (03 §5). It also checks this claim with an LL(1) conflict report on the left-factored grammar.

---

## 9. Type system

### 9.1 Overview

- **Static and sound** (D5, 04 L3). Types are nominal for records, enums and traits, and structural for tuples and functions. There is **no subtyping**, with two exceptions: `Never <: T`, and a non-suspending function type is usable where the same suspending type is expected (§15.3).
- **No implicit conversions** (04 L3). **No truthiness:** conditions must be `Bool` (04 L4).
- **No `any`, no unchecked casts.** Dynamic JS values are the opaque `JsValue`, which can only be decoded (§19.2).
- **All values are immutable.** Mutation exists only as *rebinding* of `var` locals (§10.4) and through the explicit stdlib type `Cell[T]` (§15.6).

### 9.2 Primitive types

| Type | Values | Operations and semantics |
|---|---|---|
| `Int` | 64-bit signed integers, **[−2⁶³, 2⁶³−1]**, on every backend (DR-02, D18) | `+ - *` and unary `-` trap when the result leaves the range (04 L7), so `-(−2⁶³)` panics. Shifts take an amount in `0..=63` and panic outside it. `//` floors and `%` takes the sign of the divisor (Python semantics). `/` on two `Int`s returns `Float`. Division or modulo by zero panics. Explicit `wrapping_add`, `checked_add` (→ `Option[Int]`) and `saturating_add` exist. |
| `BigInt` | arbitrary precision | Same operators, no overflow. Built from an integer literal in `BigInt` context or with `BigInt.from(i)`. |
| `Float` | IEEE-754 binary64 | `+ - * /`. `==`, `!=`, `<`, `<=`, `>`, `>=` are **built-in IEEE operators on `Float` only**. `Float` is **not** `Eq`, `Ord` or `Hash` (DR-23): use `.total_cmp()`, `.near(other, tol:)` and `.is_nan()`. `to_int()` → `Option[Int]`. |
| `Decimal` | exact base 10, 34 significant digits (IEEE 754 decimal128) | `+ - *` are exact and trap on overflow. **There is no `/`.** Use `a.div(b, scale: 2, rounding: .HalfEven)` (04 L14). `Decimal` is `Eq`, `Ord` and `Hash`. |
| `Bool` | `true`, `false` | `and`, `or` and `not`, which short-circuit |
| `String` | immutable sequence of Unicode scalar values | `+` concatenates. Interpolation. **No `s[i]`** (K0650). Use `.chars()`, `.graphemes()`, `.byte_len()` and `.slice(from, to)` (grapheme indices). **Equality is code-point equality.** `.nfc()` normalizes explicitly. |
| `Bytes` | immutable byte arrays | `.len()`, `.slice()`, base64 and hex codecs |
| `()` | the unit value `()` | Returned by functions with no `->` |
| `Never` | no values | The type of `panic(…)`, `return`, `break` and `continue` |

*Rationale for the `Int` range (D18):* a true 64-bit integer means the same thing on every backend and matches Postgres `bigint`, database IDs, Unix nanosecond timestamps and `i64`/`long` in other languages. The future C/WASM backend uses `int64_t` with checked arithmetic, so the backends agree exactly (03 §4, 04 L2). The JS backend uses a canonical dual `number`/`bigint` representation with a `number` fast path (§22). Values that need more than 64 bits go in `BigInt`. **Decode rules** (DR-02): a Postgres `bigint` always fits `Int`. A JSON integer outside the i64 range is a decode error when the target type is `Int`, never a lossy float, and JSON integers are read exactly, never through a JS `number` (§18.3).

### 9.3 Built-in composite types

| Type | Notes |
|---|---|
| `(A, B, …)` | Tuples with 2–8 elements, accessed with `.0`, `.1`, …, or destructured with `let (a, b) = t`. Unit is `()`. |
| `List[T]` | Ordered and immutable. Literal `[a, b]`. `xs[i]` panics when out of bounds, and `xs.get(i)` returns `Option[T]`. |
| `Map[K, V]` | Immutable and insertion-ordered. `K: Eq + Hash`. Literal `{k: v}`, where `{}` needs an expected type. Hashing is keyed SipHash with a per-process key (04 S14). Iteration order never leaks the key. |
| `Set[T]` | Immutable, insertion-ordered. `T: Eq + Hash`. Built with `Set.of([…])`. |
| `Option[T]` | `Some(x)` or `None`. **The only absence value** (04 L5). |
| `Result[T, E]` | `Ok(x)` or `Err(e)`. Must-use (§14.2). |
| `fn(A, B) -> R` | Function types. Each carries an inferred suspension bit, and `@sync fn(…)` forbids suspension (§15). Functions are not `Eq`, `Hash`, `Show`, `Encode` or `Decode`. |

*Performance note (not semantics):* the compiler updates `List`, `Map` and `Set` in place when it can prove the old value is unreferenced ("functional but in place"). Semantically they are always values.

### 9.4 Records (`type`)

```kalam
export type User:
    id: Int
    name: String
    email: Email
    role: Role = .Member
```

- Nominal and immutable. Construction uses **named arguments only**: `User(id: 1, name: "Sara", email: e)` (K0416). Fields with defaults may be omitted, and field defaults must be constant expressions (§13.4).
- **Update:** `u.with(name: "Omar")` returns a copy. `with` is a reserved method name on every record, so a field may not be called `with` (K0417).
- **Visibility:**
  - `export type` exposes the type, its fields (read-only) and its constructor.
  - `export opaque type` exposes only the type. Fields, construction and `.with` stay private to the defining module. This is how validated types such as `Email` work: `Email.parse(s) -> Result[Email, EmailError]` is the only way to make one.
  - A non-exported record is module-private.
- **Decoding an opaque type** is possible only through its own validator, `@decode_with(Email.parse)` on the declaration. The function must be in the same module, pure, non-suspending, capability-free, and must return `Result[Self, E]` (Security 7.3).
- `type UserId = Int` is a **transparent alias**: the two names are the same type. For a distinct type, use a one-field record or an opaque record.

### 9.5 Enums (sum types)

```kalam
enum Shape:
    Circle(radius: Float)
    Rect(width: Float, height: Float)
    Empty
```

- Variants are constructed with a **leading dot** when the expected type is known (`.Circle(radius: 2.0)`), or qualified (`Shape.Circle(radius: 2.0)`). A bare `Circle` is never a variant (DR-05).
- Variant fields are all named or all positional. Named fields are constructed with named arguments, but a single-field variant may be constructed positionally (`.Member("Payments")`). In patterns, fields may be bound positionally in declaration order.
- Generic enums are allowed: `enum Tree[T]:`. `export opaque enum` hides the variants.
- `@exhaustive_strict` on an enum makes a `_` arm over it an error in other packages (W0403 promoted, §11).

### 9.6 Generics

- Type parameters appear on functions, records, enums, aliases, traits and impls: `fn first[T](xs: List[T]) -> Option[T]`.
- Bounds use traits: `fn largest[T: Ord](xs: List[T]) -> Option[T]`. Several bounds are written `T: Eq + Hash`.
- There are no higher-kinded types, no variance annotations (everything is invariant, which is sound with no subtyping), no const generics and no specialization.
- Call sites never write type arguments (DR-25). They come from inference or the expected type.

### 9.7 Traits

```kalam
trait Show:
    fn show(self) -> String

impl Money as Show:
    fn show(self) -> String => "{self.amount} {self.currency}"

impl Money:
    fn zero(currency: String) -> Money => Money(amount: 0, currency: currency)
```

- Inherent methods are written `impl Type:`. A trait implementation is written **`impl Type as Trait:`** (DR-10). Generic impls: `impl[T: Show] Page[T] as Show:`.
- **Coherence:** at most one `impl T as Tr` exists in the whole program, with no overlap. The **orphan rule:** it must live in the module that defines `Tr` or the one that defines `T`.
- **No trait objects** in v0.1. Use enums or records of functions. The one built-in existential is `AnyError` (§14.3).
- **Built-in traits:**

| Trait | Purpose | Derivation | Never implemented by |
|---|---|---|---|
| `Eq` | `==`, `!=` | automatic for records, enums and tuples whose components are all `Eq` | `Float`, functions, capabilities, `Secret[T]` |
| `Hash` | `Map`/`Set` keys | automatic when all components are `Hash` | `Float`, functions, capabilities, `Secret[T]` |
| `Ord` | `< <= > >=`, `sorted()` | **explicit** `@derive(Ord)`: lexicographic by declaration order (enums by variant order) | `Float`, `Secret[T]` |
| `Show` | interpolation, `print`, logs | automatic. A custom impl overrides it | — (`Secret[T]` shows `[REDACTED]`, capabilities show `<Net>`) |
| `Encode` | JSON/wire output | **explicit** `@derive(Encode)` | **`Secret[T]`** (deriving `Encode` on a type with a `Secret` field is K0612: a compile error, not a redaction), capabilities, functions |
| `Decode` | schema-first input (§18.3) | **explicit** `@derive(Decode)`; opaque types only via `@decode_with` | capabilities, functions, `Sql`, `Html`, `Command`, `Path` (`Url` and `PathSegment` decode through their validating parsers). `Secret[T]: Decode` when `T: Decode`, so a password field can be read from a form |
| `Row` | decoding SQL rows by column name | **explicit** `@derive(Row)` | as `Decode` |
| `Message` | live-component messages (§20.3) | **explicit** `@derive(Message)` (enums only) | — |
| `Error` | `.message()`, used by `AnyError` and logs | `impl E as Error:` | — |
| `From[E]` | conversion used by `?` | written by hand | — |
| `Template` | user-defined tagged literals (§17.8) | written by hand, with `@tag("name")` | — |

- `@name("title")` on a field sets its wire name (JSON key, SQL column) explicitly. Without it, the wire name is the field's canonical name (§4.5).

### 9.8 Equality and comparison

- `==` and `!=` are defined only between two values of the **same** type that implements `Eq`, or between two `Float`s with IEEE semantics (K0602 otherwise, 04 L4). `1 == 1.0` is K0602 (`Int` vs. `Float`).
- `<`, `<=`, `>`, `>=` require `Ord`, or two `Float`s (IEEE).
- Equality on records and enums is structural. `Secret`, `Mac` and `PasswordHash` deliberately do not implement `Eq`. Compare them with `crypto.equal(a, b)`, which is constant time (04 S5).

### 9.9 Inference rules

1. **Mandatory annotations:** every top-level, `impl` and local `fn` declares its parameter types, and its return type unless it returns `()`. Module constants infer from their initializer. Extern declarations are fully annotated.
2. **Local inference** is bidirectional, with Hindley–Milner unification inside a body. Local `let` bindings are **monomorphic**, which keeps errors local.
3. **Lambdas** take parameter types from the expected function type. Without one, annotations are required (K0801).
4. **Literals** follow §5.6. Empty `[]` and `{}` need an expected type (K0802).
5. **Return-type inference for generic calls:** `json.decode(text)` takes `T` from the expected type (`let u: User = json.decode(text)?`).
6. **Implicit members** resolve against the expected type (K0803 without one).
7. Type errors report the expected and found types with their origins (*"`price` was inferred as `Float` here …"*), following 01 §3.4 rule 7.
8. Inference is decidable, rank-1, with no type-level computation.

### 9.10 Soundness statement

For every well-typed program with no `unsafe.*` call and no `extern` call, evaluation produces a value, diverges, or stops with a **defined panic** (overflow, out-of-bounds index, division by zero, explicit `panic`, failed `expect`, stack exhaustion, out of memory) (Security §8). Every value has its static type. A capability value exists only if it was derived from the `World` given to `main`. Every `Sql`, `Html`, `Command`, `Url`, `Path` and `Regex` value was built from literal source text plus encoded holes (§17.2). A mechanized proof of Kalam Core is a 1.0 deliverable (Q11).

---

## 10. Expressions and statements

### 10.1 Scoping and names

- Scoping is lexical and block-scoped. **One namespace per scope** (DR-11): types, enums, aliases, traits, functions, values and module aliases share it.
- **No shadowing** (DR-28). A binding may not reuse any name visible at that point: enclosing locals, parameters, module items, imports or core names (K0405, with a rename fix-it). In Arabic, values conventionally take the article (`العالم: عالم`, `الشكل: شكل`). In Franco they take `el_` (`el_donya: donya`).
- Every binding is initialized at its declaration.

### 10.2 `let` and `var`

- `let pattern [: T] = expr` binds immutably. The pattern must be irrefutable (K1003): `let (lat, lng) = point.coords()`.
- `var name [: T] = expr` declares a rebindable local. `var` is allowed **only inside function bodies**. There are no module-level variables (K1202).

### 10.3 `if` and `match` as values

- As a statement, `if` without `else` has type `()`.
- **Inline:** `if c: a else: b` is an expression anywhere (§8.3). Example: `let label = if n == 1: "item" else: "items"`.
- **Multi-line value form:** after `let x =`, `var x =`, an assignment or `return`, `if_form` and `match_form` are expressions. Each arm yields its **last expression**, and all arms must have the same type. `if` without `else` is not allowed there (K0731).
  ```kalam
  let fee = match plan:
      case .Free: 0
      case .Pro(seats):
          let base = 10 * seats
          base - base // 10
  ```

### 10.4 Assignment has value semantics

Assignment rebinds a `var`. It never mutates a value that anyone else can see.

| Written | Means |
|---|---|
| `x = e` | rebind `x` |
| `p.x = e` | `p = p.with(x: e)` (nested `a.b.c = e` → `a = a.with(b: a.b.with(c: e))`) |
| `xs[i] = e` | `xs = xs.set(i, e)` (panics when out of bounds) |
| `m[k] = e` | `m = m.insert(k, e)` |
| `x += e` | `x = x + e` (likewise `-=`, `*=`, `/=`) |

The root of a place must be a `var` (K0710). Because values are never shared mutably, aliasing bugs cannot happen.

### 10.5 Loops

- `for pattern in expr:` iterates any `Iterable[T]`: `List`, `Set`, `Map` (yielding `(key, value)` in insertion order), ranges, `String.chars()`/`.graphemes()`, and `.enumerate()` (yielding `(index, item)`).
- **Ranges:** `a..b` excludes `b`, `a..=b` includes it (DR-24). Both are `Range[Int]`.
- `while cond:` loops while `cond` is `true`.
- `break` and `continue` apply to the innermost loop. There are no labels and no `for … else`. Loops produce no value.

### 10.6 Operator precedence (high to low)

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

- `+` is defined on `Int`, `BigInt`, `Float`, `Decimal`, `String`, `List` (concatenation) and the sink types `Sql`/`Html` (safe concatenation). Users cannot overload operators.
- Mixing numeric types (`Int + Float`) is K0601, with a fix to `.to_float()`.
- Evaluation order is **strictly left to right** everywhere: operands, arguments, record fields in source order and interpolation holes. The C backend must emit temporaries to preserve this.

### 10.7 Discarded values

An expression statement whose type is not `()` is K0730 ("value is unused"). Write `let _ = expr` to discard deliberately. For `Result` the code is K1303 (§14.2). `kalam audit` counts deliberate discards of `Result` in non-test code.

### 10.8 Indexing

- `xs[i]` on a `List` panics when out of bounds. `xs.get(i)` returns `Option[T]`.
- `m[k]` on a `Map` panics when the key is absent. `m.get(k)` returns `Option[V]`.
- Negative indices panic. They never count from the end. Use `xs.last()`.
- Strings cannot be indexed (K0650).
- Postfix `[…]` is only ever indexing. It never means explicit type arguments (DR-25).

---

## 11. Pattern matching

```kalam
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

1. **Exhaustiveness is mandatory** (04 L8). A non-exhaustive `match` is K1001. The diagnostic lists the missing cases as ready-to-paste `case` arms, as a machine fix that inserts `panic("TODO")` bodies. The checker is Maranget's usefulness algorithm.
2. **An unreachable arm is an error** (K1002), not a warning. This catches the "catch-all placed first" bug. Guarded arms do not count toward exhaustiveness.
3. **Bindings:** a bare identifier always binds. Variants need `.V` or `Type.V`. The core constructors `Some`, `None`, `Ok` and `Err` are reserved, so `None` is never a binding (DR-05).
4. **A `_` arm over an enum from another package** draws W0403 ("variants added upstream will silently go to this arm"). It is an error if that enum is `@exhaustive_strict`. Use this for authorization roles and state machines.
5. **Or-patterns** (`case .Card | .Wallet:`) must bind the same names with the same types in every alternative (K1004).
6. **Literal patterns:** `Int`, `String` and `Bool` literals, and `Int` ranges (`case 1..=9:`). `Float` patterns are not allowed (K1005). Matching on `Int`, `String` or `Decimal` needs a final `_` or binding arm.
7. **List patterns:** `[]`, `[x]`, `[first, ..rest]`, `[.., last]`.
8. **Record patterns:** `User(role: .Admin, name: n, ..)`. Without `..`, every field must be listed.
9. `as` binds the whole value: `case .Circle(_) as c:`.
10. Nested patterns are fully supported, for example `case Err(.NotFound(id)):`.
11. **Matching never runs user code.** Patterns inspect structure and compare literals with built-in equality. No user `Eq` impl or getter runs (Security 10.3).
12. There is no `if let` and no `let … else`. Use `match`, `?`, or `Option` methods (`.map`, `.or`, `.ok_or`). One way.

---

## 12. Functions and closures

### 12.1 Declarations

```kalam
fn total(items: List[Item], tax: Decimal = 0.14) -> Decimal:
    var sum: Decimal = 0
    for item in items:
        sum += item.price * Decimal.from(item.qty)
    return sum + sum * tax

fn double(x: Int) -> Int => x * 2
```

- **Block-bodied functions with a non-`()` return type need an explicit `return`** on every path (K1101; the fix inserts `return` before a final expression of the right type) (DR-08).
- **Expression-bodied** functions use `=> expr`.
- **Arguments:** positional first, then named (`f(a, b, tax: 0.1)`). Any parameter may be passed by name.
- **Default values** are allowed (DR-35). They are evaluated on each call and may not refer to other parameters.
- **Named arguments are required** at the call site when an `export`ed function has two **adjacent parameters of the same type** (K1104; the fix inserts the labels). `transfer(db, from: a, to: b, amount: x)` cannot be called with `from` and `to` swapped by position. The receiver `self` never counts as one of the adjacent parameters. An audited `@positional` on the declaration opts out for symmetric or conventional orders. The stdlib marks as `@positional` every method whose argument order is conventional: `min`, `max`, `clamp`, `crypto.equal`, `task.both`, `slice(from, to)`, `replace(old, new)`, `set(index, value)`, `insert(key, value)`, `zip`, `div`, and the drawing primitives that take `x, y`. Money-moving and path-joining APIs are deliberately **not** positional. `kalam audit` lists every `@positional` function with adjacent same-type parameters (Security 11.1).
- Parameters are immutable. To mutate one, copy it into a `var`.
- There is no overloading, no variadics (use `List[T]`) and no implicit `this`.
- Tail calls are not guaranteed, because of the JS target. Stack exhaustion is a defined panic.

### 12.2 Methods and resolution

- Methods are declared in `impl` blocks with `self` as the first parameter and called as `x.m(args)`. Associated functions have no `self` and are called as `Type.f(args)`.
- `Type.method` without a call is a function value whose first parameter is the receiver.
- **Resolution (DR-27):** the candidates for `x.m` are the inherent methods of `x`'s type plus the methods of every trait implemented for that type. **Exactly one candidate must exist**, otherwise it is K1103 (disambiguate with `Trait.m(x)`). Imports never change resolution. There is no UFCS: a free function `f` is never callable as `x.f()`.

### 12.3 Closures and local functions

- `fn(x) => x + 1` is a lambda. **Its body is one expression** (DR-15). There are no multi-line lambdas.
- For a multi-statement callback, declare a **local `fn`**. It may capture `let` bindings and capabilities:
  ```kalam
  fn transfer(db: Db, from: Int, to: Int, amount: Decimal) -> Result[(), DbError]:
      fn move(tx: Db) -> Result[(), DbError]:
          tx.exec(sql"UPDATE accounts SET balance = balance - {amount} WHERE id = {from}")?
          tx.exec(sql"UPDATE accounts SET balance = balance + {amount} WHERE id = {to}")?
          return Ok(())
      return db.transaction(move)
  ```
- **Capture is by value, of `let` bindings only.** Capturing a `var` is K1110, with the fix `let snapshot = v` inserted before the closure. Together with immutable values, this means closures never observe mutation, and that makes concurrency race-free (§15.6).
- Closures are ordinary function values with an inferred suspension bit (§15).

---

## 13. Modules, packages and visibility

### 13.1 Packages and modules

```
notes/                        # package "@team/notes"
  kalam.toml
  kalam.lock
  names.toml                  # optional display glossary (§4.5)
  src/
    main.kl                   # module notes/main
    db.kl                     # module notes/db
    web/views.ar.kl           # module notes/web/views (Arabic skin)
    internal/money.franco.kl  # module notes/internal/money (package-private, Franco skin)
  test/
    views_test.kl
```

- **One file is one module.** Its path is `<package>/<path without src/ and without the skin suffix>`.
- **Imports always use absolute paths:** `use notes/db.{App}`, `use std/http`. There are no relative imports. **There are no glob imports** (`*` is K1203): globs feed hallucination and hide where names come from.
- `use std/http` binds `http`. `use std/http.{Router, Response}` binds names directly. `as` renames. `use std/http.{self, Router}` binds both.
- Imports are static: `use` takes a path, never a string or an expression (04 L9).
- **Import cycles are K1201.** Module constants are evaluated once, in dependency order.
- **The prelude** is imported implicitly: the core names (§7.2), `BigInt`, `Bytes`, `Never`, `Html`, `Sql`, `Command`, `Url`, `Path`, `PathSegment`, `Regex`, `Secret`, `World`, `AnyError`, `Cell`, the capability types `Net`, `Fs`, `Env`, `Clock`, `Rand`, `Proc`, `Stdin` and `Screen`, the stdlib tags, and the modules `log` and `json`.

### 13.2 Visibility

| Marker | Visible to |
|---|---|
| none | the defining module only |
| `export` | every module of the package, and dependents too, unless the module is under `src/internal/` |
| `export opaque` (type/enum) | like `export`, but fields, variants and the constructor stay private |
| `export use path.{X}` | re-export |

- An `export`ed item may not expose a private type in its signature (K0303).
- The public API of a package is every `export`ed item outside `internal/`. `kalam pkg api` prints it, and `kalam pkg diff` classifies changes for semver. **The suspension bit of an exported function is part of the API** (§15.5), and so is its capability set (`kalam caps`).

### 13.3 One namespace (DR-11)

Each scope has a single namespace. A type `Shape`, a function `shape` and a value `shape` are three different names in English, because Latin case distinguishes them. In Arabic, `شكل` cannot be both a type and a value in one scope. The idiomatic form is `الشكل: شكل`. Without a glossary, conversion keeps identifiers as written, so `main(world: World)` converts to `رئيسي(world: عالم)`. With the display glossary, it shows as `رئيسي(العالم: عالم)`.

### 13.4 Module-level items

- A module-level `let` must be a **constant expression**: literals, constructors of constant values, other constants, and stdlib functions marked `const`. It cannot take or produce a capability, cannot suspend, and cannot panic (a constant that would panic is a compile error).
- **No module-level `var`** (K1202). Kalam has no global mutable state. Shared state is an explicit `Cell` created in `main` and passed down (§15.6).
- Module initialization therefore has no observable effects and no ordering hazards.

### 13.5 Manifest (`kalam.toml`)

```toml
[package]
name = "@team/notes"            # ASCII, namespaced (04 P7)
version = "0.1.0"
edition = "2027"
targets = ["js-node", "js-browser"]

[skin.ar]
digits = "ascii"                # or "arabic-indic" (§6.2). The only skin setting.

[capabilities]                  # what this package's externs and console use may touch (§16.4)
net = ["api.stripe.com"]
console = false                 # true if a library calls print/log (X9)

[dependencies]
"@kalam/web" = "0.3"

[npm]                           # JS dependencies used only through `extern` (§19)
nanoid = { version = "5.0.7", caps = [] }
stripe = { version = "17.5.0", caps = ["net:api.stripe.com"] }
```

- `kalam.lock` records KCF hashes for Kalam packages and integrity hashes for npm packages. CI installs are frozen (04 P2).
- **No install-time code runs on any fetch path**, for Kalam or npm dependencies (04 P1). npm packages that need lifecycle scripts or native builds are unsupported in v0.1. New npm versions have a 48-hour release-age cooldown by default (04 P9).
- `kalam add` and `kalam update` show a **capability diff** for the whole dependency tree, and additions need explicit approval (04 P6).
- Publish-time checks include cross-script typosquat detection over Latin, Arabic and Franco transliterations (04 P7).

---

## 14. Error handling: `Result`, `?`, `panic`

### 14.1 Two channels (D16)

| Situation | Mechanism |
|---|---|
| Expected failure: bad input, a missing row, a network timeout | `Result[T, E]`, propagated with `?` |
| A bug: a broken invariant, an out-of-bounds index, `Int` overflow, division by zero | `panic`, which aborts the current task |

There are no exceptions, no `try`/`catch`/`throw`, and no way to catch a panic inside Kalam code (04 L6).

### 14.2 `Result` is must-use

An unused `Result` is **K1303**, an error rather than a warning. `let _ = r` discards it explicitly, and `kalam audit` counts those discards outside tests.

### 14.3 The `?` operator

`e?`, where `e: Result[T, E]`, inside a function returning `Result[U, F]` (DR-12):

1. if `E == F`, it propagates `Err(e)` unchanged;
2. otherwise, if `impl F as From[E]` exists, it propagates `Err(F.from(e))`;
3. otherwise, if `F == AnyError` and `E: Error`, it boxes the error;
4. otherwise it is K1301, with the fix-its `.map_err(fn(e) => …)` and "add `impl F as From[E]`".

`e?`, where `e: Option[T]`, inside a function returning `Option[U]` propagates `None`. Using an `Option` with `?` in a function that returns `Result` is K1302, with the fix `.ok_or(<error>)?`.

`?` is the only implicit control flow in the language. The LSP highlights every `?` as an exit point. In the Arabic skin it prints as `؟`.

**`AnyError`** is the application-level error box (comparable to Rust's `anyhow`). It is opaque, not dynamically typed:
- `.message()` and `.chain()`;
- `.downcast() -> Option[E]`, where `E` comes from the expected type: `let db_err: Option[DbError] = e.downcast()`;
- `result.context("loading config")` turns any `Result[T, E: Error]` into `Result[T, AnyError]` and adds a message.

`main` may return `()` or `Result[(), E]` with `E: Error` (usually `AnyError`). If it returns `Err`, the message chain goes to stderr, with secrets redacted, and the exit code is 1.

**Stdlib conversions for web apps:** `impl HttpError as From[DbError]` (→ 500; details are logged, not shown), `From[DecodeError]` (→ 400) and `From[AnyError]` (→ 500). A handler that returns `Result[Response, HttpError]` can therefore use `?` directly on database and decoding calls.

### 14.4 Panics

- Raised by `panic(msg)`, `assert(cond, msg)`, `opt.expect(msg)`, `res.expect(msg)`, out-of-bounds indexing, `Int`/`Decimal` overflow, division by zero, and stack exhaustion.
- There is no bare `.unwrap()`. `.expect("reason")` requires a reason, so every forced unwrap documents its invariant (K0915 fixes `unwrap()`).
- **No user code runs during unwinding** (there are no destructors or `finally`). **The runtime rolls back any open DB transaction and releases locks** held by the task (Security 13.4).
- **Panic messages redact `Secret` values.** Production builds never render stack traces, and they have **no runtime switch** that turns traces on. A dev build is a different artifact (04 S11).
- Where a panic stops:
  - **HTTP handler or live event:** the response is 500 with a request ID. The panic is logged. The server keeps serving.
  - **Main task:** the message goes to stderr (plus a Kalam-source trace via source maps, in dev builds only). The exit code is 101.
  - **Child task in `task.scope`:** its siblings are cancelled and the panic re-raises in the parent.
  - **Tests:** `test.expect_panic(fn() => …)` asserts that a panic happens.

---

## 15. Concurrency without an `async` keyword (D12)

### 15.1 Model

- A program runs in an **isolate**: one heap and one event loop. v0.1 has one isolate per process. Multi-isolate message passing is planned.
- Inside an isolate, **tasks** are scheduled cooperatively. A task can be suspended **only during a call to a suspending function**.
- Source code looks synchronous. There are no `async` or `await` keywords, no promises and no callbacks in user code (K0907 removes `async`/`await` written by mistake).

### 15.2 Suspension inference

- The **base suspending functions** are stdlib I/O primitives (network, filesystem, database, timers, `task.*` joins, `Screen.next_frame`) and externs declared `@js(async: true)`.
- A function is **suspending** if its body calls a suspending function, or calls a function-typed value whose suspension bit is set. Inference is a fixpoint per strongly connected component of the call graph, so mutual recursion works.
- The bit is never written in source. **Tooling obligation:** the LSP and code-review renderer MUST mark every suspension point with an inlay hint (`⏸`) and show "suspends" on hover. This replaces the visual cue `await` would have given. The inlay is not in the file, so it does not affect skins or hashes.

### 15.3 Function types, higher-order functions and stored functions (DR-13)

- Every function type has a hidden bit `s ∈ {sync, susp}`. A sync function may be used wherever a suspending one is expected.
- **Function-typed parameters are effect-polymorphic.** In `fn map[T, U](xs: List[T], f: fn(T) -> U) -> List[U]`, the bit of `f` is an implicit effect variable, and `map` suspends exactly when it is instantiated with a suspending `f`. The compiler emits at most two versions (sync and suspending), only those that are used.
- **Function types inside data** (record fields, enum payloads, list and map elements, extern parameters) are **suspending by default**. Storing a sync function there is allowed, and calling it is treated as a possible suspension. Write `@sync fn(…) -> R` in the field type to require and preserve a sync function. No whole-program analysis is needed, and a library's lowering never depends on its users.
- Lambdas get their bit from their body.

### 15.4 Requiring a non-suspending callback

- `@sync` on a function type (`update: @sync fn(State, Input, Float) -> State`) means the argument must not suspend. Passing a suspending function is **K1401**, and the diagnostic shows the chain that makes it suspend (*"`update` calls `save_score`, which calls `db.exec` (suspends)"*).
- `@sync` on a declaration (`@sync fn price(…)`) asserts that it does not suspend (K1402 if it does). It freezes an API promise.
- **The stdlib uses `@sync` for:** game `update`/`view` callbacks, `Cell.update`, comparators in `sort_by`, live-component `update`/`view` when they are pure, and template helpers called during rendering.

### 15.5 Suspension is part of the API

The suspension bit of every exported function is stored in the module interface (`.kli`). A change from sync to suspending is a **breaking change**: `kalam pkg diff` reports it, and the registry requires a major version bump. The other direction is compatible.

### 15.6 Shared state and atomicity

- **Between two suspension points, a task runs without interruption.**
- Values are immutable, closures cannot capture `var`, and there is no module-level `var`. So the **only shared mutable state** is `Cell[T]` plus capability-backed external stores (databases, caches).
- `Cell[T]`: `Cell.new(v)`, `.get()`, `.set(v)`, `.update(@sync fn(T) -> T)`. The update function cannot suspend, so **`update` is atomic**. A `Cell` cannot be stored in a module-level constant.
- **Result:** data races are impossible in v0.1 (04 L11). Race *conditions* against external stores stay visible as separate calls and are handled with DB transactions (serializable by default in the first-party Postgres driver). Lint W0421 flags a value read from a `Cell` before a suspension point and used after it (the in-process TOCTOU shape).

### 15.7 Structured concurrency (`std/task`)

| Function | Meaning |
|---|---|
| `task.all(fs: List[fn() -> T]) -> List[T]` | run concurrently, results in order; the first panic cancels the rest |
| `task.both(f: fn() -> A, g: fn() -> B) -> (A, B)` | typed pair version (`@positional`) |
| `task.race(fs: List[fn() -> T]) -> T` | first to finish wins; the others are cancelled |
| `task.timeout(clock: Clock, ms: Int, f: fn() -> T) -> Result[T, TimedOut]` | deadline |
| `task.sleep(clock: Clock, ms: Int)` | suspend |
| `task.scope(body: fn(Scope) -> T) -> T` | `scope.spawn(f)` starts a child; the scope waits for every child before returning |

- **No task outlives its parent scope.** There is no unscoped fire-and-forget. Work that should outlive a request goes to the DB-backed job queue (`std/jobs`).
- **Cancellation is cooperative and unobservable** (DR-14). A cancelled task stops at its next suspension point and unwinds like a panic: no user code runs, open transactions roll back, locks are released. Kalam code cannot catch it or mistake it for an ordinary `Err`. The scope's caller sees a normal result, or the sibling's panic.

### 15.8 Lowering sketch

```kalam
fn load_dashboard(app: App, user_id: Int) -> Result[Dashboard, AnyError]:
    let (user, invoices) = task.both(
        fn() => users.find(app.db, user_id),
        fn() => invoices.recent(app.db, user_id),
    )
    return Ok(Dashboard(user: user?, invoices: invoices?))

fn names(users: List[User]) -> List[String] => users.map(fn(u) => u.name)
```

```js
export async function load_dashboard(app, user_id) {           // suspending
  const [user, invoices] = await $task.both(
    async () => await users.find(app.db, user_id),
    async () => await invoices.recent(app.db, user_id),
  );
  if (!user.ok) return $Err($box(user.error));                 // ? rule 3: boxed into AnyError
  if (!invoices.ok) return $Err($box(invoices.error));
  return $Ok(new Dashboard(user.value, invoices.value));
}
export function names(users) { return $List.map(users, (u) => u.name); }   // sync instance
```

---

## 16. Capabilities and effects for I/O (04 L12)

### 16.1 Rules

1. Only `main` receives the root capability: `fn main(world: World)`. `World` cannot be constructed or decoded (K1501). A program that needs nothing declares `fn main():`.
2. **Ambient (no capability needed):** pure computation; `print`, `log.*` (X9, §16.4); `panic`; `Cell`; JSON, strings and math; `Duration` arithmetic; hashing; and crypto that needs no randomness.
3. **Capability-gated**, and only obtainable by being passed in:

| Capability | From `World` | Grants | Attenuation |
|---|---|---|---|
| `Net` | `world.net` | outbound HTTP, listening sockets. TLS certificate verification is always on and no attenuation or option turns it off (04 S12) | `.only(hosts: […])`; `.allow_private()` (needed to reach SSRF-blocked ranges, 04 S12) |
| `Fs` | `world.fs` | files | `.dir(path"…") -> Result[Fs, FsError]` (rooted, `..`-safe, symlinks that leave the root refused, 04 S13); `.read_only()` |
| `Env` | `world.env` | environment variables | `.only([…])`. Keys matching `*_KEY`, `*_SECRET`, `*_TOKEN` or `PASSWORD` come back as `Secret[String]` (04 S5) |
| `Clock` | `world.clock` | current time, timers | — |
| `Rand` | `world.rand` | CSPRNG (04 S7) | — (`Rand.seeded(n)` is a deterministic PRNG that needs no capability and has no `token`/`uuid` methods) |
| `Proc` | `world.proc` | running `Command`s (never a shell) | `.only(programs: [path"/usr/bin/git"])` |
| `Stdin` | `world.stdin` | reading standard input | — |
| `Screen` | `world.screen(width:, height:)` | canvas and input (`js-browser` target only, K1502 elsewhere) | — |

4. **Capabilities are ordinary values** with no public constructor, no `Decode`, no `Eq` and no `Hash`. They show as `<Net>`. They can be passed, captured and stored in records, but never in module-level constants. Read a signature to know what a function can do: `fn charge(net: Net, order: Order)` can reach the network. `fn total(order: Order) -> Decimal` provably cannot.
5. Higher-level handles are built from capabilities: `sql.postgres(world.net, url: secret)` returns `Result[Db, DbError]`. A library that needs a database takes `db: Db`. It never takes a connection string and opens the connection itself.
6. **Testing:** `std/test` provides fakes: `test.clock(at:)`, `test.net(routes)`, `test.fs(files)`, `test.env(map)`, `Rand.seeded(n)`.
7. There is no reflection and no call-by-name, so there is no `reflect` capability (04 L9).

### 16.2 Formal rule

For any function `f`, the effects `f` can perform are bounded by the capability *types* reachable from its parameters and captured values, plus the effects of the `extern`s its package declares. `kalam caps @team/notes` reports this set for every exported function, and the registry shows it.

### 16.3 Web apps

Applications collect their capabilities into their own record and hand it to the server once:

```kalam
type App:
    db: Db
    clock: Clock
    payments: Net
```

Handlers receive `Request[App]` and reach capabilities through `req.app`. Each handler's power is still visible from the `App` type, which is ordinary code.

### 16.4 Ambient console and defence in depth

- `print` writes to stdout and `log.*` writes structured lines to stderr, with no capability (deviation X9, locked by the founder as D20). A **library** package that calls them must declare `console = true`, which appears in the capability diff. In production web servers, stdout goes into the structured logger, which escapes CR/LF and ANSI sequences (CWE-117). `Show` redacts `Secret`s.
- **Defence in depth on JS:** the launcher that `kalam build` generates derives OS-level permission flags from the root manifest's capability list (Node `--permission` with `--allow-fs-read=…`, Deno `--allow-net=…`). A bug in an extern binding then still hits a second wall (Security 15.5).
- **Worked threat:** a compromised dependency with API `pad(s: String, n: Int) -> String` cannot read `~/.aws/credentials` or call home. No capability reaches it, it cannot construct one or find one in a global, and its only route to ambient authority would be an `extern`. That needs an `[npm]`/`ffi` entry, which shows up in the capability diff and in `kalam audit` (04 §3.3).

---

## 17. Safe string kinds (typed sinks)

### 17.1 The rule

**A sink never accepts `String`** (04 S1). An API that interprets text as code or as a structured command takes a distinct type, and that type can come from only three places:

- a **tagged literal**, whose literal parts the developer wrote and whose holes are encoded for their context;
- a **validating parser** that returns `Result`;
- an **`unsafe.*` call**.

There is no implicit or explicit `String → Sql` cast. Passing a `String` gives K1601: *"`db.all` expects `Sql`, but this is `String`. Write the query as `sql"…"` and put values in `{…}`."* When the rewrite is mechanical, the fix turns `"…" + x` concatenation into a tagged literal.

| Tag | Type | Example sinks | Hole handling | Escape hatch (counted by audit) |
|---|---|---|---|---|
| `sql"…"` | `Sql` | `db.all`, `db.one`, `db.exec` | **bound parameter**, never spliced | `unsafe.raw_sql(s)` |
| `html"…"` | `Html` | responses, components, live updates | **contextual auto-escape** | `unsafe.raw_html(s)` |
| `cmd"…"` | `Command` (argv) | `proc.run` | **exactly one argv element**; no shell, ever | **none** |
| `url"…"` | `Url` | `http.get`, `href`/`src` holes, `redirect` | percent-encoded per URL component | `unsafe.raw_url(s)` |
| `path"…"` | `Path` | `fs.read_text`, `fs.dir` | hole must be `PathSegment` | none |
| `regex"…"` | `Regex` | `.matches`, `.replace` | hole escaped as literal text | none |

- Tags are lowercase ASCII names, identical in every skin, resolved statically. The stdlib tags are always in scope. A third-party tag comes into scope when its module is imported. Two tags with one name in scope are K1650. `sql "…"` (with a space) is a call on a `String` and is rejected because sinks never take `String`.
- **Tag names form their own namespace**, because a tag is recognized only directly before a quote. The module `std/sql` (bound as `sql`) and the tag `sql` therefore coexist, and neither shadows the other.
- Values of one safe type compose with `+` or `Sql.join(parts, sep:)` / `Html.join(parts)`.

### 17.2 Literal provenance (the soundness property)

The compiler passes a tag's builder a value of type `StaticParts`. Only the compiler can create one, and only from literal source text. The holes arrive separately as typed values. Therefore:

- runtime strings can never become the *structure* of a query, a page or a command;
- the structure of every `Sql`/`Html`/`Command`/`Url`/`Path` value appears verbatim in the source, so `kalam audit --sinks` can list every query a program can ever run.

The stdlib tags parse their literal text **at compile time**. Malformed SQL quoting, unbalanced HTML and shell metacharacters are compile errors (Security 16.2).

### 17.3 `sql"…"` → `Sql`

- Each `{expr}` becomes a **bound parameter**, with placeholders generated per driver (`$1` for Postgres, `?` for SQLite). Accepted hole types implement `SqlParam`: `Int`, `BigInt`, `Float`, `Decimal`, `String`, `Bool`, `Bytes`, date/time types, `Option[T]` (as `NULL`), `List[T]` (expanded for `IN {ids}`; an empty list is `DbError.EmptyList` at runtime, never invalid SQL), and `Sql` (splices a fragment that is itself safe).
- **A hole inside SQL quotes** (`'{name}'`) is **K1602** ("this hole is inside quotes; remove the quotes, the value is sent as a parameter").
- **A hole in an identifier position** (table or column name, `ORDER BY {col}`, `ASC`/`DESC`) is **K1603**. Dynamic identifiers come only from `sql.ident(name, allowed: […]) -> Result[Sql, DbError]`, or from an enum whose impl returns literal identifiers. User input picks a variant. It never names a column.
- **Multiple statements** (a `;` outside quotes) are **K1604**. Each `Sql` value is exactly one statement. `unsafe.raw_sql` also rejects a top-level `;` at runtime.
- `--` and `/* */` comments are allowed in the literal. A hole inside a comment is **K1605**.
- **`LIKE` wildcards:** `sql.like_contains(s)` (and `like_prefix`, `like_suffix`) escapes `%`, `_` and `\`. A raw `String` in a `LIKE` hole draws **W1606**.
- `Db` API (`std/sql`): `db.all(q) -> Result[List[T], DbError]` with `T: Row` taken from the expected type, `db.one(q) -> Result[Option[T], DbError]`, `db.exec(q) -> Result[Int, DbError]` (rows affected), and `db.transaction(f: fn(Db) -> Result[T, E]) -> Result[T, E]` with `E: From[DbError]`. A transaction commits on `Ok` and rolls back on `Err`, on panic or on cancellation.
- Rows decode **schema-first** by column name. A missing or extra column, or an `Int` column value outside the i64 range (for example a Postgres `numeric`), is `Err(DbError.Shape)`. A Postgres `bigint` always decodes into `Int` exactly; the driver's text form is parsed, never passed through a JS `number`.
- `kalam db check` (P2, 04 S16) optionally verifies every `sql` literal against a migration-derived schema.

### 17.4 `html"…"` → `Html` (contextual auto-escaping, 04 S3)

The literal is tokenized as an HTML5 fragment at compile time. It must be well-formed (balanced tags, valid nesting, quoted attributes), or it is K1610. Each hole is classified by context (Go `html/template` semantics):

| Hole position | Accepted types | Treatment |
|---|---|---|
| text content `<p>{x}</p>` | `Html`, `List[Html]`, `Option[Html]`; any other `T: Show` | `Html` is inserted as is (already safe). `None` renders nothing. `Show` values are HTML-escaped. |
| quoted attribute value `class="{c}"` | `String`, `Int`, `Bool`, `AttrValue` | attribute-escaped |
| **URL attribute** (`href`, `src`, `action`, `formaction`, `poster`, `srcset`, `k-get`, `k-post`, …) | **`Url` only** (K1613 for `String`) | `Url` cannot hold `javascript:`, `data:` or `vbscript:` unless it was built with `unsafe.raw_url` |
| `k-click`, `k-submit`, `k-input`, `k-change` | a type that derives `Message` (K1616) | encoded as a signed opaque token (§20.3) |
| unquoted attribute `class={c}` | — | **K1611** (fix-it adds quotes) |
| `on*` attribute, `<script>`/`<style>` content, `style` attribute | — | **K1612**: no holes. The one exception is `{json(x)}` inside `<script type="application/json">`, which escapes `<`, `>`, `&`, U+2028 and U+2029. |
| tag name or attribute name (`<{t}>`, `<a {n}="x">`) | — | **K1614**. Use `Attrs` for a dynamic *set* of attributes with closed names. |
| inside an HTML comment | — | **K1615** |
| unknown `k-*` attribute | — | **K1617** |

- Forms with `method="post"` get a CSRF token injected automatically (04 S8). Pages carry a nonce-based CSP and Trusted Types (04 §4.3).
- **RTL:** the first-party UI kit wraps interpolated text holes in `<bdi>` when the surrounding element has a fixed `dir` (§6.5).

### 17.5 `cmd"…"` → `Command` (no shell, ever: 04 S2)

```kalam
let c = cmd"git log --oneline -n {count} -- {file}"
let out = proc.run(c)?
```

- The literal is split into argv words **at compile time**, at whitespace outside double quotes. Each hole is **exactly one argv element**, or part of one when it is glued to literal text (`--author={name}`). Hole values are never split, globbed, or tilde- or variable-expanded.
- **The program (first word) must be literal text** (K1621).
- **Shell metacharacters** outside quotes (`| ; & && || > < >> $( ) \` * ? ~ !`) are **K1620** ("Kalam never runs a shell. Use `proc.pipe([…])` for pipelines, and `stdin:`/`stdout:` for files").
- **Argument-injection guard:** if a whole-word hole *value* starts with `-` and no literal `--` precedes it, `proc.run` returns `Err(.OptionLikeArgument(index))` without starting the process. The compiler warns at such holes (**W1622**) and suggests adding `--` (Security 16.5).
- Hole types: `String`, `Int`, `Path`, `PathSegment`. A `List[String]` hole expands to one element per item.
- **The child process gets an empty environment** unless `env:` is passed explicitly. `Proc.only([…])` restricts which programs may run.
- `Command` has **no raw form**. A program that genuinely needs a shell writes `cmd"/bin/sh -c {script}"`, and `kalam audit` flags every `Command` whose program is a known shell interpreter.

### 17.6 `url"…"`, `path"…"`, `regex"…"`

- `url"https://api.example.com/users/{id}?q={term}"`: the scheme and host must be literal (K1631). Holes are percent-encoded for their component (path segment, query key or value, fragment). Runtime URLs come from `Url.parse(s) -> Result[Url, UrlError]`, which rejects `javascript:`, `data:` and `vbscript:`. `Response.redirect(to: Url)` additionally requires a relative URL or a host on the app's allow-list (04 S17).
- `path"uploads/{name}"`: a hole must be a `PathSegment`, not a `String` (K1641, fix-it `PathSegment.parse(name)?`). `PathSegment.parse` rejects empty strings, `.`, `..`, `/`, `\`, NUL, drive letters and Windows reserved names. `Path.join(base, rel)` returns `Result` and refuses to escape `base` (04 S13). Uploads are stored under random names and are never executable.
- `regex"^[a-z0-9_]\{3,20\}$"` is compiled and syntax-checked at compile time with **RE2 semantics**: linear time, no backreferences or lookaround, no ReDoS (CWE-1333). A hole is escaped as literal text. `Regex.parse(s) -> Result` exists for runtime patterns and uses the same engine.

### 17.7 What tagged literals do not solve

Tagged literals prevent **injection**. They do not prevent authorization bugs (handled by deny-by-default routes, §20.2), business-logic errors, or second-order data problems (stored HTML-looking text is safe only because output is escaped again at its sink). These limits are stated so the guarantee is not oversold.

### 17.8 User-defined tags

```kalam
@tag("graphql")
impl GraphQl as Template:
    fn build(parts: StaticParts, holes: List[TemplateHole]) -> GraphQl:
        …
```

A package can define its own safe-string type (`graphql"…"`, `ldap"…"`) by implementing `Template` and registering a tag. The compiler guarantees `StaticParts` provenance. The package owns its encoder, and it may validate the template's structure at compile time with a `const fn check(parts: StaticParts) -> Result[(), String]`. `kalam audit` lists third-party `Template` impls as security-relevant code.

---

## 18. Secrets, randomness and decoding

### 18.1 `Secret[T]`

- Created by config loaders (`env.get` on secret-like names, `config.load()` for fields typed `Secret[…]`), by decoding a `Secret`-typed field (for example a password form field), and by `Secret.new(x)`.
- `Show` prints `[REDACTED]`. **There is no `Encode`**, so putting a secret into JSON is a compile error, not a redaction (K0612). **There is no `Eq`, `Ord` or `Hash`.** Compare with `crypto.equal(a, b)`, which is constant time (04 S5).
- `.expose() -> T` returns the value. It is an escape hatch: ASCII in every skin and counted by `kalam audit` (04 L13).
- Memory is zeroized where the backend allows. On JS this is best effort, so `Secret[Bytes]` is preferred for keys.

### 18.2 Randomness

`world.rand` is a CSPRNG (`crypto.getRandomValues` on JS). It provides `rand.int(0..10)`, `rand.token(bytes: 32) -> Secret[String]` and `rand.uuid_v4()`. `Rand.seeded(n)` is a deterministic PRNG for games, simulations and tests. It needs no capability and **has no `token` or `uuid` methods**, so using it for secrets is a type error by omission (04 S7).

### 18.3 Decoding (schema-first, 04 S4, S10)

- `json.decode(input) -> Result[T, DecodeError]`, with `T` taken from the expected type, builds exactly a `T`. It never builds a type named by the payload. `Decode` is derived structurally and cannot be hand-written. The only user code a decoder ever runs is an opaque type's `@decode_with` validator, which is pure, non-suspending, capability-free, and chosen by the static target type.
- **Default limits:** 1 MiB input, depth 32, 10,000 elements per container, 64 KiB per string. **Unknown fields are rejected. Duplicate keys are rejected.** Integer tokens are parsed **exactly** from the input text, never through a lossy floating-point step (on JS, never via `JSON.parse` into a `number`). Integers outside the `Int` range [−2⁶³, 2⁶³−1] are rejected for `Int` targets. A JSON number with a fraction or exponent is rejected for `Int` targets. NaN and Infinity are rejected. Override per call with `limits: DecodeLimits(…)`.
- The same rules apply to request binding (`req.json()`, `req.form()`, `req.query()`) and to DB row decoding.
- **Mass-assignment lint W0710:** decoding request data directly into a type that also derives `Row` (a DB row type) is flagged. Define a separate input type, so fields such as `role` or `owner_id` can never be bound from a request (04 S10; Q9 asks whether this should become an error).

### 18.4 Cryptography

`std/crypto` exposes only high-level, misuse-resistant APIs: `password.hash`/`verify` (Argon2id), `aead.seal`/`open`, `sign`/`verify` (Ed25519), `hkdf`, `mac` and `crypto.equal`. MD5, SHA-1 and raw RSA live only under `std/crypto/legacy`. Importing it draws W0720, and `kalam audit` counts it (04 S6).

---

## 19. Interop with JavaScript and npm

### 19.1 `extern` declarations

`extern` is **fixed ASCII in every skin** (DR-16, 04 L13).

```kalam
@js(module: "nanoid", name: "nanoid")
extern fn nanoid(size: Int) -> String

@js(module: "stripe", name: "default")
extern type StripeClient

@js(module: "stripe", name: "default", new: true)
extern fn stripe_client(key: Secret[String]) -> StripeClient

@js(member: "checkout.sessions.create", async: true, throws: true)
extern fn create_session(client: StripeClient, params: SessionParams) -> Result[Session, JsError]
```

| `@js` option | Meaning |
|---|---|
| `module`, `name` | ESM import specifier and export name (`"default"` allowed). The module must be listed in `[npm]` (K1701) |
| `new: true` | call as a constructor |
| `member: "a.b.c"` | call a member path on the first argument (a method call) |
| `async: true` | the JS function returns a Promise, so this extern **suspends** (§15.2) |
| `throws: true` | JS throws and rejections are caught, and the return type must be `Result[T, JsError]`. Without it, a JS throw becomes a Kalam **panic** at the call site |
| `global: "fetch"` | a global instead of a module import. The package must declare the matching capability |

- Any module containing `extern` makes its package an **FFI package**. That shows on the registry page and is counted by `kalam audit`. A capability parameter in an extern signature (`net: Net`) is a *token*: the glue does not pass it to JS, but Kalam callers must hold one. The manifest's `[npm] caps` and the runtime permission flags back the claim (§16.4).
- `Secret[T]` parameters pass the raw value. The declaration is the explicit, counted exposure.

### 19.2 Boundary validation (checked on every crossing)

| Kalam | To JS | From JS (failure → panic, or `Err(JsError.Shape)` with `throws: true`) |
|---|---|---|
| `Int` | `number` when the value is in the safe range ±(2⁵³−1), `bigint` otherwise (`.d.ts`: `number \| bigint`) | a `number` that passes `Number.isSafeInteger`, or a `bigint` in [−2⁶³, 2⁶³−1] (normalized to `number` when safe). A `number` outside the safe range fails, because it may already have been rounded. **No `parseInt`, no truncation** |
| `BigInt` | `bigint` | `bigint`, or a safe integer `number` |
| `Float` | `number` | `typeof === "number"` |
| `Decimal` | `string` (canonical decimal text) | a `string` that parses as a Decimal |
| `Bool` | `boolean` | `typeof === "boolean"` (no truthiness) |
| `String` | `string` | `typeof === "string"` **and well-formed UTF-16** (a lone surrogate fails) |
| `Bytes` | fresh `Uint8Array` copy | `Uint8Array`/`ArrayBuffer`, copied |
| `()` | `undefined` | ignored |
| `Option[T]` | value or `undefined` | `undefined` **or `null`** → `None` |
| `List[T]` | fresh frozen `Array` | `Array.isArray`, each element validated, **copied** |
| `Map[K, V]` | fresh `Map` | a `Map` instance, entries validated, copied |
| record | fresh object with a `null` prototype | an object. **Each declared field is read with `Object.getOwnPropertyDescriptor` and must be an own data property, so getters never run.** The prototype chain is never consulted. `__proto__`, `constructor` and `prototype` keys are rejected. Extra properties are rejected under `@js(strict)` |
| enum | `{ tag: "Circle", radius: 1.5 }` | the same shape, with the tag checked against the variant list |
| `fn(A) -> R` | a wrapper that validates its arguments | a JS function wrapped as a Kalam function, with its results validated. It is treated as suspending if declared `async` |
| `JsValue` | as is | **opaque and unvalidated.** It can only be decoded with `js.decode(v) -> Result[T, JsError]` or passed back to JS. `unsafe.js_cast(v)` exists and is counted |
| capabilities | cannot cross (K1702), except as erased tokens | — |

Copying at the boundary means JS can never mutate a Kalam value, so immutability holds even next to buggy or hostile npm code (Security 18.3).

### 19.3 Binding generation from TypeScript (`kalam bind`, v0.1)

`kalam bind npm:stripe@17.5.0 --only Stripe,checkout.sessions` reads the `.d.ts` files and generates a reviewable `extern` module:
- interfaces become `@derive(Encode, Decode)` records;
- classes become `extern type` plus `member:` externs;
- `Promise<T>` becomes `async: true`;
- **TypeScript `any` and `unknown` become `JsValue`**, never a Kalam type.

Unsupported TS features (overloads, conditional and mapped types) become `# TODO(bind)` comments. Curated bindings are published as ordinary packages (`@kalam-bind/stripe`), so applications rarely write `extern` themselves (06 §5.3). Generated externs count in `kalam audit` exactly like hand-written ones.

### 19.4 Calling Kalam from JS/TS

`@js.export` on an exported function emits it in an ESM entry with a `.d.ts`:

| Kalam | TypeScript |
|---|---|
| record | `readonly` interface |
| enum | discriminated union `{ tag: "Circle"; radius: number }` |
| `Option[T]` | `T \| undefined` |
| `Result[T, E]` | `{ ok: true; value: T } \| { ok: false; error: E }` |
| suspending function | returns `Promise<T>` |
| panic | a thrown `KalamPanic`, with no stack trace in production builds |

Arguments coming from JS are validated with the §19.2 rules, so JS callers cannot break Kalam's invariants either.

---

## 20. Standard library and the web kit

The rule from 06 §5.3 applies: **the beginner path to a working SaaS must be first-party.** npm is for advanced users. Stdlib names are English in every skin (§7.2). Every function listed here has a runnable doc example, and each example is a test (05 D8).

### 20.1 Module map (v0.1)

| Module | Contents | Capability |
|---|---|---|
| prelude | core names, `BigInt`, `Bytes`, `Html` (`.render() -> String`), `Sql`, `Command`, `Url`, `Path`, `PathSegment`, `Regex`, `Secret`, `World`, `AnyError`, `Never`, `Cell`, capability types, `log`, `json`, stdlib tags | — |
| `std/string` | `len_chars`, `byte_len`, `chars`, `graphemes`, `slice`, `split`, `join`, `trim`, `is_empty`, `starts_with`, `contains`, `replace`, `to_upper`, `to_lower`, `nfc`, `to_int → Result[Int, ParseError]`, `to_float`, `to_decimal`, `arabic_fold` | — |
| `std/list`, `std/map`, `std/set` | `len`, `is_empty`, `get`, `set`, `append`, `insert`, `remove`, `map`, `filter`, `fold`, `find`, `any`, `all`, `sort_by(@sync cmp)`, `sorted` (`T: Ord`), `enumerate`, `zip`, `take`, `skip`, `first`, `last`, `group_by`, `sum` | — |
| `std/option`, `std/result` | `map`, `or`, `ok_or`, `expect`, `is_some`, `is_none` / `map`, `map_err`, `ok`, `expect`, `context`, `is_ok` | — |
| `std/math` | `PI`, `abs`, `min`, `max`, `clamp`, `sqrt`, `floor`, `round`, `pow` | — |
| `std/decimal`, `std/bigint` | `Decimal.from`, `Decimal.parse`, `div(b, scale:, rounding:)`, `round(scale:)`, `Rounding`; `BigInt.from`, arithmetic | — |
| `std/json` | `json.encode(T: Encode) → String`, `json.decode(String) → Result[T, DecodeError]` with limits | — |
| `std/time` | `Instant`, `Date`, `Duration`, `Zone`, `clock.now()`, `clock.today(zone)`, Hijri formatting | `Clock` to read the time |
| `std/log` | `log.info/warn/error`, structured fields, secrets redacted | ambient (X9) |
| `std/env` | `env.get(key) → Option[String]`, `env.secret(key) → Option[Secret[String]]`. A literal key that looks secret passed to `env.get` is K1503 | `Env` |
| `std/fs`, `std/path` | `fs.read_text`, `fs.write_text`, `fs.list`, `fs.dir`; `Path.join` → `Result` | `Fs` |
| `std/http` | `http.router()`, `Router[A]` with `.get/.post/.put/.delete(path, handler, access:)` and `.live(…)`, `http.serve(net, port:, router:, app:)`, `Request[A]` (`.app`, `.path_int`, `.path`, `.query_param`, `.query`, `.json`, `.form`, `.session`), `Response` (`.json`, `.html`, `.text`, `.redirect`, `.status`), `HttpError`, `Access`, and the client `http.get(net, url)` etc. (SSRF-blocking, 04 S12) | `Net` |
| `std/web` | layouts, form binding into declared types, live components and `k-*` runtime (§20.3), the RTL-first component kit, flash messages, assets with no Node toolchain | via `std/http` |
| `std/auth` | cookie sessions (`__Host-`, `Secure`, `HttpOnly`, `SameSite=Lax`), `password.hash`/`verify`, login rate limiting | `Rand`, `Clock` |
| `std/sql` | `sql.sqlite(fs, path)`, `sql.postgres(net, url: Secret[String])`, `Db`, `sql.ident`, `sql.like_contains`, `DbError`, migrations (`kalam db migrate`) | `Fs` / `Net` |
| `std/jobs` | DB-backed job queue with retries | `Db` |
| `std/task` | §15.7, plus `Cell` | `Clock` for timeouts and sleep |
| `std/proc` | `proc.run(Command, env:) → Result[Output, ProcError]`, `proc.pipe` | `Proc` |
| `std/crypto` | §18.4 | `Rand` where random |
| `std/test` | `test` blocks, `expect_eq`, `expect_panic`, snapshot tests, fake capabilities | — |
| `std/i18n` | message tables for `en`/`ar`/`franco`, plural rules, number formatting with Arabic-Indic digits | — |
| `std/game` | `game.run(screen, init:, update: @sync fn(S, Input, Float) -> S, view: @sync fn(S) -> Picture)`, `Input`, `Key`, `Picture`, `draw.*`, `Color` | `Screen` |
| `std/js` | `JsError`, `JsValue`, `js.decode` | FFI packages |
| `unsafe.*` (pseudo-module) | `raw_html`, `raw_sql`, `raw_url`, `js_cast` | counted by audit |

**Planned for 0.2, not part of v0.1:** Postgres pooling options, OAuth, Paymob/Fawry/Stripe payment adapters (06 §5.4), S3 storage, websockets for app code, and client islands.

### 20.2 HTTP essentials (deny-by-default routes)

```kalam
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

# documentation notation for methods declared in impl blocks of std/http
fn get[A](self: Router[A], path: String, handler: fn(Request[A]) -> Result[Response, HttpError],
          access: Access) -> Router[A]
fn serve[A](net: Net, port: Int, router: Router[A], app: A) -> Result[(), ServeError]
fn path_int[A](self: Request[A], name: String) -> Result[Int, HttpError]       # 400 on bad input
fn json[A, T: Decode](self: Request[A]) -> Result[T, HttpError]                 # 400 on bad input
```

- **Route patterns are plain `String`s in v0.1**, so a path parameter's braces must be escaped (`"/notes/\{id\}"`); an unescaped `{id}` is an interpolation hole (§5.7). A dedicated route-pattern syntax is open question Q15.
- **`access:` has no default. Forgetting it is a compile error** (a missing argument). That is how routes are deny-by-default (04 S9).
- `.LoggedIn` and `.Role` use the session middleware from `std/auth`, which `http.router()` installs. A failing check returns 401 or 403 before the handler runs.
- Server defaults (04 S8): CSRF on, cookies `Secure`/`HttpOnly`/`SameSite=Lax`, CORS closed, CSP + `nosniff` + `frame-ancestors` + `Referrer-Policy` + HSTS, and body, header and time limits.

### 20.3 Server-driven UI (D11): live components and hypermedia directives

**Live components** (typed, TEA-style, DR-32). A live component is three ordinary things and no new syntax:

1. a **state** type;
2. an **update** function `fn(State, Msg) -> State` (or `-> Result[State, E]`). It may take capabilities through the app record, and it is suspending if it touches I/O;
3. a **view** function `fn(State) -> Html`.

`Msg` is an enum with `@derive(Message)`. In a view, the attributes `k-click`, `k-submit`, `k-input` and `k-change` take a hole whose type derives `Message` (K1616):

```kalam
<button k-click="{Msg.Increment}">+</button>
<form k-submit="{Msg.Save}"> … </form>
```

Registration ties the three together, and the compiler checks that the message type of `update` matches every `k-*` message hole in the literals inside `view`'s body (K1618):

```kalam
http.router().live("/counter", init: Model(count: 0), update: update, view: view, access: .Public)
```

**Runtime model:**

1. The page loads with a small first-party client script. No hand-written JS is needed.
2. An event sends the message to the server over SSE or WebSocket as an **opaque, signed, CSRF-bound token**. The token binds the session, the component instance, the message type ID and the encoded value. It is never code (Security 16.4.1).
3. The server verifies the token (a forged or mismatched token is rejected with 400 and never decoded into the wrong type), runs `update` then `view`, diffs the new `Html` against the previous one, and sends a minimal patch.
4. The browser runtime inserts patches only through the `kalam` Trusted Types policy, which accepts nothing except framework-produced `Html` (04 §4.3).
5. Form fields bound to a message decode into **declared types only** (04 S10).

State is kept in server memory per connection by default. The multi-instance strategy belongs to the web kit (Q7).

**Hypermedia directives** for plain actions (htmx-style): `k-get="{url}"`, `k-post="{url}"`, `k-target="#id"` and `k-swap="append|replace|…"`. URL holes must be `Url` (K1613). The `k-` namespace is reserved, and unknown `k-*` attributes are K1617.

---

## 21. Canonical form for LLMs, diagnostics and tooling

### 21.1 What "canonical Kalam" is

**Canonical text** is English-skin source exactly as `kalam fmt` prints it. Docs show it first, `llms.txt` teaches it, and agents are asked to write it. Humans read and edit in any skin, because conversion is deterministic (06 §7.2). **Canonical identity** is the KCF hash (§4.4).

**Formatter rules** (not configurable, except `[skin.ar] digits`):

1. 4-space indentation, 100 columns. Long lines break inside brackets, with one item per line and a trailing comma.
2. One statement per line. A one-line block is kept only for one simple statement that fits.
3. `use` declarations come first and are sorted: `std/` first, then dependencies, then the current package. Names inside `{…}` are sorted.
4. Items are separated by one blank line, and blank lines are capped at one.
5. Strings use double quotes. Multi-line text uses `"""`.
6. Franco variants are rewritten to canonical spellings, Arabic keywords to their canonical hamza spelling, and digits per §6.2. Tatweel and harakat are removed from keywords and identifiers.
7. Redundant backticks, parentheses and `pass` are removed.
8. Attributes go one per line above the item.

### 21.2 Rules that exist for writability

| Rule | Why it helps an LLM (and a reviewer) |
|---|---|
| Fully annotated top-level signatures | Errors stay local, and the model generates a body against a fixed contract |
| Explicit `return` in block bodies | Removes the Rust/Ruby/Python ambiguity about implicit returns |
| No shadowing, no globals, no `var` capture | Removes classes of silent bugs that models introduce often |
| No glob imports, absolute module paths | Every name's origin is visible in the file, which lowers the hallucinated-API rate |
| One spelling per construct, a closed keyword set | Fewer valid outputs means more probability mass on the right one |
| `.Variant`, `Some`/`None`/`Ok`/`Err`, `match`/`case`, `fn`, `let`, `..`/`..=`, `xs[i]`, `{k: v}`, `if c: a else: b` | Borrowed directly from Python, Rust, Swift and TypeScript |
| Named record construction; required labels for adjacent same-type parameters | Field-order and argument-swap mistakes become impossible |
| Spec budget | `llms.txt` ≤ 15k tokens at MVP and ≤ 12k at 1.0 (05 D3). `llms-full.txt` ≤ 40k, generated from this spec and the stdlib docs |

### 21.3 Known LLM mistakes: errors with machine-applicable fixes

Each row is an **error**, never a silent alias, so canonical code stays canonical. Each has a fix that `kalam fix` and agents may apply without review. They are detected only in the obvious syntactic position, so `def` as a variable name stays legal.

| Code | Written | Fix |
|---|---|---|
| K0901 | `a === b`, `a !== b` | `==`, `!=` |
| K0902 | `def f(x):`, `function f(x)`, `func f` | `fn f(x: _):` (type holes left for the model) |
| K0903 | `elif c:`, `else if (c) {` | `else if c:` |
| K0904 | trailing `;` or `؛` | remove it, splitting statements onto separate lines |
| K0905 | `{ … }` used as a block | colon plus indentation |
| K0906 | `null`, `nil`, `undefined` | `None`, plus a hint about `Option[T]` |
| K0907 | `async fn`, `await x` | delete it: suspension is inferred (§15) |
| K0908 | `try:`, `catch`, `throw`, `raise` | `?` on a `Result`, or `return Err(…)` |
| K0909 | `x++`, `x--` | `x += 1` |
| K0910 | `c ? a : b` | `if c: a else: b` (valid in every expression position, §8.3) |
| K0911 | `&&`, `\|\|`, `!x` | `and`, `or`, `not x` |
| K0912 | `let mut x`, `const x` | `var x`, `let x` |
| K0913 | `f"…{x}"`, `` `…${x}` `` | `"…{x}"` |
| K0914 | `List<T>`, `Vec<T>`, `Array<T>`, `T[]` | `List[T]` |
| K0915 | `x.unwrap()` | `x.expect("…")` (state the invariant) |
| K0916 | `print(a, b)` with several arguments | `print("{a} {b}")` |
| K0917 | `Circle(r)` as a pattern or construction for a variant | `.Circle(r)` |
| K0918 | `len(xs)`, `str(x)`, `int(s)` | `xs.len()`, `"{x}"`, `s.to_int()?` |
| K0919 | `impl Show for Money:` | `impl Money as Show:` (DR-10) |
| K0920 | `class X:`, `struct X {`, `record X:`, `interface X` | `type X:` (or `trait X:` for an interface) |
| K0921 | a multi-line lambda (`fn(x):` + block, or `lambda x:`) | a local `fn` declared just before the call (DR-15) |
| K0922 | `f[T](x)` (explicit type arguments) | an annotated `let` (`let r: T = f(x)`) (DR-25) |
| K0923 | `a ** b` | `a.pow(b)` |
| K0924 | `'text'` (single quotes) | `"text"` |
| K0925 | `switch x {` / `case X ->` / `x => …` arms | `match x:` with `case pattern:` arms |
| K0926 | `for (let i = 0; i < n; i++)`, `for i in range(n)` | `for i in 0..n:` |
| K1601 | string-built SQL, HTML or command | tagged literal with holes |

**Hallucinated-API guard:** an unknown method is K0420. It lists the three closest stdlib names by edit distance **and** by a synonym table (`push`→`append`, `length`→`len`, `includes`→`contains`, `size`→`len`, `toUpperCase`→`to_upper`). Collections are immutable, so when the unknown call is a mutating method used as a statement (`items.push(x)`), the machine fix rewrites it to a rebinding (`items = items.append(x)`, which needs `items` to be a `var`). A bare `items.append(x)` statement is K0730 with the same fix.

### 21.4 Diagnostics: format, tone and languages

Every diagnostic follows one template (01 §3.4, 05 D5):

```text
-- NUMBER TYPES DON'T MIX ------------------------------------ cart.kl  K0601

I can't add an Int to a Float:

12│     let total = count + price
                    ^^^^^^^^^^^^^
`count` is an Int and `price` is a Float. Kalam never converts numbers
by itself, so you choose:

    let total = count.to_float() + price      # keep the fraction
    let total = count + price.round()         # drop the fraction

Learn more: kalam explain K0601
```

Rules:
1. **Message language** follows `KALAM_LANG` (`en`, `ar`, `franco`). **Code in the message stays in the file's skin**, including the suggested fix. For `ar` files on non-bidi terminals, an English-skin copy of the excerpt is printed underneath (§6.1).
2. **One root cause first.** Follow-on errors are hidden, with a count ("2 more errors may be caused by this one").
3. **Positive tone:** never "illegal", "invalid", "fatal" or "bad". First person ("I expected…").
4. **A concrete fix** in most errors. Machine-applicable fixes appear as LSP code actions and in JSON.
5. **Stable codes**, with an `explain` page in all three message languages (05 P6). Every diagnostic is golden-tested in all three.
6. **Cross-skin help:** an English keyword in an Arabic file gives K0201 (*"`if` is English. In Arabic files, write `إذا`."*). An attached Arabic و gets the hint from §5.4. A digit-leading Franco word gets K0127 with the `did_you_mean` spelling.

**JSON format** (`kalam check --json`, the LSP and `kalam mcp`):

```json
{
  "code": "K1301",
  "severity": "error",
  "message": {"en": "…", "ar": "…", "franco": "…"},
  "file": "src/notes.kl", "skin": "en",
  "span": {"start": {"line": 12, "col": 9}, "end": {"line": 12, "col": 27}},
  "labels": [{"span": {"start": {"line": 12, "col": 9}, "end": {"line": 12, "col": 20}}, "text": "this is `DbError`"}],
  "fixes": [{"title": "add .map_err", "applicability": "machine",
             "edits": [{"span": {"start": {"line": 12, "col": 27}, "end": {"line": 12, "col": 27}},
                        "insert": ".map_err(fn(e) => …)"}]}],
  "explain": "https://kalam.dev/errors/K1301"
}
```

`"applicability": "machine"` means the fix is safe for `kalam fix` and for agents to apply without review. `"suggestion"` means a human or the model must choose.

### 21.5 The toolchain (one binary, D10)

| Command | Purpose |
|---|---|
| `kalam new` | a new project, including `AGENTS.md`, which tells agents to write canonical English skin, run `kalam check --json` after every edit, and apply machine fixes |
| `kalam run`, `kalam build` | build and run; JS output with source maps and `.d.ts` |
| `kalam check [--json] [--bidi]` | type-check only; `--bidi` runs the display-order check |
| `kalam fmt [--check]` | canonical formatting |
| `kalam convert --to en\|ar\|franco` | deterministic, lossless skin conversion. It never uses an LLM (02 §7.2, the APL anti-pattern) |
| `kalam view --skin S [--glossary]` | read-only rendering in any skin, with the optional display glossary |
| `kalam rename --glossary names.toml` | on-disk identifier renaming (new KCF; inserts `@name` for wire stability) |
| `kalam diff --skin S` | review any change in any skin, with KCF hashes |
| `kalam test`, `kalam explain Kxxxx`, `kalam fix` | tests, long explanations, apply machine fixes |
| `kalam bind npm:<pkg>` | generate `extern` modules from `.d.ts` |
| `kalam audit`, `kalam caps` | `unsafe.*`, `extern`, `.expose()`, `@positional`, discarded `Result`s, legacy crypto, capability sets, OSV advisories |
| `kalam db migrate`, `kalam db check` | migrations; SQL literal checking against the schema |
| `kalam lsp` | the language server (semantic tokens, UTS #55 display, suspension inlays, glossary display) |
| `kalam mcp` | an MCP server with the tools `check`, `fix`, `fmt`, `convert_skin`, `run_tests`, `doc_search` and `explain` (06 §7.3) |

The playground is the same compiler built to WASM, with a skin toggle (03 §5).

---

## 22. Lowering to JavaScript (reference semantics)

Output is ES2022 ESM for Node ≥ 22, Bun, Deno and modern browsers, with Source Map v3 and `.d.ts` for `@js.export`.

**Guarantees of generated code (04 P14):**
- No `eval`, `new Function`, `with`, string `setTimeout` or computed `import()` anywhere in generated code or in the runtime. A strict CSP without `'unsafe-eval'` works.
- Never emitted: `==`/`!=`, `for…in`, `in` on user-keyed objects, `obj[userKey]` on plain objects, or `Object.assign` from user data.
- Browser bundles get SRI hashes on emitted `<script>` tags. Nothing is fetched from a CDN at runtime by default.

| Kalam | JS lowering |
|---|---|
| module | one ESM file. Module constants are `const`s evaluated in import order |
| record | an instance of a generated class with fixed fields. `.with` copies |
| enum | one generated class per variant with a numeric `$tag`. Payload-free variants are singletons |
| `Option[T]` | `undefined` for `None` and the value itself for `Some(v)`, **unless** `v` could itself be `undefined` (a nested `Option`, `()` or a type variable), in which case a runtime check boxes it as `$Some(v)` |
| `Result[T, E]` | `{ok: true, value}` / `{ok: false, error}` (shared shapes for V8) |
| `Int` | **Canonical dual representation (D18):** an `Int` is a JS `number` exactly when its value is in the safe range ±(2⁵³−1), and a `bigint` otherwise. Every runtime operation returns the canonical form, so `===`, `Map`/`Set` keys and hashing need no special case. **Fast path:** `+ - *` emit `$add(a, b)` etc. When both operands are `number`s, the helper computes the double result `r` and returns it if `Number.isSafeInteger(r)`. This test is exact for `+ - *`: when the true result is safe the double is exact, and when it is not, rounding cannot bring it back into the safe range, because 2⁵³ is representable. **Slow path:** otherwise the helper computes in `bigint`, panics if `BigInt.asIntN(64, x) !== x`, and normalizes a safe result back to `number`. Range analysis may inline the fast path or remove checks it proves redundant, and nothing else. `//` and `%` use `$floordiv`/`$mod` (floored, panic on 0). Comparisons use the JS operators, which are exact across `number` and `bigint`. `Show` and JSON encoding print the exact decimal digits. **Performance target:** ≤ 10% overhead on the 05 D6 corpus versus plain `number` arithmetic |
| `Float` | `number` |
| `Decimal`, `BigInt` | a small runtime decimal128 class (no `number` in its arithmetic), and native `bigint` |
| `String` | a JS string, always well-formed (guaranteed by the lexer, decoders and boundary checks). APIs work in code points and graphemes (`Intl.Segmenter`) and never expose UTF-16 units |
| `List[T]` | a frozen `Array`, or a persistent vector for large lists. In-place updates happen when uniqueness is proven |
| `Map`, `Set` | JS `Map`/`Set` for primitive keys, and a runtime `HashMap` with keyed SipHash for structural keys. **Never plain objects** (04 §4.3) |
| `==` | `===` for primitives, generated `$eq` for structural types |
| `match` | `switch` on `$tag` plus nested tests. A `default` that panics is emitted only as defence in depth |
| `panic` | `throw new $Panic(msg)`. Only runtime boundaries catch it (§14.4). A JS `RangeError` from stack exhaustion becomes a panic |
| suspending function | `async function`, with `await` at each suspending call site. Effect-polymorphic functions get two variants (`f` / `f$s`) |
| cancellation | an `AbortSignal` threaded through the runtime task context, checked by suspending stdlib calls on entry and resume |
| tagged literals | precompiled templates: `$Sql([…parts], [args])`, an HTML template with per-hole escapers, an argv array |
| `Html` on the client | inserted only through the `kalam` Trusted Types policy. The default CSP includes `require-trusted-types-for 'script'` |
| capabilities | runtime objects created only by the entry module. Kalam code cannot forge one, because the classes are not exported to user scope |
| `extern` | a direct import plus boundary checks (§19.2) |

---

## 23. Deliberately excluded (v0.1)

| Excluded | Use instead | Why |
|---|---|---|
| Classes, inheritance, `this` | records, enums, traits, functions | fewer concepts; inheritance is a frequent LLM and human design bug |
| `null`/`undefined`/`nil` | `Option[T]` | 04 L5 |
| Exceptions, `try`/`catch`/`throw`/`finally` | `Result` + `?`; panic for bugs | D16, 04 L6 |
| `async`/`await`, promises in user code | inferred suspension (§15) | D12 |
| `eval`, dynamic `import`, reflection, call-by-name | static `use`; `enum` + `match` | 04 L9 |
| Macros, compile-time metaprogramming beyond the closed `@derive` set | stdlib, code generators (`kalam bind`) | greppable code, readable errors |
| User-defined operators, operator overloading | named methods | one meaning per symbol |
| Implicit conversions, truthiness, loose equality | named conversions | 04 L3, L4 |
| Function overloading, variadics | named parameters with defaults; `List` parameters | one way to call |
| Explicit type arguments at call sites | annotated `let` | DR-25 |
| Multi-line lambdas | local `fn` | DR-15 |
| Module-level mutable state, mutable shared references | `Cell` passed down; capabilities | §15.6 race freedom |
| Shadowing | a new name | DR-28 |
| `if let`, `let … else`, `is` tests | `match`, `?`, `Option` methods | one way |
| Bitwise operators, `**`, `++`, `?:`, `===`, `\|>` | methods; canonical spellings (§21.3) | one way |
| Semicolons, braces for blocks, block comments | colon + indentation; `#` per line | D15 |
| Glob imports, relative imports, import cycles | absolute `use` | §13.1 |
| Shell-string process APIs; raw `Command` | `cmd"…"` | 04 S2 |
| Trait objects, higher-kinded types, variance, specialization | enums, records of functions | v0.1 complexity budget |
| Shared-memory threads | isolates with message passing (later) | 04 L11 |
| `unsafe` blocks of raw memory | `unsafe.*` named operations only | no native backend yet (D4) |
| Install scripts in any dependency | — | 04 P1 |
| Per-file skin mixing, per-skin syntax or word order | `kalam convert` | D6, P7 |
| Arabic lexical keyword synonyms; digit-leading Franco spellings | one canonical word per concept | 05 D4, P7 |
| Client islands with keyword reactivity | server-driven live components | D11; planned as a web-kit feature (X14) |

---

## 24. Security charter traceability (report 04)

| Rule | Where satisfied |
|---|---|
| L1 memory safety | §9 (GC semantics, no pointers), §10.8 (bounds-checked indexing) |
| L2 no UB on any backend | §9.2 (defined overflow, division by zero), §10.6 (left-to-right evaluation), §22 |
| L3 strong static typing, no implicit conversions | §9.1, §9.9, §10.6 |
| L4 one strict equality, no truthiness | §9.8, §9.1 |
| L5 no null | §9.3 `Option`, §23 |
| L6 errors as values; unused `Result` is an error | §14.2 (K1303) |
| L7 overflow traps | §9.2 |
| L8 exhaustive match | §11 (K1001, K1002, W0403, `@exhaustive_strict`) |
| L9 no eval / dynamic import / call-by-name | §13.1, §16.1 rule 7, §23 |
| L10 immutable by default | §9.1, §10.2, §10.4 |
| L11 data-race freedom | §12.3, §15.6 |
| L12 capability-passing I/O | §16 (deviation X9 for the console) |
| L13 escape hatches ASCII and countable | §7.2, §17.1, §18.1 (`.expose()`), §19.1, `kalam audit` |
| L14 Decimal in core | §9.2 |
| U1 bidi controls rejected | §5.1 |
| U2 NFC source | §4.1, §5.1 (SEC0101) |
| U3 UAX #31, single script, no tatweel/harakat/ZWJ | §5.2, §5.3 |
| U4 confusable skeleton collisions | §5.3 (SEC0104), §4.5 (SEC0108) |
| U5 one digit system per literal; keywords never numbers | §5.6, §6.2, §7.4 |
| U6 one skin per file | §4.1, §5.4 (K0201) |
| U7 canonical serialization for hashing and signing | §4.4 (KCF) |
| U8 UTS #55 display in first-party tools | §6.1 |
| U9 invisible-character lint in strings | §5.1, §5.7 (W0133) |
| S1 typed sinks | §17 |
| S2 no shell strings | §17.5 |
| S3 contextual auto-escaping | §17.4 |
| S4 schema-first decoding with limits | §18.3, §19.2 |
| S5 secrets | §18.1 |
| S6 misuse-resistant crypto | §18.4 |
| S7 CSPRNG by default | §18.2 |
| S8 HTTP defaults | §20.2 |
| S9 deny-by-default routes | §20.2 (`access:` required) |
| S10 request binding into declared types | §18.3 (W0710), §20.3 |
| S11 no stack traces in production | §14.4 |
| S12 SSRF blocking | §16.1 (`Net`) |
| S13 path safety | §16.1 (`Fs`), §17.6 |
| S14 DoS-resistant hashing | §9.3, §22 |
| S16 compile-time SQL checks (P2) | §17.3 (`kalam db check`) |
| S17 open-redirect-safe redirects | §17.6 |
| P1 no install-time code | §13.5 |
| P2 lockfile with hashes | §13.5 (KCF hashes) |
| P6 capability manifest and diffs | §13.5, §16.4 |
| P7 namespaces and cross-script typosquat checks | §13.5 |
| P9 release-age cooldown | §13.5 |
| P10 audit | §21.5 |
| P14 safe generated JS | §19.2, §22 |
| S15 sandboxed scripting VM (SHOULD, P2) | not in v0.1 (no plugin or mod system yet) |
| P3 transparency log, P4 OIDC trusted publishing + Sigstore/SLSA, P5 reproducible registry artifacts, P8 namespace-pinned private registries, P11 maintainer 2FA and ownership-transfer delay | registry and toolchain obligations, not language semantics; delivered in ROADMAP M7 |
| P12 reproducible builds and pinned C toolchain, P13 native hardening flags | ROADMAP M7 (reproducible builds) and M10 (C backend) |
| G1 security policy and CVE process, G2 continuous fuzzing, G3 review for new escape hatches, G4 no "insecure now, fix later" | project process: ROADMAP §0.1 and M0 (`SECURITY.md`), scorecard gate G-FUZZ |

---

## 25. Conformance tests this spec implies

These belong in CI from the first compiler commit (05 D1, D2 and the parity gate). Each row is a directory under `conformance/`.

| Area | Tests |
|---|---|
| Trojan Source | Each code-point class in §5.1, placed in code, comments, strings and identifiers, is rejected with the right code. The escaped form compiles. Legitimate Arabic text in strings and comments compiles. NBSP and U+2028 are rejected. ZWJ between two emoji is accepted in a string. (05 NEG task 50) |
| Identifiers | Mixed-script rejection; skeleton collisions (`على`/`علي`, Latin `o` vs. Arabic-Indic `٥`); tatweel, harakat and presentation forms rejected with fix-its; Persian/Urdu letters accepted and folded (`کتاب` and `كتاب` declared in one scope are SEC0104; `لکل` in an `ar` file is the keyword `لكل`); digit-leading rejected in every skin; an unfolded use (`اكتب` vs. declared `أكتب`) is K0401 with a machine fix. |
| Numerals | `1٢3`, Persian digits, `٫` with ASCII digits, `،` as a decimal, U+066C, leading zeros, `0x1Fg`, `1e5x`, `7f` and `3ala` are rejected with the right codes. `0x1e+5` lexes as three tokens. `t.0.1` lexes as two member accesses. `1e5` is a `Float`. Arabic-Indic literals are accepted only in `.ar.kl`. |
| Keyword table | The seven rules of §7.4, run over `keywords.json`. Every Franco variant formats to canonical. `fmt --check` rejects variants. Every `did_you_mean` entry appears in K0127's message. |
| Layout | Leading-dot continuation: a value block whose final line is `.None` at the same indentation parses as a statement; a block whose first line starts with `.X` is not joined to its header; a deeper `.map(…)` line after `let x = xs` joins. |
| Inline if | `let x = if c: a else: b`, `return if c: a else: b`, `f(if c: a else: b)` parse as expressions. `if x < 0: return Err(.Neg)` parses as a statement. The K0910 fix output parses. |
| Skins and parity | 05 P1–P5 round-trips for every corpus program, including backtick escapes, Arabic-Indic digit settings and glossary display (which must not change KCF). English keywords in `.ar.kl` give K0201. |
| Types | No implicit conversions (a matrix of every primitive pair under `==`, `+` and as a call argument). `Float` is not `Eq`/`Ord`/`Hash`. `Secret` is not `Encode`/`Eq`. Capabilities have no `Decode`. Method resolution is unchanged by adding imports. |
| Integers | Overflow traps exactly outside [−2⁶³, 2⁶³−1] on JS and in the reference interpreter, with differential execution, including across the JS `number`/`bigint` boundary (2⁵³−1 + 1, 2⁶³−1 + 1, −2⁶³ − 1, −(−2⁶³), products that cross 2⁵³). Every JS result is in canonical form (`number` iff safe). A Postgres `bigint` of 2⁶³−1 decodes into `Int` exactly. A JSON integer 9007199254740993 decodes exactly; 2⁶³ decoding into `Int` is `DecodeError.OutOfRange`. `9223372036854775808` as an `Int` literal is K0126 and `-9223372036854775808` is legal. |
| Patterns | Exhaustiveness, unreachable arms (K1002), bare identifier as binding, `.Variant` resolution, W0403 over a foreign enum. |
| Must-use | Discarded `Result` → K1303. Discarded value → K0730. `let _ =` is accepted and counted. |
| Concurrency | Suspension inference over the call graph, higher-order specialization, `@sync` fields, K1401 call chains, cancellation only at suspension points, and transaction rollback on panic and on cancellation. |
| Sinks | 05 NEG tasks 48–49 in all three skins, plus each K16xx rule with a positive twin. The exploit-payload harness runs against the good twins. `cmd"git {x}"` with `x = "--upload-pack=…"` returns `OptionLikeArgument`. |
| JS boundary | Each row of §19.2 with hostile JS: getters, proxies, `__proto__` keys, lone surrogates, `NaN` as an `Int`, `2**53` as an `Int`, `null` in a non-optional field. Each yields a validation failure, never a Kalam-visible wrong value. |
| Generated JS | Static scan of all emitted JS: no `eval`, `Function(`, `==`, `for…in`. Test pages run under a strict CSP with Trusted Types enforced. |
| Live UI | A forged, replayed or cross-session `k-click` token is rejected with 400. A message hole of the wrong type is K1616/K1618. |

---

## 26. Scorecard self-report and concept budget

This section is a **living conformance artifact**. CI regenerates the numbers from `keywords.json`, the EBNF and the corpus, and fails if a number crosses its threshold (05 D4).

### 26.1 Simplicity metrics (05 D4)

| Metric | v0.1 value | MVP limit | 1.0 limit | Note |
|---|---|---|---|---|
| Keywords per skin | **31** (29 translated + `unsafe`, `extern`) | ≤ 40 | ≤ 35 | passes |
| Reserved spellings per skin (keywords + unshadowable core names) | **50** (31 + 19) | (≤ 40 if core names count) | (≤ 35) | **Over, if core names count as keywords.** Reported honestly. The metric in 05 D4 counts keywords only. Reducing the 19 core names (for example leaving `assert`, `Set` and `Decimal` untranslated) is open question Q3 |
| Synonyms (more than one word per concept) | **0** in `en` and `ar`. Franco: closed spelling-variant lists, rewritten by `fmt` | 0 | 0 | variants are orthographic, like Arabic hamza folding (DR-20) |
| Grammar productions (§8.2) | **75** | ≤ 120 | ≤ 100 | passes |
| Punctuation aliases | 2 (`،`, `؟`), plus `٫` with Arabic-Indic digits | — | — | |
| Concepts before "hello web app" | **10** (§26.2; 13 for the DB-backed CRUD of §28.5) | ≤ 12 | ≤ 8 | passes MVP for hello-web |

### 26.2 Concept budget for "hello web app"

05 D4 counts the distinct spec concepts needed before a first web app. The reference program is the smallest JSON/text endpoint (corpus tasks 31–32), and CI keeps it in `conformance/budget/hello_web.kl`:

```kalam
use std/http.{self, HttpError, Request, Response}

fn hello(req: Request[()]) -> Result[Response, HttpError]:
    let name = req.query_param("name").or("world")
    return Ok(Response.text("Hello, {name}!"))

fn main(world: World) -> Result[(), AnyError]:
    let routes = http.router().get("/hello", hello, access: .Public)
    http.serve(world.net, port: 8080, router: routes, app: ())?
    return Ok(())
```

Concepts used:

1. `use` of a module
2. `fn` with typed parameters and a return type
3. `let` binding
4. `return`
5. method-call syntax (`a.f(b)`)
6. `Option` with a default (`.or(…)`)
7. string interpolation `{}`
8. `Result` with `Ok` and `?`
9. `main(world: World)` and passing a capability (`world.net`)
10. a route with a required access rule, written as a leading-dot member (`access: .Public`)

That is **10 concepts**, within the MVP limit of 12, with no `async`, no classes, no decorators and no build configuration. **Honest second data point:** the DB-backed CRUD handler of §28.5 (corpus task 33) uses 13 concepts. It adds a record `type` with `@derive`, `match`/`case` on `Option`, the `sql"…"` literal and `if`, and it drops interpolation and `.or`. To reach the 1.0 target of 8 for hello-web, teach `World` together with `main`, and `Result`/`?` together with `return`.

### 26.3 Parity and security gates this spec is designed to pass

05 P1–P10 (§4.3, §7.4, §25). 05 D2: Trojan Source 100% (§5.1), 0 Vulnerable for SQLi/XSS (§17), and Arabic/Franco lexing hazards 100% (§5.3, §5.6, §6).

---

## 27. Diagnostic index

`K` = error. `W` = warning (can be allowed per item with `@allow(Wxxxx)`, which `kalam audit` counts). `H` = hint. `L` = lint. `SEC` = source-text security error (never suppressible). Ranges: K00xx files; K01xx lexing; K02xx skins; K03xx modules and visibility; K04xx names and resolution; K06xx types and operators; K07xx statements; K08xx inference; K09xx known LLM mistakes; K10xx patterns; K11xx functions; K12xx modules and globals; K13xx errors; K14xx concurrency; K15xx capabilities; K16xx sinks and web; K17xx interop.

| Code | Meaning |
|---|---|
| SEC0100 | invalid UTF-8, or BOM |
| SEC0101 | file not in NFC |
| SEC0102 | control character |
| SEC0103 | non-ASCII whitespace in code |
| SEC0104 | two distinct identifiers with the same skeleton in one scope |
| SEC0105 | mixed-script identifier |
| SEC0106 | tatweel, harakat, Quranic mark, ZWJ/ZWNJ or presentation form in an identifier or keyword |
| SEC0107 | *retired* (was: Persian/Urdu letter in an identifier; these letters are folded since D19). The code is never reused |
| SEC0108 | glossary display name collides by skeleton |
| SEC0110 | bidi control or mark |
| SEC0111 | invisible format character or misplaced variation selector |
| SEC0112 | tag character, private use, unassigned or noncharacter |
| SEC0113 | U+2028/U+2029 or lone CR |
| SEC0120 | mixed digit systems in one literal |
| SEC0121 | Persian digits |
| SEC0122 | Arabic-Indic digits in an `en` or `franco` file |
| SEC0123 | U+066C Arabic thousands separator |
| SEC0124 | Arabic-Indic digit in an identifier |
| K0103 | tab outside a string |
| K0105 | unexpected character |
| K0120 | bad indentation (unit, jump or unexpected indent) |
| K0122 | dedent to an unknown level |
| K0126 | literal out of range for its type (`Int`: [−2⁶³, 2⁶³−1], D18; reported by the type checker) |
| K0125 | leading zero |
| K0127 | number glued to letters / name starts with a digit |
| K0128 | hex, binary, octal or exponent with Arabic-Indic digits |
| K0129 | unterminated string literal (adopted from ARCHITECTURE.md §12.2) |
| K0130 | unknown escape |
| K0131 | string literal inside an interpolation hole |
| K0132 | lone surrogate escape |
| W0114 | non-ASCII space inside a string |
| W0133 | unusual code point in a string |
| L0114 | mixed `و`/`أو` chain without parentheses |
| K0201 | keyword, core name or punctuation from another skin |
| H0202 | identifier equals another skin's reserved word |
| K0301 | two skin files for one module |
| K0302 | `opaque` on something other than a type or enum |
| K0303 | exported signature exposes a private type |
| K0401 | name not defined (with skeleton-based fix) |
| K0405 | shadowing |
| K0413 | core name declared or shadowed |
| K0416 | record construction without named arguments |
| K0417 | record field named `with` |
| K0420 | unknown method (hallucinated-API guard) |
| W0403 | `_` arm over an enum from another package |
| K0601 | mixed numeric types |
| K0602 | `==` between different types, or a non-`Eq` type |
| K0612 | `Encode` derived on a type with a `Secret` field |
| K0650 | string indexing |
| K0710 | assignment target is not a `var` place |
| K0720 | chained comparison |
| K0730 | unused value |
| K0731 | value-position `if` without `else` |
| K0801 | lambda parameter types unknown |
| K0802 | empty collection literal without an expected type |
| K0803 | implicit member without an expected type |
| K0901–K0926 | known LLM mistakes (§21.3) |
| K1001 | non-exhaustive `match` |
| K1002 | unreachable arm |
| K1003 | refutable pattern in `let` |
| K1004 | or-pattern binds different names |
| K1005 | `Float` literal pattern |
| K1101 | missing `return` |
| K1103 | ambiguous method |
| K1104 | missing labels for adjacent same-type parameters |
| K1110 | closure captures a `var` |
| K1201 | import cycle |
| K1202 | module-level `var` |
| K1203 | glob import |
| K1301 | `?` cannot convert the error type |
| K1302 | `?` on `Option` in a `Result` function |
| K1303 | unused `Result` |
| K1401 | suspending function passed where `@sync` is required |
| K1402 | `@sync` function suspends |
| W0421 | `Cell` value read before a suspension point and used after |
| K1501 | constructing or decoding `World` |
| K1502 | `Screen` on a non-browser target |
| K1503 | secret-looking key passed to `env.get` |
| K1504 | a library package calls `print` or `log.*` without `console = true` in its manifest (X9, §16.4; adopted from ARCHITECTURE.md §9.2) |
| K1601 | `String` passed to a sink |
| K1602 | SQL hole inside quotes |
| K1603 | SQL hole in an identifier position |
| K1604 | multiple SQL statements |
| K1605 | SQL hole inside a comment |
| W1606 | raw `String` in a `LIKE` hole |
| K1610 | malformed HTML literal |
| K1611 | unquoted attribute hole |
| K1612 | hole in `<script>`, `<style>`, `style` or `on*` |
| K1613 | URL attribute hole is not `Url` |
| K1614 | hole in a tag or attribute name |
| K1615 | hole in an HTML comment |
| K1616 | `k-<event>` hole is not a `Message` |
| K1617 | unknown `k-*` attribute |
| K1618 | live view message type differs from `update`'s message type |
| K1620 | shell metacharacter in `cmd"…"` |
| K1621 | program in `cmd"…"` is not literal |
| W1622 | option-like hole without a preceding `--` |
| K1631 | URL scheme or host from a hole |
| K1641 | `path"…"` hole is not a `PathSegment` |
| K1650 | two tags with one name in scope |
| K1701 | extern module not declared in `[npm]` |
| K1702 | capability crossing the JS boundary |
| W0710 | request data decoded into a DB row type |
| W0720 | `std/crypto/legacy` imported |

---

## 28. Example programs (8 programs × 3 skins)

**How to read these:**
- The **English** block is canonical text (§21.1).
- **[converter]** marks blocks that are exactly what `kalam convert` prints. Only keywords, core names, punctuation and digits change. Identifiers stay as written, and the KCF hash is identical to the English original.
- **[view: glossary]** marks blocks shown through a project display glossary (`names.toml`, §4.5), so identifiers are displayed in Arabic or Franco. This is a *view* of the same file: the KCF hash, JSON keys and SQL columns do not change.
- **String literal text is never translated**, which is why output strings are identical in all three skins. User-facing text belongs in `std/i18n` tables.
- Arabic blocks use the default `digits = "ascii"`, and the Arabic printer always uses `،` and `؟`.
- Markdown viewers do not apply Kalam's per-token bidi isolation (§6.1), so Arabic lines may look reordered here. The first-party tools display them correctly.

### 28.1 Hello world

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
# hello.franco.kl  [converter]
dala ra2isi():
    ektb("Hello, world!")
```

### 28.2 FizzBuzz

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
# fizzbuzz.ar.kl  [converter: identifiers are kept as written]
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
# fizzbuzz.franco.kl  [converter]
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
With `[skin.ar] digits = "arabic-indic"`, the second line of the Arabic file prints as `لكل n في ١..=١٠٠:` and `n % 15` prints as `n % ١٥`. Both forms have the same KCF.

### 28.3 Records, enums and `match`

```kalam
# shapes.kl
enum Shape:
    Circle(radius: Float)
    Rect(width: Float, height: Float)

type Item:
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
# shapes.ar.kl  [view: glossary]
تعداد شكل:
    دائرة(نصف_القطر: كسري)
    مستطيل(العرض: كسري، الارتفاع: كسري)

هيكل عنصر:
    الاسم: نص
    الشكل: شكل

دالة المساحة(الشكل: شكل) -> كسري:
    أرجع حسب الشكل:
        عندما .دائرة(ن): 3.14159 * ن * ن
        عندما .مستطيل(ع، ط): ع * ط

دالة رئيسي():
    ليكن العناصر = [
        عنصر(الاسم: "coin"، الشكل: .دائرة(نصف_القطر: 1.5))،
        عنصر(الاسم: "card"، الشكل: .مستطيل(العرض: 8.5، الارتفاع: 5.4))،
    ]
    لكل العنصر في العناصر:
        اطبع("{العنصر.الاسم}: {المساحة(العنصر.الشكل)}")
```
```kalam
# shapes.franco.kl  [view: glossary]
ta3dad shakl:
    dayra(nos_qotr: kasri)
    mostateel(el_ard: kasri, el_ertefa3: kasri)

haykal onsor:
    el_esm: nass
    el_shakl: shakl

dala el_mesa7a(el_shakl: shakl) -> kasri:
    rag3 hasab el_shakl:
        lama .dayra(n): 3.14159 * n * n
        lama .mostateel(a, t): a * t

dala ra2isi():
    khali el_anaser = [
        onsor(el_esm: "coin", el_shakl: .dayra(nos_qotr: 1.5)),
        onsor(el_esm: "card", el_shakl: .mostateel(el_ard: 8.5, el_ertefa3: 5.4)),
    ]
    lekol el_onsor fi el_anaser:
        ektb("{el_onsor.el_esm}: {el_mesa7a(el_onsor.el_shakl)}")
```
*Notes:*
- Kalam has one namespace per scope (§13.3). The type `شكل` and the value `الشكل` are different names, and the definite article is the idiomatic way to name a value in Arabic (`el_` in Franco).
- The variants `.دائرة` and `.dayra` need no capital letters, because of the leading dot.
- In glossary view, runtime names stay canonical. `print` output and any JSON would still use `label` and `shape`.

### 28.4 `Result` and `?`

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
# config.ar.kl  [view: glossary]
تعداد مشكلة_الإعداد:
    مفقود(المفتاح: نص)
    غير_رقمي(المفتاح: نص، القيمة: نص)

دالة اقرأ_المنفذ(البيئة: Env) -> نتيجة[عدد، مشكلة_الإعداد]:
    ليكن الخام = البيئة.get("PORT").ok_or(مشكلة_الإعداد.مفقود(المفتاح: "PORT"))؟
    أرجع حسب الخام.to_int():
        عندما نجاح(المنفذ): نجاح(المنفذ)
        عندما فشل(_): فشل(.غير_رقمي(المفتاح: "PORT"، القيمة: الخام))

دالة رئيسي(العالم: عالم):
    حسب اقرأ_المنفذ(العالم.env):
        عندما نجاح(المنفذ):
            اطبع("Listening on port {المنفذ}")
        عندما فشل(.مفقود(المفتاح)):
            اطبع("Please set {المفتاح}")
        عندما فشل(.غير_رقمي(المفتاح، القيمة)):
            اطبع("{المفتاح}={القيمة} is not a number")
```
```kalam
# config.franco.kl  [view: glossary]
ta3dad moshkelet_e3dad:
    na2es(el_moftah: nass)
    ghair_raqami(el_moftah: nass, el_qeema: nass)

dala e2ra_el_port(el_bee2a: Env) -> natiga[adad, moshkelet_e3dad]:
    khali el_khaam = el_bee2a.get("PORT").ok_or(moshkelet_e3dad.na2es(el_moftah: "PORT"))?
    rag3 hasab el_khaam.to_int():
        lama naga7(el_port): naga7(el_port)
        lama fashal(_): fashal(.ghair_raqami(el_moftah: "PORT", el_qeema: el_khaam))

dala ra2isi(el_donya: donya):
    hasab e2ra_el_port(el_donya.env):
        lama naga7(el_port):
            ektb("Listening on port {el_port}")
        lama fashal(.na2es(el_moftah)):
            ektb("Please set {el_moftah}")
        lama fashal(.ghair_raqami(el_moftah, el_qeema)):
            ektb("{el_moftah}={el_qeema} is not a number")
```
*Notes:*
- `ok_or` is generic in its error type, so the argument is qualified (`ConfigError.Missing`), because nothing gives an expected type for `.Missing`.
- In `Err(.NotANumber …)`, the return type supplies the expected type, so the leading-dot form works.
- `Env` and `.get` are stdlib names and stay ASCII (§7.2).

### 28.5 HTTP handler (JSON API, deny-by-default routes)

```kalam
# notes.kl
use std/http.{self, HttpError, Request, Response, Router}
use std/sql.{self, Db}

type App:
    db: Db

@derive(Encode, Row)
type Note:
    id: Int
    title: String
    body: String

@derive(Decode)
type NewNote:
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
        .get("/notes/\{id\}", show_note, access: .Public)
        .post("/notes", create_note, access: .LoggedIn)

fn main(world: World) -> Result[(), AnyError]:
    let db = sql.sqlite(world.fs, "notes.db")?
    http.serve(world.net, port: 8080, router: routes(), app: App(db: db))?
    return Ok(())
```
```kalam
# notes.ar.kl  [view: glossary]
استخدم std/http.{ذات، HttpError، Request، Response، Router}
استخدم std/sql.{ذات، Db}

هيكل تطبيق:
    القاعدة: Db

@derive(Encode، Row)
هيكل ملاحظة:
    المعرف: عدد
    العنوان: نص
    المحتوى: نص

@derive(Decode)
هيكل ملاحظة_جديدة:
    العنوان: نص
    المحتوى: نص

دالة اعرض_ملاحظة(الطلب: Request[تطبيق]) -> نتيجة[Response، HttpError]:
    ليكن المعرف = الطلب.path_int("id")؟
    ليكن الاستعلام = sql"SELECT id, title, body FROM notes WHERE id = {المعرف}"
    ليكن المطلوبة: اختياري[ملاحظة] = الطلب.app.القاعدة.one(الاستعلام)؟
    أرجع حسب المطلوبة:
        عندما موجود(الملاحظة): نجاح(Response.json(الملاحظة))
        عندما لا_شيء: فشل(.NotFound)

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
        .get("/notes/\{id\}"، اعرض_ملاحظة، access: .Public)
        .post("/notes"، أنشئ_ملاحظة، access: .LoggedIn)

دالة رئيسي(العالم: عالم) -> نتيجة[()، AnyError]:
    ليكن القاعدة = sql.sqlite(العالم.fs، "notes.db")؟
    http.serve(العالم.net، port: 8080، router: المسارات()، app: تطبيق(القاعدة: القاعدة))؟
    أرجع نجاح(())
```
```kalam
# notes.franco.kl  [view: glossary]
hat std/http.{nafso, HttpError, Request, Response, Router}
hat std/sql.{nafso, Db}

haykal tatbee2:
    el_qa3da: Db

@derive(Encode, Row)
haykal molahza:
    el_raqam: adad
    el_3enwan: nass
    el_mo7tawa: nass

@derive(Decode)
haykal molahza_gdeda:
    el_3enwan: nass
    el_mo7tawa: nass

dala e3rad_molahza(el_talab: Request[tatbee2]) -> natiga[Response, HttpError]:
    khali el_raqam = el_talab.path_int("id")?
    khali el_estelam = sql"SELECT id, title, body FROM notes WHERE id = {el_raqam}"
    khali el_matlooba: ekhtiyari[molahza] = el_talab.app.el_qa3da.one(el_estelam)?
    rag3 hasab el_matlooba:
        lama mawgood(el_molahza): naga7(Response.json(el_molahza))
        lama mafeesh: fashal(.NotFound)

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
        .get("/notes/\{id\}", e3rad_molahza, access: .Public)
        .post("/notes", ensha2_molahza, access: .LoggedIn)

dala ra2isi(el_donya: donya) -> natiga[(), AnyError]:
    khali el_qa3da = sql.sqlite(el_donya.fs, "notes.db")?
    http.serve(el_donya.net, port: 8080, router: el_masarat(), app: tatbee2(el_qa3da: el_qa3da))?
    rag3 naga7(())
```
*Notes:*
- `?` on `DbError` inside a handler returning `HttpError` uses the stdlib `impl HttpError as From[DbError]` (§14.3). In `main`, `?` boxes into `AnyError`.
- Removing `access:` from a route is a compile error (§20.2).
- The commas *inside* `sql"…"` stay ASCII in the Arabic file, because they are literal text. The holes are code and are converted.
- In the Arabic `main`, the local `القاعدة` and the field label `القاعدة` share a spelling. That is legal: field labels are not bindings.
- `NewNote` derives only `Decode`, and `Note` derives `Encode` and `Row`. Decoding a request straight into `Note` would draw W0710 (mass assignment).
- The glossary is display-only, so the JSON keys and SQL columns are still `id`, `title` and `body` without any `@name` attributes (§4.5).

### 28.6 SQL query (and the injection that does not compile)

```kalam
# debtors.kl
use std/sql.{self, Db, DbError}

@derive(Row)
type Customer:
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
# debtors.ar.kl  [view: glossary]
استخدم std/sql.{ذات، Db، DbError}

@derive(Row)
هيكل عميل:
    المعرف: عدد
    الاسم: نص
    البريد: نص
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
Glossary note: the natural Franco spelling `3amil` (customer) would be rejected, because identifiers cannot start with a digit (§5.3, K0127). The glossary therefore uses `ameel`.
```kalam
# debtors.franco.kl  [view: glossary]
hat std/sql.{nafso, Db, DbError}

@derive(Row)
haykal ameel:
    el_raqam: adad
    el_esm: nass
    el_bareed: nass
    el_raseed: ashri

dala akbar_el_madyoneen(el_qa3da: Db, el_madina: nass, el_7ad: adad) -> natiga[qayma[ameel], DbError]:
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

### 28.7 HTML view: a live, server-driven component (D11)

```kalam
# todos.kl
use std/http.{self, Access}

type Todo:
    title: String
    done: Bool

type Model:
    owner: String
    todos: List[Todo]

@derive(Message)
enum Msg:
    Toggle(index: Int)
    ClearDone

fn update(model: Model, msg: Msg) -> Model:
    return match msg:
        case .Toggle(i):
            var todos = model.todos
            todos[i] = todos[i].with(done: not todos[i].done)
            model.with(todos: todos)
        case .ClearDone: model.with(todos: model.todos.filter(fn(t) => not t.done))

fn todo_item(index: Int, todo: Todo) -> Html:
    let state = if todo.done: "done" else: "open"
    return html"""<li class="{state}" k-click="{Msg.Toggle(index: index)}">{todo.title}</li>"""

fn view(model: Model) -> Html:
    let items = model.todos.enumerate().map(fn(pair) => todo_item(pair.0, pair.1))
    return html"""
        <section dir="auto">
          <h1>{model.owner}'s tasks</h1>
          <ul>{items}</ul>
          <button k-click="{Msg.ClearDone}">Clear done</button>
        </section>
    """

fn main(world: World) -> Result[(), AnyError]:
    let start = Model(owner: "Sara", todos: [
        Todo(title: "Pay rent", done: true),
        Todo(title: "<script>alert(1)</script>", done: false),
    ])
    let routes = http.router().live("/todos", init: start, update: update, view: view, access: .LoggedIn)
    http.serve(world.net, port: 8080, router: routes, app: ())?
    return Ok(())
```
```kalam
# todos.ar.kl  [view: glossary]
استخدم std/http.{ذات، Access}

هيكل مهمة:
    العنوان: نص
    منجزة: منطقي

هيكل نموذج:
    المالك: نص
    المهام: قائمة[مهمة]

@derive(Message)
تعداد رسالة:
    بدل(الرقم: عدد)
    امسح_المنجز

دالة حدث(النموذج: نموذج، الرسالة: رسالة) -> نموذج:
    أرجع حسب الرسالة:
        عندما .بدل(ر):
            متغير المهام = النموذج.المهام
            المهام[ر] = المهام[ر].with(منجزة: ليس المهام[ر].منجزة)
            النموذج.with(المهام: المهام)
        عندما .امسح_المنجز: النموذج.with(المهام: النموذج.المهام.filter(دالة(م) => ليس م.منجزة))

دالة عنصر_مهمة(الرقم: عدد، المهمة: مهمة) -> Html:
    ليكن الوضع = إذا المهمة.منجزة: "done" وإلا: "open"
    أرجع html"""<li class="{الوضع}" k-click="{رسالة.بدل(الرقم: الرقم)}">{المهمة.العنوان}</li>"""

دالة اعرض(النموذج: نموذج) -> Html:
    ليكن العناصر = النموذج.المهام.enumerate().map(دالة(الزوج) => عنصر_مهمة(الزوج.0، الزوج.1))
    أرجع html"""
        <section dir="auto">
          <h1>{النموذج.المالك}'s tasks</h1>
          <ul>{العناصر}</ul>
          <button k-click="{رسالة.امسح_المنجز}">Clear done</button>
        </section>
    """

دالة رئيسي(العالم: عالم) -> نتيجة[()، AnyError]:
    ليكن البداية = نموذج(المالك: "Sara"، المهام: [
        مهمة(العنوان: "Pay rent"، منجزة: صح)،
        مهمة(العنوان: "<script>alert(1)</script>"، منجزة: خطأ)،
    ])
    ليكن المسارات = http.router().live("/todos"، init: البداية، update: حدث، view: اعرض، access: .LoggedIn)
    http.serve(العالم.net، port: 8080، router: المسارات، app: ())؟
    أرجع نجاح(())
```
```kalam
# todos.franco.kl  [view: glossary]
hat std/http.{nafso, Access}

haykal mohemma:
    el_3enwan: nass
    khalset: mante2i

haykal namozag:
    el_saheb: nass
    el_maham: qayma[mohemma]

@derive(Message)
ta3dad resala:
    badel(el_raqam: adad)
    emsa7_el_khalsa

dala haddes(el_namozag: namozag, el_resala: resala) -> namozag:
    rag3 hasab el_resala:
        lama .badel(r):
            motghayer el_maham = el_namozag.el_maham
            el_maham[r] = el_maham[r].with(khalset: mesh el_maham[r].khalset)
            el_namozag.with(el_maham: el_maham)
        lama .emsa7_el_khalsa: el_namozag.with(el_maham: el_namozag.el_maham.filter(dala(m) => mesh m.khalset))

dala bond_mohemma(el_raqam: adad, el_mohemma: mohemma) -> Html:
    khali el_wad3 = law el_mohemma.khalset: "done" walla: "open"
    rag3 html"""<li class="{el_wad3}" k-click="{resala.badel(el_raqam: el_raqam)}">{el_mohemma.el_3enwan}</li>"""

dala e3red(el_namozag: namozag) -> Html:
    khali el_anaser = el_namozag.el_maham.enumerate().map(dala(el_zoog) => bond_mohemma(el_zoog.0, el_zoog.1))
    rag3 html"""
        <section dir="auto">
          <h1>{el_namozag.el_saheb}'s tasks</h1>
          <ul>{el_anaser}</ul>
          <button k-click="{resala.emsa7_el_khalsa}">Clear done</button>
        </section>
    """

dala ra2isi(el_donya: donya) -> natiga[(), AnyError]:
    khali el_bedaya = namozag(el_saheb: "Sara", el_maham: [
        mohemma(el_3enwan: "Pay rent", khalset: sa7),
        mohemma(el_3enwan: "<script>alert(1)</script>", khalset: ghalat),
    ])
    khali el_masarat = http.router().live("/todos", init: el_bedaya, update: haddes, view: e3red, access: .LoggedIn)
    http.serve(el_donya.net, port: 8080, router: el_masarat, app: ())?
    rag3 naga7(())
```
*Notes:*
- The second todo's title renders as `&lt;script&gt;…`. `{items}` is a `List[Html]` hole in text context, so it is inserted as is. The `class="{state}"` hole is attribute-escaped.
- Each `k-click` hole is a `Msg` value (K1616 otherwise). On the wire it is a signed, CSRF-bound opaque token, never JavaScript (§20.3). Clicking an item runs `update` on the server, re-renders `view`, and sends back a minimal patch.
- `.Toggle(i)` is a block arm in value position. Its last expression is the arm's value (§10.3). `todos[i] = …` is value-semantic rebinding of the local `var` (§10.4).
- `pair.0` and `pair.1` read the `(index, item)` tuple that `enumerate()` yields (§5.6 member rule).
- The Arabic `.with(…)`, `.filter(…)` and `.enumerate()` are stdlib names, so they stay ASCII (§7.2). In the lambda, `دالة(م) => …` is the Arabic spelling of `fn(t) => …`.

### 28.8 Small game loop (browser target)

```kalam
# bounce.kl
use std/game.{self, Input, Picture, draw}

type Ball:
    x: Float
    y: Float
    dx: Float
    dy: Float

type State:
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
# bounce.ar.kl  [view: glossary]
استخدم std/game.{ذات، Input، Picture، draw}

هيكل كرة:
    س: كسري
    ص: كسري
    سرعة_س: كسري
    سرعة_ص: كسري

هيكل وضع_اللعبة:
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
# bounce.franco.kl  [view: glossary]
hat std/game.{nafso, Input, Picture, draw}

haykal kora:
    x: kasri
    y: kasri
    sor3et_x: kasri
    sor3et_y: kasri

haykal halet_el_le3ba:
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
- `s.ball.x += …` is value-semantic rebinding (§10.4). The caller's `state` is never mutated.
- The glossary view may exceed 100 columns on some lines. The view wraps them deterministically per skin.

---

## 29. Open questions (design team)

These are engineering questions the design loop can settle by measurement. None of them needs the founder.

1. **Suspend-by-default for stored functions (§15.3).** Is it too pessimistic for performance-sensitive code? Measure the microtask cost on the corpus before adding effect variables on generic records.
2. **Capability ergonomics.** Is an `App` record carried by `Request[A]` enough, or do larger apps need a dependency-injection convention? Per-function capability manifests (Austral-style, 04 §7 Q2) were rejected as too heavy for v0.1.
3. **Core-name count.** Could `assert`, `Set` and `Decimal` stay untranslated to bring reserved spellings closer to the 05 D4 budget, if core names are counted (§26.1)? *Closed by D21: all 19 core names stay translated in every skin; the budget question is only how §26.1 counts them.*
4. **Record field privacy.** v0.1 has only whole-record `opaque`. Is per-field `export` needed?
5. **Stdlib alias packs** (02 §9 Q2). Should `Request`, `.map` and `Env` get official Arabic and Franco names? v0.1 says no, so APIs stay greppable and docs single-sourced.
6. **LLM mistakes as errors vs. hidden aliases.** Re-measure with 05 D3 self-repair rates whether accepting `def`/`elif` silently would raise first-try pass rates enough to justify a second spelling.
7. **Live-component state** across multiple server instances: sticky sessions, a store, or signed client state (Hotwire-style)? This belongs to the web kit, not the language.
8. **Trait objects / dynamic dispatch.** Postponed. Check whether records of functions are enough for the component kit.
9. **W0710 (mass assignment):** should it be an error?
10. **Typed views.** K1618 checks message holes in literals directly inside the registered `view`. Holes in helper functions are checked only for `Message`. Would a phantom `Html[Msg]` type be worth its inference cost?
11. **Mechanized soundness proof** of Kalam Core (§9.10) in Lean or Coq before 1.0: owner and budget.
12. **Terminal bidi detection** (§6.1). No reliable standard exists. Should the CLI always print the English-skin excerpt for Arabic files?
13. **No multi-line lambdas** (DR-15). Measure the effect on 05 D3 agent success. Block islands are the rejected alternative, and a trailing-block syntax is another candidate.
14. **Arabic confusable table and `arabic_fold` list** need a linguist review (04 §7 Q3).
15. **Route-pattern syntax.** Route patterns are plain strings, so path parameters need `\{id\}` (§20.2). A `route"…"` tag, an Express-style `:id`, or a typed route builder would remove the escapes. The same brace cost applies to JSON text, regex quantifiers and CSS in strings (no raw strings in v0.1, §5.7).

---

## 30. Founder decisions (resolved)

The founder answered every question this section used to ask (`DECISIONS.md`, 2026-10-25). They are locked decisions now, and the spec text cited for each one states them as normative. There are no open founder questions in v0.1.

| Former question | Decision | Choice | Spec |
|---|---|---|---|
| F1. `Int` range | **D18** | `Int` is a **true 64-bit checked integer** on every backend, [−2⁶³, 2⁶³−1], trapping on overflow. JS uses the canonical dual `number`/`bigint` representation with a `number` fast path. The 53-bit default is withdrawn. | §0 item 4, DR-02, §5.6, §9.2, §18.3, §19.2, §22 |
| F2. Ratify D7 | **D7 (resolved)** | §6 as written: logical order, no bidi controls anywhere, RTL display by tools, ASCII digits by default, Arabic-Indic digits only in Arabic files. | §6, DR-01 |
| F3. Vocabulary survey | **D22** | The `keywords.json` tables stay provisional. A survey of 200+ developers and a native-speaker review run before beta, covering the items flagged in DR-31 and §7.4 (`walla`/`aw`, `و`/`أو`, `عندما`/`lama`, `هيكل`/`haykal`, `بصفة`/`ka`, `علني`/`talla3`, `motghayer`). | §7, DR-31 |
| F4. Ambient console | **D20** | `print` and `log.*` are ambient (no capability). A library package that calls them must declare `console = true`. | X9, §16.4, K1504 |
| F5. Wire names in Arabic-first projects | **D23** | Arabic identifiers may be JSON keys and DB columns freely. There is **no** default ASCII wire-name lint. `@name("…")` stays available as an opt-in. | §4.5 |
| F6. Persian/Urdu letters | **D19** | ی ک ە ھ ۀ are **folded** to ي ك ه ه ه for keyword and identifier comparison, not rejected. A lookalike collision after folding stays a compile error (SEC0104). SEC0107 is retired. | X8, DR-04, §5.2, §5.3, §5.4 |
| F7. Franco skin naming | **D24** | Skin ID `franco`, file extension `.franco.kl`. | DR-18, §4.1 |
| F8. Translating core names | **D21** | All 19 core names are translated in every skin. | DR-19, §7.2 |
| (roadmap) | **D25** | About 28 months to 1.0, with the C/WASM backend before 1.0. | `ROADMAP.md` |

---

## Errata log

Spec errata are recorded here with a date, as ROADMAP M0 requires. Each erratum also needs a conformance test once `conformance/` exists.

| Date | Section | Change | Source |
|---|---|---|---|
| 2026-10-24 | §4.3 | Backtick escaping on conversion uses the target skin's full lookup: Franco spelling variants and Arabic `arabic_fold` equivalents count as reserved | `spec/CRITIQUE.md` C3 |
| 2026-10-24 | §4.4 | KCF keeps string text byte for byte only outside interpolation holes; hole code is serialized as code, so P1 can hold | CRITIQUE C4 |
| 2026-10-24 | §5.4 | K0201 is reported only where the other-skin word would otherwise be a syntax error or an undefined name; this removes the conflict with "reserved words of other skins are ordinary identifiers". The attached-`و` hint also covers keywords and core names | CRITIQUE C1, H6 |
| 2026-10-24 | §7.4 rule 1 | "Fold key" defined as the lexer's lookup key; the diagnostic-only Franco fold is excluded (under it, `we`/`aw` collide) | CRITIQUE C2 |
| 2026-10-24 | §17.6, §20.2, §28.5 | Unescaped `{3,20}` and `"/notes/{id}"` were interpolation holes (§5.7). Escaped as `\{…\}`; route syntax opened as Q15 | CRITIQUE C5 (corpus gaps G1, G2) |
| 2026-10-24 | §16.1 | `Net`: TLS verification always on (04 S12 MUST was untraced) | CRITIQUE S1 |
| 2026-10-24 | §24 | Traceability rows added for S15, P3–P5, P8, P11–P13 and G1–G4 | CRITIQUE S2 |
| 2026-10-24 | §27 | K0129 and K1504 adopted from ARCHITECTURE.md | CRITIQUE C6 |
| 2026-10-25 | §0, DR-02, §5.6, §5.9, §9.2, §17.3, §18.3, §19.2, §22, §25, §27 | `Int` is a true 64-bit checked integer, [−2⁶³, 2⁶³−1], on every backend; the 53-bit default is withdrawn. JS lowering is specified as a canonical dual `number`/`bigint` representation with a sound `Number.isSafeInteger` fast path. JSON integers decode exactly. Shift amounts range over `0..=63`. K0126 bounds are the i64 range and K0126 is reported by the type checker (unary minus admits −2⁶³) | `DECISIONS.md` D18 |
| 2026-10-25 | §5.2, §5.3, §5.4, §6.2, §7.4, DR-04, X8, §25, §27 | Persian/Urdu letters ی ک ە ھ ۀ are allowed and folded by `arabic_fold` to ي ك ه ه ه for keyword lookup and skeletons; a collision after folding is SEC0104. SEC0107 is retired | `DECISIONS.md` D19 |
| 2026-10-25 | §4.5, §30 | Arabic canonical names are valid JSON keys and DB columns; no default ASCII wire-name lint | `DECISIONS.md` D23 |
| 2026-10-25 | §6, DR-01, §30 | D7 recorded as founder-ratified (no longer "for ratification") | `DECISIONS.md` D7 (resolved) |
| 2026-10-25 | X9, §16.4, §30 | Ambient `print`/`log` with library `console = true` is locked, not a founder question | `DECISIONS.md` D20 |
| 2026-10-25 | DR-19, Q3, §30 | Translating all 19 core names in every skin is locked | `DECISIONS.md` D21 |
| 2026-10-25 | DR-31, §7.4, §30 | Vocabulary tables provisional pending the pre-beta survey (was F3) | `DECISIONS.md` D22 |
| 2026-10-25 | DR-18, §30 | Franco skin ID `franco` and extension `.franco.kl` are locked | `DECISIONS.md` D24 |
| 2026-10-25 | header, §30 | §30 "Founder questions" replaced by "Founder decisions (resolved)"; binding inputs are now D1–D25 | `DECISIONS.md` D18–D25 |
| 2026-10-25 | §4.4 | KCF no longer keeps blank lines "byte for byte" (that contradicted §21.1 rules 1, 2 and 4, so `kcf(fmt(x)) == kcf(x)` could not hold). KCF2 records a blank-line run as one blank line, and only where `fmt` keeps blank lines. It drops trailing commas except in `(x,)`, and serializes a one-line block like its indented form. The header is bumped to `KCF2`. Provisional | `FOUNDER-QUEUE.md` Q8; `eval/suites/conformance` |
