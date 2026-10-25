//! `kalam mcp`: an MCP server that exposes the compiler to coding agents
//! (spec §21.5, report 06 §7.3, ARCHITECTURE.md §18).
//!
//! Every tool is a thin wrapper over a driver entry point, so agent results equal CLI
//! results. `check` returns the spec §21.4 JSON diagnostics with all three message
//! languages; `fix` applies only fixes marked `"applicability": "machine"`.
//!
//! Status: stub (Phase 1).

/// The tools the server exposes (spec §21.5).
pub const TOOLS: &[&str] = &[
    "check",
    "fix",
    "fmt",
    "convert_skin",
    "run_tests",
    "doc_search",
    "explain",
];

/// Runs the server on stdin/stdout.
pub fn run_stdio() -> Result<(), String> {
    Err("`kalam mcp` is not implemented yet (Phase 1)".to_string())
}
