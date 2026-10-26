//! `kalam fix`: apply the machine-applicable fixes `kalam check` reports (spec §21.3, §21.5).
//!
//! Only fixes with `applicability: machine` are applied; a `suggestion` needs a human or a
//! model to choose, so it is never applied. The file is checked exactly as `kalam check FILE`
//! checks it ([`crate::check_bytes`]), and the loop runs to a fixpoint.
//!
//! **The acceptance rule** (FOUNDER-QUEUE Q42, the `D5.machine_fix_rate` rule): a fix is
//! kept only if the file it gives has no error code the file did not have, at any stage
//! (lexer, parser, name resolution, types), and fewer errors. When the file has no error,
//! a fix must leave it with none and with fewer warnings. A fix that fails the rule is
//! *withheld*: the text stays as it was, and the fix is shown as a suggestion. Each stage
//! runs only when the ones before it are clean, so removing a stray `;` can reveal type
//! errors the file had all along. Their codes are new to `kalam check`, so that fix is
//! withheld too, and the person sees what it would expose before choosing it.
//!
//! 1. Check the text. Take the first machine fix of every diagnostic, in source order, and
//!    keep those whose edits overlap no fix taken before (touching counts as overlapping,
//!    so two zero-width edits at one offset never race); the rest wait for the next round.
//! 2. Apply them together and check again. If the result passes the rule, keep the round.
//! 3. Otherwise try the round's fixes one at a time; the first that passes on its own is
//!    kept, and each one tried before it is withheld for now.
//! 4. After a kept change, every fix is tried again (the text changed, so a fix withheld
//!    before may pass now). Stop when a round keeps nothing, when a text repeats, or after
//!    [`MAX_FIX_ROUNDS`]. Each kept change lowers the error count (or, with no error, the
//!    warning count), so the loop ends; when it ends because no fix passes, `kalam fix` on
//!    the result changes nothing.
//!
//! A file that is not valid UTF-8 is never changed: `check_bytes` reads each invalid run as
//! U+FFFD, and writing that text back would silently replace the bytes.

use std::collections::HashSet;

use kalam_diagnostics::{Applicability, Edit, Fix};

use crate::{check_bytes, CheckedFile, NotKalamFile};

/// The most check-and-apply rounds `kalam fix` runs on one file.
pub const MAX_FIX_ROUNDS: usize = 32;

/// One fix `kalam fix` applied (or withheld).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedFix {
    pub code: &'static str,
    /// The fix's title (English, as in `kalam check --json`).
    pub title: String,
    /// 1-based line of the diagnostic, in the text the fix was applied to.
    pub line: usize,
}

/// A machine fix that `kalam fix` withheld (module docs), and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithheldFix {
    pub fix: AppliedFix,
    /// The error codes that applying it alone would add, in order of first appearance.
    /// Empty when it adds none but does not lower the error count.
    pub new_codes: Vec<&'static str>,
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
    /// Machine fixes of `fixed` that the acceptance rule withholds. In `checked`, each is
    /// shown as a suggestion.
    pub withheld: Vec<WithheldFix>,
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

/// What a round compares: the file's diagnostics and how many there are.
struct Snapshot {
    checked: CheckedFile,
    errors: usize,
    all: usize,
}

fn snapshot(path: &str, text: &str) -> Result<Snapshot, NotKalamFile> {
    let checked = check_bytes(path, text.as_bytes())?;
    Ok(Snapshot {
        errors: error_codes(&checked).count(),
        all: checked.diagnostics.len(),
        checked,
    })
}

fn error_codes(checked: &CheckedFile) -> impl Iterator<Item = &'static str> + '_ {
    checked
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code)
}

/// The error codes `after` reports that `before` does not, in order of first appearance.
fn new_codes(before: &Snapshot, after: &Snapshot) -> Vec<&'static str> {
    let old: HashSet<&str> = error_codes(&before.checked).collect();
    let mut out: Vec<&'static str> = Vec::new();
    for c in error_codes(&after.checked) {
        if !old.contains(c) && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The acceptance rule (module docs): `after` adds no error code and has fewer errors, or,
/// with no error on either side, fewer warnings.
fn better(before: &Snapshot, after: &Snapshot) -> bool {
    if !new_codes(before, after).is_empty() {
        return false;
    }
    if before.errors > 0 {
        after.errors < before.errors
    } else {
        after.errors == 0 && after.all < before.all
    }
}

/// What identifies a fix in a check: its code, title and inserted texts.
type FixKey = (&'static str, String, Vec<String>);

fn fix_key(code: &'static str, fix: &Fix) -> FixKey {
    (
        code,
        fix.title.clone(),
        fix.edits.iter().map(|e| e.insert.clone()).collect(),
    )
}

/// A machine fix found in one round.
struct Candidate {
    fix: Fix,
    info: AppliedFix,
    key: FixKey,
}

fn candidates(snap: &Snapshot) -> Vec<Candidate> {
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
        let start = (d.span.start as usize).min(text.len());
        out.push(Candidate {
            info: AppliedFix {
                code: d.code,
                title: fix.title.clone(),
                line: text[..start].matches('\n').count() + 1,
            },
            key: fix_key(d.code, fix),
            fix: fix.clone(),
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
        withheld: Vec::new(),
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
    for _ in 0..MAX_FIX_ROUNDS {
        let found = candidates(&snap);
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
        // Step 3: one at a time; the first fix that passes on its own is kept.
        let mut withheld = Vec::new();
        let mut kept = None;
        for c in &found {
            let after = snapshot(path, &apply(&text, &[&c.fix]))?;
            if better(&snap, &after) {
                kept = Some((c.info.clone(), after));
                break;
            }
            withheld.push(WithheldFix {
                fix: c.info.clone(),
                new_codes: new_codes(&snap, &after),
            });
        }
        if let Some((info, after)) = kept {
            // Step 4: the text changed, so every fix is tried again.
            result.applied.push(info);
            snap = after;
            continue;
        }
        // No fix passes on this text: each is shown as a suggestion.
        let keys: HashSet<&FixKey> = found.iter().map(|c| &c.key).collect();
        for d in &mut snap.checked.diagnostics {
            let code = d.code;
            for f in &mut d.fixes {
                if f.applicability == Applicability::Machine && keys.contains(&fix_key(code, f)) {
                    f.applicability = Applicability::Suggestion;
                }
            }
        }
        result.withheld = withheld;
        break;
    }
    result.fixed = snap.checked.text.clone();
    result.checked = snap.checked;
    Ok(result)
}

fn snapshot_bytes(path: &str, bytes: &[u8]) -> Result<Snapshot, NotKalamFile> {
    match std::str::from_utf8(bytes) {
        Ok(text) => snapshot(path, text),
        Err(_) => Ok(Snapshot {
            checked: check_bytes(path, bytes)?,
            errors: 0,
            all: 0,
        }),
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
    fn a_syntax_fix_that_reveals_a_new_type_error_is_withheld() {
        // K0904 (parse) hides K1302 (types): removing the `;` would add K1302, a code the
        // file does not have, so the fix is withheld and shown as a suggestion (Q42).
        let src = "fn head(xs: List[Int]) -> Result[Int, String]:
    let n = xs.get(0)?;
    return Ok(n)

fn main():
    let _ = head([2])
";
        let f = fix("main.kl", src);
        assert!(!f.changed() && f.applied.is_empty(), "{}", f.fixed);
        assert_eq!(f.withheld.len(), 1);
        assert_eq!(f.withheld[0].fix.code, "K0904");
        assert_eq!(f.withheld[0].new_codes, ["K1302"]);
        let d = &f.checked.diagnostics[0];
        assert_eq!(d.code, "K0904");
        assert!(d
            .fixes
            .iter()
            .all(|x| x.applicability == Applicability::Suggestion));
        // A second run withholds it again and changes nothing.
        let again = fix("main.kl", &f.fixed);
        assert!(!again.changed() && again.withheld.len() == 1);
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
        // Fewer errors and no new code is better; a new code is not.
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
        assert!(!better(&one_type_error, &other_code));
        assert_eq!(new_codes(&one_type_error, &other_code), ["K0601"]);
        // The stage does not matter: a syntax error that gives way to an error of another
        // code (at a later stage) is a new code, and the other way round too.
        let syntax = snap(
            "fn main():
    let a: Int = 1;
    print(\"{a}\")
",
        );
        assert!(!better(&one_type_error, &syntax));
        assert!(!better(&syntax, &one_type_error));
        // As many errors, same codes: not better.
        assert!(!better(&one_type_error, &one_type_error));
    }

    #[test]
    fn a_syntax_fix_that_reveals_other_errors_is_withheld() {
        // Two type errors hide behind one `;` (the parser stops the pipeline): removing it
        // would show K0600 twice, a code the file does not have (Q42).
        let src = "fn main():
    let a: Int = \"x\"
    let b: Int = \"y\";
    print(\"{a} {b}\")
";
        let f = fix("main.kl", src);
        assert!(!f.changed() && f.applied.is_empty());
        assert_eq!(f.withheld.len(), 1);
        assert_eq!(f.withheld[0].new_codes, ["K0600"]);
        let left: Vec<&str> = f.checked.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(left, ["K0904"]);
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
