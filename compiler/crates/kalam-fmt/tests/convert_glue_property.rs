//! Property (spec §4.3 G1/G2, M1/M2 fuzz oracles `convert_output_rejected` and
//! `convert_roundtrip`): after conversion every token boundary re-lexes identically.
//!
//! Seeds are the fuzzer's file seeds (`kalam-fuzz` `corpus_dirs`: `eval/corpus` and
//! `eval/suites`; never `eval/holdout`). Each clean seed is "glued": every run of spaces
//! between two tokens on a line is deleted wherever the file still lexes to the same token
//! kinds and parses cleanly in its own skin (`أرجع "x"` → `أرجع"x"`, `f(a, b)` → `f(a,b)`,
//! `x = -1` → `x=-1`). That is exactly the layout the converter must not break, because it
//! copies the bytes between tokens and re-spells the tokens themselves. The fuzzer's
//! mutations reach such layouts by chance; this test reaches all of them in every seed.
//!
//! For the seed and its glued form, under both `[skin.ar] digits` settings: the fuzzer's
//! convert oracle holds ([`common::oracle_failures`]).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use kalam_base::{FileId, Skin};
use kalam_lexer::{Token, TokenKind};

/// The fuzzer's skin rule for a seed file name (`kalam-fuzz` `corpus::skin_of`).
fn skin_of(path: &Path) -> Option<Skin> {
    let name = path.file_name()?.to_str()?;
    if let Some(s) = Skin::from_path(name) {
        return Some(s);
    }
    let stem = name.strip_suffix(".kalam")?;
    let last = stem.rsplit(['.', '-', '_']).next().unwrap_or(stem);
    Skin::ALL.into_iter().find(|s| s.id() == last)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn seeds() -> Vec<(String, Skin, String)> {
    let root = repo_root();
    let mut files = Vec::new();
    for d in ["eval/corpus", "eval/suites"] {
        walk(&root.join(d), &mut files);
    }
    files
        .into_iter()
        .filter_map(|p| {
            let skin = skin_of(&p)?;
            let bytes = std::fs::read(&p).ok()?;
            let name = p
                .strip_prefix(&root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((name, skin, String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect()
}

fn layout(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
    )
}

fn solid(text: &str, skin: Skin) -> Vec<Token> {
    kalam_lexer::lex(text, skin, FileId(0))
        .tokens
        .into_iter()
        .filter(|t| t.span.end > t.span.start && !layout(t.kind))
        .collect()
}

fn kinds(tokens: &[Token]) -> Vec<TokenKind> {
    tokens.iter().map(|t| t.kind).collect()
}

/// `text` with the byte ranges in `gaps` (sorted, disjoint) deleted.
fn without(text: &str, gaps: &[(usize, usize)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for &(s, e) in gaps {
        out.push_str(&text[at..s]);
        at = e;
    }
    out.push_str(&text[at..]);
    out
}

/// The glued form of a clean `text` (see the module doc), and how many gaps it deleted.
fn glue(text: &str, skin: Skin) -> (String, usize) {
    let tokens = solid(text, skin);
    let want = kinds(&tokens);
    // Candidate gaps, grouped by line: spaces only, and the two tokens re-lex alone as the
    // same pair once glued.
    let mut by_line: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut line_of_gap = usize::MAX;
    for w in tokens.windows(2) {
        let (s, e) = (w[0].span.end as usize, w[1].span.start as usize);
        let gap = &text[s..e];
        if gap.is_empty() || !gap.bytes().all(|b| b == b' ') {
            continue;
        }
        let pair = format!("{}{}", &text[w[0].span.range()], &text[w[1].span.range()]);
        let alone = solid(&pair, skin);
        if kinds(&alone) != [w[0].kind, w[1].kind]
            || alone[0].span.end != w[0].span.end - w[0].span.start
        {
            continue;
        }
        let line = text[..s].matches('\n').count();
        if line != line_of_gap {
            by_line.push(Vec::new());
            line_of_gap = line;
        }
        by_line.last_mut().unwrap().push((s, e));
    }
    // Delete line by line, keeping a line's deletions only if the whole file still lexes to
    // the same token kinds and parses cleanly.
    let mut kept: Vec<(usize, usize)> = Vec::new();
    for line in by_line {
        let mut trial = kept.clone();
        trial.extend(line);
        let t = without(text, &trial);
        if kinds(&solid(&t, skin)) == want && errors(&t, skin).is_empty() {
            kept = trial;
        }
    }
    let n = kept.len();
    (without(text, &kept), n)
}

#[test]
fn glued_fuzz_seeds_convert_cleanly_in_every_skin() {
    let mut failures = Vec::new();
    let (mut clean, mut gaps) = (0usize, 0usize);
    for (name, skin, text) in seeds() {
        if !errors(&text, skin).is_empty() {
            continue; // the fuzzer's convert oracle only runs on clean inputs
        }
        clean += 1;
        let (glued, n) = glue(&text, skin);
        gaps += n;
        for config in configs() {
            failures.extend(oracle_failures(&name, &text, skin, &config));
            if n > 0 {
                failures.extend(oracle_failures(
                    &format!("{name} (glued)"),
                    &glued,
                    skin,
                    &config,
                ));
            }
        }
    }
    eprintln!("{clean} clean seeds, {gaps} gaps glued");
    assert!(clean > 150, "too few clean seeds: {clean}");
    assert!(gaps > 1000, "too few gaps glued: {gaps}");
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

#[test]
fn glue_finds_the_keyword_string_layout() {
    // The glue step itself: `أرجع "x"` is glued in `ar` (a keyword and a string), while
    // `return "x"` is not in `en` (`return"x"` would be a tagged literal).
    let (ar, n) = glue("دالة f() -> نص:\n    أرجع \"x\"\n", Skin::Ar);
    assert_eq!(ar, "دالة f()->نص:\n    أرجع\"x\"\n");
    assert_eq!(n, 3);
    let (en, _) = glue("fn f() -> String:\n    return \"x\"\n", Skin::En);
    assert_eq!(en, "fn f()->String:\n    return \"x\"\n");
}
