# The Kalam build loop: exact procedure per `/loop` iteration

**Applies to:** every `/loop` iteration that builds Kalam (DECISIONS D14, D17).
**Reads:** [`SCORECARD.md`](SCORECARD.md), [`scorecard.json`](scorecard.json), [`corpus/INDEX.md`](corpus/INDEX.md), `spec/KALAM-SPEC.md`, `DECISIONS.md`, `ROADMAP.md`. Where they conflict, `DECISIONS.md` wins over the spec and the roadmap. For example, D18–D25 override spec defaults.
**Writes:** code under `compiler/` (and `stdlib/`, `conformance/pending/`, `docs/`), one line in `eval/history.jsonl` per iteration, and `eval/iterations/NNNN/`.

The loop is **autonomous within a milestone** and **stops for founder approval at every milestone** (DECISIONS D17). It does exactly one slice per iteration.

The milestones are the **milestone tiers** of SCORECARD.md §10: `M1`–`M10`, `beta` and `1.0`, aligned to ROADMAP.md. Two kinds exist:
- **Checklist tiers** (M1–M4, M6–M10). A tier passes when every listed check holds. The loop builds only in the tier's `stages_in_scope`.
- **Scorecard tiers** (M5 = `mvp`, Beta, 1.0). These use the full SCORECARD.md §6 rule.

The current target tier is `target_milestone` in `eval/FREEZE.json`. The first one is **M1**.

---

## 0. Roles

| Role | Who | Can see | Can change |
|---|---|---|---|
| **Fixer** | the `/loop` agent | the repo except the holdout store | `compiler/`, `stdlib/`, `docs/`, `conformance/pending/`, `eval/iterations/NNNN/`; it appends to `eval/history.jsonl` and `eval/questions.jsonl` through the harness only |
| **Reviewer** | a separate agent session with its own prompt. It is given the spec, the diff of protected files and the new tests, and never the fixer's reasoning | the repo | approves or rejects changes to protected paths (§1), test promotions and `--bless`; records sign-off in `eval/reviews.jsonl` |
| **Holdout author** | a separate session that never fixes code | the spec and the held-out titles in INDEX.md | writes the sealed holdout store (§6.2) |
| **Holdout runner** | a script run as its own process, holding the decryption key | the sealed store and a built `kalam` binary | returns aggregate JSON only |
| **Founder** | human | everything | thresholds, milestone approval, DECISIONS, spec F-questions |

Only one iteration runs at a time. Step 0 takes the lock file `eval/.loop.lock`, which holds the PID and start time, and step 12 releases it. A lock older than 6 hours is stale: the loop logs it in `eval/questions.jsonl` and removes it.

## 1. Paths

| Path | Status |
|---|---|
| `eval/SCORECARD.md`, `eval/scorecard.json`, `eval/LOOP.md`, `eval/corpus/**`, `eval/harness/**`, `eval/diagnostics/**`, `eval/seccorpus/**`, `eval/FREEZE.json`, `eval/MANIFEST.sha256` | **protected:** reviewer sign-off required; frozen per eval version |
| `spec/**`, `DECISIONS.md`, `research/**` | **protected:** spec changes go through `spec/proposals/` and the reviewer; DECISIONS changes need the founder |
| `conformance/**` (except `pending/`), `**/*.stderr` goldens | **protected after creation:** the fixer may *add* files under `conformance/pending/`; promotion, edits, deletions and `--bless` need the reviewer |
| `eval/history.jsonl`, `eval/questions.jsonl`, `eval/reviews.jsonl` | **append-only**, written through `kalam-eval` |
| `eval/iterations/NNNN/` | fixer scratch for that iteration (plan, logs, raw results) |
| `eval/STOP` | when this file exists, every iteration exits at step 0 |
| holdout store `E:\knowcap-dev\6-R and D\kalam-holdout\` (outside the repo) | **sealed:** never read by the fixer (§6.2) |

## 2. Phase 0: bootstrap and freeze (once per eval version)

This phase runs before iteration 1, and again whenever the founder approves a milestone and a new eval version starts.

1. **Build S0 (the harness).** Write `eval/harness/` (Python 3.12, pinned `requirements.lock` including the tokenizer) with the subcommands named in SCORECARD.md §5:
   - `run --job fast|deep|llm-eval|milestone` (the job runner) and `holdout` (which launches the sealed holdout runner, §6.2);
   - `integrity`, `corpus`, `parity`, `differential`, `conformance`, `seccorpus`, `trojan`, `lexhaz`, `keywords`, `grammar`, `tokens`, `complexity`, `diag`, `llm`, `agent-apps`, `perf`, `fuzz-status`, `lsp`, `dap`, `editor`, `pkg`, `stdlib`, `docs`, `deploy`, `soak`;
   - `score`, which turns raw results plus `scorecard.json` into normalized, dimension and composite scores;
   - `history append`.

   The corpus runner implements INDEX.md §2–§3, including staging. The reviewer approves the harness. Until a subcommand's stage exists, it returns `status: unavailable`.
2. **Author the remaining pieces of the frozen set:**
   - the dev corpus tasks of the target tier (reviewer-gated);
   - the seed security corpus (`eval/seccorpus/`) and Trojan Source suite;
   - the seeded-error suite (`eval/diagnostics/broken/`);
   - the held-out tasks (holdout author, §6.2).
3. **Pin** in `eval/FREEZE.json`:
   - `eval_version` (for example `eval-v0.1`) and `target_milestone`, a tier id from `scorecard.json` → `milestone_tiers.order` (`M1` for eval-v0.1);
   - the model panel (exact model IDs plus date), the LLM-judge model and rubric hash, and the `spec/llms.txt` SHA-256;
   - the API cost cap;
   - the holdout manifest SHA-256 (§6.2).
4. **Freeze.** Run `kalam-eval integrity --write-manifest` to produce `eval/MANIFEST.sha256`, a SHA-256 of every protected file under `eval/`. The reviewer signs it, and it is committed as `eval: freeze eval-v0.1`.
5. **Baseline.** Run iteration 0, which is steps 1, 7 and 9 only, on the frozen HEAD. Its history line is the first baseline. Before the compiler exists, almost every metric is `unavailable`, and that is the honest starting point.

## 3. The iteration (do these steps in order; skip none)

**Step 0: Preflight.**
- If `eval/STOP` exists, print its reason and exit. Do not take the lock.
- Take the lock. Set `N` to the last `iteration` in `history.jsonl` plus 1.
- Create `eval/iterations/N/`.
- Make a worktree on branch `loop/iter-N` from the tip of `loop/<milestone>`. The first time, create `loop/<milestone>` from `master`.

**Step 1: Integrity (G-INT).**
- Run `kalam-eval integrity`. It checks the manifest, the append-only files, reviewer sign-offs for every protected-path change since the last iteration, and the corpus-leak scan.
- On failure: append a history line with `outcome: "integrity_fail"`, write `eval/STOP` with the reason, release the lock and exit. **Never repair the manifest yourself.**

**Step 2: Baseline.**
- If the last `accepted` history line has `commit == HEAD` of `loop/<milestone>` and the same `eval_version`, its results are the baseline.
- Otherwise run the `fast` and `deep` jobs plus the holdout runner on HEAD, and use their results as the baseline. This happens after a founder merge or a reviewer change.

**Step 3: Choose the target** (deterministic; §4 has the exact algorithm). Record `target` = a gate check, or a dimension plus a metric.

**Step 4: Choose the slice.**
1. If the target metric's `stage` (from `scorecard.json`), or any stage before it on the S0–S11 DAG, is missing, the slice is **the earliest missing stage on the path from S0 to that stage**. This is a *prerequisite slice*. A stage counts as present once its first metric reports `status: measured`.
   - **Scope fence (checklist tiers):** a slice may only build stages in the target tier's `stages_in_scope`. For M1 these are S0, S1, S2 and S10.
   - If a target seems to need an out-of-scope stage, the target is mis-scoped. Log it in `questions.jsonl` and pick the next target. Never build the out-of-scope stage.
2. If the target is `D1.corpus_tier_pass` or a parity check, pick the **failing dev task** of the lowest unfinished tier with the lowest task number. Its `task.toml` `features` and spec clauses define the slice.
3. Otherwise the slice is the smallest change that moves the target metric. Examples: one diagnostic family, one stdlib module, one LSP capability.
4. **Size limit:** about 600 changed lines excluding tests and generated files. One target metric. If more is needed, split the work and do only the first part.

Write `eval/iterations/N/plan.json` with these fields:
- `target`, `slice`, `stage`;
- `spec_clauses` (the § numbers the slice implements);
- `expected_effect` (which metrics should move, in which direction);
- `files` (the planned file list).

**Step 5: Spec check.** Every behaviour the slice implements must be cited to a spec clause.
- **Spec silent or contradictory** (for example INDEX.md §6 G1–G18): use the spec's stated *working default*. If there is none, use the corpus assumption already recorded. Append the question to `eval/questions.jsonl`. **Never invent semantics that a corpus expectation then depends on.**
- **Design fork: the slice needs a founder decision with no working default.** Examples:
  - two or more spec-consistent designs with user-visible, hard-to-reverse consequences (syntax, keywords, KCF format, diagnostic codes, skin rules);
  - a spec text that contradicts DECISIONS.md;
  - a corpus expectation that contradicts the spec.

  In that case:
  1. Do not build it, and do not pick another target.
  2. Append the question to `eval/questions.jsonl` with `kind: "design_fork"`. Include the options, the evidence (spec clauses, failing tests), the consequences of each option, and the loop's recommendation.
  3. Append a history line with `outcome: "stopped"`.
  4. Fire stop rule 2 (§5) and release the lock.

**Step 6: Build (TDD).**
1. First write failing tests:
   - unit tests in `compiler/`;
   - conformance tests in `conformance/pending/<area>/`, each carrying `clause: §x.y` front-matter and having a positive and a negative twin where the clause allows one;
   - for diagnostics, `.en/.ar/.franco.stderr` goldens in `pending/`.
2. Implement until those tests pass.
3. Run `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test --workspace`. All must pass.
4. You may run the `fast` job on demand while building.
5. Up to **3 attempts** at a failing approach. After that, stop building and go to step 7 with what you have; step 8 will reject it if nothing improved.

**Step 7: Evaluate.** Run every job on the worktree HEAD, in this order:
1. `kalam-eval run --job fast`: G-INT again, G-PAR (P1–P3, P5–P9), G-SEC, data metrics, dev corpus.
2. `kalam-eval run --job deep`: P4, differential, fuzz (15 min per target), performance, coverage.
3. The holdout runner, which returns aggregates only. At M1 it returns only the `m1_syntax`, `parity`, `trojan` and `diag_recovery` aggregates; the rest stay `unavailable` (§6.2).
4. `kalam-eval run --job llm-eval --sample 0.25` (once S8 exists).
5. `kalam-eval score`: normalized metrics, dimension scores, composite, gate states and ratchet check, written to `eval/iterations/N/result.json`.

Tests in `conformance/pending/` count toward the conformance metrics only after the reviewer promotes them into `conformance/`. **Before step 8,** call the reviewer for new pending tests and any protected-path change.

**Step 8: Accept or reject.** The iteration is **accepted** only if all of the following hold:
1. G-INT passes, and no gate check that passed at baseline fails now.
2. G-RATCHET passes.
3. **Progress:** the target improved beyond its noise rule (`exact`: any improvement; `ci95`: the CIs do not overlap; `mannwhitney`: more than 5% with p < 0.01). For a **prerequisite slice** (step 4.1), it is enough that at least one metric changed from `unavailable` to `measured`, or that at least one reviewer-promoted conformance test now passes.
4. The composite did not decrease, ignoring changes inside the noise band of noisy metrics. **On a checklist tier**, use the *tier score* (SCORECARD.md §10) instead. The full composite is then recorded but is advisory only.
5. `cargo test`, `clippy` and `fmt` are clean, and every reviewer verdict for this iteration is `approve`.

Otherwise the iteration is **rejected**.

**Step 9: Record.** Run `kalam-eval history append eval/iterations/N/result.json`. It adds exactly one line to `eval/history.jsonl` (schema in §8), with the outcome, gates, every metric record, dimension scores, composite, holdout aggregates and the next target.

**Step 10: Integrate.**
- **Accepted:** commit on `loop/iter-N` with the message `loop(N): <slice> — <target> <before>→<after>`. Use the session's required commit attribution. Then run `git merge --ff-only` into `loop/<milestone>`.
- **Rejected:** do not merge. Tag the branch `rejected/iter-N` for forensics, and remove the worktree.
- Never force-push. Never rewrite `loop/<milestone>`.

**Step 11: Stop rules** (§5). Evaluate them in order. If one fires, write `eval/STOP` with the reason and the evidence paths.

**Step 12: Hand off.** Release the lock. The last history line carries `next_target`, so the next iteration starts from data, not from memory.

## 4. Target selection (exact)

```text
input: baseline results, scorecard.json, target milestone tier t (FREEZE.json target_milestone)
T = scorecard.json milestone_tiers.tiers[id == t]

If T.mode == "checklist" (M1-M4, M6-M10), use steps 1, 2, C3, C4 and 5:
  1-2. as below, with the gate checks limited to those in T.checks (out-of-tier checks are never "fail").
  C3. Weakest check. Among T.checks plus the checks of every tier T ratchets from, pick the check with the
      largest (100 - check_score) (SCORECARD.md §10 tier score). Tie-break: earliest stage, then table order
      in T.checks. Time-accumulated checks (D1.fuzz_crash_free_hours) are skipped while the fuzz target exists
      and is running; they move by waiting, not by slices.
  C4. Every check holds (or only time-accumulated ones remain): pick the T.report_only metric with the lowest
      normalized score whose stage is in T.stages_in_scope. Never pick an out-of-tier metric.
      If none is left, the iteration does no slice and records outcome "waiting" (for example while the
      24 h fuzz clock runs). Waiting iterations never count toward the stall rule (§5 rule 4).
  The milestone stop (§5 rule 1) is checked at step 11 of every iteration, so the loop never drifts past a met tier.

If T.mode == "scorecard" (M5, beta, 1.0), use steps 1-5 with
anchor = {mvp: 60, beta: 75, 1.0: 90}[T.scorecard_tier]

1. Failing gates first. For each gate check in the state "fail", in the order G-INT, G-SEC, G-PAR, G-FUZZ, G-RATCHET:
     return target = that check.
   ("pending" checks are not failures. Steps 3-4 build toward them through the stage DAG.)
2. Overfitting alarm. If dev pass rate minus held-out pass rate is above 15 pp in any tier (corpus,
   security corpus or Trojan suite):
     return target = "generalization". The slice adds spec-derived conformance tests for the clauses
     the failing held-out aggregates point to. It never adds corpus-specific code.
3. Weakest dimension. For each dimension d, compute
     gap_d = weight_d * max(0, anchor - score_d) / 100.
   Pick the d with the largest gap_d. Tie-break: larger weight, then lower dimension number.
   Skip any d marked "stuck" (see 5).
4. Weakest metric inside d. Among d's metrics that count for m, pick the largest (anchor - normalized).
   Tie-break: earliest stage, then table order in scorecard.json.
5. Stuck rule. If the same (dimension, metric) target was rejected in 3 consecutive iterations, mark it
   "stuck" for the next 5 iterations, log it in questions.jsonl, and re-run step 3 without it.
```

At iteration 1 the target tier is M1. Every check scores 0. Step C3 then picks by stage:
- `G-INT` and `G-PAR.P7` need only S0, the harness.
- `D2.trojan_source_suite` is next, at stage S1.

So the first slices are the S0 harness and then the S1 lexer, which covers the source-text checks and the three-skin keyword table. That lexer turns the G-SEC Trojan Source and lexing-hazard checks from pending to measured. The S2 parser, KCF and printer follow, and they light up P1–P4, P6, P8, formatter, convert and the corpus syntax pass. This is the intended bootstrap order.

## 5. Stop rules

Check them in this order. Any rule that fires writes `eval/STOP` and ends the loop until a human removes it.

1. **Milestone tier reached.**
   - **Trigger** (on the frozen set, held-out included):
     - checklist tier: every check in the target tier holds, and so does every ratchet from earlier tiers (SCORECARD.md §10);
     - scorecard tier: SCORECARD.md §6 rules 1–3 hold, plus rule 4 for beta and 1.0.
   - **Action:**
     - Run the `milestone` job.
     - Write `eval/milestones/<t>/scorecard.result.json` and `eval/milestones/<t>/summary.md`. The summary covers: the tier's checklist with each box ticked and its evidence path, what was built, scores, gates, checkpoints, the tier's `founder_items`, open questions and spec gaps.
     - Write `eval/STOP` with `awaiting founder approval for <t>`.
     - **Report to the founder.** Point to `summary.md`, and end the loop.
   - The loop **never** does any of these on its own:
     - start the next milestone tier;
     - bump the eval version;
     - change thresholds or tier definitions.

     After approval, the founder (or the reviewer on the founder's instruction) removes `STOP`, sets the next `target_milestone`, and runs Phase 0 for the next eval version.
   - **For M1** the trigger is exactly the checklist in §5.1.
2. **Design fork.**
   - **Trigger:** a slice needs a founder decision with no working default (§3 step 5). This fires **at once**. It does not wait until every target is blocked.
   - **Action:**
     - Write `eval/STOP` with `design fork: <question id>`, naming the `questions.jsonl` entry.
     - Report the fork to the founder: the options, the evidence and the recommendation.
3. **Integrity failure.** G-INT failed (step 1).
4. **Stalled.**
   - **Trigger:** either of these:
     - 8 consecutive iterations with no accepted iteration;
     - 15 consecutive accepted iterations in which neither the composite (the tier score on checklist tiers) nor any gate improved.
   - Iterations with `outcome: "waiting"` are skipped in both counts. They neither break a streak nor extend one.
   - **Action:** escalate with the history window.
5. **Gate keeps regressing.** The same gate check failed and was then fixed 3 times within 10 iterations. This points to a design problem, so escalate.
6. **Budget.** The cost cap in `FREEZE.json` (API spend or wall-clock per milestone) is reached.

### 5.1 M1 exit checklist (the stop trigger for M1)

This block matches `scorecard.json` → `milestone_tiers` → M1 → `checklist` and the table in SCORECARD.md §10.1. It is the ROADMAP.md M1 exit criteria in scorecard ids. When every box holds, the loop runs stop rule 1 for `M1`.

<!--CHECKLIST:M1-->
```text
M1 EXIT CHECKLIST  (frozen eval set, held-out included; every box must hold)
[ ] 1  G-INT passes: manifest, append-only logs and reviewer sign-offs are clean
[ ] 2  P7 keyword-table integrity: 0 conflicts in spec/keywords.json (spec §7.4)
[ ] 3  D2.trojan_source_suite: 100% of bad cases rejected with a SEC code naming the code point, 100% of good cases accepted
[ ] 4  D2.lexing_hazard_suite: 100% of the lexer cases, including ی ک ە ھ ۀ folding in keywords (DECISIONS D19; SEC0104 collision cases count from M2)
[ ] 5  P1 canonical identity: identical KCF SHA-256 across en/ar/franco for 100% of parsing corpus cases
[ ] 6  P2 pairwise round-trip: all 6 ordered skin pairs and the 3-hop cycles KCF-equal, comments and blank lines kept
[ ] 7  P3 source fixpoint: byte-identical for 100% of cases
[ ] 8  P8 mixed-skin policy: a keyword from another skin gives K0201 in every skin (K0301 is deferred to M2)
[ ] 9  D7.formatter_idempotence: fmt(fmt(x)) == fmt(x) and kcf(fmt(x)) == kcf(x) for every corpus and conformance file, all skins
[ ] 10 D7.skin_convert: P2 and P3 pass through the real `kalam convert`
[ ] 11 D1.corpus_syntax_pass = 21/21: every case of T01-T15, T31-T33, T48-T50 (dev + held-out) parses, formats and round-trips in all three skins (T50 main: the same SEC rejection in all three)
[ ] 12 D1.property_tests: the fmt-idempotence and round-trip properties pass on >= 10k cases each
[ ] 13 D5.error_recovery_precision >= 0.5 on eval/diagnostics/broken/
[ ] 14 D5.diag_completeness >= 70% over the M1 code set (every spec §27 K01xx, K02xx, K09xx and SEC01xx code except retired ones such as SEC0107, plus any other code `kalam explain` lists)
[ ] 15 P6 diagnostic catalog parity = 100% over the M1 code set (en/ar/franco message, help and `kalam explain` page)
[ ] 16 P4 property round-trip: >= 10k generated programs per run through P1-P3 with 0 failures
[ ] 17 D1.fuzz_crash_free_hours >= 24 on the lexer, parser and formatter targets (G-FUZZ at M1 scope)
[ ] 18 G-RATCHET passes, and the M0 data metrics still hold (keywords <= 40, productions <= 120, synonyms pass)
When all 18 hold: run the milestone job, write eval/milestones/M1/scorecard.result.json and summary.md,
write eval/STOP "awaiting founder approval for M1", and report to the founder. Open human item: Q14.
```
<!--/CHECKLIST:M1-->

## 6. Anti-gaming rules

The fixer optimizes against what it can see, so these rules are mandatory. A violation found in review voids the iteration and every iteration that built on it.

### 6.1 Rules

| # | Rule | Enforced by |
|---|---|---|
| A1 | **Composite is advisory; gates are binding.** A higher composite never excuses a failed gate or ratchet | step 8 |
| A2 | **Unmeasured = 0.** Only the founder, at a milestone review, may mark a metric `n/a` | `kalam-eval score` |
| A3 | **No test editing by the fixer.** Fixer changes to `expected.txt`, goldens, conformance expectations, `scorecard.json` thresholds, normalization, weights, holdout aggregation or harness code are rejected without reviewer sign-off. `--bless` is reviewer-only | G-INT, reviewer |
| A4 | **Frozen eval per milestone.** The corpus, models, doc snapshot and judge are pinned in `FREEZE.json`. New tests go into the *next* eval version. Thresholds change only at a milestone review by the founder | G-INT |
| A5 | **No special-casing.** Compiler, runtime and stdlib code may not mention task ids, corpus paths, holdout names, or any expected-output line of 20 or more characters. They may not branch on file names, the number of skins or the harness environment | G-INT corpus-leak scan, reviewer |
| A6 | **Parity can't be gamed by rejection.** A case accepted in some skins and rejected in others fails G-PAR. Rejecting everything scores 0 on D1 | G-PAR scope rule |
| A7 | **Significance gating.** LLM and performance metrics count only beyond their noise rule | `kalam-eval score` |
| A8 | **Ratchets.** A threshold already met stays met (G-RATCHET) | step 8 |
| A9 | **No self-grading.** The LLM judge (D5) and the D3 model panel are pinned and calibrated. The fixer never runs as the judge or as the eval agent of `D3.agent_app_success` | FREEZE.json, reviewer |
| A10 | **Self-scaffolding from the dev split only.** Verified solutions may become few-shot examples or docs only if they come from dev-split problems | reviewer, holdout runner (it rejects a doc snapshot containing holdout text) |
| A11 | **Escape hatches are counted.** `unsafe.*`, `extern` and `.expose()` added to stdlib or corpus code show up in `D2.escape_hatch_density` and need reviewer sign-off. A test never passes by adding an escape hatch | `kalam audit`, reviewer |
| A12 | **Same command in CI and in the loop.** Scores come only from `kalam-eval` runs on a clean worktree with pinned toolchains (`rust-toolchain.toml`, `requirements.lock`, Node version). Numbers typed into the history line by hand are invalid | harness signs `result.json` with the commit hash; G-INT compares |
| A13 | **Tests come from the spec.** Every conformance test cites a clause. The reviewer checks that the test follows the spec and not the implementation | reviewer |
| A14 | **No disabling.** Skipping, `#[ignore]`, xfail or deleting a test, fuzz target or metric needs reviewer sign-off with a reason | G-INT diff check |
| A15 | **Overfitting alarm.** A dev minus held-out gap above 15 pp forces a generalization slice (§4 step 2). A gap above 30 pp at a milestone blocks the milestone | §4, §5 |

### 6.2 The hidden 30% holdout

1. **What is held out.** 30% of each evaluation set:
   - the corpus: the 15 [H] tasks of INDEX.md §5;
   - HumanEval/MBPP problems for D3;
   - the security corpus;
   - the Trojan Source suite;
   - the seeded-error diagnostics suite.

   For the corpus the split is fixed by research/05. For the other sets, the holdout author draws it with a seed recorded in `FREEZE.json`.
2. **Where it lives.** `E:\knowcap-dev\6-R and D\kalam-holdout\`, outside the repository.
   - It is encrypted at rest with `age`.
   - The key is available only to the holdout-runner process, through the environment variable `KALAM_HOLDOUT_KEY`. Fixer and reviewer sessions never get it.
   - **Founder setup step** (the loop must not change settings itself): deny the fixer session any read of that path in the Claude Code permission settings.
3. **Who writes it.** The holdout author session writes it from the INDEX.md titles and the spec only. For corpus tasks it writes the same files as dev tasks (three skins, `ref.py`, `expected.txt`, `task.toml` with `split = "heldout"`), and validates them by running `ref.py`. It records the SHA-256 manifest in `FREEZE.json`.
4. **How it is scored.**
   - `kalam-eval holdout --kalam <path-to-built-binary>` starts the holdout runner as a separate process. It decrypts the store into a temp dir, runs the same runners as for dev, then deletes the temp dir.
   - It returns **only** this aggregate JSON: `{tier_pass: {mvp, beta, "1.0"}, m1_syntax: {tasks_passed, tasks_total}, parity: {P1..P5: bool}, seccorpus: {impossible, escaped, mitigated, vulnerable_by_cwe}, trojan: {bad_rejected, good_accepted}, diag_recovery: number, manifest_sha256}`.
   - `m1_syntax` counts the held-out MVP-tier tasks that pass the `D1.corpus_syntax_pass` rule.
   - It never returns names, diffs, diagnostics or stdout. If its `manifest_sha256` differs from `FREEZE.json`, G-INT fails.
5. **How it is used.** Held-out aggregates count toward D1, D2 and G-SEC exactly like dev results, and they are required for milestone passage. The dev-versus-holdout gap drives A15.
6. **Rotation.** At each new eval version, held-out tasks already disclosed in a milestone report move to dev, and the holdout author writes fresh held-out tasks. A sealed task never becomes visible during its own eval version.

## 7. Changing the eval

The eval itself changes only through this process.

1. Proposals (a new metric, a corpus fix, a threshold recalibration, a harness bug fix) go to `eval/proposals/NNNN.md`.
2. The reviewer decides on harness bug fixes and corpus corrections where the expectation contradicts the spec.
3. Threshold, weight and milestone-rule changes need the **founder**.
4. Every accepted change:
   - regenerates `scorecard.json` and the SCORECARD.md tables from the single generator;
   - rewrites `MANIFEST.sha256`;
   - bumps `eval_version` (patch for fixes, minor for new metrics or tasks);
   - appends a `reviews.jsonl` record.

   Scores are compared only within one `eval_version`.

## 8. `eval/history.jsonl` schema (`kalam-history/1`)

Each line is one JSON object for one iteration. Lines are never edited. Every value comes from a `kalam-eval` run (A12); nothing in it is estimated.

```text
{
  "schema": "kalam-history/1",
  "iteration": <int>,                       # 0 = baseline after freeze
  "ts": "<ISO-8601 UTC>",
  "eval_version": "eval-v0.1",
  "target_milestone": "M1" | "M2" | ... | "M10" | "beta" | "1.0",   # milestone tier (SCORECARD.md §10)
  "tier_mode": "checklist" | "scorecard",
  "tier_score": <0..100|null>,              # checklist tiers only
  "tier_checks": {"<check id>": "pass" | "fail" | "unavailable", ...},   # every check of the target tier and its ratchets
  "commit": "<sha of the evaluated tree>", "base_commit": "<sha of loop/<m> before the iteration>",
  "outcome": "baseline" | "accepted" | "rejected" | "integrity_fail" | "stopped" | "waiting",
             # "stopped": design fork or milestone reached; "waiting": no slice, only time-accumulated checks remain
  "target": {"kind": "gate"|"metric"|"generalization", "id": "<G-PAR.P1 | D1.corpus_tier_pass | ...>",
             "dimension": "D1"|...|null, "weighted_gap": <float|null>},
  "slice": {"title": "<text>", "stage": "S0".."S11", "spec_clauses": ["§5.1", ...],
            "plan": "eval/iterations/<N>/plan.json"},
  "gates": {"G-INT": "pass|fail", "G-PAR": {"state": "pass|fail|pending", "checks": {"P1": "pass|fail|pending", ...}},
            "G-SEC": {...}, "G-FUZZ": {...}, "G-RATCHET": {...}},
  "metrics": [ {"dimension": "D1", "metric": "D1.corpus_tier_pass", "value": <number|bool|null>,
                "ci95": [<lo>, <hi>] | null, "threshold_mvp": ..., "threshold_beta": ..., "threshold_1_0": ...,
                "normalized": <0..100|null>, "gate": "<gate id>|null", "eval_version": "...", "commit": "...",
                "status": "measured|unavailable|n/a|report_only"} , ... one per scorecard metric ... ],
  "dimensions": {"D1": <0..100>, ..., "D8": <0..100>},
  "composite": <0..100>,
  "weakest": {"dimension": "D?", "weighted_gap": <float>},
  "next_target": {"kind": ..., "id": ...},
  "holdout": {"tier_pass": {...}, "dev_minus_holdout_pp": {...}, "manifest_sha256": "..."},
  "reviews": [{"reviewer": "<session id>", "scope": "<paths/tests>", "verdict": "approve|reject"}],
  "questions": ["<ids appended to eval/questions.jsonl this iteration>"],
  "result_sha256": "<hash of eval/iterations/<N>/result.json>",
  "notes": "<one line, optional>"
}
```

The file is created empty. The first line is written by iteration 0 (Phase 0, step 5).

## 9. One-screen checklist

```text
0  STOP? -> exit.        lock, N, worktree loop/iter-N
1  kalam-eval integrity  -> fail: history(integrity_fail), STOP, exit
2  baseline (reuse if same commit + eval_version, else fast+deep+holdout)
3  target: failing gate > overfit alarm > (checklist tier) weakest in-tier check, then in-scope report-only
                                       | (scorecard tier) max weight*(anchor-score) dimension > weakest metric
4  slice: missing stage on DAG (only stages_in_scope) > failing lowest dev task > smallest mover; <=600 lines; plan.json
5  spec check: cite clauses; gaps -> working default + questions.jsonl; never invent semantics;
   design fork (no working default) -> questions.jsonl design_fork, history(stopped), STOP, report to founder
6  TDD build; tests to conformance/pending; cargo fmt/clippy/test clean; <=3 attempts
7  fast, deep, holdout aggregates, llm sample, score -> result.json; reviewer for pending tests and protected paths
8  accept iff integrity ok, no new gate failure, ratchets ok, target moved (or stage became measurable), composite (tier score) not down, reviews approve
9  kalam-eval history append (one line)
10 accepted: commit + ff-merge; rejected: tag rejected/iter-N, drop worktree
11 stop rules: milestone tier met (M1: §5.1 checklist) -> summary + STOP + report to founder | design fork | integrity | stalled | regressing | budget
12 release lock
```
