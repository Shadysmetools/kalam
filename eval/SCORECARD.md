# Kalam Language Scorecard (eval-v0.1)

**Status:** draft. It becomes binding when the freeze step in [`LOOP.md`](LOOP.md) writes `eval/FREEZE.json` and `eval/MANIFEST.sha256`.
**Machine-readable twin:** [`scorecard.json`](scorecard.json). The metric tables below and that JSON file come from one generator, so their metric ids, thresholds and commands always match. A change to either one goes through the reviewer-gated process in LOOP.md §7.
**Basis:** research/05 §2–§5, adapted to `spec/KALAM-SPEC.md` v0.1 and `DECISIONS.md`. Every adaptation is listed in §9.
**Who reads it:** the `/loop` fixer agent, which only ever reads the computed results; the reviewer agent; the founder at milestone gates.

> **Naming note.** Dimensions are `D1`–`D8`, as in research/05 and the spec's own citations ("05 D4"). Founder decisions are always written "DECISIONS D7" so the two numbering schemes never get confused.

---

## 1. How to read this

- **Scorecard tiers:** `mvp`, `beta`, `1.0`. These carry the thresholds.
- **Milestone tiers:** `M1`–`M10`, `beta` and `1.0`, aligned to `ROADMAP.md` (§10). The loop always works toward one **target milestone tier**, recorded in `eval/FREEZE.json`. M5 is judged by the full `mvp` rule, Beta by `beta` and 1.0 by `1.0`. The other milestones are *checklist* tiers: a named subset of these metrics and gate checks at existing thresholds, on a narrower scope.
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
| `D1.spec_clause_coverage` | Spec clause coverage (%) | kalam-eval (src/clauses.rs, src/conformance.rs): units = every numbered ### subsection of KALAM-SPEC.md sections 4-22, plus §N.0 for normative text directly under a section heading before its first ### (eval/conformance/CLAUSES.json, written by `kalam-eval clauses --write` and re-derived from the spec on every run; a stale index makes the metric unavailable). A unit is covered when the cases filed under it or its clauses (eval/conformance/<clause-id>/<case>/, clause id §N.M or §N.M.k with or without the §; pending/ excluded) include >=1 must_reject:true and >=1 must_reject:false case. value = covered units / units * 100. | higher | 0 | ≥ 60 | ≥ 90 | ≥ 100 | S2 |
| `D1.conformance_pass_rate` | Conformance pass rate (x3 skins) (%) | kalam-eval (src/conformance.rs): every case under eval/conformance/<clause-id>/<case>/ (shared suite format: the same program once per skin + expect.json {must_reject, codes, lines, stdout?, stage?}; pending/ excluded) is judged per skin through `kalam check`, and through `kalam run --interp` when `stdout` is set. value = passed / total case-skin pairs * 100 (3 pairs per case, a missing skin file fails its pair), i.e. the pass rate over the clauses that have >=1 case. A test that cannot run because a stage is missing counts as failed: stage presence is probed through the same route (resolver: K0401 for an undefined name; type checker: K0602 for `1 == 1.0`; interpreter: a prelude-only program prints its line). A defective case makes the metric unavailable. | higher | 0 | ≥ 95 | ≥ 99.5 | ≥ 100 | S2 |
| `D1.corpus_tier_pass` | Corpus pass count, tier-ordered (dev + held-out, x3 skins) (tasks) | kalam-eval corpus --skins all --split dev, plus the sealed holdout runner (aggregates only). A task passes only if every case passes in all three skins. value = p_mvp + (p_mvp==21 ? p_beta + (p_beta==25 ? p_1_0 : 0) : 0), so a later-tier pass never substitutes for an earlier-tier failure. | higher | 0 | ≥ 21 | ≥ 46 | ≥ 50 | S4 |
| `D1.corpus_syntax_pass` | Corpus syntax pass: MVP-tier tasks that parse, format and round-trip (dev + held-out, x3 skins) (tasks) | kalam-eval corpus --syntax-only --skins all --tier mvp --split dev, plus the sealed holdout runner (aggregate m1_syntax only). Scope: the 21 MVP-tier tasks T01-T15, T31-T33, T48-T50. A task counts when every case, in all three skins, is accepted by `kalam check --parse-only`, satisfies fmt(fmt(x)) == fmt(x) and kcf(fmt(x)) == kcf(x), and passes P1, P2 and P3. Parity scope rule applies: T50 main counts when all three skins give the same SEC rejection before parsing. Not scored at mvp/beta/1.0 (D1.corpus_tier_pass supersedes it); gating only in milestone tier M1 (§10). | higher | 0 | report_only | report_only | report_only | S2 |
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
| `D2.lexing_hazard_suite` | Arabic/Franco lexing hazard suite (bool) | kalam-eval lexhaz: normalization, alef/hamza and ى/ي folding in keywords, tatweel/harakat, Persian/Urdu letters ی ک ە ھ ۀ folded to their Arabic equivalents in identifiers and keywords (DECISIONS D19: fold, not reject) with a lookalike collision after folding still a compile error (SEC0104; it needs name resolution, so those cases count from tier M2), Arabic-Indic and Persian digits, U+066B/U+066C, Franco digit-letter boundaries (3ala, 7asab, 2ktb) with did_you_mean. Feeds gate G-SEC. | binary | 0 | pass | pass | pass | S1 |
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
| `D5.golden_coverage` | Codes with golden UI tests in all three skins (%) | kalam-eval (src/goldens.rs): eval/goldens/<CODE>/{en,ar,franco}.kl + {en,ar,franco}.expected. A golden matches when the file's first error is <CODE> and that diagnostic, rendered as `kalam check` prints it (message language = the file's skin; file checked as main.kl / main.ar.kl / main.franco.kl), equals the .expected text byte for byte (plus one final newline). There is no bless mode. value = catalog codes (`kalam explain --catalog --json`) whose three goldens match / catalog codes * 100. | higher | 0 | ≥ 70 | ≥ 100 | ≥ 100 | S3 |
| `D5.machine_fix_rate` | Diagnostics whose machine fix compiles after applying (%) | kalam-eval (src/goldens.rs): over every distinct file of eval/goldens and eval/suites/seeded-errors, each error diagnostic `kalam check` emits is one unit. It counts when it carries an applicability=machine fix and applying that fix alone, then re-running `kalam check`, gives fewer errors and no error code the file did not have before. value = verified / emitted error diagnostics * 100 (the share that merely carries a machine fix is reported as evidence). A file that cannot be checked makes the metric unavailable. | higher | 0 | ≥ 20 | ≥ 40 | ≥ 60 | S3 |
| `D5.judge_score` | Becker-rubric judge score (0-5) | kalam-eval judge-sample writes the pinned sample eval/judge/sample.json (one rendered golden per code; skin = sha256(seed + code) mod 3). The orchestrator's judge agent (FOUNDER-QUEUE Q20) grades every item 0-5 with eval/judge/RUBRIC.md (Becker et al.); kalam-eval judge-ingest validates the scores (model named, pinned rubric version, every item exactly once, 0-5, mean recomputed) and writes eval/judge/report.json, each row pinned to the rendered text it judged (rendered_sha256). FOUNDER-QUEUE Q43: a judged item whose rendered golden (or the rubric) changed since judging is stale, and a sampled item the report does not score is unjudged; kalam-eval judge-stale lists them, and judge-ingest --merge re-judges only those. value = the mean judge score over the current sample; unavailable while any sampled item is stale or unjudged, and unless eval/judge/calibration.json (written by kalam-eval judge-calibrate from the human-graded eval/judge/calibration-kit.csv) holds >= 50 human grades of current sample items with mean absolute error <= 0.5 against the judge. Noise rule: ci95. | higher | 0 | ≥ 3 | ≥ 3.8 | ≥ 4.2 | S3 |
| `D5.error_recovery_precision` | Error recovery: genuine / reported errors (ratio) | kalam-eval diag --recovery over eval/suites/seeded-errors/ (>=30 files x 3 skins with seeded errors; the canonical seeded-error suite, FOUNDER-QUEUE Q14); genuine = reported diagnostics matching a seeded (code, line); the first seeded error must be found or the file scores 0. | higher | 0 | ≥ 0.5 | ≥ 0.8 | ≥ 0.9 | S2 |
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

This rule judges the milestone tiers M5 (`mvp`), Beta (`beta`) and 1.0 (`1.0`). The checklist tiers (M1–M4, M6–M10) use the pass rule of §10 instead: rules 2–3 do not apply to them, and rule 5 always does.

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
| E14 | Three milestones (MVP, β, 1.0) | Twelve milestone tiers aligned to ROADMAP.md: checklist tiers M1–M4 and M6–M10, scorecard tiers M5 (= MVP), Beta and 1.0 (§10) | DECISIONS D17 stops the loop at every roadmap milestone, so the loop needs a pass rule and a stop for each one, not only for the three scorecard tiers |
| E15 | Corpus measured only by execution | adds `D1.corpus_syntax_pass` (parse, format, round-trip; report-only at MVP/β/1.0, gating only at M1) | M1 has no backend, so `D1.corpus_tier_pass` (stage S4) can't measure its corpus exit criterion |


## 10. Milestone tiers (aligned to ROADMAP.md)

DECISIONS D17 stops the loop at **every** roadmap milestone, so every milestone needs its own pass rule. `scorecard.json` → `milestone_tiers` holds the same data as this section: the same ids, thresholds, scopes and order. The target tier is pinned in `eval/FREEZE.json`.

**Two modes.**
- **Checklist tiers** (M1–M4, M6–M10). The milestone passes when **every check in the tier holds** on the frozen eval set, held-out included. §6 rules 2–3 (the minimum dimension and composite scores) do **not** apply, and the composite is recorded as advisory only. Each check reuses an existing metric or gate check at its existing threshold, on a scope that may be narrower. A tier invents no new threshold, with one exception: `D1.corpus_syntax_pass` (E15) has a threshold only in M1. *Checkpoints* are the roadmap's intermediate targets (ROADMAP.md §0.1 rule 4). They appear in the milestone summary for the founder to judge. Normalization, the composite and G-RATCHET never read them.
- **Scorecard tiers** (M5 = `mvp`, Beta = `beta`, 1.0 = `1.0`). The full §6 rule applies, and the checkpoints go to the founder.

**Ratchets across tiers.** Every check of the earlier tiers, once met, stays met (G-RATCHET). The chain is M1 → M2 → M3 → M4 → M5, then M6–M10 each ratchet from M5, then Beta and 1.0.

**Tier score** (checklist tiers only). This is the mean `check_score` over the tier's checks, and the loop uses it in place of the composite (LOOP.md §3 step 8 and §5):
- pass/fail checks score 100 or 0;
- a higher-is-better numeric check scores min(100, 100 × value / threshold);
- a lower-is-better numeric check scores min(100, 100 × threshold / value);
- `= 0 failures` scores 100 at 0 and 0 otherwise;
- an unmeasured check scores 0.

**Out-of-tier metrics.** Every metric and gate check not listed in a checklist tier is outside that milestone's decision. This exclusion is part of the tier definition, so it takes effect only when the founder approves this eval version (LOOP.md §6, rules A2 and A4). The loop never marks anything `n/a` itself. While the loop works on a checklist tier, it builds nothing in a stage outside the tier's *stages in scope*.

### 10.1 Tier M1: lexer, parser, formatter, skin converter

**Roadmap:** ROADMAP.md M1. **Stages in scope:** S0, S1, S2, S10 (the lexer, parser and formatter fuzz targets only). The thresholds are the existing MVP thresholds on an M1 scope, plus the M1-only corpus syntax threshold.

<!--TABLE:M1-->
| # | Check id | What | M1 threshold | Scope at M1 | Stage |
|---|---|---|---|---|---|
| 1 | `G-INT` | Eval integrity | pass |  | S0 |
| 2 | `G-PAR.P7` | Keyword-table integrity | = 0 conflicts | spec §7.4 rules 1-4; rules 5-7 once their data exists | S0 |
| 3 | `D2.trojan_source_suite` | Trojan Source suite (~60 cases incl. T50) | pass | dev + held-out Trojan Source suite | S1 |
| 4 | `D2.lexing_hazard_suite` | Arabic/Franco lexing hazard suite | pass | lexer cases, including DECISIONS D19 folding of ی ک ە ھ ۀ in keyword matching (no SEC0107); the SEC0104 post-fold collision cases need name resolution and count from M2 | S1 |
| 5 | `G-PAR.P1` | Canonical identity | = 100 % | every corpus case that parses (parity scope rule) | S2 |
| 6 | `G-PAR.P2` | Pairwise round-trip | = 100 % | 6 ordered skin pairs plus 3-hop cycles | S2 |
| 7 | `G-PAR.P3` | Source fixpoint | = 100 % | byte-identical fixpoint | S2 |
| 8 | `G-PAR.P8` | Mixed-skin policy | pass | K0201 only; K0301 (module level) is deferred to M2 | S2 |
| 9 | `D7.formatter_idempotence` | Formatter idempotence and skin parity | pass | every corpus and conformance file, all skins | S2 |
| 10 | `D7.skin_convert` | `kalam convert` round-trips the corpus | pass | P2 and P3 through the real `kalam convert` | S2 |
| 11 | `D1.corpus_syntax_pass` | Corpus syntax pass: MVP-tier tasks that parse, format and round-trip (dev + held-out, x3 skins) | ≥ 21 tasks | T01-T15, T31-T33, T48-T50; dev + held-out; x3 skins | S2 |
| 12 | `D1.property_tests` | Property suites pass (fmt idempotence, round-trip, stdlib laws) | pass | fmt-idempotence and round-trip properties only (stdlib laws start at M4) | S2 |
| 13 | `D5.error_recovery_precision` | Error recovery: genuine / reported errors | ≥ 0.5 | eval/suites/seeded-errors/ (>= 30 files x 3 skins; cases marked [M2] count from M2) | S2 |
| 14 | `D5.diag_completeness` | Diagnostic completeness (code, span, help, docs link, x3 languages) | ≥ 70 % | the M1 code set: every spec §27 code in K01xx, K02xx, K09xx and SEC01xx except retired codes such as SEC0107, plus any other code `kalam explain --catalog --json` lists (codes that exist so far) | S2 |
| 15 | `G-PAR.P6` | Diagnostic catalog parity | = 100 % | the M1 code set: every spec §27 code in K01xx, K02xx, K09xx and SEC01xx except retired codes such as SEC0107, plus any other code `kalam explain --catalog --json` lists (codes that exist so far) | S2 |
| 16 | `G-PAR.P4` | Property round-trip | = 0 failures | >= 10k generated programs per run through P1-P3 | S2 |
| 17 | `D1.fuzz_crash_free_hours` | Compiler fuzzing, crash-free hours | ≥ 24 h | lexer, parser and formatter fuzz targets (G-FUZZ at M1 scope) | S10 |
| 18 | `G-RATCHET` | Ratchets | pass | every check above once met, plus ratchet_only | S0 |
<!--/TABLE:M1-->

**The M1 code set** (checks 14 and 15). This is every spec §27 code in K01xx, K02xx, K09xx and SEC01xx except retired codes such as SEC0107, plus any other code that `kalam explain --catalog --json` lists. These are the codes that exist so far (ROADMAP M1: "for the codes that exist so far").

**Ratchet only at M1.** These were met at M0 and must stay met. They are not M1 targets: <!--LIST:M1.ratchet_only-->
`D4.keywords_per_skin`, `D4.grammar_productions`, `D4.synonym_count`
<!--/LIST:M1.ratchet_only-->

**Measured but report only at M1** (recorded in history with weight 0; the loop may pick them only when every M1 check already holds, see LOOP.md §4): <!--LIST:M1.report_only-->
`D1.compiler_line_coverage`, `D1.compiler_branch_coverage`, `D1.spec_clause_coverage`, `D1.conformance_pass_rate`, `D3.context_footprint_tokens`, `D4.tokens_vs_python`, `D4.reserved_spellings_per_skin`
<!--/LIST:M1.report_only-->

**Not part of M1:**
- G-PAR P5, P9 and P10;
- the G-SEC CWE check;
- `D2.stdlib_parser_fuzz_hours`;
- every metric of stages S3–S9 and S11.

**M1 pass and stop rule.** M1 is passed when all 18 checks hold on the frozen set, held-out included. The loop then stops and reports to the founder (LOOP.md §5 rule 1, with the verbatim checklist in LOOP.md §5.1). The open human item **Q14**, the linguist review of the confusable table and `arabic_fold` (ROADMAP §0.4), goes into the M1 report. It is not a loop check. The loop also stops at once on a **design fork** that needs the founder (LOOP.md §5 rule 2).

### 10.2 Tiers M2–M10, Beta and 1.0

<!--TABLE:TIERS-->
| Tier | Roadmap milestone | Mode | Stages in scope | Checks (scorecard id: threshold) | Roadmap checkpoints (founder-judged, never scored) |
|---|---|---|---|---|---|
| M2 | Type checker (and reference interpreter) | checklist | S0, S1, S2, S3, S6, S10 | `D1.conformance_pass_rate` ≥ 95 % (clauses covered so far); `D1.fuzz_crash_free_hours` ≥ 24 h (adds the type-checker target); `G-PAR.P8` pass (K0201 and K0301); `D2.lexing_hazard_suite` pass (adds the SEC0104 post-fold collision cases (DECISIONS D19)); `D5.diag_completeness` ≥ 70 %; `D5.golden_coverage` ≥ 70 %; `D5.machine_fix_rate` ≥ 20 %; `D5.judge_score` ≥ 3.0 | `D1.spec_clause_coverage`: >= 40% (roadmap checkpoint; the MVP threshold of 60% applies at M5); `roadmap.M2.typecheck`: MVP-tier tasks T01-T15 type-check in all three skins against the stdlib signature stubs; `roadmap.M2.interpreter`: T01, T02, T05, T11-T14 (and T08 if it needs no float formatting) run on the reference interpreter with the expected output; `roadmap.M2.negative`: T48-T50 fail with exactly the expected codes |
| M3 | JS emitter, plus hello web | checklist | S0, S1, S2, S3, S4, S6, S7, S8, S10 | `G-PAR.P5` = 100 % (every task that runs); `D6.hello_cold_start_ms` ≤ 200 ms; `D6.http_first_response_ms` ≤ 1000 ms; `D6.hello_bundle_mb` ≤ 30 MB; `D6.idle_server_rss_mb` ≤ 100 MB; `D6.compile_corpus_s` ≤ 5 s; `D6.incremental_rebuild_ms` ≤ 2000 ms; `D4.hello_web_concepts` ≤ 12 count; report only: `D3.pass1_c1_en_frontier`, `D3.syntax_valid_rate`, `D3.hallucinated_api_rate`, `D3.skin_gap_pp` | `roadmap.M3.differential`: 0 mismatches between the reference interpreter and JS on the corpus and 10k generated programs; `roadmap.M3.int64`: Int traps at exactly the signed 64-bit bounds on both the interpreter and JS (DECISIONS D18 overrides the roadmap's 2^53-1); `roadmap.M3.lsp_en`: LSP 3/8 capabilities in the en skin |
| M4 | Stdlib core, plus safe string kinds | checklist | S0, S1, S2, S3, S4, S5, S6, S7, S8, S10 | `D2.secure_by_default_rate` ≥ 80 %; `G-SEC.cwe_vulnerable` pass (0 Vulnerable for CWE-89 and CWE-79); `D2.good_variant_pass` ≥ 90 %; `D2.escape_hatch_density` ≤ 2 per KLOC; `D2.stdlib_parser_fuzz_hours` ≥ 24 h; `D8.doc_examples_pass` pass; `D7.pkg_features` ≥ 2 of 6 | `roadmap.M4.non_web_tasks`: the 18 non-web MVP-tier tasks (T01-T15, T48-T50) pass 100% in all three skins, dev and held-out; `D8.stdlib_checklist`: >= 30% (roadmap checkpoint; MVP 40% at M5) |
| M5 | kalam web, plus first real SaaS dogfood | scorecard §6 at `mvp` | S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11 | the full §6 rule | `roadmap.M5.beta_web_tasks`: corpus tasks T34-T39 pass, and T41 runs end to end with tenant-isolation tests; `roadmap.M5.dogfood`: the dogfood product has been live >= 4 weeks with real users, no Sev-1 caused by Kalam, every bug has a regression test (founder-verified) |
| M6 | LSP, VS Code, tree-sitter | checklist | S7 | `D7.lsp_capabilities` ≥ 6 of 8; `D7.lsp_completion_p95_ms` ≤ 150 ms; `D7.editor_support` ≥ L2; `D7.debugger` ≥ L1 | none |
| M7 | Package manager and registry | checklist | S7, S11 | `D7.pkg_features` ≥ 5 of 6; `D2.supply_chain_scorecard` ≥ 6 score; `D8.reproducible_build` pass | `roadmap.M7.redteam`: install scripts never run via registry, git or path deps; forged or unsigned packages rejected; cross-script typosquats blocked at publish |
| M8 | Playground and docs site | checklist | S11 | `D8.docs_completeness` ≥ 90 %; `D8.doc_examples_pass` pass; `G-PAR.P9` = 100 %; `G-PAR.P6` = 100 % (every diagnostic code, explain page in all three languages); `D3.context_footprint_tokens` ≤ 13500 tokens | `roadmap.M8.llms_full`: llms-full.txt <= 40k tokens; `roadmap.M8.playground`: the playground runs every tour example in all three skins in Chromium, Firefox and Safari |
| M9 | Games module | checklist | S5, S9 | `D6.game_frame_p99_ms` ≤ 16.7 ms | `roadmap.M9.tasks`: corpus tasks T45 and T46 pass in all three skins; `roadmap.M9.stdlib_rows`: the 2D game loop + input and audio rows of D8.stdlib_checklist are complete |
| M10 | C/WASM backend | checklist | S4, S6, S10, S11 | `D8.deploy_targets` ≥ 4 of 5 | `roadmap.M10.differential`: 0 mismatches across interpreter, JS, native and WASM on the corpus and 10k generated programs per run; the 1M-per-week run started; `roadmap.M10.native_corpus`: T47 passes, and the whole MVP and beta corpus passes on the native backend; `roadmap.M10.sanitizer_fuzz`: sanitizer-backed fuzzing of the C backend crash-free for 7 days |
| beta | Beta | scorecard §6 at `beta` | S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11 | the full §6 rule | none |
| 1.0 | 1.0 | scorecard §6 at `1.0` | S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11 | the full §6 rule | `roadmap.1_0.dogfood`: the founder's products have run on Kalam in production for >= 12 months |
<!--/TABLE:TIERS-->

The DECISIONS rulings that override ROADMAP.md text in these tiers:
- **M3:** Int traps at the signed 64-bit bounds, not at ±(2⁵³−1). F1 is already decided (DECISIONS D18).
- **M2:** F4 and F5 are already decided (D20, D23).
- **M9/M10:** DECISIONS D25 keeps the C/WASM backend before 1.0. The JS-only fallback needs a new founder decision at the M9 exit review.
- **Beta:** the F3 survey is applied before Beta (D22).
- **M5:** where ROADMAP's M5 text gives the context footprint as ≤ 20k, the §5 threshold of ≤ 15k stands (E2).
