//! The Trojan Source suite (`eval/suites/trojan-source/<case>/`, eval
//! `D2.trojan_source_suite`) read as **bytes** and judged on `kalam_lexer::lex_bytes`, with
//! the pass rule of `eval/suites/README.md`:
//!
//! - `must_reject: true`: every expected code is reported, the first report of `codes[i]` is
//!   on `lines[i]` (LF-counted; the same in the bytes and the decoded text), and every
//!   expected `SEC` code names its code point (`U+XXXX` of a character under its span, or
//!   on its line, in the decoded text);
//! - `must_reject: false`: no error-severity diagnostic at all.
//!
//! Every case in the suite is in the lexer's reach (source-text, identifier and digit rules),
//! so every non-`[M2]` case must pass here, the invalid-UTF-8 ones included (SEC0100,
//! spec §4.1, §5.1). Cases whose notes start with `[M2]` need name resolution and are skipped.

use std::path::{Path, PathBuf};

use kalam_base::Skin;

fn suite_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../eval/suites/trojan-source")
}

fn line_of(bytes: &[u8], offset: usize) -> usize {
    bytes[..offset.min(bytes.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// `U+XXXX` code points named in `message` (printable ASCII never counts).
fn named(message: &str) -> Vec<char> {
    let mut out = Vec::new();
    for (i, _) in message.match_indices("U+") {
        let hex: String = message[i + 2..]
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .take(6)
            .collect();
        if hex.len() >= 4 {
            if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                if !(' '..='~').contains(&c) {
                    out.push(c);
                }
            }
        }
    }
    out
}

fn judge(expect: &serde_json::Value, bytes: &[u8], skin: Skin) -> Result<(), String> {
    let out = kalam_lexer::lex_bytes(bytes, skin);
    let text = &out.text;
    let errors: Vec<_> = out.diagnostics.iter().filter(|d| d.is_error()).collect();
    let line = |offset: u32| line_of(text.as_bytes(), offset as usize);
    let got: Vec<String> = errors
        .iter()
        .map(|d| format!("{}@{}", d.code, line(d.span.start)))
        .collect();
    if !expect["must_reject"].as_bool().expect("must_reject") {
        return if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("expected no error, got {got:?}"))
        };
    }
    let codes = expect["codes"].as_array().cloned().unwrap_or_default();
    let mut problems = Vec::new();
    for (i, code) in codes.iter().enumerate() {
        let code = code.as_str().expect("code");
        let Some(d) = errors.iter().find(|d| d.code == code) else {
            problems.push(format!("missing {code} (got {got:?})"));
            continue;
        };
        let at = line(d.span.start);
        if let Some(l) = expect["lines"].get(i).and_then(|n| n.as_u64()) {
            if l as usize != at {
                problems.push(format!("first {code} is on line {at}, expected {l}"));
            }
        }
        if code.starts_with("SEC") {
            let under = text.get(d.span.range()).unwrap_or("");
            let context = if under.is_empty() {
                text.split('\n').nth(at - 1).unwrap_or("")
            } else {
                under
            };
            if !named(&d.message(Skin::En))
                .iter()
                .any(|c| context.contains(*c))
            {
                problems.push(format!("{code} does not name the code point it reports"));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

#[test]
fn every_trojan_source_case_passes_on_bytes() {
    let Ok(entries) = std::fs::read_dir(suite_dir()) else {
        return; // the suites are not checked out
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    let mut judged = 0;
    let mut invalid_utf8 = 0;
    let mut failures = Vec::new();
    for dir in dirs {
        let Ok(expect) = std::fs::read_to_string(dir.join("expect.json")) else {
            continue;
        };
        let expect: serde_json::Value = serde_json::from_str(&expect).expect("expect.json");
        if expect["notes"].as_str().unwrap_or("").starts_with("[M2]") {
            continue;
        }
        judged += 1;
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        files.sort_by_key(|f| f.file_name());
        for f in files {
            let name = f.file_name().to_string_lossy().into_owned();
            let Some(skin) = Skin::from_path(&name) else {
                continue;
            };
            let bytes = std::fs::read(f.path()).unwrap();
            if std::str::from_utf8(&bytes).is_err() {
                invalid_utf8 += 1;
            }
            if let Err(why) = judge(&expect, &bytes, skin) {
                let case = dir.file_name().unwrap().to_string_lossy().into_owned();
                failures.push(format!("{case}/{name}: {why}"));
            }
        }
    }
    if judged == 0 {
        return;
    }
    assert!(judged >= 100, "only {judged} cases");
    assert!(invalid_utf8 >= 2, "the invalid-UTF-8 cases are missing");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
