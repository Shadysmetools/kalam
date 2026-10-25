//! Measurements that exist today: keyword-table counts, grammar productions, corpus lexing,
//! lex-level skin round-trips, source-text security expectations and `cargo test` totals.

use crate::corpus::{Case, Expect};
use kalam_base::{FileId, LineIndex, Skin};
use kalam_fmt::{convert, FmtConfig};
use kalam_lexer::lex;
use kalam_skins::arabic_fold;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------- keyword table (S0)

/// `kalam-eval keywords`: counts per skin over `spec/keywords.json` rows.
#[derive(Clone, Debug, PartialEq)]
pub struct KeywordCounts {
    /// kinds `keyword` + `ascii_fixed`, max over skins (D4.keywords_per_skin).
    pub keywords_max: usize,
    /// kinds `keyword` + `ascii_fixed` + `core_name`, max over skins (D4.reserved_spellings_per_skin).
    pub reserved_max: usize,
    pub per_skin: BTreeMap<String, (usize, usize)>,
}

pub fn keyword_counts(rows: &[Value]) -> KeywordCounts {
    let mut per_skin = BTreeMap::new();
    for skin in Skin::ALL {
        let (mut kw, mut reserved) = (0, 0);
        for row in rows {
            let kind = row.get("kind").and_then(Value::as_str).unwrap_or("");
            let spelled = row
                .get(skin.id())
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty());
            if !spelled {
                continue;
            }
            if matches!(kind, "keyword" | "ascii_fixed") {
                kw += 1;
                reserved += 1;
            } else if kind == "core_name" {
                reserved += 1;
            }
        }
        per_skin.insert(skin.id().to_string(), (kw, reserved));
    }
    KeywordCounts {
        keywords_max: per_skin.values().map(|v| v.0).max().unwrap_or(0),
        reserved_max: per_skin.values().map(|v| v.1).max().unwrap_or(0),
        per_skin,
    }
}

/// `kalam-eval keywords --synonyms` (D4.synonym_count): returns the concepts that have more
/// than one `en` spelling or more than one `ar` spelling after `arabic_fold`. Franco
/// variant lists are exempt (spec DR-20). Punctuation rows are not keywords.
pub fn synonym_offenders(rows: &[Value]) -> Vec<String> {
    let mut en: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut ar: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        if row.get("kind").and_then(Value::as_str) == Some("punctuation") {
            continue;
        }
        let concept = row
            .get("concept")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let strs = |k: &str| -> Vec<String> {
            match row.get(k) {
                Some(Value::String(s)) => vec![s.clone()],
                Some(Value::Array(a)) => a
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                _ => Vec::new(),
            }
        };
        let e = en.entry(concept.clone()).or_default();
        e.extend(strs("en"));
        e.extend(strs("en_aliases"));
        let a = ar.entry(concept).or_default();
        a.extend(
            strs("ar")
                .iter()
                .chain(strs("ar_also_accepts").iter())
                .map(|s| arabic_fold(s)),
        );
    }
    let mut out: Vec<String> = en
        .iter()
        .filter(|(_, s)| s.len() > 1)
        .map(|(c, s)| format!("{c}: en {s:?}"))
        .chain(
            ar.iter()
                .filter(|(_, s)| s.len() > 1)
                .map(|(c, s)| format!("{c}: ar {s:?}")),
        )
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------- grammar (S0)

/// Counts `name = ... ;` productions in the first ```` ```ebnf ```` block after the
/// `### 8.2` heading of the spec (or in a whole `.ebnf` file when `whole_file`).
pub fn grammar_productions(text: &str, whole_file: bool) -> Option<usize> {
    let body: Vec<&str> = if whole_file {
        text.lines().collect()
    } else {
        let mut lines = text.lines().skip_while(|l| !l.starts_with("### 8.2"));
        lines.next()?;
        let mut lines = lines.skip_while(|l| !l.trim_start().starts_with("```ebnf"));
        lines.next()?;
        lines.take_while(|l| l.trim() != "```").collect()
    };
    Some(body.iter().filter(|l| is_production_head(l)).count())
}

fn is_production_head(line: &str) -> bool {
    let t = line.trim_start();
    let name_len = t
        .char_indices()
        .take_while(|(i, c)| c.is_ascii_lowercase() || *c == '_' || (*i > 0 && c.is_ascii_digit()))
        .count();
    if name_len == 0 {
        return false;
    }
    let rest = t[name_len..].trim_start();
    rest.starts_with('=') && !rest.starts_with("==")
}

// ---------------------------------------------------------------- lexing (S1)

/// Diagnostic codes `kalam-lexer` is responsible for (ARCHITECTURE.md §5). An expected
/// diagnostic outside this set belongs to a later stage, so lexing cannot decide it.
pub const LEX_STAGE_CODES: &[&str] = &[
    "SEC0100", "SEC0102", "SEC0103", "SEC0106", "SEC0110", "SEC0111", "SEC0112", "SEC0113",
    "SEC0120", "SEC0121", "SEC0122", "SEC0123", "SEC0124", "K0103", "K0105", "K0120", "K0122",
    "K0125", "K0127", "K0128", "K0129", "K0130", "K0131", "K0132", "K0904", "K0911", "K0924",
];

pub fn is_lex_stage(code: &str) -> bool {
    LEX_STAGE_CODES.contains(&code)
}

#[derive(Clone, Debug, PartialEq)]
pub struct LexResult {
    pub skin: Skin,
    pub staged: String,
    pub clean: bool,
    /// `(code, 1-based line)` of every error diagnostic.
    pub errors: Vec<(String, usize)>,
    /// Lex-stage expectations from `expected.txt`.
    pub lex_expectations: usize,
    pub pass: bool,
    pub detail: String,
}

/// Lexes one case file and judges it against the lex-stage part of `expected.txt`:
/// - with lex-stage expected diagnostics: each must be reported on one of its lines with
///   every listed code point named in the message, and (with `others: none`) no other
///   lex error may appear;
/// - otherwise the file must lex cleanly (later-stage expectations are out of scope).
pub fn lex_case(text: &str, skin: Skin, staged: &str, expect: &Expect) -> LexResult {
    let out = lex(text, skin, FileId(0));
    let index = LineIndex::new(text);
    let errs: Vec<(&kalam_diagnostics::Diagnostic, usize)> = out
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| (d, index.line_of(d.span.start) + 1))
        .collect();
    let errors: Vec<(String, usize)> = errs.iter().map(|(d, l)| (d.code.to_string(), *l)).collect();
    let wanted: Vec<_> = expect
        .diagnostics
        .iter()
        .filter(|d| is_lex_stage(&d.code))
        .collect();

    let mut problems = Vec::new();
    if wanted.is_empty() {
        if !errors.is_empty() {
            problems.push(format!("unexpected lex errors {errors:?}"));
        }
    } else {
        let mut matched = vec![false; errs.len()];
        for w in &wanted {
            let hits: Vec<usize> = (0..errs.len())
                .filter(|&i| errs[i].0.code == w.code && w.lines.contains(&errs[i].1))
                .collect();
            if hits.is_empty() {
                problems.push(format!("missing {} on line {:?}", w.code, w.lines));
                continue;
            }
            let messages: String = hits
                .iter()
                .map(|&i| errs[i].0.message(Skin::En))
                .collect::<Vec<_>>()
                .join("\n");
            for cp in &w.codepoints {
                if !messages.contains(cp.as_str()) {
                    problems.push(format!(
                        "{} on line {:?} does not name {cp}",
                        w.code, w.lines
                    ));
                }
            }
            for i in hits {
                matched[i] = true;
            }
        }
        if expect.others_none {
            let extra: Vec<_> = (0..errs.len())
                .filter(|&i| !matched[i])
                .map(|i| errors[i].clone())
                .collect();
            if !extra.is_empty() {
                problems.push(format!("unexpected extra lex errors {extra:?}"));
            }
        }
    }
    LexResult {
        skin,
        staged: staged.to_string(),
        clean: errors.is_empty(),
        errors,
        lex_expectations: wanted.len(),
        pass: problems.is_empty(),
        detail: problems.join("; "),
    }
}

// ---------------------------------------------------------------- round-trip (token converter)

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoundTrip {
    pub checks: usize,
    pub failures: Vec<String>,
}

impl RoundTrip {
    fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        self.checks += 1;
        if !ok {
            self.failures.push(what());
        }
    }
}

/// Lex-level parity probes through `kalam_fmt::convert` (the token converter). This is a
/// proxy for P2/P3, which are defined on KCF and need the parser:
/// - X→Y→X gives back the source byte for byte, for all 6 ordered pairs;
/// - both 3-hop cycles from `en` give back the source;
/// - fixpoint: converting Y's output to Y again changes nothing;
/// - `en`→Y reproduces the committed Y file byte for byte (INDEX.md rule 2).
///
/// Only files that lex cleanly take part; the rest are reported by [`lex_case`].
pub fn round_trip(sources: &BTreeMap<Skin, String>) -> RoundTrip {
    let cfg = FmtConfig::default();
    let mut rt = RoundTrip::default();
    let conv = |s: &str, a: Skin, b: Skin| convert(s, a, b, &cfg).ok();
    for (&x, src) in sources {
        if lex(src, x, FileId(0)).has_errors() {
            continue;
        }
        for y in Skin::ALL {
            if y == x {
                continue;
            }
            let Some(out) = conv(src, x, y) else {
                rt.check(false, || format!("{x}->{y}: convert failed"));
                continue;
            };
            let back = conv(&out, y, x);
            rt.check(back.as_deref() == Some(src.as_str()), || {
                format!("{x}->{y}->{x} differs from source")
            });
            rt.check(conv(&out, y, y).as_deref() == Some(out.as_str()), || {
                format!("{x}->{y}: not a fixpoint")
            });
            if x == Skin::En {
                if let Some(committed) = sources.get(&y) {
                    rt.check(&out == committed, || {
                        format!("en->{y} differs from the committed {y} file")
                    });
                }
            }
        }
    }
    if let Some(src) = sources
        .get(&Skin::En)
        .filter(|s| !lex(s, Skin::En, FileId(0)).has_errors())
    {
        for path in [
            [Skin::En, Skin::Ar, Skin::Franco, Skin::En],
            [Skin::En, Skin::Franco, Skin::Ar, Skin::En],
        ] {
            let mut cur = Some(src.clone());
            for w in path.windows(2) {
                cur = cur.and_then(|s| conv(&s, w[0], w[1]));
            }
            rt.check(cur.as_deref() == Some(src.as_str()), || {
                format!(
                    "3-hop {}->{}->{}->{} differs from source",
                    path[0], path[1], path[2], path[3]
                )
            });
        }
    }
    rt
}

/// Per-case JSON record for the corpus checks.
pub fn case_record(case: &Case, lexed: &[LexResult], rt: &RoundTrip) -> Value {
    json!({
        "task": case.task,
        "case": case.case,
        "kind": case.expect.kind,
        "lex": lexed.iter().map(|l| json!({
            "skin": l.skin.id(), "staged_as": l.staged, "clean": l.clean,
            "errors": l.errors.iter().map(|(c, n)| format!("{c}@{n}")).collect::<Vec<_>>(),
            "lex_stage_expectations": l.lex_expectations, "pass": l.pass,
            "detail": if l.detail.is_empty() { Value::Null } else { Value::String(l.detail.clone()) },
        })).collect::<Vec<_>>(),
        "roundtrip": {"checks": rt.checks, "failures": rt.failures},
    })
}

// ---------------------------------------------------------------- cargo test

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CargoTotals {
    pub passed: u64,
    pub failed: u64,
    pub ignored: u64,
    pub suites: u64,
}

/// Sums the `test result: ...` lines of `cargo test` output.
pub fn parse_cargo_test(output: &str) -> CargoTotals {
    let mut t = CargoTotals::default();
    for line in output.lines() {
        let Some(rest) = line.trim().strip_prefix("test result: ") else {
            continue;
        };
        t.suites += 1;
        for part in rest.split(';') {
            let words: Vec<&str> = part.split_whitespace().collect();
            let n = |w: &str| -> Option<u64> {
                words
                    .iter()
                    .position(|x| *x == w)
                    .and_then(|i| words.get(i.checked_sub(1)?)?.parse().ok())
            };
            if let Some(v) = n("passed") {
                t.passed += v;
            }
            if let Some(v) = n("failed") {
                t.failed += v;
            }
            if let Some(v) = n("ignored") {
                t.ignored += v;
            }
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{parse_expected, ExpectedDiag};

    fn rows(json: &str) -> Vec<Value> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn keyword_counts_by_kind() {
        let r = rows(
            r#"[{"concept":"A","kind":"keyword","en":"let","ar":"ليكن","franco":"khali"},
                {"concept":"B","kind":"ascii_fixed","en":"unsafe","ar":"unsafe","franco":"unsafe"},
                {"concept":"C","kind":"core_name","en":"Int","ar":"صحيح","franco":"rakam"},
                {"concept":"D","kind":"punctuation","en":",","ar":"،","franco":","}]"#,
        );
        let c = keyword_counts(&r);
        assert_eq!(c.keywords_max, 2);
        assert_eq!(c.reserved_max, 3);
        assert_eq!(c.per_skin["ar"], (2, 3));
    }

    #[test]
    fn synonyms_detected_after_fold_and_punctuation_exempt() {
        let ok = rows(
            r#"[{"concept":"A","kind":"keyword","en":"if","ar":"إذا","franco":"law","franco_aliases":["lw"]},
                {"concept":"P","kind":"punctuation","en":",","ar":"،","franco":",","ar_also_accepts":[","]}]"#,
        );
        assert!(synonym_offenders(&ok).is_empty());
        // a hamza variant folds to the same key: not a synonym
        let folded = rows(
            r#"[{"concept":"A","kind":"keyword","en":"if","ar":"إذا","franco":"law","ar_also_accepts":["اذا"]}]"#,
        );
        assert!(synonym_offenders(&folded).is_empty());
        let bad = rows(
            r#"[{"concept":"A","kind":"keyword","en":"if","ar":"إذا","franco":"law","ar_also_accepts":["لو"]}]"#,
        );
        assert_eq!(synonym_offenders(&bad).len(), 1);
        let dup = rows(
            r#"[{"concept":"A","kind":"keyword","en":"if","ar":"إذا","franco":"law"},
                {"concept":"A","kind":"keyword","en":"when","ar":"إذا","franco":"law"}]"#,
        );
        assert_eq!(
            synonym_offenders(&dup),
            vec![r#"A: en {"if", "when"}"#.to_string()]
        );
    }

    #[test]
    fn counts_grammar_productions_in_section_8_2_only() {
        let spec = "## 8\n```ebnf\nnotme = x ;\n```\n### 8.2 Productions\n\n```ebnf\n(* c *)\nfile = a ;\nuse_decl = [ \"x\" ]\n    b ;\n  item2 = c | d ;\nx == y\n```\nlater = z ;\n";
        assert_eq!(grammar_productions(spec, false), Some(3));
        assert_eq!(grammar_productions("a = b ;\nc = d ;\n", true), Some(2));
        assert_eq!(grammar_productions("no section", false), None);
    }

    #[test]
    fn clean_file_passes_when_no_lex_expectations() {
        let e = parse_expected("kind: run\n");
        let r = lex_case("fn main():\n    print(\"hi\")\n", Skin::En, "main.kl", &e);
        assert!(r.clean, "{:?}", r.errors);
        assert!(r.pass);
    }

    #[test]
    fn bidi_control_must_be_reported_and_named() {
        let src = "fn main():\n    let s = \"a\u{202E}b\"\n";
        let expect = Expect {
            kind: Some("compile-error".into()),
            diagnostics: vec![ExpectedDiag {
                code: "SEC0110".into(),
                lines: vec![2],
                codepoints: vec!["U+202E".into()],
            }],
            others_none: true,
        };
        let r = lex_case(src, Skin::En, "main.kl", &expect);
        assert!(r.pass, "{}", r.detail);
        assert!(!r.clean);

        let wrong_line = Expect {
            diagnostics: vec![ExpectedDiag {
                code: "SEC0110".into(),
                lines: vec![1],
                codepoints: vec![],
            }],
            ..expect.clone()
        };
        assert!(!lex_case(src, Skin::En, "main.kl", &wrong_line).pass);

        // a bidi control where none is expected fails a run case
        assert!(!lex_case(src, Skin::En, "main.kl", &parse_expected("kind: run\n")).pass);
    }

    #[test]
    fn later_stage_expectations_require_a_clean_lex() {
        let e = parse_expected("kind: compile-error\ndiagnostic: code=K1601 line=2\n");
        let r = lex_case("fn main():\n    print(\"hi\")\n", Skin::En, "main.kl", &e);
        assert_eq!(r.lex_expectations, 0);
        assert!(r.pass);
    }

    #[test]
    fn round_trip_of_converter_output_is_clean() {
        let en = "fn main():\n    print(\"hi\")\n".to_string();
        let cfg = FmtConfig::default();
        let ar = convert(&en, Skin::En, Skin::Ar, &cfg).unwrap();
        let fr = convert(&en, Skin::En, Skin::Franco, &cfg).unwrap();
        let sources = BTreeMap::from([(Skin::En, en), (Skin::Ar, ar), (Skin::Franco, fr)]);
        let rt = round_trip(&sources);
        assert!(rt.checks >= 14, "{}", rt.checks);
        assert!(rt.failures.is_empty(), "{:?}", rt.failures);
    }

    #[test]
    fn round_trip_flags_a_committed_file_that_drifted() {
        let en = "fn main():\n    print(\"hi\")\n".to_string();
        let sources = BTreeMap::from([
            (Skin::En, en),
            (
                Skin::Franco,
                "dala ra2isi():\n    ektb(\"bye\")\n".to_string(),
            ),
        ]);
        let rt = round_trip(&sources);
        assert!(
            rt.failures.iter().any(|f| f.contains("committed franco")),
            "{:?}",
            rt.failures
        );
    }

    #[test]
    fn parses_cargo_test_totals() {
        let out = "running 3 tests\ntest result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\
                   test result: FAILED. 5 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out\n";
        let t = parse_cargo_test(out);
        assert_eq!(
            t,
            CargoTotals {
                passed: 8,
                failed: 2,
                ignored: 1,
                suites: 2
            }
        );
    }
}
