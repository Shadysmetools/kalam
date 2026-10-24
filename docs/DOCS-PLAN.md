# Kalam Documentation Plan

> **Status:** proposed, 2026-10-24. The site ships in roadmap milestone M8. Writing starts in M3 (see `ROADMAP.md` §0.2).
> **Binding inputs:** spec §4 (skins, KCF), §21 (canonical form, diagnostics, `llms.txt`), §27 (diagnostic index); research 05 P6, P9, P10 and SC D8; research 06 §7.3; research 02 §4–5.
> **Domain:** `kalam.dev`. The spec already uses `https://kalam.dev/errors/Kxxxx` in diagnostic JSON.

---

## 1. Principles

1. **One source, three languages.** Every page exists in English, Arabic and Franco (05 P9, a hard gate). A missing translation fails the build.
2. **Code is generated and prose is translated.** Code blocks are written once in canonical English skin and produced in the other skins by `kalam convert`. They are never hand-translated, so they cannot drift (02 §7.2, the APL anti-pattern). Prose is written by people, with translation memory and native-speaker review, because the converter does not translate prose.
3. **Every code block is a test.** The doctest runner compiles (and where marked, runs) each block **in its page's skin** (P9, SC D8 doc example pass rate 100%).
4. **Prose language and code skin are two separate settings.** A reader can read Arabic prose with English-skin code, or English prose with Arabic-skin code. The default pairs them (EN↔`en`, AR↔`ar`, Franco↔`franco`). The choice is remembered per reader.
5. **Canonical English comes first for LLMs** (spec §21.1). `llms.txt` and `llms-full.txt` teach the formatted English skin only.
6. **Error messages are a primary doc surface.** 71% of Arabic-speaking students named error messages as a major barrier (02 §4). Every diagnostic links to a page in the reader's language.
7. **Plain language.** Short sentences, a positive tone, no "illegal" or "invalid" (spec §21.4 rule 3). Arabic prose is فصحى that reads easily (simple MSA). Franco prose is Egyptian colloquial written in Latin letters with the digit letters of spec §7.3.

---

## 2. Site map

```
kalam.dev/
├── /                       landing: what Kalam is, hello world ×3 skins, try-it button
├── /{lang}/                lang ∈ {en, ar, franco}; ar pages use dir="rtl"
│   ├── install/            install, editor setup, `kalam new`
│   ├── tour/               interactive tour (runs in the playground)
│   ├── book/               "The Kalam Book": the full language guide
│   ├── web/                "kalam web" guide: build and ship a SaaS
│   ├── std/                stdlib reference (generated)
│   ├── cookbook/           task recipes
│   ├── errors/             error index (generated from the diagnostic catalog)
│   ├── guides/             migration guides and topic guides
│   ├── security/           the security model, written for app developers
│   └── community/          RFCs, governance, contributing, code of conduct
├── /play                   playground (WASM compiler, skin toggle, share links)
├── /spec                   the normative spec (English only; see the P9 exception below)
├── /errors/Kxxxx           stable short links → /{reader lang}/errors/Kxxxx
├── /pkg                    package registry (roadmap M7)
├── /llms.txt               compact guide for LLMs (≤ 15k tokens)
└── /llms-full.txt          full guide for LLMs (≤ 40k tokens)
```

**P9 scope exception (founder decision at M0, `ROADMAP.md` §0.4):** the normative spec at `/spec` is English only, because two translated normative texts could disagree. It is outside the docs set that P9 checks. Inside `/{lang}/`, every page, including `/ar/spec-summary` and `/franco/spec-summary`, exists ×3 and is P9-checked. If the founder declines the exception, the spec gets full human translations, with English as the normative text.

**Language selection:** the URL decides (`/ar/...`). The first visit goes by `Accept-Language`, and a visible switcher is on every page. The code-skin toggle lives on each code block and in the page header.

---

## 3. Content sections

### 3.1 Tour (the first hour)

An interactive, playground-embedded sequence of 20–25 short steps. Each step has one concept, one runnable example and one small exercise. It follows the concept budget of spec §26.2, so a reader ships a hello web app within the first 10 concepts.

| Part | Steps |
|---|---|
| Basics | hello world · `let`/`var` · numbers (`Int`, `Float`, `Decimal`) · strings and interpolation · `if` · `for`/`while` · functions |
| Data | records (`type`) · enums and `.Variant` · `match`/`case` · `List`/`Map`/`Set` · `Option` |
| Errors | `Result` and `?` · `panic` for bugs |
| Skins | write in your skin · `kalam convert` · Arabic digits and RTL |
| Web | `main(world: World)` and capabilities · a route with `access:` · `sql"…"` and `html"…"` · a live component |

### 3.2 The Kalam Book

A full guide, written for professional developers (decision D2). Proposed chapters:

1. Getting started (install, `kalam new`, project layout, `AGENTS.md`)
2. Skins: English, Arabic, Franco. KCF, conversion, glossaries (spec §4)
3. Values, types and inference
4. Control flow and pattern matching
5. Functions, closures and local `fn`
6. Records, enums and traits (`impl Type as Trait`)
7. Modules, packages and visibility
8. Errors: `Result`, `?`, `From`, `AnyError`, `panic`
9. Concurrency without `async` (suspension inference, `std/task`, cancellation)
10. Capabilities and authority
11. Safe strings: `sql`, `html`, `cmd`, `url`, `path`, `regex`
12. Secrets, randomness, crypto and decoding
13. Testing (`test` blocks, fake capabilities, snapshots)
14. JavaScript and npm interop (`extern`, `kalam bind`, boundary checks)
15. Tooling: `fmt`, `check --json`, `fix`, LSP, `mcp`
16. Working with AI agents (canonical form, machine fixes, MCP)
17. Appendix: keyword table ×3 skins (generated from `keywords.json`), operator precedence, the excluded-features list (spec §23) with "use instead"

### 3.3 `kalam web` guide

Builds one real app (an invoicing SaaS, mirroring the M5 dogfood) from `kalam new` to `kalam deploy`: routing and access rules, the server-driven UI (live components, `k-*` directives), the RTL component kit, forms, SQLite to Postgres and migrations, auth (passwords, sessions, OAuth), multi-tenancy, jobs and email, **payments with Paymob, Fawry and Stripe**, admin, file storage, deploy, observability.

### 3.4 Stdlib reference

- **Generated** from `##` doc comments by `kalam doc`. Every public item has a description, its signature, the capability it needs, the errors it returns, and at least one runnable example (spec §20).
- Stdlib names are English in every skin (spec §7.2). Descriptions are translated. Examples are skin-rendered.
- Each page shows the module's suspension and capability facts, which are part of the API (spec §13.2).

### 3.5 Cookbook

Short task recipes, one problem per page, grouped as: strings and Unicode (Arabic normalization, Hijri dates, Arabic-Indic number display) · files and CLI · JSON and HTTP clients · databases · web (forms, uploads, pagination, websockets) · auth · payments · jobs · testing · interop (calling an npm package) · games. The 50 corpus tasks (05 §5) are a seed list. Held-out [H] tasks are **never** published as recipes (05 §3).

### 3.6 Error index

- One page per diagnostic code in spec §27 (`K`, `W`, `H`, `L`, `SEC`), **generated from the diagnostic catalog**, the same data `kalam explain Kxxxx` prints.
- Each page has: the message in three languages (P6), why it happens, a wrong example and a fixed example in the page's skin, whether the fix is machine-applicable, and related codes.
- The K09xx "known LLM mistakes" family gets an overview page for agent authors.
- CI fails if a code lacks a page in any language (P6, SC D5 diagnostic completeness).

### 3.7 Guides

- **Coming from…** Python, TypeScript/JS, PHP/Laravel, Ruby/Rails, Rust. Each includes a side-by-side table of the K09xx fixes.
- Arabic and Franco orthography: hamza and alef folding, Franco spelling variants, why digit-leading words are rejected.
- Security model for app developers (deny-by-default routes, typed sinks, capabilities, the supply-chain rules).
- Deploying (VPS, Docker), upgrading editions.

### 3.8 `llms.txt` and `llms-full.txt`

- **`llms.txt`:** the compact guide that already exists at `spec/llms.txt`. **Budget ≤ 15k tokens at MVP and ≤ 12k at 1.0** (spec §21.2). This is stricter than research 05's context-footprint threshold (≤ 20k MVP, ≤ 15k β, ≤ 12k 1.0). The spec's numbers govern, and SC D3 measures both.
- **`llms-full.txt`:** ≤ 40k tokens, generated from the spec and the stdlib reference.
- Both teach the canonical English skin and the "run `kalam check --json`, apply machine fixes" loop. They are versioned with each release, and their token counts are reported in CI.
- Also served through `kalam mcp` `doc_search` (spec §21.5).

---

## 4. The three language versions

| | English (`/en`) | Arabic (`/ar`) | Franco (`/franco`) |
|---|---|---|---|
| Prose | English | Simple فصحى (MSA) | Egyptian colloquial in Latin letters (`3`, `7`, `2`, `5`, `8`) |
| Default code skin | `en` (`.kl`) | `ar` (`.ar.kl`) | `franco` (`.franco.kl`) |
| Page direction | LTR | **RTL** (`dir="rtl"`, mirrored layout, sidebar on the right) | LTR |
| Digits in prose | ASCII | ASCII by default, matching the `[skin.ar] digits` default (spec §6.2) | ASCII |
| Diagnostic message language | `KALAM_LANG=en` | `ar` | `franco` |

**RTL rules for the Arabic site:**
- Code blocks use the skin's base direction: an `ar` block is RTL, and an `en` block inside an Arabic page stays LTR.
- Code blocks are rendered by the first-party highlighter with **UTS #55 per-token isolation** (spec §6.1), never by a generic Markdown renderer. This prevents the display reordering warned about in spec §28.
- Inline code and identifiers inside Arabic prose are wrapped in `<bdi>`.
- The site never contains raw bidi control characters, matching spec §5.1.
- A Playwright screenshot test covers the Arabic pages (same harness as SC D7's RTL editor test).

**Translation workflow:** the English source is written first → the converter generates code blocks for `ar` and `franco` → translators work on prose only, in a translation-memory tool with a glossary of terms from `keywords.json` and the stdlib → a native reviewer signs off → the doctest runner and the P9 checker run in CI. Terminology follows the keyword and core-name choices in `keywords.json`, so if the F3 survey changes a keyword before Beta, the docs follow automatically.

---

## 5. Build and tooling

- **Source:** Markdown with fenced `kalam` blocks in `docs/src/en/`, plus prose-only translation files for `ar` and `franco`. Code blocks are inserted at build time.
- **Generator:** a static site built by `kalam doc site`, so the docs depend only on the toolchain. Search runs per language with Arabic-aware normalization (`arabic_fold`).
- **Code-block annotations:** `run` (execute and compare output), `check` (type-check only), `fail=K0601` (must fail with that code), `skip-skins` (forbidden except on the skins page).
- **CI checks:** P9 page parity, doctest pass rate 100%, error-index completeness (P6), `llms.txt` token budget, link checking, RTL screenshot tests.
- **Versioning:** docs are versioned per release, with an edition switcher after 1.0.

---

## 6. Milestones (aligned with `ROADMAP.md`)

| Roadmap milestone | Documentation deliverable | Target |
|---|---|---|
| M1 | `kalam explain` pages for K01xx, K02xx, K09xx and SEC codes ×3 | P6 100% for existing codes |
| M3 | Tour parts 1–2 ×3, install page, `llms.txt` kept in sync with the compiler | — |
| M4 | Stdlib reference generated, book chapters 1–8 | doctests 100% |
| M5 | `kalam web` guide, tour complete, cookbook core | **SC D8 docs completeness ×3 ≥ 50% (MVP)** |
| M8 | Full site live: book, cookbook, error index, guides, `/play`, `llms-full.txt` | **≥ 90% (β)**, P9 100% |
| M9 | Games tutorial track (Arabic first, for education partners) | — |
| 1.0 | Every page complete in three languages, versioned docs | **100% (1.0)** |
