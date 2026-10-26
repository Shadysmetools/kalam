//! Code shown inside a diagnostic, spelled in the skin of the file it points into (spec
//! §21.4 rule 1, D21).
//!
//! The type checker prints types for messages (`List[Int]`, `fn(Int) -> Bool`). The words
//! of a type that differ between skins are the 11 core names that name types, the keyword
//! `fn` and the list comma (`،` in Arabic files). This module is the one place that knows
//! those spellings for a type printer: [`TypeSpelling`] is what `kalam_typeck::Ty`'s
//! display asks, so `Arg::Type(Ty::Int)` renders as `Int` in a `.kl` file and as `عدد` in a
//! `.ar.kl` file (ARCHITECTURE.md §12.1), exactly as `kalam convert` spells the annotation.
//!
//! Every other name in a type stays as the producer gives it: user types keep their declared
//! spelling (a type named `Int` in an Arabic file is legal, FOUNDER-QUEUE Q21, and is not a
//! core name there), and stdlib names that are not core names (`BigInt`, `Bytes`, `Secret`,
//! `Sql`, `Net`, …) are fixed ASCII in every skin (spec §7.2).

use kalam_base::Skin;
use kalam_skins::{Concept, KeywordTable};

/// The core names that name types (D21), by their English spelling.
const CORE_TYPES: &[(&str, Concept)] = &[
    ("Int", Concept::CoreInt),
    ("Float", Concept::CoreFloat),
    ("Decimal", Concept::CoreDecimal),
    ("Bool", Concept::CoreBool),
    ("String", Concept::CoreString),
    ("List", Concept::CoreList),
    ("Map", Concept::CoreMap),
    ("Set", Concept::CoreSet),
    ("Option", Concept::CoreOption),
    ("Result", Concept::CoreResult),
    ("World", Concept::CoreWorld),
];

/// The spelling of `concept` in `skin` (keywords, core names, punctuation).
pub fn spelling(concept: Concept, skin: Skin) -> &'static str {
    KeywordTable::builtin().spelling(concept, skin)
}

/// The spelling in `skin` of the core type name whose English spelling is `en` (`"Int"` →
/// `"عدد"` in `ar`, `"adad"` in `franco`). `None` when `en` is not one of the 11 core type
/// names.
pub fn core_type_name(en: &str, skin: Skin) -> Option<&'static str> {
    CORE_TYPES
        .iter()
        .find(|(name, _)| *name == en)
        .map(|(_, concept)| spelling(*concept, skin))
}

/// How a type printer spells the words of a type that differ between skins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeSpelling {
    pub skin: Skin,
}

impl TypeSpelling {
    pub fn new(skin: Skin) -> TypeSpelling {
        TypeSpelling { skin }
    }

    /// The English skin: what a printer with no file shows.
    pub fn english() -> TypeSpelling {
        TypeSpelling::new(Skin::En)
    }

    /// A core type name given in English (`"Option"`); any other name unchanged.
    pub fn name<'a>(&self, en: &'a str) -> &'a str {
        core_type_name(en, self.skin).unwrap_or(en)
    }

    /// The keyword that starts a function type (`fn`, `دالة`, `dala`).
    pub fn fn_keyword(&self) -> &'static str {
        spelling(Concept::KwFn, self.skin)
    }

    /// The separator between type arguments, tuple elements and parameters: `", "`, or
    /// `"، "` in Arabic files.
    pub fn separator(&self) -> String {
        format!("{} ", spelling(Concept::PunctComma, self.skin))
    }

    /// `items` joined with [`TypeSpelling::separator`].
    pub fn join(&self, items: &[String]) -> String {
        items.join(&self.separator())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny printer over a type tree, the way `kalam_typeck::Ty` uses the API.
    enum T {
        Named(&'static str, Vec<T>),
        Fn(Vec<T>, Box<T>),
    }

    fn show(t: &T, s: TypeSpelling) -> String {
        match t {
            T::Named(n, args) if args.is_empty() => s.name(n).to_string(),
            T::Named(n, args) => {
                let args: Vec<String> = args.iter().map(|a| show(a, s)).collect();
                format!("{}[{}]", s.name(n), s.join(&args))
            }
            T::Fn(ps, r) => {
                let ps: Vec<String> = ps.iter().map(|a| show(a, s)).collect();
                format!("{}({}) -> {}", s.fn_keyword(), s.join(&ps), show(r, s))
            }
        }
    }

    fn n(name: &'static str) -> T {
        T::Named(name, vec![])
    }

    #[test]
    fn core_type_names_follow_the_keyword_table() {
        assert_eq!(core_type_name("Int", Skin::En), Some("Int"));
        assert_eq!(core_type_name("Int", Skin::Ar), Some("عدد"));
        assert_eq!(core_type_name("Int", Skin::Franco), Some("adad"));
        assert_eq!(core_type_name("World", Skin::Ar), Some("عالم"));
        // Not core names: fixed ASCII in every skin (spec §7.2).
        for fixed in [
            "BigInt", "Bytes", "Secret", "Sql", "Net", "Ordering", "Point",
        ] {
            assert_eq!(core_type_name(fixed, Skin::Ar), None, "{fixed}");
        }
        // Constructors and functions are core names, but not type names.
        assert_eq!(core_type_name("Some", Skin::Ar), None);
        assert_eq!(core_type_name("print", Skin::Ar), None);
    }

    #[test]
    fn types_print_like_kalam_convert_prints_the_annotation() {
        // Oracle: `kalam convert --to ar|franco` of
        // `fn f(m: Map[String, Int], g: fn(Int) -> Bool, r: Result[List[Float], String])`.
        let map = T::Named("Map", vec![n("String"), n("Int")]);
        let func = T::Fn(vec![n("Int")], Box::new(n("Bool")));
        let res = T::Named(
            "Result",
            vec![T::Named("List", vec![n("Float")]), n("String")],
        );
        let ar = TypeSpelling::new(Skin::Ar);
        assert_eq!(show(&map, ar), "قاموس[نص، عدد]");
        assert_eq!(show(&func, ar), "دالة(عدد) -> منطقي");
        assert_eq!(show(&res, ar), "نتيجة[قائمة[كسري]، نص]");
        let fr = TypeSpelling::new(Skin::Franco);
        assert_eq!(show(&map, fr), "qamoos[nass, adad]");
        assert_eq!(show(&func, fr), "dala(adad) -> mante2i");
        assert_eq!(show(&res, fr), "natiga[qayma[kasri], nass]");
        let en = TypeSpelling::english();
        assert_eq!(show(&res, en), "Result[List[Float], String]");
        // A user type keeps its declared spelling: the printer only asks for core names.
        assert_eq!(
            show(&T::Named("Option", vec![n("Decimal")]), ar),
            "اختياري[عشري]"
        );
    }
}
