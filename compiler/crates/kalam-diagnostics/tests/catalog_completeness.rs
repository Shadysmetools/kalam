//! M1 metrics D5.diag_completeness and G-PAR P6 (eval/scorecard.json; FOUNDER-QUEUE Q11,
//! strict reading): every code in the P6 code set has a title, a message, help text and an
//! explain page in all three languages, and every placeholder a template uses is supplied by
//! every emitter of that code.
//!
//! **P6 code set:** every spec §27 code in the K01xx, K02xx, K09xx and SEC01xx families
//! (ranges such as `K0901–K0926` expanded, rows marked *retired* dropped), read from
//! `spec/KALAM-SPEC.md` itself, plus every catalog code and every emitted code.
//!
//! kalam-diagnostics cannot depend on the crates that emit codes (they depend on it), so this
//! test scans their source: every quoted `SEC`/`K`/`W`/`H`/`L` + four-digit literal in
//! `crates/*/src/**/*.rs`, ignoring everything from the first `#[cfg(test)]` in a file
//! (test fixtures and mocks mention codes nobody emits). Emitters reach codes indirectly
//! (`Err("K0127")` returned by a validator, `self.diag(code, span)`, `error_at(codes::X)`),
//! so the args each code's emit sites all supply are kept in [`EMITTER_ARGS`], a table read
//! off the emitters by hand. The scan keeps that table honest: its codes must equal the
//! scanned codes, and each listed arg must appear as `.arg("name"` in the emitting crate.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use kalam_diagnostics::catalog;

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Quoted literals of the form `"PREFIX0000"` where PREFIX is one of the code families.
fn codes_in(text: &str, out: &mut BTreeSet<String>) {
    for (i, _) in text.match_indices('"') {
        let rest = &text[i + 1..];
        let Some(end) = rest.find('"') else { continue };
        let lit = &rest[..end];
        let prefix_len = lit.len().saturating_sub(4);
        if !lit.is_char_boundary(prefix_len) {
            continue; // a non-ASCII literal (`"… §27."`) is never a code
        }
        let (prefix, digits) = lit.split_at(prefix_len);
        if matches!(prefix, "SEC" | "K" | "W" | "H" | "L")
            && digits.bytes().all(|b| b.is_ascii_digit())
        {
            out.insert(lit.to_string());
        }
    }
}

fn emitted_codes() -> BTreeSet<String> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in fs::read_dir(&crates).expect("crates dir").flatten() {
        if entry.file_name() == "kalam-diagnostics" {
            continue; // the catalog itself
        }
        if entry.file_name() == "kalam-eval" {
            continue; // the eval harness names codes (retired ones too) but emits none
        }
        rust_files(&entry.path().join("src"), &mut files);
    }
    let mut codes = BTreeSet::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_default();
        let live = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => &text[..],
        };
        codes_in(live, &mut codes);
    }
    codes
}

#[test]
fn scanner_finds_codes_in_quotes_only() {
    let mut out = BTreeSet::new();
    codes_in(r#"x("K0250"); "SEC0111" "K12" "Kx123" "note""#, &mut out);
    assert_eq!(
        out.into_iter().collect::<Vec<_>>(),
        vec!["K0250", "SEC0111"]
    );
}

#[test]
fn every_emitted_code_has_a_catalog_entry_in_three_languages() {
    let codes = emitted_codes();
    // Sanity: the scan really sees the lexer and parser.
    for known in ["K0129", "K0250", "K0921"] {
        assert!(codes.contains(known), "scan missed {known}: {codes:?}");
    }
    let mut missing = Vec::new();
    for code in &codes {
        match catalog::lookup(code) {
            None => missing.push(format!("{code}: no entry")),
            Some(e) => {
                for (k, text) in e.title.iter().chain(e.message.iter()).enumerate() {
                    if text.trim().is_empty() {
                        missing.push(format!("{code}: empty text #{k}"));
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "catalog gaps: {missing:#?}");
}

#[test]
fn messages_have_no_unfilled_placeholders_for_known_args() {
    // Parser codes: K0250 gets `expected`/`found` and K0260 `tag`; these get no args, so a
    // stray `{x}` would be printed raw.
    let no_args = [
        "K0251", "K0252", "K0253", "K0254", "K0255", "K0256", "K0259", "K0710", "K0720", "K0731",
        "K0921",
    ];
    for code in no_args {
        let e = catalog::lookup(code).unwrap_or_else(|| panic!("{code} missing"));
        for msg in e.message {
            let rendered = catalog::render_template(msg, &[]);
            assert!(
                !rendered.contains("{expected}") && !rendered.contains("{found}"),
                "{code}: {msg}"
            );
        }
    }
    let e = catalog::lookup("K0250").expect("K0250");
    let args = vec![
        ("expected", "`:`".to_string()),
        ("found", "end of line".to_string()),
    ];
    for msg in e.message {
        let rendered = catalog::render_template(msg, &args);
        assert!(
            rendered.contains("`:`") && rendered.contains("end of line"),
            "{rendered}"
        );
        assert!(!rendered.contains('{'), "{rendered}");
    }
}

#[test]
fn messages_keep_a_positive_tone() {
    // Spec §21.4 rule 3, over every English text a user reads.
    for e in catalog::CATALOG {
        for text in [e.title[0], e.message[0], e.help[0], e.explain[0]] {
            let en = text.to_lowercase();
            for word in ["illegal", "invalid", "fatal", " bad "] {
                assert!(!en.contains(word), "{}: {text}", e.code);
            }
        }
    }
}

// ---- P6 code set (FOUNDER-QUEUE Q11) -------------------------------------------------

/// The P6 families (eval `kalam-eval/src/diagcat.rs` `M1_FAMILIES`).
const P6_FAMILIES: &[&str] = &["K01", "K02", "K09", "SEC01"];

fn spec_text() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../spec/KALAM-SPEC.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `K0901` → `("K", 901)`; anything that is not one code → `None`.
fn code_parts(s: &str) -> Option<(&str, u32)> {
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let (prefix, digits) = s.split_at(split);
    let ok = matches!(prefix, "K" | "W" | "H" | "L" | "SEC")
        && digits.len() == 4
        && digits.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| (prefix, digits.parse().unwrap()))
}

/// The spec §27 table: `(code, retired)`, with `K0901–K0926` ranges expanded.
fn spec_codes(spec: &str) -> Vec<(String, bool)> {
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
        let (Some(cell), Some(meaning)) = (cells.get(1), cells.get(2)) else {
            continue;
        };
        let retired = meaning.to_lowercase().contains("*retired*");
        let codes: Vec<String> = match cell.split_once('–').or_else(|| cell.split_once('-')) {
            Some((a, b)) => match (code_parts(a.trim()), code_parts(b.trim())) {
                (Some((pa, na)), Some((pb, nb))) if pa == pb && na <= nb => {
                    (na..=nb).map(|n| format!("{pa}{n:04}")).collect()
                }
                _ => Vec::new(),
            },
            None => code_parts(cell)
                .map(|(p, n)| vec![format!("{p}{n:04}")])
                .unwrap_or_default(),
        };
        out.extend(codes.into_iter().map(|c| (c, retired)));
    }
    out
}

/// The P6 code set: spec §27 families minus retired codes, plus catalog and emitted codes.
fn p6_codes() -> BTreeSet<String> {
    let spec = spec_codes(&spec_text());
    let retired: BTreeSet<&str> = spec
        .iter()
        .filter(|(_, r)| *r)
        .map(|(c, _)| c.as_str())
        .collect();
    let mut set: BTreeSet<String> = spec
        .iter()
        .filter(|(c, r)| !r && P6_FAMILIES.iter().any(|f| c.starts_with(f)))
        .map(|(c, _)| c.clone())
        .collect();
    set.extend(catalog::CATALOG.iter().map(|e| e.code.to_string()));
    set.extend(
        emitted_codes()
            .into_iter()
            .filter(|c| !retired.contains(c.as_str())),
    );
    set
}

#[test]
fn p6_code_set_comes_from_spec_section_27() {
    let spec = spec_codes(&spec_text());
    assert!(
        spec.iter().any(|(c, r)| c == "SEC0107" && *r),
        "SEC0107 is retired"
    );
    let k09 = spec.iter().filter(|(c, _)| c.starts_with("K09")).count();
    assert_eq!(k09, 26, "K0901–K0926 expands to 26 codes");
    let set = p6_codes();
    for code in [
        "SEC0100", "SEC0124", "K0103", "K0132", "K0201", "K0901", "K0926", "K0250",
    ] {
        assert!(set.contains(code), "P6 set misses {code}");
    }
    // Retired codes never enter the set. (Codes of other families can: since M2 the
    // catalog covers every spec §27 family, and the P6 set includes every catalog code.)
    assert!(!set.contains("SEC0107"), "P6 set must not hold SEC0107");
    assert!(set.len() >= 67, "P6 set has {} codes: {set:?}", set.len());
}

#[test]
fn proposed_flag_matches_spec_section_27() {
    // An explain page says "proposed, not yet in spec §27" exactly for codes §27 lacks.
    let listed: BTreeSet<String> = spec_codes(&spec_text())
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    for e in catalog::CATALOG {
        assert_eq!(
            e.proposed,
            !listed.contains(e.code),
            "{}: `proposed` disagrees with spec §27",
            e.code
        );
    }
}

#[test]
fn every_p6_code_has_title_message_help_and_explain_in_three_languages() {
    let mut gaps = Vec::new();
    for code in p6_codes() {
        let Some(e) = catalog::lookup(&code) else {
            gaps.push(format!("{code}: no catalog entry"));
            continue;
        };
        let fields = [
            ("title", e.title),
            ("message", e.message),
            ("help", e.help),
            ("explain", e.explain),
        ];
        for (field, texts) in fields {
            for (lang, text) in ["en", "ar", "franco"].iter().zip(texts) {
                if text.trim().is_empty() {
                    gaps.push(format!("{code}.{field}.{lang}: empty"));
                }
            }
        }
        for lang in kalam_base::Skin::ALL {
            let page = kalam_diagnostics::explain::page(&code, lang).unwrap_or_default();
            if !page.contains(e.explain[lang.index()]) || !page.contains(e.title[lang.index()]) {
                gaps.push(format!("{code}: explain page ({lang:?}) is incomplete"));
            }
        }
    }
    assert!(gaps.is_empty(), "P6 gaps: {gaps:#?}");
}

// ---- M2 strict code set (FOUNDER-QUEUE Q24) -------------------------------------------

/// The M2 strict denominator of D5.diag_completeness (Q24): every spec §27 code in every
/// family, minus retired rows, plus every catalog code and every emitted code.
fn m2_codes() -> BTreeSet<String> {
    let spec = spec_codes(&spec_text());
    let retired: BTreeSet<&str> = spec
        .iter()
        .filter(|(_, r)| *r)
        .map(|(c, _)| c.as_str())
        .collect();
    let mut set: BTreeSet<String> = spec
        .iter()
        .filter(|(_, r)| !r)
        .map(|(c, _)| c.clone())
        .collect();
    set.extend(catalog::CATALOG.iter().map(|e| e.code.to_string()));
    set.extend(
        emitted_codes()
            .into_iter()
            .filter(|c| !retired.contains(c.as_str())),
    );
    set
}

#[test]
fn m2_code_set_is_every_spec_section_27_code_plus_the_catalog() {
    let set = m2_codes();
    for code in [
        "K0601", "K1001", "K1202", "K1203", "K1601", "K1702", "W0114", "H0202", "L0114", "W0720",
        "K0418",
    ] {
        assert!(set.contains(code), "M2 set misses {code}");
    }
    assert!(!set.contains("SEC0107"), "retired codes are dropped");
    assert!(set.len() >= 136, "M2 set has {} codes", set.len());
}

#[test]
fn every_m2_code_has_title_message_help_and_explain_in_three_languages() {
    // D5.diag_completeness (strict, Q24): the goal is >= 90%; the catalog aims for all.
    let set = m2_codes();
    let mut gaps = Vec::new();
    for code in &set {
        let Some(e) = catalog::lookup(code) else {
            gaps.push(format!("{code}: no catalog entry"));
            continue;
        };
        for (field, texts) in [
            ("title", e.title),
            ("message", e.message),
            ("help", e.help),
            ("explain", e.explain),
        ] {
            for (lang, text) in ["en", "ar", "franco"].iter().zip(texts) {
                if text.trim().is_empty() {
                    gaps.push(format!("{code}.{field}.{lang}: empty"));
                }
            }
        }
    }
    assert!(
        gaps.is_empty(),
        "M2 gaps ({} of {}): {gaps:#?}",
        gaps.len(),
        set.len()
    );
}

// ---- Placeholders are supplied by the emitters ------------------------------------------

/// The args that **every** emit site of a code supplies (read off kalam-lexer, kalam-parser,
/// kalam-resolve and kalam-typeck; a site that supplies more is fine). A template may use only these names,
/// plus keyword placeholders (`{kw_*}`, `{core_*}`, `{punct_*}`), which the renderer spells
/// in the file's skin.
const EMITTER_ARGS: &[(&str, &[&str])] = &[
    // kalam-lexer: `check_text_char` (cp, name) and the identifier/code-character paths.
    // SEC0100 also comes from `lex_bytes` (invalid UTF-8), with the same args.
    ("SEC0100", &["cp", "name"]),
    // kalam-lexer `not_nfc`: one per run of text that is not NFC.
    ("SEC0101", &["cp", "name"]),
    ("SEC0102", &["cp", "name"]),
    ("SEC0103", &["cp", "name"]),
    ("SEC0105", &["cp", "name", "word"]),
    ("SEC0106", &["cp", "name"]),
    ("SEC0110", &["cp", "name"]),
    ("SEC0111", &["cp", "name"]),
    ("SEC0112", &["cp", "name"]),
    ("SEC0113", &["cp", "name"]),
    // kalam-lexer `number`: validators return the code, the `Err(code)` arm adds args.
    ("SEC0120", &["cp", "literal"]),
    ("SEC0121", &["cp", "literal"]),
    ("SEC0122", &["cp", "literal", "fixed"]),
    ("SEC0123", &[]),
    ("SEC0124", &["cp"]),
    ("K0103", &[]),
    ("K0105", &["cp", "name"]),
    ("K0120", &[]), // lexer and parser (`codes::UNEXPECTED_INDENT`)
    ("K0122", &[]),
    ("K0125", &["literal", "fixed"]),
    ("K0127", &["word"]),
    ("K0128", &["literal"]),
    ("K0129", &[]),
    ("K0130", &["escape"]),
    ("K0131", &["kw_let"]),
    ("K0132", &["escape"]),
    ("K0201", &["found", "expected"]),
    ("K0904", &[]),
    ("K0911", &["found", "expected"]),
    ("K0924", &[]),
    // kalam-parser (`codes.rs`): only K0250 and K0260 have args. K0253 has one site with `problem`
    // and one (`error_at`) without, so `problem` is not guaranteed.
    ("K0250", &["expected", "found"]),
    ("K0251", &[]),
    ("K0252", &[]),
    ("K0253", &[]),
    ("K0254", &[]),
    ("K0255", &[]),
    ("K0256", &[]),
    ("K0259", &[]),
    // K0260 (FOUNDER-QUEUE Q53): `mistakes.rs` `f_string`, the unknown tag.
    ("K0260", &["tag"]),
    ("K0710", &[]),
    ("K0720", &[]),
    ("K0731", &[]),
    // kalam-parser `mistakes.rs`: the K09xx "common mistake" rules (spec §27) carry no args;
    // their templates use keyword placeholders only.
    ("K0901", &[]),
    ("K0902", &[]),
    ("K0903", &[]),
    ("K0905", &[]),
    ("K0906", &[]),
    ("K0907", &[]),
    ("K0908", &[]),
    ("K0909", &[]),
    ("K0910", &[]),
    ("K0912", &[]),
    ("K0913", &[]),
    ("K0914", &[]),
    ("K0915", &[]),
    ("K0917", &[]),
    ("K0918", &[]),
    ("K0919", &[]),
    ("K0920", &[]),
    ("K0921", &[]),
    ("K0923", &[]),
    ("K0925", &[]),
    ("K0926", &[]),
    // kalam-resolve (`walk.rs`): names and modules. K0201 and K0917 are also emitted there,
    // with the same args as the lexer's and the parser's (`found`, `expected`; none).
    ("SEC0104", &["name", "other"]),
    ("K0301", &["name"]),
    ("K0302", &[]),
    ("K0303", &["name"]),
    ("K0401", &["name"]),
    ("K0405", &["name"]),
    ("K0413", &["name"]),
    ("K0417", &[]),
    ("K1201", &["cycle"]),
    // kalam-resolve, FOUNDER-QUEUE Q23: importing a private item, or reaching it through a
    // module alias (both args come in backticks, like K0401's `name`).
    ("K0418", &["name", "module"]),
    // kalam-parser (`codes::MODULE_VAR`, `codes::GLOB_IMPORT`, through `mistake`): no args.
    ("K1202", &[]),
    ("K1203", &[]),
    // kalam-typeck (`error.rs`, `TypeError::to_diagnostic`): the args every variant mapped to
    // the code supplies. K0600 has none in common (Mismatch gives `expected`/`found`, the
    // others `type`); K0603 only `type` (NoOperator adds `op`, MissingTrait `trait`).
    // K0710 and K0731 are also emitted by the parser, with no args there either.
    ("K0126", &["type"]),
    ("K0416", &[]),
    ("K0420", &["type", "name"]),
    ("K0600", &[]),
    ("K0601", &["left", "right"]),
    ("K0602", &["left", "right"]),
    ("K0603", &["type"]),
    ("K0612", &[]),
    ("K0650", &[]),
    ("K0730", &["type"]),
    ("K0801", &[]),
    ("K0802", &[]),
    ("K0803", &["name"]),
    ("K0916", &[]),
    ("K1001", &["missing"]),
    ("K1003", &[]),
    ("K1005", &[]),
    ("K1101", &[]),
    ("K1103", &["name"]),
    ("K1105", &["detail"]),
    ("K1106", &[]),
    ("K1303", &[]),
    ("K1601", &["expected", "found"]),
    // kalam-typeck, M2 iteration 2: usefulness (K1002), or-pattern bindings (K1004), labels
    // (K1104), var capture (K1110), `?` (K1301/K1302), path holes (K1641), and `f[T](x)`
    // (K0922, emitted by the checker because it needs to know `f` is a function).
    ("K0922", &[]),
    ("K1002", &[]),
    ("K1004", &["name"]),
    ("K1104", &["names"]),
    ("K1110", &["name"]),
    ("K1301", &["found", "expected"]),
    ("K1302", &[]),
    ("K1641", &["found"]),
    // kalam-typeck, M2 iteration 3: an impl that does not match its trait (FOUNDER-QUEUE Q27,
    // spec §27 erratum): the trait as written in the impl head, and the first difference.
    ("K1107", &["trait", "detail"]),
    // kalam-typeck, M2 iteration 4: K0804 and K1108 are proposed codes; K1204 is the
    // spec §27 erratum of FOUNDER-QUEUE Q33 (its reason is a label, so it has no args).
    ("K0804", &["type"]),
    ("K1108", &["type", "name"]),
    ("K1204", &[]),
    // kalam-effects: suspension inference (§15). The call chain is a label, not an arg.
    ("K1401", &[]),
    ("K1402", &[]),
    ("W0421", &[]),
    // kalam-sinks (wired through kalam-typeck): capabilities (K1501 for constructing `World`,
    // and from typeck's `error.rs` for decoding it; K1503) and the tagged-literal parsers
    // and hole checks (§17). Their messages use keyword placeholders only.
    ("K1501", &[]),
    ("K1503", &[]),
    ("K1602", &[]),
    ("K1603", &[]),
    ("K1604", &[]),
    ("K1605", &[]),
    ("W1606", &[]),
    ("K1610", &[]),
    ("K1611", &[]),
    ("K1612", &[]),
    ("K1613", &[]),
    ("K1614", &[]),
    ("K1615", &[]),
    ("K1616", &[]),
    ("K1617", &[]),
    ("K1620", &[]),
    ("K1621", &[]),
    ("W1622", &[]),
    ("K1631", &[]),
];

/// Codes nobody emits yet: the args their future emitter must supply. Every other code with
/// no emitter may use keyword placeholders only.
const FUTURE_ARGS: &[(&str, &[&str])] = &[];

/// The crate directory under `crates/` that holds `file`.
fn crate_of(file: &Path) -> String {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let rel = file.strip_prefix(&crates).unwrap_or(file);
    rel.components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Per emitter crate, its live (non-test) source text.
fn emitter_sources() -> BTreeMap<String, String> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in fs::read_dir(&crates).expect("crates dir").flatten() {
        let name = entry.file_name();
        if name == "kalam-diagnostics" || name == "kalam-eval" {
            continue;
        }
        rust_files(&entry.path().join("src"), &mut files);
    }
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_default();
        let live = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => &text[..],
        };
        out.entry(crate_of(&file)).or_default().push_str(live);
    }
    out
}

#[test]
fn emitter_args_table_matches_the_emitter_sources() {
    let table: BTreeSet<String> = EMITTER_ARGS.iter().map(|(c, _)| c.to_string()).collect();
    let scanned = emitted_codes();
    let unlisted: Vec<_> = scanned.difference(&table).collect();
    let stale: Vec<_> = table.difference(&scanned).collect();
    assert!(
        unlisted.is_empty() && stale.is_empty(),
        "update EMITTER_ARGS: emitted but not listed {unlisted:?}; listed but not emitted {stale:?}"
    );
    for (code, _) in FUTURE_ARGS {
        assert!(
            !scanned.contains(*code),
            "{code} is emitted now: move it to EMITTER_ARGS"
        );
    }
    // Each listed arg is really passed by the crate that emits the code.
    let sources = emitter_sources();
    for (code, args) in EMITTER_ARGS {
        let quoted = format!("\"{code}\"");
        let crates: Vec<&String> = sources
            .iter()
            .filter(|(_, text)| text.contains(&quoted))
            .map(|(name, _)| name)
            .collect();
        assert!(!crates.is_empty(), "{code}: no emitter source");
        for arg in *args {
            let call = format!(".arg(\"{arg}\"");
            assert!(
                crates.iter().any(|c| sources[*c].contains(&call)),
                "{code}: no `{call}` in {crates:?}"
            );
        }
    }
}

#[test]
fn template_placeholders_are_supplied_by_every_emitter() {
    let supplied: BTreeMap<&str, &[&str]> =
        EMITTER_ARGS.iter().chain(FUTURE_ARGS).copied().collect();
    let mut problems = Vec::new();
    for e in catalog::CATALOG {
        let args = supplied.get(e.code).copied().unwrap_or(&[]);
        for (field, texts) in [("message", e.message), ("help", e.help)] {
            for (lang, text) in ["en", "ar", "franco"].iter().zip(texts) {
                for name in catalog::placeholders(text) {
                    if catalog::keyword_concept(name).is_none() && !args.contains(&name) {
                        problems.push(format!(
                            "{}.{field}.{lang}: {{{name}}} is not supplied (emitter args {args:?})",
                            e.code
                        ));
                    }
                }
            }
        }
        // Titles and explain bodies are shown without a diagnostic: no placeholders at all.
        for (field, texts) in [("title", e.title), ("explain", e.explain)] {
            for (lang, text) in ["en", "ar", "franco"].iter().zip(texts) {
                let names = catalog::placeholders(text);
                if !names.is_empty() {
                    problems.push(format!("{}.{field}.{lang}: placeholders {names:?}", e.code));
                }
            }
        }
        // A rendered diagnostic never shows a raw placeholder, in any skin.
        let mut d = kalam_diagnostics::Diagnostic::new(
            e.code,
            kalam_base::FileId(0),
            kalam_base::Span::new(0, 0),
        );
        for name in args {
            d = d.arg(name, "x");
        }
        for lang in kalam_base::Skin::ALL {
            for skin in kalam_base::Skin::ALL {
                let text = format!(
                    "{} {}",
                    d.message_in(lang, skin),
                    d.help_in(lang, skin).unwrap_or_default()
                );
                let left = catalog::placeholders(&text);
                if !left.is_empty() {
                    problems.push(format!("{} ({lang:?}/{skin:?}): raw {left:?}", e.code));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_language_uses_the_same_placeholders() {
    // A translation that drops `{fixed}` loses the fix; one that adds `{name}` shows nothing.
    for e in catalog::CATALOG {
        for (field, texts) in [("message", e.message), ("help", e.help)] {
            let sets: Vec<BTreeSet<&str>> = texts
                .iter()
                .map(|t| catalog::placeholders(t).into_iter().collect())
                .collect();
            assert!(
                sets[0] == sets[1] && sets[0] == sets[2],
                "{}.{field}: en {:?}, ar {:?}, franco {:?}",
                e.code,
                sets[0],
                sets[1],
                sets[2]
            );
        }
    }
}
