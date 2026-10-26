//! `kalam explain` pages (spec §21.4 rule 5) and the machine-readable catalog behind
//! `kalam explain --catalog --json` (eval M1 checks 14 and 15).
//!
//! A catalog message is a template whose `{name}` placeholders are filled from a
//! diagnostic's arguments. An explain page has no diagnostic, so each placeholder is shown
//! as a description in the reader's message language (`<the word from another skin>`),
//! never as the raw `{found}`. A keyword placeholder (`{kw_let}`, `{core_none}`,
//! `{punct_try}`) is shown in the reader's own skin spelling.
//!
//! A page has the title, the message, the help text and the explain body of the catalog
//! entry, in that order.

use kalam_base::Skin;
use kalam_skins::KeywordTable;
use serde_json::{json, Value};

use crate::catalog::{self, Entry, CATALOG};
use crate::{Lang, Severity};

/// Placeholder descriptions: `(code, name, [en, ar, franco])`. A row with a code applies to
/// that code only and wins over the generic row (code `""`).
const DESCRIPTIONS: &[(&str, &str, [&str; 3])] = &[
    (
        "",
        "cp",
        [
            "the code point, such as U+202E",
            "رمز الحرف، مثل U+202E",
            "el code point, zay U+202E",
        ],
    ),
    (
        "",
        "name",
        [
            "the character's Unicode name",
            "اسم الحرف في يونيكود",
            "esm el 7arf f Unicode",
        ],
    ),
    (
        "",
        "literal",
        [
            "the literal as you wrote it",
            "القيمة الحرفية كما كتبتها",
            "el literal zay ma katabto",
        ],
    ),
    (
        "",
        "fixed",
        [
            "the corrected spelling",
            "الكتابة المصححة",
            "el ketaba el sa7",
        ],
    ),
    ("", "type", ["the type", "النوع", "el type"]),
    (
        "K0126",
        "type",
        ["the literal's type", "نوع القيمة الحرفية", "no3 el literal"],
    ),
    // The type checker (kalam-typeck). ar/franco: pending native review.
    (
        "",
        "left",
        [
            "the left operand's type",
            "نوع الطرف الأيسر",
            "no3 el taraf el shemal",
        ],
    ),
    (
        "",
        "right",
        [
            "the right operand's type",
            "نوع الطرف الأيمن",
            "no3 el taraf el yemeen",
        ],
    ),
    (
        "",
        "missing",
        [
            "the cases no arm handles",
            "الحالات التي لا يعالجها أي فرع",
            "el 7alat elly mafeesh far3 bey3alegha",
        ],
    ),
    (
        "",
        "detail",
        [
            "how the arguments differ from the parameters",
            "الفرق بين المعاملات الممرَّرة ومعاملات الدالة",
            "el far2 ben el arguments wel parameters",
        ],
    ),
    (
        "K0420",
        "type",
        [
            "the type of the value before the dot",
            "نوع القيمة التي قبل النقطة",
            "no3 el qeema elly 2abl el no2ta",
        ],
    ),
    (
        "K0420",
        "name",
        [
            "the field or method name as you wrote it",
            "اسم الحقل أو الدالة كما كتبته",
            "esm el field aw el method zay ma katabto",
        ],
    ),
    (
        "K0603",
        "type",
        ["the value's type", "نوع القيمة", "no3 el qeema"],
    ),
    (
        "K0730",
        "type",
        ["the expression's type", "نوع التعبير", "no3 el expression"],
    ),
    (
        "K0803",
        "name",
        [
            "the member's name, such as Circle",
            "اسم العضو، مثل دائرة",
            "esm el member, zay Circle",
        ],
    ),
    (
        "K1103",
        "name",
        ["the method's name", "اسم الدالة", "esm el method"],
    ),
    (
        "K1107",
        "trait",
        [
            "the trait, such as Show or From[ParseError]",
            "السمة، مثل Show أو From[ParseError]",
            "el trait, zay Show aw From[ParseError]",
        ],
    ),
    (
        "K1107",
        "detail",
        [
            "how the implementation differs from the trait",
            "الفرق بين التنفيذ والسمة",
            "el far2 ben el implementation wel trait",
        ],
    ),
    (
        "K0804",
        "type",
        [
            "the type as far as it is known, with `_` for the missing part",
            "النوع بالقدر المعروف منه، و`_` مكان الجزء الناقص",
            "el type 3ala add ma howa ma3roof, w `_` makan el goz2 el na2es",
        ],
    ),
    (
        "K1108",
        "type",
        ["the record's type", "نوع السجل", "no3 el record"],
    ),
    (
        "K1108",
        "name",
        ["the field's name", "اسم الحقل", "esm el field"],
    ),
    (
        "K0418",
        "name",
        [
            "the private item's name",
            "اسم العنصر الخاص",
            "esm el 7aga el private",
        ],
    ),
    (
        "K0418",
        "module",
        [
            "the module that declares it",
            "الوحدة التي تعرّفه",
            "el module elly bet3arrafo",
        ],
    ),
    (
        "K1601",
        "expected",
        [
            "the sink type, such as Sql",
            "نوع المصبّ، مثل Sql",
            "no3 el sink, zay Sql",
        ],
    ),
    (
        "K1601",
        "found",
        [
            "the type you passed, such as String",
            "النوع الذي مرّرته، مثل نص",
            "el type elly ba3ato, zay nass",
        ],
    ),
    (
        "",
        "word",
        [
            "the word as you wrote it",
            "الكلمة كما كتبتها",
            "el kelma zay ma katabtaha",
        ],
    ),
    (
        "",
        "escape",
        [
            "the escape as you wrote it",
            "تسلسل الهروب كما كتبته",
            "el escape zay ma katabto",
        ],
    ),
    ("", "found", ["what I found", "ما وجدته", "elly la2eto"]),
    (
        "",
        "expected",
        ["what I expected", "ما توقعته", "elly kont mestanneeh"],
    ),
    (
        "K0201",
        "found",
        [
            "the word from another skin",
            "الكلمة المكتوبة بواجهة أخرى",
            "el kelma elly men skin tany",
        ],
    ),
    (
        "K0201",
        "expected",
        [
            "the same word in this file's skin",
            "الكلمة نفسها بواجهة هذا الملف",
            "nafs el kelma bel skin bta3 el file da",
        ],
    ),
    // Name resolution (kalam-resolve): `{name}` is an identifier or a module path.
    (
        "K0301",
        "name",
        ["the module's path", "مسار الوحدة", "el path bta3 el module"],
    ),
    (
        "K0303",
        "name",
        [
            "the private type's name",
            "اسم النوع الخاص",
            "esm el type el private",
        ],
    ),
    (
        "K0401",
        "name",
        [
            "the name as you wrote it",
            "الاسم كما كتبته",
            "el esm zay ma katabto",
        ],
    ),
    (
        "K0405",
        "name",
        [
            "the name as you wrote it",
            "الاسم كما كتبته",
            "el esm zay ma katabto",
        ],
    ),
    (
        "K0413",
        "name",
        [
            "the core name as you wrote it",
            "الاسم الأساسي كما كتبته",
            "el core name zay ma katabto",
        ],
    ),
    (
        "SEC0104",
        "name",
        [
            "one name with its code points",
            "أحد الاسمين برموز حروفه",
            "esm menhom b el code points bta3to",
        ],
    ),
    (
        "SEC0104",
        "other",
        [
            "the other name with its code points",
            "الاسم الآخر برموز حروفه",
            "el esm el tany b el code points bta3to",
        ],
    ),
    (
        "K1201",
        "cycle",
        [
            "the modules of the cycle, in import order",
            "وحدات الدائرة بترتيب الاستيراد",
            "el modules elly f el dayra b tarteeb el import",
        ],
    ),
    (
        "K0911",
        "found",
        [
            "the symbol you wrote, such as &&",
            "الرمز الذي كتبته، مثل &&",
            "el ramz elly katabto, zay &&",
        ],
    ),
    (
        "K0911",
        "expected",
        [
            "the keyword for it in this file's skin",
            "الكلمة المحجوزة المقابلة بواجهة هذا الملف",
            "el keyword elly badalo bel skin bta3 el file da",
        ],
    ),
];

/// The description of placeholder `name` in `code`'s templates, in `lang`.
pub fn describe(code: &str, name: &str, lang: Lang) -> Option<String> {
    if let Some(concept) = catalog::keyword_concept(name) {
        return Some(KeywordTable::builtin().spelling(concept, lang).to_string());
    }
    DESCRIPTIONS
        .iter()
        .find(|(c, n, _)| *c == code && *n == name)
        .or_else(|| {
            DESCRIPTIONS
                .iter()
                .find(|(c, n, _)| c.is_empty() && *n == name)
        })
        .map(|(_, _, texts)| format!("<{}>", texts[lang.index()]))
}

/// Every placeholder name across `entry`'s message and help templates (all languages).
fn entry_placeholders(entry: &Entry) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for template in entry.message.iter().chain(entry.help.iter()) {
        for name in catalog::placeholders(template) {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// `template` with every placeholder replaced by its description in `lang`.
fn described(entry: &Entry, template: &str, lang: Lang) -> String {
    let args: Vec<(&str, String)> = entry_placeholders(entry)
        .into_iter()
        .filter_map(|name| describe(entry.code, name, lang).map(|d| (name, d)))
        .collect();
    catalog::render_template(template, &args)
}

fn docs_url(code: &str) -> String {
    format!("https://kalam.dev/errors/{code}")
}

const PROPOSED_NOTE: [&str; 3] = [
    "(This code is proposed and not yet listed in spec §27.)",
    "(هذا الرمز مقترح ولم يُدرج بعد في القسم 27 من المواصفة.)",
    "(El code da mo2tara7 w lessa mesh mawgood f spec §27.)",
];

fn render_page(entry: &Entry, lang: Lang) -> String {
    let i = lang.index();
    let mut out = format!("{}  {}\n\n", entry.code, entry.title[i]);
    out.push_str(&described(entry, entry.message[i], lang));
    out.push_str("\n\n");
    out.push_str(catalog::ui(lang, catalog::Ui::Help));
    out.push(' ');
    out.push_str(&described(entry, entry.help[i], lang));
    out.push_str("\n\n");
    out.push_str(entry.explain[i]);
    out.push('\n');
    if entry.proposed {
        out.push('\n');
        out.push_str(PROPOSED_NOTE[i]);
        out.push('\n');
    }
    out.push_str(&format!(
        "\n{} {}\n",
        catalog::ui(lang, catalog::Ui::LearnMore),
        docs_url(entry.code)
    ));
    out
}

/// The `kalam explain CODE` page in `lang`, or `None` for a code with no catalog entry.
/// `code` is matched case-insensitively.
pub fn page(code: &str, lang: Lang) -> Option<String> {
    let code = code.trim().to_ascii_uppercase();
    catalog::lookup(&code).map(|entry| render_page(entry, lang))
}

/// `kalam explain --catalog`: one line per code, `CODE  TITLE`, in `lang`.
pub fn catalog_list(lang: Lang) -> String {
    let mut entries: Vec<&Entry> = CATALOG.iter().collect();
    entries.sort_by_key(|e| e.code);
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!("{:<8} {}\n", e.code, e.title[lang.index()]));
    }
    out
}

fn by_lang(f: impl Fn(Lang) -> Value) -> Value {
    let mut map = serde_json::Map::new();
    for lang in Skin::ALL {
        map.insert(lang.id().to_string(), f(lang));
    }
    Value::Object(map)
}

/// `kalam explain --catalog --json`: every catalog code, sorted, with its title, message
/// and help templates, placeholder descriptions and explain page in `en`, `ar` and
/// `franco`.
pub fn catalog_value() -> Value {
    let mut entries: Vec<&Entry> = CATALOG.iter().collect();
    entries.sort_by_key(|e| e.code);
    let codes: Vec<Value> = entries
        .into_iter()
        .map(|e| {
            let placeholders: Vec<Value> = entry_placeholders(e)
                .into_iter()
                .map(|name| {
                    json!({
                        "name": name,
                        "description": by_lang(|lang| match describe(e.code, name, lang) {
                            Some(d) => Value::String(d),
                            None => Value::Null,
                        }),
                    })
                })
                .collect();
            json!({
                "code": e.code,
                "severity": Severity::from_code(e.code).as_str(),
                "proposed": e.proposed,
                "title": by_lang(|lang| Value::String(e.title[lang.index()].to_string())),
                "message": by_lang(|lang| Value::String(e.message[lang.index()].to_string())),
                "placeholders": placeholders,
                "help": by_lang(|lang| Value::String(e.help[lang.index()].to_string())),
                "explain": by_lang(|lang| Value::String(render_page(e, lang))),
                "url": docs_url(e.code),
            })
        })
        .collect();
    json!({
        "schema": "kalam-explain-catalog/1",
        "languages": ["en", "ar", "franco"],
        "codes": codes,
    })
}

/// [`catalog_value`] as pretty-printed JSON text.
pub fn catalog_json() -> String {
    serde_json::to_string_pretty(&catalog_value()).expect("catalog JSON serializes")
}
