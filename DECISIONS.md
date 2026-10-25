# Kalam (كلام) — Locked Decisions

Interview with founder, 2026-10-24.

| # | Decision | Choice |
|---|---|---|
| D1 | Name | **Kalam** (كلام) |
| D2 | Primary audience | Professional developers (MENA first) |
| D3 | First domain / killer app | Full-stack web / SaaS (Rails/Laravel-class) |
| D4 | Compile target | Hybrid: JS/TS first (source maps, npm interop); C/WASM backend later for native + games |
| D5 | Typing | Static with inference |
| D6 | Skins | One file = one skin (English / Arabic / Franco); lossless auto-convert between skins via shared AST |
| D7 | Arabic RTL + digits | Deferred to research (02-arabic-and-franco-languages.md) |
| D8 | Timeline | Long-term, no rush — get it right (1–2 yr horizon to 1.0) |
| D9 | License | Open-source core (MIT/Apache-2.0); monetize hosting/cloud/education |
| D10 | Compiler implementation | Rust (single binary: compiler, formatter, LSP, package manager; WASM build for playground) |
| D11 | Web UI model | Server-driven + components (LiveView/HTMX style) |
| D12 | Concurrency | No `async` keyword — sync-looking code, compiler/runtime handles async |
| D13 | Location | `E:\knowcap-dev\6-R and D\new-language` (new git repo, GitHub later) |
| D15 | Blocks | Significant indentation (formatter-enforced) |
| D16 | Errors | Both: `Result` + `?` for expected errors; panic for bugs |
| D17 | Loop autonomy | Autonomous within a milestone; stop for founder approval at each milestone |
| D7 (resolved 2026-10-25) | Arabic RTL + digits | Spec §6 as written: logical order, no bidi controls anywhere, RTL display by tools, ASCII digits default, Arabic-Indic only in Arabic files |
| D18 (F1) | `Int` | **True 64-bit checked** on every backend (overrides spec's 53-bit default; JS uses dual number/bigint repr) |
| D19 (F6) | Persian/Urdu letters | **Fold** ی ک ە ھ ۀ to Arabic equivalents (not reject); lookalike-collision after folding stays a compile error |
| D20 (F4) | `print`/`log` | Ambient (no capability); libraries must declare `console = true` |
| D21 (F8) | Core names | Translate all 19 core type names in every skin |
| D22 (F3) | Vocabulary | Current keyword tables provisional; 200-dev survey + native-speaker review before beta |
| D23 (F5) | Wire names | Arabic identifiers may be JSON keys / DB columns freely (no ASCII lint by default) |
| D24 (F7) | Franco skin | Skin id `franco`, extension `.franco.kl` |
| D25 | Roadmap | Accept ~28 months to 1.0 with C/WASM backend before 1.0 |
| D14 | Dev process | Dynamic workflow + /loop: build slice → evaluate against scorecard → fix weakest → repeat |

## Research inputs
- research/01-english-like-languages.md — controlled English, ~25–35 keywords, no synonyms, Elm-grade errors
- research/03-implementation-strategy.md — JS-first multi-backend, Rust compiler, design int/float split + single null + Result before JS backend
- research/02, 04, 05, 06 — pending
