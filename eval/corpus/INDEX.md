# Kalam test corpus: index (eval-v0.1)

There are **50 tasks**, and every program in them exists in all three skins. Numbering, titles, tiers and held-out flags follow research/05 §5 exactly.

- **Status:** 10 tasks are written (14 cases); 25 dev tasks are still to write; 15 held-out tasks are sealed outside this repository.
- **Scoring:** the corpus feeds `D1.corpus_tier_pass`, parity checks P1/P2/P3/P5, the security corpus (T48–T50) and the D4 ratios. See [`../SCORECARD.md`](../SCORECARD.md).

## 1. Rules

1. **Every case passes in all three skins with identical behaviour** (parity P5). Output text is data: string literals are never translated, so the expected output is the same for every skin.
2. **One canonical source per case.** Each case is written once in the English skin. `ar.kalam` and `franco.kalam` are *converter output* (spec §4.3): only keywords, core names, Arabic `،`/`؟` and digits change, while identifiers, strings and comments stay byte-identical. That is why P1 (identical KCF hash) can hold. The Arabic-identifier versions in spec §28 are *glossary views* (§4.5). They are display-only, and they are deliberately **not** used here.
3. **Expected output is never produced by the compiler under test.** It comes from the task's `ref.py` (a Python program with the same behaviour), or, for negative cases, from the spec's diagnostic index (§27). Changing an `expected.txt` needs reviewer sign-off (LOOP.md §6, rule A3).
4. **Frozen tasks are never edited.** A frozen task stays as it is; a later eval version adds new tasks or cases instead (research/05 §5).
5. **Behaviour the spec does not define is never printed.** Where a task leans on an unstated assumption, `task.toml` names it and §6 records it as a spec gap.

## 2. Layout and staging

```
eval/corpus/<tNN-slug>/
  task.toml              manifest (schema in §4)
  en.kalam               English skin, canonical text
  ar.kalam               Arabic skin (converter output)
  franco.kalam           Franco skin (converter output)
  expected.txt           expectation for the root case (grammar in §3)
  ref.py                 Python reference: D4 baseline and source of the expected stdout
  <case>/                extra cases (good/bad twins); same file names, with their own expected.txt
```

**Staging rule (deviation E13).** Spec DR-18 says the file extension alone declares the skin, so a file named `ar.kalam` declares no skin. For each (case, skin), the runner therefore:
1. creates a fresh temporary project with `kalam.toml` (`name = "@corpus/tNN"`, `edition = "2027"`, `targets = ["js-node"]`, `[skin.ar] digits = "ascii"`);
2. copies the file to `src/main.kl`, `src/main.ar.kl` or `src/main.franco.kl`;
3. runs the case's command from that project root.

There is **one project per skin**, because two skin files for one module would be K0301. The working directory is a fresh empty temp dir, and `Fs` is rooted there.

**File invariants** (checked by G-INT and by the harness):
- UTF-8 and NFC.
- No BOM, LF line endings, final newline.
- 4-space indentation.
- No raw bidi, zero-width or control characters, **except** where a negative case requires them (T50 `main` contains raw U+202E, U+2066 and U+2069 on lines 5 and 7).

## 3. `expected.txt` grammar (`kalam-expect/1`)

The first line is a `#` comment. After it come header lines of the form `key: value`, ending at a section marker or at end of file.

| Key | Values | Meaning |
|---|---|---|
| `task`, `case` | `T01`, `main` | identity |
| `kind` | `run` \| `compile-error` \| `http` | which runner to use |
| `command` | `kalam run` \| `kalam check --json` | what is executed in the staged project |
| `fixtures` | `none` \| text | `empty-cwd …` means a fresh temp dir with `Fs` rooted there |
| `stdin` | `empty` | stdin is closed |
| `exit` | integer \| `nonzero` | required exit status |
| `stderr` | `ignore` | stderr is not compared (it holds logs) |
| `diagnostic` | `code=<K/SEC code> line=<n>[\|<m>] [token=<text>] [codepoints=U+XXXX,…]` | **compile-error:** a diagnostic with this code must have its primary span on that line (or on one of the alternatives). `token`, if given, must lie within the span. Every listed code point must be named in the message. Lines are compared, never columns, because conversion keeps line numbers but changes columns |
| `fix` | `required` \| text | at least one fix must be attached to the matched diagnostic |
| `fix-check` | text | an extra check on applying the fix (it feeds `D5.machine_fix_rate`) |
| `others` | `none` | no *error*-severity diagnostic other than the listed ones (warnings, hints and lints are allowed) |
| `ready` | `tcp 127.0.0.1:8080 within 10s` | **http:** poll until the port accepts, then run the script; terminate the process afterwards |

Section markers:
- `--- stdout` (kind `run`): every byte after the marker line, up to end of file, is the exact expected stdout.
- `--- http` (kind `http`): exchanges separated by blank lines. Request lines start with `> `:
  - `> METHOD /path`
  - `> header Name: value`
  - `> body <raw text to end of line>`

  Expectation lines start with `< `:
  - `< STATUS`
  - `< header name ~ prefix`: case-insensitive prefix match
  - `< header name = value`: exact match
  - `< json <value>`: the body is compared as a parsed JSON value, so key order and whitespace are ignored

  Bodies of error responses are never compared (§6, G11). HTTP cases run one skin at a time on port 8080, never in parallel.

## 4. `task.toml` schema

```toml
id = "T32"                     # report-05 number
title = "..."
tier = "mvp" | "beta" | "1.0"
split = "dev" | "heldout"      # held-out manifests live only in the sealed store
kind = "pos" | "neg"           # neg = research/05 section G; neg tasks may also carry positive twins
section = "A".."G"
features = ["http.router", ...]   # used by LOOP.md slice selection (stage S5)
spec = ["§20.2", ...]
assumptions = ["G1", ...]      # spec gaps from section 6 that this task relies on
files = { en = "en.kalam", ar = "ar.kalam", franco = "franco.kalam" }
reference = "ref.py"
provenance = "..."

[[case]]                       # one per case; a task passes only if every case passes in all skins
name = "main"
path = "."                     # directory relative to the task
expect = "run" | "compile-error" | "http"
```

## 5. The 50 tasks

**Tiers** (research/05 §5):
- **MVP:** tasks 1–15, 31–33 and 48–50 (21 tasks).
- **β:** adds 16–30, 34–40, 42, 43 and 45 (25 tasks; 46 cumulative).
- **1.0:** adds 41, 44, 46 and 47 (4 tasks; 50 in total).

**Split:** 15 tasks are **[H] held-out** (30%): 3 MVP, 11 β and 1 1.0. For those, only the title appears here (§7).

**Status:**
- `written`: the directory is in this repo.
- `to write`: a dev task. A reviewer-gated authoring step must write it before the tier it belongs to is frozen.
- `sealed`: a held-out task.

| # | Task | Tier | Split | Kind | Exercises / feature tags | Status (directory) |
|---|---|---|---|---|---|---|
| **A** | **Fundamentals** | | | | | |
| 1 | Hello world | MVP | dev | pos | I/O, skin keywords, Arabic text in strings | written (`t01-hello`) |
| 2 | FizzBuzz 1..100 | MVP | dev | pos | loops, conditionals, modulo | written (`t02-fizzbuzz`) |
| 3 | Read name, greet (Arabic and Latin input) | MVP | dev | pos | `Stdin`, Unicode strings, RTL output | to write |
| 4 | Sum / average / min / max of a number list (incl. Arabic-Indic digit input) | MVP | dev | pos | lists, numeric parsing, §6.2 runtime digits | to write |
| 5 | Fibonacci, iterative and recursive | MVP | **[H]** | pos | — | sealed |
| 6 | Word frequency count from text | MVP | dev | pos | maps, sorting, string split | to write |
| 7 | Palindrome check (Arabic with diacritics stripped) | MVP | dev | pos | Unicode normalization, `arabic_fold` | to write |
| 8 | Temperature conversion table | MVP | dev | pos | formatting, floats (blocked on spec gap G3) | to write |
| 9 | Guess-the-number (seeded RNG) | MVP | dev | pos | input loop, `Rand.seeded` determinism | to write |
| 10 | Prime sieve to 1e6 | MVP | **[H]** | pos | — | sealed |
| **B** | **Data and types** | | | | | |
| 11 | Define record `User`, validate fields, print | MVP | dev | pos | records, validation, opaque types | to write |
| 12 | Sum types / enum + exhaustive match (shape area) | MVP | dev | pos | ADTs, guards, or-patterns, `.with`, record equality | written (`t12-shapes-match`) |
| 13 | Optional/null handling: safe lookup chain | MVP | **[H]** | pos | — | sealed |
| 14 | Error handling: parse int, propagate errors | MVP | dev | pos | `Result`, `?`, `map_err`, Arabic-Indic runtime parse | written (`t14-result-try`) |
| 15 | JSON parse → typed struct → re-serialize (round-trip) | MVP | dev | pos | `json`, derive, schema-first rejection | written (`t15-json-roundtrip`) |
| 16 | CSV read, group-by, write report | β | **[H]** | pos | — | sealed |
| 17 | Date/time: days between dates, Cairo timezone, Hijri display | β | **[H]** | pos | — | sealed |
| 18 | Generic stack/queue with tests | β | dev | pos | generics, `test` blocks | to write |
| 19 | Immutable vs. mutable update | β | dev | pos | value semantics, `var`, `Cell` | to write |
| 20 | Money arithmetic in decimal (no float rounding bug) | β | dev | pos | `Decimal`, `div(scale:, rounding:)` | to write |
| **C** | **Algorithms** | | | | | |
| 21 | Binary search + quicksort | β | dev | pos | algorithms | to write |
| 22 | BFS shortest path on a grid | β | **[H]** | pos | — | sealed |
| 23 | LRU cache | β | dev | pos | maps + linked structure | to write |
| 24 | Regex-free tokenizer for arithmetic expressions + evaluator | β | **[H]** | pos | — | sealed |
| 25 | Benchmarks Game ports: n-body, binary-trees, fannkuch, spectral-norm, mandelbrot (one task, 5 programs) | β | dev | pos | performance (also D6) | to write |
| **D** | **Concurrency and I/O** | | | | | |
| 26 | Fetch 10 URLs concurrently with timeout (mock server) | β | dev | pos | `task.all`, `task.timeout`, HTTP client | to write |
| 27 | Producer/consumer with channels | β | **[H]** | pos | — | sealed |
| 28 | Rate limiter (token bucket) | β | **[H]** | pos | — | sealed |
| 29 | File watcher → rebuild log | β | dev | pos | fs events | to write |
| 30 | CLI tool with args, flags, help in 3 languages | β | dev | pos | CLI, `std/i18n` | to write |
| **E** | **Web and SaaS** | | | | | |
| 31 | Static website: 3 pages, layout template, auto-escaped variables | MVP | dev | pos | `html"…"`, `std/web` layouts | to write |
| 32 | REST API: CRUD `/todos` in memory, JSON | MVP | dev | pos | router, required `access:`, JSON, `Cell` | written (`t32-todo-api`) |
| 33 | Same API on SQLite with parameterized queries | MVP | dev | pos | `std/sql`, `sql"…"` | to write |
| 34 | Auth: signup/login, hashed passwords (argon2), session cookie (HttpOnly, Secure, SameSite) | β | dev | pos | `std/auth`, `Secret` | to write |
| 35 | Form with CSRF token + server validation, RTL form in Arabic | β | **[H]** | pos | — | sealed |
| 36 | File upload with size/type limits, safe path storage | β | **[H]** | pos | — | sealed |
| 37 | Outbound webhook with SSRF allow-list | β | dev | pos | `Net.only`, SSRF blocking | to write |
| 38 | Pagination + search endpoint (SQL `LIKE` with bound params) | β | dev | pos | `sql.like_contains` | to write |
| 39 | Background job queue + retries | β | **[H]** | pos | — | sealed |
| 40 | TechEmpower suite: json, db, queries, fortunes, updates, plaintext | β | dev | pos | performance + XSS-escaped fortunes | to write |
| **F** | **Full applications** | | | | | |
| 41 | Todo SaaS: multi-tenant, auth, Postgres, tenant isolation tests, HTML UI | 1.0 | dev | pos | end-to-end SaaS, live components | to write |
| 42 | Blog CMS with markdown → sanitized HTML, admin role | β | **[H]** | pos | — | sealed |
| 43 | Real-time chat (WebSocket) with rooms | β | **[H]** | pos | — | sealed |
| 44 | Invoice generator → PDF, Arabic RTL layout | 1.0 | dev | pos | i18n, RTL rendering | to write |
| 45 | Pong game: 2-player, 60 fps, input, collision, score | β | dev | pos | game loop; case `headless` written, case `browser` to write | written, partly (`t45-pong-headless`) |
| 46 | Snake or Breakout with sprites and sound | 1.0 | **[H]** | pos | — | sealed |
| 47 | Browser game compiled to WASM | 1.0 | dev | pos | WASM target (DECISIONS D4: later backend) | to write |
| **G** | **Negative and security** | | | | | |
| 48 | SQL query built by string concatenation with user input | MVP | dev | neg | K1601 with a fix; `good` twin proves the payloads stay data | written (`t48-sqli-concat`) |
| 49 | Render user input into HTML with no escaping; shell command with interpolated input; `../` path join from request | MVP | dev | neg | `bad`: K1601 (HTML), K1641 (path), K1601 (command). `good`: the safe string kinds | written (`t49-safe-string-kinds`) |
| 50 | Trojan Source file (bidi override hiding logic) and homoglyph identifiers | MVP | dev | neg | `main`: SEC0110 ×2; `homoglyph`: SEC0104 ×2; `good`: escaped form + Arabic comment compile | written (`t50-trojan-source`) |

**Count check:**
- Tiers: MVP 21, β 25, 1.0 4, total 50.
- Held-out: 15 (3 + 11 + 1).
- Negative tasks: 3.
- `D3.agent_app_success` uses T31, T32, T33, T34, T38, T41, T42, T45, T46 and T47. A separate eval agent attempts the held-out ones, and only their scores come back.

### The 10 written tasks, case by case

| Task | Case | Expectation | What it proves |
|---|---|---|---|
| T01 | main | stdout, 2 lines | skin keywords; Arabic text inside an English-skin string |
| T02 | main | stdout, 100 lines | `..=`, `else if`, interpolation |
| T12 | main | stdout, 8 lines | exhaustive `match`, guard, or-pattern, `..` rest, default field, `.with`, structural `==`. Every number printed is an `Int` |
| T14 | main | stdout, 9 lines | `Result` + `?` inside a loop, `map_err`, `"٤٢".to_int() == Ok(42)`, out-of-range text is an error |
| T15 | main | stdout, 12 lines | decode → encode → decode equality; rejection of unknown, duplicate, missing, mistyped, fractional and out-of-range fields, and of truncated input |
| T32 | main | HTTP transcript, 14 exchanges | CRUD, 201/204/400/404, mass assignment rejected (`{"done": true}` on create → 400), `nosniff` header |
| T45 | headless | stdout, 6 lines | nested value-semantic updates (`g.ball.x += …`), `while`, module constants; a deterministic game loop |
| T48 | main / good | K1601 at line 12 / stdout, 5 lines | injection is a type error; with `sql"…"`, the payloads `' OR '1'='1` and `'; DROP TABLE` stay data |
| T49 | good / bad | stdout, 10 lines / K1601, K1641, K1601 | `html` escaping, `url` hole encoding, `PathSegment`, RE2 `regex`, `Secret` redaction; plain strings rejected by three sinks |
| T50 | main / homoglyph / good | SEC0110 ×2 / SEC0104 ×2 / stdout, 2 lines | raw bidi controls rejected in strings **and** comments; Latin/Cyrillic and ى/ي skeleton collisions; the escaped form and plain Arabic compile |

## 6. Spec gaps and assumptions surfaced by the corpus

Writing executable expectations exposed places where the spec is silent or contradicts itself. **The loop must not settle these by editing expectations.** They go to the spec owner (design-team questions) or the founder.

| ID | Gap | What the corpus does | Tasks |
|---|---|---|---|
| G1 | **Route patterns vs interpolation.** Spec §28.5, §20.2 examples and `llms.txt` write `.get("/notes/{id}", …)`, but §5.7 makes `{id}` in a plain string an interpolation hole. With no `id` in scope that is K0401 | writes `"/todos/\{id\}"`, which is spec-consistent. The spec should fix its examples or define a route-pattern type | T32 |
| G2 | **Regex literal braces.** §17.6 writes `regex"^[a-z0-9_]{3,20}$"`, where `{3,20}` would be a hole | writes `\{3,20\}` | T49 |
| G3 | `Show` for `Float` (digits, `1.0` vs `1`) is unspecified | no `Float` is ever printed; T08 waits for a decision | all |
| G4 | `Show` for `Bool` is unspecified (P5 implies it is the same in every skin) | no `Bool` is printed directly; labels are chosen with `if` | all |
| G5 | `json.encode` byte format (whitespace, key order) is unspecified | compares decoded values, never encoded text | T15 |
| G6 | HTML entity choice for `"` and `'` in holes is unspecified | hole values use only `&`, `<`, `>` | T49 |
| G7 | Percent-encoding set for `url` holes (space as `%20` or `+`) is unspecified | the hole uses only `&`, `=`, `#` | T49 |
| G8 | `String.to_int` on text outside the `Int` range | assumes `Err` (DR-02: never a silent float) | T14 |
| G9 | `String.to_int` accepts a leading `-` and Arabic-Indic digits (§6.2 states only the digits) | assumes both | T14 |
| G10 | "CSRF on" (§20.2) for cookie-less JSON requests | assumes no CSRF token is needed without a session cookie | T32 |
| G11 | `HttpError` response body format is unspecified | checks status only for errors | T32 |
| G12 | `sql.sqlite(fs, "shop.db")` takes a `String` (§28.5, §28.6), while `Fs` APIs take `Path` (§16.1, §17.6) | follows §28 usage | T48 |
| G13 | Exact homes of `FsError`, `Output` and `ProcError` (`std/fs`, `std/proc`) and the return type of `fs.read_text` | assumes `Result[String, FsError]` and those modules | T49 bad |
| G14 | Do source-text checks report every violating line, or stop at the first? | expects every line (per-character, non-cascading checks) | T50 |
| G15 | Can SEC0104 (name stage) be reported when the same file also fails source-text checks? | keeps homoglyphs in their own case | T50 |
| G16 | A `List[Html]` hole in text context renders items concatenated with no separator ("inserted as is", §17.4) | assumes no separator | T49 |
| G17 | Exit code of `kalam check` on errors is unspecified | expects `nonzero` | negative cases |
| G18 | `Option` fields: is JSON `null` and/or a missing key decoded as `None`? | avoided (T13 will need a decision) | — |
| G19 | Which way round is `Regex` matching? §17.1 lists `.matches` among the *sinks*, which would read `text.matches(re)`; §20.1 gives no signature | assumes `re.matches(text) -> Bool` | T49 good |
| G20 | Does `Html.render()` of a bare `html"…"` (no layout, no fixed `dir`) add `dir` or `<bdi>` around an Arabic hole (§6.5, §17.4)? | assumes it adds nothing | T49 good |

## 7. Held-out policy (summary; the procedure is in LOOP.md §6)

- The [H] tasks' manifests, programs, `ref.py`, fixtures and `expected.txt` live in the **sealed holdout store**, outside this repository. No fixer session may read that store.
- That store is written by a separate *holdout author* session from the titles above and the spec. Their SHA-256 manifest is recorded in `eval/FREEZE.json`.
- The holdout runner receives only a built compiler. It returns **aggregate** results: tier pass counts, parity booleans, and the security and Trojan Source suite counts. Diagnostic text, task names and diffs never come back.
- Until a held-out task exists, it counts as failing. So MVP cannot be passed until T05, T10 and T13 are sealed.

## 8. Provenance and checks already run (bootstrap, 2026-10-24)

- **Sources.** The EN sources were written by hand against spec v0.1. `ar.kalam` and `franco.kalam` came from a bootstrap converter driven by `spec/keywords.json`. Its checks:
  - en→ar→en and en→franco→en are byte-identical;
  - ar→franco and franco→ar agree with the en hub;
  - no English reserved word is left in code position;
  - no backtick escapes were needed.

  The real `kalam convert` must re-verify all of this at stage S2. A native-speaker review of the Arabic and Franco files is still pending (spec F3).
- **Generated from these inputs:**
  - `spec/keywords.json`: SHA-256 `ca6033653bb9ee8e374e55200311ba815c0c1e117d5d5045fd24bad05d50ade7`
  - `spec/KALAM-SPEC.md`: SHA-256 `1c69d6351bd53ac79bc11210ec9444ce02dd44fb555f4df25cb026f53635acbe`

  If the keyword table changes, the `ar.kalam` and `franco.kalam` files are **stale**. They must be regenerated (reviewer-gated) before any parity result counts. A table change is not a parity bug.
- **Expected output.** Every `stdout` expectation was captured from `ref.py` (`python -X utf8`; T48 `good` in an empty temp dir). The T32 HTTP transcript was executed against `t32-todo-api/ref.py` and all 14 exchanges pass.
- **Byte-level checks.**
  - T50 `main` contains the raw bytes `E2 80 AE` (U+202E) and `E2 81 A6` (U+2066) in all three skins.
  - T50 `homoglyph` contains Cyrillic `рау`.
  - No file has a BOM or a CR.
