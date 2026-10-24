//! The playground build: the frontend, formatter and JS emitter compiled to
//! `wasm32-unknown-unknown` (D10, ARCHITECTURE.md §17).
//!
//! These functions take and return plain strings so the crate builds without extra
//! dependencies. Phase 1 adds `wasm-bindgen` attributes on top of them. No file system, no
//! network, no package manager: the playground supplies sources through an in-memory VFS,
//! and the keyword table is embedded in the binary.

use kalam_base::Skin;
use kalam_fmt::FmtConfig;

/// `check(path, source)` → the spec §21.4 JSON diagnostics array.
pub fn check(path: &str, source: &str) -> String {
    match kalam_driver::check_source(path, source) {
        Ok(checked) => checked.render_json(),
        Err(_) => "[]".to_string(),
    }
}

/// `convert(source, from, to)` → the converted source, or the JSON diagnostics if the source
/// does not lex cleanly. Skin IDs are `en`, `ar` and `franco`.
pub fn convert(source: &str, from: &str, to: &str) -> Result<String, String> {
    let from = Skin::from_id(from).ok_or_else(|| format!("unknown skin `{from}`"))?;
    let to = Skin::from_id(to).ok_or_else(|| format!("unknown skin `{to}`"))?;
    kalam_fmt::convert(source, from, to, &FmtConfig::default()).map_err(|diagnostics| {
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code).collect();
        format!("source has errors: {}", codes.join(", "))
    })
}
