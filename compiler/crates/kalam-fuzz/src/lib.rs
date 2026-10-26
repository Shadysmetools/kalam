//! A coverage-agnostic mutational fuzzer for the Kalam compiler (`D1.fuzz_crash_free_hours`,
//! targets in [`target::TARGETS`]: lexer, parser, KCF, fmt, convert, and from M2 the type
//! checker and the reference interpreter).
//!
//! `cargo-fuzz`/libFuzzer does not build on the windows-gnu toolchain, so this is an
//! in-house fuzzer: seeds from `eval/corpus`, `eval/suites` and generated programs;
//! byte, token and grammar-aware mutations; worker processes so that stack overflows and
//! aborts are caught too; crashes in `eval/fuzz/crashes/`; the clock in
//! `eval/fuzz/status.json`. See `kalam-fuzz --help`.

pub mod corpus;
pub mod mutate;
pub mod proto;
pub mod status;
pub mod supervisor;
pub mod target;
pub mod util;
pub mod worker;
