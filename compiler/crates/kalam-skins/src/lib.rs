//! The Kalam keyword table.
//!
//! `spec/keywords.json` is the single source of truth for every reserved spelling in every
//! skin (spec header and §7). It is embedded at build time, so the `kalam` binary and the
//! WASM playground are self-contained and each compiler release pins one table.
//!
//! Every later compiler stage works with [`Concept`] values, never with spellings
//! (ARCHITECTURE.md §3.2 rule 2). Only the lexer, the printer, diagnostics and LSP completion
//! look at spellings, and they get them from here.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use kalam_base::Skin;
use serde::Deserialize;

/// The embedded keyword table. Path is relative to this file:
/// `compiler/crates/kalam-skins/src/lib.rs` → `new-language/spec/keywords.json`.
/// Before publishing to crates.io, an `xtask sync-keywords` step copies the file into the
/// crate (crates cannot include files outside their directory); the spec copy stays canonical.
pub const KEYWORDS_JSON: &str = include_str!("../../../../spec/keywords.json");

macro_rules! concepts {
    ($($variant:ident = $name:literal,)*) => {
        /// One reserved concept: a row of `keywords.json`. The concept ID is also the KCF token.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Concept {
            $($variant,)*
        }

        impl Concept {
            pub const ALL: &'static [Concept] = &[$(Concept::$variant,)*];

            /// The stable concept ID, for example `KW_IF`.
            pub fn name(self) -> &'static str {
                match self {
                    $(Concept::$variant => $name,)*
                }
            }

            pub fn from_name(name: &str) -> Option<Concept> {
                match name {
                    $($name => Some(Concept::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

concepts! {
    KwLet = "KW_LET",
    KwVar = "KW_VAR",
    KwFn = "KW_FN",
    KwReturn = "KW_RETURN",
    KwIf = "KW_IF",
    KwElse = "KW_ELSE",
    KwMatch = "KW_MATCH",
    KwCase = "KW_CASE",
    KwFor = "KW_FOR",
    KwIn = "KW_IN",
    KwWhile = "KW_WHILE",
    KwBreak = "KW_BREAK",
    KwContinue = "KW_CONTINUE",
    KwPass = "KW_PASS",
    KwAnd = "KW_AND",
    KwOr = "KW_OR",
    KwNot = "KW_NOT",
    KwTrue = "KW_TRUE",
    KwFalse = "KW_FALSE",
    KwType = "KW_TYPE",
    KwEnum = "KW_ENUM",
    KwTrait = "KW_TRAIT",
    KwImpl = "KW_IMPL",
    KwUse = "KW_USE",
    KwAs = "KW_AS",
    KwExport = "KW_EXPORT",
    KwOpaque = "KW_OPAQUE",
    KwTest = "KW_TEST",
    KwSelf = "KW_SELF",
    KwUnsafe = "KW_UNSAFE",
    KwExtern = "KW_EXTERN",
    CoreInt = "CORE_INT",
    CoreFloat = "CORE_FLOAT",
    CoreDecimal = "CORE_DECIMAL",
    CoreBool = "CORE_BOOL",
    CoreString = "CORE_STRING",
    CoreList = "CORE_LIST",
    CoreMap = "CORE_MAP",
    CoreSet = "CORE_SET",
    CoreOption = "CORE_OPTION",
    CoreResult = "CORE_RESULT",
    CoreSome = "CORE_SOME",
    CoreNone = "CORE_NONE",
    CoreOk = "CORE_OK",
    CoreErr = "CORE_ERR",
    CorePrint = "CORE_PRINT",
    CorePanic = "CORE_PANIC",
    CoreAssert = "CORE_ASSERT",
    CoreMain = "CORE_MAIN",
    CoreWorld = "CORE_WORLD",
    PunctComma = "PUNCT_COMMA",
    PunctTry = "PUNCT_TRY",
    PunctDecimalSepArabicIndic = "PUNCT_DECIMAL_SEP_ARABIC_INDIC",
}

impl Concept {
    /// `Some`, `None`, `Ok` and `Err`: the reserved core constructors (grammar token `CORE_CTOR`).
    pub fn is_core_ctor(self) -> bool {
        matches!(
            self,
            Concept::CoreSome | Concept::CoreNone | Concept::CoreOk | Concept::CoreErr
        )
    }
}

/// The `kind` column of `keywords.json` (spec §7, schema paragraph).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Keyword,
    AsciiFixed,
    CoreName,
    Punctuation,
}

/// One row of `keywords.json`, exactly as stored.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub concept: String,
    pub kind: Kind,
    pub en: String,
    pub ar: String,
    pub franco: String,
    #[serde(default)]
    pub franco_aliases: Vec<String>,
    #[serde(default)]
    pub did_you_mean: Vec<String>,
    #[serde(default)]
    pub ar_also_accepts: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
}

impl Row {
    pub fn spelling(&self, skin: Skin) -> &str {
        match skin {
            Skin::En => &self.en,
            Skin::Ar => &self.ar,
            Skin::Franco => &self.franco,
        }
    }
}

/// Whether a matched word used the canonical spelling or an accepted variant
/// (an Arabic hamza/alef form, or a Franco spelling variant). `fmt` rewrites variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    Canonical,
    Variant,
}

/// The result of looking a word up in one skin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordMatch {
    pub concept: Concept,
    pub kind: Kind,
    pub spelling: Spelling,
}

/// Lookup structures for one skin.
#[derive(Clone, Debug)]
pub struct SkinTable {
    skin: Skin,
    /// lookup key (see [`SkinTable::key`]) → match
    words: HashMap<String, WordMatch>,
    canonical: HashMap<Concept, String>,
}

impl SkinTable {
    pub fn skin(&self) -> Skin {
        self.skin
    }

    /// The lookup key for a word in this skin: exact in `en` and `franco`,
    /// `arabic_fold` in `ar` (spec §5.4).
    pub fn key(&self, word: &str) -> String {
        match self.skin {
            Skin::Ar => arabic_fold(word),
            Skin::En | Skin::Franco => word.to_string(),
        }
    }

    /// Looks up a word run (keywords, fixed-ASCII keywords and core names only;
    /// punctuation aliases are lexed as punctuation).
    pub fn lookup_word(&self, word: &str) -> Option<WordMatch> {
        let found = *self.words.get(&self.key(word))?;
        let spelling = if self.canonical.get(&found.concept).map(|s| s.as_str()) == Some(word) {
            Spelling::Canonical
        } else {
            Spelling::Variant
        };
        Some(WordMatch { spelling, ..found })
    }

    /// The canonical spelling of a concept in this skin, as the printer writes it.
    pub fn spelling(&self, concept: Concept) -> &str {
        self.canonical
            .get(&concept)
            .map(|s| s.as_str())
            .expect("every concept has a spelling in every skin (checked at load)")
    }

    /// True if `word` is reserved in this skin, so the printer must backtick-escape it.
    pub fn is_reserved(&self, word: &str) -> bool {
        self.lookup_word(word).is_some()
    }
}

/// The whole table, loaded and validated.
#[derive(Clone, Debug)]
pub struct KeywordTable {
    rows: Vec<Row>,
    by_concept: HashMap<Concept, usize>,
    skins: [SkinTable; 3],
    /// Spellings that are never accepted, used only to improve K0127/K0401 messages.
    did_you_mean: HashMap<String, Concept>,
}

/// A problem found while loading or validating the table. Each one is a spec bug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableError(pub String);

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "keywords.json: {}", self.0)
    }
}

impl std::error::Error for TableError {}

impl KeywordTable {
    /// The table embedded in this compiler build. Panics only if the embedded JSON is
    /// broken, which the unit tests catch.
    pub fn builtin() -> &'static KeywordTable {
        static TABLE: OnceLock<KeywordTable> = OnceLock::new();
        TABLE.get_or_init(|| {
            KeywordTable::load(KEYWORDS_JSON).expect("embedded keywords.json must be valid")
        })
    }

    /// Parses and validates a table (spec §7.4 rules 1–4).
    pub fn load(json: &str) -> Result<KeywordTable, TableError> {
        let rows: Vec<Row> = serde_json::from_str(json).map_err(|e| TableError(e.to_string()))?;

        let mut by_concept = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            let concept = Concept::from_name(&row.concept)
                .ok_or_else(|| TableError(format!("unknown concept `{}`", row.concept)))?;
            if by_concept.insert(concept, i).is_some() {
                return Err(TableError(format!("duplicate concept `{}`", row.concept)));
            }
        }
        for concept in Concept::ALL {
            if !by_concept.contains_key(concept) {
                return Err(TableError(format!("missing concept `{}`", concept.name())));
            }
        }

        let mut skins = Vec::with_capacity(3);
        for skin in Skin::ALL {
            skins.push(build_skin(skin, &rows, &by_concept)?);
        }
        let skins: [SkinTable; 3] = match skins.try_into() {
            Ok(array) => array,
            Err(_) => unreachable!("exactly three skins"),
        };

        let mut did_you_mean = HashMap::new();
        for row in &rows {
            let concept = Concept::from_name(&row.concept).expect("checked above");
            for word in &row.did_you_mean {
                did_you_mean.insert(word.clone(), concept);
            }
        }

        let table = KeywordTable {
            rows,
            by_concept,
            skins,
            did_you_mean,
        };
        table.check_integrity()?;
        Ok(table)
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn row(&self, concept: Concept) -> &Row {
        &self.rows[self.by_concept[&concept]]
    }

    pub fn kind(&self, concept: Concept) -> Kind {
        self.row(concept).kind
    }

    pub fn skin(&self, skin: Skin) -> &SkinTable {
        &self.skins[skin.index()]
    }

    /// The canonical spelling of `concept` in `skin`.
    pub fn spelling(&self, concept: Concept, skin: Skin) -> &str {
        self.row(concept).spelling(skin)
    }

    /// `word` is a keyword or core name of a skin other than `skin` (with that skin's own
    /// lookup: Franco variants, Arabic fold). This is the lexer's K0201 test (spec §5.4):
    /// such a word, used as a plain name, must be declared in the file or written in
    /// backticks (FOUNDER-QUEUE Q9, Q16). Fixed-ASCII keywords are the same in every skin, so
    /// they are never cross-skin words.
    pub fn cross_skin_word(&self, word: &str, skin: Skin) -> Option<WordMatch> {
        Skin::ALL
            .into_iter()
            .filter(|&s| s != skin)
            .filter_map(|s| self.skin(s).lookup_word(word))
            .find(|m| matches!(m.kind, Kind::Keyword | Kind::CoreName))
    }

    /// `word` is reserved in at least one skin (keyword, fixed-ASCII keyword or core name,
    /// with every skin's own lookup).
    pub fn reserved_in_some_skin(&self, word: &str) -> bool {
        Skin::ALL
            .into_iter()
            .any(|s| self.skin(s).is_reserved(word))
    }

    /// If `word` is a known never-accepted spelling (for example the digit-leading Franco
    /// `5ali`), the concept it was meant to be.
    pub fn did_you_mean(&self, word: &str) -> Option<Concept> {
        self.did_you_mean.get(word).copied()
    }

    /// Spec §7.4 rules 1–4. Rules 5–7 need the confusable table and the 2,000-word list,
    /// which become `xtask` checks.
    fn check_integrity(&self) -> Result<(), TableError> {
        for row in &self.rows {
            let words: Vec<&String> = std::iter::once(&row.franco)
                .chain(row.franco_aliases.iter())
                .collect();
            for word in words {
                // Rule 2: no Franco spelling starts with a digit.
                if word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    return Err(TableError(format!(
                        "{}: Franco spelling `{}` starts with a digit",
                        row.concept, word
                    )));
                }
            }
            // Rule 3: no Arabic canonical spelling contains tatweel or harakat.
            if row.ar.chars().any(is_tatweel_or_haraka) {
                return Err(TableError(format!(
                    "{}: Arabic spelling `{}` contains tatweel or harakat",
                    row.concept, row.ar
                )));
            }
            // Rule 4: fixed-ASCII rows are identical ASCII in every skin.
            if row.kind == Kind::AsciiFixed
                && !(row.en == row.ar && row.en == row.franco && row.en.is_ascii())
            {
                return Err(TableError(format!(
                    "{}: ascii_fixed spellings differ between skins",
                    row.concept
                )));
            }
        }
        Ok(())
    }
}

fn build_skin(
    skin: Skin,
    rows: &[Row],
    by_concept: &HashMap<Concept, usize>,
) -> Result<SkinTable, TableError> {
    let mut table = SkinTable {
        skin,
        words: HashMap::new(),
        canonical: HashMap::new(),
    };
    for (concept, &i) in by_concept {
        let row = &rows[i];
        table
            .canonical
            .insert(*concept, row.spelling(skin).to_string());
        if row.kind == Kind::Punctuation {
            continue; // punctuation aliases are lexed as punctuation, not as words
        }
        let mut spellings = vec![row.spelling(skin).to_string()];
        if skin == Skin::Franco {
            spellings.extend(row.franco_aliases.iter().cloned());
        }
        for word in spellings {
            let key = table.key(&word);
            let entry = WordMatch {
                concept: *concept,
                kind: row.kind,
                spelling: Spelling::Canonical,
            };
            // Rule 1: no two concepts share a lookup key in one skin.
            if let Some(previous) = table.words.insert(key.clone(), entry) {
                if previous.concept != *concept {
                    return Err(TableError(format!(
                        "skin {}: `{}` is reserved by both {} and {}",
                        skin,
                        word,
                        previous.concept.name(),
                        concept.name()
                    )));
                }
            }
        }
    }
    Ok(table)
}

/// Arabic orthographic folding used for keyword matching and for identifier skeletons
/// (spec §5.3): أ إ آ ٱ → ا, ى → ي, ة → ه, ؤ → و, ئ → ي, plus the Persian/Urdu letters
/// ی → ي, ک → ك, ە ھ ۀ → ه (DECISIONS.md D19). Identifiers themselves are never
/// rewritten (DR-04): the fold is a comparison key only.
pub fn arabic_fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{0623}' | '\u{0625}' | '\u{0622}' | '\u{0671}' => '\u{0627}', // أ إ آ ٱ → ا
            '\u{0649}' | '\u{06CC}' => '\u{064A}',                           // ى ی → ي
            '\u{06A9}' => '\u{0643}',                                        // ک → ك
            '\u{0629}' | '\u{06D5}' | '\u{06BE}' | '\u{06C0}' => '\u{0647}', // ة ە ھ ۀ → ه
            '\u{0624}' => '\u{0648}',                                        // ؤ → و
            '\u{0626}' => '\u{064A}',                                        // ئ → ي
            other => other,
        })
        .collect()
}

/// Tatweel (U+0640), harakat (U+064B–U+065F, U+0670) and Quranic marks (U+06D6–U+06ED).
/// These are rejected inside keywords and identifiers (SEC0106).
pub fn is_tatweel_or_haraka(c: char) -> bool {
    matches!(c, '\u{0640}' | '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{06D6}'..='\u{06ED}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_skin_words_match_the_lexer_rule() {
        let t = KeywordTable::builtin();
        // Another skin's keyword or core name, canonical or variant (Franco aliases, Arabic
        // fold), is a cross-skin word; the file's own words and fixed-ASCII ones are not.
        let cross = |w: &str, s: Skin| t.cross_skin_word(w, s).map(|m| m.concept);
        assert_eq!(cross("law", Skin::En), Some(Concept::KwIf));
        assert_eq!(cross("lw", Skin::Ar), Some(Concept::KwIf));
        assert_eq!(cross("Some", Skin::Franco), Some(Concept::CoreSome));
        assert_eq!(
            cross("\u{0627}\u{0630}\u{0627}", Skin::En),
            Some(Concept::KwIf)
        );
        assert_eq!(cross("if", Skin::En), None);
        assert_eq!(cross("law", Skin::Franco), None);
        assert_eq!(cross("unsafe", Skin::Ar), None);
        assert_eq!(cross("x", Skin::Ar), None);
        // Reserved in some skin: any skin's own words (all kinds, variants included).
        assert!(t.reserved_in_some_skin("law"));
        assert!(t.reserved_in_some_skin("unsafe"));
        assert!(t.reserved_in_some_skin("None"));
        assert!(!t.reserved_in_some_skin("total"));
    }

    #[test]
    fn embedded_table_loads_with_expected_shape() {
        let table = KeywordTable::builtin();
        let count = |kind: Kind| table.rows().iter().filter(|r| r.kind == kind).count();
        assert_eq!(count(Kind::Keyword), 29);
        assert_eq!(count(Kind::AsciiFixed), 2);
        assert_eq!(count(Kind::CoreName), 19);
        assert_eq!(count(Kind::Punctuation), 3);
        assert_eq!(table.rows().len(), Concept::ALL.len());
    }

    #[test]
    fn lookups_per_skin() {
        let table = KeywordTable::builtin();
        let en = table.skin(Skin::En);
        let ar = table.skin(Skin::Ar);
        let franco = table.skin(Skin::Franco);

        assert_eq!(en.lookup_word("if").map(|m| m.concept), Some(Concept::KwIf));
        assert_eq!(en.lookup_word("law"), None); // other skins' words are identifiers

        let canonical = ar.lookup_word("إذا").unwrap();
        assert_eq!(canonical.concept, Concept::KwIf);
        assert_eq!(canonical.spelling, Spelling::Canonical);
        let folded = ar.lookup_word("اذا").unwrap(); // hamza-less form, matched by arabic_fold
        assert_eq!(folded.concept, Concept::KwIf);
        assert_eq!(folded.spelling, Spelling::Variant);

        let variant = franco.lookup_word("lw").unwrap();
        assert_eq!(variant.concept, Concept::KwIf);
        assert_eq!(variant.spelling, Spelling::Variant);
        assert_eq!(franco.lookup_word("5ali"), None); // never accepted
        assert_eq!(table.did_you_mean("5ali"), Some(Concept::KwLet));
    }

    #[test]
    fn core_names_and_fixed_ascii() {
        let table = KeywordTable::builtin();
        assert_eq!(
            table.skin(Skin::Ar).lookup_word("رئيسي").map(|m| m.concept),
            Some(Concept::CoreMain)
        );
        assert_eq!(
            table
                .skin(Skin::Franco)
                .lookup_word("unsafe")
                .map(|m| m.kind),
            Some(Kind::AsciiFixed)
        );
        assert_eq!(table.spelling(Concept::KwFn, Skin::Franco), "dala");
        assert_eq!(table.spelling(Concept::PunctComma, Skin::Ar), "\u{060C}");
    }

    #[test]
    fn arabic_fold_folds_persian_and_urdu_letters() {
        // DECISIONS.md D19: ی ک ە ھ ۀ fold to their Arabic equivalents.
        assert_eq!(arabic_fold("\u{06CC}"), "\u{064A}"); // ی → ي
        assert_eq!(arabic_fold("\u{06A9}"), "\u{0643}"); // ک → ك
        assert_eq!(arabic_fold("\u{06D5}"), "\u{0647}"); // ە → ه
        assert_eq!(arabic_fold("\u{06BE}"), "\u{0647}"); // ھ → ه
        assert_eq!(arabic_fold("\u{06C0}"), "\u{0647}"); // ۀ → ه
                                                         // Persian and Arabic spellings of one word share a fold key, so they match the
                                                         // same keyword and collide as identifier skeletons (SEC0104).
        assert_eq!(arabic_fold("\u{06A9}تاب"), arabic_fold("كتاب"));
        assert_eq!(arabic_fold("عل\u{06CC}"), arabic_fold("علي"));
        assert_eq!(arabic_fold("عل\u{06CC}"), arabic_fold("على"));
    }

    #[test]
    fn persian_letters_match_arabic_keywords_as_variants() {
        let table = KeywordTable::builtin();
        let ar = table.skin(Skin::Ar);
        let m = ar.lookup_word("ل\u{06A9}ل").unwrap(); // لکل with keheh
        assert_eq!(m.concept, Concept::KwFor);
        assert_eq!(m.spelling, Spelling::Variant);
        let m = ar.lookup_word("ف\u{06CC}").unwrap(); // فی with Farsi yeh
        assert_eq!(m.concept, Concept::KwIn);
        assert_eq!(m.spelling, Spelling::Variant);
        // Other skins are unaffected.
        assert_eq!(table.skin(Skin::Franco).lookup_word("ف\u{06CC}"), None);
    }

    #[test]
    fn integrity_rejects_digit_leading_franco() {
        let bad = KEYWORDS_JSON.replace("\"franco\": \"khali\"", "\"franco\": \"5ali\"");
        assert!(KeywordTable::load(&bad).is_err());
    }
}
