//! `kcf(fmt(x)) == kcf(x)` (spec §4.4 KCF2, FOUNDER-QUEUE Q8; eval `D7.formatter_idempotence`):
//! `kalam fmt` changes layout only, and KCF records no layout `fmt` may change.
//!
//! Checked on:
//! - the non-canonical repros that broke KCF1: `f(a, b,)`, blank lines in brackets, three
//!   blank lines, a blank line at the start of a block, a missing blank line between items,
//!   and a one-line `if` whose body is too long to stay on one line;
//! - every eval corpus file in all three skins, as written and with its blank lines
//!   tripled or removed;
//! - every `eval/suites/conformance/` case (non-canonical layout, `must_reject: false`).

use std::path::{Path, PathBuf};

use kalam_base::{FileId, Skin};
use kalam_fmt::FmtConfig;
use kalam_syntax::kcf;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root")
}

/// The KCF of `text`, or `None` if it does not lex and parse cleanly.
fn kcf_clean(text: &str, skin: Skin) -> Option<String> {
    let out = kalam_parser::parse_source(text, skin, FileId(0));
    if out.diagnostics.iter().any(|d| d.is_error()) {
        return None;
    }
    Some(String::from_utf8(kcf::serialize(&out.cst)).expect("KCF is UTF-8"))
}

/// `Err(report)` if `fmt` changes the KCF of `text`; `Ok(false)` if `text` is not a clean
/// source (nothing to check).
fn check(name: &str, text: &str, skin: Skin) -> Result<bool, String> {
    let Some(before) = kcf_clean(text, skin) else {
        return Ok(false);
    };
    let formatted = kalam_fmt::format(text, skin, &FmtConfig::default())
        .map_err(|d| format!("{name}: fmt failed: {d:?}"))?;
    let after = kcf_clean(&formatted, skin)
        .ok_or_else(|| format!("{name}: fmt output does not parse:\n{formatted}"))?;
    if before == after {
        Ok(true)
    } else {
        Err(format!(
            "{name} ({skin:?}): kcf(fmt(x)) != kcf(x)\n--- x\n{text}\n--- fmt(x)\n{formatted}\n{}",
            first_difference(&before, &after)
        ))
    }
}

fn first_difference(a: &str, b: &str) -> String {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let i = la.iter().zip(&lb).take_while(|(x, y)| x == y).count();
    let ctx = |l: &[&str]| l[i.saturating_sub(3)..(i + 3).min(l.len())].join(" | ");
    format!(
        "first difference at KCF record {i}:\n  x:      {}\n  fmt(x): {}",
        ctx(&la),
        ctx(&lb)
    )
}

#[track_caller]
fn assert_kcf_fmt(text: &str, skin: Skin) {
    match check("repro", text, skin) {
        Ok(true) => {}
        Ok(false) => panic!("{skin:?}: the repro does not parse:\n{text}"),
        Err(report) => panic!("{report}"),
    }
}

/// The repro in all three skins (converted from English), each checked on its own.
#[track_caller]
fn assert_kcf_fmt_all_skins(en: &str) {
    assert_kcf_fmt(en, Skin::En);
    for skin in [Skin::Ar, Skin::Franco] {
        let text = kalam_fmt::convert(en, Skin::En, skin, &FmtConfig::default())
            .unwrap_or_else(|d| panic!("convert to {skin:?}: {d:?}"));
        assert_kcf_fmt(&text, skin);
    }
}

#[test]
fn trailing_comma_repro() {
    assert_kcf_fmt_all_skins("fn main():\n    f(a, b,)\n");
    assert_kcf_fmt_all_skins("fn add(a: Int, b: Int,) -> Int:\n    return a + b\n");
    assert_kcf_fmt_all_skins("fn main():\n    let t = (1,)\n    let u = (1, 2,)\n");
}

#[test]
fn blank_lines_in_brackets_repro() {
    assert_kcf_fmt_all_skins("fn main():\n    f(\n\n        a,\n\n        b\n\n    )\n");
}

#[test]
fn triple_blank_lines_repro() {
    assert_kcf_fmt_all_skins(
        "fn one() -> Int:\n    return 1\n\n\n\nfn main():\n    let x = one()\n\n\n\n    print(x)\n",
    );
}

#[test]
fn block_start_blank_line_repro() {
    assert_kcf_fmt_all_skins("fn main():\n\n    print(1)\n");
    assert_kcf_fmt_all_skins("fn main():\n\n    # first\n\n    print(1)\n");
}

#[test]
fn missing_blank_line_between_items_repro() {
    assert_kcf_fmt_all_skins("fn a():\n    pass\nfn main():\n    a()\n");
    assert_kcf_fmt_all_skins("fn a():\n    pass\n# about main\nfn main():\n    a()\n");
}

#[test]
fn one_line_if_with_long_body_repro() {
    let long = "x".repeat(90);
    let en = format!("fn main():\n    let c = 1\n    if c > 0: print(\"{long}\")\n");
    // The body no longer fits: `fmt` indents it.
    let formatted = kalam_fmt::format(&en, Skin::En, &FmtConfig::default()).unwrap();
    assert!(
        formatted.contains("if c > 0:\n        print("),
        "{formatted}"
    );
    assert_kcf_fmt_all_skins(&en);
    // A short one stays on one line: same KCF either way.
    assert_kcf_fmt_all_skins("fn main():\n    let c = 1\n    if c > 0: print(c)\n");
}

/// Every `eval/corpus/<task>[/<case>]/{en,ar,franco}.kalam`.
fn corpus_files() -> Vec<(String, Skin, String)> {
    let root = repo_root().join("eval").join("corpus");
    let mut dirs = Vec::new();
    for task in std::fs::read_dir(&root).expect("eval/corpus").flatten() {
        let task = task.path();
        if task.is_dir() {
            dirs.push(task.clone());
            for sub in std::fs::read_dir(&task).unwrap().flatten() {
                if sub.path().is_dir() {
                    dirs.push(sub.path());
                }
            }
        }
    }
    dirs.sort();
    let mut out = Vec::new();
    for d in dirs {
        for skin in Skin::ALL {
            let p = d.join(format!("{}.kalam", skin.id()));
            if let Ok(text) = std::fs::read_to_string(&p) {
                let name = p
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((name, skin, text));
            }
        }
    }
    out
}

#[test]
fn corpus_files_and_their_blank_line_mutations() {
    let files = corpus_files();
    assert!(files.len() >= 42, "found {} corpus files", files.len());
    let mut checked = 0;
    let mut failures = Vec::new();
    for (name, skin, text) in &files {
        let mutations = [
            ("as written", text.clone()),
            ("blank lines tripled", text.replace("\n\n", "\n\n\n\n")),
            ("blank lines removed", text.replace("\n\n", "\n")),
        ];
        for (how, m) in mutations {
            match check(&format!("{name} [{how}]"), &m, *skin) {
                Ok(true) => checked += 1,
                Ok(false) => {}
                Err(report) => failures.push(report),
            }
        }
    }
    eprintln!("kcf(fmt(x)) == kcf(x): {checked} corpus sources checked");
    assert!(checked >= 100, "only {checked} corpus sources parse");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Conformance cases whose `fmt` output changes the KCF for a reason KCF2 does not cover,
/// each with the reason. The test fails if one of them starts to pass, so the list stays
/// true.
const KNOWN_GAPS: &[(&str, &str)] = &[];

#[test]
fn conformance_suite() {
    let root = repo_root().join("eval").join("suites").join("conformance");
    let Ok(entries) = std::fs::read_dir(&root) else {
        eprintln!("no eval/suites/conformance: nothing to check");
        return;
    };
    let mut cases: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    cases.sort();
    let mut checked = 0;
    let mut failures = Vec::new();
    let mut gaps_seen = Vec::new();
    for case in cases {
        let Ok(expect) = std::fs::read_to_string(case.join("expect.json")) else {
            continue;
        };
        let expect: serde_json::Value = serde_json::from_str(&expect).expect("expect.json");
        if expect["must_reject"] != false {
            continue;
        }
        let id = case.file_name().unwrap().to_string_lossy().into_owned();
        for file in std::fs::read_dir(&case).unwrap().flatten() {
            let path = file.path();
            let fname = path.file_name().unwrap().to_string_lossy().into_owned();
            let Some(skin) = Skin::from_path(&fname) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let gap = KNOWN_GAPS.iter().find(|(g, _)| *g == id);
            match (
                check(&format!("conformance/{id}/{fname}"), &text, skin),
                gap,
            ) {
                (Ok(true), None) => checked += 1,
                (Ok(true), Some(_)) => failures.push(format!(
                    "conformance/{id}: listed in KNOWN_GAPS but now passes; remove it"
                )),
                (Ok(false), _) => {} // does not parse yet: other checks report it
                (Err(_), Some(_)) => gaps_seen.push(id.clone()),
                (Err(report), None) => failures.push(report),
            }
        }
    }
    eprintln!(
        "kcf(fmt(x)) == kcf(x): {checked} conformance files checked, known gaps {gaps_seen:?}"
    );
    assert!(checked >= 30, "only {checked} conformance files checked");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
