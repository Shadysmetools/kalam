//! Identifier skeletons (spec §5.3, DR-04):
//! `skeleton(id) = UTS39_skeleton(arabic_fold(NFKC(id)))`.
//!
//! The per-code-point pieces come from a vendored table generated from UTS #39
//! `confusables.txt` and Python `unicodedata` (`tools/gen_skeleton_table.py`, see its header
//! for versions and the one documented approximation). Identifiers are NFC when they reach
//! the resolver (SEC0101), so the skeleton is the concatenation of each code point's piece;
//! Hangul syllables are decomposed arithmetically (UAX #15) and each jamo is looked up.
//!
//! Skeletons are a comparison key only. Identifiers are never rewritten, and a use is never
//! resolved through its skeleton: a skeleton match only produces SEC0104 (two declarations)
//! or the machine fix of K0401 (a use spelled differently from its declaration).

pub use crate::skeleton_table::CONFUSABLES_VERSION;
use crate::skeleton_table::PIECES;

const S_BASE: u32 = 0xAC00;
const L_BASE: u32 = 0x1100;
const V_BASE: u32 = 0x1161;
const T_BASE: u32 = 0x11A7;
const V_COUNT: u32 = 21;
const T_COUNT: u32 = 28;
const N_COUNT: u32 = V_COUNT * T_COUNT;
const S_COUNT: u32 = 19 * N_COUNT;

fn piece_of(c: char, out: &mut String) {
    match PIECES.binary_search_by_key(&(c as u32), |&(cp, _)| cp) {
        Ok(i) => out.push_str(PIECES[i].1),
        Err(_) => out.push(c),
    }
}

/// The skeleton of an identifier (spec §5.3). Two distinct identifiers with equal skeletons
/// look the same to a reader.
pub fn skeleton(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for c in id.chars() {
        let s = (c as u32).wrapping_sub(S_BASE);
        if s < S_COUNT {
            let jamo = [
                L_BASE + s / N_COUNT,
                V_BASE + (s % N_COUNT) / T_COUNT,
                T_BASE + s % T_COUNT,
            ];
            for (k, cp) in jamo.into_iter().enumerate() {
                if k == 2 && cp == T_BASE {
                    break; // no trailing consonant
                }
                if let Some(j) = char::from_u32(cp) {
                    piece_of(j, &mut out);
                }
            }
        } else {
            piece_of(c, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &str, b: &str) -> bool {
        skeleton(a) == skeleton(b)
    }

    #[test]
    fn table_is_sorted_and_versioned() {
        assert!(PIECES.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(CONFUSABLES_VERSION, "16.0.0");
    }

    #[test]
    fn suite_pairs_collide() {
        // eval/suites trojan-source [M2] cases (spec §5.3 examples, D19 folds).
        assert!(same("على", "علي")); // alef maqsura vs yeh
        assert!(same("كتاب", "کتاب")); // kaf vs keheh
        assert!(same("كرسي", "کرسی")); // keheh + Farsi yeh
        assert!(same("مدرسة", "مدرسه")); // teh marbuta vs heh
        assert!(same("مدرسه", "مدرسە")); // heh vs ae
        assert!(same("أحمد", "احمد")); // hamza forms
        assert!(same("al", "a1")); // l vs 1
        assert!(same("ax", "\u{0430}\u{0445}")); // Latin vs Cyrillic
        assert!(same("pay", "\u{0440}\u{0430}\u{0443}"));
        assert!(same("Int", "lnt")); // I vs l
    }

    #[test]
    fn case_and_ordinary_names_stay_distinct() {
        // Spec §13.3: case distinguishes names in English, so these are three names.
        assert!(!same("Shape", "shape"));
        assert!(!same("Item", "item"));
        assert!(!same("World", "world"));
        assert!(!same("total", "totals"));
        assert!(!same("شكل", "الشكل"));
        assert!(!same("ب", "ت"));
    }

    #[test]
    fn arabic_fold_matches_kalam_skins() {
        // The generator copies `arabic_fold`; every folded letter must have the skeleton of
        // its fold target, exactly as kalam-skins folds it.
        for c in "أإآٱىیکةەھۀؤئ".chars() {
            let s = c.to_string();
            let folded = kalam_skins::arabic_fold(&s);
            assert_ne!(folded, s, "{c} folds");
            assert_eq!(skeleton(&s), skeleton(&folded), "{c}");
        }
    }

    #[test]
    fn hangul_syllables_decompose() {
        // 한 = ᄒ ᅡ ᆫ; 하 = ᄒ ᅡ (no trailing consonant).
        assert_eq!(skeleton("하"), skeleton("\u{1112}\u{1161}"));
        assert_eq!(skeleton("한"), skeleton("\u{1112}\u{1161}\u{11AB}"));
        assert!(!same("하", "한"));
    }
}
