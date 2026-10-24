# 03 — Implementation Strategy: How to Build the Language

**Date:** 2026-10-24
**Scope:** How production languages were actually built, how long it took, how they solved the library cold-start problem, which architecture to choose, what a "complete" toolchain includes, and which language to write the compiler in.
**Project context:** A founder plus AI coding agents. The target is a general-purpose language for SaaS backends, websites (frontend and backend) and games, with three keyword skins (English, Arabic, Egyptian Franco) over one shared semantics.

> **How to read the sources.** Primary sources (official release notes, project blogs, language docs) are marked with links. Some 2026 facts (the TypeScript 7 GA date, Mojo 1.0, the Qualcomm/Modular deal) come only from trade press, and the report says so. Where a team size could not be sourced, the report says "small core team" or "unknown" instead of guessing.

---

## 0. TL;DR

1. **"Every language is built on C" is not true as stated.** It is roughly true for one layer (operating systems, and many VMs/runtimes are written in C or C++). It is false for the other layers. TypeScript's compiler was written in TypeScript and is now written in Go. Go's compiler has been written in Go since 2015. Rust's compiler is written in Rust and began in OCaml. Gleam's compiler is written in Rust. See section 1.
2. **Languages take years, not months.** The fast cases took 2 to 4 years from first public release to 1.0, and every one of them sat on an existing ecosystem: TypeScript on npm/JS, Kotlin on the JVM, Elixir on BEAM/OTP, Gleam on BEAM and JS. The slow cases build their own ecosystem: Zig is about 10 years in and still pre-1.0, Nim took 11 years, Crystal 10, and V promised 1.0 for December 2019 and is still on 0.5.x in 2026. **Parasitizing an existing ecosystem is the single strongest predictor of reaching production quickly.**
3. **Recommended architecture: E (one frontend, several backends), shipped in stages, starting with A (emit TypeScript/JavaScript).** The JS backend comes first: npm, browser, Node/Bun/Deno and Canvas/WebGL games on day one. A native backend (C emit, Nim/V style, for raylib/SDL games and single-binary servers) comes second, behind a typed IR that has been designed for it from the start. Avoid building your own VM or LLVM backend in years 1 to 2.
4. **Compiler implementation language: Rust**, following the Gleam precedent: one binary holding compiler, formatter, LSP and package manager, plus a WASM build for the browser playground. **Go is a close second**, and Microsoft's TypeScript 7 port is the precedent for it. TypeScript is fine for a throwaway two-week spike, but don't build the production compiler in it. Microsoft just spent 2025–26 moving off TS for performance.
5. **Decide the semantics before the backends exist,** or the JS-first choice will lock in JS quirks. The decisions are: integer versus float types, the memory model (GC or reference counting), the async model, null handling, and the lexing rules for the Franco skin's digit-letters (`3`=ع, `7`=ح, `2`=ء). See section 4.

---

## 1. Fact check: "every language like TypeScript is built on C"

### 1.1 What "built on" can mean: four separate layers

| Layer | Question it answers | Example |
|---|---|---|
| **1. Implementation language** | What is the *compiler/interpreter* written in? | TypeScript's `tsc` was written in TypeScript; TS 7 is written in Go |
| **2. Compile target** | What does the compiler *emit*? | TypeScript emits JavaScript; Nim emits C; Rust emits LLVM IR; Gleam emits Erlang and JS; Kotlin emits JVM bytecode |
| **3. Runtime / VM** | What *executes* the emitted program? | JS runs on V8 (C++); Erlang/Gleam/Elixir run on BEAM (C); Python runs on CPython (C); Lua runs on its own VM (C) |
| **4. Bootstrap** | What built the *first* compiler, before it could compile itself? | rustc first in OCaml; Go first in C (auto-translated to Go in 1.5); Crystal first in Ruby; Zig first in C++, now bootstrapped from a WASM blob |

The founder's intuition is **partly right at layer 3 and below.** Most production runtimes and VMs (V8, CPython, BEAM, Lua, the JVM's HotSpot) and almost all operating systems are written in C or C++. So nearly every program eventually rests on C/C++ code underneath. That is not the same as the *language* being "built on C." A language is its compiler (layer 1) plus its target (layer 2), and those vary widely.

### 1.2 Corrected facts per language

| Language | Layer 1: compiler written in | Layer 2: target | Layer 3: runtime | Verdict on "built on C" | Source |
|---|---|---|---|---|---|
| **TypeScript ≤ 6** | TypeScript (self-hosted, runs on Node) | JavaScript | V8/JSC/SpiderMonkey (C++) | No. Only the JS engine underneath is C++ | [MS dev blog, "A 10x Faster TypeScript"](https://devblogs.microsoft.com/typescript/typescript-native-port/) |
| **TypeScript 7 ("Corsa")** | **Go** (native port, structure preserved) | JavaScript | same | No | Announced Mar 2025; RC in [VSM Jun 2026](https://visualstudiomagazine.com/articles/2026/06/22/typescript-7-0-rc-moves-microsofts-go-rewrite-into-the-mainline-compiler.aspx); GA reported by [VSM Jul 8 2026](https://visualstudiomagazine.com/articles/2026/07/08/typescript-7-arrives-to-rock-vs-code-with-go-powered-speed.aspx) (InfoQ's coverage is [dated Aug 2026](https://infoq.com/news/2026/08/typescript-7-released/)); MS says TS 6.0 was the last JS-based version ([VSM Dec 2025](https://visualstudiomagazine.com/articles/2025/12/02/microsoft-gets-real-on-native-typescript-remake.aspx)); claimed 8–12x faster full builds |
| **Python (CPython)** | **C** | CPython bytecode | CPython VM (C) | **Yes.** This is the case the founder has in mind (PyPy is written in RPython) | [CPython devguide](https://devguide.python.org/) |
| **Rust** | Rust (self-hosted since ~2011); first compiler was ~38k lines of **OCaml** (2006–2011) | **LLVM IR** (LLVM is C++), with Cranelift and GCC backends in progress | native, no VM | No. It uses LLVM (C++) as a *library* for codegen | [Wikipedia: Rust](https://en.wikipedia.org/wiki/Rust_(programming_language)); [rustc-dev-guide bootstrapping](https://rust.googlesource.com/rust-lang/rustc-dev-guide/+show/e60f2f03f8cf8babb3de043098b336d123e4be21/src/building/bootstrapping/intro.md) |
| **Go** | **Go** since 1.5 (2015). The C compiler was *auto-translated* to Go | own codegen to machine code (no LLVM) | Go runtime (Go + asm) | No, since 2015 | [Go 1.5 release notes](https://go.dev/doc/go1.5): "the compiler and runtime are now implemented in Go and assembler, without C" |
| **Gleam** | **Rust** | Erlang source (→ BEAM) and JavaScript | BEAM (C) or JS engine | No | [Gleam v1 announcement](https://gleam.run/news/gleam-version-1/) |
| **Nim** | Nim (self-hosted) | **C**, C++, Obj-C, JS | native | It *emits* C (layer 2), but it is not written in C | [InfoWorld: Nim 1.0](https://www.infoworld.com/article/3440522/nim-language-reaches-10-status.html) |
| **Zig** | Zig (self-hosted since 0.10, 2022; earlier C++) | LLVM *and* its own x86_64 backend (default for Debug builds since 0.15) | native | No. It aims to remove its LLVM dependency | [Zig 0.15.1 release notes](https://ziglang.org/download/0.15.1/release-notes.html) |
| **Lua** | **C** (portable ANSI C) | own bytecode | own register VM (C) | **Yes** | [The Evolution of Lua (HOPL III)](https://www.lua.org/doc/hopl.pdf) |

**The right way to say it to the team:** *"Most runtimes and operating systems are written in C/C++, so C sits at the bottom of nearly every stack. A language, though, is defined by its compiler and its target. Those can be written in anything (Go, Rust, TypeScript, OCaml) and can target anything (JS, C, LLVM, BEAM, JVM, WASM). We get to pick both."*

---

## 2. Case studies: how successful newer languages were built

### 2.1 Master table

"Years to 1.0" is counted from first public release (or first commit, where noted) to the 1.0 stable release.

| Language | Compiler written in | Backend / target | Start → 1.0 | Years | Team (at 1.0) | Ecosystem cold-start solution |
|---|---|---|---|---|---|---|
| **TypeScript** | TS → Go (TS 7, 2026) | JavaScript (source-to-source) | 0.8 Oct 2012 → 1.0 Apr 2014 | ~1.5 public (≈3.5 from internal start in 2010) | Microsoft team led by Anders Hejlsberg | **Superset of JS.** All of npm on day one; `.d.ts` typings ([DefinitelyTyped](https://github.com/DefinitelyTyped/DefinitelyTyped)) |
| **Kotlin** | Kotlin/Java | JVM bytecode; later JS, Native (LLVM), Wasm | started 2010 → 1.0 Feb 15 2016 | ~6 (4.5 from public reveal Jul 2011) | JetBrains (dozens) | **100% Java interop.** All Maven libraries; Android adoption ([media kit](https://kotlinlang.org/assets/kotlin-media-kit.pdf), [InfoQ](https://www.infoq.com/news/2016/02/jetbrains-kotlin-1)) |
| **Elixir** | Elixir + Erlang | Erlang abstract format → BEAM | first commit Jan 9 2011 → 1.0 Sep 18 2014 | ~3.7 | José Valim + small core (Plataformatec) | **Seamless Erlang/OTP interop.** 25+ years of OTP libraries ([1.0 post](https://elixir-lang.org/blog/2014/09/18/elixir-v1-0-0-released/)) |
| **Gleam** | Rust | Erlang + JavaScript | ~2016 → 1.0 Mar 4 2024 | ~8 (JS target added v0.16) | Louis Pilfold + small core, sponsor-funded | **BEAM FFI + JS FFI.** Calls any Erlang/Elixir/JS function via `@external` ([v1 post](https://gleam.run/news/gleam-version-1/), [InfoQ](https://www.infoq.com/news/2024/03/gleam-erlang-virtual-machine-1-0/)) |
| **Swift** | C++ (increasingly Swift) | LLVM | Jul 2010 → 1.0 Sep 2014 | ~4 | Apple (Chris Lattner + team) | **Obj-C/C interop.** Every Apple framework on day one; Apple mandated it ([Wikipedia](https://en.wikipedia.org/wiki/Swift_(programming_language))) |
| **Dart / Flutter** | C++ VM; Dart (dart2js, analyzer) | own VM/JIT, AOT native, JS (dart2js), Wasm | announced 2011 → Dart 1.0 Nov 2013; Dart 2 (sound types) Aug 2018; Flutter 1.0 Dec 2018 | 2 to Dart 1.0; ~7 to real traction | Google | Dart 1 failed to get adoption as a "JS replacement." **Flutter (a killer app) rescued it** ([Dart 2 type system](https://dart.cn/blog/dart-gets-a-type-system), [Android Police](https://www.androidpolice.com/2018/08/07/dart-googles-home-grown-programming-language-reaches-version-2-0/)) |
| **Lua** | ANSI C | own bytecode VM | 1993 → still evolving (5.4) | n/a | 3 people at PUC-Rio | **Embeddability.** A tiny C library that games and apps embed; host apps provide the libraries ([HOPL paper](https://www.lua.org/doc/hopl.pdf)) |
| **Haxe** | OCaml | JS, C++, JVM, Lua, PHP, Python, HashLink, Neko, Flash | 2005 → 3.0 2013 (versioned from 1.0 in 2006) | n/a | Nicolas Cannasse + Haxe Foundation (small) | **Transpile into many hosts' ecosystems** with externs per target. Games: *Dead Cells* (Haxe + Heaps) ([Haxe repo](https://github.com/HaxeFoundation/haxe), [MCV](https://mcvuk.com/development-news/when-we-made-dead-cells/)) |
| **Nim** | Nim | **C**/C++/Obj-C/JS | 2008 → 1.0 Sep 2019 (2.0 Aug 2023) | ~11 | Andreas Rumpf + small core | **C/C++ FFI via C emission.** Any C header is usable; `c2nim`; JS backend ([InfoWorld](https://www.infoworld.com/article/3440522/nim-language-reaches-10-status.html)) |
| **Crystal** | Crystal (first compiler in Ruby) | LLVM | 2011 → 1.0 Mar 22 2021 | ~10 | Manas.tech + small core | **C bindings** (`lib` declarations); Ruby-like syntax to attract Rubyists ([SD Times](https://sdtimes.com/softwaredev/crystal-programming-language-reaches-1-0/), [InfoQ](https://www.infoq.com/news/2021/04/crystal-ruby-c-release-1)) |
| **Zig** | Zig (was C++) | LLVM + own x86_64/aarch64 backends + C backend | 2016 → **no 1.0** (0.15.x in 2026) | 10+ and counting | Andrew Kelley + ZSF-funded small paid team | **"Zig is also a C compiler."** `@cImport`, `zig cc` drop-in; builds C projects ([0.15.1 notes](https://ziglang.org/download/0.15.1/release-notes.html)) |
| **V** | V | **C** (main), JS, native experiments | public 2019 → **no 1.0** (0.5.x 2026; 1.0 was promised for Dec 2019) | 7 and counting | Alexander Medvednikov + community | C interop; C→V translator ([0.5.1 release](https://newreleases.io/project/github/vlang/v/release/0.5.1)) |
| **Odin** | C++ | LLVM (plus custom backend work to cut compile time) | 2016 → **no 1.0** (dev-YYYY-MM releases) | 10 and counting | Ginger Bill + JangaFX-backed | Built-in C FFI plus big vendored bindings (raylib, SDL, Vulkan) in `vendor:`. Production: JangaFX's *EmberGen* ([Wikipedia](https://en.wikipedia.org/wiki/Odin_(programming_language)), [backend blog](https://odin.handmade.network/blog/p/2210-working_on_a_custom_backend_for_odin)) |
| **Roc** | Rust → **rewritten in Zig** (2025–26) | LLVM + dev backends + WASM | ~2019 → **no 0.1** | 7 and counting | Richard Feldman + small core | Platform/host model: the host (Rust/Zig/C) supplies I/O. The rewrite was for compile speed and arena-allocator architecture ([Weekly Rust](https://weeklyrust.substack.com/p/why-roc-is-moving-away-from-rust)) |
| **Mojo** | C++ on MLIR | MLIR → LLVM / GPU | announced May 2023 → 1.0 reported Aug 2026 | ~3 | Modular (Chris Lattner); Modular reportedly acquired by Qualcomm Jul 2026 (trade press) | **Python superset direction + CPython interop.** Imports Python modules. Compiler open-sourced Aug 2026 per reports ([Wikipedia](https://en.wikipedia.org/wiki/Mojo_(programming_language)), [HWBusters](https://hwbusters.com/news/qualcomm-open-sourced-the-mojo-compiler-three-weeks-after-paying-3-9-billion-for-it/)) |
| **Jai** | C++ | own fast x64 backend + LLVM | announced 2014 → **closed beta** (0.2.x, Feb 2026) | 12 and counting | Jonathan Blow / Thekla (tiny) | C FFI; no public ecosystem; built for one studio's games ([Wikipedia](https://en.wikipedia.org/wiki/Jai_(programming_language))) |
| **Carbon** | C++ | LLVM | announced Jul 2022 → **no 0.1** (0.1 slipped from 2022 to 2025+) | 4 and counting | Google-led | **Bidirectional C++ interop is the whole point** ([2024 roadmap](https://docs.carbon-lang.dev/proposals/p3564.html), [2021 roadmap](https://docs.carbon-lang.dev/proposals/p0253.html)) |

### 2.2 Patterns that matter for this project

1. **Host on someone else's ecosystem.** TypeScript (npm), Kotlin (JVM), Elixir (OTP), Gleam (BEAM + npm), Swift (Apple frameworks) and Mojo (Python) all reached production fastest because users could ship real products on day one with *existing* libraries. Zig, Nim, Crystal, V, Odin and Jai built their own ecosystems through C FFI, and all of them took a decade or are still pre-1.0.
2. **A killer app or a mandate beats language quality.** Swift had Apple, Kotlin had Android (Google made it official in 2017), Dart had Flutter, Elixir had Phoenix, and Odin has EmberGen. Plan a flagship product built in the language from month one: the founder's own SaaS.
3. **One-binary toolchains won developer experience.** Gleam ships compiler, build tool, package manager, formatter and LSP in a single Rust binary ([Wikipedia: Gleam](https://en.wikipedia.org/wiki/Gleam_(programming_language))). Go and Zig do the same. That is the model to copy.
4. **Multi-target is proven but costs effort.** Haxe (9+ targets), Nim (C/JS) and Gleam (Erlang/JS) show that one frontend with several backends works. The price is that stdlib behaviour must be identical across targets (Gleam limits this with per-target `@external` implementations).
5. **Compiler rewrites happen, and they cost years.** Go (C→Go via auto-translation), TypeScript (TS→Go, about 18 months), Roc (Rust→Zig, ~300k LoC, reported ~487 days), Zig (C++→Zig). Pick the compiler's language deliberately now (see section 6).
6. **Promised 1.0 dates slip.** V (Dec 2019 → still 0.5), Carbon (0.1 in 2022 → still pending), Jai (closed beta 12 years). Commit to *production-usable for our own product* milestones, not to a public "1.0" date.

---

## 3. Architecture options

### 3.1 Comparison

Effort is for a founder plus AI agents, to reach "can ship a real product."

| Option | What it is | Pros | Cons | Effort to production-usable |
|---|---|---|---|---|
| **A. Transpile to TS/JS** | Emit readable JS (or TS) + source maps + `.d.ts` | **Instant npm** (2M+ packages); browser + Node/Bun/Deno + edge (Cloudflare/Vercel); browser games (Canvas, WebGL, PixiJS, Phaser, three.js); WASM interop via JS; debugging through source maps in Chrome/VS Code; trivially deployable; AI agents know JS deeply | Inherits JS numeric model unless you add `BigInt`/int emulation; perf ceiling = V8 (fine for SaaS, limiting for AAA games); no true threads (workers only); can't produce small native binaries | **Lowest: 6–12 months** to "ship our own SaaS in it" |
| **B. Compile to C** | Emit C, invoke system C compiler (clang/gcc/zig cc/tcc) | Portability (every platform has a C compiler); native perf; direct use of C libs (raylib, SDL3, SQLite, libuv, OpenSSL); consoles/mobile possible; Nim/V precedent; `zig cc` gives easy cross-compiling | Need your own GC or ARC/ORC (Nim) or Boehm; C undefined behaviour leaks through; debugging maps back to generated C (`#line` directives help); no web story without Emscripten; must *build* a server/HTTP/DB ecosystem | **Medium-high: 18–36 months** for an ecosystem good enough for SaaS; games earlier via raylib |
| **C. LLVM backend** | Emit LLVM IR | Best native codegen, debug info (DWARF), WASM target, many architectures | Large heavy dependency (C++, slow builds; Odin reports ~90% of compile time in LLVM); version churn; still need GC/runtime and own ecosystem; hardest for AI agents to get right | **High: 24–48 months**. Wrong first move |
| **D. Own bytecode VM** | Compile to bytecode for your own interpreter (Lua/Python style) | Full control; REPL and hot reload are easy; embeddable in games (Lua's niche); simple to start | You own perf work forever (JIT = years); **zero ecosystem**; must write HTTP/TLS/DB drivers or bind C; a VM in C/Rust is another big codebase | **High: 24–48 months** to SaaS-ready; good only as an embedded scripting niche |
| **E. Hybrid: one frontend → many backends** | Parser → typed AST → **typed IR** → {JS, C, (later WASM)} | Each domain gets its best runtime: JS for web/SaaS, C for native games/CLIs; Haxe/Nim/Gleam/Kotlin precedent; frontend, LSP, formatter and skins shared by all | Must keep semantics target-neutral; stdlib split into portable core + per-target `extern` modules; testing matrix grows | **= A's effort first**, then +12–18 months per extra backend |

### 3.2 Recommendation

**Choose E, executed as A-first.**

- **For "real production products fast": A (JS/TS emit).** A SaaS backend on Node/Bun with Postgres via existing npm drivers, a website frontend via existing frameworks or a thin native component layer, and 2D/3D browser games via PixiJS/Phaser/three.js all become possible within the first year. This is how TypeScript, Gleam's JS target, ReScript, Elm and Kotlin/JS reached usefulness.
- **Long term: add a C backend** (not LLVM, and not your own VM) for native games (raylib/SDL3), single-binary CLIs and high-performance services. C emission is the cheapest native path with the biggest library reach. Nim has run it in production for 15+ years. `zig cc` can be the bundled C toolchain for cross-compilation.
- **WASM:** reach it first by running the JS output alongside existing WASM modules. Later, consider a direct WASM-GC backend (Kotlin/Wasm and Dart already target WASM-GC), or compile the C output with clang/Emscripten for browser games that need native speed.
- **Don't**: start with LLVM, build a VM, or design semantics that only work in JS.

### 3.3 Why not "C first" (the founder's instinct)?

C emission gives native speed and C libraries. It gives **no** HTTP framework, ORM, auth, payments, email, cloud SDKs, React-style UI or deploy story, and those make up 90% of a SaaS product. Nim and V emit C and remain niche for web after 7 to 17 years, largely because the web ecosystem has to be rebuilt by hand. JS-first gets Stripe, Supabase, Prisma/Drizzle, AWS/GCP SDKs, auth libraries and UI on day one.

---

## 4. Semantics decisions that must be locked before backend #1

If you skip these, the JS backend quietly defines the language and the C backend becomes impossible later. This is the Dart 1 lesson: its unsound optional types had to be replaced in Dart 2 ([Dart: sound types](https://dart.cn/blog/dart-gets-a-type-system)).

| Decision | Recommendation | Why / how each backend handles it |
|---|---|---|
| **Number types** | Distinct `Int` (64-bit, or arbitrary precision) and `Float` (f64). Never "just number" | JS: `Int` → JS number if provably 53-bit-safe, else `BigInt`, or a 32-bit int fast path. C: `int64_t`/`double`. Gleam shows the pitfall: Int semantics differ on its JS target |
| **Null / absence** | `Option[T]` / `T?` with **one** absent value. No null/undefined split | Maps to `undefined` in JS and a tagged/nullable pointer in C |
| **Errors** | `Result[T, E]` + `?` propagation; panics for bugs | Avoids exception-semantics mismatch across targets |
| **Memory** | Automatic memory: **GC semantics, no user-visible finalizer ordering** | JS: host GC. C: Nim-style ARC/ORC (reference counting + cycle collector) or Boehm GC for v1 of the C backend |
| **Concurrency** | `async/await` over a portable event-loop abstraction; actors/channels later | JS: native promises. C: libuv or an io_uring/IOCP loop. Don't expose shared-memory threads in v1 |
| **Strings** | Immutable UTF-8 semantics with grapheme-aware APIs (Arabic needs this) | JS strings are UTF-16, so the stdlib hides that; C uses UTF-8 natively |
| **Interop** | Explicit `extern` declarations per target (Gleam's `@external(javascript, ...)` / `@external(c, ...)`) | Keeps the core portable while still allowing npm and C libs |
| **Modules/packages** | One module system; packages declare which targets they support | As in Gleam/Haxe |

### 4.1 The three keyword skins: compiler implications

- **One canonical AST and IR, with skins only at the lexer/printer layer.** Each skin is a keyword table plus (optionally) punctuation preferences. The parser consumes skin-neutral tokens.
- **Formatter = skin translator.** `fmt --skin=ar file.xx` re-prints the canonical AST in another skin. Round-trip tests (`en → ar → franco → en` yields identical AST) belong in CI from day one.
- **Franco digit-letters (`3`=ع, `7`=ح, `2`=ء, `5`=خ, `8`=ق) collide with numeric-literal lexing.** Decide a rule now. Proposed rule:
  1. Take the maximal run of `[0-9A-Za-z_]`.
  2. If it fully matches the number grammar (`123`, `1_000`, `0x1F`, `1.5e3`), it is a number.
  3. Otherwise, if it is in the active skin's keyword table, it is a keyword.
  4. Otherwise, if it contains at least one letter, it is an identifier (so `3ala`, `7aga` are legal).
  5. Franco keyword spellings must never be valid numbers (ban forms like `2e5`). Enforce this with a unit test over the keyword table.
- **Arabic skin:** accept Arabic-Indic digits (٠–٩) and the Arabic decimal separator (٫) as numeric literals. Allow Arabic letters in identifiers (Unicode XID per [UAX #31](https://unicode.org/reports/tr31/)). Watch for bidi-control characters, which are a security issue ("Trojan Source"), so the lexer should reject unbalanced bidi controls.
- **Skin-aware tooling:** compiler diagnostics in the file's skin language; LSP completion offers the file's skin keywords; tree-sitter needs either one grammar per skin or one grammar with keyword aliases (generate all three from a single keyword table); source maps are unaffected because they map to positions.
- **Skin detection:** file extension or a header pragma (e.g., `#skin ar`) so mixed-skin projects work. Packages are skin-agnostic at the AST/interface level.

---

## 5. What a "complete" language toolchain includes

| Component | Minimum viable | "Complete" | Prior art / reuse | Phase |
|---|---|---|---|---|
| **Compiler** | Parser, type checker, JS emitter, good errors | Incremental, multiple backends, fast (<1s rebuilds) | Gleam, TS 7, `salsa` (Rust incremental framework used by rust-analyzer) | 1 |
| **Formatter** | Canonical printer (doubles as skin translator) | Zero-config, idempotent, in-editor | `gofmt`, `gleam format`, Prettier | 1 |
| **LSP** | Diagnostics, hover, go-to-def, completion | Rename, refs, code actions, inlay hints, semantic tokens | LSP spec; `tower-lsp` (Rust), `gopls` (Go) | 1–2 |
| **VS Code extension** | TextMate grammar + LSP client | Snippets, debugger config, test explorer, skin switcher command | `vscode-languageclient` | 1 |
| **tree-sitter grammar** | Highlighting for GitHub/Neovim/Zed/Helix | Folding, textobjects, all three skins | [tree-sitter](https://tree-sitter.github.io/tree-sitter/) | 2 |
| **Package manager** | Lockfile, git/registry deps, semver resolver | Workspaces, per-target deps, **npm deps declared directly** | Cargo, `gleam add`, pub | 2 |
| **Registry** | Static index on GitHub + tarballs on S3/R2 | Website, search, docs hosting, auth, yanking | hex.pm (Gleam reuses it), crates.io | 2–3 |
| **Test runner** | `test` blocks + assertions + CLI | Snapshot tests, property tests, coverage, watch mode | Zig `test`, Vitest | 1 |
| **Debugger** | Source maps → Chrome DevTools / VS Code JS debugger | C backend: `#line` + DWARF via C compiler → gdb/lldb; DAP adapter | Source Map v3 spec, DAP | 1 (JS) / 3 (C) |
| **REPL** | Compile-and-eval via Node | Notebook kernel | Elixir `iex` | 2 |
| **Playground** | Compiler → WASM, runs in browser, executes JS output | Shareable links, skin toggle (huge for marketing) | Gleam playground, Rust playground | 1–2 |
| **Stdlib** | core (collections, string, option/result, math), `json`, `fs`, `path`, `time`, `async`, `http` client/server, `env`, `log` | `crypto`, `db` (Postgres/SQLite), `uuid`, `regex`, `test`, `cli` args, `websocket`, `i18n` (Arabic-aware) | On JS: thin wrappers over Node/Web APIs. On C: libuv, SQLite, OpenSSL/BoringSSL, yyjson | 1–3 |
| **Web framework** | Router + middleware + JSON + validation + Postgres | SSR + components/reactivity frontend, auth, jobs, migrations | Phoenix (+LiveView), Hono, Next.js; Gleam's Lustre/Wisp | 2–3 |
| **Game library** | 2D: wraps PixiJS/Canvas (JS backend) | Native: raylib/SDL3 bindings (C backend); ECS; asset pipeline | raylib, Heaps (Haxe), Bevy | 2 (web) / 3 (native) |
| **Docs** | Language tour (in all 3 skins), stdlib API docs generated from doc-comments | "Book", cookbook, examples gallery, Arabic-first docs site | The Rust Book, Gleam tour, `exdoc` | 1–3 |
| **Distribution** | Single binary via GitHub releases + `npm i -g` shim | Installers, Homebrew/winget/scoop, Docker image, version manager | Bun/Deno/Gleam distribution | 1–2 |

---

## 6. Which language should the compiler be written in?

| Option | Strengths | Weaknesses | Precedent | Fit (AI-assisted) |
|---|---|---|---|---|
| **Rust** | Algebraic data types and `match` are ideal for ASTs; fast; single binary; **compiles to WASM** for playground; `salsa`, `tower-lsp`, tree-sitter bindings; strict compiler catches AI-generated mistakes at compile time | Slow compile times (Roc's main complaint), borrow checker friction for graph-heavy IR (use arenas/indices) | **Gleam** (exact model), rust-analyzer, Deno, Oxc/SWC/Biome, Ruff | **Best overall** |
| **Go** | Very fast builds; simple, so AI agents rarely produce unidiomatic code; GC removes ownership puzzles; single static binary; WASM possible (large) | Weaker sum types / pattern matching (lots of type switches); verbose error handling | **TypeScript 7** compiler, esbuild | **Close second.** Pick it if the team values build speed and simplicity over type expressiveness |
| **TypeScript** | Same language as the first target; runs in browser natively; huge AI training data; npm distribution | Microsoft just moved *off* TS for 8–12x speed; single-threaded; a rewrite becomes likely at scale | tsc ≤ 6, ReScript's tooling (partly), Elm-in-JS tools | Spike/prototype only |
| **Zig** | Fast compile; arena-friendly; C interop; Roc chose it | Pre-1.0 (breaking changes each release); smaller AI training corpus; fewer libraries (LSP, etc.) | Roc (2025–26), Bun runtime | Risky for a small team |
| **C** | Universal | Memory-unsafe, slow to write, worst AI failure modes (UB) | Lua, CPython | Not recommended |
| **OCaml** | Arguably best language for compilers | Small talent/AI corpus, Windows tooling weaker | Haxe, original rustc, Flow, ReScript | Fine, but Rust has more ecosystem |
| **Self-hosting (in the new language)** | Dogfooding | Bootstrap complexity; wait until language is stable | Go 1.5, Zig, Nim, Crystal | **Year 3+ only, maybe never** |

**Recommendation: Rust**, structured like Gleam: one binary with `compile | fmt | lsp | test | add | build | run | repl`. Use index-based arenas for AST/IR (sidesteps borrow-checker pain; Roc's rewrite rationale), keep crates small for incremental builds, and compile the frontend to WASM for the playground.
**If build-speed pain dominates, Go is an acceptable alternative.** TypeScript 7 proves Go scales to a production compiler with an LSP.

---

## 7. Phased roadmap

| Phase | Duration (est.) | Deliverables | Exit criterion |
|---|---|---|---|
| **0. Spec & spike** | 4–8 weeks | Core semantics doc (section 4 decisions locked); keyword tables for 3 skins + Franco lexing rule + tests; tiny Rust compiler: lexer (3 skins) → parser → AST → naive JS emit; round-trip formatter | `hello`, `fizzbuzz`, a JSON HTTP endpoint run on Node from all 3 skins; round-trip skin translation passes |
| **1. Usable language on JS** | months 2–9 | Type checker with inference + good errors; modules; `Option/Result`; async; `extern` JS interop + `.d.ts` import (consume npm typed APIs); source maps; test runner; formatter; LSP (diagnostics/hover/goto/complete) + VS Code extension; WASM playground; stdlib core + json/fs/http/time | **Founder's own small internal SaaS service runs in production on it** |
| **2. Ecosystem & web** | months 9–18 | Package manager + simple registry (GitHub index); web framework (router, middleware, validation, Postgres, auth); frontend story (components → DOM or JSX-compatible output usable with existing React); 2D game lib over PixiJS/Canvas; tree-sitter grammars; docs site + Arabic/Franco tour; REPL | 3+ real products (SaaS, website, browser game) shipped; external early adopters |
| **3. Native backend** | months 18–36 | Typed IR → C emitter; ARC/ORC-style or Boehm memory management; `zig cc` bundled for cross-compiling; raylib/SDL3 bindings; stdlib per-target externs (libuv, SQLite); DAP debugging via `#line`/DWARF; incremental compilation | Native desktop game and single-binary server built from the same codebase that also targets web |
| **4. Hardening / 1.0** | months 30–48+ | Stability guarantee, edition system, perf work, WASM-GC backend (optional), security review of package registry, LTS docs | Public 1.0 when your own products have run on it for ≥12 months, not before |

**Realistic expectation:** with AI agents compressing implementation work, phases 0–1 can be fast. The historical bottlenecks were never typing speed, though. They were *design stability, ecosystem, docs and user trust*, and those still take years (TypeScript ~1.5–3.5 years and Elixir ~3.7 years even with large hosted ecosystems). Budget **~12 months to production-usable for your own products, 3–4 years to a credible public 1.0.**

### 7.1 Risk register

| Risk | Mitigation |
|---|---|
| JS semantics leak into the language | Section 4 decisions locked in Phase 0; C backend prototype (even toy) by month 12 to keep semantics honest |
| Three skins fragment the community / docs | Canonical AST, formatter-as-translator, docs generated in all skins from one source |
| Franco digit-letter lexing ambiguity | Formal rule + exhaustive keyword-table test (section 4.1) |
| Stdlib divergence across targets | Shared conformance test suite executed on every backend in CI |
| Scope creep (games + web + SaaS at once) | Ship SaaS/web first (JS); browser games second; native games only in Phase 3 |
| Compiler rewrite later | Choose Rust/Go now; never self-host before semantics are frozen |

---

## 8. Sources

**Primary / official**
- TypeScript native port announcement: https://devblogs.microsoft.com/typescript/typescript-native-port/
- Go 1.5 release notes (compiler/runtime moved off C): https://go.dev/doc/go1.5
- Gleam v1.0 announcement: https://gleam.run/news/gleam-version-1/ ; multi-target projects: https://gleam.run/news/v0.34-multi-target-projects
- Zig 0.15.1 release notes: https://ziglang.org/download/0.15.1/release-notes.html
- Elixir v1.0 release: https://elixir-lang.org/blog/2014/09/18/elixir-v1-0-0-released/ ; team/history: https://elixir-lang.org/development.html
- Kotlin media kit (timeline): https://kotlinlang.org/assets/kotlin-media-kit.pdf
- The Evolution of Lua (HOPL III): https://www.lua.org/doc/hopl.pdf
- Haxe repository (targets, OCaml): https://github.com/HaxeFoundation/haxe
- Carbon roadmaps: https://docs.carbon-lang.dev/proposals/p3564.html , https://docs.carbon-lang.dev/proposals/p0253.html
- rustc bootstrapping (dev guide): https://rust.googlesource.com/rust-lang/rustc-dev-guide/+show/e60f2f03f8cf8babb3de043098b336d123e4be21/src/building/bootstrapping/intro.md
- Dart sound type system: https://dart.cn/blog/dart-gets-a-type-system ; https://dart.cn/blog/dart-and-the-performance-benefits-of-sound-types
- Odin custom backend blog: https://odin.handmade.network/blog/p/2210-working_on_a_custom_backend_for_odin
- CPython developer guide: https://devguide.python.org/
- Crystal 1.0 announcement (forum): https://forum.crystal-lang.org/t/crystal-1-0-0-has-been-released/3056
- Unicode UAX #31 (identifiers): https://unicode.org/reports/tr31/
- tree-sitter: https://tree-sitter.github.io/tree-sitter/

**Secondary (trade press / encyclopedias). Treat dates with care.**
- TS 7 RC: https://visualstudiomagazine.com/articles/2026/06/22/typescript-7-0-rc-moves-microsofts-go-rewrite-into-the-mainline-compiler.aspx
- TS 7 GA: https://visualstudiomagazine.com/articles/2026/07/08/typescript-7-arrives-to-rock-vs-code-with-go-powered-speed.aspx ; https://infoq.com/news/2026/08/typescript-7-released/
- TS 6 last JS version: https://visualstudiomagazine.com/articles/2025/12/02/microsoft-gets-real-on-native-typescript-remake.aspx
- Gleam 1.0: https://www.infoq.com/news/2024/03/gleam-erlang-virtual-machine-1-0/ ; https://en.wikipedia.org/wiki/Gleam_(programming_language)
- Nim 1.0: https://www.infoworld.com/article/3440522/nim-language-reaches-10-status.html ; https://i-programmer.info/news/98/12871.html
- Crystal 1.0: https://sdtimes.com/softwaredev/crystal-programming-language-reaches-1-0/ ; https://www.infoq.com/news/2021/04/crystal-ruby-c-release-1
- Kotlin 1.0: https://www.infoq.com/news/2016/02/jetbrains-kotlin-1 ; https://adtmag.com/articles/2016/02/16/jetbrains-kotlin-ga.aspx
- Rust history: https://en.wikipedia.org/wiki/Rust_(programming_language)
- Swift history: https://en.wikipedia.org/wiki/Swift_(programming_language)
- Dart 2.0: https://www.androidpolice.com/2018/08/07/dart-googles-home-grown-programming-language-reaches-version-2-0/
- Odin: https://en.wikipedia.org/wiki/Odin_(programming_language)
- Roc Rust→Zig: https://weeklyrust.substack.com/p/why-roc-is-moving-away-from-rust ; https://tildes.net/~comp/1v5c/how_our_rust_to_zig_rewrite_is_going
- V 0.5.1: https://newreleases.io/project/github/vlang/v/release/0.5.1
- Mojo: https://en.wikipedia.org/wiki/Mojo_(programming_language) ; https://hwbusters.com/news/qualcomm-open-sourced-the-mojo-compiler-three-weeks-after-paying-3-9-billion-for-it/
- Jai: https://en.wikipedia.org/wiki/Jai_(programming_language)
- Dead Cells (Haxe + Heaps): https://mcvuk.com/development-news/when-we-made-dead-cells/ ; https://en.wikipedia.org/wiki/Motion_Twin

**Not independently verified in this pass (stated from general knowledge):** TypeScript 0.8/1.0 dates (Oct 2012 / Apr 2014), Dart 1.0 (Nov 2013), Flutter 1.0 (Dec 2018), Nim 2.0 (Aug 2023), Zig's start year (2016) and its WASM-blob bootstrap, Crystal's original Ruby compiler, Odin's start year, Northgard's use of Haxe/HashLink (omitted from the table for that reason).
