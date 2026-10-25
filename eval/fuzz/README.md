# eval/fuzz: fuzzing state (`D1.fuzz_crash_free_hours`)

Written by `kalam-fuzz` (crate `compiler/crates/kalam-fuzz`):

    cargo build -p kalam-fuzz --release
    cp target/release/kalam-fuzz.exe <scratch>/kalam-fuzz.exe
    <scratch>/kalam-fuzz.exe --hours 24 --jobs 4

On Windows a running exe is locked: run a copy from outside `target/`, or every
`cargo build --release` of the workspace fails while the fuzzer runs. (A copy hashes the
same, so the clock still resumes.)

- `status.json`: `crash_free_hours`, `total_execs`, `crashes`, `last_run`, `version`
  (SHA-256 of the fuzz target binary), plus `crash_list`, `oracle_violations`,
  `execs_per_sec`, `version_execs`, `runs` and the replay counters.
- `crashes/<kind>-<sig>.<skin>.kl`: minimized crash inputs (kinds: `panic`,
  `stack_overflow`, `abort`, `timeout`, `crash`), with `<kind>-<sig>.json` metadata and the
  unminimized `<kind>-<sig>.orig.txt`. Reproduce with
  `kalam-fuzz --repro eval/fuzz/crashes/<file> --skin <skin>`. Every run re-checks them first.
- `oracle/`: inputs that ran without crashing but broke an oracle (`cst_lossless`,
  `fmt_idempotence`, `convert_roundtrip`). They do not stop the clock.

Clock rule: crash-free time accumulates across runs only while `version` is unchanged and no
crash is open on that binary; a rebuilt binary or any crash resets it to 0.
