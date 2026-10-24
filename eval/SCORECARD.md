# Kalam Language Scorecard (eval-v0.1)

**Status:** draft. It becomes binding when the freeze step in [`LOOP.md`](LOOP.md) writes `eval/FREEZE.json` and `eval/MANIFEST.sha256`.
**Machine-readable twin:** [`scorecard.json`](scorecard.json). The metric tables below and that JSON file come from one generator, so their metric ids, thresholds and commands always match. A change to either one goes through the reviewer-gated process in LOOP.md §7.
**Basis:** research/05 §2–§5, adapted to `spec/KALAM-SPEC.md` v0.1 and `DECISIONS.md`. Every adaptation is listed in §9.
**Who reads it:** the `/loop` fixer agent, which only ever reads the computed results; the reviewer agent; the founder at milestone gates.

> **Naming note.** Dimensions are `D1`–`D8`, as in research/05 and the spec's own citations ("05 D4"). Founder decisions are always written "DECISIONS D7" so the two numbering schemes never get confused.

---

## 1. How to read this

- **Milestones:** `mvp`, `beta`, `1.0`. The loop always works toward one **target milestone**, recorded in `eval/FREEZE.json`.
- **Eight weighted dimensions** (§4) are each scored 0–100. Their weighted mean is the **composite**. The composite is advisory.
- **Five hard gates** (§3) are pass/fail and binding. A higher composite never overrides a failed gate.
- **Every metric is computed by a command.** No human judgement is needed per iteration. Human studies run only at the beta and 1.0 gates (§6).
- **Measurement stages** `S0`–`S11` (§7) say which piece of the toolchain must exist before a metric can be measured. A metric that can't be measured yet scores **0**, not "n/a". This is deliberate: the loop must never gain points by leaving a measurement unbuilt.

## 2. Scoring math

**Normalization.** Each metric maps to 0–100 piecewise-linearly through five anchors on its own axis:

| Anchor | floor | MVP threshold | β threshold | 1.0 threshold | stretch |
|---|---|---|---|---|---|
| Score | 0 | 60 | 75 | 90 | 100 |

- **floor:** what an empty implementation scores. For lower-is-better metrics this is 3× the MVP limit, unless the table says otherwise.
- **lower-is-better:** the same mapping with the value axis inverted.
- **ties:** when two anchors share a value (for example β = 1.0 = 16.7 ms), meeting that value earns the higher score.
- **stretch:** if a metric gives none, two rules apply. When the 1.0 threshold is the metric's natural bound (100%, a full count, zero failures), meeting it scores 100. Otherwise stretch = 1.0 + (1.0 − β) on the metric's axis.
- **binary** metrics score 100 for pass and 0 for fail.
- **ladder** metrics score by level: the level a milestone requires earns that milestone's anchor, and the top level earns 100.
- **`n/a`** for the target milestone: the metric is left out of the dimension mean.
- **`report_only`:** recorded, weight 0.
- **unavailable** (tool or stage missing, LLM judge not calibrated, holdout runner not run): **0**.

**Dimension score** = mean of its normalized metrics that are not `n/a` or `report_only` for the target milestone.
**Composite** = Σ weight × dimension score / 100.

*Worked example.* Take `D4.tokens_vs_python` (lower is better; floor 3.9, MVP 1.3, β 1.15, 1.0 1.1). A measured 1.22 lies between MVP and β: 60 + (1.30 − 1.22) / (1.30 − 1.15) × 15 = **68**.

## 3. Hard gates

| Gate | What must hold | When | On failure |
|---|---|---|---|
| **G-INT** eval integrity | `eval/MANIFEST.sha256` matches; nothing under protected paths changed without a reviewer sign-off; `history.jsonl` is append-only; no corpus leak into `compiler/` or the stdlib (no task id, corpus path or expected-output line of 20 or more characters); `FREEZE.json` version equals the scored version | first step of every iteration | iteration void, nothing committed, loop stops and escalates |
| **G-PAR** multilingual parity | P1–P10 below | P1–P3 and P5–P9 in `fast`, P4 in `deep`, P10 nightly and at milestones | the iteration's change is rejected. If the gate was already failing, the next iteration must target it |
| **G-SEC** security | 0 *Vulnerable* cases (MVP: CWE-89 SQLi and CWE-79 XSS; β adds CWE-78, -22, -918; 1.0: every CWE in the security corpus); the Trojan Source suite at 100%; the Arabic/Franco lexing-hazard suite at 100% | every iteration (`fast`) | as G-PAR |
| **G-FUZZ** fuzzing | `deep`: 15 minutes per target finds no new crash. Milestone: the crash-free hours of D1/D2 meet the milestone threshold | `deep`, milestone | crash input becomes a regression test; iteration rejected |
| **G-RATCHET** ratchets | a metric that has met its target-milestone threshold may not fall below it by more than its noise band. Performance: a regression of more than 5% with Mann-Whitney U p < 0.01 fails | every iteration | iteration rejected even if the composite rose |

**Parity checks (G-PAR).** These are research/05 §2, made concrete for the spec.

| ID | Check | Implementation | Pass criterion |
|---|---|---|---|
| P1 | Canonical identity | `kalam kcf --hash` (spec §4.4) of each corpus case parsed from `en.kalam`, `ar.kalam` and `franco.kalam` | identical SHA-256 ×3 for 100% of cases |
| P2 | Pairwise round-trip | `kalam convert` over all 6 ordered skin pairs, plus 3-hop cycles | KCF equal; comments and blank lines preserved |
| P3 | Source fixpoint | `print_Y(parse(print_Y(parse_X(s)))) == print_Y(parse_X(s))` | byte-identical, 100% |
| P4 | Property round-trip | at least 10k generated programs per run (typed generator plus grammar fuzzer) through P1–P3 | 0 failures; each failure saved as a regression test |
| P5 | Behavioural identity | run every corpus case in all three skins | identical stdout, exit code and HTTP transcript; diagnostics compared by code and line (columns differ by skin) |
| P6 | Diagnostic catalog parity | every code has en/ar/franco message, help and explain page | 100% |
| P7 | Keyword-table integrity | spec §7.4 rules over `spec/keywords.json`. Rules 5–7 count once their data (confusable table, 2,000-word list) exists | 0 conflicts |
| P8 | Mixed-skin policy | an English keyword in an `ar`/`franco` file gives K0201; two skin files for one module give K0301 (spec X1, §4.1) | matches policy |
| P9 | Docs parity | every docs page exists ×3 and its code blocks compile in their own skin | 100% |
| P10 | LLM skin gap | `D3.skin_gap_pp` | within the target milestone threshold (15 / 8 / 5 pp) |

**Parity scope rule.** The rule differs by case kind.
- **`run` and `http` cases:** a case the parser rejects in all three skins is *unsupported*. That costs D1 points, but it is not a parity failure. A case accepted in some skins and rejected in others **is** a parity failure. So a compiler that rejects everything gains nothing.
- **`compile-error` cases:** P1–P3 apply only when the file parses (T48 `main`, T49 `bad`, T50 `homoglyph`). P5 compares the set of (code, line) error diagnostics across the three skins, and an identical, correct rejection in all three skins passes. T50 `main` is rejected before parsing, by design.

## 4. Dimensions and weights

| # | Dimension | Weight | Why this weight (research/05 §4.1) |
|---|---|---|---|
| D1 | Correctness and conformance | 18 | nothing else matters if programs mis-execute |
| D2 | Security by default | 16 | a core promise; three of its checks are also hard gates |
| D3 | LLM-writability | 16 | users will mostly write Kalam through AI |
| D4 | Simplicity and learnability | 12 | MENA professionals, many writing in a second language |
| D5 | Diagnostics quality | 8 | helps humans and drives LLM self-repair |
| D6 | Performance | 10 | SaaS, web and games need adequate speed, not the best |
| D7 | Tooling | 10 | LSP, formatter and package manager are adoption prerequisites |
| D8 | Production readiness and ecosystem | 10 | stdlib, docs, deploy, stability |
| | **Total** | **100** | |

## 5. Metrics, how CI computes them, and thresholds

`kalam` is the compiler binary (DECISIONS D10). `kalam-eval` is the harness entry point in `eval/harness/`, built in stage S0 and reviewer-gated. "≥"/"≤" give the direction. "pass" marks a binary metric. `L<n>` is a ladder level.

### D1 Correctness and conformance (weight 18)
<!--TABLE:D1-->
| Metric id | What | How CI computes it | Dir | Floor | MVP | β | 1.0 | Stage |
|---|---|---|---|---|---|---|---|---|
| `D1.spec_clause_coverage` | Spec clause coverage (%) | kalam-eval conformance --coverage: clauses = every numbered ### subsection of KALAM-SPEC.md sections 4-22; a clause is covered when >=1 positive and >=1 negative test under conformance/** carries `clause: <section>` front-matter. value = covered / clauses * 100. | higher | 0 | ≥ 60 | ≥ 90 | ≥ 100 | S2 |
| `D1.conformance_pass_rate` | Conformance pass rate (x3 skins) (%) | kalam-eval conformance --run --skins en,ar,franco: passed / total test-skin pairs * 100. A test that cannot run because a stage is missing counts as failed. | higher | 0 | ≥ 95 | ≥ 99.5 | ≥ 100 | S2 |
| `D1.corpus_tier_pass` | Corpus pass count, tier-ordered (dev + held-out, x3 skins) (tasks) | kalam-eval corpus --skins all --split dev, plus the sealed holdout runner (aggregates only). A task passes only if every case passes in all three skins. value = p_mvp + (p_mvp==21 ? p_beta + (p_beta==25 ? p_1_0 : 0) : 0), so a later-tier pass never substitutes for an earlier-tier failure. | higher | 0 | ≥ 21 | ≥ 46 | ≥ 50 | S4 |
| `D1.differential_mismatches` | Differential execution (scope with 0 mismatches) — levels: L0 none; L1 MVP: JS(node) vs reference interpreter on the corpus + 1k generated programs; L2 beta: + JS(browser, Playwright) and 10k generated programs per iteration; L3 1.0: + WASM/C backend when present and 1M generated programs per week | kalam-eval differential: run corpus + generated programs on each backend pair and compare stdout/stderr/exit. value = highest level whose scope had 0 mismatches. | ladder | 0 | L1 | L2 | L3 | S6 |
| `D1.fuzz_crash_free_hours` | Compiler fuzzing, crash-free hours (h) | kalam-eval fuzz-status: hours since the last new unfixed crash across the lexer, parser, type checker and formatter targets (AFL++/cargo-fuzz), counted on the current commit lineage. | higher | 0 | ≥ 24 | ≥ 168 | ≥ 720 | S10 |
| `D1.property_tests` | Property suites pass (fmt idempotence, round-trip, stdlib laws) (bool) | cargo test -p kalam-props --release; value = all properties pass on >=10k cases each. | binary | 0 | pass | pass | pass | S2 |
| `D1.compiler_line_coverage` | Compiler line coverage (%) | cargo llvm-cov --workspace --json over the full test run; value = lines.percent. | higher | 0 | ≥ 60 | ≥ 80 | ≥ 90 | S1 |
| `D1.compiler_branch_coverage` | Compiler branch coverage (%) | cargo llvm-cov --workspace --branch --json; value = branches.percent. | higher | 0 | ≥ 45 | ≥ 65 | ≥ 75 | S1 |
<!--/TABLE:D1-->

### D2 Security by default (weight 16)
<!--TABLE:D2-->
| Metric id | What | How CI computes it | Dir | Floor | MVP | β | 1.0 | Stage |
|---|---|---|---|---|---|---|---|---|
| `D2.secure_by_default_rate` | Secure-by-default rate (inverted OWASP/Juliet corpus) (%) | kalam-eval seccorpus: ~150 'bad' cases x 3 skins (CWE-89/78/94/79/22/918/502/601/798/862/352/1333; T48-T50 included). Each case is classified Impossible (typed compile error), Escaped (compiles only via unsafe.*/extern/.expose()), Mitigated (compiles; exploit payload neutralized at runtime) or Vulnerable (payload succeeds or case not decidable). value = (I+E+M)/total*100. Feeds gate G-SEC. | higher | 0 | ≥ 80 | ≥ 95 | ≥ 100 | S3 |
| `D2.impossible_share` | Share of bad cases that are Impossible (%) | same run as D2.secure_by_default_rate; value = Impossible/total*100. | higher | 0 | report_only | report_only | ≥ 70 | S3 |
| `D2.good_variant_pass` | Good-twin usability (%) | kalam-eval seccorpus --good: the secure twin of each case compiles and passes its tests. | higher | 0 | ≥ 90 | ≥ 98 | ≥ 100 | S4 |
| `D2.trojan_source_suite` | Trojan Source suite (~60 cases incl. T50) (bool) | kalam-eval trojan: every bad case rejected with the expected SEC code naming the code point; every good case (escaped controls, legitimate Arabic text, emoji ZWJ) accepted. Feeds gate G-SEC. | binary | 0 | pass | pass | pass | S1 |
| `D2.lexing_hazard_suite` | Arabic/Franco lexing hazard suite (bool) | kalam-eval lexhaz: normalization, alef/hamza and ى/ي folding in keywords, tatweel/harakat, Arabic-Indic and Persian digits, U+066B/U+066C, Franco digit-letter boundaries (3ala, 7asab, 2ktb) with did_you_mean. Feeds gate G-SEC. | binary | 0 | pass | pass | pass | S1 |
| `D2.escape_hatch_density` | Escape hatches per KLOC of reference corpus (per KLOC) | kalam audit --json over all dev corpus programs (en skin): count(unsafe.* + extern + .expose()) / KLOC. Counted on the AST, never by grep (spec P10). | lower | 6 | ≤ 2 | ≤ 1 | ≤ 0.5 | S3 |
| `D2.stdlib_parser_fuzz_hours` | Stdlib parser fuzzing, crash-free hours (h) | kalam-eval fuzz-status --stdlib: JSON, URL, HTTP request parser, html literal runtime, regex (with a linear-time/ReDoS oracle). | higher | 0 | ≥ 24 | ≥ 168 | ≥ 720 | S10 |
| `D2.supply_chain_scorecard` | OpenSSF Scorecard on project repos (score) | scorecard --repo=<public repo> --format json; value = aggregate score. Measurable once the repo is public (DECISIONS D13). | higher | 0 | n/a | ≥ 6 | ≥ 8 | S11 |
<!--/TABLE:D2-->

### D3 LLM-writability (weight 16)
LLM metrics use the anti-Goodhart rules of LOOP.md §6:
- The **model panel** is pinned in `eval/FREEZE.json`: one frontier model, one mid-tier model and one open-weights model, with exact model IDs and dates.
- The **doc snapshot** (the `spec/llms.txt` hash) is pinned there too.
- Scoring uses only the **held-out 30%** of HumanEval/MBPP.
- **Budget:** each iteration runs a random 25% sample of the held-out problems, EN skin, condition C1, n = 5. The full run of 3 models × 3 skins × C0–C3 × n = 10 is nightly and at milestones, under the API cost cap in `FREEZE.json`.
<!--TABLE:D3-->
| Metric id | What | How CI computes it | Dir | Floor | MVP | β | 1.0 | Stage |
|---|---|---|---|---|---|---|---|---|
| `D3.pass1_c1_en_frontier` | pass@1, docs in context (C1), EN skin, frontier model (%) | kalam-eval llm: MultiPL-E-style HumanEval-164 + MBPP-sanitized translated to all three skins; pinned model panel and doc snapshot from eval/FREEZE.json; n=10, T=0.2, 95% bootstrap CI; held-out 30% of problems only. Condition C1 = spec/llms.txt in context. Noise rule: ci95. | higher | 0 | ≥ 50 | ≥ 70 | ≥ 80 | S8 |
| `D3.relative_gap_python` | pass@1(Kalam, C1) / pass@1(Python), same model (ratio) | kalam-eval llm: MultiPL-E-style HumanEval-164 + MBPP-sanitized translated to all three skins; pinned model panel and doc snapshot from eval/FREEZE.json; n=10, T=0.2, 95% bootstrap CI; held-out 30% of problems only. Python ceiling run on the same held-out problems. Noise rule: ci95. | higher | 0 | ≥ 0.6 | ≥ 0.8 | ≥ 0.9 | S8 |
| `D3.skin_gap_pp` | Skin gap: max-min pass@1 across EN/AR/FR (pp) | kalam-eval llm: MultiPL-E-style HumanEval-164 + MBPP-sanitized translated to all three skins; pinned model panel and doc snapshot from eval/FREEZE.json; n=10, T=0.2, 95% bootstrap CI; held-out 30% of problems only. Same as parity check P10. Feeds gate G-PAR. Noise rule: ci95. | lower | 45 | ≤ 15 | ≤ 8 | ≤ 5 | S8 |
| `D3.self_repair_rate` | Self-repair within 3 turns from compiler output only (C3) (%) | kalam-eval llm: MultiPL-E-style HumanEval-164 + MBPP-sanitized translated to all three skins; pinned model panel and doc snapshot from eval/FREEZE.json; n=10, T=0.2, 95% bootstrap CI; held-out 30% of problems only. Failures of C1 re-prompted with `kalam check --json` output only. Noise rule: ci95. | higher | 0 | ≥ 40 | ≥ 60 | ≥ 75 | S8 |
| `D3.syntax_valid_rate` | Samples that parse (%) | kalam-eval llm: MultiPL-E-style HumanEval-164 + MBPP-sanitized translated to all three skins; pinned model panel and doc snapshot from eval/FREEZE.json; n=10, T=0.2, 95% bootstrap CI; held-out 30% of problems only. value = samples accepted by `kalam check --parse-only` / samples. Noise rule: ci95. | higher | 0 | ≥ 85 | ≥ 95 | ≥ 99 | S8 |
| `D3.hallucinated_api_rate` | Calls to stdlib names that do not exist (%) | kalam-eval llm: MultiPL-E-style HumanEval-164 + MBPP-sanitized translated to all three skins; pinned model panel and doc snapshot from eval/FREEZE.json; n=10, T=0.2, 95% bootstrap CI; held-out 30% of problems only. value = samples with >=1 K0420 or unknown-module K0401 / samples. Noise rule: ci95. | lower | 45 | ≤ 15 | ≤ 5 | ≤ 2 | S8 |
| `D3.agent_app_success` | Agent builds corpus apps from task text + docs (agentic) (apps of 10) | kalam-eval agent-apps: fixed set {T31,T32,T33,T34,T38,T41,T42,T45,T46,T47}; a separate eval agent (never the fixer) gets the task text, llms.txt and `kalam mcp`; success = the task's expected.txt passes in the EN skin within the turn budget. Noise rule: ci95. | higher | 0 | ≥ 3 | ≥ 7 | ≥ 10 | S8 |
| `D3.context_footprint_tokens` | Tokens in spec/llms.txt (tokens) | kalam-eval tokens spec/llms.txt with the tokenizer pinned in eval/harness/requirements.lock (tiktoken o200k_base). Spec section 21.2 budget. | lower | 45000 | ≤ 15000 | ≤ 13500 | ≤ 12000 | S0 |
| `D3.open_weights_pass1` | pass@1 C1, pinned open-weights model (%) | kalam-eval llm: MultiPL-E-style HumanEval-164 + MBPP-sanitized translated to all three skins; pinned model panel and doc snapshot from eval/FREEZE.json; n=10, T=0.2, 95% bootstrap CI; held-out 30% of problems only. Open-weights member of the panel. Noise rule: ci95. | higher | 0 | report_only | ≥ 40 | ≥ 55 | S8 |
<!--/TABLE:D3-->

*Threshold note (research/05):* zero-shot rates on new languages are far below Python. If the MVP measurement shows a structurally lower ceiling, the founder recalibrates at the milestone review. The loop never changes a threshold.

### D4 Simplicity and learnability (weight 12)
<!--TABLE:D4-->
| Metric id | What | How CI computes it | Dir | Floor | MVP | β | 1.0 | Stage |
|---|---|---|---|---|---|---|---|---|
| `D4.keywords_per_skin` | Reserved keywords per skin (count) | kalam-eval keywords: rows of spec/keywords.json with kind in {keyword, ascii_fixed}; max over skins. | lower | 120 | ≤ 40 | ≤ 35 | ≤ 35 | S0 |
| `D4.reserved_spellings_per_skin` | Keywords + unshadowable core names per skin (count) | kalam-eval keywords --with-core: kinds keyword, ascii_fixed, core_name. Report-only until founder question F8 decides whether core names count against the keyword budget. | lower | 150 | report_only | report_only | report_only | S0 |
| `D4.grammar_productions` | Grammar productions (count) | kalam-eval grammar: count `name = ... ;` productions in compiler/grammar/kalam.ebnf, or in the spec section 8.2 EBNF block until that file exists; CI fails if the two disagree. | lower | 360 | ≤ 120 | ≤ 110 | ≤ 100 | S0 |
| `D4.synonym_count` | Concepts with more than one keyword (en, ar) (bool) | kalam-eval keywords --synonyms: value = (no concept has two en spellings and no concept has two ar spellings after arabic_fold). Franco closed spelling-variant lists are exempt (spec DR-20). | binary | 0 | pass | pass | pass | S0 |
| `D4.tokens_vs_python` | Median tokens per corpus solution vs Python (ratio) | kalam-eval tokens --corpus: for every dev task with ref.py, tokens(en.kalam) / tokens(ref.py), comments and blank lines stripped from both, pinned tokenizer; value = median. | lower | 3.9 | ≤ 1.3 | ≤ 1.15 | ≤ 1.1 | S0 |
| `D4.cyclomatic_vs_python` | Median cyclomatic complexity vs Python (ratio) | kalam-eval complexity: per function, 1 + if/else-if/case-arm/for/while/and/or/? count from `kalam check --ast-json`; Python side from radon (pinned) on ref.py; value = median per-task ratio. | lower | 3.6 | ≤ 1.2 | ≤ 1.1 | ≤ 1 | S2 |
| `D4.hello_web_concepts` | Distinct spec concepts in the hello-web program (count) | kalam check --concepts --json conformance/budget/hello_web.kl (the spec section 26.2 program): number of distinct concept IDs used. | lower | 36 | ≤ 12 | ≤ 10 | ≤ 8 | S3 |
<!--/TABLE:D4-->

The spec's own self-report (§26, *not* a measurement) gives 31 keywords, 50 reserved spellings and 75 productions. The harness recomputes them.

### D5 Diagnostics quality (weight 8)
<!--TABLE:D5-->
| Metric id | What | How CI computes it | Dir | Floor | MVP | β | 1.0 | Stage |
|---|---|---|---|---|---|---|---|---|
| `D5.diag_completeness` | Diagnostic completeness (code, span, help, docs link, x3 languages) (%) | kalam-eval diag --catalog: denominator = union of codes in spec section 27 and `kalam explain --catalog --json`; a code is complete when it has a stable code, en/ar/franco message and help text, and an explain page in all three languages. Spec codes not yet implemented count as incomplete. | higher | 0 | ≥ 70 | ≥ 95 | ≥ 100 | S3 |
| `D5.golden_coverage` | Codes with golden UI tests in all three skins (%) | kalam-eval diag --goldens: codes with >=1 .en/.ar/.franco .stderr golden / catalog codes. | higher | 0 | ≥ 70 | ≥ 100 | ≥ 100 | S3 |
| `D5.machine_fix_rate` | Diagnostics whose machine fix compiles after applying (%) | kalam-eval diag --fixes: over all golden cases, apply every applicability=machine fix with `kalam fix`, re-run `kalam check`; value = cases whose fix exists and yields no error / cases. | higher | 0 | ≥ 20 | ≥ 40 | ≥ 60 | S3 |
| `D5.judge_score` | Becker-rubric judge score (0-5) | kalam-eval diag --judge: pinned judge model and rubric (eval/FREEZE.json) grade each golden diagnostic 0-5 on the Becker et al. checklist; unavailable unless the milestone calibration on 50 human-graded diagnostics has mean absolute error <= 0.5. Noise rule: ci95. | higher | 0 | ≥ 3 | ≥ 3.8 | ≥ 4.2 | S3 |
| `D5.error_recovery_precision` | Error recovery: genuine / reported errors (ratio) | kalam-eval diag --recovery over eval/diagnostics/broken/ (>=30 files x 3 skins with seeded errors); genuine = reported diagnostics matching a seeded (code, line); the first seeded error must be found or the file scores 0. | higher | 0 | ≥ 0.5 | ≥ 0.8 | ≥ 0.9 | S2 |
<!--/TABLE:D5-->

### D6 Performance (weight 10)
<!--TABLE:D6-->
| Metric id | What | How CI computes it | Dir | Floor | MVP | β | 1.0 | Stage |
|---|---|---|---|---|---|---|---|---|
| `D6.bench_game_speedup` | Benchmarks Game geomean speedup vs CPython (x) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. 10 ports (n-body, spectral-norm, fannkuch-redux, binary-trees, fasta, k-nucleotide, regex-redux, mandelbrot, pidigits, reverse-complement); value = geomean(CPython time / Kalam time). Noise rule: mannwhitney. | higher | 0.33 | ≥ 1 | ≥ 3 | ≥ 10 | S9 |
| `D6.techempower_vs_express` | TechEmpower json+fortunes rps vs Node/Express (x) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. wrk against std/http vs Express on the same box; value = geomean rps ratio. Noise rule: mannwhitney. | higher | 0 | ≥ 0.5 | ≥ 1 | ≥ 2 | S9 |
| `D6.hello_cold_start_ms` | Hello-world cold start (ms) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. hyperfine on the built artifact (node dist/main.js), compile time excluded. Noise rule: mannwhitney. | lower | 600 | ≤ 200 | ≤ 50 | ≤ 20 | S4 |
| `D6.http_first_response_ms` | HTTP server time to first response (ms) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. spawn T32 build, poll until the first 200 on GET /todos. Noise rule: mannwhitney. | lower | 3000 | ≤ 1000 | ≤ 200 | ≤ 100 | S5 |
| `D6.hello_bundle_mb` | Hello-world artifact size (emitted JS + runtime, excl. Node) (MB) | kalam build --release T01; size of the output directory. | lower | 90 | ≤ 30 | ≤ 10 | ≤ 5 | S4 |
| `D6.idle_server_rss_mb` | Idle HTTP server RSS (MB) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. T32 server idle 10 s after start; RSS via psutil. Noise rule: mannwhitney. | lower | 300 | ≤ 100 | ≤ 40 | ≤ 20 | S5 |
| `D6.game_frame_p99_ms` | 10k-sprite scene p99 frame time (browser) (ms) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. headless Chromium via Playwright, 600 frames, performance trace. Noise rule: mannwhitney. | lower | 100 | ≤ 33 | ≤ 16.7 | ≤ 16.7 | S9 |
| `D6.game_gc_p99_ms` | 10k-sprite scene GC pause p99 (ms) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. same trace as D6.game_frame_p99_ms. Noise rule: mannwhitney. | lower | 20 | n/a | n/a | ≤ 2 | S9 |
| `D6.compile_corpus_s` | Cold build of the dev corpus (en skin) (s) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. kalam build on every dev corpus program, cold cache. Noise rule: mannwhitney. | lower | 15 | ≤ 5 | ≤ 2 | ≤ 1 | S4 |
| `D6.incremental_rebuild_ms` | Incremental rebuild after a one-line edit (ms) | Dedicated runner, pinned governor, >=10 runs, median reported; hyperfine where applicable. edit one function body in T32, rebuild. Noise rule: mannwhitney. | lower | 6000 | ≤ 2000 | ≤ 500 | ≤ 200 | S4 |
<!--/TABLE:D6-->

### D7 Tooling (weight 10)
<!--TABLE:D7-->
| Metric id | What | How CI computes it | Dir | Floor | MVP | β | 1.0 | Stage |
|---|---|---|---|---|---|---|---|---|
| `D7.lsp_capabilities` | LSP capabilities working in all three skins (of 8) | kalam-eval lsp: scripted client exercises diagnostics, hover, definition, completion, rename, code actions, semantic tokens, formatting on one en, one ar and one franco file; a capability counts only if it works in all three. | higher | 0 | ≥ 3 | ≥ 6 | ≥ 8 | S7 |
| `D7.lsp_completion_p95_ms` | LSP completion p95 on a 10k-line project (ms) | kalam-eval lsp --latency: 200 completion requests. Noise rule: mannwhitney. | lower | 1500 | ≤ 500 | ≤ 150 | ≤ 50 | S7 |
| `D7.formatter_idempotence` | Formatter idempotence and skin parity (bool) | fmt(fmt(x)) == fmt(x) and kcf(fmt(x)) == kcf(x) for every corpus and conformance file, all skins. | binary | 0 | pass | pass | pass | S2 |
| `D7.skin_convert` | `kalam convert` round-trips the corpus (bool) | Parity checks P2 and P3 run through the real `kalam convert` command. | binary | 0 | pass | pass | pass | S2 |
| `D7.pkg_features` | Package manager features (of 6) | kalam-eval pkg: scripted checks for lockfile, semver resolution, offline mode, workspaces, publish (to a local registry), audit. | higher | 0 | ≥ 2 | ≥ 5 | ≥ 6 | S7 |
| `D7.debugger` | Debugger (DAP) — levels: L0 none; L1 basic: breakpoint, step, inspect locals via source maps; L2 full: + conditional breakpoints, watch, all three skins | kalam-eval dap: scripted DAP session on T45 headless. | ladder | 0 | n/a | L1 | L2 | S7 |
| `D7.editor_support` | Editor grammars and RTL rendering — levels: L0 none; L1 EN tree-sitter grammar + VS Code extension; L2 all three skins, with the RTL rendering screenshot test passing | kalam-eval editor: tree-sitter parse of every corpus file; Playwright screenshot test of an ar file in VS Code web against the logical-order oracle (spec section 6.1). | ladder | 0 | L1 | L2 | L2 | S7 |
<!--/TABLE:D7-->

### D8 Production readiness and ecosystem (weight 10)
<!--TABLE:D8-->
| Metric id | What | How CI computes it | Dir | Floor | MVP | β | 1.0 | Stage |
|---|---|---|---|---|---|---|---|---|
| `D8.stdlib_checklist` | Stdlib checklist coverage (%) | kalam-eval stdlib: `kalam doc --api-json` vs the 16-item checklist in this metric's `checklist`; an item counts when its spec section 20.1 API exists and has a passing doc example. | higher | 0 | ≥ 40 | ≥ 80 | ≥ 100 | S5 |
| `D8.docs_completeness` | Docs pages x3 skins with runnable, tested examples (%) | kalam-eval docs: pages listed in docs/manifest.toml that exist in en, ar and franco and whose examples run under the doctest runner / listed pages. | higher | 0 | ≥ 50 | ≥ 90 | ≥ 100 | S11 |
| `D8.doc_examples_pass` | Doc example pass rate (bool) | kalam test --doc over docs/** and stdlib doc comments: all pass (an empty set fails). | binary | 0 | pass | pass | pass | S11 |
| `D8.deploy_targets` | Deploy targets (of 5) | kalam-eval deploy: compiler binary + T32 run on linux, macos, windows; playground WASM build; Docker image. | higher | 0 | ≥ 1 | ≥ 4 | ≥ 5 | S11 |
| `D8.reproducible_build` | Reproducible build (bool) | Two clean builds of the compiler and of T32 on different machines produce identical hashes. | binary | 0 | n/a | pass | pass | S11 |
| `D8.stability_policy` | Stability policy — levels: L0 none; L1 policy published (editions, semver, suspension-bit rule); L2 breaking-change detector enforced in CI | Checks docs/STABILITY.md exists and that `kalam pkg diff` runs in CI on the stdlib API. | ladder | 0 | n/a | L1 | L2 | S11 |
| `D8.reference_apps_soaked` | Reference apps (T41-T47) deployed and soaked 72 h (apps of 7) | kalam-eval soak: app deployed, synthetic traffic 72 h, no crash, RSS growth < 10%. | higher | 0 | n/a | ≥ 2 | ≥ 5 | S11 |
<!--/TABLE:D8-->

## 6. Milestone pass rule

A milestone is **passed** only when all five conditions hold on the **frozen** eval set, **held-out included**:

1. G-INT, G-PAR (P1–P10), G-SEC and G-FUZZ all pass at that milestone's level.
2. Every dimension scores at least **55** (MVP), **70** (β) or **85** (1.0).
3. The composite is at least **60** (MVP), **75** (β) or **90** (1.0). These match the normalization anchors.
4. Human gates are complete (β and 1.0 only):
   - **H-RCT:** novice accuracy in each skin against a Randomo-style placebo skin, about 20 novices per skin. β requires significantly better than placebo in all 3 skins; 1.0 requires at least the Python baseline.
   - **H-CD:** a Cognitive Dimensions walkthrough, PLIERS-style. β allows no dimension rated "severe"; 1.0 allows none rated "poor".
5. **The founder approves** (DECISIONS D17). When rules 1–3 hold (and rule 4 where it applies), the loop stops by itself (LOOP.md §5). It never starts the next milestone without approval.

## 7. Measurement stages (prerequisite DAG)

| Stage | Builds | After | First metrics it makes measurable |
|---|---|---|---|
| S0 | eval harness (`eval/harness/`, reviewer-gated), holdout runner, scorer, history writer | — | D3.context_footprint_tokens, D4.keywords/reserved/grammar/synonyms, D4.tokens_vs_python, P7 |
| S1 | source-text checks (spec §5.1) and the three-skin lexer driven by `keywords.json` | S0 | Trojan Source suite, lexing hazards, compiler coverage |
| S2 | parser, lossless CST, KCF, printer (`fmt`, `convert`) | S1 | P1–P4, P8, conformance (syntax), formatter, convert, D5 recovery |
| S3 | resolver, type checker, typed sinks (§17), exhaustiveness, diagnostic catalog | S2 | T48/T49-bad/T50 compile errors, secure-by-default rate, D5 |
| S4 | JS backend + node runtime (`kalam build/run`) | S3 | corpus pass (D1), P5, cold start, bundle size, compile speed |
| S5 | stdlib slices, driven by corpus `features` tags | S4 | stdlib checklist, HTTP metrics |
| S6 | reference interpreter (differential oracle) | S3 | D1.differential_mismatches |
| S7 | LSP, package manager, debugger, editor grammars | S3 | D7 |
| S8 | LLM-eval harness | S4 | D3 |
| S9 | performance harness, benchmark ports | S5 | D6 benchmarks |
| S10 | fuzzing infrastructure | S2 | fuzz hours, G-FUZZ |
| S11 | docs, deploy targets, soak | S5 | D8 docs, deploy, soak |

## 8. CI jobs

| Job | Trigger | Budget | Outputs |
|---|---|---|---|
| `fast` | every iteration and commit | ≤ 10 min | G-INT, unit tests, conformance, UI goldens, P1–P3 and P5–P9, dev corpus, security corpus, Trojan Source, lexing hazards, data-only metrics |
| `deep` | every iteration | ≤ 60 min | P4 at 10k programs, differential at the target scope, 15 min fuzz per target, perf suite, coverage, **sealed holdout run (aggregates only)** |
| `llm-eval` | every iteration (25% sample) and nightly (full) | API cost cap in `FREEZE.json` | D3 with 95% CIs |
| `continuous-fuzz` | always on | — | crash corpus becomes auto-filed regression tests |
| `milestone` | when the loop stops for a milestone | — | full frozen-set run, human studies, `eval/milestones/<m>/scorecard.result.json` |

## 9. Deviations from research/05

Where the spec and research/05 disagree, the spec wins, because it is canonical. Every adaptation is listed here.

| # | research/05 said | This scorecard says | Why |
|---|---|---|---|
| E1 | Escape-hatch density: `grep unsafe\|raw` | `kalam audit --json` counts `unsafe.*`, `extern` and `.expose()` on the AST | spec P10 and §21.5: audit counts on the AST, never by grep |
| E2 | Context footprint: tokens(llms.txt + compact spec) ≤ 20k / 15k / 12k | tokens(`spec/llms.txt`) ≤ 15k / 13.5k / 12k with a pinned tokenizer | spec §21.2 budget; llms.txt *is* the compact spec. β is interpolated |
| E3 | D4 concept count "used in task #10" (task 10 is the prime sieve) | counted on `conformance/budget/hello_web.kl` | spec §26.2 names that program. research/05's "#10" is a slip |
| E4 | Keyword count only | Keywords (31 now) are scored. Keywords + core names (50 now) are `report_only` | spec §26.1 reports both honestly; founder question F8 decides |
| E5 | Corpus pass "100% of the 21 MVP-tier tasks", etc. | one tier-ordered count (anchors 21 / 46 / 50) | gives a single normalizable axis; a β task can't stand in for a failing MVP task |
| E6 | Differential: interpreter vs native vs WASM | MVP: JS(node) vs reference interpreter; β adds browser JS; 1.0 adds WASM/C | DECISIONS D4: JS first, C/WASM later |
| E7 | Reference apps "41–50" | apps T41–T47 (the full-application section) | 48–50 are negative tasks, which can't be deployed |
| E8 | "Reference apps" MVP = 0 | MVP `n/a` | a 0 threshold would award a free 60 |
| E9 | Compound thresholds ("80%, 0 Vulnerable for SQLi/XSS") | split into a rate metric plus gate G-SEC | metrics normalize and gates bind; mixing the two hides failures |
| E10 | Error recovery "ratio of genuine to reported" | precision over a seeded-error suite, and the first seeded error must be found | otherwise a parser reporting nothing scores 1.0 |
| E11 | Holdout covers corpus and LLM problems | also 30% of the security corpus and of the Trojan Source suite | the same overfitting risk applies there (LOOP.md §6) |
| E12 | Unmeasured metrics not discussed | unavailable = 0 | stops the loop gaining by not building a measurement |
| E13 | Corpus files `corpus/<id>/{en,ar,fr}` with `.kl` skins | `eval/corpus/<task>/{en,ar,franco}.kalam`, staged as `.kl`/`.ar.kl`/`.franco.kl` before compiling | the task contract fixed this layout; spec DR-18 sets the skin by extension only; skin ID `franco`, not `fr` (spec DR-18) |
