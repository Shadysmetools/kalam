//! Diagnostic goldens (`D5.golden_coverage`) and machine-applicable fixes
//! (`D5.machine_fix_rate`).
//!
//! **Layout.** `eval/goldens/<CODE>/` holds the same erroneous program once per skin and
//! what `kalam check` prints for it, rendered in the matching message language:
//!
//! ```text
//! eval/goldens/K0901/en.kl      en.expected       (KALAM_LANG=en)
//!                    ar.kl      ar.expected       (KALAM_LANG=ar)
//!                    franco.kl  franco.expected   (KALAM_LANG=franco)
//! ```
//!
//! The source is checked as `main.kl` / `main.ar.kl` / `main.franco.kl` (the skin comes
//! from the file stem, as for `eval/corpus/*.kalam`; the extension is the skin declaration
//! of spec §4.1, so the staged name carries it). The file's **first error** must be
//! `<CODE>`, and the `.expected` file is that diagnostic's rendering (spec §21.4 template)
//! byte for byte, plus one final newline. Only the golden code's own diagnostic is
//! compared, so a later stage adding follow-on findings elsewhere does not break it. There is **no bless mode**: this runner only compares. New or
//! changed goldens are reviewed and committed by hand (LOOP.md A3).
//!
//! **Coverage** = catalog codes (`kalam explain --catalog --json`) whose three goldens all
//! match / catalog codes.
//!
//! **Fix rate.** Over every file of the goldens and of `eval/suites/seeded-errors`, each
//! emitted error diagnostic is one unit. It counts as fixed when it carries an
//! `applicability: machine` fix and applying that fix alone, then re-running `kalam check`,
//! gives strictly fewer errors and no error code the file did not have before (the fix
//! "compiles after applying" for the error it is about, scorecard.json). The fraction that
//! merely carries a machine fix is reported as evidence.

use crate::parity::{DiagError, ParseHook, RenderedDiag};
use crate::suites::SuiteCase;
use kalam_base::Skin;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const GOLDENS_DIR: &str = "eval/goldens";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFile {
    pub code: String,
    pub skin: Skin,
    pub source: Vec<u8>,
    /// `None` when the `.expected` file is missing.
    pub expected: Option<String>,
}

/// Loads `eval/goldens/<CODE>/<skin>.kl` (+ `.expected`). `Ok(None)` without the directory.
/// Every code directory yields three entries (missing sources become empty, failing ones).
pub fn load(dir: &Path) -> Result<Option<Vec<GoldenFile>>, String> {
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut codes: Vec<String> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    codes.sort();
    let mut out = Vec::new();
    for code in codes {
        for skin in Skin::ALL {
            let base = dir.join(&code);
            let src = fs::read(base.join(format!("{}.kl", skin.id()))).unwrap_or_default();
            let expected = fs::read_to_string(base.join(format!("{}.expected", skin.id()))).ok();
            out.push(GoldenFile {
                code: code.clone(),
                skin,
                source: src,
                expected,
            });
        }
    }
    Ok(Some(out))
}

#[derive(Clone, Debug, Default)]
pub struct GoldenRun {
    /// code → skins whose golden matches.
    pub matched: BTreeMap<String, Vec<Skin>>,
    /// `CODE/skin: why`.
    pub failures: Vec<String>,
    pub route: String,
}

impl GoldenRun {
    /// Codes whose three goldens match.
    pub fn complete(&self) -> BTreeSet<String> {
        self.matched
            .iter()
            .filter(|(_, s)| s.len() == Skin::ALL.len())
            .map(|(c, _)| c.clone())
            .collect()
    }
}

/// The expected text without its one final newline.
fn expected_text(e: &str) -> &str {
    e.strip_suffix('\n').unwrap_or(e)
}

/// First line where `a` and `b` differ, for evidence.
fn first_diff(a: &str, b: &str) -> String {
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}: got {x:?}, golden {y:?}", i + 1);
        }
    }
    format!(
        "length differs (got {} lines, golden {})",
        a.lines().count(),
        b.lines().count()
    )
}

pub fn run(hook: &dyn ParseHook, files: &[GoldenFile]) -> GoldenRun {
    let mut r = GoldenRun {
        route: hook.diag_route(),
        ..GoldenRun::default()
    };
    for g in files {
        r.matched.entry(g.code.clone()).or_default();
        let at = format!("{}/{}", g.code, g.skin.id());
        if g.source.is_empty() {
            r.failures
                .push(format!("{at}: no {}.kl source", g.skin.id()));
            continue;
        }
        let Some(expected) = &g.expected else {
            r.failures
                .push(format!("{at}: no {}.expected golden", g.skin.id()));
            continue;
        };
        let rendered = match hook.check_rendered(&g.source, g.skin, g.skin) {
            Ok(x) => x,
            Err(DiagError::Crash(c)) => {
                r.failures.push(format!("{at}: crash: {c}"));
                continue;
            }
            Err(DiagError::NotMeasurable(w)) => {
                r.failures.push(format!("{at}: {w}"));
                continue;
            }
        };
        let first = rendered.diags.iter().find(|d| d.error);
        let Some(first) = first.filter(|d| d.code == g.code) else {
            r.failures.push(format!(
                "{at}: the first error is {:?}, not {}",
                first.map(|d| d.code.as_str()),
                g.code
            ));
            continue;
        };
        if first.rendered != expected_text(expected) {
            r.failures.push(format!(
                "{at}: rendered text differs from the golden ({})",
                first_diff(&first.rendered, expected_text(expected))
            ));
            continue;
        }
        r.matched.entry(g.code.clone()).or_default().push(g.skin);
    }
    r
}

/// `D5.golden_coverage` over `catalog` (the `kalam explain --catalog --json` codes).
pub fn coverage(run: &GoldenRun, catalog: &[String]) -> (f64, String) {
    let complete = run.complete();
    let covered: Vec<&String> = catalog.iter().filter(|c| complete.contains(*c)).collect();
    let missing: Vec<&String> = catalog.iter().filter(|c| !complete.contains(*c)).collect();
    let extra: Vec<&String> = complete.iter().filter(|c| !catalog.contains(c)).collect();
    let pct = if catalog.is_empty() {
        0.0
    } else {
        100.0 * covered.len() as f64 / catalog.len() as f64
    };
    let ev = format!(
        "{GOLDENS_DIR} via {} (rendered as `kalam check` prints it, message language = file skin, no bless mode): \
         {}/{} catalog codes have matching goldens in all three skins; codes without: {:?}; golden directories for \
         codes outside the catalog (not counted): {:?}; failures {}{}",
        run.route,
        covered.len(),
        catalog.len(),
        missing,
        extra,
        run.failures.len(),
        run.failures
            .first()
            .map(|f| format!("; first: {f}"))
            .unwrap_or_default()
    );
    (pct, ev)
}

/// One file for the fix-rate runner.
#[derive(Clone, Debug)]
pub struct FixFile {
    pub label: String,
    pub skin: Skin,
    pub bytes: Vec<u8>,
}

/// Golden sources plus every file of the seeded-error suite. A file whose skin and bytes
/// equal an earlier one (a golden copied from a seeded-error case) is counted once.
pub fn fix_corpus(goldens: &[GoldenFile], seeded: &[SuiteCase]) -> Vec<FixFile> {
    let mut all = fix_corpus_all(goldens, seeded);
    let mut seen = BTreeSet::new();
    all.retain(|f| seen.insert((f.skin, f.bytes.clone())));
    all
}

fn fix_corpus_all(goldens: &[GoldenFile], seeded: &[SuiteCase]) -> Vec<FixFile> {
    let mut out: Vec<FixFile> = goldens
        .iter()
        .filter(|g| !g.source.is_empty())
        .map(|g| FixFile {
            label: format!("goldens/{}/{}", g.code, g.skin.id()),
            skin: g.skin,
            bytes: g.source.clone(),
        })
        .collect();
    for c in seeded {
        for f in &c.files {
            out.push(FixFile {
                label: format!("seeded-errors/{}/{}", c.id, f.name),
                skin: f.skin,
                bytes: f.bytes.clone(),
            });
        }
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct FixRun {
    pub files: usize,
    pub errors: usize,
    pub with_machine_fix: usize,
    pub fixed: usize,
    /// `label CODE@line: why` for errors whose machine fix did not verify.
    pub unverified: Vec<String>,
    /// Files no entry point could render (excluded, listed).
    pub unchecked: Vec<String>,
    pub per_code: BTreeMap<String, (usize, usize)>,
}

impl FixRun {
    pub fn pct(&self) -> Option<f64> {
        (self.errors > 0).then(|| 100.0 * self.fixed as f64 / self.errors as f64)
    }

    pub fn evidence(&self) -> String {
        format!(
            "goldens + eval/suites/seeded-errors, {} files: {} error diagnostics emitted, {} carry an \
             applicability=machine fix ({:.2}%), {} verified (the fix alone gives fewer errors and no new error \
             code on re-check); per code (errors, verified): {:?}; files that could not be checked {}{}",
            self.files,
            self.errors,
            self.with_machine_fix,
            if self.errors == 0 { 0.0 } else { 100.0 * self.with_machine_fix as f64 / self.errors as f64 },
            self.fixed,
            self.per_code,
            self.unchecked.len(),
            self.unverified
                .first()
                .map(|f| format!("; first unverified fix: {f}"))
                .unwrap_or_default()
        )
    }
}

/// Applies the edits of one fix (`(start, end, insert)`, byte offsets into `text`).
pub fn apply(text: &str, edits: &[(usize, usize, String)]) -> Result<String, String> {
    let mut edits: Vec<&(usize, usize, String)> = edits.iter().collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_string();
    let mut limit = usize::MAX;
    for (s, e, ins) in edits {
        if s > e || *e > out.len() || *e > limit {
            return Err(format!("edit {s}..{e} is out of range or overlaps"));
        }
        if !out.is_char_boundary(*s) || !out.is_char_boundary(*e) {
            return Err(format!("edit {s}..{e} splits a character"));
        }
        out.replace_range(*s..*e, ins);
        limit = *s;
    }
    Ok(out)
}

fn error_codes(d: &[RenderedDiag]) -> Vec<String> {
    d.iter()
        .filter(|d| d.error)
        .map(|d| d.code.clone())
        .collect()
}

pub fn fix_rate(hook: &dyn ParseHook, files: &[FixFile]) -> FixRun {
    let mut r = FixRun {
        files: files.len(),
        ..FixRun::default()
    };
    for f in files {
        let before = match hook.check_rendered(&f.bytes, f.skin, Skin::En) {
            Ok(x) => x,
            Err(DiagError::Crash(c)) => {
                r.unchecked.push(format!("{}: crash: {c}", f.label));
                continue;
            }
            Err(DiagError::NotMeasurable(w)) => {
                r.unchecked.push(format!("{}: {w}", f.label));
                continue;
            }
        };
        let before_codes = error_codes(&before.diags);
        for d in before.diags.iter().filter(|d| d.error) {
            r.errors += 1;
            let slot = r.per_code.entry(d.code.clone()).or_default();
            slot.0 += 1;
            let Some(fix) = d.machine_fixes.first() else {
                continue;
            };
            r.with_machine_fix += 1;
            let verdict = apply(&before.text, fix).and_then(|t| {
                let after = hook
                    .check_rendered(t.as_bytes(), f.skin, Skin::En)
                    .map_err(|e| format!("re-check failed: {e:?}"))?;
                let after_codes = error_codes(&after.diags);
                let new: Vec<&String> = after_codes
                    .iter()
                    .filter(|c| !before_codes.contains(c))
                    .collect();
                if after_codes.len() >= before_codes.len() {
                    Err(format!(
                        "errors {} -> {} after applying it",
                        before_codes.len(),
                        after_codes.len()
                    ))
                } else if !new.is_empty() {
                    Err(format!("applying it introduces {new:?}"))
                } else {
                    Ok(())
                }
            });
            match verdict {
                Ok(()) => {
                    r.fixed += 1;
                    r.per_code.entry(d.code.clone()).or_default().1 += 1;
                }
                Err(why) => r
                    .unverified
                    .push(format!("{} {}@{}: {why}", f.label, d.code, d.line)),
            }
        }
    }
    r
}

pub fn fix_json(r: &FixRun) -> Value {
    json!({
        "files": r.files, "errors": r.errors, "with_machine_fix": r.with_machine_fix, "verified": r.fixed,
        "per_code": r.per_code.iter().map(|(c, (n, v))| (c.clone(), json!({"errors": n, "verified": v}))).collect::<serde_json::Map<_, _>>(),
        "unverified": r.unverified, "unchecked": r.unchecked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::{FmtOut, Parsed, Rendered};
    use crate::suites::tests::temp_dir;

    /// `ERR(CODE)` markers are errors; `FIX(CODE)` markers are errors with a machine fix
    /// that deletes the marker. The rendering is `CODE@line` per error, one per line.
    struct Fake;

    fn diags(text: &str) -> Vec<RenderedDiag> {
        let mut out = Vec::new();
        let mut off = 0;
        for (i, l) in text.split_inclusive('\n').enumerate() {
            for (tag, fixable) in [("ERR(", false), ("FIX(", true)] {
                if let Some(p) = l.find(tag) {
                    let code = l[p + 4..].split(')').next().unwrap().to_string();
                    let end = p + 4 + code.len() + 1;
                    out.push(RenderedDiag {
                        rendered: String::new(),
                        code,
                        line: i + 1,
                        error: true,
                        machine_fixes: if fixable {
                            vec![vec![(off + p, off + end, String::new())]]
                        } else {
                            vec![]
                        },
                    });
                }
            }
            off += l.len();
        }
        out
    }

    impl ParseHook for Fake {
        fn name(&self) -> String {
            "fake".into()
        }
        fn probe(&self) -> Result<(), String> {
            Ok(())
        }
        fn probe_format(&self) -> Result<(), String> {
            Ok(())
        }
        fn parse(&self, _: &str, _: Skin) -> Result<Parsed, String> {
            Ok(Parsed::default())
        }
        fn convert(&self, t: &str, _: Skin, _: Skin) -> Result<String, String> {
            Ok(t.into())
        }
        fn format(&self, t: &str, _: Skin) -> Result<FmtOut, String> {
            Ok(FmtOut {
                text: t.into(),
                layout_kept: false,
            })
        }
        fn check_rendered(&self, b: &[u8], skin: Skin, lang: Skin) -> Result<Rendered, DiagError> {
            let text = String::from_utf8_lossy(b).into_owned();
            let mut d = diags(&text);
            for x in &mut d {
                x.rendered = format!("{}@{} {}/{}", x.code, x.line, skin.id(), lang.id());
            }
            let output = d
                .iter()
                .map(|x| x.rendered.clone())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(Rendered {
                text,
                output,
                diags: d,
            })
        }
    }

    fn golden(dir: &Path, code: &str, skin: &str, src: &str, expected: Option<&str>) {
        let d = dir.join(code);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(format!("{skin}.kl")), src).unwrap();
        if let Some(e) = expected {
            fs::write(d.join(format!("{skin}.expected")), e).unwrap();
        }
    }

    #[test]
    fn goldens_compare_byte_for_byte_and_need_all_three_skins() {
        let d = temp_dir("goldens");
        for s in ["en", "ar", "franco"] {
            golden(
                &d,
                "K0901",
                s,
                "x\nERR(K0901)\n",
                Some(&format!("K0901@2 {s}/{s}\n")),
            );
        }
        golden(&d, "K0902", "en", "ERR(K0902)\n", Some("K0902@1 en/en\n"));
        golden(
            &d,
            "K0902",
            "ar",
            "ERR(K0902)\n",
            Some("K0902@1 ar/ar changed\n"),
        );
        // the first error must be the code itself
        for s in ["en", "ar", "franco"] {
            golden(
                &d,
                "K0903",
                s,
                "ERR(K0904)\n",
                Some(&format!("K0904@1 {s}/{s}\n")),
            );
        }
        let files = load(&d).unwrap().unwrap();
        assert_eq!(files.len(), 9);
        let r = run(&Fake, &files);
        assert_eq!(r.complete(), BTreeSet::from(["K0901".to_string()]));
        assert_eq!(r.matched["K0902"], vec![Skin::En]);
        assert!(
            r.failures
                .iter()
                .any(|f| f.starts_with("K0902/ar: rendered text differs")),
            "{:?}",
            r.failures
        );
        assert!(r
            .failures
            .iter()
            .any(|f| f.starts_with("K0902/franco: no franco.kl")));
        assert!(r
            .failures
            .iter()
            .any(|f| f.contains("K0903/en: the first error is Some(\"K0904\")")));
        let cat = vec![
            "K0901".to_string(),
            "K0902".into(),
            "K0903".into(),
            "K0904".into(),
        ];
        let (pct, ev) = coverage(&r, &cat);
        assert_eq!(pct, 25.0);
        assert!(ev.contains("1/4"), "{ev}");
        assert!(load(&d.join("missing")).unwrap().is_none());
    }

    #[test]
    fn a_fix_counts_only_when_it_verifies() {
        let files = vec![
            FixFile {
                label: "a".into(),
                skin: Skin::En,
                bytes: b"FIX(K0901)\nERR(K0105)\n".to_vec(),
            },
            FixFile {
                label: "b".into(),
                skin: Skin::Ar,
                bytes: b"ok\n".to_vec(),
            },
        ];
        let r = fix_rate(&Fake, &files);
        assert_eq!((r.errors, r.with_machine_fix, r.fixed), (2, 1, 1));
        assert_eq!(r.pct(), Some(50.0));
        assert_eq!(r.per_code["K0901"], (1, 1));
        assert!(r.evidence().contains("2 error diagnostics"));
    }

    #[test]
    fn apply_edits_back_to_front_and_rejects_overlaps() {
        let t = "let x = a === b\n";
        assert_eq!(
            apply(t, &[(10, 13, "==".into())]).unwrap(),
            "let x = a == b\n"
        );
        assert_eq!(
            apply(t, &[(0, 3, "var".into()), (10, 13, "==".into())]).unwrap(),
            "var x = a == b\n"
        );
        assert!(apply(t, &[(0, 5, "".into()), (3, 6, "".into())]).is_err());
        assert!(apply(t, &[(0, 99, "".into())]).is_err());
    }
}
