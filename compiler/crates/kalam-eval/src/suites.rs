//! The shared suite format (`eval/suites/<suite>/<case>/`) and its runners.
//!
//! A case directory holds one or more source files named with the spec skin extensions
//! (`.kl`, `.ar.kl`, `.franco.kl`; DR-18) and an `expect.json`:
//!
//! ```text
//! {"must_reject": bool, "codes": ["K0201", ...], "lines": [optional 1-based lines], "notes": "why"}
//! ```
//!
//! `codes` is order-insensitive; `lines[i]`, when present, is the line where the **first**
//! error with `codes[i]` must point. Every source file of a case is judged on its own
//! against the same expectation (skins keep layout, so lines agree).
//!
//! Every runner checks what the compiler reported through [`ParseHook::parse`] (lexer and
//! parser diagnostics, `kalam_parser::parse_source`): it never synthesizes a diagnostic.
//! A missing suite, an empty suite or a malformed case makes the result unmeasurable, never
//! a pass.

use crate::parity::{fmt_check, ParseHook, Reported};
use kalam_base::Skin;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub const SUITES_DIR: &str = "eval/suites";
pub const TROJAN: &str = "trojan-source";
pub const LEXHAZ: &str = "lexing-hazards";
pub const SEEDED: &str = "seeded-errors";
pub const MIXED: &str = "mixed-skin";
pub const CONFORMANCE: &str = "conformance";

/// The `notes` prefix of a case that counts from tier M2 (FOUNDER-QUEUE Q10).
pub const M2_MARKER: &str = "[M2]";

/// Codes that spec §27 retired: a case expecting one is a suite defect.
pub const RETIRED: &[&str] = &["SEC0107"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SuiteExpect {
    pub must_reject: bool,
    pub codes: Vec<String>,
    /// Parallel to `codes`.
    pub lines: Vec<Option<usize>>,
    pub notes: String,
}

impl SuiteExpect {
    pub fn parse(text: &str) -> Result<SuiteExpect, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        let must_reject = v
            .get("must_reject")
            .and_then(Value::as_bool)
            .ok_or("missing boolean `must_reject`")?;
        let codes: Vec<String> = match v.get("codes") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a
                .iter()
                .map(|c| {
                    c.as_str()
                        .map(str::to_string)
                        .ok_or("`codes` must be strings")
                })
                .collect::<Result<_, _>>()?,
            Some(_) => return Err("`codes` must be an array".into()),
        };
        let mut lines: Vec<Option<usize>> = match v.get("lines") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a
                .iter()
                .map(|l| match l {
                    Value::Null => Ok(None),
                    Value::Number(n) => n
                        .as_u64()
                        .filter(|n| *n >= 1)
                        .map(|n| Some(n as usize))
                        .ok_or("`lines` entries must be 1-based integers"),
                    _ => Err("`lines` entries must be integers or null"),
                })
                .collect::<Result<_, _>>()?,
            Some(_) => return Err("`lines` must be an array".into()),
        };
        if lines.len() > codes.len() {
            return Err("`lines` is longer than `codes`".into());
        }
        lines.resize(codes.len(), None);
        if must_reject && codes.is_empty() {
            return Err("`must_reject: true` needs at least one code".into());
        }
        Ok(SuiteExpect {
            must_reject,
            codes,
            lines,
            notes: v
                .get("notes")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }

    /// The case needs name resolution: its `notes` open with `[M2]` (eval/suites/README.md,
    /// FOUNDER-QUEUE Q10). Every runner here is an M1 runner, so such cases are never
    /// judged; each runner counts them as `deferred_m2` (checked from tier M2 on).
    pub fn is_m2(&self) -> bool {
        self.notes.trim_start().starts_with(M2_MARKER)
    }

    /// `(code, line)` pairs.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, Option<usize>)> {
        self.codes
            .iter()
            .map(String::as_str)
            .zip(self.lines.iter().copied())
    }
}

#[derive(Clone, Debug)]
pub struct SuiteFile {
    pub skin: Skin,
    /// File name inside the case directory.
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SuiteCase {
    /// Path of the case directory relative to the suite directory, `/`-separated.
    pub id: String,
    pub files: Vec<SuiteFile>,
    pub expect: Result<SuiteExpect, String>,
}

impl SuiteCase {
    pub fn skins(&self) -> Vec<Skin> {
        let mut s: Vec<Skin> = self.files.iter().map(|f| f.skin).collect();
        s.sort();
        s.dedup();
        s
    }
}

/// Loads every case (a directory holding `expect.json`) under `dir`, sorted by id.
/// `Ok(None)` when the suite directory does not exist.
pub fn load(dir: &Path) -> Result<Option<Vec<SuiteCase>>, String> {
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut out = Vec::new();
    collect(dir, dir, &mut out).map_err(|e| format!("{}: {e}", dir.display()))?;
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Some(out))
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<SuiteCase>) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    let expect_path = dir.join("expect.json");
    if expect_path.is_file() {
        let mut files = Vec::new();
        for p in entries.iter().filter(|p| p.is_file()) {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if let Some(skin) = name
                .ends_with(".kl")
                .then(|| Skin::from_path(&name))
                .flatten()
            {
                files.push(SuiteFile {
                    skin,
                    name,
                    bytes: fs::read(p)?,
                });
            }
        }
        let expect = fs::read_to_string(&expect_path)
            .map_err(|e| e.to_string())
            .and_then(|t| SuiteExpect::parse(&t));
        let id = dir
            .strip_prefix(base)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        out.push(SuiteCase { id, files, expect });
    }
    for p in entries.iter().filter(|p| p.is_dir()) {
        collect(base, p, out)?;
    }
    Ok(())
}

/// What the compiler reported for one file: `Err` when it could not be checked at all.
pub fn diagnose(hook: &dyn ParseHook, file: &SuiteFile) -> Result<Vec<Reported>, String> {
    let text = std::str::from_utf8(&file.bytes).map_err(|_| {
        "not valid UTF-8; no compiler entry point accepts raw bytes (kalam check reads text), \
         so nothing reports SEC0100 for it"
            .to_string()
    })?;
    hook.parse(text, file.skin)
        .map(|p| p.errors)
        .map_err(|e| format!("crash: {e}"))
}

/// `U+XXXX` code points named in a message.
pub fn named_codepoints(message: &str) -> Vec<char> {
    let mut out = Vec::new();
    for (i, _) in message.match_indices("U+") {
        let hex: String = message[i + 2..]
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .take(6)
            .collect();
        if hex.len() >= 4 {
            if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                out.push(c);
            }
        }
    }
    out
}

/// The diagnostic names the code point it is about: its English message names a
/// `U+XXXX` that occurs under its span (or on its line when the span is empty). Printable
/// ASCII (U+0020-U+007E) never counts, so a contrast such as "not the ordinary space
/// U+0020" cannot match an ordinary line; ASCII controls do count (SEC0102, SEC0113).
/// Quoting the glyph without its `U+` code does not count as naming it.
pub fn names_its_codepoint(r: &Reported) -> bool {
    named_codepoints(&r.message_en)
        .iter()
        .filter(|c| !(' '..='~').contains(*c))
        .any(|c| r.context.contains(*c))
}

/// Codes that the M1 stages (S0 harness, S1 lexer, S2 parser/printer) cannot emit: name
/// resolution and later (SEC0104/SEC0108 need scopes; K03xx-K06xx, K08xx, K10xx-K17xx).
pub fn needs_later_stage(code: &str) -> bool {
    if matches!(code, "SEC0104" | "SEC0108") {
        return true;
    }
    let Some(n) = code.strip_prefix('K').and_then(|d| d.parse::<u32>().ok()) else {
        return false;
    };
    matches!(n / 100, 3..=6 | 8 | 10..=17)
}

/// How a rejection suite is judged.
#[derive(Clone, Debug, Default)]
pub struct RejectRules {
    /// Every expected `SEC` code must name its code point (Trojan Source).
    pub name_codepoints: bool,
    /// Cases expecting one of these codes are out of scope at this tier (counted, skipped).
    pub out_of_scope: Vec<&'static str>,
}

/// Judges one file against a case expectation. `Err(reason)` on failure.
pub fn judge_file(
    expect: &SuiteExpect,
    reported: &[Reported],
    rules: &RejectRules,
) -> Result<(), String> {
    if !expect.must_reject {
        return if reported.is_empty() {
            Ok(())
        } else {
            let got: Vec<String> = reported
                .iter()
                .map(|r| format!("{}@{}", r.code, r.line))
                .collect();
            Err(format!("expected no error, got {got:?}"))
        };
    }
    let mut problems = Vec::new();
    for (code, line) in expect.pairs() {
        match reported.iter().find(|r| r.code == code) {
            None => {
                let got: Vec<String> = reported
                    .iter()
                    .map(|r| format!("{}@{}", r.code, r.line))
                    .collect();
                problems.push(format!("missing {code} (got {got:?})"));
            }
            Some(r) => {
                if let Some(l) = line.filter(|l| *l != r.line) {
                    problems.push(format!("first {code} is on line {}, expected {l}", r.line));
                }
                if rules.name_codepoints && code.starts_with("SEC") && !names_its_codepoint(r) {
                    problems.push(format!("{code} does not name the code point it reports"));
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

/// The result of a rejection suite (Trojan Source, lexing hazards, mixed-skin).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RejectResult {
    pub bad_passed: usize,
    pub bad_total: usize,
    pub good_passed: usize,
    pub good_total: usize,
    /// Case ids skipped as out of scope at this tier, with the reason.
    pub out_of_scope: Vec<String>,
    /// Cases marked `[M2]` ([`SuiteExpect::is_m2`]): left out of the M1 pass/fail.
    pub deferred_m2: usize,
    /// Malformed or invalid cases: the suite cannot be judged while any exist.
    pub defects: Vec<String>,
    /// `case/file: reason`.
    pub failures: Vec<String>,
    /// Skins that host at least one passing or failing must-reject file.
    pub reject_skins: Vec<Skin>,
    /// Failing cases that expect a code no M1 stage can emit ([`needs_later_stage`]).
    pub later_stage: Vec<String>,
}

impl RejectResult {
    pub fn total(&self) -> usize {
        self.bad_total + self.good_total
    }

    pub fn all_pass(&self) -> bool {
        self.bad_passed == self.bad_total && self.good_passed == self.good_total
    }
}

pub fn run_reject(hook: &dyn ParseHook, cases: &[SuiteCase], rules: &RejectRules) -> RejectResult {
    let mut r = RejectResult::default();
    for case in cases {
        let expect = match &case.expect {
            Err(e) => {
                r.defects.push(format!("{}: expect.json: {e}", case.id));
                continue;
            }
            Ok(e) => e,
        };
        if case.files.is_empty() {
            r.defects
                .push(format!("{}: no .kl/.ar.kl/.franco.kl file", case.id));
            continue;
        }
        if let Some(c) = expect.codes.iter().find(|c| RETIRED.contains(&c.as_str())) {
            r.defects.push(format!(
                "{}: expects the retired code {c} (spec §27)",
                case.id
            ));
            continue;
        }
        if expect.is_m2() {
            r.deferred_m2 += 1;
            continue;
        }
        if let Some(c) = expect
            .codes
            .iter()
            .find(|c| rules.out_of_scope.contains(&c.as_str()))
        {
            r.out_of_scope.push(format!(
                "{}: expects {c}, out of scope at this tier",
                case.id
            ));
            continue;
        }
        let mut ok = true;
        for f in &case.files {
            let verdict = diagnose(hook, f).and_then(|rep| judge_file(expect, &rep, rules));
            if let Err(why) = verdict {
                ok = false;
                r.failures.push(format!("{}/{}: {why}", case.id, f.name));
            }
            if expect.must_reject && !r.reject_skins.contains(&f.skin) {
                r.reject_skins.push(f.skin);
            }
        }
        if !ok {
            let later: Vec<&String> = expect
                .codes
                .iter()
                .filter(|c| needs_later_stage(c))
                .collect();
            if !later.is_empty() {
                r.later_stage.push(format!("{}: {later:?}", case.id));
            }
        }
        if expect.must_reject {
            r.bad_total += 1;
            r.bad_passed += ok as usize;
        } else {
            r.good_total += 1;
            r.good_passed += ok as usize;
        }
    }
    r.reject_skins.sort();
    r
}

/// `D5.error_recovery_precision` (scorecard.json): per file, genuine / reported, where
/// genuine = reported errors matching a seeded `(code, line)`, and 0 when the first seeded
/// error is not reported. The metric is the mean over files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recovery {
    pub files: usize,
    pub score_sum: f64,
    /// Cases that have a file in each of the three skins.
    pub full_skin_cases: usize,
    /// Files whose first reported error matches a seeded `(code, line)`.
    pub first_match: usize,
    /// Files where every reported error matches a seeded `(code, line)` (no cascade).
    pub cascade_free: usize,
    /// Cases with `must_reject: false` (nothing is seeded), skipped.
    pub skipped_clean: usize,
    /// Cases marked `[M2]`, left out of the M1 value.
    pub deferred_m2: usize,
    pub defects: Vec<String>,
}

impl Recovery {
    pub fn value(&self) -> Option<f64> {
        (self.files > 0).then(|| self.score_sum / self.files as f64)
    }
}

/// The score of one file (see [`Recovery`]).
pub fn recovery_score(expect: &SuiteExpect, reported: &[Reported]) -> f64 {
    let seeded: Vec<(&str, usize)> = expect
        .pairs()
        .filter_map(|(c, l)| l.map(|l| (c, l)))
        .collect();
    let Some(&first) = seeded.first() else {
        return 0.0;
    };
    let matches = |r: &Reported| seeded.contains(&(r.code.as_str(), r.line));
    if reported.is_empty() || !reported.iter().any(|r| (r.code.as_str(), r.line) == first) {
        return 0.0;
    }
    reported.iter().filter(|r| matches(r)).count() as f64 / reported.len() as f64
}

pub fn run_recovery(hook: &dyn ParseHook, cases: &[SuiteCase]) -> Recovery {
    let mut r = Recovery::default();
    for case in cases {
        let expect = match &case.expect {
            Err(e) => {
                r.defects.push(format!("{}: expect.json: {e}", case.id));
                continue;
            }
            Ok(e) => e,
        };
        if expect.is_m2() {
            r.deferred_m2 += 1;
            continue;
        }
        if !expect.must_reject {
            r.skipped_clean += 1;
            continue;
        }
        if case.files.is_empty() {
            r.defects
                .push(format!("{}: no .kl/.ar.kl/.franco.kl file", case.id));
            continue;
        }
        if expect.lines.iter().any(Option::is_none) {
            r.defects.push(format!(
                "{}: every seeded code needs its line (D5 matches (code, line))",
                case.id
            ));
            continue;
        }
        if case.skins().len() == Skin::ALL.len() {
            r.full_skin_cases += 1;
        }
        for f in &case.files {
            r.files += 1;
            let Ok(reported) = diagnose(hook, f) else {
                continue; // unreadable or crashed: scores 0
            };
            r.score_sum += recovery_score(expect, &reported);
            let seeded = |x: &Reported| {
                expect
                    .pairs()
                    .any(|(c, l)| c == x.code && l == Some(x.line))
            };
            if reported.first().is_some_and(seeded) {
                r.first_match += 1;
            }
            if !reported.is_empty() && reported.iter().all(seeded) {
                r.cascade_free += 1;
            }
        }
    }
    r
}

/// `D7.formatter_idempotence` over a set of files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FmtTally {
    pub passed: usize,
    pub total: usize,
    /// Files fmt re-spelled but whose layout it kept (`kalam fmt` prints a note).
    pub layout_kept: usize,
    /// Files out of scope (expected to be rejected before parsing).
    pub excluded: usize,
    /// Cases marked `[M2]`, left out of the M1 pass/fail.
    pub deferred_m2: usize,
    pub failures: Vec<String>,
    pub defects: Vec<String>,
}

impl FmtTally {
    pub fn add(&mut self, other: &FmtTally) {
        self.passed += other.passed;
        self.total += other.total;
        self.layout_kept += other.layout_kept;
        self.excluded += other.excluded;
        self.deferred_m2 += other.deferred_m2;
        self.failures.extend(other.failures.iter().cloned());
        self.defects.extend(other.defects.iter().cloned());
    }

    /// One file that is meant to parse: it must parse, and fmt must be idempotent and keep
    /// its KCF.
    pub fn check(&mut self, hook: &dyn ParseHook, id: &str, text: &str, skin: Skin) {
        self.total += 1;
        let verdict = match hook.parse(text, skin) {
            Err(e) => Err(format!("crash: {e}")),
            Ok(p) if !p.accepted() => Err(format!("does not parse ({:?})", p.codes())),
            Ok(p) => fmt_check(hook, text, skin, &p.kcf),
        };
        match verdict {
            Ok(kept) => {
                self.passed += 1;
                self.layout_kept += kept as usize;
            }
            Err(why) => self.failures.push(format!("{id}: {why}")),
        }
    }
}

/// fmt over a suite (the conformance suite): cases with `must_reject: true` are excluded.
pub fn run_fmt(hook: &dyn ParseHook, cases: &[SuiteCase]) -> FmtTally {
    let mut t = FmtTally::default();
    for case in cases {
        match &case.expect {
            Err(e) => t.defects.push(format!("{}: expect.json: {e}", case.id)),
            Ok(e) if e.is_m2() => t.deferred_m2 += 1,
            Ok(e) if e.must_reject => t.excluded += case.files.len(),
            Ok(_) => {
                for f in &case.files {
                    let id = format!("{}/{}", case.id, f.name);
                    match std::str::from_utf8(&f.bytes) {
                        Ok(text) => t.check(hook, &id, text, f.skin),
                        Err(_) => {
                            t.total += 1;
                            t.failures.push(format!("{id}: not valid UTF-8"));
                        }
                    }
                }
            }
        }
    }
    t
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::parity::default_hook;

    /// A unique temp directory per test (test threads share a pid).
    pub fn temp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "kalam-eval-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Writes one case: `files` are `(name, text)`.
    pub fn write_case(suite: &Path, id: &str, expect: &str, files: &[(&str, &str)]) {
        let d = suite.join(id);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("expect.json"), expect).unwrap();
        for (n, t) in files {
            fs::write(d.join(n), t).unwrap();
        }
    }

    const HELLO_EN: &str = "fn main():\n    print(\"hi\")\n";
    const HELLO_AR: &str = "دالة رئيسي():\n    اطبع(\"hi\")\n";
    const HELLO_FR: &str = "dala ra2isi():\n    ektb(\"hi\")\n";

    #[test]
    fn parses_expect_json() {
        let e = SuiteExpect::parse(
            r#"{"must_reject": true, "codes": ["SEC0110", "K0201"], "lines": [2], "notes": "n"}"#,
        )
        .unwrap();
        assert!(e.must_reject);
        assert_eq!(e.lines, vec![Some(2), None]);
        assert_eq!(e.notes, "n");
        assert!(SuiteExpect::parse(r#"{"codes": []}"#).is_err());
        assert!(SuiteExpect::parse(r#"{"must_reject": true, "codes": []}"#).is_err());
        assert!(
            SuiteExpect::parse(r#"{"must_reject": true, "codes": ["K1"], "lines": [0]}"#).is_err()
        );
        assert!(SuiteExpect::parse(r#"{"must_reject": false}"#).is_ok());
    }

    #[test]
    fn missing_suite_is_none_and_cases_load_sorted_with_skins() {
        let d = temp_dir("suite-load");
        assert!(load(&d.join("nope")).unwrap().is_none());
        write_case(
            &d,
            "b",
            r#"{"must_reject": false}"#,
            &[
                ("x.kl", HELLO_EN),
                ("x.ar.kl", HELLO_AR),
                ("notes.txt", "n"),
            ],
        );
        write_case(
            &d,
            "a/nested",
            r#"{"must_reject": false}"#,
            &[("y.franco.kl", HELLO_FR)],
        );
        let cases = load(&d).unwrap().unwrap();
        let _ = fs::remove_dir_all(&d);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].id, "a/nested");
        assert_eq!(cases[0].files[0].skin, Skin::Franco);
        assert_eq!(cases[1].skins(), vec![Skin::En, Skin::Ar]);
    }

    #[test]
    fn codepoint_naming() {
        assert_eq!(
            named_codepoints("found U+202E and U+2066."),
            vec!['\u{202E}', '\u{2066}']
        );
        assert!(named_codepoints("U+12 is short").is_empty());
        let r = Reported {
            code: "SEC0110".into(),
            line: 2,
            message_en: "a bidi control U+202E".into(),
            context: "\u{202E}".into(),
        };
        assert!(names_its_codepoint(&r));
        let wrong = Reported {
            context: "x".into(),
            ..r.clone()
        };
        assert!(!names_its_codepoint(&wrong));
        // a contrast code point in the message does not hide the named one
        let contrast = Reported {
            message_en: "I found U+00A0, not the ordinary space U+0020".into(),
            context: "\u{A0}".into(),
            ..r.clone()
        };
        assert!(names_its_codepoint(&contrast));
        // only the contrast code point is on the line: not named
        let only_contrast = Reported {
            message_en: "not the ordinary space U+0020".into(),
            context: "let x = 1".into(),
            ..r.clone()
        };
        assert!(!names_its_codepoint(&only_contrast));
        // an ASCII control counts
        let control = Reported {
            message_en: "a control character U+0000".into(),
            context: "\u{0}".into(),
            ..r.clone()
        };
        assert!(names_its_codepoint(&control));
        assert!(needs_later_stage("SEC0104") && needs_later_stage("K0401"));
        assert!(needs_later_stage("K1601"));
        assert!(!needs_later_stage("K0201") && !needs_later_stage("K0731"));
        assert!(!needs_later_stage("SEC0110"));
        let unnamed = Reported {
            message_en: "a bidi control".into(),
            ..r
        };
        assert!(!names_its_codepoint(&unnamed));
    }

    #[test]
    fn trojan_suite_with_the_real_compiler() {
        let d = temp_dir("suite-trojan");
        // bad: a bidi override inside a string (spec §6: SEC0110 names the code point)
        let bad = "fn main():\n    let s = \"a\u{202E}b\"\n    print(s)\n";
        write_case(
            &d,
            "bad-rlo",
            r#"{"must_reject": true, "codes": ["SEC0110"], "lines": [2]}"#,
            &[("main.kl", bad)],
        );
        // good: legitimate Arabic text in a string
        write_case(
            &d,
            "good-arabic",
            r#"{"must_reject": false}"#,
            &[("main.kl", "fn main():\n    print(\"مرحبا\")\n")],
        );
        // a wrong line expectation fails
        write_case(
            &d,
            "bad-wrong-line",
            r#"{"must_reject": true, "codes": ["SEC0110"], "lines": [1]}"#,
            &[("main.kl", bad)],
        );
        let cases = load(&d).unwrap().unwrap();
        let _ = fs::remove_dir_all(&d);
        let hook = default_hook();
        let rules = RejectRules {
            name_codepoints: true,
            out_of_scope: vec![],
        };
        let r = run_reject(hook.as_ref(), &cases, &rules);
        assert_eq!((r.bad_passed, r.bad_total), (1, 2), "{r:?}");
        assert_eq!((r.good_passed, r.good_total), (1, 1), "{r:?}");
        assert!(r.failures[0].starts_with("bad-wrong-line/main.kl"), "{r:?}");
        assert!(!r.all_pass());
    }

    #[test]
    fn defects_and_out_of_scope_cases_are_reported_not_judged() {
        let d = temp_dir("suite-defects");
        write_case(&d, "broken", "{not json", &[("main.kl", HELLO_EN)]);
        write_case(
            &d,
            "retired",
            r#"{"must_reject": true, "codes": ["SEC0107"]}"#,
            &[("main.ar.kl", HELLO_AR)],
        );
        write_case(
            &d,
            "m2",
            r#"{"must_reject": true, "codes": ["SEC0104"]}"#,
            &[("main.ar.kl", HELLO_AR)],
        );
        write_case(&d, "empty", r#"{"must_reject": false}"#, &[]);
        let cases = load(&d).unwrap().unwrap();
        let _ = fs::remove_dir_all(&d);
        let rules = RejectRules {
            name_codepoints: false,
            out_of_scope: vec!["SEC0104"],
        };
        let r = run_reject(default_hook().as_ref(), &cases, &rules);
        assert_eq!(r.defects.len(), 3, "{r:?}");
        assert_eq!(r.out_of_scope.len(), 1);
        assert_eq!(r.total(), 0);
    }

    #[test]
    fn m2_marked_cases_are_deferred_in_every_runner() {
        let d = temp_dir("suite-m2");
        // a case only name resolution can judge (FOUNDER-QUEUE Q10): never judged at M1
        let m2 = r#"{"must_reject": true, "codes": ["K0401"], "lines": [2], "notes": " [M2] needs names"}"#;
        write_case(&d, "m2-reject", m2, &[("main.kl", HELLO_EN)]);
        write_case(
            &d,
            "m2-good",
            r#"{"must_reject": false, "notes": "[M2] later"}"#,
            &[(
                "main.kl",
                "fn main(:
",
            )],
        );
        // the marker must open the notes: a mention elsewhere does not defer the case
        write_case(
            &d,
            "mention",
            r#"{"must_reject": false, "notes": "not deferred, unlike [M2] cases"}"#,
            &[("main.kl", HELLO_EN)],
        );
        // a malformed case is still a defect, marker or not
        write_case(
            &d,
            "m2-broken",
            r#"{"notes": "[M2] x"}"#,
            &[("main.kl", HELLO_EN)],
        );
        let cases = load(&d).unwrap().unwrap();
        let _ = fs::remove_dir_all(&d);
        // sorted: m2-broken, m2-good, m2-reject, mention
        assert!(cases[1].expect.as_ref().unwrap().is_m2());
        assert!(!cases[3].expect.as_ref().unwrap().is_m2());
        let hook = default_hook();
        let r = run_reject(hook.as_ref(), &cases, &RejectRules::default());
        assert_eq!(r.deferred_m2, 2, "{r:?}");
        assert_eq!(r.defects.len(), 1, "{r:?}");
        assert_eq!(
            (r.good_passed, r.good_total, r.bad_total),
            (1, 1, 0),
            "{r:?}"
        );
        let rec = run_recovery(hook.as_ref(), &cases);
        assert_eq!(rec.deferred_m2, 2, "{rec:?}");
        assert_eq!(rec.files, 0, "{rec:?}");
        let t = run_fmt(hook.as_ref(), &cases);
        assert_eq!(t.deferred_m2, 2, "{t:?}");
        assert_eq!((t.passed, t.total), (1, 1), "{t:?}");
    }

    #[test]
    fn non_utf8_file_is_never_counted_as_rejected() {
        let d = temp_dir("suite-utf8");
        let dir = d.join("bad-bytes");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("expect.json"),
            r#"{"must_reject": true, "codes": ["SEC0100"], "lines": [1]}"#,
        )
        .unwrap();
        fs::write(dir.join("main.kl"), b"fn main():\n    print(\"\xff\")\n").unwrap();
        let cases = load(&d).unwrap().unwrap();
        let _ = fs::remove_dir_all(&d);
        let r = run_reject(default_hook().as_ref(), &cases, &RejectRules::default());
        assert_eq!((r.bad_passed, r.bad_total), (0, 1));
        assert!(r.failures[0].contains("UTF-8"), "{r:?}");
    }

    #[test]
    fn mixed_skin_cases_record_hosting_skins() {
        let d = temp_dir("suite-mixed");
        // an English keyword in an Arabic file: K0201 (spec §21.4 rule 6)
        write_case(
            &d,
            "en-in-ar",
            r#"{"must_reject": true, "codes": ["K0201"], "lines": [2]}"#,
            &[(
                "main.ar.kl",
                "دالة رئيسي():\n    if صح:\n        اطبع(\"x\")\n",
            )],
        );
        let cases = load(&d).unwrap().unwrap();
        let _ = fs::remove_dir_all(&d);
        let r = run_reject(default_hook().as_ref(), &cases, &RejectRules::default());
        assert_eq!(r.reject_skins, vec![Skin::Ar]);
        assert_eq!((r.bad_passed, r.bad_total), (1, 1), "{r:?}");
    }

    #[test]
    fn recovery_score_formula() {
        let e = SuiteExpect::parse(r#"{"must_reject": true, "codes": ["K0250"], "lines": [3]}"#)
            .unwrap();
        let hit = Reported::new("K0250", 3);
        let other = Reported::new("K0251", 4);
        assert_eq!(recovery_score(&e, std::slice::from_ref(&hit)), 1.0);
        assert_eq!(recovery_score(&e, &[hit.clone(), other.clone()]), 0.5);
        // the seeded error is missing: 0 whatever else is reported
        assert_eq!(recovery_score(&e, std::slice::from_ref(&other)), 0.0);
        assert_eq!(recovery_score(&e, &[]), 0.0);
        // wrong line is not genuine
        assert_eq!(recovery_score(&e, &[Reported::new("K0250", 2)]), 0.0);
    }

    #[test]
    fn recovery_runner_with_the_real_compiler() {
        let d = temp_dir("suite-recovery");
        // a seeded error the lexer reports: a leading zero (K0125) on line 2
        let en = "fn main():\n    let x = 007\n    print(\"{x}\")\n";
        write_case(
            &d,
            "leading-zero",
            r#"{"must_reject": true, "codes": ["K0125"], "lines": [2]}"#,
            &[("main.kl", en)],
        );
        write_case(
            &d,
            "unseeded-line",
            r#"{"must_reject": true, "codes": ["K0125"]}"#,
            &[("main.kl", en)],
        );
        write_case(
            &d,
            "clean",
            r#"{"must_reject": false}"#,
            &[("main.kl", HELLO_EN)],
        );
        let cases = load(&d).unwrap().unwrap();
        let _ = fs::remove_dir_all(&d);
        let r = run_recovery(default_hook().as_ref(), &cases);
        assert_eq!(r.files, 1, "{r:?}");
        assert_eq!(r.skipped_clean, 1);
        assert_eq!(r.defects.len(), 1);
        assert_eq!(r.full_skin_cases, 0);
        assert_eq!(r.first_match, 1, "{r:?}");
        assert!(r.value().unwrap() > 0.0, "{r:?}");
        assert_eq!(Recovery::default().value(), None);
    }

    #[test]
    fn fmt_runner_on_a_non_canonical_layout() {
        let d = temp_dir("suite-fmt");
        write_case(
            &d,
            "spacing",
            r#"{"must_reject": false}"#,
            &[
                ("main.kl", "fn main( ):\n    print( \"hi\" )\n"),
                ("main.ar.kl", HELLO_AR),
                ("main.franco.kl", HELLO_FR),
            ],
        );
        write_case(
            &d,
            "rejected",
            r#"{"must_reject": true, "codes": ["K0125"]}"#,
            &[("main.kl", "x = 007\n")],
        );
        write_case(
            &d,
            "broken",
            r#"{"must_reject": false}"#,
            &[("main.kl", "fn main(:\n")],
        );
        let cases = load(&d).unwrap().unwrap();
        let _ = fs::remove_dir_all(&d);
        let t = run_fmt(default_hook().as_ref(), &cases);
        assert_eq!(t.excluded, 1);
        assert_eq!(t.total, 4, "{t:?}");
        assert_eq!(t.passed, 3, "{t:?}");
        assert!(t.failures[0].starts_with("broken/main.kl"), "{t:?}");
    }
}
