# eval/props: property suites (`D1.property_tests`)

Written by `cargo test -p kalam-props --release` (crate `compiler/crates/kalam-props`).
The metric passes when `report.json` has `"meets_m1": true`: every gating property has 0
failures on at least 10 000 cases.

| Variable | Default | Meaning |
|---|---|---|
| `KALAM_PROPS_CASES` | 10 000 (release), 300 (debug) | cases per property |
| `KALAM_PROPS_SEED` | `0x4b414c414d` | run seed (decimal or `0x` hex) |
| `KALAM_PROPS_CASE` | unset | run one case only, to reproduce a failure (writes no report) |
| `KALAM_PROPS_REPORT` | `eval/props/report.json` (release builds only) | report path; `-` writes none |
| `KALAM_PROPS_THREADS` | cores, max 16 | worker threads |

`report.json`: `{suite, metric, version, profile, cases, seed, seed_hex, failures,
report_only_failures, passed, meets_m1, elapsed_secs, properties: [{name, gating,
description, cases, failures, passed, busy_secs, examples: [{case, case_seed, skin, detail,
input, reproduce}]}]}`. `failures` counts gating properties only.

Properties: random AST printed in each skin parses back to the same AST; fmt idempotence;
convert round trips (6 ordered pairs, six 3-hop cycles, G2 fixpoint from fmt output); KCF
equality across skins and conversion steps; KCF hash stability (hash = SHA-256 of KCF, skin
inference, determinism, fmt fixpoint, pinned golden hashes); lexer+parser never panic on
hostile text; lossless CST. `kcf_fmt_invariance` (`kcf(fmt(x)) == kcf(x)`) is report-only.
