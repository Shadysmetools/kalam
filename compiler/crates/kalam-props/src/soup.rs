//! Hostile text for the "never panics" and "lossless CST" properties: random runs of the
//! characters and fragments that stress a Kalam lexer (bidi controls, ZWJ, tatweel, harakat,
//! BOM, NUL, CR/CRLF, tabs, Arabic punctuation and digits, keywords of every skin, unfinished
//! strings and escapes, deep brackets, odd indentation), and generated programs with such
//! fragments spliced in.

use kalam_base::Skin;
use kalam_skins::KeywordTable;

use crate::gen::Gen;
use crate::rng::{splitmix, Rng};

pub const HAZARDS: &[&str] = &[
    "\u{202A}",
    "\u{202B}",
    "\u{202C}",
    "\u{202D}",
    "\u{202E}",
    "\u{2066}",
    "\u{2067}",
    "\u{2068}",
    "\u{2069}",
    "\u{200E}",
    "\u{200F}",
    "\u{061C}",
    "\u{200D}",
    "\u{200C}",
    "\u{200B}",
    "\u{FEFF}",
    "\u{0640}",
    "\u{064B}",
    "\u{0651}",
    "\u{0652}",
    "\u{0670}",
    "\u{06D6}",
    "\u{0000}",
    "\u{FFFD}",
    "\u{0301}",
    "\u{0430}",
    "\u{00A0}",
    "\u{3000}",
    "\u{2028}",
    "\u{2029}",
    "\u{000B}",
    "\u{000C}",
    "\u{0085}",
    "\u{1F600}",
    "\u{FB50}",
    "\u{FE8D}",
    "\u{06CC}",
    "\u{06A9}",
    "\u{06D5}",
    "\u{06BE}",
    "\u{06C0}",
    "\u{0660}",
    "\u{0669}",
    "\u{06F0}",
    "\u{06F9}",
    "\u{066B}",
    "\u{066C}",
    "\u{060C}",
    "\u{061F}",
    "\u{061B}",
    "\u{066A}",
];

pub const FRAGMENTS: &[&str] = &[
    "\"",
    "\"\"\"",
    "\\",
    "\\u{",
    "\\u{10FFFF}",
    "\\u{D800}",
    "\\q",
    "{",
    "}",
    "`",
    "``",
    "#",
    "##",
    "@",
    "@sync",
    "..",
    "..=",
    "...",
    "=>",
    "->",
    "//",
    "+=",
    "==",
    "!=",
    "?",
    ":",
    ";",
    "0x",
    "0b",
    "1e",
    "1e+",
    "1.",
    ".5",
    "1_",
    "__",
    "_",
    "99999999999999999999999999999999999",
    "(",
    ")",
    "[",
    "]",
    ",",
    ".",
    "-",
    "html\"",
    "sql\"",
    "\n",
    "\r\n",
    "\r",
    "\t",
    "    ",
    "  ",
    "\n    ",
    "\n        ",
    "\n  ",
    " ",
    "x",
    "abc",
    "قيمة",
    "اسم",
    "سلام",
];

fn keyword(rng: &mut Rng) -> String {
    let table = KeywordTable::builtin();
    let row = rng.pick(table.rows());
    match rng.below(4) {
        0 => row.en.clone(),
        1 => row.ar.clone(),
        2 => row.franco.clone(),
        _ => row
            .franco_aliases
            .first()
            .cloned()
            .unwrap_or_else(|| row.franco.clone()),
    }
}

fn piece(rng: &mut Rng) -> String {
    match rng.below(10) {
        0 | 1 => rng.pick(HAZARDS).to_string(),
        2..=4 => rng.pick(FRAGMENTS).to_string(),
        5 | 6 => keyword(rng),
        7 => {
            let c = *rng.pick(&["(", "[", "{", "\"{", "(\n"]);
            c.repeat(1 + rng.below(90))
        }
        8 => char::from_u32(rng.below(0x7F) as u32)
            .unwrap_or('?')
            .to_string(),
        _ => char::from_u32(rng.below(0x3_0000) as u32)
            .unwrap_or('\u{FFFD}')
            .to_string(),
    }
}

/// One hostile input for `case_seed`. About half are generated programs with a few
/// fragments spliced in (to reach deeper parser states), half are pure fragment soup.
pub fn soup(case_seed: u64) -> String {
    let mut rng = Rng::new(splitmix(case_seed ^ 0x50_0B));
    if rng.chance(50) {
        let ast = Gen::new(case_seed).program();
        let skin = Skin::ALL[rng.below(3)];
        let mut text = crate::print::print(&ast, skin, rng.next_u64(), rng.next_u64());
        for _ in 0..1 + rng.below(4) {
            let p = piece(&mut rng);
            let mut at = rng.below(text.len() + 1);
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            if rng.chance(30) {
                // Replace a short run instead of inserting.
                let mut end = (at + 1 + rng.below(8)).min(text.len());
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                text.replace_range(at..end, &p);
            } else {
                text.insert_str(at, &p);
            }
        }
        text
    } else {
        let n = rng.below(120);
        (0..n).map(|_| piece(&mut rng)).collect()
    }
}
