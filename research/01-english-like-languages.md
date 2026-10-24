# 01 — English-like, Beginner-Simple and Natural-Language Programming Languages: What Worked, What Failed

**Purpose:** Input for designing a new general-purpose language whose surface syntax is "the simplest English" (with Arabic and Egyptian Franco/Arabizi keyword sets covered in a separate report). The target is real production use: SaaS backends, websites and games.

**Date:** 2026-10-24
**Method:** Web research into primary sources where possible (language authors' own papers and blogs, official docs), plus empirical PL-usability research. Each claim links to its source. Statements marked *(background)* are well-established general knowledge that were not separately re-verified in this pass.

---

## 0. TL;DR

1. **Natural-language syntax has been tried repeatedly since 1959. It never achieved its goal: non-programmers did not become programmers.** COBOL, AppleScript, HyperTalk, Inform 7 and Plain English all show the same pattern. The code is easy to *read* and hard to *write*, because the grammar looks flexible but is actually rigid. A passage attributed to the AppleScript architect's HOPL III retrospective calls the experiment "not successful" ([secondary excerpt](https://forums.livecode.com/viewtopic.php?p=185319); primary PDF not retrieved).
2. **The "readable" languages that won (Python, Ruby, Lua) are not English. They are small, regular, formal grammars with a handful of English *words* as keywords.** Evidence from randomized controlled trials shows that keyword choice (e.g., `for`/`repeat` vs. cryptic symbols) does measurably help novices, while C-style punctuation performs no better than random keywords ([Stefik & Siebert 2013](https://neverworkintheory.org/2014/01/29/stefik-siebert-syntax.html)).
3. **For adoption, error messages, tooling and ecosystem matter more than syntax.** Elm's error messages influenced Rust, Scala 3 and others ([Rust blog](https://blog.rust-lang.org/2016/08/10/Shape-of-errors-to-come/), [RFC 1644](https://rust-lang.github.io/rfcs/1644-default-and-expanded-rustc-errors.html)). Elm itself stalled, though, because of governance and release cadence ([Elm Discourse](https://discourse.elm-lang.org/t/request-elm-0-19-2-any-update-to-help-adoption-to-prove-that-elm-is-not-dead/8843)). LiveCode/xTalk shrank after the company closed its open-source edition ([Wikipedia](https://en.wikipedia.org/wiki/LiveCode)).
4. **Gradual languages (Hedy) and localized keywords work well for teaching.** Hedy offers keywords in many languages, including Arabic, and reports about 500k monthly users ([ICPEC 2024](https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.ICPEC.2024.1)). It is a teaching scaffold that ends in Python, not a production language.
5. **Recommendation:** use a **"controlled English" grammar**. That means a small, fixed set of single-word English keywords and one canonical way to write each construct. No optional filler words, no synonyms, no multi-word keywords. Use significant indentation, a formatter that rewrites code into canonical form, and Elm/Rust-quality errors that suggest the exact fix. Keywords are a localization table over one grammar, which is how Hedy handles Arabic support.

---

## 1. Summary table

| Language | Year | Implementation | Adoption today | What made it simple | Outcome / main failure mode | Production use? |
|---|---|---|---|---|---|---|
| **COBOL** | 1959 | Compiled (many vendors; GCC 15 added `gcobol` in 2025) | Huge legacy base, ~220B lines often cited ([BMC](https://www.bmc.com/blogs/cobol-trends/)) | English sentences, `ADD X TO Y GIVING Z` | **Verbosity trap**. Business users never wrote it; programmers did. Survives through lock-in | Yes (banking, government), legacy |
| **BASIC → Visual Basic** | 1964 / VB 1991 | Interpreter → compiled/p-code → .NET IL | VB.NET frozen, "consumption-only" ([MS Learn](https://learn.microsoft.com/dotnet/visual-basic/getting-started/strategy)) | Line-oriented, few keywords, `If…Then…End If`, RAD form designer | Huge success as long as the IDE and ecosystem led. Faded when the vendor's platform moved on | Was massive (VB6, VBA), now legacy |
| **HyperTalk** | 1987 | Interpreter inside HyperCard | Dead (HyperCard discontinued 2004) *(background)* | `put x into field "name"`, objects you can see | Tied to one product. Ambiguity in the "chunk" expressions | Yes in its era (Myst prototype, in-house tools) |
| **AppleScript** | 1993 | Compiled to bytecode on Apple's OSA | Niche, macOS automation only | Reads like English: `tell application "Finder" to …` | **"Read-only language."** The creators call the English design unsuccessful ([HOPL excerpt](https://forums.livecode.com/viewtopic.php?p=185319)) | Automation scripts, not products |
| **LiveCode (xTalk)** | 2001 (MetaCard/Revolution lineage) | C++ engine, interpreted script | Small. Open source edition killed in 2021 ([Wikipedia](https://en.wikipedia.org/wiki/LiveCode)) | HyperTalk syntax, cross-platform GUI builder | Business-model failure. Community forked to OpenXTalk | Some in-house apps |
| **Inform 7** | 2006 | Compiler in literate C (inweb), compiles to Inform 6 → Z-machine/Glulx ([zarf](https://blog.zarfhome.com/2022/04/inform-7-open-source-release)) | Dominant in a tiny niche (interactive fiction) | Source reads as prose: `The kitchen is a room.` | Works because the **domain is narrow** (world modelling). Writing is still precise programming | Yes, IF games only |
| **Plain English (Osmosian)** | ~2006 | Self-hosted compiler written in "Plain English", outputs x86 PE ([Folds/osmosian](https://github.com/Folds/osmosian)) | Curiosity | Whole programs as English sentences | Toy. No ecosystem ([comp.compilers](https://compilers.iecc.com/comparch/article/06-02-130)) | No |
| **Python** | 1991 | CPython: bytecode VM in C | #1 by many measures. Used by ~58% of SO 2025 respondents ([SO 2025](https://survey.stackoverflow.co/2025/technology/)) | Indentation from ABC, few keywords, "one obvious way" | **Success.** Readable formal grammar, batteries included, C extension ecosystem | Yes, everywhere |
| **Ruby** | 1995 | MRI/YARV VM in C, YJIT | Steady. Shopify monolith has 2.8M lines of Ruby ([Shopify](https://shopify.engineering/shopify-monolith)) | Reads like prose (`5.times do`), DSL-friendly | Success through Rails. "Too many ways" and metaprogramming hurt at scale. Shopify added Sorbet types | Yes (Shopify, GitHub, Basecamp) |
| **Lua** | 1993 | Tiny C library, register VM. LuaJIT; Luau (Roblox) | Big in games. Luau used by Roblox, Alan Wake 2, Warframe ([Wikipedia](https://en.wikipedia.org/wiki/Luau_(programming_language))) | Very small language, `if … then … end`, one data structure (table) | Success as an embedded language. Roblox had to add gradual types for large codebases ([Luau](https://luau.org/2022/11/04/luau-origins-and-evolution.html)) | Yes (games) |
| **Scratch** | 2007 | Blocks. Scratch 3 runs in JS in the browser *(background)* | 100M+ registered users (third-party, 2022–23) ([BrightChamps](https://brightchamps.com/blog/scratch-stats/)) | No syntax errors possible, blocks snap together | Education success. Does not scale to large programs | No |
| **Hedy** | 2020 | Python web app. Lark grammar per level, transpiles to Python ([Hedy paper](https://hedy.org/research/Design_Implementation_and_evaluation_of_the_Hedy_programming_language_2022.pdf)) | ~500k monthly users. Keywords in many languages incl. Arabic ([ICPEC 2024](https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.ICPEC.2024.1)) | Gradual: level 1 has almost no syntax, level 20 is a Python subset | Education success. Ends at Python by design | No (teaching) |
| **Wenyan** | 2019 | Transpiler in JS → JS/Python/Ruby ([wenyan](https://github.com/wenyan-lang/wenyan)) | Viral novelty (GitHub stars), no production use | Classical Chinese grammar | Art project. Shows that a native-language surface over a JS backend is easy to build | No |
| **Elm** | 2012 | Compiler in Haskell → JS | Shrinking. No release since 0.19.1 (2019) ([Derw](https://derw.substack.com/p/whatever-happened-to-elm-anyway)) | No runtime exceptions, **best-in-class errors** | Tech success, **governance failure**. Closed contribution model, no roadmap | Some (NoRedInk, Vendr) *(background)* |
| **Gleam** | v1.0 Mar 2024 | Compiler in Rust → Erlang (BEAM) or JS | Growing. 2nd most admired in SO 2025 (70%) ([SO 2025](https://survey.stackoverflow.co/2025/technology/)) | Small surface, "learn in an afternoon", one way to do things, friendly errors, built-in formatter and LSP | Too early to judge, but a good model for a modern simple language | Early production |
| **Nim** | 2008 (2.0 in 2023) | Self-hosted compiler → C/C++/JS ([Nim 2.0](https://nim-lang.org/blog/2023/08/01/nim-v20-released.html)) | Niche | Python-like indentation, compiles to native code | Niche because of too many features (macros, several GC modes), a small ecosystem and no backer | Some |
| **Mojo** | 2023 (1.0 in 2026) | MLIR-based compiler. Open-sourced under Apache 2 in 2026 ([Willison](https://simonwillison.net/2026/Aug/18/)) | Early, AI-performance niche | Python-looking syntax | Dropped the "Python superset" goal around Aug 2025. Looking like Python is not the same as being Python | Early |
| **LLM-era (SudoLang, DSPy, BAML, LMQL)** | 2023–2025 | Interpreted by LLMs (SudoLang) or Python libs/DSLs | DSPy has ~28k stars. Others are niche ([arXiv 2510.19850](https://arxiv.org/pdf/2510.19850)) | Write intent in pseudo-English | Not deterministic. The serious ones moved *back* toward typed schemas | DSPy/BAML: yes, for LLM pipelines |

---

## 2. Language-by-language notes

### 2.1 COBOL (1959)
- **Origin:** CODASYL committee, heavily influenced by Grace Hopper's FLOW-MATIC. The explicit goal was that managers could read the programs *(background)*.
- **Implementation:** Ahead-of-time compiled. In April 2025 GCC 15 added a COBOL front end (`gcobol`) ([NYC*BUG talk](https://lists.nycbug.org/pipermail/talk/2025-November/018861.html)).
- **Adoption:** About 220 billion lines in production is the most cited figure, which comes from a Reuters-based estimate around 2020. Later articles claim up to 800B, without a clear method ([BMC](https://www.bmc.com/blogs/cobol-trends/), [Business Today 2026](https://www.businesstoday.in/amp/technology/news/story/cobol-the-66-year-old-program-thats-running-worlds-biggest-banks-534950-2026-06-04)).
- **What was simple:** Sentences like `MULTIPLY PRICE BY QTY GIVING TOTAL.` and a strong data-description model. It also has good built-in decimal arithmetic, with defined I/O, numeric precision and 8 rounding modes ([NYC*BUG](https://lists.nycbug.org/pipermail/talk/2025-November/018861.html)).
- **Why it succeeded / failed:** It succeeded because of **domain fit** (records, decimals, batch I/O) and institutional backing, **not** because of the English syntax. The verbosity did not make managers into programmers. It only made programs longer. COBOL 2002 added `COMPUTE`-style expressions and OO features, which moved it closer to conventional syntax *(background)*.
- **Lesson:** Keep **domain-fit primitives** such as exact decimal money types, records and I/O. Drop the sentence-length syntax.

### 2.2 BASIC (1964) and Visual Basic (1991)
- **Origin:** Kemeny and Kurtz at Dartmouth, for students in all majors. VB (1991) combined BASIC with a drag-and-drop form designer *(background)*.
- **Implementation:** Originally compiled on a time-sharing system. Microcomputer BASICs were interpreters. VB6 used p-code or native compilation. VB.NET compiles to .NET IL.
- **Adoption:** VB6 and VBA were enormous in business software in the 1990s and 2000s. In 2020 Microsoft said VB would get **no new language features** ([Slashdot/Thurrott](https://developers.slashdot.org/story/20/03/13/2111213/microsoft-plots-the-end-of-visual-basic)). The current strategy is "consumption-only" with no new workloads ([MS Learn](https://learn.microsoft.com/dotnet/visual-basic/getting-started/strategy), [InfoQ 2023](https://www.infoq.com/news/2023/02/dotnet-strategy-updated/)).
- **What made it simple:** Words such as `If … Then … Else … End If`, `For i = 1 To 10 … Next`, case-insensitive code, no semicolons, and most importantly an **instant, visual edit-run loop**.
- **Why it faded:** VB was a single-vendor language. When VB.NET forced a rewrite onto a C#-style object model, the simplicity went and so did the users. Line numbers and `GOTO` in early BASIC led to Dijkstra's famous criticism *(background)*.
- **Lesson:** The **edit → see result instantly** experience (REPL, hot reload, form designer) is a big part of what "simple" means. Avoid single-vendor control.

### 2.3 HyperTalk / HyperCard (1987) and LiveCode (2001–)
- **Origin:** HyperTalk was created by Dan Winkler in 1987 for Bill Atkinson's HyperCard ([Wikipedia](https://en.wikipedia.org/wiki/HyperTalk)). Its syntax resembled casual English, with Pascal-like structure.
- **Example:** `put the number of cards into field "count"`, `repeat with i = 1 to 10`, `on mouseUp … end mouseUp`.
- **What worked:** Code was **attached to visible objects** (buttons, cards), with event handlers named after what users actually do (`on mouseUp`). Non-programmers really did build things with it, e.g. the original Myst was prototyped in HyperCard *(background)*.
- **LiveCode:** This descendant (MetaCard → Runtime Revolution → LiveCode) has a C++ engine. The GPL Community edition was **discontinued on 31 Aug 2021**. The CEO said 99% of the work was done in-house and the free edition was not worth the cost ([Wikipedia](https://en.wikipedia.org/wiki/LiveCode), [livecode.org](https://livecode.org/)). The community forked it as OpenXTalk.
- **Lesson:** An event-driven model ("when button is clicked") is very approachable. A language whose only implementation belongs to a company is fragile, so an open-source, community-governed core is necessary.

### 2.4 AppleScript (1993) — the main warning
- **Implementation:** Compiled to bytecode on the Open Scripting Architecture. It talks to applications through Apple Events *(background)*.
- **The verdict from its designers:** A passage attributed to William Cook's HOPL III paper "AppleScript" (2007; Cook was AppleScript's chief architect, [bio](https://www.cs.utexas.edu/~wcook/bio.htm); proceedings DOI [10.1145/1238844](https://doi.org/10.1145/1238844)) says the following. *Verification note: I could not retrieve the primary PDF in this pass, so the text below is quoted from a secondary excerpt. Confirm it against the ACM DL copy before citing it externally.*
  > "The experiment in designing a language that resembled natural languages (English and Japanese) was not successful… It is easy to read AppleScript, but quite hard to write it… In hindsight, we believe that AppleScript should have adopted the Professional Dialect that was developed but never shipped." ([excerpt quoted on LiveCode forum](https://forums.livecode.com/viewtopic.php?p=185319))
- It goes on to say that natural-language goals led to **multi-token keywords** and a syntactic variety that **confused programmers more than it helped**.
- **Matt Neuburg** (*AppleScript: The Definitive Guide*) has a whole section titled "The English-likeness monster", showing valid English that fails in AppleScript and the reverse. He notes that AppleScript has "no natural language intelligence at all" ([deprogrammaticaipsum](https://deprogrammaticaipsum.com/the-english-likeness-monster/)).
- **John Gruber:** "saying what you mean in English almost never compiles". He argues that Python and JavaScript are easier because their syntax is "more abstract but more obvious" ([softwareas summary](https://softwareas.com/the-uncanny-valley-of-programming-languages/)).
- **Lesson:** This is the **uncanny valley of syntax**. The closer the code looks to English, the more users expect English to work, and the more frustrating each rejection becomes.

### 2.5 Inform 7 (2006) — natural language that works because the domain is narrow
- **Implementation:** Graham Nelson's compiler is about 300–400k lines of literate C (inweb). It was open-sourced under the Artistic License 2.0 on 28 Apr 2022 ([zarf](https://blog.zarfhome.com/2022/04/inform-7-open-source-release), [intfiction](https://intfiction.org/t/inform-7-v10-1-0-is-now-open-source/55674)). It compiles to Inform 6, then to Z-machine/Glulx story files.
- **Example:** `The Kitchen is a room. The knife is in the Kitchen. Instead of taking the knife when the player is not hungry: say "You're not hungry."`
- **Nelson's argument:** ambiguity is handled by **changing diction**, meaning the writer rephrases more specifically. He accepts that underneath, Inform is a formal language ([Northwestern reading](https://users.cs.northwestern.edu/~chrdimo/teaching/eecs396-w19/9.pdf)).
- **Criticism:** Malcolm Ryan wrote that the naturalness is an illusion: "the same precision of syntax is still as necessary as ever", and that this makes mistakes **harder to spot** ([Grand Text Auto](https://grandtextauto.soe.ucsc.edu/2006/04/30/announcing-inform-7/)).
- **Why it worked anyway:** Its domain (rooms, objects, rules about what happens) is mostly **declarative facts and rules**, which English expresses well. Its users are writers. Inform 7 has never been used for general-purpose work.
- **Lesson:** English works well for **declarations** (`a User has a name and an email`) and **rules/events** (`when the order is paid: …`). It works badly for algorithms (loops, index arithmetic, data transforms).

### 2.6 Plain English / Osmosian Order (~2006)
- Self-hosted compiler written in Plain English. It produces 32-bit x86 Windows executables ([Folds/osmosian](https://github.com/Folds/osmosian), [Parallax forum](https://forums.parallax.com/discussion/comment/1368746)).
- Received as a toy with no clear value for professional programmers ([comp.compilers](https://compilers.iecc.com/comparch/article/06-02-130)). Last release 2017.
- **Lesson:** Being able to write a whole compiler in English-like sentences does not make the language attractive. Ecosystem, libraries and targets are what matter.

### 2.7 Python (1991) — the "readable" success case
- **Implementation:** CPython compiles to bytecode for a stack VM written in C. There is a large C/C++/Rust extension ecosystem.
- **Heritage:** Python's indentation came from **ABC** (CWI). Guido van Rossum worked on ABC, kept its readability, and dropped what made it hard to extend and to connect to the OS ([Meertens CWI slides](https://www.cwi.nl/documents/195216/Meertens-20191121.pdf), [Gordon College notes](https://www.cs.gordon.edu/courses/cs323/Python/PYTHON.pdf)). ABC is a useful case: it was designed for beginners and readability, but it **failed because it was closed off from the OS and from extensions**. Python succeeded by fixing exactly that.
- **Adoption:** Used by about 58% of SO 2025 respondents, up 7 points ([SO 2025](https://survey.stackoverflow.co/2025/technology/), [ssojet summary](https://ssojet.com/news/2025-developer-trends-top-programming-languages-and-ai-insights)).
- **What made it simple:** significant indentation, about 35 keywords, English words for operators (`and`, `or`, `not`, `in`, `is`), "one obvious way to do it" (PEP 20), a REPL, and batteries included.
- **What hurts at scale:** dynamic typing (fixed after the fact with type hints and mypy/pyright), performance (the GIL and an interpreter, now being addressed with free-threading and a JIT), and packaging fragmentation.
- **Lesson:** Python is the model to beat. It is a **formal grammar with English keywords**, not English. A new language should start from Python's surface and fix its weaknesses: types, packaging and speed.

### 2.8 Ruby (1995)
- **Implementation:** The YARV VM in C, with YJIT (Rust) since 3.x *(background)*.
- **Adoption:** Steady in web development through Rails. Shopify's core monolith has over **2.8M lines of Ruby** ([Shopify Engineering](https://shopify.engineering/shopify-monolith)).
- **What made it simple:** Very prose-like code (`3.times { … }`, `unless`, `until`, trailing `if`), optional parentheses, and an easy way to build internal DSLs (Rails, RSpec).
- **What hurts at scale:** Having **many ways to say the same thing** (aliases like `map`/`collect`, `unless`/`if !`, `and`/`&&` with different precedence), monkey-patching and metaprogramming "magic". At Shopify's scale this required **Sorbet** (static types) and **Packwerk** (module boundaries) ([Shopify](https://shopify.engineering/shopify-monolith)).
- **Lesson:** Prose-like optional syntax is pleasant to write but produces ambiguity and style debates in large teams. Choose **one** form for each construct.

### 2.9 Lua (1993)
- **Implementation:** A tiny C library with a register-based VM. LuaJIT is the fast tracing JIT. Lua was born at PUC-Rio in 1993 as a configuration language and grew into a general embeddable language ([HOPL paper](https://www.lua.org/doc/hopl.pdf), [versions](https://lua.org/versions.html)).
- **Adoption:** A leading scripting language for games (WoW UI, Roblox, Defold, LÖVE). Roblox's **Luau** is open source (MIT, 2021) and has been adopted by Remedy (Alan Wake 2), Digital Extremes (Warframe) and Giants (Farming Simulator 25) ([Wikipedia](https://en.wikipedia.org/wiki/Luau_(programming_language)), [Luau open source](https://luau.org/2021/11/03/luau-goes-open-source.html)).
- **What made it simple:** About 22 keywords, `if … then … end`, `while … do … end`, a single data structure (the table), and functions as values.
- **What hurts:** Arrays start at 1, global variables are the default, and there are no types. Roblox **added gradual types** because "the lack of type checking made mistakes easy in large code bases" ([Luau origins](https://luau.org/2022/11/04/luau-origins-and-evolution.html)).
- **Lesson:** Small and embeddable wins in games. A beginner-friendly language needs **local-by-default variables** and **gradual or inferred types** from day one, not added later.

### 2.10 Scratch (2007) and block languages
- MIT Media Lab. Scratch 3 runs in the browser *(background)*. Over 100M registered users by 2022–23 according to third-party aggregations ([BrightChamps](https://brightchamps.com/blog/scratch-stats/), [JetLearn](https://jetlearn.com/blog/scratch-statistics)).
- **What made it simple:** **Syntax errors are impossible**. Blocks only fit where they are valid. All available commands are visible in a palette.
- **Limits:** Large programs are hard to read and to diff, and there is no version control or text-based tooling.
- **Lesson:** The idea of "make invalid programs impossible to build" can be reused as **structured editing in the IDE** (autocomplete that only offers valid next words). An optional block view rendered from the same AST is a good onboarding tool for the new language.

### 2.11 Hedy (2020) — gradual and multilingual
- **Implementation:** Felienne Hermans (Leiden, later VU Amsterdam). A Python web app with a separate **Lark grammar for each level**, transpiled to Python ([Hedy paper 2022](https://hedy.org/research/Design_Implementation_and_evaluation_of_the_Hedy_programming_language_2022.pdf), [VU](https://research.vu.nl/en/publications/design-implementation-and-evaluation-of-the-hedy-programming-lang/)).
- **Design:** Level 1 has almost no syntax (`print hello world`, no quotes). Each level adds one concept or syntax rule (quotes, then `=`, then indentation, then brackets). The final levels are a Python subset.
- **Multilingual:** Keywords are translated into many languages. The 2024 figure is 54 UI languages, and keyword translations since 2022 include Dutch, French, Spanish, Hindi and **Arabic**. Right-to-left support needed changes to the parser, the editor and the highlighter ([ICPEC 2024](https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.ICPEC.2024.1), [Joy of Coding 2022](https://joyofcoding.org/2022/speakers/felienne-hermans.html), [Swidan, Hedy in Arabic](https://cs.ou.nl/oursi/OUrsi081-Swidan-Hedy-in-Arabic.pdf)). Arabic-specific difficulties are RTL layout, parsing, variable names and numerals.
- **Adoption:** ~500k monthly users. A user study with 39 children aged 11–14 found that they valued being able to control the difficulty level ([2021 study](https://www.hedy.org/research/Gradual_Programming_in_Hedy_A_First_User_Study_2021.pdf)).
- **Lesson (very relevant):** (a) **Keywords as a localization table over a single grammar** works, and has been tested with Arabic. (b) **Language levels or profiles** let beginners start with very little syntax without making production code ambiguous. Our language could have a "starter" profile and a "full" profile, the same idea used by Racket's teaching languages *(background)*.

### 2.12 Wenyan (2019)
- Lingdong Huang. Classical-Chinese grammar. The transpiler is written in JS and targets **JS, Python or Ruby** ([wenyan repo](https://github.com/wenyan-lang/wenyan), [wy-lang.org](https://wy-lang.org/)). Punctuation is optional. Wenyan went viral (thousands of GitHub stars in days, [IT之家](https://www.ithome.com/0/463/743.htm)). There is a reverse tool that converts JS to Wenyan ([wenyanizer](https://github.com/zxch3n/wenyanizer)).
- **Lesson:** A native-language surface over a **transpile-to-JS/Python backend** is cheap to build and gets attention quickly. It also shows that a **reversible mapping** (source ↔ target, or English ↔ Arabic keywords) is achievable. It is a novelty, not a production tool.

### 2.13 Elm (2012) — best error messages, governance failure
- **Implementation:** Compiler in Haskell, outputs JS. Pure functional, The Elm Architecture, no runtime exceptions in practice.
- **Errors:** "Compiler Errors for Humans" (2015) shows the code as written, with plain-language explanations ([elm-lang.org](https://elm-lang.org/news/compiler-errors-for-humans)). "Compilers as Assistants" ([elm-lang.org](https://elm-lang.org/blog/compilers-as-assistants)) and "The Syntax Cliff" (2019) link syntax-error quality to whether beginners keep going ([elm-lang.org](https://elm-lang.org/news/the-syntax-cliff)).
- **Influence:** Rust's 2016 error redesign cites Elm as an inspiration ([RFC 1644](https://rust-lang.github.io/rfcs/1644-default-and-expanded-rustc-errors.html), [Rust blog](https://blog.rust-lang.org/2016/08/10/Shape-of-errors-to-come/)). Scala 3 followed a similar path.
- **Why adoption stalled:** No compiler release since 0.19.1 (Oct 2019), no public roadmap, restricted JS interop in 0.19, and core contributors leaving to start forks (Gren, Derw, Roc) ([Derw substack](https://derw.substack.com/p/whatever-happened-to-elm-anyway), [Elm Discourse](https://discourse.elm-lang.org/t/request-elm-0-19-2-any-update-to-help-adoption-to-prove-that-elm-is-not-dead/8843)). Companies told one advocate they "cannot invest in a possibly dead language".
- **Lesson:** Copy Elm's errors. Do **not** copy its governance: publish a roadmap, release regularly and provide **escape hatches** (FFI/interop to JS, C or Python).

### 2.14 Gleam (v1.0, March 2024)
- **Implementation:** Compiler in Rust that targets **Erlang/BEAM and JavaScript** ([FAQ](https://gleam.run/frequently-asked-questions/), [New Stack](https://thenewstack.io/introduction-to-gleam-a-new-functional-programming-language/)).
- **Adoption:** **2nd most admired language in SO 2025 (70%)**, behind Rust ([SO 2025](https://survey.stackoverflow.co/2025/technology/)).
- **What made it simple:** A deliberately small surface ("learn in an afternoon"), no macros, no type classes, no exceptions, one way to do things. The compiler, formatter, LSP, package manager and build tool ship as **one binary**. Friendly errors in the Elm tradition. Inferred static types.
- **Lesson:** Gleam is the closest modern template for "simple but production-capable". Small surface, inferred types, built-in tooling, multiple backends (server and browser), and interop with a mature runtime ecosystem (OTP).

### 2.15 Nim (2008; 2.0 in 2023)
- **Implementation:** Self-hosted compiler that outputs C/C++/JS, then native binaries. ORC memory management became the default in 2.0 ([Nim 2.0](https://nim-lang.org/blog/2023/08/01/nim-v20-released.html), [DevClass](https://devclass.com/2023/08/03/nim-programming-language-version-2-0-released-compiles-to-native-code-or-javascript/)).
- **What made it simple:** Python-like indentation and readability with C-level speed.
- **Why it stays niche:** A large feature surface (macros, templates, several GC modes, style-insensitive identifiers) undercuts the simplicity. It also has a small ecosystem and no corporate backer.
- **Lesson:** Python-looking syntax that compiles to C is achievable. Simplicity is lost through **feature count**, not through syntax.

### 2.16 Mojo (2023 → 1.0 in 2026)
- MLIR-based compiler from Modular. Early marketing called it a "Python superset" for AI performance. According to Simon Willison (Aug 2026), the superset goal was dropped around Aug 2025. Mojo 1.0 shipped and was then open-sourced under Apache 2 ([Simon Willison](https://simonwillison.net/2026/Aug/18/), [SD Times](https://sdtimes.com/softwaredev/sd-times-open-source-project-of-the-week-mojo/)). *(Single secondary source for the 2026 status. Verify against Modular's announcement.)*
- **Lesson:** A familiar surface (Python) is a strong onboarding tool, but **promising compatibility with an existing language is a trap**. Borrow its look, not a compatibility guarantee.

### 2.17 LLM-era "natural-language" programming (2023–2026)
- **"English is the hottest new programming language"** (Karpathy, Jan 2023), followed by **"vibe coding"** (Feb 2025) ([TechRadar](https://www.techradar.com/pro/quote-of-the-day-by-openai-co-founder-andrej-karpathy-the-hottest-new-programming-language-is-english-signalling-the-shift-to-natural-language-dominance-in-the-ai-era), [New Stack](https://thenewstack.io/vibe-coding-where-everyone-can-speak-computer-programming/)). Here English is the *specification* and an LLM writes code in a conventional language. Execution stays deterministic only because the generated code is formal.
- **SudoLang** (2023): a pseudocode language that LLMs interpret. It claims 20–30% fewer tokens than plain prose ([GitHub](https://github.com/paralleldrive/sudolang), [O'Reilly](https://www.oreilly.com/radar/unlocking-the-power-of-ai-driven-development-with-sudolang)). It is not deterministic and has no real compiler.
- **DSPy / BAML / LMQL:** the production-grade tools in this space moved *toward* typed signatures, schemas and constraints, and away from free prose ([arXiv 2510.19850](https://arxiv.org/pdf/2510.19850), [arXiv 2602.21257](https://arxiv.org/pdf/2602.21257)).
- **Dijkstra (EWD667)** argued that the "obligation to use formal symbolism" is what lets programs be precise. Formal text works because a few syntactic rules rule out the nonsense that natural language allows ([EWD667](https://www.cs.utexas.edu/~EWD/transcriptions/EWD06xx/EWD667.html)). The LLM era has not changed this. It has moved the ambiguity into the prompt, and teams then add formal structure back.
- **Lesson for us:** An LLM-friendly language is valuable, since a regular English-keyword grammar is easy for LLMs to generate and for humans to review. But **the source of truth must be a deterministic grammar**. The role of an LLM is to *translate* free English or Arabic into canonical code that a human reviews, not to *be* the runtime.

---

## 3. Synthesis

### 3.1 The "English-like syntax trap"

Across COBOL, AppleScript, HyperTalk, Inform 7 and Plain English, the same failure modes appear:

1. **Uncanny valley / false affordance.** Code that looks like English invites users to write English. Each time a reasonable English sentence is rejected, it feels arbitrary ("saying what you mean almost never compiles", [Gruber](https://softwareas.com/the-uncanny-valley-of-programming-languages/)). Symbolic languages do not raise that expectation.
2. **Read-easy, write-hard asymmetry.** Readers infer meaning from context. Writers have to produce the exact accepted form. AppleScript's own designers say this explicitly ([HOPL](https://forums.livecode.com/viewtopic.php?p=185319)).
3. **Multi-word keywords and synonyms break the grammar.** `is greater than or equal to`, `the first character of`, `tell … to`, optional `the`. These require backtracking parsers, produce ambiguous parses, give bad error locations, and make tool support (completion, refactoring) harder.
4. **Verbosity stops helping after the first week.** `ADD A TO B GIVING C` costs more to read than `c = a + b` once someone knows the language. Experts scan code for its *shape*, and prose hides that shape.
5. **Ambiguity is resolved in the compiler, which is hidden from the user.** English scoping (what does "it" refer to?), precedence ("not a and b") and quantifiers ("every", "any") all need hidden rules. Hidden rules lead to bugs that cannot be seen in the code ([Ryan on Inform 7](https://grandtextauto.soe.ucsc.edu/2006/04/30/announcing-inform-7/)).
6. **The audience never changes.** In every case, programmers ended up writing the code anyway. COBOL's managers, AppleScript's "average users" and HyperCard's authors either became programmers or stopped.
7. **It works only in narrow declarative domains.** Inform 7 (world facts and rules), SQL (`SELECT … FROM … WHERE`) and Gherkin (`Given/When/Then`) succeed because they *declare* things. Algorithms do not read well as prose.

**Counter-evidence (what English words *do* buy):** Randomized controlled trials by Stefik and Siebert showed novices were significantly more accurate with Quorum, Python and Ruby than with Perl or Java, and **Perl/Java did no better than a language with randomly chosen keywords** ([Stefik & Siebert 2013](https://neverworkintheory.org/2014/01/29/stefik-siebert-syntax.html), [paper PDF](https://www.vidarholen.net/~vidar/An_Empirical_Investigation_into_Programming_Language_Syntax.pdf)). The Quorum studies found specific words matter: novices rated `repeat` as more intuitive than `for`, and found `while` and `foreach` confusing *(background: Quorum evidence-based design; note Stefik designed Quorum, a conflict of interest)*. So **well-chosen English keywords help. English grammar does not.**

### 3.2 Concrete design lessons

**Simplicity features that demonstrably help**

| Feature | Evidence / exemplar |
|---|---|
| Significant indentation (no braces, no `end` clutter, or optional `end`) | ABC → Python. Removes a whole class of mismatched-brace errors |
| No semicolons. One statement per line | Python, Lua, Ruby, Gleam, Nim |
| Few, real-word keywords (~25–35) | Lua (~22), Python (~35), Gleam (small surface). Quorum RCTs |
| One obvious way to write each construct, enforced by a built-in formatter | PEP 20, gofmt, Gleam/Elm formatters |
| Inferred static types (no annotations needed to start) | Elm, Gleam. Luau and Sorbet added types later at great cost |
| No null / no runtime exceptions; `Result`-style errors | Elm, Gleam |
| Local-by-default variables, immutable-by-default values | Lua's global-default is a known trap |
| Errors that teach (see §3.4) | Elm → Rust → Scala 3 |
| Instant feedback: REPL, hot reload, playground in the browser | BASIC/VB, HyperCard, Scratch, Hedy, Elm reactor |
| Event handlers named for user actions (`when button clicked`) | HyperTalk `on mouseUp`, Scratch hat blocks |
| Gradual levels/profiles for teaching | Hedy, Racket teaching languages |
| One toolchain binary: compiler + formatter + LSP + package manager + test runner | Gleam, Go, Cargo |
| Domain primitives built in: exact decimal money, dates, records, JSON, HTTP | COBOL's decimal and records are why it survives. Python's "batteries included" |
| Escape hatches/interop to a big ecosystem | Python's C-API, Gleam ↔ Erlang/JS, Luau ↔ C++. Absence of them killed ABC and hurt Elm |

**Features that hurt**

| Anti-feature | Seen in |
|---|---|
| Multi-word keywords (`is greater than or equal to`) | AppleScript, COBOL, HyperTalk |
| Optional filler words (`the`, `a`, `then`), synonyms (`map`/`collect`) | AppleScript, Ruby |
| Context-dependent meaning (`it`, `result`, implicit subjects) | HyperTalk `it`, AppleScript `tell` blocks |
| Global-by-default, 1-based arrays when everything else is 0-based | Lua |
| Many ways to do the same thing; metaprogramming and monkey-patching | Ruby, Nim |
| Typeless at scale (types added afterwards) | Python, Ruby, Lua, which all needed Mypy/Sorbet/Luau later |
| Single-vendor, closed governance | VB, LiveCode, Elm |
| Compatibility promises with another language | Mojo "superset", VB.NET |
| Case-insensitive identifiers or style-insensitivity | Nim, VB. Confusing in search and grep |

### 3.3 Recommended keyword/grammar philosophy for "simplest English"

**Principle: "Controlled English, not natural English."** This follows Simplified Technical English (ASD-STE100) as used in aerospace manuals, which works by fixing *one* meaning per word and a small approved vocabulary *(background)*. The language should be a **formal context-free (ideally LL(1)/LR(1)) grammar whose terminals happen to be short, common English words**.

Rules:

1. **One word = one keyword = one meaning.** No multi-word keywords. Use `and`, `or`, `not`, `is`, `in`, `to`, `from`, `each`, `if`, `else`, `when`, `repeat`, `return`, `give`, `use`, `make`, `try`. Each keyword should be a word a 10-year-old or an ESL speaker knows (aim for CEFR A1–A2 vocabulary). Any keyword must translate to a **single word** in Arabic and Franco, which keeps it localizable.
2. **No synonyms, no optional words.** `if` is the only conditional word: no `when`-as-`if`, no `unless`. `the` never appears. If two spellings are allowed, the formatter rewrites them to the canonical one.
3. **Keep math symbolic.** `a + b`, `x >= 10`, `total = price * qty`. Do *not* spell out `plus` or `is greater than`, which is the COBOL/AppleScript mistake. Symbols are universal across English, Arabic and Franco, and research does not show that word-operators help.
4. **English for structure, symbols for expressions.** Statement and declaration heads are English (`make`, `if`, `repeat`, `each`, `when`, `test`). Expressions are conventional.
5. **Reserve English sentences for declarative domains**, where Inform/SQL/Gherkin show it works:
   - Data: `type User has name: Text, email: Email, age: Number`
   - Events/rules: `when order is paid: send receipt to order.customer`
   - Routes: `on GET "/users/:id": …`
   - Tests: `test "adds tax": expect total(100) is 114`
6. **Significant indentation, colon-introduced blocks** (Python style), with no `end` keyword or optional `end` only as a formatter choice. No semicolons.
7. **Explicit references only.** No `it` and no implicit subject. Every value is named. (A pipeline `|>` gives fluent chaining without an implicit `it`.)
8. **Identifiers can be in any script**, while keywords come from a per-locale table. The AST is language-neutral, so the same program can be shown in English, Arabic or Franco keywords. This is the Hedy model and is reversible like Wenyan ⇄ JS. Identifiers are case-sensitive and normalized with Unicode NFC. Arabic needs care with numerals (٠١٢ vs 012), RTL plus mixed-direction rendering, and tatweel/diacritics normalization ([Swidan](https://cs.ou.nl/oursi/OUrsi081-Swidan-Hedy-in-Arabic.pdf)).
9. **Levels/profiles:** a `starter` profile (no types visible, `say` / `ask`, no imports) and a `full` profile on the same compiler. A beginner file is still valid production code.
10. **LLM-ready, not LLM-dependent:** the grammar is deterministic, and an optional assistant translates free English or Arabic into canonical code for the human to accept.

Illustrative sketch (not a spec):

```text
type Order has
    id: Id
    items: List[Item]
    paid: Bool = false

make total(order: Order) -> Money:
    sum = 0
    each item in order.items:
        sum = sum + item.price * item.qty
    return sum

when order is paid:
    email.send(order.customer, receipt(order))

on GET "/orders/:id":
    order = db.orders.get(id)
    if order is none:
        reply 404 "No order with that id"
    reply order
```

*Keywords used here are only `type`, `has`, `make`, `each`, `in`, `return`, `when`, `is`, `on`, `if`, `reply` and `none`. Everything else is an ordinary identifier, library call or symbol. Any phrase-like additions (e.g. `sum of each …`) must pass an ambiguity test and a one-word Arabic/Franco translation test before being accepted. Note that `when … is …` (event) and `if … is …` (comparison) both use `is`, so the grammar must fix exactly one meaning per position.*

### 3.4 Error-message best practices

Sources: Elm ([1](https://elm-lang.org/news/compiler-errors-for-humans), [2](https://elm-lang.org/blog/compilers-as-assistants), [3](https://elm-lang.org/news/the-syntax-cliff)), Rust ([blog](https://blog.rust-lang.org/2016/08/10/Shape-of-errors-to-come/), [RFC 1644](https://rust-lang.github.io/rfcs/1644-default-and-expanded-rustc-errors.html)), and the ITiCSE working group survey of 330 papers on programming error messages (1965–2019): Becker et al., "Compiler Error Messages Considered Unhelpful", 2019, [doi:10.1145/3344429.3372508](https://doi.org/10.1145/3344429.3372508), [review](https://neverworkintheory.org/2021/09/02/compiler-error-messages-considered-unhelpful.html).

1. **Show the user's code exactly as written**, with the problem underlined. Avoid making the user translate line and column numbers (Elm).
2. **Lead with a plain-language title, then explain why, then show the fix.** Example: "I expected a Number here, but `age` is Text. Did you mean `number(age)`?"
3. **Suggest a concrete fix** ("did you mean…") using edit distance over identifiers and keywords, *in the user's keyword language*. If an English keyword is typed in an Arabic-mode file, suggest the Arabic equivalent, and the other way round.
4. **Use positive, non-blaming tone.** Avoid "illegal", "fatal" and "invalid". (Becker et al. list readability, reducing cognitive load, giving context, a positive tone and examples among their 10 guidelines.)
5. **Point at the real location of the parse failure**, and for syntax errors show an example of the correct form (Elm 0.19.1 "Syntax Cliff").
6. **One error at a time for beginners** (the first root cause), with full lists available on request. Avoid cascades.
7. **Primary and secondary labels** that tell a story ("`x` defined here as Text… used here as Number") (Rust).
8. **A stable error code with a long explanation** (`rustc --explain E0308`-style), linked to docs in the user's language.
9. **Localized messages from day one:** message templates in English, Arabic and Franco, with code snippets kept in the source's own keyword set.
10. **Machine-readable output (JSON/LSP diagnostics with fix-its)** so IDEs and LLM assistants can apply fixes automatically.
11. **Treat error quality as a tested feature:** keep a corpus of real beginner mistakes, test against it like snapshot tests, and track "time to fix" as a metric (Hedy and Elm both studied real learner errors).

---

## 4. Implications for the new language (decision inputs)

- **Positioning:** "Python-simple, Gleam-safe, Elm-friendly errors, Hedy-multilingual." Do not market it as "programming in English". The AppleScript and COBOL history makes that a liability with professional developers.
- **Implementation path suggested by precedent:** (a) a compiler written in a fast language (Rust, as Gleam does) that **transpiles to an existing runtime** first (JS/TypeScript for web and Node, as Elm/Gleam/Wenyan do; or BEAM/Python) to inherit an ecosystem immediately. (b) Add a native/WASM backend later for games and performance. Building a new VM and ecosystem from nothing is what kept Osmosian, LiveCode and Nim niche.
- **Must-haves for production:** static types with inference, packages plus a registry, an FFI to the host ecosystem, a formatter, an LSP, a test runner, and HTTP/DB/JSON in the standard library.
- **Governance:** open source, a public RFC process (as Luau does), and regular releases. The Elm and LiveCode outcomes are the counterexamples.

---

## 5. Source list

- Dijkstra, EWD667 "On the foolishness of natural language programming": https://www.cs.utexas.edu/~EWD/transcriptions/EWD06xx/EWD667.html
- AppleScript HOPL III excerpt (W. Cook): https://forums.livecode.com/viewtopic.php?p=185319
- "The English-likeness monster" (Neuburg): https://deprogrammaticaipsum.com/the-english-likeness-monster/
- Uncanny valley of programming languages (Gruber, Coding Horror): https://softwareas.com/the-uncanny-valley-of-programming-languages/
- Linguistic uncanny valley, lua-l: https://lua-users.org/lists/lua-l/2006-09/msg00783.html
- HyperTalk: https://en.wikipedia.org/wiki/HyperTalk ; HyperCard: https://en.wikipedia.org/wiki/HyperCard
- LiveCode: https://en.wikipedia.org/wiki/LiveCode ; https://livecode.org/
- Inform 7 open-source: https://blog.zarfhome.com/2022/04/inform-7-open-source-release ; https://intfiction.org/t/inform-7-v10-1-0-is-now-open-source/55674
- Inform 7 design reading: https://users.cs.northwestern.edu/~chrdimo/teaching/eecs396-w19/9.pdf ; https://grandtextauto.soe.ucsc.edu/2006/04/30/announcing-inform-7/
- Plain English / Osmosian: https://github.com/Folds/osmosian ; https://compilers.iecc.com/comparch/article/06-02-130
- COBOL: https://www.bmc.com/blogs/cobol-trends/ ; https://lists.nycbug.org/pipermail/talk/2025-November/018861.html
- Visual Basic strategy: https://learn.microsoft.com/dotnet/visual-basic/getting-started/strategy ; https://www.infoq.com/news/2023/02/dotnet-strategy-updated/ ; https://developers.slashdot.org/story/20/03/13/2111213/microsoft-plots-the-end-of-visual-basic
- Python/ABC: https://www.cwi.nl/documents/195216/Meertens-20191121.pdf ; https://www.cs.gordon.edu/courses/cs323/Python/PYTHON.pdf
- Stack Overflow 2025 survey: https://survey.stackoverflow.co/2025/technology/
- Shopify monolith: https://shopify.engineering/shopify-monolith
- Lua: https://www.lua.org/doc/hopl.pdf ; https://lua.org/versions.html
- Luau: https://en.wikipedia.org/wiki/Luau_(programming_language) ; https://luau.org/2022/11/04/luau-origins-and-evolution.html ; https://luau.org/2021/11/03/luau-goes-open-source.html
- Scratch stats (third-party): https://brightchamps.com/blog/scratch-stats/ ; https://jetlearn.com/blog/scratch-statistics
- Hedy: https://hedy.org/research/Design_Implementation_and_evaluation_of_the_Hedy_programming_language_2022.pdf ; https://www.hedy.org/research/Gradual_Programming_in_Hedy_A_First_User_Study_2021.pdf ; https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.ICPEC.2024.1 ; https://joyofcoding.org/2022/speakers/felienne-hermans.html ; https://cs.ou.nl/oursi/OUrsi081-Swidan-Hedy-in-Arabic.pdf
- Wenyan: https://github.com/wenyan-lang/wenyan ; https://wy-lang.org/ ; https://github.com/zxch3n/wenyanizer
- Elm: https://elm-lang.org/news/compiler-errors-for-humans ; https://elm-lang.org/blog/compilers-as-assistants ; https://elm-lang.org/news/the-syntax-cliff ; https://derw.substack.com/p/whatever-happened-to-elm-anyway ; https://discourse.elm-lang.org/t/request-elm-0-19-2-any-update-to-help-adoption-to-prove-that-elm-is-not-dead/8843
- Rust errors: https://blog.rust-lang.org/2016/08/10/Shape-of-errors-to-come/ ; https://rust-lang.github.io/rfcs/1644-default-and-expanded-rustc-errors.html
- Gleam: https://gleam.run/frequently-asked-questions/ ; https://thenewstack.io/introduction-to-gleam-a-new-functional-programming-language/
- Nim: https://nim-lang.org/blog/2023/08/01/nim-v20-released.html ; https://devclass.com/2023/08/03/nim-programming-language-version-2-0-released-compiles-to-native-code-or-javascript/
- Mojo: https://simonwillison.net/2026/Aug/18/ ; https://sdtimes.com/softwaredev/sd-times-open-source-project-of-the-week-mojo/
- Stefik & Siebert syntax RCTs: https://neverworkintheory.org/2014/01/29/stefik-siebert-syntax.html ; https://www.vidarholen.net/~vidar/An_Empirical_Investigation_into_Programming_Language_Syntax.pdf ; https://neverworkintheory.org/2011/10/24/an-empirical-comparison-of-the-accuracy-rates-of-novices-using-the-quorum-perl-and-randomo-programming-languages.html
- Error-message research: https://doi.org/10.1145/3344429.3372508 ; https://neverworkintheory.org/2021/09/02/compiler-error-messages-considered-unhelpful.html
- LLM-era: https://www.techradar.com/pro/quote-of-the-day-by-openai-co-founder-andrej-karpathy-the-hottest-new-programming-language-is-english-signalling-the-shift-to-natural-language-dominance-in-the-ai-era ; https://thenewstack.io/vibe-coding-where-everyone-can-speak-computer-programming/ ; https://github.com/paralleldrive/sudolang ; https://www.oreilly.com/radar/unlocking-the-power-of-ai-driven-development-with-sudolang ; https://arxiv.org/pdf/2510.19850 ; https://arxiv.org/pdf/2602.21257
