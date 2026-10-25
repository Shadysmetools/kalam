//! The spec clause index, `eval/conformance/CLAUSES.json` (`D1.spec_clause_coverage`).
//!
//! `spec/KALAM-SPEC.md` sections 4–22 (the normative body) are split into **units** and
//! **clauses**:
//!
//! - a **unit** is every numbered `### N.M` subsection, plus `§N.0` for normative text that
//!   sits directly under a `## N.` heading before its first `###` (sections 6, 7, 11, 20 and
//!   22 today; §11 and §22 have no subsection at all). Units are what coverage counts
//!   (scorecard.json: "clauses = every numbered ### subsection");
//! - a **clause** is one top-level block inside a unit, numbered in source order: a
//!   paragraph, a list item (with its nested items and continuation lines) or a table data
//!   row. Its id is `§N.M.k` (for example `§9.9.3`, the third block of §9.9). Code fences,
//!   table header and separator rows, horizontal rules and non-normative paragraphs (those
//!   opening with `*Rationale` or `*Performance note`) are not clauses.
//!
//! Ids are positional, so each clause also records a SHA-256 prefix of its whitespace-
//! normalized text. The scorecard run re-derives the index from the spec and treats a
//! `CLAUSES.json` that differs as stale (coverage is then not measurable), so an edit to the
//! spec can never silently re-point a conformance case at another clause.

use kalam_syntax::sha256;
use serde_json::{json, Value};

pub const CLAUSES_PATH: &str = "eval/conformance/CLAUSES.json";
pub const SPEC_PATH: &str = "spec/KALAM-SPEC.md";
pub const SCHEMA: &str = "kalam-clauses/1";
pub const FIRST_SECTION: u32 = 4;
pub const LAST_SECTION: u32 = 22;

/// The rule, written into `CLAUSES.json` so the file explains itself.
pub const RULE: &str = "units = every `### N.M` subsection of spec sections 4-22, plus `§N.0` for normative text \
     directly under a `## N.` heading before its first `###`; clauses = the top-level blocks of a unit in source \
     order (paragraph, list item with its nested items, table data row), id `§N.M.k`; code fences, table headers, \
     rules and paragraphs opening with `*Rationale` or `*Performance note` are excluded; `sha256` = first 16 hex \
     digits of SHA-256 over the clause text with whitespace collapsed";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clause {
    /// `§N.M.k`.
    pub id: String,
    /// `paragraph`, `item` or `row`.
    pub kind: &'static str,
    /// 1-based line of the block's first line.
    pub line: usize,
    pub sha256: String,
    /// Whitespace-collapsed text.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// `§N.M` or `§N.0`.
    pub id: String,
    pub title: String,
    pub line: usize,
    pub clauses: Vec<Clause>,
}

/// The index of `spec` (the text of `spec/KALAM-SPEC.md`).
pub fn index(spec: &str) -> Vec<Unit> {
    let mut b = Builder::default();
    for (i, raw) in spec.lines().enumerate() {
        b.line(i + 1, raw);
        if b.done {
            break;
        }
    }
    b.flush();
    b.close_unit();
    b.units
}

#[derive(Default)]
struct Builder {
    units: Vec<Unit>,
    section: Option<u32>,
    section_title: String,
    section_line: usize,
    unit: Option<Unit>,
    /// `(kind, first line, lines)`.
    block: Option<(&'static str, usize, Vec<String>)>,
    fence: bool,
    /// The first row of a table, until the next row shows whether it is a header.
    table_head: Option<(usize, String)>,
    in_table: bool,
    done: bool,
}

fn heading(line: &str, hashes: &str) -> Option<(Vec<u32>, String)> {
    let rest = line.strip_prefix(hashes)?.strip_prefix(' ')?;
    let (num, title) = rest.split_once(' ').unwrap_or((rest, ""));
    let parts: Option<Vec<u32>> = num
        .trim_end_matches('.')
        .split('.')
        .map(|p| p.parse().ok())
        .collect();
    Some((parts?, title.trim().to_string()))
}

fn is_list_item(line: &str) -> bool {
    if line.starts_with("- ") || line.starts_with("* ") {
        return true;
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(". ")
}

fn is_separator_row(row: &str) -> bool {
    row.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

impl Builder {
    fn line(&mut self, n: usize, raw: &str) {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            // A fence indented under a list item belongs to that item; any other fence ends
            // the current block. Fenced lines are never clause text.
            let keeps_item = raw.starts_with(' ') && matches!(self.block, Some(("item", _, _)));
            if !self.fence && !keeps_item {
                self.flush();
            }
            self.fence = !self.fence;
            return;
        }
        if self.fence {
            return;
        }
        if let Some((nums, title)) = heading(raw, "##") {
            self.flush();
            self.close_unit();
            let n0 = nums[0];
            if n0 > LAST_SECTION {
                self.done = true;
                self.section = None;
                return;
            }
            self.section = (nums.len() == 1 && n0 >= FIRST_SECTION).then_some(n0);
            self.section_title = title;
            self.section_line = n;
            return;
        }
        if let Some((nums, title)) = heading(raw, "###") {
            self.flush();
            self.close_unit();
            if let (Some(s), [a, b]) = (self.section, nums.as_slice()) {
                if *a == s {
                    self.unit = Some(Unit {
                        id: format!("§{a}.{b}"),
                        title,
                        line: n,
                        clauses: Vec::new(),
                    });
                }
            }
            return;
        }
        if raw.starts_with('#') {
            // a deeper heading: never in sections 4-22 today; it ends the block
            self.flush();
            return;
        }
        let Some(section) = self.section else {
            return;
        };
        if trimmed.is_empty() || trimmed == "---" {
            self.flush();
            return;
        }
        if self.unit.is_none() {
            // normative text directly under the `## N.` heading
            self.unit = Some(Unit {
                id: format!("§{section}.0"),
                title: self.section_title.clone(),
                line: self.section_line,
                clauses: Vec::new(),
            });
        }
        if trimmed.starts_with('|') {
            if !self.in_table {
                self.flush();
                self.in_table = true;
                self.table_head = Some((n, trimmed.to_string()));
                return;
            }
            if is_separator_row(trimmed) {
                // the held first row was a header
                self.table_head = None;
                return;
            }
            if let Some((hl, head)) = self.table_head.take() {
                self.emit("row", hl, &[head]);
            }
            self.emit("row", n, &[trimmed.to_string()]);
            return;
        }
        if self.in_table {
            self.flush();
        }
        if is_list_item(raw) {
            self.flush();
            self.block = Some(("item", n, vec![trimmed.to_string()]));
            return;
        }
        match &mut self.block {
            Some((_, _, lines)) => lines.push(trimmed.to_string()),
            None => self.block = Some(("paragraph", n, vec![trimmed.to_string()])),
        }
    }

    fn flush(&mut self) {
        if let Some((hl, head)) = self.table_head.take() {
            self.emit("row", hl, &[head]);
        }
        self.in_table = false;
        if let Some((kind, line, lines)) = self.block.take() {
            let first = lines.first().map(String::as_str).unwrap_or("");
            let non_normative = kind == "paragraph"
                && (first.starts_with("*Rationale") || first.starts_with("*Performance note"));
            if !non_normative {
                self.emit(kind, line, &lines);
            }
        }
    }

    fn emit(&mut self, kind: &'static str, line: usize, lines: &[String]) {
        let Some(unit) = self.unit.as_mut() else {
            return;
        };
        let text = lines
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            return;
        }
        let k = unit.clauses.len() + 1;
        let digest = sha256::hex(&sha256::digest(text.as_bytes()));
        unit.clauses.push(Clause {
            id: format!("{}.{k}", unit.id),
            kind,
            line,
            sha256: digest[..16].to_string(),
            text,
        });
    }

    fn close_unit(&mut self) {
        if let Some(u) = self.unit.take() {
            if !u.clauses.is_empty() {
                self.units.push(u);
            }
        }
    }
}

/// The unit a clause or unit id belongs to: `§9.9.3` → `§9.9`, `§9.9` → `§9.9`.
pub fn unit_of(id: &str) -> String {
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() >= 2 {
        format!("{}.{}", parts[0], parts[1])
    } else {
        id.to_string()
    }
}

/// `9.9.3`, `§9.9.3` → `§9.9.3`.
pub fn normalize_id(id: &str) -> String {
    format!("§{}", id.trim().trim_start_matches('§'))
}

/// The `CLAUSES.json` document for `spec`. `excerpt_chars` bounds each clause's `text`.
pub fn to_json(units: &[Unit], spec: &str) -> Value {
    let excerpt = |t: &str| -> String {
        let mut s: String = t.chars().take(160).collect();
        if t.chars().count() > 160 {
            s.push('…');
        }
        s
    };
    json!({
        "schema": SCHEMA,
        "spec": SPEC_PATH,
        "spec_sha256": sha256::hex(&sha256::digest(spec.as_bytes())),
        "scope": format!("sections {FIRST_SECTION}-{LAST_SECTION}"),
        "rule": RULE,
        "units": units.len(),
        "clauses": units.iter().map(|u| u.clauses.len()).sum::<usize>(),
        "sections": units.iter().map(|u| json!({
            "id": u.id, "title": u.title, "line": u.line,
            "clauses": u.clauses.iter().map(|c| json!({
                "id": c.id, "kind": c.kind, "line": c.line, "sha256": c.sha256, "text": excerpt(&c.text),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// The committed index, checked against the spec: `Ok(units)` when `CLAUSES.json` lists
/// exactly the clause ids and hashes the spec gives today, `Err(why)` otherwise.
pub fn load_checked(committed: Option<&str>, spec: &str) -> Result<Vec<Unit>, String> {
    let units = index(spec);
    let text = committed.ok_or_else(|| {
        format!("{CLAUSES_PATH} does not exist (write it with `kalam-eval clauses --write`)")
    })?;
    let v: Value = serde_json::from_str(text).map_err(|e| format!("{CLAUSES_PATH}: {e}"))?;
    if v["schema"] != SCHEMA {
        return Err(format!("{CLAUSES_PATH}: schema is not {SCHEMA}"));
    }
    let listed: Vec<(String, String)> = v["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|u| u["clauses"].as_array().cloned().unwrap_or_default())
        .map(|c| {
            (
                c["id"].as_str().unwrap_or("").to_string(),
                c["sha256"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let derived: Vec<(String, String)> = units
        .iter()
        .flat_map(|u| u.clauses.iter().map(|c| (c.id.clone(), c.sha256.clone())))
        .collect();
    if listed != derived {
        let first = derived
            .iter()
            .zip(listed.iter())
            .find(|(a, b)| a != b)
            .map(|(a, b)| {
                format!(
                    "first difference: spec gives {} {}, file has {} {}",
                    a.0, a.1, b.0, b.1
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "spec gives {} clauses, file lists {}",
                    derived.len(),
                    listed.len()
                )
            });
        return Err(format!(
            "{CLAUSES_PATH} is stale against {SPEC_PATH} ({first}); regenerate it with `kalam-eval clauses --write` \
             and re-check every conformance case filed under a moved clause"
        ));
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "# Title\n\n## 3. Before\n\n### 3.1 Skipped\n\nnot indexed\n\n\
## 9. Type system\n\n### 9.1 Overview\n\n- **Static** and sound.\n  More text.\n- No casts.\n\n\
### 9.2 Primitive types\n\n| Type | Values |\n|---|---|\n| `Int` | 64-bit |\n| `Bool` | true |\n\n\
*Rationale:* not normative.\n\nA paragraph\nover two lines.\n\n```kalam\nfn x():\n```\n\n\
### 9.9 Inference rules\n\n1. **Mandatory annotations.**\n2. Local inference.\n   ```kalam\n   let x = 1\n   ```\n   after the fence.\n\n\
## 11. Pattern matching\n\n```kalam\nmatch x:\n```\n\n1. Exhaustive.\n2. Unreachable.\n\n---\n\n\
## 23. Excluded\n\n### 23.1 Not indexed\n\ntext\n";

    #[test]
    fn indexes_units_and_clauses_in_sections_4_to_22() {
        let u = index(SPEC);
        let ids: Vec<&str> = u.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(ids, ["§9.1", "§9.2", "§9.9", "§11.0"]);
        let c = |i: usize| -> Vec<(&str, &str)> {
            u[i].clauses
                .iter()
                .map(|c| (c.id.as_str(), c.kind))
                .collect()
        };
        assert_eq!(c(0), [("§9.1.1", "item"), ("§9.1.2", "item")]);
        assert_eq!(u[0].clauses[0].text, "- **Static** and sound. More text.");
        // header and separator rows dropped, rationale dropped, code fence dropped
        assert_eq!(
            c(1),
            [
                ("§9.2.1", "row"),
                ("§9.2.2", "row"),
                ("§9.2.3", "paragraph")
            ]
        );
        assert_eq!(u[1].clauses[2].text, "A paragraph over two lines.");
        // an indented fence stays inside its list item
        assert_eq!(c(2), [("§9.9.1", "item"), ("§9.9.2", "item")]);
        assert_eq!(u[2].clauses[1].text, "2. Local inference. after the fence.");
        assert_eq!(c(3), [("§11.0.1", "item"), ("§11.0.2", "item")]);
        assert_eq!(u[3].title, "Pattern matching");
    }

    #[test]
    fn hashes_are_whitespace_insensitive_and_text_sensitive() {
        let a = index("## 9. T\n\n### 9.1 O\n\nsome  words\n");
        let b = index("## 9. T\n\n### 9.1 O\n\nsome\nwords\n");
        let c = index("## 9. T\n\n### 9.1 O\n\nsome other words\n");
        assert_eq!(a[0].clauses[0].sha256, b[0].clauses[0].sha256);
        assert_ne!(a[0].clauses[0].sha256, c[0].clauses[0].sha256);
        assert_eq!(a[0].clauses[0].sha256.len(), 16);
    }

    #[test]
    fn the_committed_index_must_match_the_spec() {
        let units = index(SPEC);
        let doc = to_json(&units, SPEC).to_string();
        assert_eq!(load_checked(Some(&doc), SPEC).unwrap(), units);
        assert!(load_checked(None, SPEC)
            .unwrap_err()
            .contains("does not exist"));
        let edited = SPEC.replace("No casts.", "No unchecked casts.");
        let err = load_checked(Some(&doc), &edited).unwrap_err();
        assert!(err.contains("stale") && err.contains("§9.1.2"), "{err}");
    }

    #[test]
    fn ids_normalize_and_roll_up_to_units() {
        assert_eq!(normalize_id("9.9.3"), "§9.9.3");
        assert_eq!(normalize_id("§9.9.3"), "§9.9.3");
        assert_eq!(unit_of("§9.9.3"), "§9.9");
        assert_eq!(unit_of("§11.0.2"), "§11.0");
        assert_eq!(unit_of("§13.2"), "§13.2");
    }

    #[test]
    fn the_real_spec_has_a_plausible_index() {
        let spec = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../spec/KALAM-SPEC.md"),
        )
        .expect("spec");
        let u = index(&spec);
        let ids: Vec<&str> = u.iter().map(|u| u.id.as_str()).collect();
        for want in ["§4.1", "§9.9", "§11.0", "§13.1", "§13.4", "§22.0", "§21.5"] {
            assert!(ids.contains(&want), "{want} missing from {ids:?}");
        }
        assert!(!ids
            .iter()
            .any(|i| i.starts_with("§3.") || i.starts_with("§23.")));
        let s99 = u.iter().find(|u| u.id == "§9.9").unwrap();
        assert_eq!(s99.clauses.len(), 8, "§9.9 has 8 numbered rules");
        assert!(s99.clauses[2].text.contains("K0801"));
        let s131 = u.iter().find(|u| u.id == "§13.1").unwrap();
        assert!(s131.clauses.iter().any(|c| c.text.contains("K1203")));
    }
}
