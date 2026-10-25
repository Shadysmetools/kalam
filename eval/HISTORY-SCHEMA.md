# `eval/history.jsonl`: the lines `kalam-eval --append` writes today

**Authority:** [`LOOP.md`](LOOP.md) §8 (`kalam-history/1`) is the target schema. This page says which of its fields the current harness (`compiler/crates/kalam-eval`) actually writes, which it adds, and which are still deferred. When this page and LOOP.md disagree, LOOP.md wins and this page is out of date.

## Rules

1. **Append-only.** `--append` opens the file in append mode and writes exactly one line ending in `\n`. If the file's last byte is not `\n`, it writes a `\n` first. Existing bytes are never rewritten. G-INT checks this ("previous lines byte-identical").
2. **Harness-only (LOOP.md A12).** Every value comes from a `kalam-eval` run. Nothing is typed in by hand or estimated. The only values passed in from outside are the ones the harness cannot know: `--timestamp`, and optionally `--git-sha`, `--outcome` and `--notes`.
3. **Unmeasured is never a pass (LOOP.md A2, SCORECARD.md §1).** A metric the harness cannot measure yet gets `status: "not_measurable"`, `value: null` and `normalized: 0`, and it counts as **0** in its dimension mean.
4. **First real line.** LOOP.md Phase 0 step 5 says the first line is the post-freeze baseline (iteration 0). Do not append to the real file before the freeze. Test runs use `--history <scratch file>`.

## Command

```text
kalam-eval [--root DIR] [--target mvp|beta|1.0] [--cargo-test] [--out FILE]
           --append --timestamp <ISO-8601 UTC> [--history FILE] [--git-sha SHA]
           [--outcome baseline|accepted|rejected|integrity_fail|stopped] [--notes TEXT]
```

Run it from `compiler/`: `cargo run -p kalam-eval -- ...`. The repository root is found by walking up to `eval/scorecard.json`.

| Flag | Meaning |
|---|---|
| `--timestamp` | Required with `--append`. Written verbatim as `ts`. The harness never reads the clock, so a re-run gives the same line. |
| `--git-sha` | Optional. If omitted, the harness uses `git rev-parse HEAD` in the root. If that fails, it writes `null`; a SHA is never invented. |
| `--target` | The target milestone used to decide `n/a` and `report_only` (default `mvp`, until `eval/FREEZE.json` exists). |
| `--history` | Optional. Overrides the default path `eval/history.jsonl`. |

## One line (`kalam-history/1` subset)

```text
{
  "schema": "kalam-history/1",
  "iteration": <int>,            # last line's iteration + 1; 0 for an empty file
  "ts": "<--timestamp>",
  "git_sha": "<sha>|null",       # requested by the orchestrator; same value as commit
  "commit": "<sha>|null",        # LOOP.md §8 name
  "git_dirty": true|false|null,  # `git status --porcelain` non-empty: scores describe HEAD + uncommitted
                                 # edits, so A12 (clean worktree) did not hold; null = git unavailable
  "eval_version": "<scorecard.json eval_version>",
  "target_milestone": "mvp|beta|1.0",
  "outcome": "<--outcome>|null", # the harness does not decide acceptance; the loop passes it
  "dimensions": {"D1": <0..100>, ..., "D8": <0..100>},
  "composite": <0..100>,         # sum(weight * dimension) / 100; advisory
  "gates": {
    "G-INT":     {"state": "pass|fail|pending"},
    "G-PAR":     {"state": "...", "checks": {"P1": "pass|fail|pending", ..., "P10": "..."}},
    "G-SEC":     {"state": "...", "checks": {"vulnerable_cases": "...", "trojan_source_suite": "...", "lexing_hazard_suite": "..."}},
    "G-FUZZ":    {"state": "..."},
    "G-RATCHET": {"state": "..."}
  },
  "metrics": [ {"dimension": "D4", "metric": "D4.keywords_per_skin", "value": <number|bool|null>,
                "ci95": null, "threshold_mvp": ..., "threshold_beta": ..., "threshold_1_0": ...,
                "normalized": <0..100|null>, "gate": "<gate id>|null", "eval_version": "...",
                "commit": "...", "status": "measured|not_measurable|n/a|report_only"}, ... one per scorecard metric ... ],
  "checks": {                    # harness probes; NOT scorecard metrics, never scored
    "corpus_lex":    {"passed": <int>, "total": <int>},
    "security_lex":  {"passed": <int>, "total": <int>},
    "lex_roundtrip": {"failed": <int>, "total": <int>},
    "keyword_table_load": <bool>,
    "cargo_test":    {"status": "ran|build_failed|error|skipped", "passed": <int|null>, "failed": <int|null>}
  },
  "summary": {"metrics_total": <int>, "measured": <int>, "not_measurable": <int>, "excluded_n_a_or_report_only": <int>},
  "notes": "<--notes>|null"
}
```

### Status values

| Status | Meaning | Normalized | In the dimension mean |
|---|---|---|---|
| `measured` | computed by the harness in this run | 0–100 per scorecard.json `normalization` | yes |
| `not_measurable` | needs a component or suite that does not exist yet. This is LOOP.md §8's `unavailable` under the name the orchestrator asked for, and the next schema revision should pick one name. | 0 | yes, as 0 |
| `n/a` | the target milestone's threshold is `n/a` | `null` | no |
| `report_only` | the target milestone's threshold is `report_only` | `null` | no |

### Gate states

- `pass` only when **every** check in the gate's definition ran and passed. The current harness never emits `pass` for any gate, because each gate still has unbuilt checks.
- `fail` as soon as any check that ran has failed. For example, a corpus Trojan Source case that is not rejected at lex level fails `D2.trojan_source_suite`, and therefore G-SEC.
- `pending` otherwise.

## What the harness measures today

| Metric or check | How |
|---|---|
| `D4.keywords_per_skin` | Rows of `spec/keywords.json` with kind `keyword` or `ascii_fixed`, max over skins |
| `D4.reserved_spellings_per_skin` | The same, plus `core_name` (report_only) |
| `D4.synonym_count` | No non-punctuation concept has more than one `en` spelling, or more than one `ar` spelling after `arabic_fold` (including `ar_also_accepts`) |
| `D4.grammar_productions` | `name = …` heads in `compiler/grammar/kalam.ebnf`, or in the spec §8.2 ```` ```ebnf ```` block until that file exists |
| `D2.trojan_source_suite` | `measured: false` if any corpus case with lex-stage `SEC` expectations fails. Otherwise `not_measurable`, because the ~60-case suite does not exist yet |
| check `corpus_lex` | Every corpus case × skin, staged as `main.kl` / `main.ar.kl` / `main.franco.kl` (DR-18, DECISIONS D24). Run/http cases and later-stage compile-error cases must lex cleanly. Lex-stage expected diagnostics must match code, line and named code points |
| check `lex_roundtrip` | `kalam_fmt::convert` (the token converter): X→Y→X for the 6 ordered pairs, both 3-hop cycles, the fixpoint, and en→Y equal to the committed Y file. This is a proxy only, because P2 and P3 are defined on KCF |
| check `keyword_table_load` | `KeywordTable::load` on `spec/keywords.json` (spec §7.4 rules 1–4). G-PAR P7 stays `pending` until rules 5–7 are checked |
| check `cargo_test` | Opt-in `--cargo-test`: `cargo test --workspace --no-fail-fast` with its own target dir, summing the `test result:` lines |

## Deferred LOOP.md §8 fields

`base_commit`, `target`, `slice`, `weakest`, `next_target`, `holdout`, `reviews`, `questions`, `result_sha256` and per-metric `ci95` (always `null` now).

**Milestone tiers are not evaluated yet.** LOOP.md §8 records `target_milestone` as a milestone tier (`M1`–`M10`, `beta`, `1.0`) plus `tier_mode`, `tier_score` and `tier_checks`. The harness accepts only `--target mvp|beta|1.0`, does not read `scorecard.json` → `milestone_tiers`, and writes none of the three tier fields. So it cannot yet evaluate the M1 exit checklist (LOOP.md §5.1), which is the loop's first target. Until it can, a history line's `target_milestone` is the scorecard tier the metrics were normalized against, not the loop's milestone tier. They need the loop driver, the holdout runner, the reviewer flow and `eval/iterations/N/result.json`, none of which exist yet.
