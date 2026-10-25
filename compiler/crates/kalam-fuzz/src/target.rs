//! The fuzz target: one input through lexer → parser → KCF → `kalam fmt` → `kalam convert`,
//! exactly as the CLI runs them (through `kalam-driver`), on the calling thread's stack.
//!
//! A panic propagates to the caller (the worker catches it). Besides crashes, three cheap
//! **oracles** are checked; a violation is reported, not panicked, because it is a wrong
//! answer rather than a crash and does not reset the crash-free clock:
//! - `cst_lossless`: the CST reproduces the input byte for byte;
//! - `fmt_idempotence`: `fmt(fmt(x)) == fmt(x)` when `fmt` normalized the layout;
//! - `convert_output_rejected`: converting a clean file gives a file that does not lex and
//!   parse cleanly in the target skin (spec §4.3 G1);
//! - `convert_roundtrip`: `convert(convert(x, X→Y), Y→X) == convert(x, X→X)` (spec §4.3 G2).

use kalam_base::{FileId, Skin};
use kalam_driver::FmtConfig;
use kalam_syntax::kcf;

/// What one execution observed.
#[derive(Debug, Default)]
pub struct Observed {
    /// Coarse behaviour features (diagnostic codes, stages reached), used as a cheap
    /// coverage stand-in: an input that shows a new feature is kept in the corpus.
    pub features: Vec<u32>,
    /// `Some((oracle, detail))` if an oracle failed.
    pub oracle: Option<(&'static str, String)>,
}

/// FNV-1a, for feature ids.
pub fn fnv(parts: &[&[u8]]) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for p in parts {
        for b in *p {
            h ^= u32::from(*b);
            h = h.wrapping_mul(0x0100_0193);
        }
        h ^= 0xFF;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn path_for(skin: Skin) -> String {
    format!("fuzz{}", skin.extension())
}

/// Runs the whole pipeline on `text` in `skin`.
pub fn exercise(text: &str, skin: Skin) -> Observed {
    let mut obs = Observed::default();
    let skin_id = skin.id().as_bytes();
    let file = FileId(0);

    // Lexer and parser.
    let lexed = kalam_lexer::lex(text, skin, file);
    let parsed = kalam_parser::parse(text, &lexed, skin, file);
    let mut codes: Vec<&str> = lexed
        .diagnostics
        .iter()
        .chain(parsed.diagnostics.iter())
        .map(|d| d.code)
        .collect();
    let clean = !lexed
        .diagnostics
        .iter()
        .chain(parsed.diagnostics.iter())
        .any(|d| d.is_error());
    for pair in codes.windows(2) {
        obs.features
            .push(fnv(&[b"pair", pair[0].as_bytes(), pair[1].as_bytes()]));
    }
    codes.sort_unstable();
    codes.dedup();
    for c in &codes {
        obs.features.push(fnv(&[b"code", skin_id, c.as_bytes()]));
    }
    let tokens = lexed.tokens.len();
    obs.features.push(fnv(&[
        b"tokens",
        &[(usize::BITS - tokens.leading_zeros()) as u8],
    ]));

    let back = parsed.cst.text();
    if back != text {
        obs.oracle = Some((
            "cst_lossless",
            format!("CST text has {} bytes, input {}", back.len(), text.len()),
        ));
    }

    // KCF (skin inferred and explicit) and its hash.
    let inferred = kcf::serialize(&parsed.cst);
    let explicit = kcf::serialize_in(&parsed.cst, skin);
    let _ = kcf::KcfHash::of(&explicit).to_hex();
    let _ = kcf::hash(&parsed.cst);
    if inferred == explicit {
        obs.features.push(fnv(&[b"kcf-infer-agrees", skin_id]));
    }

    // `kalam check` (lexer diagnostics through the driver).
    let path = path_for(skin);
    if let Ok(checked) = kalam_driver::check_source(&path, text) {
        let _ = checked.render_json();
    }

    // `kalam fmt`.
    let cfg = FmtConfig::default();
    match kalam_driver::format_source(&path, text, &cfg) {
        Ok(f) => {
            obs.features.push(fnv(&[
                b"fmt",
                skin_id,
                &[u8::from(f.layout_kept.is_some())],
            ]));
            if f.layout_kept.is_none() && obs.oracle.is_none() {
                if let Ok(f2) = kalam_driver::format_source(&path, &f.text, &cfg) {
                    if f2.text != f.text {
                        obs.oracle = Some(("fmt_idempotence", "fmt(fmt(x)) != fmt(x)".into()));
                    }
                }
            }
        }
        Err(_) => obs.features.push(fnv(&[b"fmt-refused", skin_id])),
    }

    // `kalam convert` to every other skin, and back.
    if clean {
        let canon = kalam_fmt::convert(text, skin, skin, &cfg).ok();
        for to in Skin::ALL {
            if to == skin {
                continue;
            }
            let Ok(c) = kalam_driver::convert_source(&path, text, to, &cfg) else {
                obs.features
                    .push(fnv(&[b"convert-refused", skin_id, to.id().as_bytes()]));
                continue;
            };
            obs.features
                .push(fnv(&[b"convert", skin_id, to.id().as_bytes()]));
            if obs.oracle.is_some() || canon.is_none() {
                continue;
            }
            match kalam_fmt::convert(&c.text, to, skin, &cfg) {
                Err(d) => {
                    let codes: Vec<&str> = d.iter().map(|d| d.code).collect();
                    obs.oracle = Some((
                        "convert_output_rejected",
                        format!("the {to} conversion of a clean {skin} file has errors {codes:?}"),
                    ));
                }
                Ok(back) if Some(&back) != canon.as_ref() => {
                    obs.oracle = Some((
                        "convert_roundtrip",
                        format!("{skin}->{to}->{skin} is not byte-identical to {skin}->{skin}"),
                    ));
                }
                Ok(_) => {}
            }
        }
    }
    obs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_program_reaches_fmt_and_convert() {
        let o = exercise("fn main():\n    print(\"hi\")\n", Skin::En);
        assert!(o.oracle.is_none(), "{:?}", o.oracle);
        assert!(o.features.contains(&fnv(&[b"fmt", b"en", &[0]])));
        assert!(o.features.contains(&fnv(&[b"convert", b"en", b"ar"])));
    }

    #[test]
    fn garbage_is_survived() {
        let o = exercise("\u{202E}((((\"{\u{0}", Skin::Ar);
        assert!(o.oracle.is_none(), "{:?}", o.oracle);
        assert!(!o.features.is_empty());
    }
}
