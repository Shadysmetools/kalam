//! The JavaScript backend: KIR → ES2022 modules, Source Map v3 and `.d.ts`
//! (spec §22, ARCHITECTURE.md §11).
//!
//! How suspension lowers (no `async` keyword in Kalam, D12):
//! - a KIR function with `suspends = true` becomes `async function f($t, …)`, where `$t` is
//!   the runtime task context (cancellation signal, scope, open transactions);
//! - a call with `suspends = true` becomes `await g($t, …)`; sync calls are plain calls;
//! - effect-polymorphic functions are emitted as `f` and `f$s` when both are used;
//! - `Coerce::SyncToSusp` emits nothing, because `await` accepts plain values;
//! - `panic` becomes `throw new $Panic(msg)`, which only runtime boundaries catch.
//!
//! Generated code never contains `eval`, `new Function`, `with`, `==`/`!=`, `for…in`, or
//! property access on plain objects keyed by user data (04 P14); CI scans every output.
//!
//! Status: the backend seam and the source-map VLQ encoder. Code generation is Phase 1.

use kalam_ir::{Artifact, Backend, BackendError, EmitOptions, Program, Target};

/// Helpers the generated code imports from the vendored `@kalam/runtime` (spec §22).
pub const RUNTIME_HELPERS: &[&str] = &[
    "$i",
    "$floordiv",
    "$mod",
    "$Panic",
    "$Ok",
    "$Err",
    "$Some",
    "$eq",
    "$List",
    "$Map",
    "$box",
    "$task",
    "$Sql",
    "$Html",
];

/// Constructs that must never appear in emitted JS (checked by a CI scan of all output).
pub const FORBIDDEN_OUTPUT: &[&str] = &[
    "eval(",
    "new Function",
    "with (",
    " == ",
    " != ",
    "for (const k in",
];

pub struct JsBackend;

impl Backend for JsBackend {
    fn name(&self) -> &'static str {
        "js"
    }

    fn targets(&self) -> &'static [Target] {
        &[Target::JsNode, Target::JsBrowser]
    }

    fn emit(
        &self,
        _program: &Program,
        _options: &EmitOptions,
    ) -> Result<Vec<Artifact>, BackendError> {
        Err(BackendError::Unimplemented("JS code generation (Phase 1)"))
    }
}

/// Source Map v3 support.
pub mod sourcemap {
    const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Appends one Base64 VLQ value (Source Map v3 encoding).
    pub fn encode_vlq(value: i64, out: &mut String) {
        let mut v: u64 = if value < 0 {
            ((value.unsigned_abs()) << 1) | 1
        } else {
            (value as u64) << 1
        };
        loop {
            let mut digit = (v & 0b11111) as usize;
            v >>= 5;
            if v != 0 {
                digit |= 0b100000;
            }
            out.push(BASE64[digit] as char);
            if v == 0 {
                break;
            }
        }
    }

    /// One mapping: generated position → original position in the `.kl`/`.ar.kl`/
    /// `.franco.kl` file exactly as written (0-based lines and columns).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Mapping {
        pub generated_line: u32,
        pub generated_col: u32,
        pub source: u32,
        pub original_line: u32,
        pub original_col: u32,
    }

    /// Encodes the `mappings` field. Mappings must be sorted by generated position.
    pub fn encode_mappings(mappings: &[Mapping]) -> String {
        let mut out = String::new();
        let mut line = 0u32;
        let mut prev_gen_col = 0i64;
        let mut prev_source = 0i64;
        let mut prev_orig_line = 0i64;
        let mut prev_orig_col = 0i64;
        let mut first_in_line = true;
        for m in mappings {
            while line < m.generated_line {
                out.push(';');
                line += 1;
                prev_gen_col = 0;
                first_in_line = true;
            }
            if !first_in_line {
                out.push(',');
            }
            first_in_line = false;
            encode_vlq(m.generated_col as i64 - prev_gen_col, &mut out);
            encode_vlq(m.source as i64 - prev_source, &mut out);
            encode_vlq(m.original_line as i64 - prev_orig_line, &mut out);
            encode_vlq(m.original_col as i64 - prev_orig_col, &mut out);
            prev_gen_col = m.generated_col as i64;
            prev_source = m.source as i64;
            prev_orig_line = m.original_line as i64;
            prev_orig_col = m.original_col as i64;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::sourcemap::*;

    fn vlq(v: i64) -> String {
        let mut s = String::new();
        encode_vlq(v, &mut s);
        s
    }

    #[test]
    fn vlq_matches_the_source_map_spec() {
        assert_eq!(vlq(0), "A");
        assert_eq!(vlq(1), "C");
        assert_eq!(vlq(-1), "D");
        assert_eq!(vlq(15), "e");
        assert_eq!(vlq(16), "gB");
    }

    #[test]
    fn mappings_use_relative_fields_and_line_separators() {
        let m = [
            Mapping {
                generated_line: 0,
                generated_col: 0,
                source: 0,
                original_line: 0,
                original_col: 0,
            },
            Mapping {
                generated_line: 0,
                generated_col: 9,
                source: 0,
                original_line: 0,
                original_col: 3,
            },
            Mapping {
                generated_line: 2,
                generated_col: 2,
                source: 0,
                original_line: 1,
                original_col: 4,
            },
        ];
        assert_eq!(encode_mappings(&m), "AAAA,SAAG;;EACC");
    }
}
