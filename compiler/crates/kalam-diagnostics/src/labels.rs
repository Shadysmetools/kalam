//! Label texts in the message language.
//!
//! A [`crate::Label`] carries its text in English, written by the stage that found the
//! problem (resolve, the type checker, suspension inference); JSON output keeps that text
//! as it is. The human renderer shows a label in the message language (`KALAM_LANG`), so it
//! looks the English text up here, gettext-style: the English text is the key.
//!
//! - [`TEMPLATES`] are whole labels. `{0}`, `{1}`, … stand for code the producer quoted in
//!   backticks (a name, a type in the file's skin), copied unchanged. `{kw_*}` and `{core_*}`
//!   placeholders are keywords and core names: the English key spells them in the English
//!   skin (that is what the producer writes), the rendering spells them in the file's skin.
//! - [`FRAGMENTS`] translate labels that are built from parts, such as the call chain of
//!   K1401/K1402 ("`f` calls `g`, which calls `http.get` (suspends)"): every prose piece
//!   between two code spans is looked up on its own, and the code spans stay in place.
//!
//! A label that matches neither is shown only when the message language is English (see
//! [`localize`]), so an Arabic or Franco message never mixes in English prose.

use kalam_base::Skin;

use crate::catalog::{render_in_skin, render_template};
use crate::Lang;

/// Whole-label templates: `[en, ar, franco]`.
pub const TEMPLATES: &[[&str; 3]] = &[
    // kalam-resolve
    [
        "`{0}` is declared here",
        "هنا عُرِّف `{0}`",
        "`{0}` et3arraf hena",
    ],
    [
        "`{0}` is already declared here",
        "هنا عُرِّف `{0}` أول مرة",
        "`{0}` et3arraf hena el awel",
    ],
    [
        "`{0}` is declared here without `{kw_export}`",
        "هنا عُرِّف `{0}` بلا `{kw_export}`",
        "`{0}` et3arraf hena men gheir `{kw_export}`",
    ],
    // kalam-typeck: the label every K0600 gets, naming the types in the file's skin.
    [
        "expected `{0}`, found `{1}`",
        "المطلوب `{0}`، والموجود `{1}`",
        "el matloob `{0}`, wel mawgood `{1}`",
    ],
    [
        "this would need `{0}` to contain itself",
        "هذا يتطلب أن يحتوي `{0}` على نفسه",
        "da hayetlob en `{0}` ykoon gowah nafsoh",
    ],
    [
        "`{0}` is not a function",
        "`{0}` ليس دالة",
        "`{0}` mesh dala",
    ],
    [
        "a `{kw_for}` loop can't go over a `{0}`",
        "لا تستطيع حلقة `{kw_for}` المرور على `{0}`",
        "el loop `{kw_for}` mate2darsh tlef 3ala `{0}`",
    ],
    // kalam-typeck well-formedness (`wf.rs`)
    [
        "a `{core_map}` key must be `Eq` and `Hash`",
        "مفتاح `{core_map}` لا بد أن يكون `Eq` و`Hash`",
        "el key bta3 `{core_map}` lazem ykoon `Eq` w `Hash`",
    ],
    [
        "a `{core_set}` element must be `Eq` and `Hash`",
        "عنصر `{core_set}` لا بد أن يكون `Eq` و`Hash`",
        "el 3onsor bta3 `{core_set}` lazem ykoon `Eq` w `Hash`",
    ],
    [
        "`{0}` requires its `{1}` to be `{2}`",
        "`{0}` يشترط أن يكون `{1}` فيه `{2}`",
        "`{0}` byeshtarat en `{1}` bta3o ykoon `{2}`",
    ],
    // kalam-typeck constant expressions (K1204, `consts.rs`)
    [
        "a function call is not a constant expression",
        "استدعاء الدالة ليس تعبيرًا ثابتًا",
        "el call bta3 el dala mesh ta3beer sabet",
    ],
    [
        "a method call is not a constant expression",
        "استدعاء دالة على قيمة ليس تعبيرًا ثابتًا",
        "el call bta3 el method mesh ta3beer sabet",
    ],
    [
        "indexing is not a constant expression",
        "الفهرسة ليست تعبيرًا ثابتًا",
        "el indexing mesh ta3beer sabet",
    ],
    [
        "`{punct_try}` is not a constant expression",
        "`{punct_try}` ليست تعبيرًا ثابتًا",
        "`{punct_try}` mesh ta3beer sabet",
    ],
    [
        "`{kw_if}` is not a constant expression",
        "`{kw_if}` ليست تعبيرًا ثابتًا",
        "`{kw_if}` mesh ta3beer sabet",
    ],
    [
        "`{kw_match}` is not a constant expression",
        "`{kw_match}` ليست تعبيرًا ثابتًا",
        "`{kw_match}` mesh ta3beer sabet",
    ],
    [
        "an `unsafe` call is not a constant expression",
        "استدعاء `unsafe` ليس تعبيرًا ثابتًا",
        "el call `unsafe` mesh ta3beer sabet",
    ],
    [
        "a local value is not a constant expression",
        "القيمة المحلية ليست تعبيرًا ثابتًا",
        "el qeema el local mesh ta3beer sabet",
    ],
    [
        "a constant can't panic",
        "لا يجوز أن ينهار الثابت",
        "el sabet mayenfa3sh yo2a3",
    ],
    [
        "a constant can interpolate only numbers, `{core_bool}` and `{core_string}` values",
        "لا يُدرج الثابت في النص إلا الأرقام وقيم `{core_bool}` و`{core_string}`",
        "el sabet mayda5alsh f el nass gheir el arqam w qeyam `{core_bool}` w `{core_string}`",
    ],
    [
        "this panics: integer overflow",
        "هذا ينهار: تجاوز لسعة العدد الصحيح",
        "da beyo2a3: overflow f el rakam el sa7ee7",
    ],
    [
        "this panics: division by zero",
        "هذا ينهار: قسمة على صفر",
        "da beyo2a3: 2esma 3ala sefr",
    ],
    [
        "a module constant can't hold the capability `{0}`",
        "لا يحمل ثابت الوحدة الصلاحية `{0}`",
        "sabet el module mayshelsh el capability `{0}`",
    ],
    [
        "a module constant can't hold a `Cell`: create it in `{core_main}` and pass it down",
        "لا يحمل ثابت الوحدة `Cell`: أنشئها في `{core_main}` ومرّرها",
        "sabet el module mayshelsh `Cell`: e3melha f `{core_main}` w ba3atha",
    ],
];

/// Prose pieces of labels built from parts: `[en, ar, franco]`. A piece is the whole text
/// between two code spans (or before the first, or after the last).
pub const FRAGMENTS: &[[&str; 3]] = &[
    // kalam-effects call chains (K1401, K1402): `render_chain` in suspension.rs.
    [" calls ", " تستدعي ", " betnady "],
    [", which calls ", "، التي تستدعي ", ", elly betnady "],
    [" (suspends)", " (وهي تتعلق)", " (w di bte3mel suspend)"],
    [
        "this function calls ",
        "هذه الدالة تستدعي ",
        "el dala di betnady ",
    ],
    [
        " calls a function value stored in data (suspending unless its type is ",
        " تستدعي دالة مخزنة في بيانات (تتعلق ما لم يكن نوعها ",
        " betnady dala metkhazzena f data (bte3mel suspend ella law no3ha ",
    ],
    [
        ", which calls a function value stored in data (suspending unless its type is ",
        "، التي تستدعي دالة مخزنة في بيانات (تتعلق ما لم يكن نوعها ",
        ", elly betnady dala metkhazzena f data (bte3mel suspend ella law no3ha ",
    ],
    [
        "this function calls a function value stored in data (suspending unless its type is ",
        "هذه الدالة تستدعي دالة مخزنة في بيانات (تتعلق ما لم يكن نوعها ",
        "el dala di betnady dala metkhazzena f data (bte3mel suspend ella law no3ha ",
    ],
    [")", ")", ")"],
];

/// `text` (a label's English text) in message language `lang`, with keywords and core names
/// in `skin`, the file's skin. `None` when no template or fragment set covers it and `lang`
/// is not English: the renderer then leaves the label out rather than mix languages.
pub fn localize(text: &str, lang: Lang, skin: Skin) -> Option<String> {
    for t in TEMPLATES {
        let key = render_in_skin(t[0], &[], Skin::En);
        if let Some(holes) = match_template(&key, text) {
            let localized = render_in_skin(t[lang.index()], &[], skin);
            let args: Vec<(String, String)> = holes
                .into_iter()
                .enumerate()
                .map(|(i, h)| (i.to_string(), h))
                .collect();
            let args: Vec<(&str, String)> =
                args.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
            return Some(render_template(&localized, &args));
        }
    }
    if lang == Skin::En {
        return Some(text.to_string());
    }
    by_fragments(text, lang)
}

/// The holes of `key` (`{0}`, `{1}`, …, in order of appearance) as matched in `text`, or
/// `None` if `text` does not have the key's shape. A hole takes the shortest text up to
/// the literal that follows it.
fn match_template(key: &str, text: &str) -> Option<Vec<String>> {
    let mut holes = Vec::new();
    let mut rest_key = key;
    let mut rest = text;
    loop {
        let Some(open) = hole_start(rest_key) else {
            return (rest == rest_key).then_some(holes);
        };
        let literal = &rest_key[..open];
        rest = rest.strip_prefix(literal)?;
        let close = open + rest_key[open..].find('}')?;
        rest_key = &rest_key[close + 1..];
        let next = hole_start(rest_key).map_or(rest_key, |o| &rest_key[..o]);
        let end = if next.is_empty() {
            rest.len()
        } else {
            rest.find(next)?
        };
        if end == 0 {
            return None;
        }
        holes.push(rest[..end].to_string());
        rest = &rest[end..];
    }
}

/// Byte offset of the next `{<digits>}` hole in `s`.
fn hole_start(s: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = s[from..].find('{') {
        let at = from + i;
        let after = &s[at + 1..];
        if let Some(close) = after.find('}') {
            if close > 0 && after[..close].bytes().all(|b| b.is_ascii_digit()) {
                return Some(at);
            }
        }
        from = at + 1;
    }
    None
}

/// Every prose piece of `text` translated through [`FRAGMENTS`], code spans unchanged.
fn by_fragments(text: &str, lang: Lang) -> Option<String> {
    let mut out = String::new();
    let mut rest = text;
    let mut in_code = false;
    loop {
        let (piece, more) = match rest.find('`') {
            Some(i) => (&rest[..i], Some(&rest[i + 1..])),
            None => (rest, None),
        };
        if in_code {
            out.push_str(piece);
        } else if !piece.is_empty() {
            let f = FRAGMENTS.iter().find(|f| f[0] == piece)?;
            out.push_str(f[lang.index()]);
        }
        match more {
            Some(m) => {
                out.push('`');
                rest = m;
                in_code = !in_code;
            }
            None => break,
        }
    }
    (!in_code).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_translate_and_keep_the_code() {
        assert_eq!(
            localize("`total` is already declared here", Skin::Ar, Skin::Ar).as_deref(),
            Some("هنا عُرِّف `total` أول مرة")
        );
        assert_eq!(
            localize("expected `عدد`, found `نص`", Skin::Franco, Skin::Ar).as_deref(),
            Some("el matloob `عدد`, wel mawgood `نص`")
        );
        // English text of an English-file label comes back unchanged.
        let en = "`Ticket` is declared here without `export`";
        assert_eq!(localize(en, Skin::En, Skin::En).as_deref(), Some(en));
    }

    #[test]
    fn keywords_and_core_names_follow_the_file_skin() {
        // The producer writes `export`/`Map` in English; an Arabic file shows its own words,
        // in every message language.
        assert_eq!(
            localize("`T` is declared here without `export`", Skin::En, Skin::Ar).as_deref(),
            Some("`T` is declared here without `علني`")
        );
        assert_eq!(
            localize(
                "a `Map` key must be `Eq` and `Hash`",
                Skin::En,
                Skin::Franco
            )
            .as_deref(),
            Some("a `qamoos` key must be `Eq` and `Hash`")
        );
        assert_eq!(
            localize("a `for` loop can't go over a `عدد`", Skin::Ar, Skin::Ar).as_deref(),
            Some("لا تستطيع حلقة `لكل` المرور على `عدد`")
        );
    }

    #[test]
    fn punctuation_and_keywords_in_keys_match_the_english_text() {
        // The producer writes `?` and `if` in English; Arabic files show `؟` and `إذا`.
        assert_eq!(
            localize("`?` is not a constant expression", Skin::Ar, Skin::Ar).as_deref(),
            Some("`؟` ليست تعبيرًا ثابتًا")
        );
        assert_eq!(
            localize("`if` is not a constant expression", Skin::En, Skin::Franco).as_deref(),
            Some("`law` is not a constant expression")
        );
    }

    #[test]
    fn call_chains_translate_piece_by_piece() {
        let chain = "`tick` calls `load`, which calls `http.get` (suspends)";
        assert_eq!(
            localize(chain, Skin::Ar, Skin::Ar).as_deref(),
            Some("`tick` تستدعي `load`، التي تستدعي `http.get` (وهي تتعلق)")
        );
        let stored = "this function calls a function value stored in data (suspending unless its type is `@sync`)";
        assert_eq!(
            localize(stored, Skin::Franco, Skin::Franco).as_deref(),
            Some("el dala di betnady dala metkhazzena f data (bte3mel suspend ella law no3ha `@sync`)")
        );
    }

    #[test]
    fn unknown_labels_are_english_only() {
        let odd = "`x` was moved here";
        assert_eq!(localize(odd, Skin::En, Skin::Ar).as_deref(), Some(odd));
        assert_eq!(localize(odd, Skin::Ar, Skin::Ar), None);
        assert_eq!(localize(odd, Skin::Franco, Skin::En), None);
    }

    #[test]
    fn every_template_has_the_same_holes_in_every_language() {
        for t in TEMPLATES {
            let holes = |s: &str| {
                let mut h: Vec<String> = (0..4)
                    .filter(|i| s.contains(&format!("{{{i}}}")))
                    .map(|i| i.to_string())
                    .collect();
                h.extend(
                    crate::catalog::placeholders(s)
                        .into_iter()
                        .map(str::to_string),
                );
                h.sort();
                h
            };
            assert_eq!(holes(t[0]), holes(t[1]), "{t:?}");
            assert_eq!(holes(t[0]), holes(t[2]), "{t:?}");
        }
        for f in FRAGMENTS {
            assert!(f.iter().all(|s| !s.contains('`')), "{f:?}");
        }
    }
}
