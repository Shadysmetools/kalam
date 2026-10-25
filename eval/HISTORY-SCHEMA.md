# `eval/history.jsonl`: the lines `kalam-eval --append` writes today

**Authority:** [`LOOP.md`](LOOP.md) §8 (`kalam-history/1`) is the target schema. This page says which of its fields the current harness (`compiler/crates/kalam-eval`) actually writes, which it adds, and which are still deferred. It also documents how every M1 checklist item is measured, the input files other runners write, the freeze files and the ratchet baseline. When this page and LOOP.md disagree, LOOP.md wins and this page is out of date.

## Rules

1. **Append-only.** `--append` opens the file in append mode and writes exactly one line ending in `\n`. If the file's last byte is not `\n`, it writes a `\n` first. Existing bytes are never rewritten. G-INT checks this (see "G-INT").
2. **Harness-only (LOOP.md A12).** Every value comes from a `kalam-eval` run. Nothing is typed in by hand or estimated. The only values passed in from outside are the ones the harness cannot know: `--timestamp`, and optionally `--git-sha`, `--outcome` and `--notes`.
3. **Unmeasured is never a pass (LOOP.md A2, SCORECARD.md §1).** A metric the harness cannot measure yet gets `status: "not_measurable"`, `value: null` and `normalized: 0`, and it counts as **0** in its dimension mean. A missing suite, an empty suite, a suite with a malformed case, a missing or stale report, or a held-out part that did not run is `not_measurable`, never 100%.
4. **First real line.** LOOP.md Phase 0 step 5 says the first line is the post-freeze baseline (iteration 0). Do not append to the real file before the freeze. Test runs use `--history <scratch file>`.

## Commands

```text
kalam-eval [--root DIR] [--target M1..M10|beta|1.0|mvp] [--cargo-test] [--run-props] [--out FILE]
           [--holdout DIR | --no-holdout]
           [--append --timestamp <ISO-8601 UTC> [--history FILE] [--git-sha SHA]
            [--outcome baseline|accepted|rejected|integrity_fail|stopped|waiting] [--notes TEXT]]
kalam-eval freeze    [--root DIR] [--target TIER] [--timestamp TS] [--holdout DIR]
kalam-eval integrity [--root DIR] [--holdout DIR]
kalam-eval holdout   --root DIR --store DIR          # the sealed child; started by the scorecard run
```

Run it from `compiler/`: `cargo run -p kalam-eval -- ...`. The repository root is found by walking up to `eval/scorecard.json`.

| Flag | Meaning |
|---|---|
| `--timestamp` | Required with `--append`. Written verbatim as `ts`. The harness never reads the clock, so a re-run gives the same line. `freeze` writes it as `frozen_at` (or `null`). |
| `--git-sha` | Optional. If omitted, the harness uses `git rev-parse HEAD` in the root. If that fails, it writes `null`; a SHA is never invented. The props report must carry this commit. |
| `--target` | The milestone tier (SCORECARD.md §10; any id in `scorecard.json` → `milestone_tiers.order`). `mvp` is an alias of M5. Default: `eval/FREEZE.json` `target_milestone`, or `M1` before the freeze. Metrics are normalized (and `n/a` / `report_only` decided) against the tier's scorecard milestone: its own `scorecard_tier`, else the next scorecard tier in `order` (M1–M4 → `mvp`, M6–M10 → `beta`). That milestone is written as `normalized_against`. |
| `--holdout` | The held-out store (default `eval/holdout`). The scorecard run starts `kalam-eval holdout` on it as a child process and keeps only the aggregate. `--no-holdout` skips it (every held-out-split check is then `not_measurable`). |
| `--run-props` | Runs `cargo test -p kalam-props --release` with `KALAM_PROPS_REPORT` set to the harness's own path (`compiler/target/kalam-eval-props/report.json`, deleted first) and reads that report: it describes this tree by construction. Without it, `eval/props/report.json` is read and must carry the evaluated commit. |
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
  "target_milestone": "M1".."M10"|"beta"|"1.0",   # the resolved tier (`--target mvp` writes "M5")
  "normalized_against": "mvp|beta|1.0",
  "tier_mode": "checklist|scorecard",
  "tier_score": <0..100|null>,   # checklist tiers: mean of check scores (milestone_tiers.tier_score); scorecard tiers: null
  "tier_checks": [ {"id": "G-PAR.P1", "tier": "M1", "status": "pass|fail|not_measurable",
                    "score": <0..100>, "evidence": "<text>"}, ... ],
  "milestone_met": true|false,   # true only when tier_checks is non-empty and every entry is "pass"
  "outcome": "<--outcome>|null", # the harness does not decide acceptance; the loop passes it
  "dimensions": {"D1": <0..100>, ..., "D8": <0..100>},
  "composite": <0..100>,         # sum(weight * dimension) / 100; advisory
  "gates": {
    "G-INT":     {"state": "pass|fail|pending", "checks": [{"check", "state", "detail"}, ...]},
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
  "holdout": <the held-out aggregate (below)> | {"status": "not_run", "reason": "<text>"},
  "summary": {"metrics_total": <int>, "measured": <int>, "not_measurable": <int>, "excluded_n_a_or_report_only": <int>},
  "notes": "<--notes>|null"
}
```

The scorecard JSON on stdout (and `--out`) carries more detail under `checks`: `parity` (P1, P2, P3, corpus syntax, corpus fmt, with failures), `suites` (per suite: counts, failures, defects, `deferred_m2`, out-of-scope and later-stage cases), `catalog`, `p7_conflicts`, `props` and the per-case corpus records.

### Status values

| Status | Meaning | Normalized | In the dimension mean |
|---|---|---|---|
| `measured` | computed by the harness in this run | 0–100 per scorecard.json `normalization` | yes |
| `not_measurable` | needs a component, suite, report or held-out part that does not exist or did not run. LOOP.md §8 uses this name; SCORECARD.md §1 and `scorecard.json` still say `unavailable` for the same thing. | 0 | yes, as 0 |
| `n/a` | the target milestone's threshold is `n/a` | `null` | no |
| `report_only` | the target milestone's threshold is `report_only` | `null` | no |

### Tier checks

`tier_checks` lists every check of the target tier, then every check of each tier it ratchets from (`ratchet_from`, transitively), then each tier's `ratchet_only` metrics. The same id can appear once per tier (M2 repeats `D1.fuzz_crash_free_hours` with a wider scope), so entries are a list, not a map. A check id whose scope differs from its M1 scope is `not_measurable` ("measured at its M1 scope only"): the M1 runners never vouch for a wider scope. A scorecard tier (M5, `beta`, `1.0`) adds the SCORECARD.md §6 rule as checks: `G-INT`, `G-PAR`, `G-SEC`, `G-FUZZ`, `dimension_min.D1`..`D8`, `composite_min` and `human_gate.<id>`.

| Status | When |
|---|---|
| `pass` | the check's whole scope was measured and holds (for a held-out-split check: dev **and** held-out) |
| `fail` | a measured part already makes the check impossible to meet, e.g. a dev case fails a 100% check, or a gate is `fail` |
| `not_measurable` | anything else: no runner, missing input, a gate that is `pending`, or a held-out-split check whose held-out part did not run |

`score` follows `milestone_tiers.tiers[].tier_score`: pass/fail = 100/0; `>=` = min(100, 100·value/threshold); `<=` = min(100, 100·threshold/value); `== 100 %` = the value; `== 0` = 100 at 0, else 0; `not_measurable` = 0. A check with `op: pass` on a numeric metric (and every `ratchet_only` metric) is judged against the metric's threshold at `normalized_against`.

### Held-out split (LOOP.md §6.2)

The held-out sets are the corpus, the Trojan Source suite and the seeded-error suite (plus the security corpus and HumanEval/MBPP, which no M1 check uses). These checks combine dev and held-out results and pass only when both ran and passed:

| Check | Dev part | Held-out part (aggregate key) |
|---|---|---|
| `G-PAR.P1`, `G-PAR.P2`, `G-PAR.P3` | dev corpus | `parity.P1/P2/P3` `{passed, total}` (total must be > 0) |
| `D1.corpus_syntax_pass` | dev tasks in the check's scope | `m1_syntax` `{tasks_passed, tasks_total}`; value = dev passed + held-out passed, judged `>= 21` |
| `D2.trojan_source_suite` | `eval/suites/trojan-source` + corpus source-text cases | `trojan` |
| `D5.error_recovery_precision` | `eval/suites/seeded-errors` | `diag_recovery` (files and score sum are pooled) |
| `D7.formatter_idempotence` | corpus files + `eval/suites/conformance` | `fmt` (held-out corpus files) |
| `D7.skin_convert` | P2 and P3 above | P2 and P3 above |

**`[M2]` cases (FOUNDER-QUEUE Q10).** In every suite, dev and held-out, a case whose `expect.json` `notes` start with `[M2]` needs name resolution. Every runner leaves it out of the M1 pass/fail and the D5 value, and counts it as `deferred_m2` (in the evidence, the `suites` detail and the held-out `trojan`/`diag_recovery` objects). A malformed `expect.json` is still a defect. Those cases are checked from tier M2 on.

Every other M1 check is judged on its whole scope without a held-out part: `G-INT`, `G-PAR.P4`, `G-PAR.P6`, `G-PAR.P7`, `G-PAR.P8`, `D2.lexing_hazard_suite`, `D1.property_tests`, `D5.diag_completeness`, `D1.fuzz_crash_free_hours`, `G-RATCHET` and the `ratchet_only` metrics.

### Gate states

- A G-PAR, G-SEC or G-FUZZ check's state is its M1 tier check's judgment: `pass`, `fail`, or `pending` (= `not_measurable`). P5, P9 and P10 have no M1 runner and stay `pending`, so G-PAR as a whole cannot pass at M1; G-SEC's CWE check (`vulnerable_cases`) is out of M1 scope and stays `pending`; G-FUZZ is `fail` when `eval/fuzz/status.json` reports `crashes > 0`, else `pending` (its deep-job check has no runner; too few crash-free hours is the time-accumulated M1 check, not a gate failure).
- A gate is `pass` only when **every** check in its definition passed, `fail` as soon as one failed, else `pending`.

## How each M1 checklist item is measured

All compiler access goes through one hook (`src/parity.rs`), the same entry points as the `kalam` CLI: parse = `kalam_parser::parse_source` (lexer and parser diagnostics), canonical form = KCF bytes (`kalam_syntax::kcf::serialize_in`, spec §4.4; KCF records comments and blank lines), `kalam convert` = `kalam_driver::convert_source`, `kalam fmt` = `kalam_driver::format_source`. There is no fallback canonical form: if parse or KCF panics on an empty file, every parser-dependent check is `not_measurable`. "Reported" errors are the error-severity diagnostics of that call; the harness never synthesizes one.

| # | Check | Runner and rule |
|---|---|---|
| 1 | `G-INT` | `src/integrity.rs`; see "G-INT and freeze" |
| 2 | `G-PAR.P7` | `spec/keywords.json`: spec §7.4 rules 1–4 (`KeywordTable::load`) and rule 7 (the word-choice spellings). Value = conflicts, judged `== 0`. Rules 5–6 need the confusable table and the 2,000-word list, which do not exist (spec §7.4), so the M1 scope ("rules 5-7 once their data exists") leaves them out |
| 3 | `D2.trojan_source_suite` | `eval/suites/trojan-source`: every `must_reject` case reports each expected code (first one on the expected line) and every expected `SEC` code **names its code point**: the English message contains a `U+XXXX` that occurs under the diagnostic's span (or on its line when the span is empty); printable ASCII (U+0020–U+007E) never counts, ASCII controls do. Every good case reports no error (warnings are fine). Plus the corpus source-text cases (T50) at lex level. Needs bad and good cases. Held-out split |
| 4 | `D2.lexing_hazard_suite` | `eval/suites/lexing-hazards`, same rule without code-point naming. Cases expecting SEC0104 are out of M1 scope (counted, skipped). A case expecting a retired code (SEC0107) is a suite defect |
| 5–7 | `G-PAR.P1`–`P3` | dev corpus (`src/parity.rs`): P1 = equal KCF across the three skins; P2 = the 6 ordered pairs and both 3-hop cycles through `kalam convert`, re-parsed, KCF-equal; P3 = `convert(convert(s, X, Y), Y, Y) == convert(s, X, Y)` byte for byte, all 9 ordered pairs. G-PAR scope rule: a case rejected in all three skins is unsupported (costs D1), not a parity failure; lex-stage rejection cases (T50 main) are outside P1–P3. Held-out split |
| 8 | `G-PAR.P8` | `eval/suites/mixed-skin`: K0201 cases must report K0201 (on the expected line), good cases no error. K0301 cases are out of M1 scope. "In every skin" needs at least one must-reject file hosted in each of en, ar and franco |
| 9 | `D7.formatter_idempotence` | every corpus file and every `eval/suites/conformance` file with `must_reject: false`, all skins: it parses, `fmt(fmt(x)) == fmt(x)` and `kcf(fmt(x)) == kcf(x)`. A valid file `fmt` refuses is a failure. Files of a case meant to be rejected before parsing are excluded. Files `fmt` re-spelled but whose layout it kept are counted (`layout_kept`). Held-out split |
| 10 | `D7.skin_convert` | P2 and P3 above, both through `kalam_driver::convert_source` |
| 11 | `D1.corpus_syntax_pass` | a task counts when every case, in all three skins, is accepted, passes the formatter rule, P1, P2 and P3 (T50 main: the same lex-stage rejection in all three). Held-out split; value judged `>= 21` |
| 12 | `D1.property_tests` | the props report (below): every **gating** fmt-idempotence property (name contains `idempot`) and every gating round-trip property (name contains `round_trip`/`roundtrip`; today `ast_print_parse_roundtrip` and `convert_roundtrip`), at least one of each, each with `>= 10000` cases and 0 failures. Any failure is a `fail` |
| 13 | `D5.error_recovery_precision` | `eval/suites/seeded-errors`, the canonical seeded-error suite (FOUNDER-QUEUE Q14; it replaces the earlier `eval/diagnostics/broken/` path). scorecard.json formula: per file, genuine / reported, where genuine = reported errors matching a seeded `(code, line)`; a file whose first seeded `(code, line)` is not reported scores 0. Value = mean over files. Every seeded code needs its line. Scope: `>= 30` cases with all three skins (dev + held-out). The spec sets no numeric cascade limit (§21.4 rule 2 hides follow-on errors), so cascades lower the ratio; the evidence also gives the files whose first reported error is seeded and the cascade-free files. Held-out split |
| 14 | `D5.diag_completeness` | **In M1 scope**: LOOP.md §5.1 item 14 and the scorecard.json M1 checks list it at stage S2. Denominator: every spec §27 code in K01xx, K02xx, K09xx and SEC01xx (ranges such as `K0901–K0926` expanded, *retired* rows dropped) plus every other code `kalam explain --catalog --json` lists (read in process from `kalam_diagnostics::explain::catalog_value()`). A code is complete when its en/ar/franco title and message, `help` text and rendered `explain` page are all non-empty. The listing has `help: null` for every code today, so the strict value is 0 until help text exists. The evidence also gives the message-and-explain-only figure |
| 15 | `G-PAR.P6` | the same code set and per-code rule, judged `== 100 %` |
| 16 | `G-PAR.P4` | the props report: failures over the gating properties whose own `parity` list names `P1`, `P2` or `P3` (coverage is never guessed from a name or description); each of P1, P2, P3 must be covered by a property with `>= 10000` cases (a failure is a failure at any count). Judged `== 0` |
| 17 | `D1.fuzz_crash_free_hours` | `eval/fuzz/status.json` (below). The lexer, parser and formatter targets must be confirmed, from its `targets` list or from `fuzz_targets/*.rs` in the repository; otherwise `not_measurable` |
| 18 | `G-RATCHET` | `src/ratchet.rs`; see "G-RATCHET" |

`ratchet_only` metrics (`D4.keywords_per_skin`, `D4.grammar_productions`, `D4.synonym_count`) are measured from `spec/keywords.json` and the spec §8.2 EBNF block (or `compiler/grammar/kalam.ebnf`).

## The shared suite format

`eval/suites/<suite>/<case>/` holds one or more source files named with the spec skin extensions (`.kl`, `.ar.kl`, `.franco.kl`) and an `expect.json`:

```text
{"must_reject": bool, "codes": ["K0201", ...], "lines": [1-based line | null, ...], "notes": "why"}
```

`codes` is order-insensitive; `lines[i]` belongs to `codes[i]` and is the line where the **first** error with that code must point. Each source file is judged on its own. Suites: `trojan-source`, `lexing-hazards`, `seeded-errors`, `mixed-skin`, `conformance`. A case directory is any directory holding `expect.json` (nesting is allowed). Files that are not valid UTF-8 are read as bytes; no compiler entry point accepts bytes, so such a file is never counted as rejected.

## Inputs written by other runners

**`eval/props/report.json`** (`kalam-props`). Accepted schema:

```text
{"schema": "kalam-props-report/1", "commit": "<sha of the tested tree>",
 "properties": [{"name": "fmt_idempotence", "cases": 10000, "failures": 0, "parity": []},
                {"name": "skin_round_trip", "cases": 10000, "failures": 0, "parity": ["P1", "P2", "P3"]}]}
```

What `kalam-props` (`cargo test -p kalam-props --release`, 10 000 cases per property by default, the M1 minimum) writes: `commit` (`git rev-parse HEAD` of the tree it ran on; `null` without git, which the harness treats as not measurable), and per property `name`, `gating`, `parity`, `cases`, `failures`, `examples`. The parity lists name only checks the property's code performs: `kcf_cross_skin_equality` `["P1", "P2"]` (equal KCF across the three printouts and every one-hop conversion; a program accepted in some skins but not others is a failure under the G-PAR scope rule, one rejected in all three is not run), `kcf_hash_stability` `["P1"]`, `convert_roundtrip` `["P2"]` (6 ordered pairs, both 3-hop cycles per skin, byte-identical), `source_fixpoint` `["P3"]` (`convert(convert(x, X->Y), Y->Y) == convert(x, X->Y)` for all 6 ordered pairs); every other property `[]`. Properties other than `ast_print_parse_roundtrip` and `kcf_cross_skin_equality` run only on programs that parse in all three skins; the run keeps drawing programs (up to 4x `cases`, reported as `cases_drawn`) until every gating per-program property has at least `cases` cases. `unsupported` counts drawn programs that no skin accepts.

`properties` may also be an object keyed by name. Accepted aliases: `cases_run`/`programs` for `cases`, `failed` for `failures`, `git_sha` for `commit`. `gating: false` marks a report-only property, which never counts (default `true`). `parity` lists the G-PAR checks the property runs on every generated program (used by P4); a property without it covers none. `commit` is required for `eval/props/report.json` and must match the evaluated commit (full or abbreviated SHA, 7+ hex digits); a report without it, or for another commit, is `not_measurable` (stale). A report written by `--run-props` needs no commit. Other fields (`suite`, `passed`, `meets_m1`, `examples`, …) are ignored: the harness judges the per-property counts itself.

**`eval/fuzz/status.json`** (fuzz runner): `{"crash_free_hours": <number >= 0>, "crashes": <int>, "last_run": "<ISO-8601>", "targets": ["lexer", "parser", "fmt", ...]}`. Other fields are ignored except `oracle_violations` (`{name: {count}}`), whose total is shown in the evidence. `targets` is optional; without it the harness looks for `compiler/fuzz/fuzz_targets/*.rs` or `compiler/crates/*/fuzz/fuzz_targets/*.rs`. Target names must cover `lex`, `pars` and `fmt`/`format`.

## The held-out aggregate (`kalam-holdout-aggregate/1`)

`kalam-eval holdout` reads the store (corpus tasks under `corpus/` or the store root, where only `tNN-*` directories are tasks; held-out suite splits in the shared suite format under `suites/<suite>/` or `<suite>/`: `trojan-source` gives `trojan`, `seeded-errors` gives `diag_recovery`, each `null` when its split does not exist) and prints one line, then exits. It never prints a task name, case id, file name, diagnostic or source text; its panic hook is silent and the parent discards its stderr.

```text
{"schema": "kalam-holdout-aggregate/1", "status": "measured"|"empty"|"encrypted", "manifest_sha256": "<hex>",
 "m1_syntax": {"status": "measured"|"not_measurable", "tasks_passed": n, "tasks_total": n},
 "parity": {"P1"|"P2"|"P3": {"pass": bool|null, "passed": n, "total": n}},
 "fmt": {"passed": n, "total": n, "layout_kept": n, "excluded": n},
 "trojan": {"bad_rejected": n, "bad_total": n, "good_accepted": n, "good_total": n, "deferred_m2": n,
            "defects": n} | null,
 "diag_recovery": {"value": x|null, "files": n, "score_sum": x, "full_skin_cases": n, "first_match": n,
                   "cascade_free": n, "deferred_m2": n, "defects": n} | null,
 "leak": {"hits": n}}
```

`leak.hits` counts held-out task ids, task paths and expected-output lines found in compiler sources; any hit fails G-INT. A store with `.age` files is `encrypted`: this runner has no decryption, so nothing is measured.

## G-INT and freeze

`kalam-eval freeze` (reviewer only; LOOP.md §2 step 4) writes:

- **`eval/FREEZE.json`** (`kalam-freeze/1`): `eval_version` (copied from scorecard.json), `target_milestone`, `frozen_at` (`--timestamp` or `null`), `frozen_roots`, `frozen_files` (every frozen file except FREEZE.json and the manifest), `append_only`, `holdout` (`store`, `manifest_sha256`, `manifest_rule`, `split_seed`, `split_seed_note`; seed and note are carried over from the existing FREEZE.json), `spec_llms_txt_sha256`, the founder fields `model_panel`, `llm_judge`, `api_cost_cap_usd`, `wallclock_cap_hours` (carried over from an existing FREEZE.json, `null` until the founder sets them) and `pending_founder` (the founder fields still `null`, plus `holdout.split_seed` while it is `null` or its `split_seed_note` starts with "provisional"; today it is the provisional `d3abd2d`, FOUNDER-QUEUE Q15). The held-out store is pinned by its manifest hash only: SHA-256 over the sorted lines `<sha256(file)>  <relative path>` of every file in the store.
- **`eval/MANIFEST.sha256`**: `# kalam-manifest/1 <eval_version>`, then `# append-only <path> bytes=<n> sha256=<hex>` for each existing append-only log, then `<sha256>  <path>` for every frozen file (FREEZE.json included, the manifest itself excluded).

Frozen roots: `eval/SCORECARD.md`, `eval/scorecard.json`, `eval/LOOP.md`, `eval/HISTORY-SCHEMA.md`, `eval/FREEZE.json`, `eval/corpus/`, `eval/suites/`, `eval/harness/`, `eval/diagnostics/`, `eval/seccorpus/` and the harness crate `compiler/crates/kalam-eval/` (LOOP.md A3: harness code changes need the reviewer). `target/` directories are skipped. The freeze is deterministic: it never reads the clock.

`kalam-eval integrity` (and every scorecard run) checks:

| Check | `pass` | `fail` | `pending` |
|---|---|---|---|
| manifest | every frozen file matches; re-walking the roots finds no added or deleted file | any change, addition or deletion | no manifest yet |
| sign-off | `eval/reviews.jsonl` has `{"verdict": "approve", "manifest_sha256": "<SHA-256 of MANIFEST.sha256>"}` | — | no such record (a freeze run by the fixer never makes G-INT pass) |
| append-only | each log extends its freeze-time bytes and its `git HEAD` version; history iterations strictly increase | a rewrite, a deletion, a non-increasing iteration | no freeze-time record |
| corpus-leak | no dev task id (`T07`, whole word), task directory name, `eval/corpus/<task>` path or expected-output line of 20+ characters in `compiler/crates/*/src/**/*.rs` (except the harness crate, and only up to the first `#[cfg(test)]`), `stdlib/**`, `compiler/runtime/**` | any hit (also any held-out hit reported by the held-out runner) | — |
| version-pin | FREEZE.json and the manifest name scorecard.json's `eval_version`; the store's manifest hash (and the one the held-out runner reported) equal the pinned one | any mismatch | no FREEZE.json |

G-INT is `fail` if any check fails, `pass` only if all pass, else `pending`.

## G-RATCHET

**Baseline** = the last `history.jsonl` line with `outcome: "accepted"` for the same `eval_version` and `target_milestone`; if there is none, the first line of that eval version and tier that carries `tier_checks` (the first measured run). Lines with other outcomes (including the pre-schema `kept` of iterations 0–2) are never a baseline unless they are that first measured run. **Rule:** every baseline tier check with status `pass` (G-RATCHET excluded), matched by `(id, tier)`, must still be `pass`; `fail`, `not_measurable` or missing breaks the ratchet. No baseline = `pending`.

## Deferred LOOP.md §8 fields

`base_commit`, `target`, `slice`, `weakest`, `next_target`, `reviews`, `questions`, `result_sha256`, per-metric `ci95` (always `null` now) and `holdout.dev_minus_holdout_pp`. They need the loop driver, the reviewer flow and `eval/iterations/N/result.json`, none of which exist yet.
