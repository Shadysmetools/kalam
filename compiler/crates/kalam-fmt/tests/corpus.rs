//! The formatter and the skin converter over every eval/corpus case (all cases, all three
//! skins). The corpus is canonical text (INDEX.md §1): the English file is formatted, and the
//! Arabic and Franco files are exactly what the converter prints from it. So:
//! - `fmt` is a fixpoint on every file, idempotent, and keeps the AST and every comment;
//! - converting any skin file gives the sibling skin file byte for byte (all 6 pairs);
//! - every 2-hop and 3-hop round trip comes back byte-identical to `fmt(original)`;
//! - a case the lexer rejects (T50 main) is rejected by `fmt` and `convert` in every skin.

mod common;

use common::*;
use kalam_base::Skin;

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn fmt_is_a_fixpoint_idempotent_and_keeps_the_ast_on_the_corpus() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in corpus_cases() {
        for (skin, path) in &case.files {
            let text = read(path);
            let Some(before) = parse_clean(&text, *skin) else {
                assert!(
                    kalam_fmt::format(&text, *skin, &cfg()).is_err(),
                    "{}/{skin}: rejected source must not be formatted",
                    case.name
                );
                continue;
            };
            checked += 1;
            let f1 = match kalam_fmt::format_strict(&text, *skin, &cfg()) {
                Ok(f) => f,
                Err(e) => {
                    failures.push(format!("{}/{skin}: fmt failed: {e}", case.name));
                    continue;
                }
            };
            if f1 != text {
                failures.push(format!(
                    "{}/{skin}: corpus file is not a fmt fixpoint\n{}",
                    case.name,
                    first_diff(&text, &f1)
                ));
            }
            let f2 = kalam_fmt::format_strict(&f1, *skin, &cfg()).unwrap_or_default();
            if f2 != f1 {
                failures.push(format!("{}/{skin}: fmt is not idempotent", case.name));
            }
            let after = parse_clean(&f1, *skin).expect("formatted text parses");
            if tree(&after) != tree(&before) || comments(&after) != comments(&before) {
                failures.push(format!("{}/{skin}: fmt changed the AST", case.name));
            }
        }
    }
    assert!(checked >= 60, "only {checked} corpus files were accepted");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn convert_reproduces_the_sibling_skin_files_exactly() {
    let mut failures = Vec::new();
    for case in corpus_cases() {
        let texts: Vec<(Skin, String)> = case.files.iter().map(|(s, p)| (*s, read(p))).collect();
        if texts.iter().any(|(s, t)| parse_clean(t, *s).is_none()) {
            for (s, t) in &texts {
                for to in Skin::ALL {
                    assert!(
                        kalam_fmt::convert(t, *s, to, &cfg()).is_err(),
                        "{}: {s}->{to}: rejected source must not be converted",
                        case.name
                    );
                }
            }
            continue;
        }
        for (from, src) in &texts {
            for (to, want) in &texts {
                match kalam_fmt::convert(src, *from, *to, &cfg()) {
                    Ok(got) if &got == want => {}
                    Ok(got) => failures.push(format!(
                        "{}: {from}->{to} differs from {to}.kalam\n{}",
                        case.name,
                        first_diff(want, &got)
                    )),
                    Err(d) => failures.push(format!("{}: {from}->{to}: {d:?}", case.name)),
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn every_round_trip_is_byte_identical_to_fmt_of_the_original() {
    let mut failures = Vec::new();
    let mut trips = 0;
    for case in corpus_cases() {
        for (skin, path) in &case.files {
            let text = read(path);
            let Some(before) = parse_clean(&text, *skin) else {
                continue;
            };
            let Ok(canon) = kalam_fmt::format(&text, *skin, &cfg()) else {
                failures.push(format!("{}/{skin}: fmt failed", case.name));
                continue;
            };
            for start in [&text, &canon] {
                for path in round_trip_paths(*skin) {
                    trips += 1;
                    match convert_path(start, &path) {
                        Ok(steps) => {
                            if steps.last() != Some(&canon) {
                                failures.push(format!(
                                    "{}: {path:?} does not return fmt(original)",
                                    case.name
                                ));
                            }
                            for (i, step) in steps.iter().enumerate() {
                                let s = path[i + 1];
                                match parse_clean(step, s) {
                                    Some(a)
                                        if tree(&a) == tree(&before)
                                            && comments(&a) == comments(&before) => {}
                                    _ => failures.push(format!(
                                        "{}: {path:?} step {s} does not parse to the same AST",
                                        case.name
                                    )),
                                }
                            }
                        }
                        Err(e) => failures.push(format!("{}: {path:?}: {e}", case.name)),
                    }
                }
            }
        }
    }
    assert!(trips >= 400, "only {trips} round trips ran");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

fn first_diff(want: &str, got: &str) -> String {
    for (i, (a, b)) in want.lines().zip(got.lines()).enumerate() {
        if a != b {
            return format!("line {}:\n  want: {a}\n  got:  {b}", i + 1);
        }
    }
    format!(
        "line counts differ: want {} lines, got {}\n--- got\n{got}",
        want.lines().count(),
        got.lines().count()
    )
}
