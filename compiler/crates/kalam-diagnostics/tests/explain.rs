//! `kalam explain` pages (spec §21.4 rule 5, eval M1 checks 14 and 15).
//!
//! - An explain page never shows a raw `{name}` placeholder: each one is replaced by a
//!   description in the reader's message language (`kw_let` by that language's own keyword).
//! - Every placeholder any catalog template uses has a description in all three languages.
//! - `kalam explain --catalog --json` lists every catalog code with its title, message and
//!   help templates, placeholder descriptions and explain page in en, ar and franco.

use kalam_base::Skin;
use kalam_diagnostics::catalog::{self, CATALOG};
use kalam_diagnostics::explain;

/// `{name}` placeholders in a template (the same rule `render_template` fills).
fn placeholders(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close)
                if close > 0
                    && after[..close]
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'_') =>
            {
                out.push(after[..close].to_string());
                rest = &after[close + 1..];
            }
            _ => rest = after,
        }
    }
    out
}

#[test]
fn name_resolution_pages_describe_names_not_characters() {
    // `{name}` in these codes is an identifier or a module path, never one character.
    for code in ["K0301", "K0303", "K0401", "K0405", "K0413", "SEC0104"] {
        for lang in Skin::ALL {
            let page = explain::page(code, lang).unwrap();
            let generic = explain::describe("SEC0100", "name", lang).unwrap();
            assert!(!page.contains(&generic), "{code} ({lang:?}):\n{page}");
        }
    }
}

#[test]
fn k0201_page_describes_its_placeholders() {
    let page = explain::page("K0201", Skin::En).expect("K0201 has a page");
    assert!(!page.contains("{found}"), "{page}");
    assert!(!page.contains("{expected}"), "{page}");
    assert!(page.contains("K0201"));
    assert!(page.contains("WORD FROM ANOTHER SKIN"));
    assert!(page.contains("<the word from another skin>"), "{page}");
    // The lookup is case-insensitive, like the CLI argument.
    assert_eq!(explain::page("k0201", Skin::En), Some(page));
    for lang in Skin::ALL {
        let page = explain::page("K0201", lang).unwrap();
        assert!(
            !page.contains("{found}") && !page.contains("{expected}"),
            "{page}"
        );
    }
}

#[test]
fn no_page_has_a_raw_placeholder() {
    for entry in CATALOG {
        for lang in Skin::ALL {
            let page = explain::page(entry.code, lang).unwrap();
            let left = placeholders(&page);
            assert!(
                left.is_empty(),
                "{} ({lang:?}) still shows {left:?}:\n{page}",
                entry.code
            );
            // Literal braces the templates mention (`\u{...}`) survive.
            if entry.message[lang.index()].contains("\\u{...}") {
                assert!(page.contains("\\u{...}"), "{}: {page}", entry.code);
            }
        }
    }
}

#[test]
fn every_placeholder_has_three_descriptions() {
    for entry in CATALOG {
        for (i, template) in entry.message.iter().chain(entry.help.iter()).enumerate() {
            for name in placeholders(template) {
                for lang in Skin::ALL {
                    let d = explain::describe(entry.code, &name, lang);
                    assert!(
                        d.is_some_and(|d| !d.trim().is_empty()),
                        "{} message/help[{i}]: no {lang:?} description for {{{name}}}",
                        entry.code
                    );
                }
            }
        }
    }
}

#[test]
fn kw_let_is_shown_in_the_readers_language() {
    let en = explain::page("K0131", Skin::En).unwrap();
    let ar = explain::page("K0131", Skin::Ar).unwrap();
    let franco = explain::page("K0131", Skin::Franco).unwrap();
    assert!(en.contains("`let`"), "{en}");
    assert!(ar.contains("`ليكن`"), "{ar}");
    assert!(franco.contains("`khali`"), "{franco}");
}

#[test]
fn pages_show_message_help_and_explanation() {
    let en = explain::page("K0902", Skin::En).unwrap();
    assert!(en.contains("Kalam declares a function with `fn`"), "{en}");
    assert!(en.contains("\nHelp: Write `fn name(param: Type):`"), "{en}");
    assert!(en.contains("Why: "), "{en}");
    assert!(en.contains("    fn greet(name: String):"), "{en}");
    // Examples and keywords are in the reader's own skin.
    let ar = explain::page("K0902", Skin::Ar).unwrap();
    assert!(ar.contains("مساعدة: اكتب `دالة الاسم"), "{ar}");
    assert!(ar.contains("    دالة رحب(الاسم: نص):"), "{ar}");
    let franco = explain::page("K0902", Skin::Franco).unwrap();
    assert!(franco.contains("Mosa3da: Ekteb `dala esm"), "{franco}");
    assert!(franco.contains("    dala rahhab(esm: nass):"), "{franco}");
}

#[test]
fn unknown_codes_have_no_page() {
    assert_eq!(explain::page("K9999", Skin::En), None);
    assert_eq!(explain::page("", Skin::En), None);
}

#[test]
fn catalog_json_lists_every_code_in_three_languages() {
    let text = explain::catalog_json();
    let v: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(v["schema"], "kalam-explain-catalog/1");
    assert_eq!(v["languages"], serde_json::json!(["en", "ar", "franco"]));
    let codes = v["codes"].as_array().unwrap();
    assert_eq!(codes.len(), CATALOG.len());
    let mut sorted: Vec<&str> = codes.iter().map(|c| c["code"].as_str().unwrap()).collect();
    let listed = sorted.clone();
    sorted.sort();
    assert_eq!(listed, sorted, "codes are sorted");
    for c in codes {
        let code = c["code"].as_str().unwrap();
        let entry = catalog::lookup(code).unwrap();
        assert_eq!(c["proposed"], entry.proposed);
        assert!(c["severity"].is_string());
        assert_eq!(c["url"], format!("https://kalam.dev/errors/{code}"));
        for lang in ["en", "ar", "franco"] {
            for field in ["title", "message", "explain"] {
                let s = c[field][lang].as_str().unwrap_or("");
                assert!(!s.trim().is_empty(), "{code}.{field}.{lang}");
            }
            assert!(
                placeholders(c["explain"][lang].as_str().unwrap()).is_empty(),
                "{code}.explain.{lang}"
            );
        }
        // Help text (FOUNDER-QUEUE Q11, strict reading) is a template like the message.
        for lang in ["en", "ar", "franco"] {
            let help = c["help"][lang].as_str().unwrap_or("");
            assert!(!help.trim().is_empty(), "{code}.help.{lang}");
        }
        for p in c["placeholders"].as_array().unwrap() {
            for lang in ["en", "ar", "franco"] {
                assert!(p["description"][lang].is_string(), "{code}: {p}");
            }
        }
    }
    let k0201 = codes.iter().find(|c| c["code"] == "K0201").unwrap();
    let names: Vec<&str> = k0201["placeholders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["found", "expected"]);
}
