//! The Trojan Source and lexing-hazard suites (`eval/suites/<suite>/<case>/`, eval
//! `D2.trojan_source_suite` and `D2.lexing_hazard_suite`) judged on lexer + parser
//! diagnostics (`kalam_parser::parse_source`), with the pass rule of `eval/suites/README.md`:
//!
//! - `must_reject: true`: every expected code is reported, the first report of `codes[i]` is
//!   on `lines[i]` (LF-counted), and every expected `SEC` code names its code point (`U+XXXX`
//!   of a character under its span or on its line);
//! - `must_reject: false`: no error-severity diagnostic at all.
//!
//! Files are read as bytes (spec §4.1, §5.1: encoding is judged on the file's bytes). Valid
//! UTF-8 goes through `parse_source`; anything else goes through `kalam_lexer::lex_bytes`
//! plus the parser, exactly as `kalam_driver::check_bytes` (`kalam check`) does, and is
//! judged against the decoded text.
//!
//! Cases whose notes start with `[M2]` need name resolution and are skipped. A case listed in
//! [`KNOWN_GAPS`] is expected to fail for a documented reason; the test fails if one of them
//! starts passing, so the list only shrinks. It is empty now.

use std::path::{Path, PathBuf};

use kalam_base::{FileId, Skin};
use kalam_diagnostics::Diagnostic;

/// `suite/case`: failing today for a documented reason (see the module doc).
const KNOWN_GAPS: &[&str] = &[];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root")
}

fn line_of(text: &str, offset: u32) -> usize {
    text.as_bytes()[..(offset as usize).min(text.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// The text of the 1-based line `line` (LF-separated).
fn line_text(text: &str, line: usize) -> &str {
    text.split('\n').nth(line - 1).unwrap_or("")
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

struct Expect {
    must_reject: bool,
    codes: Vec<String>,
    lines: Vec<Option<usize>>,
    m2: bool,
}

fn expect_of(dir: &Path) -> Expect {
    let text = std::fs::read_to_string(dir.join("expect.json")).expect("expect.json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("valid expect.json");
    let codes: Vec<String> = v["codes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let lines = (0..codes.len())
        .map(|i| {
            v["lines"]
                .get(i)
                .and_then(|n| n.as_u64())
                .map(|n| n as usize)
        })
        .collect();
    Expect {
        must_reject: v["must_reject"].as_bool().expect("must_reject"),
        codes,
        lines,
        m2: v["notes"].as_str().unwrap_or("").starts_with("[M2]"),
    }
}

/// The text the diagnostics' spans index, and lexer + parser diagnostics, for a file's bytes.
fn check(bytes: &[u8], skin: Skin) -> (String, Vec<Diagnostic>) {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let out = kalam_parser::parse_source(text, skin, FileId(0));
            (text.to_string(), out.diagnostics)
        }
        Err(_) => {
            let lexed = kalam_lexer::lex_bytes(bytes, skin);
            let parsed = kalam_parser::parse(&lexed.text, &lexed.lexed, skin, FileId(0));
            let diagnostics = kalam_parser::merge_diagnostics(
                &lexed.text,
                lexed.lexed.diagnostics.clone(),
                parsed.diagnostics,
            );
            (lexed.text, diagnostics)
        }
    }
}

/// `Err(reason)` if the file does not pass its case.
fn judge(expect: &Expect, bytes: &[u8], skin: Skin) -> Result<(), String> {
    let (text, diagnostics) = check(bytes, skin);
    let text = text.as_str();
    let errors: Vec<_> = diagnostics.iter().filter(|d| d.is_error()).collect();
    let got = || -> Vec<String> {
        errors
            .iter()
            .map(|d| format!("{}@{}", d.code, line_of(text, d.span.start)))
            .collect()
    };
    if !expect.must_reject {
        return if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("expected no error, got {:?}", got()))
        };
    }
    let mut problems = Vec::new();
    for (code, line) in expect.codes.iter().zip(&expect.lines) {
        let Some(d) = errors.iter().find(|d| d.code == code) else {
            problems.push(format!("missing {code} (got {:?})", got()));
            continue;
        };
        let at = line_of(text, d.span.start);
        if let Some(l) = line.filter(|l| *l != at) {
            problems.push(format!("first {code} is on line {at}, expected {l}"));
        }
        if code.starts_with("SEC") {
            let under = text.get(d.span.range()).unwrap_or("");
            let context = if under.is_empty() {
                line_text(text, at)
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

/// Every failing `(suite/case, reason)` in `suite`, and the number of cases judged.
fn run(suite: &str) -> (Vec<(String, String)>, usize) {
    let root = repo_root().join("eval").join("suites").join(suite);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return (Vec::new(), 0);
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    let mut failures = Vec::new();
    let mut judged = 0;
    for dir in dirs.iter().filter(|d| d.join("expect.json").exists()) {
        let expect = expect_of(dir);
        if expect.m2 {
            continue;
        }
        judged += 1;
        let id = format!("{suite}/{}", dir.file_name().unwrap().to_string_lossy());
        let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
        files.sort_by_key(|f| f.file_name());
        for f in files {
            let name = f.file_name().to_string_lossy().into_owned();
            let Some(skin) = Skin::from_path(&name) else {
                continue;
            };
            let bytes = std::fs::read(f.path()).unwrap();
            if let Err(why) = judge(&expect, &bytes, skin) {
                failures.push((id.clone(), format!("{name}: {why}")));
            }
        }
    }
    (failures, judged)
}

fn check_suite(suite: &str, min_cases: usize) {
    let (failures, judged) = run(suite);
    if judged == 0 {
        return; // the suites are not checked out
    }
    assert!(judged >= min_cases, "{suite}: only {judged} cases");
    let unexpected: Vec<String> = failures
        .iter()
        .filter(|(id, _)| !KNOWN_GAPS.contains(&id.as_str()))
        .map(|(id, why)| format!("{id}/{why}"))
        .collect();
    let fixed: Vec<&&str> = KNOWN_GAPS
        .iter()
        .filter(|g| g.starts_with(&format!("{suite}/")))
        .filter(|g| !failures.iter().any(|(id, _)| id == *g))
        .collect();
    assert!(
        unexpected.is_empty() && fixed.is_empty(),
        "unexpected failures:\n{}\nknown gaps that now pass (remove them): {fixed:?}",
        unexpected.join("\n")
    );
}

#[test]
fn trojan_source_suite() {
    check_suite("trojan-source", 100);
}

#[test]
fn lexing_hazard_suite() {
    check_suite("lexing-hazards", 40);
}
