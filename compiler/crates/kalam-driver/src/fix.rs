//! `kalam fix`: apply the machine-applicable fixes `kalam check` reports (spec §21.3, §21.5).
//!
//! Only fixes with `applicability: machine` are applied; a `suggestion` needs a human or a
//! model to choose, so it is never applied. The file is checked exactly as `kalam check FILE`
//! checks it ([`crate::check_bytes`]), and the loop runs to a fixpoint:
//!
//! 1. Check the text. Take the first machine fix of every diagnostic, in source order, and
//!    keep those whose edits overlap no fix taken before (touching counts as overlapping,
//!    so two zero-width edits at one offset never race); the rest wait for the next round.
//! 2. Apply them together and check again. The round is kept only if the file got better:
//!    it reaches a later pipeline [`Stage`], or it stays at the same stage with no error code
//!    it did not have, and fewer errors (or as many errors and fewer findings in all). This is
//!    the `D5.machine_fix_rate` rule, widened to stages. Each stage runs only when the ones
//!    before it are clean, so the errors a later stage reports were hidden until now: a
//!    stray `;` next to two type errors is still removed, although the file then shows two
//!    errors instead of one. A fix that sends the file back to an earlier stage is undone.
//! 3. If the round made the file worse, its fixes are tried one at a time instead, each kept
//!    only if it alone makes the file better. A fix that makes it worse is remembered (by
//!    code, title and inserted text) and not tried again.
//! 4. Stop when no fix is left to try, when a text repeats, or after [`MAX_FIX_ROUNDS`].
//!
//! A file that is not valid UTF-8 is never changed: `check_bytes` reads each invalid run as
//! U+FFFD, and writing that text back would silently replace the bytes.

use std::collections::HashSet;

use kalam_diagnostics::{Applicability, Diagnostic, Edit, Fix};

use crate::{check_bytes_staged, CheckedFile, NotKalamFile, Stage};

/// The most check-and-apply rounds `kalam fix` runs on one file.
pub const MAX_FIX_ROUNDS: usize = 32;

/// One fix `kalam fix` applied (or tried and undid).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedFix {
    pub code: &'static str,
    /// The fix's title (English, as in `kalam check --json`).
    pub title: String,
    /// 1-based line of the diagnostic, in the text the fix was applied to.
    pub line: usize,
}

/// The outcome of `kalam fix` on one file.
pub struct FixedFile {
    pub path: String,
    /// The file as read (invalid UTF-8 runs as U+FFFD).
    pub original: String,
    /// The text after every kept fix; equal to `original` when nothing applied.
    pub fixed: String,
    /// Kept fixes, in the order they were applied.
    pub applied: Vec<AppliedFix>,
    /// Machine fixes that were tried and undone because the file got worse.
    pub rejected: Vec<AppliedFix>,
    /// `kalam check` of `fixed`: what is left for a human.
    pub checked: CheckedFile,
    /// The file is not valid UTF-8, so it was left unchanged.
    pub not_utf8: bool,
}

impl FixedFile {
    pub fn changed(&self) -> bool {
        self.fixed != self.original
    }
}

/// What a round compares: how far the pipeline got and what it found.
struct Snapshot {
    checked: CheckedFile,
    stage: Stage,
    errors: usize,
    all: usize,
    error_codes: HashSet<&'static str>,
}

fn snapshot(path: &str, text: &str) -> Result<Snapshot, NotKalamFile> {
    let (checked, stage) = check_bytes_staged(path, text.as_bytes())?;
    let errors: Vec<&Diagnostic> = checked
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .collect();
    Ok(Snapshot {
        stage,
        errors: errors.len(),
        all: checked.diagnostics.len(),
        error_codes: errors.iter().map(|d| d.code).collect(),
        checked,
    })
}

/// True when `after` is closer to a clean file than `before` (module docs, step 2).
fn better(before: &Snapshot, after: &Snapshot) -> bool {
    if after.stage != before.stage {
        return after.stage > before.stage;
    }
    if after
        .error_codes
        .iter()
        .any(|c| !before.error_codes.contains(c))
    {
        return false;
    }
    (after.errors, after.all) < (before.errors, before.all)
}

/// A machine fix found in one round.
struct Candidate {
    fix: Fix,
    info: AppliedFix,
    key: (String, String, Vec<String>),
}

fn candidates(
    snap: &Snapshot,
    rejected: &HashSet<(String, String, Vec<String>)>,
) -> Vec<Candidate> {
    let text = &snap.checked.text;
    let mut out = Vec::new();
    for d in &snap.checked.diagnostics {
        let Some(fix) = d
            .fixes
            .iter()
            .find(|f| f.applicability == Applicability::Machine)
        else {
            continue;
        };
        if fix.edits.is_empty() || !edits_fit(text, &fix.edits) {
            continue;
        }
        let key = (
            d.code.to_string(),
            fix.title.clone(),
            fix.edits.iter().map(|e| e.insert.clone()).collect(),
        );
        if rejected.contains(&key) {
            continue;
        }
        let start = (d.span.start as usize).min(text.len());
        out.push(Candidate {
            info: AppliedFix {
                code: d.code,
                title: fix.title.clone(),
                line: text[..start].matches('\n').count() + 1,
            },
            fix: fix.clone(),
            key,
        });
    }
    out
}

/// Every edit lies inside `text`, on character boundaries, and no two of them touch.
fn edits_fit(text: &str, edits: &[Edit]) -> bool {
    let in_text = edits.iter().all(|e| {
        let (s, t) = (e.span.start as usize, e.span.end as usize);
        s <= t && t <= text.len() && text.is_char_boundary(s) && text.is_char_boundary(t)
    });
    in_text
        && edits
            .iter()
            .enumerate()
            .all(|(i, a)| edits[i + 1..].iter().all(|b| !touch(a, b)))
}

/// Two edits overlap or touch (a zero-width edit at either end of another counts).
fn touch(a: &Edit, b: &Edit) -> bool {
    a.span.start <= b.span.end && b.span.start <= a.span.end
}

/// `text` with every edit applied (the edits must not touch).
fn apply(text: &str, fixes: &[&Fix]) -> String {
    let mut edits: Vec<&Edit> = fixes.iter().flat_map(|f| f.edits.iter()).collect();
    edits.sort_by_key(|e| std::cmp::Reverse((e.span.start, e.span.end)));
    let mut out = text.to_string();
    for e in edits {
        out.replace_range(e.span.start as usize..e.span.end as usize, &e.insert);
    }
    out
}

/// `kalam fix` on one file: `bytes` as read from `path` (module docs).
pub fn fix_file(path: &str, bytes: &[u8]) -> Result<FixedFile, NotKalamFile> {
    let mut snap = snapshot_bytes(path, bytes)?;
    let original = snap.checked.text.clone();
    let mut result = FixedFile {
        path: path.to_string(),
        fixed: original.clone(),
        original,
        applied: Vec::new(),
        rejected: Vec::new(),
        checked: CheckedFile {
            path: String::new(),
            skin: snap.checked.skin,
            text: String::new(),
            lines: kalam_base::LineIndex::new(""),
            diagnostics: Vec::new(),
        },
        not_utf8: std::str::from_utf8(bytes).is_err(),
    };
    if result.not_utf8 {
        result.checked = snap.checked;
        return Ok(result);
    }
    let mut rejected = HashSet::new();
    let mut seen = HashSet::new();
    for _ in 0..MAX_FIX_ROUNDS {
        if !seen.insert(snap.checked.text.clone()) {
            break;
        }
        let found = candidates(&snap, &rejected);
        if found.is_empty() {
            break;
        }
        // Step 1: a batch of fixes that touch no fix taken before them.
        let mut batch: Vec<&Candidate> = Vec::new();
        for c in &found {
            let free = batch.iter().all(|b| {
                b.fix
                    .edits
                    .iter()
                    .all(|x| c.fix.edits.iter().all(|y| !touch(x, y)))
            });
            if free {
                batch.push(c);
            }
        }
        let text = snap.checked.text.clone();
        let fixes: Vec<&Fix> = batch.iter().map(|c| &c.fix).collect();
        let after = snapshot(path, &apply(&text, &fixes))?;
        if better(&snap, &after) {
            result.applied.extend(batch.iter().map(|c| c.info.clone()));
            snap = after;
            continue;
        }
        // Step 3: one at a time; the first fix that helps on its own is kept.
        let mut progressed = false;
        for c in &found {
            let after = snapshot(path, &apply(&text, &[&c.fix]))?;
            if better(&snap, &after) {
                result.applied.push(c.info.clone());
                snap = after;
                progressed = true;
                break;
            }
            rejected.insert(c.key.clone());
            result.rejected.push(c.info.clone());
        }
        if !progressed {
            break;
        }
    }
    result.fixed = snap.checked.text.clone();
    result.checked = snap.checked;
    Ok(result)
}

fn snapshot_bytes(path: &str, bytes: &[u8]) -> Result<Snapshot, NotKalamFile> {
    match std::str::from_utf8(bytes) {
        Ok(text) => snapshot(path, text),
        Err(_) => {
            let (checked, stage) = check_bytes_staged(path, bytes)?;
            Ok(Snapshot {
                stage,
                errors: 0,
                all: 0,
                error_codes: HashSet::new(),
                checked,
            })
        }
    }
}

/// A unified diff (`diff -u` style, 3 lines of context) from `old` to `new`, labelled with
/// `path`; empty when the texts are equal. `kalam fix --dry-run` prints it.
pub fn unified_diff(path: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let a: Vec<&str> = old.split_inclusive('\n').collect();
    let b: Vec<&str> = new.split_inclusive('\n').collect();
    let ops = line_ops(&a, &b);
    const CONTEXT: usize = 3;
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| op.kind != Kind::Same)
        .map(|(i, _)| i)
        .collect();
    let mut out = format!("--- {path}\n+++ {path} (fixed)\n");
    let mut i = 0;
    while i < changed.len() {
        let first = changed[i];
        let mut last = first;
        while i + 1 < changed.len() && changed[i + 1] <= last + 2 * CONTEXT + 1 {
            i += 1;
            last = changed[i];
        }
        i += 1;
        let hunk = &ops[first.saturating_sub(CONTEXT)..(last + CONTEXT + 1).min(ops.len())];
        let a_len = hunk.iter().filter(|op| op.kind != Kind::Ins).count();
        let b_len = hunk.iter().filter(|op| op.kind != Kind::Del).count();
        // `diff -u` numbers an empty range by the line before it.
        let start = |at: usize, len: usize| if len == 0 { at } else { at + 1 };
        out.push_str(&format!(
            "@@ -{},{a_len} +{},{b_len} @@\n",
            start(hunk[0].a, a_len),
            start(hunk[0].b, b_len)
        ));
        for op in hunk {
            let (mark, line) = match op.kind {
                Kind::Same => (' ', a[op.a]),
                Kind::Del => ('-', a[op.a]),
                Kind::Ins => ('+', b[op.b]),
            };
            out.push(mark);
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Same,
    Del,
    Ins,
}

/// One line of an edit script. `a` and `b` are the line's index in each text, or for a
/// line the text does not have, the number of that text's lines before it.
#[derive(Clone, Copy, Debug)]
struct Op {
    kind: Kind,
    a: usize,
    b: usize,
}

/// The edit script from `a` to `b`: common prefix and suffix, and a longest common
/// subsequence of the lines between them (or all of them replaced, for very large middles).
fn line_ops(a: &[&str], b: &[&str]) -> Vec<Op> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (am, bm) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let mut kinds: Vec<Kind> = vec![Kind::Same; prefix];
    let (n, m) = (am.len(), bm.len());
    if n.saturating_mul(m) <= 4_000_000 {
        // lcs[i][j] = length of a longest common subsequence of am[i..] and bm[j..].
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if am[i] == bm[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && am[i] == bm[j] {
                kinds.push(Kind::Same);
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                kinds.push(Kind::Del);
                i += 1;
            } else {
                kinds.push(Kind::Ins);
                j += 1;
            }
        }
    } else {
        kinds.extend(std::iter::repeat(Kind::Del).take(n));
        kinds.extend(std::iter::repeat(Kind::Ins).take(m));
    }
    kinds.extend(std::iter::repeat(Kind::Same).take(suffix));
    let (mut x, mut y) = (0, 0);
    kinds
        .into_iter()
        .map(|kind| {
            let op = Op { kind, a: x, b: y };
            match kind {
                Kind::Same => {
                    x += 1;
                    y += 1;
                }
                Kind::Del => x += 1,
                Kind::Ins => y += 1,
            }
            op
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden(code: &str) -> String {
        let p = format!(
            "{}/../../../eval/goldens/{code}/en.kl",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(p).unwrap()
    }

    fn fix(path: &str, text: &str) -> FixedFile {
        fix_file(path, text.as_bytes()).unwrap()
    }

    fn codes(f: &[AppliedFix]) -> Vec<&'static str> {
        f.iter().map(|a| a.code).collect()
    }

    #[test]
    fn applies_a_machine_fix() {
        let f = fix(
            "main.kl",
            "fn main():\n    let same = 1 === 1\n    print(\"{same}\")\n",
        );
        assert_eq!(
            f.fixed,
            "fn main():\n    let same = 1 == 1\n    print(\"{same}\")\n"
        );
        assert_eq!(codes(&f.applied), ["K0901"]);
        assert!(f.changed() && !f.checked.has_errors());
    }

    #[test]
    fn applies_several_fixes_and_reaches_a_fixpoint() {
        let src = "fn main():\n    let a = 1;\n    let b = a === 1;\n    print(\"{b}\")\n";
        let f = fix("main.kl", src);
        assert_eq!(
            f.fixed,
            "fn main():\n    let a = 1\n    let b = a == 1\n    print(\"{b}\")\n"
        );
        assert!(!f.checked.has_errors(), "{:?}", f.checked.diagnostics);
        assert_eq!(f.applied.len(), 3, "{:?}", f.applied);
        // A second run has nothing left to do.
        let again = fix("main.kl", &f.fixed);
        assert!(!again.changed() && again.applied.is_empty());
    }

    #[test]
    fn a_fixed_syntax_error_can_reveal_a_type_error_that_is_fixed_next() {
        // K0904 (parse) hides K1302 (types) until the `;` is gone.
        let src = "fn head(xs: List[Int]) -> Result[Int, String]:\n    let n = xs.get(0)?;\n    return Ok(n)\n\nfn main():\n    let _ = head([2])\n";
        let f = fix("main.kl", src);
        assert_eq!(codes(&f.applied), ["K0904", "K1302"]);
        assert!(
            f.fixed
                .contains("let n = xs.get(0).ok_or(\"missing value\")?\n"),
            "{}",
            f.fixed
        );
        assert!(!f.checked.has_errors(), "{:?}", f.checked.diagnostics);
        assert_eq!(fix("main.kl", &f.fixed).fixed, f.fixed);
    }

    #[test]
    fn suggestions_are_never_applied() {
        let src = golden("K1301");
        let f = fix("main.kl", &src);
        assert!(!f.changed() && f.applied.is_empty());
        assert_eq!(f.checked.diagnostics[0].code, "K1301");
        assert!(f.checked.diagnostics[0]
            .fixes
            .iter()
            .all(|x| x.applicability == Applicability::Suggestion));
    }

    fn snap(text: &str) -> Snapshot {
        snapshot("main.kl", text).unwrap()
    }

    #[test]
    fn only_a_better_file_is_kept() {
        let one_type_error = snap(
            "fn main():
    let a: Int = \"x\"
    print(\"{a}\")
",
        );
        assert_eq!(one_type_error.stage, Stage::Types);
        // Same stage: fewer errors and no new code is better; a new code is not.
        let clean = snap(
            "fn main():
    let a: Int = 1
    print(\"{a}\")
",
        );
        assert!(better(&one_type_error, &clean));
        let other_code = snap(
            "fn main():
    let a = 1
    let b = 2.5
    print(\"{a + b}\")
",
        );
        assert_eq!(other_code.stage, Stage::Types);
        assert!(!better(&one_type_error, &other_code));
        // Back to an earlier stage is worse, whatever the counts.
        let syntax = snap(
            "fn main():
    let a: Int = 1;
    print(\"{a}\")
",
        );
        assert_eq!(syntax.stage, Stage::Syntax);
        assert!(!better(&one_type_error, &syntax));
        assert!(better(&syntax, &one_type_error));
    }

    #[test]
    fn a_syntax_fix_is_kept_even_when_it_reveals_more_type_errors() {
        // Two type errors hide behind one `;` (the parser stops the pipeline): removing it is
        // progress, and the type errors are then reported.
        let src = "fn main():
    let a: Int = \"x\"
    let b: Int = \"y\";
    print(\"{a} {b}\")
";
        let f = fix("main.kl", src);
        assert_eq!(codes(&f.applied), ["K0904"]);
        assert!(f.rejected.is_empty());
        let left: Vec<&str> = f.checked.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(left, ["K0600", "K0600"]);
    }

    #[test]
    fn other_skins_and_invalid_utf8() {
        let f = fix(
            "main.ar.kl",
            "دالة رئيسي():\n    ليكن x = 1 === 1\n    اطبع(\"{x}\")\n",
        );
        assert!(f.fixed.contains("1 == 1") && !f.checked.has_errors());
        let mut bytes = b"fn main():\n    let x = 1 === 1\n    print(\"{x}\")\n".to_vec();
        bytes.push(0xFF);
        let f = fix_file("main.kl", &bytes).unwrap();
        assert!(f.not_utf8 && !f.changed() && f.applied.is_empty());
        assert!(fix_file("notes.txt", b"").is_err());
    }

    #[test]
    fn unified_diff_has_hunks_with_context() {
        assert_eq!(unified_diff("a.kl", "x\n", "x\n"), "");
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
        let new = "1\n2\n3\n4\nfive\n6\n7\n8\n9\nten\n";
        assert_eq!(
            unified_diff("a.kl", old, new),
            "--- a.kl\n+++ a.kl (fixed)\n@@ -2,8 +2,9 @@\n 2\n 3\n 4\n-5\n+five\n 6\n 7\n 8\n 9\n+ten\n"
        );
        assert_eq!(
            unified_diff("a.kl", "a\nb", "a\nc"),
            "--- a.kl\n+++ a.kl (fixed)\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+c\n\\ No newline at end of file\n"
        );
    }
}
