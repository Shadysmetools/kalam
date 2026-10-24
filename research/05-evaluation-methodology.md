# 05 — Evaluation Methodology and Language Scorecard

**Scope:** how the automated build → evaluate → score → fix loop rates the new language objectively. The language has three keyword skins (Simple English **EN**, Arabic **AR**, Egyptian Franco-Arabizi **FR**) over one shared semantics. It targets SaaS, websites and games, and is secure by default.
**Out of scope:** language design itself, Arabic-language precedent, implementation strategy and threat modelling. Other research notes cover those.
**Date:** 2026-10-24

---

## 0. TL;DR

1. Score the language on **8 weighted dimensions plus a pass/fail multilingual-parity gate** (section 4). Every metric comes from a CI job, so no human judgement is needed per iteration. Human studies (RCTs, Cognitive Dimensions walkthroughs) run only at **milestone gates**.
2. **Multilingual parity is a hard gate, not a weighted score.** Every program must produce the same canonical AST hash in EN, AR and FR. It must also round-trip losslessly through all six skin-to-skin transpilations, and give byte-identical output. Any parity failure fails the build.
3. **Security is scored by inverting vulnerable-code corpora.** OWASP Benchmark and NIST Juliet normally grade scanners. Here, we port their vulnerable patterns and measure what fraction *cannot be written* without an explicit `unsafe`-style escape hatch.
4. **LLM-writability is measured with a MultiPL-E-style harness.** We translate HumanEval and MBPP into all three skins and give the model only a frozen `llms.txt` and a compact spec. We report pass@1 with confidence intervals against a Python ceiling on the same model, plus a *self-repair rate* when compiler errors are fed back.
5. **Anti-Goodhart rules.** The loop optimizes against its own scorecard, so we keep held-out test slices, freeze corpora per milestone, pin model IDs and doc snapshots, and require statistically significant deltas.

---

## 1. How languages are evaluated: research basis

### 1.1 Usability and learnability (human studies; milestone gates only)

| Method | What it gives us | Source |
|---|---|---|
| **Randomized controlled trials on syntax** (Stefik & Siebert, TOCE 2013) | Four studies: two surveys rating how intuitive words and symbols are, and two accuracy studies with novices in Ruby, Java, Perl, Python, Quorum and **Randomo**. Randomo is a placebo language whose keywords were chosen at random. Java and Perl were *not* significantly more accurate than Randomo, while Quorum, Python and Ruby were. Non-programmers rated `for`, `while` and `foreach` the least intuitive loop words. **Template for us:** run each skin against a Randomo-style placebo skin with novice native speakers. | [Paper PDF](https://www.vidarholen.net/~vidar/An_Empirical_Investigation_into_Programming_Language_Syntax.pdf), [review](https://neverworkintheory.org/2014/01/29/stefik-siebert-syntax.html), [Quorum/Perl/Randomo study](https://neverworkintheory.org/2011/10/24/an-empirical-comparison-of-the-accuracy-rates-of-novices-using-the-quorum-perl-and-randomo-programming-languages.html), [2023 replication (Lappi et al.)](https://jyx.jyu.fi/handle/123456789/87476) |
| **Keyword-intuitiveness surveys** | A cheap pre-study. Show candidate keywords for one concept and ask native speakers which one fits. Run it separately for AR and FR, because Franco-Arabizi conventions vary. | Same as above |
| **Cognitive Dimensions of Notations** (Green & Petre, JVLC 1996) | A 14-dimension vocabulary: viscosity, hidden dependencies, error-proneness, role-expressiveness, premature commitment, consistency, diffuseness, and others. Use it for a structured design-review walkthrough at each milestone. | [Wikipedia overview](https://en.wikipedia.org/wiki/Cognitive_dimensions_of_notations), [Blackwell CD tutorial](https://www.cl.cam.ac.uk/~afb21/CognitiveDimensions/CDtutorial.pdf), [paper](https://www.academia.edu/3050347/Usability_Analysis_of_Visual_Programming_Environments_a_cognitive_dimensions_framework) |
| **PLIERS** (Coblenz et al., TOCHI 2021) | A process that adapts formative and summative HCI methods to language design. It was validated on Glacier and Obsidian. Use it as the outer process for milestone studies. | [arXiv 1912.04719](https://arxiv.org/abs/1912.04719v4) |
| **Hedy** (Hermans): a gradual language with keywords in many natural languages, including Arabic | The closest precedent for multi-skin keywords. Its right-to-left work covers the parser and the syntax highlighter. Arabic-specific evaluation studies were *planned* but we found none published, so our parity metrics are novel. | [ICPEC 2024](https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.ICPEC.2024.1), [Swidan "Hedy in Arabic"](https://cs.ou.nl/oursi/OUrsi081-Swidan-Hedy-in-Arabic.pdf), [Weblate AR keywords](https://hosted.weblate.org/projects/hedy/-/ar) |

**CI proxies for usability** (computed automatically every iteration):
- **Grammar size:** number of grammar productions, reserved keywords per skin, and distinct punctuation tokens. Lower is better, up to a floor.
- **Concept-to-keyword ratio:** each semantic concept has exactly one keyword per skin, so there are no synonyms in the core.
- **Tokens-to-solution:** median token count of the reference corpus solutions, compared with Python solutions to the same tasks.
- **Cyclomatic and Halstead complexity** of the corpus solutions, compared with Python.
- **Readability model score:** a Buse & Weimer style feature model trained on human ratings ([ISSTA 2008](https://hiper.cis.udel.edu/lp/lib/exe/fetch.php/courses/cisc879testing/a_metric_for_software_readability.pdf), [TSE 2010](https://web.eecs.umich.edu/%7Eweimerw/p/weimer-tse2010-readability-preprint.pdf)). Use it only as a trend signal: these metrics are known to transfer poorly across paradigms ([arXiv 2110.15246](https://arxiv.org/pdf/2110.15246)).

### 1.2 Error-message quality

- **Becker et al., "Compiler Error Messages Considered Unhelpful"** (ITiCSE-WGR 2019) surveys about 50 years of research. Its guidelines: improve readability, reduce cognitive load, give context, use a positive tone, show examples, and avoid jargon. ([review](https://neverworkintheory.org/2021/09/02/compiler-error-messages-considered-unhelpful.html), DOI 10.1145/3344429.3372508)
- **Elm, "Compiler Errors for Humans"** shows the code exactly as written, highlights the problem, and offers hints. Elm 0.19.1 added examples and links to explanatory pages. ([Elm blog](https://elm-lang.org/news/compiler-errors-for-humans), [Compilers as Assistants](https://elm-lang.org/blog/compilers-as-assistants))
- **LLM-enhanced error messages** have been studied as well ([arXiv 2210.11630](https://arxiv.org/pdf/2210.11630), [arXiv 2402.08072](https://arxiv.org/pdf/2402.08072)). That matters here, because an LLM will often be the one reading our diagnostics.

**Automatable error metrics:**
- **Diagnostic coverage:** the share of compile-fail tests whose diagnostic has a stable code (`E0123`), a source span, a message in all three skins, a "help:" suggestion, and a docs link.
- **Machine-applicable fix rate:** the share of diagnostics that carry an auto-fix, and the share of those fixes that compile once applied.
- **Rubric score:** an LLM judge with a pinned model and rubric grades each diagnostic on the Becker guideline checklist (0–5). Calibrate the judge against 50 human-graded diagnostics at each milestone.
- **Repair efficacy:** the LLM self-repair rate in section 1.5. It is the most objective signal we have that a message is useful.

### 1.3 Correctness and conformance testing

| Technique | Precedent | How we use it | Source |
|---|---|---|---|
| **Spec conformance suite** | test262: one file per spec clause, with metadata front-matter (`features`, `negative: {phase, type}`) | The spec has numbered clauses. Every clause has at least one positive and one negative test, and tests are tagged with the clause ID. We report clause coverage. | [tc39/test262](https://github.com/tc39/test262) |
| **UI / golden tests** | Rust `compiletest` UI tests compare `.stderr`/`.stdout` against blessed snapshots and use `//~ ERROR` annotations. | Every diagnostic is golden-tested in all three skins (`.en.stderr`, `.ar.stderr`, `.fr.stderr`). Updates use a `--bless` flag that needs review. | [rustc-dev-guide: UI tests](https://rustc-dev-guide.rust-lang.org/tests/ui.html), [compiletest](https://rustc-dev-guide.rust-lang.org/tests/compiletest.html), [insta](https://insta.rs) |
| **Differential testing** | Csmith found more than 325 bugs, and every compiler it tested silently miscompiled valid input. | Compare backends against each other: interpreter vs. compiled vs. WASM, debug vs. release, and each skin's front-end. Also check reference semantics by translating corpus programs to Python or TypeScript and comparing outputs. | [Csmith PLDI'11](https://www-old.cs.utah.edu/~regehr/papers/pldi11-preprint.pdf), [Regehr blog](https://blog.regehr.org/?p=492) |
| **Random program generation** | Csmith and YARPGen, which avoid undefined behaviour by construction | A typed generator for well-typed programs, used for crash and miscompile detection. | [impact study arXiv 1902.09334](https://arxiv.org/pdf/1902.09334) |
| **Equivalence Modulo Inputs** | Orion (PLDI 2014) deletes code a given input never reaches; outputs must not change. | A cheap metamorphic oracle for the optimizer. | [CS6120 explainer](https://www.cs.cornell.edu/courses/cs6120/2019fa/blog/equivalence-modulo-inputs) |
| **Coverage-guided fuzzing** | AFL++, libFuzzer | Fuzz targets: lexer, parser, type checker, formatter, stdlib parsers (JSON, URL, HTTP, templates). Run 24/7 between iterations, with a fixed budget per CI run. | [AFL++](https://github.com/AFLplusplus/AFLplusplus), [libFuzzer](https://llvm.org/docs/LibFuzzer.html) |
| **Grammar-based fuzzing** | Grammarinator (ANTLR grammars) | Generates syntactically valid input per skin, so fuzzing gets past the parser into deeper stages. | [Grammarinator](https://github.com/renatahodovan/grammarinator) |
| **Property-based testing** | QuickCheck, Hypothesis | Properties include: parse∘print = id, formatter idempotence, type soundness (well-typed programs never get stuck), and stdlib laws such as `decode(encode(x)) == x`. | [Hypothesis](https://hypothesis.readthedocs.io/) |
| **Round-trip / skin parity** | No direct precedent (new) | See section 2. | |

### 1.4 Performance benchmarking

- **The Computer Language Benchmarks Game:** about 10 standard programs (n-body, spectral-norm, fannkuch-redux, binary-trees, fasta, k-nucleotide, regex-redux, mandelbrot, pidigits, reverse-complement). It reports CPU time, elapsed time, peak memory and gzipped source size. Port all of them; source size doubles as a conciseness metric. ([site](https://benchmarksgame-team.pages.debian.net/benchmarksgame/))
- **TechEmpower Web Framework Benchmarks:** JSON serialization, single query, multiple queries, fortunes (HTML templating plus XSS escaping), updates and plaintext. Implement them against the stdlib web stack and the default ORM; fortunes doubles as a security test. ([site](https://www.techempower.com/benchmarks/))
- **Startup and binary size:** measure `hello world` cold start, HTTP server time-to-first-response, and stripped binary or bundle size with [hyperfine](https://github.com/sharkdp/hyperfine) (many runs, report median and p95).
- **Games:** frame time p50/p99 and GC pause p99 for a 10k-sprite scene at 60 fps.
- **Compiler throughput:** lines compiled per second on the corpus, and incremental rebuild time after a one-line edit (this matters for the iterative loop itself).
- **Noise control:** use a dedicated runner, pinned CPU governor, at least 10 runs, and Mann-Whitney U for regressions. A regression counts only if it is >5% and p<0.01.

### 1.5 LLM-friendliness (a top-priority dimension)

**Evidence base:**
- **MultiPL-E** (Cassano et al., IEEE TSE 2023) translates the HumanEval and MBPP prompts and unit tests into 18+ languages with small per-language compilers. It is the standard harness for adding a new language. ([paper page](https://ccs.neu.edu/~arjunguha/main/papers/2023-multipl-e.html), [repo](https://github.com/nuprl/MultiPL-E))
- **MultiPL-T** (OOPSLA 2024) found that Code LLMs struggle on low-resource languages (Julia, Lua, OCaml, R, Racket), and that pass rates track how much of the language appears in training data. Synthesizing verified training data by translating from Python, then filtering with tests, closes much of the gap. **Implication:** our language starts at zero training data, so in-context docs plus a test-verified synthetic corpus are the strategy. ([arXiv 2308.09895](https://arxiv.org/html/2308.09895v6))
- **In-context learning for esoteric languages** (NeurIPS 2025) gave models docs and sparse examples for Minipy, Pyth, Rhokell and 0815. They wrote some correct code, but far below mainstream-language rates. **Self-scaffolding**, which feeds verified model solutions back in as examples, raised Pyth accuracy from 16.67% to 30.82%. ([NeurIPS 2025](https://neurips.cc/virtual/2025/131647))
- **Inference-time acquisition of unfamiliar languages** (Cangjie-bench, 2026): an agent that consults the official docs and an execution environment beats RAG baselines. ([arXiv 2602.06976](https://arxiv.org/html/2602.06976v1))
- **In-context learning of libraries and a new language** (Isabelle): GPT-4 showed non-trivial from-scratch learning in context. ([arXiv 2311.09635](https://arxiv.org/pdf/2311.09635))
- **llms.txt** (Howard, 2024) is a proposed markdown index of key docs for inference-time use. It is *not* a ratified standard, so ship it but don't depend on it. ([llmstxt.org](https://llmstxt.org/))
- Base benchmarks: HumanEval ([arXiv 2107.03374](https://arxiv.org/abs/2107.03374)), MBPP ([arXiv 2108.07732](https://arxiv.org/abs/2108.07732)), and EvalPlus for stronger tests ([repo](https://github.com/evalplus/evalplus)).

**Design levers these findings suggest, which we can also score:**
- Keep keywords close to what LLMs already know: same block structure and operator precedence as Python/TypeScript, and no novel punctuation.
- One obvious way to do each thing.
- A compact spec that fits in context. Target ≤ 12k tokens for `llms.txt` plus the core spec, and ≤ 40k for `llms-full.txt`.
- Example-dense docs: every stdlib function has a runnable example, and the examples are also tests.
- Diagnostics that state the fix.
- A self-scaffolding corpus: the loop's verified solutions become few-shot examples. These must come from the **train split only**; see the anti-Goodhart rules.

**Evaluation protocol (LLM-Eval harness):**
1. Translate HumanEval-164 and MBPP-sanitized (about 400 problems) into the language, in all three skins, MultiPL-E style. Translate the prompt docstrings into simple English, Arabic and Franco as well.
2. **Context conditions:** C0 = zero docs (a floor measure). C1 = `llms.txt` plus the compact spec. C2 = C1 plus 3 few-shot examples. C3 = an agentic run with compiler and test feedback (k=3 turns).
3. **Models:** a pinned panel of 3: one frontier closed model, one mid-tier, and one open-weights model, with IDs and dates frozen per milestone. Run the **same model on Python** as the ceiling.
4. **Metrics:**
   - pass@1 at T=0.2 with n=10 samples, reported with a 95% bootstrap CI.
   - **Relative gap** = pass@1(ours, C1) / pass@1(Python).
   - **Skin gap** = max − min pass@1 across EN, AR and FR.
   - **Self-repair rate**: the share of failures fixed within 3 turns when given only our compiler output.
   - **Hallucinated-API rate**: the share of samples calling a stdlib name that doesn't exist.
   - **Syntax-valid rate**.
5. **Significance:** a change counts as improved or regressed only if the CI does not overlap the previous baseline. Otherwise the loop must treat it as noise.

### 1.6 Security evaluation

- **OWASP Benchmark:** thousands of servlet test cases, each either truly vulnerable or a false positive for one CWE, with an expected-results CSV and true-positive/false-positive-rate scoring. ([OWASP Benchmark](https://owasp.org/www-project-benchmark/))
- **NIST Juliet / SARD:** about 28.9k Java and 64k C/C++ cases across more than 100 CWEs, each with "bad" and "good" variants. ([SARD](https://samate.nist.gov/SARD/), [Juliet Java guide](https://samate.nist.gov/SRD/resources/Juliet_Test_Suite_v1.2_for_Java_-_User_Guide.pdf))
- **Inversion for a language:** we are not grading a scanner. For each CWE pattern relevant to SaaS (injection CWE-89/78/94, XSS CWE-79, path traversal CWE-22, SSRF CWE-918, deserialization CWE-502, open redirect CWE-601, hard-coded secrets CWE-798, missing auth CWE-862, CSRF CWE-352, ReDoS CWE-1333), a security engineer or LLM writes the most idiomatic port of the "bad" variant in each skin. The case then gets one outcome:
  - **Impossible:** it fails to compile with a typed error such as "string cannot be used as `SqlQuery`".
  - **Escaped:** it compiles only by using an explicit, greppable `unsafe`/`raw` escape hatch.
  - **Mitigated:** it compiles, but runtime auto-escaping or parameterization neutralizes the payload, verified by an exploit payload test.
  - **Vulnerable:** the exploit succeeds.
  - **Score:** *Secure-by-default rate* = (Impossible + Escaped + Mitigated) / total, with Impossible weighted highest.
  - The matching "good" variant must compile and run correctly. That is the false-positive side: secure code shouldn't fight the developer.
- **Trojan Source** (CVE-2021-42574 bidi, CVE-2021-42694 homoglyphs) worked against C, C++, C#, JS, Java, Rust, Go and Python ([trojansource.codes](https://trojansource.codes/), [The Record](https://therecord.media/new-trojan-source-attack-impacts-compilers-for-most-programming-languages/)). **This is the hardest test for us, because the AR skin legitimately needs right-to-left text.** The tests must show three things:
  1. *Any* raw bidi control (U+202A–U+202E, U+2066–U+2069) is rejected anywhere in source: code, comments *and* string literals, whether balanced or not. It must be written as an escape such as `\u{202E}`. This matches Rust's `text_direction_codepoint_in_literal` and `text_direction_codepoint_in_comment` lints. Legitimate Arabic never needs these characters, because the Unicode bidi algorithm orders it implicitly. Apply the same escape-only policy, at least inside identifiers, to U+061C (Arabic Letter Mark), U+200E/U+200F (LRM/RLM), and zero-width characters (U+200B–U+200D, U+FEFF).
  2. Legitimate Arabic identifiers, strings and comments compile, and the reordered display matches logical order.
  3. Mixed-script confusable identifiers are rejected, per [UTS #39 (Security Mechanisms)](https://www.unicode.org/reports/tr39/) and [UTS #55 (Source Code Handling)](https://www.unicode.org/reports/tr55/).
- **Arabic and Franco lexing hazards (fuzz targets):**
  - Normalize identifiers to NFC, and decide the policy for alef/hamza variants (ا أ إ آ) and taa marbuta/haa (ة/ه). Tatweel (ـ) and harakat must not create distinct identifiers.
  - Arabic-Indic digits (٠–٩) and Eastern Arabic-Indic digits (۰–۹) in numeric literals.
  - **Franco digit-letters (convention-dependent):** `2`=ء, and in Egyptian usage often ق; `3`=ع; `5`=خ; `6`=ط; `7`=ح; `8`=ق (Levantine/Gulf); `9`=ص. There are also apostrophe-modified forms: `3'`=غ, `7'`=خ, `6'`=ظ, `9'`=ض. The lexer must tell `3ala` (identifier) from `3` (number), and handle `3'`, unambiguously. Use differential fuzzing on this boundary, covering every variant in the table. Whether to accept variants is open question 2.
- **Supply chain and stdlib:** fuzz every stdlib parser; enforce [OpenSSF Scorecard](https://github.com/ossf/scorecard) on our own repos; package registry checks (signed packages, no install scripts by default, typosquat detection).

### 1.7 Ecosystem and production readiness

There is no single academic metric for this, so it is scored as a checklist modelled on the "are we X yet" pages and the OpenSSF Scorecard. Every item is binary, or a percentage measured by a script:
- Toolchain: formatter, linter, LSP (diagnostics, go-to-definition, hover, completion, rename, code actions, semantic tokens), debugger (DAP), test runner, package manager with a lockfile, REPL, docs generator, and a skin-switch command.
- Editors: VS Code extension and a tree-sitter grammar per skin, with right-to-left rendering verified in VS Code.
- Stdlib coverage measured against a target API checklist (section 5).
- Docs: tutorial, reference, and cookbook in all three skins; `llms.txt` and `llms-full.txt`.
- Deployment: Docker image, build for at least 3 OSes, WASM target, reproducible builds.

---

## 2. Multilingual parity: the hard gate

Each check is fully automatable and runs on every commit. **Any failure blocks the iteration.** These checks are not weighted, because a parity bug is a semantics bug.

| ID | Check | Implementation | Pass criterion |
|---|---|---|---|
| P1 | **Canonical AST identity** | Parse each corpus program in EN, AR and FR, then hash the skin-free canonical AST (keywords become concept IDs; identifiers keep their exact NFC code points). | Identical hash ×3 for 100% of the corpus |
| P2 | **Pairwise transpile round-trip** | For every ordered pair (EN→AR, AR→FR, …, 6 pairs) and three-hop cycles: `parse(print_X(parse_Y(src)))`. | AST equal; comments and blank lines preserved (lossless CST) for 100% |
| P3 | **Source fixpoint** | `print_Y(parse(print_Y(parse_X(s)))) == print_Y(parse_X(s))` byte-for-byte. | 100% |
| P4 | **Property-based round-trip** | Use the random program generator (section 1.3) and grammar fuzzer to generate at least 10k programs per CI run, then apply P1–P3. | 0 failures. Every failure is saved as a regression test. |
| P5 | **Behavioural identity** | Run every corpus task in all three skins against the same stdin and fixtures. | Byte-identical stdout, stderr and exit code, *except* skin-localized diagnostic text, which is compared by diagnostic code and span. |
| P6 | **Diagnostic catalog parity** | Every diagnostic code has message, help text and docs in EN, AR and FR. | 100% (a missing translation fails the build) |
| P7 | **Keyword-table integrity** | No keyword collides with another skin's keyword or identifier rules; no Franco keyword is a prefix-ambiguous number; no AR keyword differs from another only by diacritics. | 0 conflicts |
| P8 | **Mixed-skin policy** | A file declares its skin, or the skin is inferred. Mixing skins in one file is either rejected or allowed, as the design decides, and that behaviour is tested. | Matches policy |
| P9 | **Docs parity** | Every docs page exists ×3, and its code blocks compile in its own skin. | 100% |
| P10 | **LLM skin gap** | From section 1.5. **Exception:** this check is noisy and runs in the `llm-eval` job, so it is a nightly and milestone gate, not a per-commit gate. | ≤ 15 pp (MVP), ≤ 8 pp (beta), ≤ 5 pp (1.0) |

---

## 3. Anti-Goodhart rules for the automated loop

In a build → score → fix cycle, the fixer will overfit to whatever it can see. These rules are mandatory:

1. **Corpus split:** 70% *dev* (visible to the fixer), 30% *held-out* (scored only; tasks and tests are never shown to the fixing agent). The LLM-Eval harness uses the same split on HumanEval and MBPP.
2. **Freeze per milestone:** corpora, eval models and the doc snapshot are versioned (`eval-v0.3`). A milestone is passed only on its frozen eval set, and new tests go into the *next* eval version.
3. **No test editing by the fixer:** golden-file `--bless` and changes to test expectations need a separate reviewer step, which can be a human or a second agent with a different prompt and the original spec.
4. **Significance gating:** noisy metrics (LLM pass@k, performance) need CI-non-overlap or p<0.01 before they count as a change.
5. **Ratchets:** a metric that reaches its milestone threshold may not regress by more than its noise band. Regressions block the merge even if the composite score goes up.
6. **Composite score is advisory; gates are binding.** A higher weighted score never overrides a failed hard gate (parity, security "Vulnerable" cases, crash-free fuzzing).

---

## 4. The Language Scorecard

Each dimension score is 0–100, computed as the mean of its normalized metrics. **Composite = Σ weight × score / 100.** Weights sum to 100.

**Normalization rule (global):** each metric is mapped piecewise-linearly through four anchors:
- floor → 0
- MVP threshold → 60
- β threshold → 75
- 1.0 threshold → 90
- 1.0 exceeded by its improvement margin → 100

The floor is the value a trivial or empty implementation scores (0% pass, or 3× the MVP limit for lower-is-better metrics). With these anchors, meeting every threshold for a milestone produces exactly that milestone's composite pass score (section 4.3).
- **Lower-is-better metrics** use the same rule with the axis inverted.
- **Binary metrics** score 0 or 90, or 100 if 1.0 requires them.
- **"n/a" cells** for a milestone are excluded from the mean.
- **"Report only" metrics** carry no weight.

### 4.1 Dimensions and weights

| # | Dimension | Weight | Rationale |
|---|---|---|---|
| D1 | Correctness and conformance | 18 | Nothing else matters if programs mis-execute |
| D2 | Security by default | 16 | A core promise of the language |
| D3 | LLM-writability | 16 | Users will write code mainly through AI |
| D4 | Simplicity and learnability | 12 | Target audience includes non-native-English novices |
| D5 | Diagnostics quality | 8 | Helps humans and drives LLM self-repair |
| D6 | Performance | 10 | SaaS, web and games need adequate, not best-in-class, speed |
| D7 | Tooling | 10 | LSP, formatter and package manager are adoption prerequisites |
| D8 | Production readiness and ecosystem | 10 | Stdlib, docs, deploy, stability |
| **Gate** | **Multilingual parity** (section 2) | pass/fail | Blocks everything |
| | **Total** | **100** | |

### 4.2 Metrics, CI computation, and thresholds

Abbreviations: **MVP** = first usable slice, **β** = beta, **1.0** = production.

#### D1: Correctness and conformance (18)
| Metric | How CI computes it | MVP | β | 1.0 |
|---|---|---|---|---|
| Spec clause coverage | `conformance --report`: clauses with at least 1 positive and 1 negative test / total clauses | 60% | 90% | 100% |
| Conformance pass rate | test262-style runner over `spec-tests/**` ×3 skins | 95% | 99.5% | 100% |
| Corpus pass rate (dev / held-out) | `corpus run --skins all` (section 5), using each task's `tier` field | 100% of the 21 MVP-tier tasks | 100% of MVP+β tiers (46/50) | 50/50, including 100% held-out |
| Differential-backend mismatches | interpreter vs. native vs. WASM on corpus plus 10k generated programs | 0 on corpus | 0 | 0 on 1M generated/week |
| Fuzz crash-free hours | AFL++/libFuzzer on lexer, parser, typechecker, formatter; OSS-Fuzz-style continuous run | 24 h | 7 d | 30 d with 0 new crashes |
| Property tests | Hypothesis-style suites (formatter idempotence, round-trip, stdlib laws) | 100% pass | 100% | 100% |
| Line / branch coverage of compiler | coverage tool on test run | 60/45% | 80/65% | 90/75% |

#### D2: Security by default (16)
| Metric | How CI computes it | MVP | β | 1.0 |
|---|---|---|---|---|
| Secure-by-default rate (inverted OWASP/Juliet corpus, about 150 cases ×3 skins) | `seccorpus run`: classify Impossible / Escaped / Mitigated / Vulnerable via compile result plus an exploit payload harness | 80%, 0 Vulnerable for SQLi/XSS | 95% | 100%, ≥70% Impossible |
| Good-variant usability | the "good" twin of each case compiles and passes its tests | 90% | 98% | 100% |
| Trojan Source suite | about 60 cases: bidi controls in code, comments and strings; homoglyphs; mixed scripts; legitimate Arabic text | 100% bad rejected, 100% good accepted | same | same |
| Arabic/Franco lexing hazard suite | normalization, digits, tatweel, Franco digit-letters | 100% | 100% | 100% |
| Escape-hatch density in corpus | `grep unsafe\|raw` count / KLOC in the reference corpus | ≤ 2 | ≤ 1 | ≤ 0.5 |
| Stdlib parser fuzzing | JSON, URL, HTTP, template, regex (with ReDoS detection) | 24 h clean | 7 d | 30 d |
| Dependency and registry policy | signed packages, lockfile, no install scripts, OpenSSF Scorecard on our repos | n/a | Scorecard ≥ 6 | Scorecard ≥ 8 |

#### D3: LLM-writability (16)
| Metric | How CI computes it | MVP | β | 1.0 |
|---|---|---|---|---|
| pass@1, C1 (docs-in-context), EN skin, frontier model | MultiPL-E-style harness on held-out HumanEval and MBPP, n=10, T=0.2, 95% CI | ≥ 50% | ≥ 70% | ≥ 80% |
| Relative gap vs. Python, same model | pass@1(ours) / pass@1(Python) | ≥ 0.6 | ≥ 0.8 | ≥ 0.9 |
| Skin gap (max−min across EN/AR/FR) | same harness ×3 skins | ≤ 15 pp | ≤ 8 pp | ≤ 5 pp |
| Self-repair rate (C3, 3 turns, compiler feedback only) | failures from C1 that are fixed | ≥ 40% | ≥ 60% | ≥ 75% |
| Syntax-valid rate | samples that parse | ≥ 85% | ≥ 95% | ≥ 99% |
| Hallucinated-API rate | calls to stdlib names that don't exist | ≤ 15% | ≤ 5% | ≤ 2% |
| Corpus-task agent success | agent builds corpus apps (todo SaaS, pong) from the task text plus docs, in the agentic condition | 3/10 apps | 7/10 | 10/10 |
| Context footprint | tokens(`llms.txt` + compact spec) | ≤ 20k | ≤ 15k | ≤ 12k |
| Open-weights model pass@1 (C1) | same harness with a pinned open model | report only | ≥ 40% | ≥ 55% |

*Threshold note: MultiPL-T and the esoteric-language paper show that zero-shot rates on new languages fall far below Python. The MVP targets assume a frontier model with docs in context and a deliberately Python-like core. If MVP measurements show a structurally lower ceiling, recalibrate the thresholds at the milestone review. The loop may not change them on its own.*

#### D4: Simplicity and learnability (12)
| Metric | How CI computes it | MVP | β | 1.0 |
|---|---|---|---|---|
| Reserved keywords per skin | count in keyword table | ≤ 40 | ≤ 35 | ≤ 35 |
| Grammar productions | count in grammar file | ≤ 120 | ≤ 110 | ≤ 100 |
| Synonym count (more than one keyword per concept) | keyword table analysis | 0 | 0 | 0 |
| Median tokens per corpus solution vs. Python | tokenizer over the reference corpus | ≤ 1.3× | ≤ 1.15× | ≤ 1.1× |
| Median cyclomatic complexity vs. Python | static analysis on the corpus | ≤ 1.2× | ≤ 1.1× | ≤ 1.0× |
| Concepts needed before "hello web app" | count of distinct spec concepts used in task #10 | ≤ 12 | ≤ 10 | ≤ 8 |
| *Milestone gate (human):* novice RCT accuracy vs. placebo skin | Stefik-style study, about 20 novices per skin | n/a | significantly > placebo in all 3 skins | ≥ Python baseline |
| *Milestone gate (human):* Cognitive Dimensions walkthrough | expert panel, PLIERS-style | n/a | no dimension rated "severe" | no dimension rated "poor" |

#### D5: Diagnostics quality (8)
| Metric | How CI computes it | MVP | β | 1.0 |
|---|---|---|---|---|
| Diagnostic completeness (code + span + help + docs link, ×3 skins) | catalog lint | 70% | 95% | 100% |
| Golden UI-test coverage of diagnostics | each code has at least 1 `.stderr` golden ×3 skins | 70% | 100% | 100% |
| Machine-applicable fix rate | diagnostics with an auto-fix that compiles after applying | 20% | 40% | 60% |
| Becker-rubric judge score | pinned LLM judge, calibrated against human grades | ≥ 3.0/5 | ≥ 3.8 | ≥ 4.2 |
| Error-recovery: errors reported per broken file | the parser keeps going and does not cascade (ratio of genuine to reported errors) | ≥ 0.5 | ≥ 0.8 | ≥ 0.9 |

#### D6: Performance (10)
| Metric | How CI computes it | MVP | β | 1.0 |
|---|---|---|---|---|
| Benchmarks Game geomean vs. CPython | hyperfine on 10 ports | ≤ 1× (no slower) | ≥ 3× faster | ≥ 10× faster (Go-ish tier) |
| TechEmpower JSON and fortunes rps vs. Node/Express | wrk on a dedicated runner | ≥ 0.5× | ≥ 1× | ≥ 2× |
| Hello-world cold start | hyperfine | ≤ 200 ms | ≤ 50 ms | ≤ 20 ms |
| HTTP server first response | script | ≤ 1 s | ≤ 200 ms | ≤ 100 ms |
| Hello-world binary / bundle size | stripped artifact | ≤ 30 MB | ≤ 10 MB | ≤ 5 MB |
| Idle HTTP server RSS | `/proc` or psutil | ≤ 100 MB | ≤ 40 MB | ≤ 20 MB |
| Game scene p99 frame time (10k sprites) | built-in profiler | ≤ 33 ms | ≤ 16.7 ms | ≤ 16.7 ms, GC p99 ≤ 2 ms |
| Compile throughput / incremental rebuild | corpus build timing | ≤ 5 s / ≤ 2 s | ≤ 2 s / ≤ 500 ms | ≤ 1 s / ≤ 200 ms |
| Regression rule | Mann-Whitney U, >5% and p<0.01 = fail | on | on | on |

#### D7: Tooling (10)
| Metric | How CI computes it | MVP | β | 1.0 |
|---|---|---|---|---|
| LSP capability coverage | scripted LSP client exercises diagnostics, hover, definition, completion, rename, code actions, semantic tokens, formatting ×3 skins | 3/8 | 6/8 | 8/8 |
| LSP latency (completion p95 on a 10k-line project) | scripted client | ≤ 500 ms | ≤ 150 ms | ≤ 50 ms |
| Formatter idempotence and parity | property test | 100% | 100% | 100% |
| Skin-switch command (`lang convert --to ar`) | P2 suite | yes | yes | yes |
| Package manager features | lockfile, semver resolution, offline mode, workspaces, publish, audit | 2/6 | 5/6 | 6/6 |
| Debugger (DAP) | scripted breakpoint, step, inspect | n/a | basic | full |
| Editor grammars (tree-sitter + VS Code), RTL rendering verified | screenshot test (Playwright) of AR file in VS Code web | EN only | ×3 | ×3 |

#### D8: Production readiness and ecosystem (10)
| Metric | How CI computes it | MVP | β | 1.0 |
|---|---|---|---|---|
| Stdlib checklist coverage | script compares exported API with the target checklist: HTTP server and client, router, JSON, SQL (parameterized only), templates (auto-escaped), auth/session, crypto (vetted primitives), time/tz, i18n, logging, file I/O, async, testing, 2D game loop, input, audio | 40% | 80% | 100% |
| Docs completeness ×3 skins | pages with runnable, tested examples | 50% | 90% | 100% |
| Doc example pass rate | doctest runner | 100% | 100% | 100% |
| Deploy targets | Linux, macOS, Windows, WASM, Docker image | 1 | 4 | 5 |
| Reproducible build | two builds → identical hash | n/a | yes | yes |
| Stability policy | edition/semver policy published; breaking-change detector on stdlib API | n/a | policy | enforced |
| Reference apps in production | corpus apps 41–50 deployed and soak-tested for 72 h with no crash or leak | 0 | 2 | 5 |

### 4.3 Milestone pass rule

A milestone is **passed** when all of the following hold on the frozen held-out eval set:
1. All hard gates pass: parity P1–P10, 0 "Vulnerable" cases for injection and XSS, Trojan Source 100%, and fuzz crash-free at that milestone's duration.
2. Every dimension score is ≥ 55 for MVP, ≥ 70 for beta, and ≥ 85 for 1.0. This allows a little slack below the anchor in any single dimension.
3. The composite is ≥ 60 for MVP, ≥ 75 for beta, and ≥ 90 for 1.0. These values match the normalization anchors.
4. The human-study gates for D4 are completed (beta and 1.0 only).

### 4.4 CI layout (suggested)

| Job | Trigger | Budget | Outputs |
|---|---|---|---|
| `fast` | every commit | ≤ 10 min | unit, conformance, UI goldens, parity P1–P3/P5–P9, corpus (dev), security corpus, Trojan Source |
| `deep` | every loop iteration | ≤ 60 min | P4 property round-trip at 10k, differential at 10k, 15-min fuzz per target, perf suite, held-out corpus |
| `llm-eval` | per iteration (sampled 25%) plus nightly (full) | API cost cap | D3 metrics with confidence intervals; pinned models |
| `continuous-fuzz` | always on | n/a | crash corpus becomes auto-filed regression tests |
| `milestone` | manual | n/a | frozen-set full run, human studies, `scorecard.json` + HTML report |

`scorecard.json` holds one record per metric: `{dimension, metric, value, ci95, threshold_mvp, threshold_beta, threshold_1_0, normalized, gate, eval_version, commit}`. The loop reads only this file to decide what to fix next. The fixer picks the lowest-scoring failing gate first, then the dimension with the largest weighted gap.

---

## 5. Test-program corpus (50 tasks × 3 skins = 150 programs)

Each task has: a natural-language spec in EN, AR and FR; a reference solution in each skin; stdin/fixture inputs; expected outputs; and a tier label. **MVP tier** = tasks the MVP must pass. Tasks marked **[H]** belong to the held-out set. Tasks marked **[NEG]** are negative tests: they must *fail to compile* or be neutralized with a specific diagnostic.

### A. Fundamentals (MVP)
| # | Task | Exercises |
|---|---|---|
| 1 | Hello world | I/O, skin keywords |
| 2 | FizzBuzz 1..100 | loops, conditionals, modulo |
| 3 | Read name, greet (Arabic and Latin input) | stdin, Unicode strings, RTL output |
| 4 | Sum / average / min / max of a number list (incl. Arabic-Indic digit input) | lists, numeric parsing |
| 5 | Fibonacci, iterative and recursive **[H]** | recursion, big ints |
| 6 | Word frequency count from text | maps, sorting, string split |
| 7 | Palindrome check (Arabic with diacritics stripped) | Unicode normalization |
| 8 | Temperature conversion table | formatting, floats |
| 9 | Guess-the-number (seeded RNG) | input loop, RNG determinism |
| 10 | Prime sieve to 1e6 **[H]** | arrays, performance |

### B. Data and types (MVP: 11–15; β: 16–20)
| # | Task | Exercises |
|---|---|---|
| 11 | Define record `User`, validate fields, print | records, validation |
| 12 | Sum types / enum + exhaustive match (shape area) | ADTs, exhaustiveness diagnostic |
| 13 | Optional/null handling: safe lookup chain **[H]** | null safety |
| 14 | Error handling: parse int, propagate errors | result types |
| 15 | JSON parse → typed struct → re-serialize (round-trip) | stdlib JSON |
| 16 | CSV read, group-by, write report **[H]** | file I/O, collections |
| 17 | Date/time: days between dates, Cairo timezone, Hijri display **[H]** | time, tz, i18n |
| 18 | Generic stack/queue with tests | generics, test runner |
| 19 | Immutable vs. mutable update | ownership/mutability model |
| 20 | Money arithmetic in decimal (no float rounding bug) | decimal type |

### C. Algorithms (β; also performance)
| # | Task | Exercises |
|---|---|---|
| 21 | Binary search + quicksort | algorithms |
| 22 | BFS shortest path on a grid **[H]** | graphs, queues |
| 23 | LRU cache | maps + linked structure |
| 24 | Regex-free tokenizer for arithmetic expressions + evaluator **[H]** | string processing, recursion |
| 25 | Benchmarks Game ports: n-body, binary-trees, fannkuch, spectral-norm, mandelbrot (counted as one task, 5 programs) | performance |

### D. Concurrency and I/O (β)
| # | Task | Exercises |
|---|---|---|
| 26 | Fetch 10 URLs concurrently with timeout (mock server) | async, HTTP client |
| 27 | Producer/consumer with channels **[H]** | concurrency |
| 28 | Rate limiter (token bucket) **[H]** | time, concurrency |
| 29 | File watcher → rebuild log | fs events |
| 30 | CLI tool with args, flags, help in 3 languages | CLI lib, i18n |

### E. Web and SaaS (MVP: 31–33; β: the rest)
| # | Task | Exercises |
|---|---|---|
| 31 | Static website: 3 pages, layout template, auto-escaped variables | templating |
| 32 | REST API: CRUD `/todos` in memory, JSON | router, JSON |
| 33 | Same API on SQLite with parameterized queries | SQL layer |
| 34 | Auth: signup/login, hashed passwords (argon2), session cookie (HttpOnly, Secure, SameSite) | auth defaults |
| 35 | Form with CSRF token + server validation, RTL form in Arabic **[H]** | CSRF, i18n |
| 36 | File upload with size/type limits, safe path storage **[H]** | path traversal defense |
| 37 | Outbound webhook with SSRF allow-list | SSRF defense |
| 38 | Pagination + search endpoint (SQL `LIKE` with bound params) | SQL safety |
| 39 | Background job queue + retries **[H]** | jobs |
| 40 | TechEmpower suite: json, db, queries, fortunes, updates, plaintext | performance + XSS-escaped fortunes |

### F. Full applications (β: 42, 43, 45; 1.0: 41, 44, 46, 47; deployed and soak-tested)
| # | Task | Exercises |
|---|---|---|
| 41 | **Todo SaaS**: multi-tenant, auth, Postgres, tenant isolation tests, HTML UI | end-to-end SaaS |
| 42 | Blog CMS with markdown → sanitized HTML, admin role **[H]** | XSS, authz |
| 43 | Real-time chat (WebSocket) with rooms **[H]** | websockets |
| 44 | Invoice generator → PDF, Arabic RTL layout | i18n, RTL rendering |
| 45 | **Pong** game: 2-player, 60 fps, input, collision, score | game loop |
| 46 | Snake or Breakout with sprites and sound **[H]** | 2D engine, assets |
| 47 | Browser game compiled to WASM | WASM target |

### G. Negative and security tasks (MVP; must fail or be neutralized) [NEG]
| # | Task | Expected result |
|---|---|---|
| 48 | SQL query built by string concatenation with user input | compile error: `String` is not `SqlQuery`, with a help message pointing at the parameterized form |
| 49 | Render user input into HTML with no escaping; shell command with interpolated input; `../` path join from request | 3 sub-cases: compile error or auto-escape verified by an exploit payload; access via `raw`/`unsafe` only |
| 50 | Trojan Source file (bidi override in a string or comment that hides an early return) and a homoglyph identifier | lexer error naming the code point (U+202E, …); the same file with an escaped `\u{202E}` compiles |

**Corpus rules:**
- Each task passes in **all three skins** (parity P5), with identical outputs. Output text that is legitimately localized lives in an i18n table, not in the code.
- Reference solutions are written once in EN and machine-converted to AR and FR. They are then reviewed by a native speaker for natural keyword usage. The machine-converted versions *are* the parity test.
- Held-out tasks ([H]: 15 of 50, or 30%) have fixtures, tests and reference solutions stored in an access-controlled location that is separate from this report and the repo the fix loop can see. This report lists only their titles. Only the scores come back.
- Every task manifest (`corpus/<id>/task.toml`) has machine-readable `tier = "mvp"|"beta"|"1.0"`, `split = "dev"|"heldout"` and `kind = "pos"|"neg"` fields, so CI computes tier pass rates mechanically. The tiers: MVP = 1–15, 31–33, 48–50 (21 tasks); β adds 16–30, 34–40, 42, 43, 45 (25 tasks, 46 cumulative); 1.0 adds 41, 44, 46, 47.
- Each milestone adds to the corpus but never edits a frozen task.

---

## 6. Open questions to resolve with the other research tracks

1. The mixed-skin-file policy (P8) needs a decision from the language-design track.
2. The Franco-Arabizi spelling conventions vary (`3ala` vs. `ala`, `7aga` vs. `haga`). Decide whether to accept aliases in the lexer. Accepting them conflicts with the zero-synonyms simplicity metric, so it needs a decision.
3. Arabic identifier normalization (merging alef variants) has security implications through confusables and must be agreed with the security track.
4. The LLM-eval budget: the full nightly run is about 3 models × 3 skins × 4 conditions × about 560 problems × 10 samples. That is about 200k generations, so it needs a cost cap and sampling strategy.

---

## Sources

- Stefik & Siebert 2013, *An Empirical Investigation into Programming Language Syntax*: https://www.vidarholen.net/~vidar/An_Empirical_Investigation_into_Programming_Language_Syntax.pdf ; review: https://neverworkintheory.org/2014/01/29/stefik-siebert-syntax.html
- Quorum/Perl/Randomo: https://neverworkintheory.org/2011/10/24/an-empirical-comparison-of-the-accuracy-rates-of-novices-using-the-quorum-perl-and-randomo-programming-languages.html
- Replication (Lappi et al. 2023): https://jyx.jyu.fi/handle/123456789/87476
- Cognitive Dimensions: https://en.wikipedia.org/wiki/Cognitive_dimensions_of_notations ; https://www.cl.cam.ac.uk/~afb21/CognitiveDimensions/CDtutorial.pdf
- PLIERS (Coblenz et al.): https://arxiv.org/abs/1912.04719v4
- Hedy: https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.ICPEC.2024.1 ; https://cs.ou.nl/oursi/OUrsi081-Swidan-Hedy-in-Arabic.pdf
- Becker et al. 2019, error messages: https://neverworkintheory.org/2021/09/02/compiler-error-messages-considered-unhelpful.html
- Elm: https://elm-lang.org/news/compiler-errors-for-humans ; https://elm-lang.org/blog/compilers-as-assistants
- LLM-enhanced errors: https://arxiv.org/pdf/2210.11630 ; https://arxiv.org/pdf/2402.08072
- Buse & Weimer readability: https://hiper.cis.udel.edu/lp/lib/exe/fetch.php/courses/cisc879testing/a_metric_for_software_readability.pdf ; https://web.eecs.umich.edu/%7Eweimerw/p/weimer-tse2010-readability-preprint.pdf ; limits: https://arxiv.org/pdf/2110.15246
- test262: https://github.com/tc39/test262
- Rust compiletest / UI tests: https://rustc-dev-guide.rust-lang.org/tests/ui.html ; https://rustc-dev-guide.rust-lang.org/tests/compiletest.html
- Csmith: https://www-old.cs.utah.edu/~regehr/papers/pldi11-preprint.pdf ; https://blog.regehr.org/?p=492
- Fuzzer-found compiler bug study: https://arxiv.org/pdf/1902.09334
- EMI: https://www.cs.cornell.edu/courses/cs6120/2019fa/blog/equivalence-modulo-inputs
- AFL++: https://github.com/AFLplusplus/AFLplusplus ; libFuzzer: https://llvm.org/docs/LibFuzzer.html ; Grammarinator: https://github.com/renatahodovan/grammarinator ; Hypothesis: https://hypothesis.readthedocs.io/
- Benchmarks Game: https://benchmarksgame-team.pages.debian.net/benchmarksgame/ ; TechEmpower: https://www.techempower.com/benchmarks/ ; hyperfine: https://github.com/sharkdp/hyperfine
- MultiPL-E: https://ccs.neu.edu/~arjunguha/main/papers/2023-multipl-e.html ; https://github.com/nuprl/MultiPL-E
- MultiPL-T: https://arxiv.org/html/2308.09895v6
- Esoteric-language ICL (NeurIPS 2025): https://neurips.cc/virtual/2025/131647
- Cangjie / inference-time language acquisition: https://arxiv.org/html/2602.06976v1
- ICL of libraries and new languages: https://arxiv.org/pdf/2311.09635
- HumanEval: https://arxiv.org/abs/2107.03374 ; MBPP: https://arxiv.org/abs/2108.07732 ; EvalPlus: https://github.com/evalplus/evalplus
- llms.txt: https://llmstxt.org/
- OWASP Benchmark: https://owasp.org/www-project-benchmark/
- NIST SARD / Juliet: https://samate.nist.gov/SARD/ ; https://samate.nist.gov/SRD/resources/Juliet_Test_Suite_v1.2_for_Java_-_User_Guide.pdf
- Trojan Source: https://trojansource.codes/ ; https://therecord.media/new-trojan-source-attack-impacts-compilers-for-most-programming-languages/
- Unicode UTS #39: https://www.unicode.org/reports/tr39/ ; UTS #55: https://www.unicode.org/reports/tr55/
- OpenSSF Scorecard: https://github.com/ossf/scorecard
