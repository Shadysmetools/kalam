//! A coverage-agnostic mutational fuzzer for the Kalam front end (`D1.fuzz_crash_free_hours`,
//! M1 scope: lexer, parser and formatter targets; here also KCF and skin conversion).
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
