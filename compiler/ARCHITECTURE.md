# Kalam compiler and toolchain architecture

> **Status:** v0.1 architecture, 2026-10-24. It implements `spec/KALAM-SPEC.md` (v0.1) under the locked decisions in `DECISIONS.md`.
> **Binding inputs:** D4 (JS/TS first, C/WASM later), D5 (static types with inference), D6 (one file = one skin, lossless conversion), D10 (one Rust binary), D11 (server-driven UI), D12 (no `async`), D15 (indentation), D16 (`Result` + `?` and panic). Research inputs are reports 03 (implementation), 04 (security charter), 05 (scorecard and parity gates), 02 (Arabic and Franco) and 06 (ecosystem).
> **Precedence:** where this document and the spec disagree, the spec wins and this document has a bug. Where `spec/keywords.json` and anything else disagree, `keywords.json` wins (spec header).
> **Scaffold status:** see §22. The code under `compiler/` is a scaffold. It was written without a Rust toolchain on the machine, so `cargo check` has **not** been run yet.

---

## 0. Summary

- **One frontend, many backends.** This is architecture E from report 03, executed A-first. Every skin feeds one lexer, one lossless CST and one typed, target-neutral IR called **KIR**. The first backend emits ES2022 JavaScript with Source Map v3 and `.d.ts`. A C backend plugs into the same `Backend` trait later (§20).
- **Skins are data, not code.** The lexer loads `spec/keywords.json`, embedded at build time. Each file's extension picks one column of that table. Every later stage sees **concept IDs** (`KW_IF`, `CORE_INT`), never spellings. The printer turns concept IDs back into spellings for any skin. That one table drives lexing, formatting, conversion, KCF hashing, LSP completion and diagnostics.
- **One Rust binary, `kalam`,** holds the compiler, formatter, skin converter, LSP, MCP server, package manager and test runner (D10). The same frontend crates also build to WASM for the playground.
- **Security is structural.** Trojan Source checks run in the lexer, before any token exists. Injection is prevented by types: tagged literals are parsed at compile time by `kalam-sinks`. Capabilities are enforced at five layers, described in §9.2. Generated JS is scanned for forbidden constructs in CI.
- **No `async` in source.** The effect pass infers a suspension bit per function. KIR marks every suspension point, and the JS emitter lowers them to `async`/`await` (§11.2).

---

## 1. What the architecture must guarantee

| Requirement | Source | Where it lives |
|---|---|---|
| A file's skin comes from its extension, and there is one skin per file | D6, spec §4.1, DR-18 | `kalam-base::Skin::from_path`, `kalam-lexer` |
| Skins change lexemes only. There is one grammar | spec P7, 02 §7.1 | The parser consumes concept tokens only (§6) |
| Conversion is lossless (G1 identity, G2 fixpoint) | spec §4.3, 05 P1–P3 | The lossless CST plus a skin-parameterized printer (§13) |
| KCF hash as the unit of identity | spec §4.4, 04 U7 | `kalam-syntax::kcf` |
| Trojan-Source-proof source text | spec §5.1, 04 U1–U9 | `kalam-lexer::source_check`, the first pass over every file |
| D7: logical order, no bidi controls, ASCII digits by default, Arabic-Indic digits only in `ar` files | spec §6 (DR-01, a–d), founder question F2 | Lexer, printer, diagnostics renderer and LSP (§4.4) |
| Static types with inference, no implicit conversions | D5, 04 L3/L4 | `kalam-typeck` |
| `Result` + `?`, panic for bugs, must-use `Result` | D16, 04 L6 | `kalam-typeck` (K1301–K1303) and the KIR `?` desugaring |
| No `async` keyword; suspension is inferred | D12, spec §15 | `kalam-effects`, KIR call flags, `kalam-emit-js` |
| Capability-passing I/O | 04 L12, spec §16 | `kalam-typeck`, `kalam-effects`, the JS runtime, `kalam-pkg` (§9.2) |
| Typed sinks with compile-time template parsing | 04 S1–S3, spec §17 | `kalam-sinks` |
| JS output with no `eval`, `==` or `for…in`, prototype-safe maps, Trusted Types | 04 P14, spec §22 | `kalam-emit-js` plus a CI static scan |
| Target-neutral semantics, so C is possible later | 03 §4, spec P8 | KIR invariants (§10.2) and the `Backend` trait (§20) |
| One binary, plus a WASM playground | D10, 03 §6 | `kalam-cli`, `kalam-wasm` |
| Diagnostics in three languages, with stable codes and machine fixes | spec §21.4, 05 P6/D5, 01 §3.4 | `kalam-diagnostics` (§12) |

---

## 2. Pipeline and data flow

This is the spec §4.2 pipeline, with the crate that owns each arrow:

```
 bytes (.kl / .ar.kl / .franco.kl)
   │  kalam-lexer::source_check     §5.1 Trojan Source rules, NFC, BOM, CR (SEC01xx)
   ▼
 skin lexer (kalam-lexer + kalam-skins: keywords.json column for this skin)
   │  tokens carry concept IDs; trivia (comments, gaps) kept; INDENT/DEDENT/NEWLINE
   ▼
 lossless CST (kalam-syntax green tree, built by kalam-parser)
   ├──► kalam-fmt printer ──► text in any skin (fmt, convert, view, diff, rename --glossary)
   ├──► KCF serializer (kalam-syntax::kcf) ──► SHA-256 ──► lock, registry, signing, parity P1
   ▼
 AST views over the CST (typed accessors; no copy)
   ▼
 name resolution (kalam-resolve) ──► HIR: DefIds, resolved paths, module graph, imports
   ▼
 type and suspension inference (kalam-typeck, then kalam-effects)
   │  TypeckResults: expression types, method picks, `?` conversions, coercions
   │  EffectResults: suspension bit per fn, effect instances, capability sets
   │  kalam-sinks: compile-time parsing of sql/html/cmd/url/path/regex literals
   ▼
 capability and sink checks (K15xx, K16xx)  ──► module interface `.kli` (types, suspension bits, caps)
   ▼
 lowering (kalam-ir) ──► KIR: typed, target-neutral, desugared, explicit suspension points
   ▼
 Backend (trait in kalam-ir)
   ├── kalam-emit-js  ──► .mjs + .mjs.map (Source Map v3) + .d.ts (@js.export) + launcher
   ├── kalam-interp   (planned) ──► reference interpreter for differential testing
   └── kalam-emit-c   (future)  ──► .c + #line directives + bundled zig cc
```

**Artifacts and their consumers:**

| Artifact | Produced by | Consumed by |
|---|---|---|
| `LexOutput { tokens, trivia, diagnostics }` | lexer | parser; token-level converter (scaffold) |
| CST (green tree, `Arc`-shared, immutable) | parser | fmt/convert/view, AST views, KCF, LSP (semantic tokens, folding) |
| KCF bytes and SHA-256 | `kalam-syntax::kcf` | `kalam.lock`, registry, `kalam diff`, parity gate P1 |
| HIR plus `DefMap` | resolve | typeck, LSP (go to definition, references, rename) |
| `TypeckResults` | typeck | effects, lowering, LSP (hover, inlay types, completion) |
| `EffectResults` | effects | lowering, `.kli`, `kalam caps`, LSP suspension inlays (`⏸`) |
| `.kli` module interface | driver | dependents (separate compilation), `kalam pkg api` and `kalam pkg diff`, registry |
| KIR `Program` | lowering | backends; reference interpreter |
| `.mjs`, `.map`, `.d.ts`, launcher | emit-js | Node/Bun/Deno/browser, debuggers, npm consumers |

**Incrementality.** The driver is a query database: inputs are file texts and manifests, and everything else is a memoized function of them, keyed by content hash. Phase 0 uses a hand-written per-file cache. Phase 1 adopts `salsa`, the rust-analyzer model that report 03 §5 names. Dependency packages are never re-type-checked: their `.kli` files are the interface. The module is the unit of invalidation. A body-only edit that leaves the `.kli` unchanged does not re-check dependents. This is how 05 D6 (incremental ≤ 2 s at MVP, ≤ 200 ms at 1.0) and the LSP latency gates become reachable.

**Data representation.** All trees and IR use index-based arenas (`Idx<T>` into `Vec<T>`), as report 03 §6 recommends, to sidestep borrow-checker friction. Identifiers are interned (`Symbol`) as exact NFC code points. Nothing is folded (DR-04). Every node carries a `Span` (byte range) and its `FileId`. Line and column are computed on demand from `LineIndex`, and the LSP converts them to UTF-16 when the client negotiates it.

---

## 3. Workspace layout

```
compiler/
  Cargo.toml                 workspace (members = crates/*), shared lints and profiles
  ARCHITECTURE.md            this document
  crates/
    kalam-base/              Skin, FileId, Span, LineIndex                     (no deps)
    kalam-skins/             keywords.json → Concept + per-skin tables, arabic_fold, did-you-mean
    kalam-diagnostics/       Diagnostic model, code catalog ×3 languages, human/JSON renderers
    kalam-lexer/             source-text checks, skin-aware tokens, numbers, strings, layout
    kalam-syntax/            SyntaxKind, lossless green CST, typed AST views, KCF + hash
    kalam-parser/            LL(1) recursive descent with error recovery → CST
    kalam-fmt/               printer, formatter, skin converter, view, glossary display
    kalam-resolve/           module graph, scopes, one namespace, no shadowing, skeletons → HIR
    kalam-sinks/             compile-time parsers for sql/html/cmd/url/path/regex literals
    kalam-typeck/            types, inference, traits, methods, exhaustiveness, must-use
    kalam-effects/           suspension inference, @sync, capability sets, audit counters
    kalam-ir/                KIR, lowering from typed HIR, IR passes, Backend trait
    kalam-emit-js/           KIR → ES2022 + Source Map v3 + .d.ts + launcher
    kalam-driver/            session, VFS, query cache, pipeline, .kli, check/build entry points
    kalam-pkg/               kalam.toml, kalam.lock, resolver, registry, npm, capability diff
    kalam-test-runner/       `test` block discovery, JS harness, reporters
    kalam-lsp/               language server (stdio)
    kalam-mcp/               MCP server for agents
    kalam-wasm/              playground build (frontend + fmt + emit-js, no fs/net)
    kalam-cli/               the `kalam` binary
  (planned) crates/kalam-interp/   KIR reference interpreter (differential testing)
  (planned) crates/kalam-emit-c/   C backend (§20)
  (planned) runtime/js/            @kalam/runtime: ES2022 runtime, written in TypeScript
  (planned) xtask/                 repo tasks: keyword-table CI (§7.4), catalog parity, codegen
  testdata/skins/            spec §28.1–28.2 programs in all three skins (shared test fixtures)
../conformance/              spec §25 test directories (repo root, shared by all backends)
```

### 3.1 Crate responsibilities

| Crate | Owns | Depends on | Phase |
|---|---|---|---|
| `kalam-base` | `Skin` (`en`/`ar`/`franco`, from file extension), `FileId`, `Span`, `FileSpan`, `LineIndex` | — | 0 |
| `kalam-skins` | The embedded `keywords.json`, the `Concept` enum (one variant per row, checked against the JSON at load), per-skin lookup (exact for `en`, `arabic_fold` for `ar`, canonical plus closed variant list for `franco`), canonical spelling per skin, `did_you_mean` index, table integrity rules 1–4 of spec §7.4 | base | 0 |
| `kalam-diagnostics` | `Diagnostic` (code, severity, spans, labels, fixes with `machine`/`suggestion` applicability), the message catalog in en/ar/franco, the human renderer (Elm/Rust template), the JSON renderer (spec §21.4 shape) | base, skins | 0 |
| `kalam-lexer` | §5.1 source checks, word runs and keyword lookup, the "a digit always starts a number" rule, member-index rule, strings and tagged-literal tokens, comments as trivia, INDENT/DEDENT/NEWLINE with bracket suppression, leading-dot continuation | base, skins, diagnostics | 0 |
| `kalam-syntax` | `SyntaxKind`, green/red tree (rowan-style; `rowan` itself is a candidate), typed AST accessors, KCF serialization and SHA-256 | base, skins | 0 |
| `kalam-parser` | The §8.2 grammar: LL(1) after left-factoring at the four DR-34 points, no backtracking, panic-mode recovery at NEWLINE/DEDENT, and recognition of known LLM mistakes (K09xx) | base, lexer, syntax, diagnostics | 0 |
| `kalam-fmt` | The printer (CST → text in skin X), formatter rules (§21.1), `convert`, `view --glossary`, backtick escaping, `[skin.ar] digits` | base, skins, lexer, syntax, parser, diagnostics | 0 |
| `kalam-resolve` | Module paths from file paths (suffix stripped, K0301), imports (absolute, no globs K1203, cycles K1201), one namespace per scope (DR-11), no shadowing (K0405, K0413), skeleton collisions (SEC0104) and skeleton-based K0401 fixes, visibility and `internal/` (K0303), the prelude, HIR lowering | base, syntax, diagnostics | 1 |
| `kalam-sinks` | `StaticParts` provenance, SQL tokenizer (quotes, comments, `;`, identifier positions: K1602–K1605), HTML5 fragment tokenizer with hole contexts (K1610–K1618), argv splitter (K1620–W1622), URL/path checks (K1631, K1641), RE2 syntax check, the `Template` protocol for user tags | base, diagnostics | 1 |
| `kalam-typeck` | Bidirectional inference with HM unification inside bodies (§9.9), generics and bounds, traits with coherence and orphan rule, built-in trait facts (`Float` is not `Eq`, `Secret` is not `Encode`, capabilities are not `Decode`), method resolution (DR-27, K1103), Maranget exhaustiveness and usefulness (K1001/K1002), must-use (K0730/K1303), `?` conversion rules (K1301/K1302), sink typing (K1601), capability construction rules (K1501) | base, resolve, sinks, diagnostics | 1 |
| `kalam-effects` | Suspension fixpoint over the call graph, effect variables on fn-typed parameters, `@sync` checks (K1401/K1402 with the call chain), capability-set computation per exported fn (`kalam caps`), ambient-console check for libraries (X9), audit counters (`unsafe.*`, `extern`, `.expose()`, `@positional`, discarded `Result`s) | base, resolve, typeck, diagnostics | 1 |
| `kalam-ir` | KIR data types, lowering from typed HIR, passes (match → decision trees, `?` desugaring, value-semantic assignment, interpolation → `Show`, tagged literals → template construction, checked arithmetic, effect-instance selection), the `Backend` trait and `Target` | base, resolve, typeck, effects | 1 |
| `kalam-emit-js` | KIR → ESM, the runtime import contract, Source Map v3 (VLQ), `.d.ts` for `@js.export`, extern glue with boundary validation (§19.2), launcher with OS permission flags (§16.4) | base, ir | 1 |
| `kalam-driver` | Session and VFS (no direct `std::fs` in frontend crates), package and module graph, query cache, the pipeline, `.kli` read/write, `check`/`build`/`audit`/`caps` entry points shared by CLI, LSP, MCP and WASM | all frontend crates, emit-js | 0–1 |
| `kalam-pkg` | Manifest and lockfile, PubGrub resolution, registry client, npm tarball fetch with integrity and **no lifecycle scripts**, the 48-hour cooldown, capability diffs, typosquat checks, `api` and `diff` from `.kli` | base, diagnostics | 2 |
| `kalam-test-runner` | Discovers `test "…":` blocks, builds them in test profile, generates the JS harness, runs it under Node, reporters (human, JSON, TAP) | driver | 1 |
| `kalam-lsp` | `tower-lsp` server: diagnostics, hover, definition, completion in the file's skin, code actions (machine fixes), semantic tokens, suspension inlays, formatting, rename, skin conversion command, glossary display, paste conversion | driver, fmt | 1–2 |
| `kalam-mcp` | MCP tools `check`, `fix`, `fmt`, `convert_skin`, `run_tests`, `doc_search`, `explain` (spec §21.5) | driver, fmt, test-runner | 1 |
| `kalam-wasm` | `wasm-bindgen` exports `check`, `fmt`, `convert`, `compile` over an in-memory VFS | driver, fmt | 1 |
| `kalam-cli` | Argument parsing, `KALAM_LANG`, command dispatch, exit codes | all of the above | 0 |

### 3.2 Layering rules (CI-enforced)

1. **The frontend is pure and WASM-safe.** `base` through `emit-js` never touch `std::fs`, `std::net`, `std::process`, threads or the clock. All I/O goes through the driver's `Vfs` trait. CI runs `cargo check --target wasm32-unknown-unknown -p kalam-wasm`.
2. **Skins stop at the lexer and the printer.** Only `kalam-skins`, `kalam-lexer`, `kalam-fmt`, `kalam-diagnostics` (to render code in a file's skin) and `kalam-lsp` (completion) may look at a spelling. A spelling comparison anywhere else is a review-blocking bug, because it would break P7.
3. **No JavaScript concepts above `kalam-ir`.** Typeck and effects never mention `Promise`, `undefined` or `number`. This is spec P8 enforced at crate level.
4. **Diagnostics are data until rendered.** Crates return `Diagnostic` values. Only the CLI, LSP, MCP and WASM layers render them.
5. **Few, vetted dependencies.** The compiler is attack surface (04 G2). Each new dependency needs a written reason. The scaffold uses only `serde` and `serde_json`. Planned additions: `unicode-ident` and `unicode-normalization` (XID and NFC), `unicode-security` or our own tables (UTS #39 skeletons), `sha2` (KCF), `rowan` (optional), `salsa`, `tower-lsp` + `lsp-types`, `wasm-bindgen`, `clap`, `toml`, `pubgrub`.

---

## 4. Skins in the compiler

### 4.1 The keyword table is data

`kalam-skins` embeds `spec/keywords.json` at compile time (`include_str!`), so the binary and the WASM playground are self-contained and every compiler release pins one table. At startup it builds:

- **`Concept`**: one Rust enum variant per JSON row (53 in v0.1: 29 `keyword`, 2 `ascii_fixed`, 19 `core_name`, 3 `punctuation`). Loading fails if a JSON row has no variant or a variant has no row. The parser matches on `Concept::KwIf`, never on a string.
- **`SkinTable` per skin**, a map from spelling key to `(Concept, Spelling::Canonical | Variant)`:
  - `en`: exact match.
  - `ar`: the key is `arabic_fold(spelling)` (أ إ آ ٱ → ا, ى → ي, ة → ه, ؤ → و, ئ → ي), so `اذا` and `إذا` are one keyword (§5.4). `fmt` writes the canonical hamza form.
  - `franco`: the canonical spelling plus the closed `franco_aliases` list. A variant is accepted, and `fmt` rewrites it to canonical.
- **`did_you_mean` index**: spellings that are **never accepted**, used only by K0127/K0401 messages (`5ali` → "did you mean `khali`?").
- **Integrity checks** (spec §7.4, rules 1–4) run on every load (the table is small) and therefore in CI: no two concepts share a fold key in one skin, no Franco spelling starts with a digit, no Arabic canonical spelling contains tatweel or harakat, and `unsafe`/`extern` are identical ASCII everywhere. Rules 5–7 (edit distance, the 2,000-word list, word-choice notes) need data files that do not exist yet, so they become `xtask` checks.

Adding a Franco variant is a minor version change, and removing one is an edition change (§7.3). Because the table is embedded, that is a compiler release. The packaging limit (crates.io refuses files outside the crate) is solved by an `xtask sync-keywords` step that copies the file into the crate before publishing. The spec copy stays authoritative.

### 4.2 Tokens carry concepts, the CST keeps spellings

A keyword token is `Keyword(Concept)` with a flag that records whether the spelling was a variant. A CST token keeps the exact source text, so the tree is lossless. The **parser sees only the concept**, so the grammar is skin-free (P7). **KCF** serializes the CST with keyword and core-name tokens as concept IDs, `،`/`؟` as `PUNCT_COMMA`/`PUNCT_TRY`, numbers as kind plus exact value (no digit glyphs), identifiers as exact NFC code points, and strings, comments and blank lines byte for byte (§4.4). G1 (`kcf(parse_X(s)) == kcf(parse_Y(print_Y(parse_X(s))))`) therefore holds by construction once the printer is right, and the tests check that it is.

### 4.3 Contextual freedom and escapes

Reserved words are identifiers after `.`, as named-argument labels and as record field names (§5.3 rule 8). The lexer handles the `.` case itself: a word right after a `.` token is always `Ident`. Labels and field names are a parser concern: the parser accepts a `Keyword`/`Core` token in those positions and builds an identifier node from its text. When the printer emits an identifier that is reserved in the target skin, it wraps it in backticks. When converting back, it removes backticks that are no longer needed. H0202 (the LSP hint that an identifier equals another skin's reserved word) reads the other two `SkinTable`s.

### 4.4 D7 (Arabic RTL and digits): how each tool implements spec §6

D7 is resolved in spec §6 (DR-01, DR-01a–d), pending the founder's ratification (F2). This architecture implements it as written and does **not** edit `DECISIONS.md`.

Reports 02 and 04 conflict on four points. In each case the security charter or a locked decision wins, and the toolchain implements the stricter rule:

- 02 §7.5 treats LRM and RLM as whitespace. 04 U1 wins: every bidi control, including ALM, is rejected raw in code, comments and strings.
- 02 §7.5 accepts Persian digits. 04 §4.1 (confusables) wins: they are rejected (SEC0121), and digits inside identifiers are rejected too (SEC0124).
- 02 §7.1 accepts English keywords in every skin. D6 (one file = one skin) wins: they are K0201, with a machine fix, and the LSP converts them on paste.
- 03 §4.1 and 04 U5 allow digit-leading Franco identifiers. They are dropped entirely. A digit always starts a number, and the natural spellings live only in `did_you_mean`.

| D7 rule | Implementation |
|---|---|
| Files are stored in logical order. Bidi controls (embeddings, overrides, isolates, LRM, RLM, **ALM U+061C**) are rejected raw everywhere, strings included | `kalam-lexer::source_check`: SEC0110 in code, comments and strings. The fix-it for a string is the `\u{…}` escape. It runs before tokenization, so no later stage ever sees one |
| Base display direction comes from the skin | `Skin::is_rtl()`. LSP semantic tokens and the VS Code extension set the paragraph direction per file. The playground sets `dir="rtl"` on the editor for `.ar.kl` |
| First-party renderers isolate each token (UTS #55) and mirror brackets | LSP semantic-token ranges are per token. The VS Code extension renders each token in a bidi isolate. `kalam view`, `kalam diff` and the playground use the same token stream. `kalam check --bidi` runs an ICU-style bidi-skeleton check over lines whose visual order could still mislead a generic viewer (04 U8) |
| Reviewers can always see an unambiguous view | `kalam diff --skin en` prints both sides through the English printer, with KCF hashes |
| Terminal diagnostics for `ar` files | The renderer prints the excerpt twice: as written, and re-printed in the English skin with the same span highlighted. Controlled by `--excerpt=ar\|en\|both` (default `both`). The message prose follows `KALAM_LANG` (§12.3) |
| ASCII digits by default. Arabic-Indic `٠–٩` only in `ar` literals, one system per literal, `٫` only with Arabic-Indic digits, no exponent or radix prefix with them | Lexer number rule (§5.2): SEC0120 (mixed), SEC0121 (Persian), SEC0122 (Arabic-Indic outside `ar`), SEC0123 (U+066C), SEC0124 (in an identifier), K0128 (radix or exponent with Arabic-Indic) |
| The `ar` printer emits Arabic-Indic digits only under `[skin.ar] digits = "arabic-indic"` | `kalam-fmt::FmtConfig::ar_digits`. KCF stores values, so G1/G2 hold under either setting |
| `،` and `؟` are the only punctuation aliases, `ar` only, and the `ar` printer always emits them | Lexer: accepted in `ar` (flagged as Arabic glyphs), K0201 in `en`/`franco`. Printer: always `،`/`؟` in `ar` |
| String contents are never normalized or reordered | Strings are opaque to every pass except escape decoding. `std/web` adds `dir="auto"` and `<bdi>` around text holes (runtime concern) |

---

## 5. Lexer

### 5.1 Order of work for each file

1. **Encoding and file-level checks:** valid UTF-8 and no BOM (SEC0100), NFC (SEC0101), lone CR, U+2028 and U+2029 (SEC0113). CRLF is accepted, and `fmt` rewrites it.
2. **Per-character class checks while scanning,** because a class's treatment depends on whether the character is in code, a comment or a string (spec §5.1 table). For example, a tab is K0103 in code, allowed in a comment, and must be written `\t` in a string.
3. **Tokenization with layout.** SEC diagnostics are never suppressible, but the lexer continues after reporting them so one run shows every problem.

### 5.2 Numbers ("a digit always starts a number")

When the lexer meets a digit (ASCII anywhere, or Arabic-Indic in an `ar` file), it reads the maximal run of letters, digits and `_`. It absorbs one `.`/`٫` only if a digit follows and the run so far is a plain decimal integer. It absorbs one `+`/`-` only directly after `e`/`E` in a decimal run, when a digit follows. The whole run must then match `INT_LIT` or `FLOAT_LIT`. Otherwise it is K0127, and the message includes a `did_you_mean` spelling when one matches. Right after a `.` token, a digit run is a decimal member index only, never a float (`t.0.1`). The lexer stores the literal's digit system as a flag, so the printer can honour `[skin.ar] digits`, and KCF stores the value.

### 5.3 Layout

- **Indentation:** Python's algorithm with a 4-space unit (K0120), growth by one level at a time, dedents that land on a known level (K0122), and EOF closing every open level.
- **Bracket suppression:** newlines and indentation inside `(`, `[` and `{` are ignored, with no exceptions (DR-15).
- **Leading-dot continuation** (DR-05): a line starting with `.` joins the previous logical line only if it is indented strictly deeper than that line's first line **and** that line does not end with a block colon. The lexer keeps a small "logical line" record (first-line indent, last significant token) to decide this without lookahead into the parser.
- Blank and comment-only lines produce no NEWLINE.

### 5.4 Deferred in the scaffold

The scaffold lexer (`crates/kalam-lexer`) implements source-character checks, words and keywords for all three skins, numbers with all the digit rules above, strings (single-line and `"""`), tagged-literal tokens, raw identifiers, comments, operators, Arabic punctuation and layout with bracket suppression. Still deferred:

- leading-dot continuation;
- NFC verification (needs `unicode-normalization`);
- exact XID and single-script checks (it uses `char::is_alphabetic` as a stand-in, and SEC0105 needs script data);
- the emoji ZWJ and variation-selector exceptions in strings;
- splitting interpolation holes into sub-token streams (the parser will re-lex holes, which are code and convert like code, §4.3);
- K0201 for English keywords in `ar`/`franco` files, which needs parser position (§5.4 bullet 2 vs. 3).

---

## 6. Parser, CST and AST

- **Green/red tree.** The parser emits events (`start node`, `token`, `finish node`) into a builder that produces an immutable, `Arc`-shared green tree. The tree holds every token, including trivia and the exact spelling. Red nodes add parent pointers and offsets on demand. This is the rust-analyzer/rowan design. Whether to use the `rowan` crate or a ~500-line in-house tree is open item A1 (§23).
- **Typed AST views:** `ast::FnDecl`, `ast::IfForm` and so on are zero-cost wrappers over CST nodes with accessor methods. There is no second tree to keep in sync.
- **Grammar class:** LL(1) after left-factoring at four points (DR-34): `if` at statement or rhs start, `(` unit/group/tuple, expression statement vs. assignment, and named vs. positional argument. The parser follows §8.3 literally, so `if x < 0: return Err(.Neg)` is a statement and `let s = if a: b else: c` is an expression. CI generates a tree-sitter grammar for all three skins from the same EBNF and runs an LL(1) conflict report (spec §8.4).
- **Error recovery:** a missing token is inserted virtually and reported once. On an unexpected token the parser skips to the next NEWLINE or DEDENT at the current block depth. An `ERROR` node keeps the skipped tokens in the CST, so `fmt` never deletes code. This is what the 05 D5 error-recovery metric (≥ 0.5 genuine/reported at MVP) measures.
- **Known LLM mistakes** (K0901–K0926) are recognized only in the obvious syntactic position, each with a machine fix: `def`/`function` at item start, `elif`, `;`, `{` as a block opener, `===`, `&&`, `c ? a : b`, `impl Show for Money`, and so on. `def` as a variable name stays legal.

---

## 7. Name resolution

- **Module graph:** a module path is the package name plus the file path, without `src/` and without the skin suffix. If `users.kl` and `users.ar.kl` both exist, that is K0301. Imports are absolute. Cycles are K1201, found with Tarjan SCC over `use` edges. Glob imports are K1203.
- **One namespace per scope** (DR-11). Types, enums, aliases, traits, functions, values and module aliases share one map. **No shadowing** (DR-28): a declaration that reuses any visible name is K0405, or K0413 when the name is a core name.
- **Skeletons:** for every declaration, `skeleton = UTS39_skeleton(arabic_fold(NFKC(id)))`. Two distinct identifiers with one skeleton visible in one scope, or exported from one module, are SEC0104, and the message shows both declarations with their code points. An unresolved use whose skeleton matches exactly one declaration is K0401 with a machine fix to the declared spelling. Uses are never folded silently.
- **Output:** HIR with `DefId`s. Every path is resolved to a definition, a local, a prelude item, or an error placeholder that suppresses follow-on errors.
- **Implicit members** (`.Circle`) are recorded as unresolved, and typeck resolves them against the expected type (K0803).

---

## 8. Type checker

- **Inference:** bidirectional checking over HM unification inside a body. Top-level, `impl` and local `fn` signatures are mandatory. Local `let` is monomorphic. Literals take their type from the expected type (`Int` literal → `Float`/`Decimal`/`BigInt`; float literal → exact `Decimal`). There is no call-site type-argument syntax (DR-25). Return-type inference covers `json.decode`.
- **Error poisoning:** an unknown type becomes `Ty::Error`, which unifies with everything and produces no further diagnostics. That is how "one root cause first" (spec §21.4 rule 2) is achieved without heuristics.
- **Traits:** coherence (one `impl T as Tr` program-wide, no overlap) and the orphan rule. Built-in trait facts encode the spec tables: `Float` is not `Eq`/`Ord`/`Hash`, `Secret[T]` is not `Encode`/`Eq` (K0612), capabilities have no `Decode`/`Eq`/`Hash`, and functions have none of `Eq`, `Hash`, `Show`, `Encode`, `Decode`.
- **Method resolution** (DR-27): candidates are the inherent methods of the receiver type plus methods of every trait implemented for it anywhere in the program. Exactly one must exist, or it is K1103. Imports never change the result.
- **Patterns:** Maranget's usefulness algorithm gives exhaustiveness (K1001, with missing arms as a machine fix) and unreachable arms (K1002). Or-pattern binding checks are K1004. W0403 fires for a `_` arm over a foreign enum, and it is an error under `@exhaustive_strict`.
- **Must-use:** a non-`()` expression statement is K0730, and a `Result` one is K1303.
- **`?`:** the four rules of §14.3. The chosen conversion (`Same`, `From(impl)`, `BoxAnyError`) is recorded in `TypeckResults` for lowering.
- **Sinks:** for every tagged literal, typeck calls `kalam-sinks` with the literal's `StaticParts` and hole spans. The sink parser classifies each hole's context and returns the required hole type for each. Typeck then checks the hole expressions against them (K1601, K1613, K1616, K1641, …).

### 8.1 The M2 typing pipeline (implemented 2026-10-25)

```
kalam-resolve output ──adapter──► kalam_typeck::hir (ResolvedProgram)
        ▼  check_program: pass 1 collect (ADTs, aliases, traits, impls, every signature → Scheme; coherence/orphan)
        ▼                 pass 2 bodies (one InferCtx per body; bidirectional + union-find)
TypeckResults ──► kalam-effects (solves SuspFlow constraints, call graph) ──► kalam_ir::lower ──► KIR ──► verify ──► interpreter / emit-js
```

**The resolved-program contract (`kalam_typeck::hir`).** Typeck consumes a small skin-free HIR in which every name is already resolved: `ExprKind::Local(LocalId)`, `Def(DefId)`, `Lang(LangItem)` for the core values (`print`, `panic`, `assert`, `Some`, `None`, `Ok`, `Err`), and `ImplicitMember` for `.V` (typeck resolves it against the expected type). Types are already lowered to `hir::Type` (`BuiltinType`, `Def`, `Param(i)`, `SelfType`, `Fn { sync }`). Every expression has an `ExprId` unique within its body; every local and parameter a `LocalId`. The trait is `ResolvedProgram { item_ids(), item(def) -> Option<&Item> }` (a `DefId` that is not an item yields `None`, which the checker treats as an already-reported error, never a panic: typeck is a fuzz target), and `HirProgram` is an in-memory implementation.
- **Dependency direction.** `kalam-typeck` depends on `kalam-resolve`, so resolve cannot implement a trait defined in typeck without a cycle. The adapter (`impl ResolvedProgram for …`, or a function that builds a `HirProgram`) lives **in `kalam-typeck` or `kalam-driver`**, never in resolve. It walks `kalam_ast` together with resolve's `Program` (its `uses`/`decls` tables keyed by name spans) and emits HIR. Resolve keeps owning `DefId` and `ModulePath`, which HIR reuses unchanged.
- **Obligations on resolve** (listed in `hir.rs`): DefIds for every item, method, variant and record; one-namespace, no-shadowing, visibility and `break`-outside-loop checked before typeck; an impl's generics numbered before its method's; constants listed in dependency order; unresolved names lowered to `ExprKind::Error` after K0401.
- `kalam_typeck::builder` builds HIR by hand, and `builder::fixtures::{hello_world, fizzbuzz}` are the spec §28.1–§28.2 programs as HIR (named after the spec, not the corpus, so compiler sources stay clear of the A5 corpus-leak scan). Later passes (lowering, the reference interpreter) test against them until resolve produces HIR from source.

**Types (`kalam_typeck::ty`).** `Ty` has the §9.2/§9.3 primitives (`Int` is the D18 64-bit checked integer), `Range`, `Fn { params, ret, suspension }`, `Adt(DefId, args)` (aliases are expanded during lowering), `Capability`, `Sink`, `Secret`, `AnyError`, `JsValue`, rigid `Param(i)`, inference `Var(i)` and the poison `Error`. A `Scheme` is a declared signature: its generics with trait bounds plus `effect_params`. **There is no let-generalization** (§9.9 rule 2), so schemes come only from annotated signatures, and pass 1 collects them all before any body is checked (mutual recursion needs no ordering).

**Suspension bits (§15.3).** A written `fn(…)` type gets its bit from its position:
- directly a declaration parameter: an effect parameter `Suspension::Param(k)`, which makes the declaration effect-polymorphic;
- inside data (fields, payloads, collection elements, nested types): `Suspending`;
- directly a local annotation: a fresh effect variable;
- `@sync fn(…)`: `Sync`.

A declaration's own bit is one more effect parameter, or `Sync` under `@sync`. Unification never equates bits, because sync-where-suspending-is-expected is a direction and the bits depend on the call graph. Unifying two fn types records `SuspFlow { from, to, span }` (from ≤ to) in `TypeckResults::flows`, and `kalam-effects` solves those together with the call graph. The same spans are where lowering inserts `Coerce::SyncToSusp`.

**The inference core (`kalam_typeck::unify`).**
- Union-find over type variables (union by rank, path compression), with values at the roots and an occurs check.
- `Ty::Error` unifies with everything. `Never` is not unified: `Never <: T` is applied as a coercion wherever a value is checked against an expected type (`if c: panic(…) else: 3`).
- Literals follow §5.6 exactly. An integer literal is `Int` unless the expected type is `Float`, `Decimal` or `BigInt`. K0126 is checked against the i64 range, and the direct operand of unary `-` admits −2⁶³.
- In a binary operator, a literal operand takes its type from the other operand, whichever side it is on. So `2 * price` types like `price * 2`, and `x + 1.5` with `x: Int` is K0601, not a plain mismatch.
- Trait requirements whose type is still a variable are deferred to the end of the body.

**Traits (`kalam_typeck::traits`).** `TraitId` is a built-in trait of the §9.7 table (`From` and `Template` included) or a user trait `DefId`. An `ImplDecl` records its module, generics, self type and trait reference.
- **Orphan rule:** the impl's module must equal the trait's module or the self type's head module. Built-in traits and built-in types are defined by the core (`None`), so `impl Int as Show` is always an orphan.
- **Overlap:** two impls of one trait overlap when their heads unify after their generics are made fresh. Bounds are ignored, so this is conservative.
- **Solving:** user impls first (a custom `Show` overrides the derived one), then explicit `@derive`s, then structural derivation for `Eq`/`Hash`/`Show`, then the §9.7 facts for built-in types, then the bounds of the generic parameters in scope. It is coinductive for recursive ADTs. `==` on two `Float`s skips `Eq` (IEEE).

**`TypeckResults`.**

| Field | Contents |
|---|---|
| `adts` | Lowered records and enums |
| `signatures` | Every function and method scheme, signature stubs included |
| `consts` | Constant types |
| `traits` | The `TraitEnv` |
| `expr_types`, `local_types` | Keyed by `(owning body DefId, ExprId/LocalId)`, fully zonked. A variable nothing constrained becomes `Error` |
| `instantiations` | Type arguments at each generic use |
| `method_picks` | DR-27 method resolutions |
| `variant_picks` | Variants chosen for implicit members |
| `flows` | Suspension flows (`SuspFlow` constraints) |
| `errors` | Typed `TypeError`s; `code()` maps each to §27 and `to_diagnostic()` renders it |

**What the checker covers now:**
- Literals and K0126; `let`/`var` and assignment (K0710).
- Functions, generic calls, named and defaulted arguments.
- Record construction (K0416) and fields; variants and implicit members (K0803).
- `if`/`match` in statement and value form (K0731).
- A shallow exhaustiveness check (K1001) for `Bool`, `Option`, `Result`, enums, and the "final `_`" rule for `Int`/`String`/`Decimal`.
- `for` over ranges and collections, `while`.
- Operators, with K0601/K0602 and `Int / Int → Float`. `Decimal` has no `/`.
- Interpolation needs `Show`. `print[T: Show](value: T)` (provisional, pending a founder fork) and K0916.
- Tagged literals typed as sink types; K1601 for `String` where a sink is expected.
- K0612; K0730/K1303; K1101; DR-27 method calls (K0420/K1103); coherence/orphan.

**Not yet:**
- Maranget usefulness: nested exhaustiveness and K1002.
- `?` (§14.3).
- Hole requirements (waits for `kalam-sinks::check_template`).
- `.with`, K1110, local `fn` items, K1004 or-pattern bindings, and the entry-point signature.

**Provisional codes** (no §27 code exists; `error::PROVISIONAL_CODES`, pending the founder):
- `K0600`: type mismatch, not callable, not iterable, infinite type.
- `K0603`: operator or trait requirement not satisfied.
- `K1105`: argument count or labels.
- `K1106`: orphan or overlapping impl.

`K0420` (§27: unknown method) is also used for unknown fields. Every code typeck emits needs a `kalam-diagnostics` catalog entry and an `EMITTER_ARGS` row (`catalog_completeness.rs`); those are owned by the diagnostics crate and still to be added.

**KIR for the interpreter (`kalam-ir`).**
- **Program:** `Program` now carries `records` and `enums`, and `Function` carries `locals` (every local with its type) and `generics`.
- **Calls:** `Call` carries `type_args`. `Callee::Prelude(Print | Assert)`; `panic` is `ExprKind::Panic`.
- **Expressions:** `If` and `Block` are expressions (value-form `if`/`match`; `and`/`or` short-circuit as `If`).
- **Loops:** `Stmt::ForRange` iterates `Int` ranges without overflow at `i64::MAX`. `Stmt::ForEach` iterates collections.
- **Values:** `Variant` covers user enums and the built-in `Option` (`None` = 0, `Some` = 1) and `Result` (`Ok` = 0, `Err` = 1).
- **Primitives:** `PrimOp` gained the full comparison set per type, `Decimal`, `BigInt` and `Show` primitives, and `ListGet`/`ListLen`/`ListConcat`, each with its spec semantics documented.
- **Verifier:** `kalam_ir::verify::verify` checks the invariants: no `Var`/`Error` types, declared locals, existing callees, primitive arity, `break` only inside loops, assignment only to mutable locals.
- **Lowering:** `kalam_ir::lower::lower_program` lowers the checker's subset. Named arguments are evaluated in source order into temporaries, interpolation becomes `*Show` + `StrConcat`, and literal `match` becomes an `if` chain ending in a defensive panic. Anything else is `LowerError::Unsupported`, never a silent miscompile.
- **End-to-end tests:** a test-only evaluator runs the lowered §28.1 and §28.2 programs and matches their expected stdout byte for byte (the same output corpus tasks T01 and T02 expect).

---

## 9. Effects: suspension and capabilities

### 9.1 Suspension inference (no `async`, D12)

- **Base set:** stdlib primitives whose declarations carry the internal attribute `@suspends` (network, fs, db, timers, `task.*` joins, `Screen.next_frame`), and externs declared `@js(async: true)`.
- **Fixpoint:** a function suspends if it calls a suspending function or calls a function value whose suspension bit is set. Each bit is `always ∨ ⋁ params` over the declaration's effect parameters (`kalam_typeck::SuspBit`). Per body, `kalam-effects` (`suspension.rs`) solves the typeck `SuspFlow`s plus one edge per call, from the callee's bit to the enclosing closure's bit (a call inside a lambda or local `fn` makes that closure suspend, not its definer). Summaries only grow, so a worklist over the call graph (callees first; callers re-solved when a summary grows) reaches the least fixpoint, which is the per-SCC fixpoint of spec §15.2. A required trait method dispatched on a generic receiver joins every impl of it in the program (provisional). Stubs (`@builtin`, body-less) take their bit from the signature: `@suspends` always, `@stores_callbacks` never, otherwise exactly their non-`@sync` callbacks.
- **Where it runs:** `kalam-effects` depends on `kalam-typeck`, so the checker cannot call it. `kalam_typeck::check_resolved_then` runs a post-pass on the checker's big stack; `kalam_effects::{check_resolved, diagnose, diagnose_against_std}` are the checker's entry points with the pass added, and the driver calls those. The result is `TypeckResults::suspension` (per function, per call site, per lambda and local `fn`).
- **Effect polymorphism:** each fn-typed *parameter* gets an effect variable. At each call site, the variables are instantiated from the arguments' bits. A polymorphic function has at most two instances, sync and suspending, named `f` and `f$s` (spec §22). Only instances that are actually used are emitted.
- **Function types in data** (fields, payloads, collection elements, extern params) are suspending unless written `@sync fn(…)` (DR-13). Storing a sync function there inserts a `Coerce(SyncToSusp)` node in KIR. It is a no-op in JS and becomes an adapter in C.
- **Checks:** `@sync` on a type (K1401) and on a declaration (K1402). The diagnostic prints the shortest call chain to a base suspending function, found by BFS with parent links.
- **Outputs:** a bit per function, a flag per call site, and per-export bits in `.kli`. sync → suspending is reported by `kalam pkg diff` as semver-major (§15.5). The LSP shows a `⏸` inlay at every suspending call site, which replaces the visual cue `await` would have given.

### 9.2 How capabilities are enforced (five layers)

Capabilities are ordinary values of prelude types (`World`, `Net`, `Fs`, `Env`, `Clock`, `Rand`, `Proc`, `Stdin`, `Screen`) with no constructor. The guarantee "a capability value exists only if it was derived from the `World` given to `main`" (§9.10) rests on five independent layers:

1. **Type system (`kalam-typeck`).** Capability types have no constructor, no `Decode`, `Eq` or `Hash`, and cannot appear in module-level constants, because the const evaluator rejects them. Constructing or decoding `World` is K1501. The entry point must be `fn main()` or `fn main(world: World)`. `Screen` outside `js-browser` is K1502. Attenuation (`net.only(hosts:)`, `fs.dir(path"…")`, `fs.read_only()`) is an ordinary method returning the same capability type, so attenuated values flow through the same rules.
2. **Stdlib shape.** Every effectful stdlib function takes a capability parameter, or receives one through a handle built from one (`Db` from `sql.postgres(world.net, …)`). The stdlib reaches the platform only through `extern` modules, which are compiled with a privileged flag that user packages cannot set. The ambient effects are a closed list: `print`, `log.*` and `panic` (X9).
3. **Effect analysis (`kalam-effects`).** For each exported function, the capability types reachable from its parameters and captures, plus its package's declared extern effects (§16.2), form its capability set. It is written to `.kli` and printed by `kalam caps`. A library package that calls `print` or `log.*` without `console = true` in its manifest gets an error (K1504, spec §27). Externs are allowed only for modules listed in `[npm]` (K1701), and a capability can cross the JS boundary only as an erased token (K1702).
4. **JS runtime (`kalam-emit-js` + `runtime/js`).** Capability objects are instances of runtime classes that are never exported to Kalam scope, so generated code cannot construct one. Only the generated entry module calls `$rt.makeWorld(manifestCaps)`. Generated code never references a JS global. A global is reachable only through `extern` with `@js(global: …)`, which needs the matching manifest capability. Attenuation is enforced at runtime too: `Net.only` checks hosts on every request, and SSRF ranges are blocked after DNS resolution.
5. **OS sandbox and supply chain.** The launcher that `kalam build` generates derives Node `--permission --allow-fs-read=…` and Deno `--allow-net=…` flags from the root manifest's capability list (§16.4), so a bug in an extern binding still hits a second wall. `kalam-pkg` shows a capability diff on `add`/`update`, and additions need approval (04 P6). FFI packages are flagged on the registry, and `kalam audit` counts them on the AST.

`kalam audit` reports, per package: `unsafe.*` calls, `extern` declarations, `.expose()` calls, `@positional` functions with adjacent same-type parameters, deliberate `let _ =` discards of `Result`, `std/crypto/legacy` imports, third-party `Template` impls, `Command`s whose program is a shell, and OSV advisories. All counts come from HIR, never from grep, so they are identical in every skin (04 L13).

---

## 10. Lowering and KIR

### 10.1 Shape

KIR is a **structured, typed, expression-oriented IR**, not an SSA CFG. Structured control flow (`Block`, `If`, `Switch`, `Loop`, `Break`, `Continue`, `Return`) keeps the JS output readable and debuggable, and it is also what a C emitter wants. One `kir::Program` exists per package. It has functions, records, enums, constants and externs, and every node keeps its source `Span`.

### 10.2 Invariants (the target-neutrality contract)

1. **Every type is explicit** on every expression. Generics are kept, and type parameters are explicit. Backends that need monomorphization (C) run the shared `monomorphize` pass. JS does not need it.
2. **Every operation has spec semantics, not host semantics.** `IntAdd`/`IntSub`/`IntMul` trap outside the signed 64-bit range [−2⁶³, 2⁶³−1] (DECISIONS D18). `IntFloorDiv` and `IntMod` floor and panic on 0. `FloatEq` is IEEE. `StrConcat`, `ListGet` and the others panic out of bounds. No backend may implement these with "whatever the host does".
3. **Evaluation order is explicit.** Lowering introduces temporaries wherever a backend could reorder evaluation, so the left-to-right rule (§10.6) is a property of the IR, not of the target.
4. **Every suspension point is marked.** `Call { suspends: bool, instance: Sync | Susp }`. Function definitions carry `suspends`. `Coerce(SyncToSusp)` is explicit.
5. **Sugar is gone:** `?` is a `Switch` plus early return with the recorded conversion. `match` becomes a decision tree with a defensive panicking default. Value-semantic assignment becomes `with`/`set`/`insert` rebinding. Interpolation becomes `Show` calls plus concatenation. Tagged literals become `MakeTemplate { tag, static_parts: StaticPartsId, holes }`. Default arguments are filled in at call sites. Named arguments become positional.
6. **No JS-only concept** (`undefined`, `Promise`, prototype) appears in KIR. `Option` is `Option`. The JS emitter chooses the `undefined` representation (§22) and the C emitter chooses a tagged one.

### 10.3 Passes

`lower` (typed HIR → KIR), `decision_trees`, `effect_instances` (clone `f` into `f$s` where it is used), `range_check_elision` (may only *remove* integer checks it proves redundant, §22), `uniqueness` (in-place updates for provably unshared collections), `monomorphize` (C only). Each pass is a `kir::Program → kir::Program` function with a verifier that checks the invariants above in debug builds.

---

## 11. JavaScript emitter

### 11.1 Output shape

- One ES2022 module per Kalam module (`notes/db.kl` → `notes/db.mjs`) plus `.mjs.map`. Module constants are `const`s, evaluated in import order. A module constant can never suspend or panic (§13.4), so top-level `await` is never needed.
- Runtime helpers come from `@kalam/runtime` (shipped with the compiler and vendored into the output, never fetched from a CDN): `$i` (checked integer result), `$floordiv`, `$mod`, `$Panic`, `$Ok`, `$Err`, `$Some`, `$eq`, `$List`, `$Map`, `$box`, `$task`, `$Sql`, `$Html`, the decimal128 class, the boundary validators, and the capability classes.
- Representation follows spec §22: records are generated classes with fixed fields; enums are one class per variant with a numeric `$tag`; `Option` is `undefined` or the value, boxed as `$Some(v)` only where `v` could itself be `undefined`; `Result` is `{ok, value}`/`{ok, error}`; `Map` and `Set` are `Map`/`Set` or the runtime keyed-SipHash map, **never plain objects**.
- **Forbidden output:** `eval`, `new Function`, `with`, string `setTimeout`, computed `import()`, `==`/`!=`, `for…in`, `in` on user-keyed objects, `obj[userKey]` on plain objects, `Object.assign` from user data. CI parses every emitted file of the corpus with a JS parser and fails on any of them (spec §25 "Generated JS").
- **Source maps:** each KIR node keeps the span of the Kalam token it came from. The writer records a mapping at every statement start, call, operator and identifier (with `names`), producing one Source Map v3 file per module. `sourcesContent` is included in dev builds. The map points at the file exactly as written, in its skin. Dev-build panic traces go through the map, so they show Kalam lines. Production builds render no traces (S11).
- **`.d.ts`:** generated only for `@js.export` items, following the §19.4 table.
- **Extern glue:** every `extern` gets a wrapper that validates each value crossing in either direction per §19.2: own data properties only, no getters run, `__proto__`/`constructor`/`prototype` rejected, collections copied and frozen.

### 11.2 How no-async lowers to JS

The effect pass (§9.1) has already decided, for every function and every call site, whether it suspends. The emitter only reads those flags:

| KIR | JS |
|---|---|
| `fn` with `suspends = false` | `function f(…) { … }` |
| `fn` with `suspends = true` | `async function f($t, …) { … }`. The hidden first parameter `$t` is the runtime task context |
| effect-polymorphic `fn`, both instances used | `function f(…)` and `async function f$s($t, …)` |
| `Call { suspends: true }` | `await g($t, …)` |
| `Call { suspends: false }` | `g(…)` |
| call through a function value typed suspending (stored in data) | `await h($t, …)`. Awaiting a plain value works in JS. The microtask cost is open question Q1 |
| `Coerce(SyncToSusp)` | nothing in JS, because `await` accepts non-promises. KIR keeps the node for C |
| lambda whose body suspends | `async ($t, x) => …` |
| `panic(…)` | `throw new $Panic(msg)`. A rejection propagates through every `await` unchanged |

**Why `$t` is explicit.** Cancellation (DR-14) must reach every suspending stdlib call. `AsyncLocalStorage` exists only on Node, so the task context is passed explicitly. That works the same in Node, Bun, Deno and browsers. `$t` holds an `AbortSignal`, the task's scope and its open transaction set. Every suspending stdlib primitive checks `$t.signal.aborted` on entry and on resume, and throws the internal `$Cancelled` unwind. Kalam code cannot catch anything, so neither a panic nor a cancellation can be swallowed. Only runtime boundaries catch them: the `task.scope` join, the HTTP handler wrapper, the live-event loop, the test harness and the entry module. Each one rolls back open DB transactions and releases locks in a `finally`. Sync functions take no `$t`, because they cannot reach a suspension point.

**Structured concurrency.** `task.both`, `task.all` and `task.race` create child tasks with child `AbortController`s and run them with `Promise.allSettled`-style joins. The first panic aborts the siblings, the scope waits for every child, and then the panic is re-raised in the parent. No task outlives its scope, so there is no floating promise anywhere in generated code.

**Atomicity.** JavaScript's run-to-completion between `await`s is exactly the §15.6 guarantee: a task runs uninterrupted between two suspension points. Combined with immutable values, no `var` capture and no module-level `var`, this makes data races impossible on the JS backend without locks.

**Evaluation order.** JS evaluates operands left to right and evaluates `await` in place, so `a + g()` with a suspending `g` evaluates `a` first, as §10.6 requires. KIR's explicit temporaries (§10.2 invariant 3) make this hold even when a future backend reorders.

**Entry point.** The generated `main.mjs` imports the runtime, builds `World` from the manifest's capability list, creates the root task, runs `await main($t, world)` (or a plain call if `main` is sync), maps `Err` to exit code 1 with a redacted message chain, and maps a panic to exit code 101.

**Example** (the spec §15.8 program, with the explicit task context):

```js
export async function load_dashboard($t, app, user_id) {
  const [user, invoices] = await $task.both($t,
    async ($t) => await users.find($t, app.db, user_id),
    async ($t) => await invoices.recent($t, app.db, user_id));
  if (!user.ok) return $Err($box(user.error));          // ? rule 3: boxed into AnyError
  if (!invoices.ok) return $Err($box(invoices.error));
  return $Ok(new Dashboard(user.value, invoices.value));
}
export function names(users) { return $List.map(users, (u) => u.name); }      // sync instance
```

Higher-order stdlib functions ship in both instances in the runtime: `$List.map` and `$List.map$s`. The suspending instance awaits each element **sequentially**, because concurrency exists only through `task.*`.

**Calling Kalam from JS** (`@js.export`): a suspending export gets a wrapper that creates a root task and returns a `Promise`. Its `.d.ts` hides `$t`.

---

## 12. Diagnostics and error-message design

### 12.1 Model

```rust
Diagnostic {
    code: Code,                 // "K0601", "SEC0110", stable forever (05 P6)
    severity: Error | Warning | Hint | Lint,
    primary: FileSpan,
    args: Vec<(name, Arg)>,     // typed: Text, Code(KCF fragment), Type(Ty), Concept, Count, CodePoint
    labels: Vec<Label>,         // secondary spans that tell the story ("defined here as Float")
    notes: Vec<MessageRef>,
    fixes: Vec<Fix>,            // { title, applicability: Machine | Suggestion, edits }
    caused_by: Option<DiagId>,  // follow-on errors are hidden behind their root cause
}
```

Prose is never stored in a diagnostic. It is rendered late from a **catalog**, for three reasons:

1. **Message language and code skin are independent.** The prose follows `KALAM_LANG` (`en`, `ar`, `franco`), while code inside the message (types, keywords, the fix) stays in the **file's** skin (spec §21.4 rule 1). `Arg::Type(Ty::Int)` therefore renders as `Int` in a `.kl` file and as `عدد` in a `.ar.kl` file, whatever the message language. In the scaffold, args are pre-rendered strings and the producer renders code in the file's skin. Typed args replace them in Phase 1.
2. **The JSON output carries all three languages** (`"message": {"en", "ar", "franco"}`), so an agent or an LSP client can choose.
3. **Parity is checkable:** a missing translation fails CI (05 P6, D5).

### 12.2 Catalog

The catalog is a table `code → { title[3], message[3], explain[3] }`. It starts inline in `kalam-diagnostics::catalog` and moves to `catalog/{en,ar,franco}.toml` plus `explain/{code}.{lang}.md` once it grows, compiled in by a build script. Templates use `{name}` placeholders. Style rules, from 01 §3.4 and spec §21.4:

- a plain-language title, then what I found, then why, then the fix;
- first person ("I expected…"), and a positive tone: never "illegal", "invalid", "fatal" or "bad";
- the user's code shown exactly as written, with the span underlined;
- a concrete fix in most errors, and a machine fix where the rewrite is mechanical;
- `Learn more: kalam explain K0601` on every message.

Codes that this architecture needs but spec §27 does not list are marked **proposed** until the spec adopts them. K0129 (unterminated string) and K1504 (a library uses the ambient console without `console = true`) were adopted into spec §27 on 2026-10-24 (spec errata log), so the catalog's `proposed: true` flag on K0129 can be cleared. Parser-error codes (unexpected token, unclosed bracket at end of file, missing block after `:`) are still missing from §27; see `spec/CRITIQUE.md`.

### 12.3 Rendering

- **Human (terminal):** the spec §21.4 template. For `ar` files, the excerpt is printed as written and then again in the English skin with the same span (`--excerpt=ar|en|both`, default `both`), because terminals do not reorder or shape Arabic (spec §6.1 rule 5, 02 §6.4). The LSP always shows full Arabic.
- **JSON:** spec §21.4 exactly. `kalam check --json`, the LSP and `kalam mcp` share one serializer.
- **LSP:** diagnostics with related information, plus code actions for every fix. Machine fixes are also offered as "fix all in file".

### 12.4 Cascades and recovery

- **Root cause first** (rule 2): errors are sorted by phase (source text, lexing, parsing, resolution, types, effects), then by position. A diagnostic produced while checking a poisoned (`Error`) node is dropped. The renderer shows the first root cause in full and summarizes the rest ("2 more errors may be caused by this one"), unless `--all` is passed.
- The parser's recovery and the type checker's `Ty::Error` are what keep the 05 D5 "genuine/reported" ratio high.

### 12.5 Tests

Every code has at least one `.kl` / `.ar.kl` / `.franco.kl` trigger in `conformance/diagnostics/`. Each trigger has a golden `.stderr` for each message language and a golden `.json`. A machine fix is applied and the result must compile (05 D5 "machine-applicable fix rate").

---

## 13. Formatter and skin converter

- **The printer is one function, `print(cst, target_skin, config) -> String`.** `kalam fmt` is "print in the file's own skin with layout normalization". `kalam convert --to X` is "print in skin X without layout changes". `kalam view --skin X [--glossary]` is "print in X with the display glossary applied, to stdout only". `kalam diff --skin X` prints both revisions through the same printer.
- **What the printer translates:** keyword and core-name tokens (concept → target spelling), `,`/`?` ↔ `،`/`؟` (always `،`/`؟` in `ar`), and digit glyphs (`ar` target only, per `[skin.ar] digits`). Everything else is copied byte for byte: identifiers, string and tagged-literal text, comments, blank lines, attribute names and module paths. Code inside interpolation holes is code, and converts.
- **Escaping:** an identifier equal to a reserved word of the target skin is printed in backticks. Unneeded backticks are removed (this is also a `fmt` rule).
- **Formatter rules** (§21.1) are not configurable: 4-space indent, 100 columns, breaking inside brackets with trailing commas, sorted `use`, one blank line between items, canonical Franco and Arabic spellings, removal of redundant parentheses, backticks and `pass`. Tatweel and harakat are removed from keywords. In identifiers they are errors (SEC0106) with a machine fix, so `fmt` applies that fix only when asked (`kalam fix`).
- **Guarantees under test:** G1 (KCF identity across skins), G2 (fixpoint), idempotence (`fmt(fmt(s)) == fmt(s)`) and P2 (pairwise and three-hop round-trips). They run on the corpus and on a grammar fuzzer generating ≥ 10k programs per deep CI run (05 P4).
- **Glossary rename** (`kalam rename --glossary names.toml`) is a refactoring, not a conversion. It changes identifiers, so it produces a new KCF, and it inserts `@name("…")` on `Encode`/`Decode`/`Row` fields to keep wire names stable.
- **Scaffold:** `kalam-fmt::convert` already converts at token level, using the lexer and the skin tables, with backticks and Arabic digits handled. Phase 0 replaces it with the CST printer.

---

## 14. Language server (`kalam lsp`)

- **Transport:** stdio via `tower-lsp`. The server holds one driver session per workspace. Edits update VFS inputs, and queries recompute lazily.
- **Features, in priority order** (05 D7 LSP coverage): diagnostics; hover (type, docs, and "suspends" with its chain); go to definition; completion (keywords and core names **in the file's skin**, from `SkinTable`; stdlib names in English); code actions from machine fixes; semantic tokens (per-token ranges, which the VS Code extension uses for UTS #55 isolation in RTL files); formatting; rename (with skeleton-collision checks); references; inlay hints (`⏸` at suspension points, inferred `let` types).
- **Skin features:** a "Convert file to…" command, glossary display mode (rendered through `kalam view`, never written), H0202 hints, and **paste conversion**: English keywords pasted into an `ar`/`franco` file are converted to the file's skin (spec X1).
- **Positions:** the driver stores byte offsets. `LineIndex` converts to UTF-16 or UTF-8 columns according to the negotiated `positionEncoding`. Arabic text makes this matter.

---

## 15. Package manager (`kalam pkg`)

The spec names `kalam add` and `kalam update` at top level (§13.5) and `kalam pkg api` / `kalam pkg diff` (§13.2). This architecture keeps exactly those spellings and puts the remaining package commands under `kalam pkg`.

| Concern | Design |
|---|---|
| Manifest | `kalam.toml` per spec §13.5: package, `[skin.ar]`, `[capabilities]`, `[dependencies]`, `[npm]` with per-package `caps` |
| Lockfile | `kalam.lock`: KCF hashes for Kalam packages, integrity hashes (SRI sha512) for npm tarballs. CI installs are frozen (`kalam pkg install --frozen`) |
| Resolution | PubGrub (the `pubgrub` crate), so conflicts come with explanations. Semver, with the suspension bit and capability set as part of the API |
| Registry | Phase 2: a static index on GitHub plus tarballs on object storage, each package stored as KCF and rendered in any skin on demand. Later: OIDC trusted publishing, Sigstore signatures, a transparency log (04 P3–P4) |
| npm | Tarballs are fetched directly from the npm registry, integrity-checked and extracted. **No lifecycle script ever runs** (04 P1). Packages that need native builds are refused in v0.1. A 48-hour release-age cooldown applies by default (04 P9) |
| Capabilities | `add`/`update` compute a capability diff over the whole tree (from `.kli` caps sets and `[npm] caps`), and additions need explicit approval (04 P6) |
| API | `kalam pkg api` prints the public API from `.kli`. `kalam pkg diff` classifies changes for semver: a removed export, a changed type, sync → suspending, or a capability addition is major |
| Publish checks | Mandatory namespaces, cross-script typosquat detection over Latin, Arabic and Franco transliterations (04 P7), and `kalam audit` counts shown on the registry page |

---

## 16. Test runner (`kalam test`)

- **Discovery:** `test "name":` blocks in `src/` (with module-private access) and in `test/`. They are compiled only in the test profile and never appear in production builds.
- **Execution:** the driver builds the package in test profile. The runner generates `__kalam_tests__.mjs`, which imports every module and registers each test. Node runs it, with one task per test. Test files are spread across worker threads, and tests run sequentially within a file. Each test gets fresh fake capabilities from `std/test` (`test.clock`, `test.net`, `test.fs`, `test.env`, `Rand.seeded`). There is no real network or filesystem unless a test asks for it explicitly.
- **Assertions:** `assert`, `test.expect_eq` (with structural diffs), `test.expect_panic(fn() => …)`, and snapshot tests (`.snap` files updated with `--update`).
- **Reporting:** human output (in `KALAM_LANG`), `--json` for agents and `kalam mcp run_tests`, and TAP. Failures map back to Kalam lines through source maps.
- **Conformance and parity** are not `kalam test`. They are an `xtask` that runs `conformance/` across skins and backends and checks 05 P1–P5. P5, behavioural identity across the three skins, compares stdout, stderr and exit codes byte for byte.

---

## 17. WASM playground build

- `kalam-wasm` is a `cdylib` built for `wasm32-unknown-unknown` with `wasm-bindgen`. It exports `check(path, source, lang) -> json`, `fmt`, `convert(source, from, to)` and `compile(files) -> {js, map}`. The scaffold has these as plain Rust functions. `wasm-bindgen` attributes are added in Phase 1.
- It contains the frontend, the formatter and the JS emitter, over an in-memory VFS. There is no package manager, no LSP and no OS access. The keyword table is embedded.
- The playground runs the emitted JS in a sandboxed iframe with the vendored runtime, a strict CSP (no `unsafe-eval`, Trusted Types required) and a `World` whose `net`/`fs` are fakes. It has a skin toggle, which renders the same file through the printer (03 §5: "huge for marketing"), and per-token bidi isolation in the editor for `ar`.
- Size budget: track the `.wasm` size in CI. Report 05 has no gate for this, so it is reported only.

---

## 18. MCP server (`kalam mcp`)

stdio MCP server with the spec §21.5 tools: `check` (JSON diagnostics), `fix` (apply machine fixes and return the diff), `fmt`, `convert_skin`, `run_tests`, `doc_search` (over `llms-full.txt` and stdlib docs) and `explain`. Every tool is a thin wrapper over driver entry points, so agent results equal CLI results. `kalam new` writes an `AGENTS.md` that tells agents to write canonical English skin and run `kalam check --json` after every edit (06 §7.3).

---

## 19. CLI commands

One binary, `kalam`. Message language comes from `KALAM_LANG` (default `en`). Exit codes: 0 for success; 1 for user errors (diagnostics, failed tests, or `main` returning `Err`); 2 for usage errors; 101 for a panic in `kalam run`.

| Command | Purpose | Main crates |
|---|---|---|
| `kalam new NAME` | project skeleton with `kalam.toml`, `src/main.kl` and `AGENTS.md` | cli, pkg |
| `kalam check [--json] [--bidi] [--excerpt=ar\|en\|both] [PATH]` | parse, resolve, type and effect check; no output files | driver |
| `kalam build [--target js-node\|js-browser] [--release]` | JS + source maps + `.d.ts` + launcher | driver, emit-js |
| `kalam run [-- ARGS]` | build, then run the launcher | driver, emit-js |
| `kalam test [--json] [--update] [FILTER]` | tests (§16) | test-runner |
| `kalam fmt [--check] [PATH]` | canonical formatting | fmt |
| `kalam convert --to en\|ar\|franco FILE` | lossless skin conversion. It renames the file's skin suffix, and no `use` line anywhere changes | fmt |
| `kalam view --skin S [--glossary] FILE` | read-only rendering | fmt |
| `kalam rename --glossary names.toml` | on-disk identifier rename (new KCF, `@name` inserted) | fmt, resolve |
| `kalam diff --skin S [REV]` | review in any skin, with KCF hashes | fmt, syntax |
| `kalam explain CODE` | long explanation in `KALAM_LANG` | diagnostics |
| `kalam fix [--dry-run]` | apply machine fixes | driver |
| `kalam audit [--sinks]` / `kalam caps [PKG]` | escape hatches, capability sets, OSV, every query/page/command structure | effects, sinks, pkg |
| `kalam bind npm:PKG@VER [--only …]` | generate `extern` modules from `.d.ts` | pkg (TS parser via `oxc` or `swc`, to be chosen) |
| `kalam add PKG` / `kalam update [PKG]` | dependency changes with a capability diff | pkg |
| `kalam pkg install [--frozen] \| api \| diff \| publish \| tree \| why PKG` | the rest of the package manager | pkg |
| `kalam db migrate` / `kalam db check` | migrations; SQL literal checking against the schema | sinks, driver |
| `kalam lsp` | language server | lsp |
| `kalam mcp` | MCP server | mcp |
| `kalam version` | compiler, spec, keyword-table and Unicode versions | cli |

---

## 20. The future C backend seam

**The seam is the `Backend` trait in `kalam-ir`.** It is implemented today by `kalam-emit-js`, and later by `kalam-interp` (planned) and `kalam-emit-c`:

```rust
pub trait Backend {
    fn name(&self) -> &'static str;
    fn targets(&self) -> &'static [Target];
    fn emit(&self, program: &kir::Program, opts: &EmitOptions) -> Result<Vec<Artifact>, BackendError>;
}
```

**What makes the seam real rather than nominal:**

1. **KIR invariants** (§10.2). Every type is explicit, every operation has spec semantics, evaluation order is explicit, suspension points are marked, and nothing JS-specific appears. These are the things a C emitter cannot recover after the fact.
2. **Per-target interop.** `extern` declarations carry per-target attributes: `@js(…)` today, `@c(header:, name:)` later. A module may provide both, and the driver picks the one for the build target. A missing implementation for the target is a compile error, never a runtime failure.
3. **The stdlib is layered** as a portable Kalam core (`std/list`, `std/json`, …) over thin per-target extern modules (`std/_platform/js/*.kl`, later `std/_platform/c/*.kl`). Report 03 §2.2 point 4 and 05 D1 both require behaviour that is identical across targets. The shared conformance suite in `conformance/` enforces it.
4. **A second executor from Phase 1.** `kalam-interp`, a tree-walking KIR interpreter in Rust, is the reference for differential testing against JS (05 D1 "differential-backend mismatches", spec §25 "Integers"). It keeps KIR honest years before C exists, which is the mitigation in 03 §7.1 ("C backend prototype, even a toy, by month 12").

**What the C backend will add** (Phase 3, report 03):

| Concern | Plan |
|---|---|
| Generics | the shared `monomorphize` KIR pass |
| Closures | closure conversion pass (environment records) |
| Memory | ARC/ORC-style reference counting with a cycle collector, or Boehm GC for a first version. GC semantics with no observable finalizers (03 §4). Values are immutable and `Cell` is the only mutable cell, which simplifies cycles |
| Suspension | stackful coroutines (for example `minicoro`) over a libuv event loop. KIR's explicit suspension points and `Coerce(SyncToSusp)` make a later switch to compiled state machines possible without touching the frontend |
| Integers | `int64_t` plus `__builtin_*_overflow`, trapping on overflow, so results equal the JS dual `number`/`bigint` lowering exactly (DR-02, DECISIONS D18) |
| UB | none reachable from safe code (04 L2): checked indexing, guarded shifts and division, zero-initialization, `-fno-strict-aliasing`, sanitizer-fuzzed output, where any sanitizer hit is a compiler security bug (04 §2.3) |
| Toolchain | bundled, pinned `zig cc`; hardening flags on by default (04 P13); `#line` directives for gdb/lldb, with a DAP adapter later |

---

## 21. How the compiler itself is tested

| Layer | What | CI job (05 §4.4) |
|---|---|---|
| Unit | per crate (`cargo test`): keyword-table loading and integrity, lexer rules, VLQ, suspension fixpoint | fast |
| Conformance | `conformance/<area>/` per spec §25: Trojan Source, identifiers, numerals, keyword table, layout, inline `if`, skins and parity, types, integers, patterns, must-use, concurrency, sinks, JS boundary, generated JS, live UI. Each case is a file plus expected diagnostics or output, ×3 skins | fast |
| Parity | P1 (KCF ×3), P2 (pairwise and three-hop round-trips), P3 (fixpoint), P5 (behaviour), P6 (catalog), P7 (table), P8 (one skin per file, K0201), P9 (docs) | fast; P4 (10k generated programs) in deep |
| Diagnostics | golden `.stderr` ×3 message languages + `.json`; machine fixes must compile | fast |
| Differential | KIR interpreter vs. JS output on the corpus and on generated programs | deep |
| Fuzzing | `cargo-fuzz` on the lexer (with Unicode-heavy dictionaries), parser, formatter (idempotence, round-trip) and sink parsers. Crashes become regression tests (04 G2) | continuous |
| Generated-JS scan | parse every emitted file and reject forbidden constructs; run test pages under strict CSP with Trusted Types | fast |
| Performance | compile throughput and incremental rebuild on the corpus; LSP latency | deep |

---

## 22. Scaffold status (this commit)

**What exists under `compiler/`:**

- A Cargo workspace with 20 crates (`members = ["crates/*"]`).
- Real code, written to be checkable by inspection:
  - `kalam-skins`: loads the embedded `keywords.json`, maps each row to `Concept`, builds the per-skin tables (`en` exact, `ar` via `arabic_fold`, `franco` with variants), the `did_you_mean` index, and integrity rules 1–4.
  - `kalam-lexer`: the subset listed in §5.4.
  - `kalam-diagnostics`: an inline catalog (en/ar/franco) for the lexer codes, a human renderer and a JSON renderer.
  - `kalam-fmt`: a token-level skin converter.
  - `kalam-effects`: the suspension fixpoint.
  - `kalam-emit-js`: the Source Map v3 VLQ encoder.
  - `kalam-cli`: `kalam check FILE` (lexing only for now), `kalam convert --to SKIN FILE`, `kalam explain CODE` and `kalam version`. Other commands print their planned phase and exit with code 2.
- Stubs: the other crates hold their core types, documentation and `todo!()` or "not implemented" entry points.
- Unit tests in most crates. The lexer and converter tests use the hello world and FizzBuzz programs of spec §28.1–28.2, copied verbatim into `testdata/skins/` in all three skins. They require identical token-kind sequences across skins and exact conversion between every pair of skins.

**Not verified:** no Rust toolchain (`cargo`/`rustc`) was installed on the authoring machine, so neither `cargo check` nor `cargo test` has been run. The first action on a machine with Rust is `cargo check --workspace && cargo test --workspace`, then fixing whatever fails.

**Phase 0 build order** (exit criterion from report 03: `hello`, `fizzbuzz` and a JSON HTTP endpoint run on Node from all three skins, and skin round-trips pass):

1. `cargo check`/`test` green. Add the `xtask` keyword-table checks (§7.4 rules 1–4) to CI.
2. Lexer completion (§5.4 deferred list), with conformance cases for Trojan Source and numerals.
3. CST, parser and AST for the §28.1–28.4 subset. The CST printer replaces the token converter. Add the KCF hash and the G1/G2/P2/P3 tests.
4. Resolve and typeck for that subset. Golden diagnostics ×3.
5. KIR, the JS emitter and a minimal runtime, enough for hello and FizzBuzz on Node, with source maps.
6. Effects, `std/http` externs and the §26.2 hello-web program, running from all three skins.

---

## 23. Open items for this architecture

| # | Question | Default until decided |
|---|---|---|
| A1 | `rowan` crate or an in-house green tree? | in-house (small, no dependency); revisit if the LSP needs rowan's incremental reparsing |
| A2 | When to adopt `salsa` | Phase 1, when the LSP lands. Phase 0 uses a per-file cache |
| A3 | `$t` task context as an explicit parameter (chosen) vs. `AsyncLocalStorage` | explicit, because it is portable to browsers and Deno. Measure its cost on the corpus |
| A4 | TypeScript parser for `kalam bind` (`oxc` vs. `swc`) | decide at Phase 2; both are Rust |
| A5 | Proposed diagnostic codes K0129 (unterminated string) and K1504 (library console use) | **resolved 2026-10-24:** adopted in spec §27 |
| A6 | Catalog storage format (inline Rust vs. TOML + Markdown) | inline for Phase 0, then TOML/Markdown compiled by `build.rs` |
| A7 | Where the JS runtime source lives and in what language | `compiler/runtime/js`, written in TypeScript, compiled to ES2022, vendored into every build output |
