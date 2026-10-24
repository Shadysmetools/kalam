# Kalam (كلام) Roadmap to 1.0

> **Status:** proposed plan, 2026-10-24. It needs founder approval as part of M0.
> **Binding inputs:** `DECISIONS.md` (D1–D17, locked), `spec/KALAM-SPEC.md` v0.1, research reports 01–06.
> **Scorecard:** exit criteria use the metric names and MVP/β/1.0 thresholds from research/05 §2 (parity gates P1–P10) and §4.2–4.3 (dimensions D1–D8). `eval/SCORECARD.md` (draft, with `eval/scorecard.json`) holds these same thresholds, and every reference below of the form "SC D2: …" points to the same row in both places. Scorecard D1–D8 (dimensions) are not decisions D1–D17. Here, decisions are always written "decision D8".

---

## 0. How to read this plan

### 0.1 Process rules (apply to every milestone)

1. **Decision D17, founder stop.** The build loop runs on its own *inside* a milestone (decision D14: build a slice → score → fix the weakest gate → repeat). It **stops for founder approval at every milestone exit**. A milestone ends with a short exit report: the scorecard run, open risks, and the next milestone's plan.
2. **Frozen eval per milestone** (05 §3). Each milestone is judged on a frozen eval set, `eval-v0.N`. New tests go into the next version. 30% of the corpus (the [H] tasks) stays held out, in a location the fixing agent cannot read.
3. **Gates are binding and the composite is advisory** (05 §3.6). A failed hard gate blocks a milestone even when the weighted score rises. The hard gates are parity P1–P10, 0 "Vulnerable" for SQLi/XSS, Trojan Source 100%, and crash-free fuzzing for that milestone's duration.
4. **Only the founder changes thresholds** (05 §4.2 D3 note). The loop may not recalibrate a threshold. "Checkpoint" targets in M1–M4 are this roadmap's intermediate targets, not scorecard thresholds.
5. **Ratchets** (05 §3.5). Once a metric meets its threshold, it may not regress by more than its noise band in any later milestone.
6. **Security defaults never wait** (04 G4). No feature ships "insecure now, fix later".

### 0.2 Mapping 11 milestones to 3 scorecard tiers

The scorecard has three tiers: **MVP**, **β** and **1.0**. This plan assesses them at three points:

| Scorecard tier | Assessed at | Why there |
|---|---|---|
| **MVP** (composite ≥ 60, every dimension ≥ 55, all hard gates) | **M5 exit** | M5 is the first usable slice: language, stdlib, web framework, and a real product running on it. Research 03 Phase 1 uses the same exit ("the founder's own SaaS runs in production"). |
| **β** (composite ≥ 75, every dimension ≥ 70, human studies) | **Beta** milestone, after M10 | β needs the corpus task 45 (Pong, M9), differential-backend checks (M10), LSP 6/8 (M6), package manager 5/6 (M7) and docs at 90% (M8). |
| **1.0** (composite ≥ 90, every dimension ≥ 85) | **1.0** milestone | — |

The MVP tier needs some metrics whose main milestone comes later. **This plan pulls a thin slice of each forward**, so that the MVP pass at M5 is honest and never relies on "n/a":

| MVP metric (05 §4.2) | MVP threshold | Pulled forward into | Full work in |
|---|---|---|---|
| SC D7 LSP capability coverage | 3/8 | M3: `kalam lsp` gives diagnostics, formatting and hover | M6 |
| SC D7 Editor grammars | EN only | M3: a TextMate grammar for EN, plus a VS Code client stub | M6 |
| SC D7 Package manager features | 2/6 | M4: `kalam.lock` (hashes) and offline mode, for npm deps under `[npm]` | M7 |
| SC D8 Docs completeness ×3 | 50% | M3–M5: tour and stdlib reference written alongside the code | M8 |
| SC D3 metrics and P10 (skin gap) | pass@1 ≥ 50%, gap ≤ 15 pp | M3: the `llm-eval` harness (MultiPL-E style) goes live | ongoing |
| SC D8 Deploy targets | 1 | M5: Linux and Docker | Beta |

### 0.3 Timeline at a glance

Decision D8 sets the horizon: long-term, no rush, **1–2 years to 1.0**. **This plan honestly comes to about 28 months (1.0 around Feb–Mar 2029), a few months past that horizon.** Exit criteria beat the calendar. **Founder decision at M0:** accept about 28 months, or cut scope to land nearer 24. The main lever is moving M10 to 1.x, which brings 1.0 to about Nov 2028 (about 25 months; the floor is set by the 12-month dogfood rule below). Effort is in calendar weeks for the founder working with the AI build loop. Two tracks overlap: M6 runs during M5's second half, and M9 runs during M10. **Each parallel track has its own founder stop** (decision D17). One track reaching its exit does not wait for the other, and it does not start its next milestone without approval.

| # | Milestone | Effort | Target window | Scorecard tier |
|---|---|---|---|---|
| M0 | Spec approved | 2–3 wk | Nov 2026 | — |
| M1 | Lexer, parser, formatter, skin converter | 6–8 wk | Nov 2026 – Jan 2027 | parity and Unicode gates |
| M2 | Type checker (and reference interpreter) | 10–12 wk | Jan – Apr 2027 | checkpoint |
| M3 | JS emitter, plus hello web | 6–8 wk | Apr – Jun 2027 | checkpoint |
| M4 | Stdlib core, plus safe string kinds | 8–10 wk | Jun – Aug 2027 | checkpoint (SC D2 at MVP) |
| M5 | `kalam web`, plus first real SaaS dogfood | 16–20 wk | Aug – Dec 2027 | **MVP gate** |
| M6 | LSP, VS Code, tree-sitter | 8–10 wk | Oct 2027 – Jan 2028 (overlaps M5) | SC D7 to β |
| M7 | Package manager and registry | 10–12 wk | Jan – Apr 2028 | SC D7/D2 supply chain to β |
| M8 | Playground and docs site | 6–8 wk | Mar – May 2028 (overlaps M7) | SC D8 docs to β |
| M9 | Games module | 6–8 wk | May – Jul 2028 | corpus 45/46, frame time |
| M10 | C/WASM backend | 16–24 wk | May – Oct 2028 (overlaps M9) | differential = 0 |
| β | Beta | 8–12 wk | Oct 2028 – Jan 2029 | **β gate** |
| 1.0 | 1.0 | ≥ 8 wk RC | about Feb–Mar 2029, when the gate passes and dogfood has run ≥ 12 months | **1.0 gate** |

**Critical constraint:** research 03 §7 requires that 1.0 ship only after the founder's own products have run on Kalam for **at least 12 months**. The dogfood SaaS therefore has to be in production by about **Nov 2027 (M5)**, or 1.0 slips one month for every month the dogfood launch slips.

### 0.4 Founder decisions placed on the timeline

| Item (spec §29–30) | Must be decided by | Default if not decided |
|---|---|---|
| **F2 / decision D7: Arabic RTL and digits.** Already resolved in spec §6 / DR-01 from reports 02 and 04 | **M0 exit.** The founder records it in `DECISIONS.md` as the locked D7 | Spec §6 stands |
| F6 Persian/Urdu letters; F7 Franco skin name (`franco`, `.franco.kl`); F8 the 19 translated core names | M0 exit | Spec defaults |
| The calendar: accept about 28 months to 1.0, or cut scope (§0.3) | M0 exit | about 28 months |
| P9 scope exception: the normative spec is English-only, with AR/franco summary pages (`docs/DOCS-PLAN.md` §2) | M0 exit | Exception granted |
| F4 ambient console; F5 Arabic wire names | M2 exit (they affect type and capability checks) | Spec defaults |
| **F1 `Int` = 53-bit.** Benchmark i64 on the corpus. Switch only if the cost is ≤ 10% (DR-02) | **M3 exit**, before the JS backend freezes. It cannot change after 1.0 | 53-bit |
| Q7 live-component state across instances | M5 mid-point | Sticky sessions |
| **F3 vocabulary survey** (200+ Egyptian developers, plus a native-speaker review: `walla`/`aw`, `و`/`أو`, `عندما`, `هيكل`, `بصفة`/`ka`, `علني`/`talla3`, `motghayer`) | Launch at M0. **Apply results before Beta.** Keyword changes after 1.0 need an edition | Current `keywords.json` |
| Q14 linguist review of the confusable table and `arabic_fold` | M1 exit | Draft table |
| Q11 mechanized soundness proof of Kalam Core (owner, budget) | Owner named at M2. Proof done before 1.0 | — |
| Choice of dogfood product (06 §8.2: an Arabic-first SMB tool such as invoicing or booking) | M4 exit | Invoicing SaaS |

---

## M0: Spec approved

**Goal:** freeze v0.1 of the language design and set up the measurement system *before* any compiler code exists.

**Deliverables**
- Founder sign-off on `spec/KALAM-SPEC.md` v0.1, with answers to F1–F8 (or acceptance of the defaults) and D7 recorded in `DECISIONS.md` by the founder.
- `eval/SCORECARD.md` and `scorecard.json` schema (05 §4.4), with `eval-v0.1` frozen.
- Corpus scaffolding: `eval/corpus/<tNN-slug>/task.toml` for all 50 tasks (`tier`, `split`, `kind`; layout in `eval/corpus/INDEX.md`), task specs in EN/AR/franco, and the 15 held-out [H] tasks stored outside the repo the loop can see.
- Repo layout: the Cargo workspace of `compiler/ARCHITECTURE.md` §3 (`kalam-lexer`, `kalam-parser`, `kalam-fmt`, `kalam-typeck`, `kalam-emit-js`, `kalam-lsp`, `kalam-cli`, …), `conformance/`, `eval/corpus/`, `eval/seccorpus/`, `docs/`.
- CI skeleton with the `fast`, `deep`, `llm-eval`, `continuous-fuzz` and `milestone` jobs (05 §4.4). Parity gates are wired in, and they fail while nothing exists yet.
- `conformance/keywords/` script implementing spec §7.4 rules 1–4 (already true for v0.1). Rules 5–7 are stubbed until the confusable table and the 2,000-word list exist.
- `SECURITY.md`, a CVE process, and a compiler bug class for "accepts what the spec rejects" (04 G1). RFC process and governance notes (06 §9: Elm-style governance risk).
- F3 survey launched.

**Exit criteria**
- The founder approves (decision D17).
- `keywords.json` passes §7.4 rules 1–4 (31 keywords, 19 core names, 0 conflicts).
- SC D4: keywords ≤ 40 (31), productions ≤ 120 (75), synonyms = 0 hold *by construction*, as CI computes them from `keywords.json` and the EBNF.
- The eval set is frozen and the held-out split is verified inaccessible to the fixer's credentials.

**Risks**
- The spec looks done but has holes that only an implementation reveals. *Mitigation:* treat spec errata as normal during M1–M3. Every erratum gets a conformance test and a dated changelog line.
- Keyword choices are locked too early. *Mitigation:* the survey runs in parallel. Keywords are data in `keywords.json`, so changing one before Beta is cheap.

---

## M1: Lexer, parser, formatter, skin converter

**Goal:** all three skins parse to one lossless CST and KCF, and `kalam fmt` and `kalam convert` are byte-exact.

**Deliverables**
- Source-text checks (§5.1): NFC, no BOM, the bidi and invisible-character table per class (code, comment, string), and SEC01xx diagnostics.
- A skin lexer driven by `keywords.json`: `arabic_fold` for keywords, Franco variants, `did_you_mean`, the digit rules of §6.2 and §5.6, `،`/`؟` aliases, and the indentation and leading-dot continuation rules of §5.8.
- A hand-written LL(1) parser (left-factored at the four points of §8.4), with error recovery. A lossless CST with trivia.
- KCF serialization and SHA-256 identity (§4.4).
- `kalam fmt [--check]` (§21.1 rules), `kalam convert --to en|ar|franco`, and `kalam view --skin`.
- Diagnostics engine: the §21.4 template, JSON output, and EN/AR/franco message catalogs for every K01xx, K02xx, K09xx and SEC code. `kalam explain`.
- Conformance directories for Trojan Source, identifiers, numerals, keyword table, layout and inline-if (§25).
- A random program generator and grammar fuzzer for P4. cargo-fuzz targets for the lexer, parser and formatter.

**Exit criteria**
- **Hard gates:** parity P1, P2, P3 and P4 (≥ 10k generated programs per run) at 100% with 0 failures. P7 0 conflicts. P8 matches policy (K0201 tested).
- SC D2: Trojan Source suite 100% bad rejected and 100% good accepted. Arabic/Franco lexing-hazard suite 100%.
- SC D7: formatter idempotence and parity 100%. Skin-switch command = yes.
- SC D1: fuzz crash-free 24 h (lexer, parser, formatter). Property tests 100%.
- SC D5: error recovery ≥ 0.5. Diagnostic completeness ≥ 70% *for the codes that exist so far*. P6 at 100% for those codes.
- Every corpus program in the 21 MVP-tier tasks parses in all three skins.

**Risks**
- The indentation, leading-dot and inline-`if` rules interact in unexpected ways. *Mitigation:* the §25 layout cases become conformance tests on day one, and the tree-sitter grammar later re-checks the LL(1) claim.
- The Arabic confusable table is incomplete (Q14). *Mitigation:* run the linguist review before exit, and track coverage as a number.
- Terminal display of Arabic excerpts (§6.1, Q12). *Mitigation:* default `--excerpt=both`.

---

## M2: Type checker (and reference interpreter)

**Goal:** the full static semantics of v0.1, plus a target-neutral oracle so that JS semantics cannot leak into the language (03 §7.1).

**Deliverables**
- Name resolution, the module graph, visibility and the one-namespace rule (§13). No-shadowing (K0405) and skeleton collisions (SEC0104).
- Type inference with annotated top-level signatures (§9.9): records, enums, generics, traits with coherence and the orphan rule, `impl T as Trait`, and `Float`'s special equality.
- Exhaustiveness and usefulness checking (§11). Must-use `Result` (K1303). `?` with `From`/`AnyError` (§14).
- Suspension inference over the call graph, effect-polymorphic function parameters and `@sync` (§15).
- Capability checks (§16). Sink types for tagged literals, type side only (§17). `Secret` rules (§18).
- The K09xx LLM-mistake catalog with machine-applicable fixes (§21.3), and the K0420 hallucinated-API guard. `kalam check --json`, `kalam fix`.
- A **reference interpreter** over typed IR. It is the oracle for differential testing (spec §25 "Integers", SC D1 differential metric). In M2 it runs only the prelude (core types, `print`, `panic`, `assert`). The rest of the stdlib exists in M2 only as **signature stubs** (types, capabilities, suspension bits), so programs that use it type-check but do not run until M4.

**Exit criteria (checkpoint)**
- All M1 gates still pass (ratchet).
- SC D1: spec clause coverage ≥ 40% (checkpoint; MVP threshold 60% at M5). Conformance pass rate ≥ 95% on the clauses covered. Fuzz 24 h, now including the type checker.
- SC D5: diagnostic completeness ≥ 70%, golden coverage ≥ 70% ×3 skins, machine-applicable fix rate ≥ 20%, Becker-rubric judge ≥ 3.0.
- All MVP-tier corpus tasks 1–15 type-check in all three skins against the stdlib signature stubs. The prelude-only tasks run on the reference interpreter with the expected output: 1, 2, 5, 11, 12, 13 and 14 (8 too, if it needs no `std/string` float formatting). The [NEG] tasks 48–50 fail with the exact expected codes.
- Founder answers F4 and F5.

**Risks**
- Suspension inference plus higher-order functions is the hardest part of the spec (§15.3, Q1). *Mitigation:* build it as a separate pass with its own conformance directory. Measure microtask cost before adding effect variables.
- Type-error quality is mediocre. *Mitigation:* the SC D5 judge runs every iteration, and the loop picks the weakest diagnostic family.
- Soundness gaps. *Mitigation:* name the owner for Q11 now. Use property tests (well-typed programs don't go wrong on the interpreter) until the proof exists.

---

## M3: JS emitter, plus hello web

**Goal:** Kalam programs run on Node, Bun, Deno and the browser, and the 10-concept hello-web app (§26.2) serves HTTP.

**Deliverables**
- Typed IR → ES2022 ESM, following §22: the `$i` overflow checks, `Option` lowering, `Result` shapes, enum classes, `match` → `switch`, suspension → `async`/`await` with `f`/`f$s` variants, and `AbortSignal` cancellation.
- Runtime: Decimal128, BigInt, the structural `HashMap` with keyed SipHash, `$Panic`, capability objects that only the entry module can create.
- Source Map v3 and `.d.ts` for `@js.export`. `extern` with boundary validation (§19.2). `kalam bind npm:<pkg>` (basic).
- Minimal `std/http` (router, `access:`, `serve`, `Request`/`Response`), enough for `conformance/budget/hello_web.kl`.
- `kalam run`, `kalam build`, `kalam test` (with `test` blocks), `kalam new` (with `AGENTS.md`), and `kalam mcp` (check, fix, fmt, convert_skin, explain).
- **Pulled forward:** `kalam lsp` with diagnostics, formatting and hover (3/8). EN TextMate grammar and VS Code client stub. `llm-eval` harness live (HumanEval and MBPP ported, MultiPL-T approach, n=10, T=0.2, pinned models). Tour chapters 1–5 in three languages.
- Static scanner for emitted JS: no `eval`, `Function(`, `==` or `for…in`. Test pages run under a strict CSP with Trusted Types.
- **i64 benchmark** for founder question F1.

**Exit criteria (checkpoint)**
- **Hard gate:** P5 behavioural identity for every task that runs. Byte-identical output across skins.
- SC D1: differential mismatches between the reference interpreter and JS = 0 on the corpus and 10k generated programs. Integer traps at exactly ±(2⁵³−1) on both.
- SC D6: hello-world cold start ≤ 200 ms, HTTP first response ≤ 1 s, bundle ≤ 30 MB, idle RSS ≤ 100 MB, compile ≤ 5 s / incremental ≤ 2 s.
- SC D4: concepts before hello web ≤ 12 (spec value: 10).
- SC D3, first measurement (not gated yet): pass@1, syntax-valid rate, hallucinated-API rate, and skin gap reported with 95% confidence intervals.
- SC D7: LSP 3/8 in EN.
- F1 decided by the founder from benchmark data. The JS backend lowering of `Int` is then frozen.

**Risks**
- `async` coloring cost (the two-variant `f`/`f$s` code size). *Mitigation:* measure bundle size and microtask overhead on the corpus each iteration.
- JS semantics leak (03 §7.1). *Mitigation:* the reference interpreter is the oracle, and every mismatch is a bug in the emitter, never in the spec.
- LLM pass@1 is structurally low (05 §4.2 note). *Mitigation:* a first number arrives early, so recalibrating the threshold (a founder decision) happens at M3, not at M5.

---

## M4: Stdlib core, plus safe string kinds

**Goal:** the v0.1 module map (§20.1, except the web kit), with injection made a type error end to end.

**Deliverables**
- `std/string`, `list`, `map`, `set`, `option`, `result`, `math`, `decimal`, `bigint`, `json` (with limits), `time` (time zones, Hijri), `log` (secret redaction), `env`, `fs`, `path`, `proc`, `crypto` (vetted primitives, CSPRNG), `task` (structured concurrency, `Cell`), `test` (snapshots, fake capabilities) and `i18n` (en/ar/franco plurals, Arabic-Indic number display).
- Tagged literals compiled to precompiled templates: `sql"…"` (parameterized), `html"…"` (contextual auto-escaping), `cmd"…"` (argv only, `OptionLikeArgument`), `url"…"`, `path"…"` (`PathSegment` holes), `regex"…"` (RE2). User-defined tags via `Template` + `@tag`.
- `unsafe.raw_*`, `.expose()`, `kalam audit` and `kalam caps` (AST-based counts, OSV advisories).
- The inverted OWASP/Juliet security corpus (`seccorpus`, about 150 cases ×3 skins) with an exploit-payload harness.
- **Pulled forward:** `kalam.lock` with integrity hashes for `[npm]` deps, frozen CI installs, offline mode, no install scripts on any fetch path (04 P1), and the 48 h release-age cooldown (P9). That gives package manager 2/6.
- Stdlib reference generated from `##` doc comments, with every example a doctest.

**Exit criteria (checkpoint; SC D2 reaches MVP here)**
- SC D1: the 18 non-web MVP-tier corpus tasks (1–15, 48–50) pass 100% in all three skins, dev and held-out.
- SC D2: secure-by-default rate ≥ 80% with **0 Vulnerable for SQLi/XSS** (hard gate). Good-variant usability ≥ 90%. Escape-hatch density ≤ 2 per KLOC. Stdlib parser fuzzing (JSON, URL, template, regex with ReDoS detection) 24 h clean.
- SC D8: stdlib checklist coverage ≥ 30% (checkpoint; MVP 40% at M5). Doc example pass rate 100%.
- SC D7: package manager 2/6.
- The founder picks the dogfood product.

**Risks**
- Contextual HTML escaping is famously subtle (attributes, URLs, `<script>`, `<style>`). *Mitigation:* port an existing context model (for example Go `html/template`) and run its test suite. The exploit harness runs on every good twin.
- Stdlib surface grows. *Mitigation:* the checklist in SC D8 is the scope, and anything else waits for an RFC.

---

## M5: `kalam web`, plus first real SaaS dogfood (MVP gate)

**Goal:** the Rails-class framework (decision D3, 06 §2.2) is proven by shipping one real Arabic-first SaaS on it (06 §8.2, the Rails/Basecamp pattern).

**Scope note:** spec §20.1 lists OAuth and the Paymob/Fawry/Stripe adapters as "planned for 0.2, not v0.1". **M5 delivers that web-kit 0.2 scope.** The language spec stays v0.1, and the web kit ships as `@kalam/web` 0.2.

**Deliverables**
- Full `std/http` and the §20.2 server defaults: CSRF, cookie flags, CORS closed, CSP and HSTS, limits, SSRF-blocking client.
- `std/web` server-driven UI (decision D11, §20.3): live components (state, `update`, `view`), signed CSRF-bound `k-*` tokens, SSE/WebSocket transport, DOM patching through the `kalam` Trusted Types policy, hypermedia directives, the RTL-first component kit, `dir="auto"` and `<bdi>` isolation (§6.5), layouts, forms bound into declared types, flash messages, and assets with no Node toolchain.
- Routing with required `access:` (deny-by-default).
- `std/sql`: SQLite (default) and Postgres, `kalam db migrate`, `kalam db check` (SQL literals checked against the schema), multi-tenant scoping helpers, and tenant-isolation tests.
- `std/auth`: password hashing (argon2), sessions, password reset, login rate limiting, OAuth (Google, Apple, Microsoft).
- `std/jobs` (DB-backed queue with retries, plus cron), SMTP email with RTL templates, local and S3-compatible file storage.
- **Payments:** one adapter interface with **Paymob, Fawry and Stripe** adapters. Subscriptions, webhooks with signature checks, idempotency keys, and per-provider conformance tests against sandboxes (06 §5.4, §9).
- Auto-generated admin (Django-admin style).
- `kalam deploy`: one Docker image or a single artifact to any VPS (Kamal model). Structured logs, request IDs, a metrics endpoint.
- **Dogfood SaaS** built entirely in Kalam, in production with real users, ideally paying ones.
- Decision on Q7 (multi-instance live state).

**Exit criteria: the MVP scorecard gate (05 §4.3), on frozen `eval-v0.5`**
- **All hard gates:** P1–P10 (P10 skin gap ≤ 15 pp), 0 Vulnerable for SQLi/XSS, Trojan Source 100%, fuzz 24 h crash-free.
- **Composite ≥ 60, every dimension ≥ 55.** Key MVP thresholds:
  - SC D1: spec clause coverage ≥ 60%, conformance ≥ 95%, **100% of the 21 MVP-tier tasks** (1–15, 31–33, 48–50), compiler coverage ≥ 60% line / 45% branch.
  - SC D2: secure-by-default ≥ 80%, good-variant ≥ 90%.
  - SC D3: pass@1 (C1, EN) ≥ 50%, ratio vs. Python ≥ 0.6, self-repair ≥ 40%, syntax-valid ≥ 85%, hallucinated API ≤ 15%, agent success 3/10 apps, context footprint ≤ 20k tokens (spec target: `llms.txt` ≤ 15k).
  - SC D4: keywords ≤ 40, productions ≤ 120, synonyms 0, tokens vs. Python ≤ 1.3×, cyclomatic complexity ≤ 1.2×, concepts ≤ 12.
  - SC D5: completeness ≥ 70%, golden coverage ≥ 70%, fix rate ≥ 20%, judge ≥ 3.0, recovery ≥ 0.5.
  - SC D6: Benchmarks Game ≤ 1× CPython (no slower), TechEmpower JSON/fortunes ≥ 0.5× Node/Express, and the M3 latency, size and RSS limits.
  - SC D7: LSP 3/8, formatter 100%, skin switch yes, package manager 2/6, editor grammars EN.
  - SC D8: stdlib checklist ≥ 40%, docs completeness ×3 ≥ 50%, doctests 100%, deploy targets 1.
- Corpus β web tasks 34–39 pass (checkpoint toward β), and 41 (Todo SaaS) runs end to end with tenant-isolation tests.
- **Dogfood:** the product has been live for **≥ 4 weeks** with real users, no Sev-1 incidents caused by the language or framework, and every bug found has a regression test.

**Risks**
- **No killer app** (06 §9, critical). *Mitigation:* the framework is built *from* the product, and success is counted in users of apps built with it, not stars.
- **Scope of M5 is the largest in the plan.** *Mitigation:* the dogfood product drives priority. Auto-admin and S3 storage may slip to M7 without blocking the MVP gate, but payments and auth may not.
- Payments regulation and API churn. *Mitigation:* an adapter interface, a webhook/idempotency core, and sandbox conformance tests per provider in CI.
- Live-component state does not scale out (Q7). *Mitigation:* sticky sessions for the dogfood. The decision is recorded, and the store-backed option comes later.

---

## M6: LSP, VS Code, tree-sitter

**Goal:** professional-grade editing in all three skins, with correct RTL display (decision D2: professionals first).

**Deliverables**
- `kalam lsp` full: diagnostics, hover, definition, completion, rename, code actions (machine fixes), semantic tokens with UTS #55 per-token isolation for `.ar.kl`, formatting, suspension inlay hints, and glossary display mode (§4.5). Incremental analysis (salsa-style).
- VS Code extension: RTL base direction for Arabic files, a skin-switch command, convert-on-paste (K0201), test explorer, and debugging through JS source maps.
- tree-sitter grammars for all three skins (GitHub, Neovim, Zed, Helix), which also check the LL(1) claim of §8.4.
- `kalam diff --skin` and review views showing the KCF hash. `kalam check --bidi`.
- Full `kalam mcp` toolset (adds `run_tests`, `doc_search`).

**Exit criteria (SC D7 to β)**
- SC D7: LSP capability coverage ≥ 6/8 ×3 skins (target 8/8), completion p95 ≤ 150 ms on a 10k-line project, editor grammars ×3 with **RTL rendering verified by a Playwright screenshot test** of an Arabic file in VS Code web, and DAP "basic" (breakpoint, step, inspect via source maps).
- Every earlier gate still passes (ratchet).

**Risks**
- RTL rendering bugs in VS Code itself, outside our control. *Mitigation:* `kalam view --skin en` as a fallback, upstream bug reports, and a screenshot test that pins the behaviour we depend on.
- LSP latency on large projects. *Mitigation:* incremental queries from the start, and a latency benchmark in the `deep` job.

---

## M7: Package manager and registry

**Goal:** Kalam packages with the supply-chain rules of 04 §5.4 enforced from day one.

**Deliverables**
- `kalam add`, `update`, `publish`, `audit`; semver resolution; workspaces; offline mode; the lockfile with KCF hashes (§13.5).
- **Supply-chain rules (04 P1–P12):**
  - P1: no install-time code on any fetch path (registry, git, path). Build-time codegen is sandboxed with no network.
  - P2: lockfile hashes, frozen CI installs, immutable versions.
  - P3: a public transparency/checksum log (Go sumdb model).
  - P4: publishing only via OIDC trusted publishing, with Sigstore signatures and SLSA provenance. No long-lived tokens.
  - P5: the registry checks that the artifact reproduces from the tagged source.
  - P6: capability manifests, with a capability diff that needs approval for additions.
  - P7: mandatory namespaces and **cross-script typosquat checks** (Latin ↔ Arabic ↔ Franco transliteration, UTS #39 skeletons).
  - P8: namespace-pinned private registries with no public fallback.
  - P9: 48 h release-age cooldown.
  - P10: OSV audit, SBOM export (CycloneDX/SPDX), and `unsafe`/`extern`/raw counters on package pages.
  - P11: 2FA or passkeys for maintainers, and a delay plus notification on ownership transfer.
  - P12: reproducible compiler builds.
- Registry: a static index plus object storage, a website with search, and per-package docs rendered in any skin from KCF (02 §7.2, the "Excel model").

**Exit criteria (SC D7 and SC D2 supply chain to β)**
- SC D7: package manager features ≥ 5/6.
- SC D2: dependency and registry policy, OpenSSF Scorecard ≥ 6 on our repos.
- SC D8: reproducible build = yes (two builds give an identical hash).
- A red-team suite passes: install scripts via registry, git and path deps never run; a forged or unsigned package is rejected; a cross-script typosquat (for example a Franco transliteration of an existing Arabic name) is blocked at publish.

**Risks**
- Running a registry is an operations and trust commitment. *Mitigation:* start with a static index on GitHub plus R2/S3 (03 §5), and make the hosted registry the first part of the commercial hosting offer (decision D9).
- npm remains the larger attack surface. *Mitigation:* the same P1/P2/P9 rules for `[npm]` deps (done in M4), plus a capability declaration per npm package.

---

## M8: Playground and docs site

**Goal:** learn Kalam in the browser, in your own language, and give LLMs the whole language in one context window. The structure is in `docs/DOCS-PLAN.md`.

**Deliverables**
- Playground: the compiler built to WASM (decision D10) with a skin toggle, shareable links, live diagnostics in three message languages, and running the JS output in a sandboxed iframe.
- `kalam.dev` docs site with the tour, the book, the `kalam web` guide, the stdlib reference, the cookbook, the error index (`/errors/Kxxxx`), the spec, and `llms.txt` / `llms-full.txt`, in English, Arabic (RTL) and Franco.
- A doctest runner that compiles every code block in its page's skin (P9).
- Published synthetic training corpus and eval results (06 §7.3).

**Exit criteria (SC D8 docs to β)**
- SC D8: docs completeness ×3 ≥ 90%, doc example pass rate 100%.
- Hard gate P9: every page exists ×3 and its code blocks compile in its own skin.
- P6: every diagnostic code has an `explain` page in all three message languages.
- SC D3: `llms.txt` ≤ 15k tokens (spec §21.2). `llms-full.txt` ≤ 40k. Context footprint ≤ 15k (β).
- The playground runs every tour example in all three skins in Chromium, Firefox and Safari.

**Risks**
- Translation load (three prose versions of everything). *Mitigation:* code blocks are generated by the converter, never translated by hand. Prose uses translation memory and native reviewers, and a missing translation fails the build (P9).
- Docs drift from the compiler. *Mitigation:* doctests in CI, and the error index generated from the diagnostic catalog.

---

## M9: Games module

**Goal:** the education on-ramp (06 §8.2): a moving sprite in five minutes, shareable in one click.

**Deliverables**
- `std/game` on the JS backend (Canvas or PixiJS behind `extern`): `game.run`, `Input`, `Key`, `Picture`, `draw.*`, `Color`, sprites, sound, Arabic text shaping in game text.
- Playground integration with one-click sharing.
- Corpus tasks 45 (Pong) and 46 (Snake or Breakout).
- An Arabic game-making tutorial track for education partners.

**Exit criteria**
- SC D1: corpus tasks 45 (β tier) and 46 pass in all three skins.
- SC D6: game scene p99 frame time ≤ 16.7 ms with 10k sprites (β).
- SC D8: the "2D game loop, input, audio" rows of the stdlib checklist are complete.

**Risks**
- Scope creep into a game engine. *Mitigation:* LÖVE-sized scope only. Native engines (raylib, Godot) come after 1.0.
- `@sync` callback ergonomics in the game loop (§15.4). *Mitigation:* this is corpus task 45's main measurement.

---

## M10: C/WASM backend

**Goal:** the second backend from decision D4. It keeps the semantics honest and enables native binaries and browser WASM.

**Deliverables**
- Typed IR → C11 emitter: `int64_t` plus range checks that match the JS `Int` exactly (DR-02), a GC strategy (Boehm first, with RC/ORC evaluated), bounds-checked everything, and no UB (04 L2).
- A bundled, pinned `zig cc` toolchain for cross-compiling (04 P12), with hardening flags by default (04 P13: FORTIFY=3, stack protector, CFI/CET/BTI, PIE, RELRO, auto-var-init=zero).
- A WASM target (via C or directly), and corpus task 47 (a browser game compiled to WASM).
- Per-target stdlib externs (libuv, SQLite) and DAP debugging via `#line` and DWARF.
- **Differential testing** across the reference interpreter, JS, native and WASM, in CI.

**Exit criteria**
- SC D1: differential-backend mismatches = 0 on the corpus and 10k generated programs per run (β). The 1M-per-week continuous run is started, as it is needed for 1.0.
- Corpus task 47 passes. The whole MVP and β corpus passes on the native backend.
- Sanitizer-backed fuzzing of the C backend (04 G2) is crash-free for 7 days.
- SC D8: deploy targets ≥ 4 (Linux, macOS, Windows, Docker; WASM makes 5).

**Risks**
- **Schedule (highest in the plan).** Research 03 §7 puts the native backend at months 18–36 and a credible public 1.0 at 3–4 years, while decision D8 targets 1–2 years. *If M10 slips by more than 12 weeks*, the fallback is to ship Beta and 1.0 on the JS backend, with M10 following as 1.x. **This changes the 1.0 gate** (corpus task 47 and the differential metric are 1.0 tier), so it is a founder decision at the M9 exit review, never a loop decision.
- GC and performance gaps between backends show up as semantic differences (for example finalization or stack depth). *Mitigation:* the spec has no finalizers, and stack exhaustion is a panic on every backend (§22).

---

## Beta

**Goal:** a feature-complete, frozen-syntax language that external teams can adopt, judged by humans as well as CI.

**Deliverables**
- **F3 survey results applied** to `keywords.json`. This is the last chance to change keywords without an edition.
- Human studies (05 §1.1): a Stefik-style novice RCT (about 20 novices per skin, against a placebo skin) and a Cognitive Dimensions expert walkthrough (PLIERS).
- A published stability policy: editions, semver, and a breaking-change detector on the stdlib API (`kalam pkg diff`).
- An external security review of the compiler output, the web kit and the registry.
- Two reference apps in production with 72 h soak tests. Early-adopter program (outside teams, MENA SMEs, education partners).
- Installers: Homebrew, winget, scoop, Docker image, version manager.

**Exit criteria: the β scorecard gate (05 §4.3), on frozen `eval-v0.β`**
- All hard gates. P10 skin gap ≤ 8 pp. Fuzz crash-free 7 days.
- **Composite ≥ 75, every dimension ≥ 70**, including: corpus 46/50 (MVP + β tiers, 100%), spec coverage ≥ 90%, conformance ≥ 99.5%, secure-by-default ≥ 95%, pass@1 ≥ 70%, agent success 7/10, LSP 6/8, package manager 5/6, docs ≥ 90%, stdlib checklist ≥ 80%, Benchmarks Game ≥ 3× CPython, TechEmpower ≥ 1× Node/Express, OpenSSF ≥ 6, 2 reference apps soak-tested.
- Human gates: the novice RCT is significantly above the placebo in all three skins, and no Cognitive Dimension is rated "severe".

**Risks**
- The survey reveals a keyword the community rejects (for example `walla` = else). *Mitigation:* keywords are data, the converter rewrites code mechanically, and this is the reason the survey happens before Beta.
- Human-study recruitment for Franco and Arabic novices. *Mitigation:* start recruiting through education partners at M8.

---

## 1.0

**Goal:** a production language whose promises (stability, security defaults, skin parity) are frozen.

**Deliverables**
- The first stable edition (the manifest's `edition` key), with syntax, keywords and security defaults frozen (04 G4).
- Q11: a mechanized soundness proof of Kalam Core in Lean or Coq, or a founder-approved decision to ship without it.
- LTS docs and a security response team.
- A commercial hosting offer ("Laravel Cloud for MENA", decision D9) ready, so that the ecosystem has a steward.

**Exit criteria: the 1.0 scorecard gate (05 §4.3), on frozen `eval-v1.0`**
- All hard gates. P10 ≤ 5 pp. Fuzz crash-free 30 days with 0 new crashes. Differential = 0 on 1M generated programs per week.
- **Composite ≥ 90, every dimension ≥ 85**, including: corpus 50/50 with 100% held-out, 100% spec coverage, ≥ 70% "Impossible" in the security corpus, pass@1 ≥ 80%, agent success 10/10, LSP 8/8, package manager 6/6, full DAP, docs 100%, stdlib checklist 100%, 5 deploy targets, stability policy enforced, OpenSSF ≥ 8, 5 reference apps soak-tested.
- Human gates: the novice RCT is at least the Python baseline, and no Cognitive Dimension is rated "poor".
- **The founder's own products have run on Kalam in production for ≥ 12 months** (03 §7).

**Risks**
- The calendar pressure of decision D8 versus the 12-month dogfood rule. *Mitigation:* the rule wins. Decision D8 says "no rush, get it right".

---

## Cross-cutting risk register

| Risk | Severity | Source | Mitigation | Watch metric |
|---|---|---|---|---|
| No killer app; the language goes dormant | Critical | 06 §9 | Build the framework from the dogfood SaaS (M5) | Real users of Kalam-built apps |
| LLMs write Kalam poorly (low-resource penalty) | High | 06 §9, 05 §1.5 | Python-like canonical form, `llms.txt`, MCP, machine fixes, published synthetic corpus | SC D3, measured from M3 |
| Timeline: decision D8 (1–2 years) vs. research 03 (3–4 years) | High | 03 §7 | Overlapping tracks, and the M10 fallback is a founder decision | Milestone dates vs. this plan |
| Three skins fragment docs and community | High | 03 §7.1, 02 §5 | One KCF, converter-generated code blocks, P9 docs parity | P9, P10 |
| Professional MENA devs won't switch | High | 06 §9 | English skin is first-class, npm interop, Paymob/Fawry wedge | Early-adopter count at Beta |
| Unicode or bidi security hole | High | 04 §4.1 | SEC rules, the Trojan Source suite as a hard gate, continuous fuzzing | SC D2 Trojan Source |
| Ecosystem surface too big for a small team | High | 06 §9 | DB as the only infra, one blessed way, a defer list per milestone | Milestone scope cuts |
| Governance failure (Elm-style) | Medium | 01 §2.13, 06 §9 | Public RFCs, never remove escape hatches, stability promise at 1.0 | — |
| Funding and stewardship | High | 06 §9 | Decision D9: open core plus hosting, cloud and education | — |
| The loop overfits the scorecard (Goodhart) | Medium | 05 §3 | Held-out split, frozen evals, no test edits by the fixer, ratchets | Dev vs. held-out gap |
