//! G-PAR P6 (diagnostic catalog parity) and `D5.diag_completeness` over the M1 code set.
//!
//! **Denominator (scorecard.json M1 scope):** every spec §27 code in the K01xx, K02xx,
//! K09xx and SEC01xx families (ranges such as `K0901–K0926` expanded, rows marked
//! *retired* dropped), plus every other code `kalam explain --catalog --json` lists. That
//! listing is read in process from `kalam_diagnostics::explain::catalog_value()`, the
//! function the CLI prints.
//!
//! **Per code**, the definitions require (SCORECARD.md P6 and `D5.diag_completeness`):
//! an en/ar/franco **message**, en/ar/franco **help text**, and an **explain page** in all
//! three languages. A code has a message when all three titles and messages are non-empty,
//! help when all three `help` texts are non-empty strings, and an explain page when all
//! three rendered `explain` pages are non-empty. The catalog lists `help: null` for every
//! code today, so the strict values are 0 until it gains help text; the evidence also gives
//! the message-and-explain-only figure so the founder can rule on the interpretation.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The code families of the M1 code set.
pub const M1_FAMILIES: &[&str] = &["K01", "K02", "K09", "SEC01"];

/// Codes in spec §27 (the "Diagnostic index" table): `(code, retired)`.
pub fn spec_codes(spec: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in spec.lines() {
        if line.starts_with("## ") {
            in_section = line.starts_with("## 27.");
            continue;
        }
        if !in_section || !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let (Some(code), Some(meaning)) = (cells.get(1), cells.get(2)) else {
            continue;
        };
        let retired = meaning.to_lowercase().contains("*retired*");
        for c in expand(code) {
            out.push((c, retired));
        }
    }
    out
}

/// `K0901–K0926` (en dash or hyphen) → every code in the range; a single code → itself;
/// anything else (table header, separator) → nothing.
fn expand(cell: &str) -> Vec<String> {
    let split = cell.split_once('–').or_else(|| cell.split_once('-'));
    if let Some((a, b)) = split {
        let (pa, na) = code_parts(a.trim());
        let (pb, nb) = code_parts(b.trim());
        return match (pa, na, pb, nb) {
            (Some(pa), Some(na), Some(pb), Some(nb)) if pa == pb && na <= nb => {
                (na..=nb).map(|n| format!("{pa}{n:04}")).collect()
            }
            _ => Vec::new(),
        };
    }
    match code_parts(cell) {
        (Some(p), Some(n)) => vec![format!("{p}{n:04}")],
        _ => Vec::new(),
    }
}

fn code_parts(s: &str) -> (Option<&str>, Option<u32>) {
    let split = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let (prefix, digits) = s.split_at(split);
    let ok_prefix = matches!(prefix, "K" | "W" | "H" | "L" | "SEC");
    if !ok_prefix || digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return (None, None);
    }
    (Some(prefix), digits.parse().ok())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeStatus {
    pub in_spec: bool,
    pub in_catalog: bool,
    pub message: bool,
    pub help: bool,
    pub explain: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatalogReport {
    pub codes: BTreeMap<String, CodeStatus>,
    pub retired_dropped: Vec<String>,
}

impl CatalogReport {
    pub fn total(&self) -> usize {
        self.codes.len()
    }

    fn count(&self, f: impl Fn(&CodeStatus) -> bool) -> usize {
        self.codes.values().filter(|s| f(s)).count()
    }

    fn pct(&self, n: usize) -> f64 {
        if self.total() == 0 {
            0.0
        } else {
            100.0 * n as f64 / self.total() as f64
        }
    }

    /// P6 (strict): message, help and explain page in all three languages.
    pub fn p6_pct(&self) -> f64 {
        self.pct(self.count(|s| s.message && s.help && s.explain))
    }

    /// `D5.diag_completeness` (strict): stable code + message + help + explain page, ×3.
    /// Every code here has a stable code (spec §27 or the catalog), so it equals P6.
    pub fn completeness_pct(&self) -> f64 {
        self.p6_pct()
    }

    /// The alternative reading (no separate help text required): message + explain page.
    pub fn message_and_explain_pct(&self) -> f64 {
        self.pct(self.count(|s| s.message && s.explain))
    }

    pub fn evidence(&self) -> String {
        let missing_entry: Vec<&str> = self
            .codes
            .iter()
            .filter(|(_, s)| !s.in_catalog)
            .map(|(c, _)| c.as_str())
            .collect();
        format!(
            "M1 code set = {} codes ({} from spec §27 families {:?}, {} catalog-only; retired dropped: {:?}). \
             message x3: {}; explain page x3 (`kalam explain`): {}; help text x3: {} \
             (source: kalam_diagnostics::explain::catalog_value, i.e. `kalam explain --catalog --json`). \
             Strict (message + help + explain): {:.2}%. \
             Alternative reading without separate help text: {:.2}%. Codes without a catalog entry ({}): {:?}",
            self.total(),
            self.count(|s| s.in_spec),
            M1_FAMILIES,
            self.count(|s| !s.in_spec),
            self.retired_dropped,
            self.count(|s| s.message),
            self.count(|s| s.explain),
            self.count(|s| s.help),
            self.p6_pct(),
            self.message_and_explain_pct(),
            missing_entry.len(),
            missing_entry,
        )
    }
}

/// One catalog entry as the harness sees it.
#[derive(Clone, Debug)]
pub struct EntryView {
    pub code: String,
    pub title: [String; 3],
    pub message: [String; 3],
    /// `None`: no help text (`"help": null`).
    pub help: Option<[String; 3]>,
    /// The rendered `kalam explain` page per language; `None` when not listed.
    pub explain: Option<[String; 3]>,
}

/// One `{en, ar, franco}` object of strings; `None` unless all three are strings.
fn by_lang(v: &Value) -> Option<[String; 3]> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Some([s("en")?, s("ar")?, s("franco")?])
}

/// Entries of a `kalam-explain-catalog/1` listing (`kalam explain --catalog --json`).
pub fn entries_from_listing(v: &Value) -> Vec<EntryView> {
    let empty = || [String::new(), String::new(), String::new()];
    v["codes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some(EntryView {
                code: c["code"].as_str()?.to_string(),
                title: by_lang(&c["title"]).unwrap_or_else(empty),
                message: by_lang(&c["message"]).unwrap_or_else(empty),
                help: by_lang(&c["help"]),
                explain: by_lang(&c["explain"]),
            })
        })
        .collect()
}

/// The compiled-in catalog, as `kalam explain --catalog --json` lists it.
pub fn builtin_entries() -> Vec<EntryView> {
    entries_from_listing(&kalam_diagnostics::explain::catalog_value())
}

pub fn measure(spec: &str, entries: &[EntryView]) -> CatalogReport {
    let in_family = |c: &str| M1_FAMILIES.iter().any(|f| c.starts_with(f));
    let mut codes: BTreeMap<String, CodeStatus> = BTreeMap::new();
    let mut retired = BTreeSet::new();
    for (code, is_retired) in spec_codes(spec) {
        if !in_family(&code) {
            continue;
        }
        if is_retired {
            retired.insert(code);
            continue;
        }
        codes.entry(code).or_default().in_spec = true;
    }
    let full = |a: &[String; 3]| a.iter().all(|s| !s.trim().is_empty());
    for e in entries {
        if retired.contains(&e.code) {
            continue;
        }
        let s = codes.entry(e.code.clone()).or_default();
        s.in_catalog = true;
        s.message = full(&e.title) && full(&e.message);
        s.explain = e.explain.as_ref().is_some_and(full);
        s.help = e.help.as_ref().is_some_and(full);
    }
    CatalogReport {
        codes,
        retired_dropped: retired.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "## 26. X\n| K0101 | not here |\n## 27. Diagnostic index\n\n| Code | Meaning |\n|---|---|\n\
        | SEC0100 | BOM |\n| SEC0107 | *retired* (was: x) |\n| K0201 | other skin |\n| H0202 | hint |\n\
        | K0601 | types |\n| K0901–K0903 | LLM |\n\n---\n\n## 28. Examples\n| K0102 | not here |\n";

    fn entry(code: &str, msg: &str) -> EntryView {
        let s = |t: &str| [t.to_string(), t.to_string(), t.to_string()];
        EntryView {
            code: code.into(),
            title: s("T"),
            message: s(msg),
            help: None,
            explain: (!msg.is_empty()).then(|| s("page")),
        }
    }

    #[test]
    fn parses_section_27_only_with_ranges_and_retired_rows() {
        let codes = spec_codes(SPEC);
        let names: Vec<&str> = codes.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(
            names,
            ["SEC0100", "SEC0107", "K0201", "H0202", "K0601", "K0901", "K0902", "K0903"]
        );
        assert!(codes.iter().any(|(c, r)| c == "SEC0107" && *r));
        assert_eq!(expand("K0901-K0902").len(), 2);
        assert!(expand("Code").is_empty());
        assert!(expand("---").is_empty());
    }

    #[test]
    fn strict_parity_needs_help_text() {
        let entries = vec![
            entry("SEC0100", "m"),
            entry("K0201", "m"),
            entry("K0250", "m"), // catalog-only code joins the set
            entry("K0901", ""),  // empty message
        ];
        let r = measure(SPEC, &entries);
        // spec: SEC0100, K0201, K0901-K0903 (K0601 and H0202 are outside the families); + K0250
        assert_eq!(r.total(), 6, "{:?}", r.codes.keys());
        assert_eq!(r.retired_dropped, vec!["SEC0107".to_string()]);
        assert_eq!(r.p6_pct(), 0.0, "no help text: never complete");
        assert!((r.message_and_explain_pct() - 50.0).abs() < 1e-9);
        assert!(r.evidence().contains("K0902"));

        let mut with_help = entries.clone();
        for e in &mut with_help {
            e.help = Some(["h".into(), "h".into(), "h".into()]);
        }
        let r = measure(SPEC, &with_help);
        assert!((r.p6_pct() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn reads_the_explain_catalog_listing() {
        let listing = serde_json::json!({"schema": "kalam-explain-catalog/1", "codes": [
            {"code": "K0201", "title": {"en": "t", "ar": "t", "franco": "t"},
             "message": {"en": "m", "ar": "m", "franco": "m"}, "help": null,
             "explain": {"en": "p", "ar": "p", "franco": "p"}},
            {"code": "K0250", "title": {"en": "t", "ar": "t", "franco": "t"},
             "message": {"en": "m", "ar": "m", "franco": "m"},
             "help": {"en": "h", "ar": "h", "franco": "h"}, "explain": {"en": "p", "ar": "", "franco": "p"}}
        ]});
        let e = entries_from_listing(&listing);
        assert_eq!(e.len(), 2);
        assert!(e[0].help.is_none() && e[0].explain.is_some());
        let r = measure("", &e);
        assert!(r.codes["K0201"].explain && !r.codes["K0201"].help);
        assert!(
            r.codes["K0250"].help && !r.codes["K0250"].explain,
            "an empty ar page"
        );
    }

    #[test]
    fn repo_catalog_and_spec_give_a_plausible_denominator() {
        let spec = include_str!("../../../../spec/KALAM-SPEC.md");
        let r = measure(spec, &builtin_entries());
        assert!(r.total() >= 60, "{}", r.total());
        assert!(r.retired_dropped.contains(&"SEC0107".to_string()));
        assert!(!r.codes.contains_key("SEC0107"));
        // the catalog has no help text, so the strict figure is 0
        assert_eq!(r.p6_pct(), 0.0);
        assert!(r.message_and_explain_pct() < 100.0);
    }
}
