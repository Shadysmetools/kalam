//! Compile-time detection of the raw-bytes entry points (spec §5.1, SEC0100: invalid UTF-8).
//!
//! Rust cannot ask whether a function exists, so this script reads the public API source of
//! the crates it depends on and turns on a `cfg` only when the function is there **with the
//! signature the harness calls**:
//!
//! - `kalam_check_bytes`: `kalam_driver::check_bytes(&str, &[u8]) -> Result<CheckedFile, NotKalamFile>`
//!   (or `-> CheckedFile`), the `kalam check` entry point for raw file bytes;
//! - `kalam_lex_bytes`: `kalam_lexer::lex_bytes(&[u8], Skin) -> BytesLexOutput` (diagnostic
//!   spans index its lossy `text`).
//!
//! A function that exists with another signature turns nothing on; the reason is passed to
//! the crate as `KALAM_EVAL_BYTES_API` so the harness reports those cases as not measurable
//! with that reason (never as a pass, never as a build failure).

use std::fs;
use std::path::Path;

/// `pub fn <name>(...) -> R` in `src` as `(parameter types, return type)`, whitespace
/// removed. `None` when there is no such public function.
pub fn signature(src: &str, name: &str) -> Option<(Vec<String>, String)> {
    let start = src.find(&format!("pub fn {name}("))?;
    let rest = &src[start + "pub fn ".len() + name.len() + 1..];
    let close = rest.find(')')?;
    let squash = |s: &str| s.split_whitespace().collect::<String>();
    let params: Vec<String> = rest[..close]
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| squash(p.split_once(':').map_or(p, |(_, t)| t)))
        .collect();
    let after = &rest[close + 1..];
    let body = after.find(['{', ';']).unwrap_or(after.len());
    let head = &after[..body];
    let head = head.split(" where ").next().unwrap_or(head);
    let ret = head
        .trim()
        .strip_prefix("->")
        .map(squash)
        .unwrap_or_default();
    Some((params, ret))
}

/// `Ok(true)`: present with an accepted signature; `Ok(false)`: absent; `Err(why)`: present
/// with another signature.
pub fn detect(src: &str, name: &str, params: &[&str], returns: &[&str]) -> Result<bool, String> {
    let Some((p, r)) = signature(src, name) else {
        return Ok(false);
    };
    if p == params && returns.contains(&r.as_str()) {
        Ok(true)
    } else {
        Err(format!(
            "{name} exists as ({}) -> {r}, but the harness calls ({}) -> {}",
            p.join(", "),
            params.join(", "),
            returns.join(" | ")
        ))
    }
}

/// `(file under crates/, function, cfg, parameter types, accepted return types)`.
type Probe = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

fn main() {
    println!("cargo:rustc-check-cfg=cfg(kalam_check_bytes)");
    println!("cargo:rustc-check-cfg=cfg(kalam_lex_bytes)");
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let crates = Path::new(&dir).join("..");
    let probes: [Probe; 2] = [
        (
            "kalam-driver/src/lib.rs",
            "check_bytes",
            "kalam_check_bytes",
            &["&str", "&[u8]"],
            &["Result<CheckedFile,NotKalamFile>", "CheckedFile"],
        ),
        (
            "kalam-lexer/src/lib.rs",
            "lex_bytes",
            "kalam_lex_bytes",
            &["&[u8]", "Skin"],
            &["BytesLexOutput"],
        ),
    ];
    let mut notes = Vec::new();
    for (file, name, cfg, params, returns) in probes {
        let path = crates.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let src = fs::read_to_string(&path).unwrap_or_default();
        match detect(&src, name, params, returns) {
            Ok(true) => println!("cargo:rustc-cfg={cfg}"),
            Ok(false) => notes.push(format!("no pub fn {name} in {file}")),
            Err(why) => {
                println!("cargo:warning=kalam-eval: {why}; SEC0100 byte cases stay not measurable");
                notes.push(why);
            }
        }
    }
    println!("cargo:rustc-env=KALAM_EVAL_BYTES_API={}", notes.join("; "));
}
