//! `kalam lsp`: the language server (ARCHITECTURE.md §14).
//!
//! Planned on `tower-lsp` + `lsp-types`, over one driver session per workspace:
//! diagnostics; hover (type, docs, "suspends" with its call chain); go to definition;
//! completion with keywords and core names in the **file's skin**; code actions from machine
//! fixes; semantic tokens (per-token ranges, used for UTS #55 isolation in RTL files);
//! formatting; rename with skeleton-collision checks; references; inlay hints (`⏸` at every
//! suspension point, inferred `let` types). Skin features: "convert file to…", glossary
//! display mode, H0202 hints, and conversion of English keywords pasted into `ar`/`franco`
//! files (spec X1).
//!
//! Positions: the driver stores byte offsets; the server converts to the negotiated
//! `positionEncoding` (UTF-16 by default), which matters for Arabic text.
//!
//! Status: stub (Phase 1–2).

/// LSP capabilities in the order they will be implemented (05 D7 coverage metric).
pub const PLANNED_CAPABILITIES: &[&str] = &[
    "diagnostics",
    "hover",
    "definition",
    "completion",
    "codeAction",
    "semanticTokens",
    "formatting",
    "rename",
    "references",
    "inlayHint",
];

/// Runs the server on stdin/stdout.
pub fn run_stdio() -> Result<(), String> {
    Err("`kalam lsp` is not implemented yet (Phase 1)".to_string())
}
