//! `kalam test` (ARCHITECTURE.md §16).
//!
//! 1. Discover `test "name":` blocks in `src/` (module-private access) and `test/`.
//! 2. Build the package in the test profile (test blocks are never in production builds).
//! 3. Generate `__kalam_tests__.mjs`, which registers every test, and run it under Node with
//!    one task per test and fresh fake capabilities from `std/test`.
//! 4. Report as human text (`KALAM_LANG`), JSON (for agents and `kalam mcp`) or TAP, with
//!    failures mapped back to Kalam lines through source maps.
//!
//! Conformance and skin-parity runs (05 P1–P5) are an `xtask`, not this runner.
//!
//! Status: data types only (Phase 1).

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    /// Module path plus the test's string name.
    pub module: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestOutcome {
    Passed,
    Failed { message: String },
    Panicked { message: String },
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reporter {
    Human,
    Json,
    Tap,
}

#[derive(Clone, Debug)]
pub struct TestOptions {
    pub filter: Option<String>,
    pub update_snapshots: bool,
    pub reporter: Reporter,
}

/// Runs the package's tests. Not implemented yet; returns an explanation instead.
pub fn run(_package_dir: &str, _options: &TestOptions) -> Result<Vec<(TestCase, TestOutcome)>, String> {
    Err("`kalam test` needs the parser, type checker and JS emitter (Phase 1)".to_string())
}
