//! Mutations: byte/char level, token level (from the real lexer's spans) and grammar aware
//! (generated program fragments, deep nesting, re-skinned keywords, Arabic glyph swaps).
//! Every mutation keeps the text valid UTF-8, because the CLI rejects anything else before
//! the lexer runs.

use kalam_base::{FileId, Skin, Span};
use kalam_props::rng::Rng;
use kalam_props::soup::{FRAGMENTS, HAZARDS};
use kalam_skins::KeywordTable;

/// One input: text and the skin it is read in.
#[derive(Clone, Debug)]
pub struct Input {
    pub skin: Skin,
    pub text: String,
}

/// Inputs stay below this size (the target is linear-ish; big inputs only slow it down).
pub const MAX_LEN: usize = 48 * 1024;

const OPS: &[&str] = &[
    "+", "-", "*", "/", "//", "%", "==", "!=", "<", "<=", ">", ">=", "=", "+=", "->", "=>", "..",
    "..=", "?", ".", ",", ":", "(", ")", "[", "]", "{", "}", "|", "@", "`",
];

fn boundary_at_or_before(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn pos(rng: &mut Rng, s: &str) -> usize {
    boundary_at_or_before(s, rng.below(s.len() + 1))
}

fn range(rng: &mut Rng, s: &str, max: usize) -> (usize, usize) {
    let a = pos(rng, s);
    let b = boundary_at_or_before(s, a + 1 + rng.below(max.max(1)));
    (a, b.max(a))
}

fn keyword(rng: &mut Rng) -> String {
    let table = KeywordTable::builtin();
    let row = rng.pick(table.rows());
    let mut options = vec![row.en.clone(), row.ar.clone(), row.franco.clone()];
    options.extend(row.franco_aliases.iter().cloned());
    options.extend(row.ar_also_accepts.iter().cloned());
    rng.pick(&options).clone()
}

fn line_spans(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        if b == b'\n' {
            out.push((start, i + 1));
            start = i + 1;
        }
    }
    if start < s.len() {
        out.push((start, s.len()));
    }
    out
}

fn token_spans(s: &str, skin: Skin) -> Vec<Span> {
    kalam_lexer::lex(s, skin, FileId(0))
        .tokens
        .iter()
        .map(|t| t.span)
        .filter(|sp| sp.end > sp.start && (sp.end as usize) <= s.len())
        .filter(|sp| s.is_char_boundary(sp.start as usize) && s.is_char_boundary(sp.end as usize))
        .collect()
}

fn random_token(rng: &mut Rng) -> String {
    match rng.below(6) {
        0 | 1 => keyword(rng),
        2 => rng.pick(OPS).to_string(),
        3 => rng
            .pick(&["x", "قيمة", "`law`", "_", "a1", "Shape", "ليكن"])
            .to_string(),
        4 => rng
            .pick(&[
                "0",
                "1_000",
                "0x1F",
                "١٢",
                "1.5",
                "١٫٥",
                "2e3",
                "99999999999999999999999",
            ])
            .to_string(),
        _ => rng.pick(HAZARDS).to_string(),
    }
}

/// One mutation of `text` (in place). `other` is another corpus entry for splicing.
fn mutate_once(rng: &mut Rng, text: &mut String, skin: Skin, other: &str) {
    if text.is_empty() {
        text.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
        return;
    }
    match rng.below(20) {
        // ----- character level
        0 => {
            let p = pos(rng, text);
            let h = *rng.pick(HAZARDS);
            text.insert_str(p, h);
        }
        1 => {
            let p = pos(rng, text);
            let f = *rng.pick(FRAGMENTS);
            text.insert_str(p, f);
        }
        2 => {
            let (a, b) = range(rng, text, 16);
            text.replace_range(a..b, "");
        }
        3 => {
            let (a, b) = range(rng, text, 64);
            let chunk = text[a..b].to_string();
            let p = pos(rng, text);
            text.insert_str(p, &chunk);
        }
        4 => {
            // Flip low bits of an ASCII byte (stays ASCII, so UTF-8 stays valid).
            let p = rng.below(text.len());
            let b = text.as_bytes()[p];
            if b.is_ascii() {
                let nb = (b ^ (1 << rng.below(7))) & 0x7F;
                let mut bytes = std::mem::take(text).into_bytes();
                bytes[p] = nb;
                *text = String::from_utf8(bytes).unwrap_or_default();
            }
        }
        // ----- line level
        5 | 6 => {
            let lines = line_spans(text);
            if lines.is_empty() {
                return;
            }
            let (a, b) = lines[rng.below(lines.len())];
            match rng.below(5) {
                0 => text.replace_range(a..b, ""),
                1 => {
                    let l = text[a..b].to_string();
                    text.insert_str(a, &l);
                }
                2 => text.insert_str(a, rng.pick(&["    ", " ", "\t", "        "])),
                3 => {
                    let n = text[a..b].len() - text[a..b].trim_start_matches(' ').len();
                    let cut = n.min(1 + rng.below(4));
                    text.replace_range(a..a + cut, "");
                }
                _ => {
                    let (c, d) = lines[rng.below(lines.len())];
                    if d <= a || c >= b {
                        let (l1, l2) = (text[a..b].to_string(), text[c..d].to_string());
                        if a < c {
                            text.replace_range(c..d, &l1);
                            text.replace_range(a..b, &l2);
                        } else {
                            text.replace_range(a..b, &l2);
                            text.replace_range(c..d, &l1);
                        }
                    }
                }
            }
        }
        7 => {
            let lines = line_spans(text);
            let (a, _) = lines[rng.below(lines.len())];
            let mut eol = "\r\n";
            if rng.chance(50) {
                eol = "\r";
            }
            if a > 0 && text.as_bytes()[a - 1] == b'\n' {
                text.replace_range(a - 1..a, eol);
            }
        }
        // ----- token level
        8..=11 => {
            let spans = token_spans(text, skin);
            if spans.is_empty() {
                return;
            }
            let sp = spans[rng.below(spans.len())];
            let (a, b) = (sp.start as usize, sp.end as usize);
            match rng.below(4) {
                0 => {
                    let t = random_token(rng);
                    text.replace_range(a..b, &t);
                }
                1 => text.replace_range(a..b, ""),
                2 => {
                    let t = text[a..b].to_string();
                    text.insert_str(b, &format!(" {t}"));
                }
                _ => {
                    let sp2 = spans[rng.below(spans.len())];
                    let (c, d) = (sp2.start as usize, sp2.end as usize);
                    if d <= a || c >= b {
                        let (t1, t2) = (text[a..b].to_string(), text[c..d].to_string());
                        if a < c {
                            text.replace_range(c..d, &t1);
                            text.replace_range(a..b, &t2);
                        } else {
                            text.replace_range(a..b, &t2);
                            text.replace_range(c..d, &t1);
                        }
                    }
                }
            }
        }
        // ----- grammar aware
        12 => {
            // Splice a chunk of another input at a line start.
            if other.is_empty() {
                return;
            }
            let (a, b) = range(rng, other, 400);
            let chunk = other[a..b].to_string();
            let lines = line_spans(text);
            let at = lines.get(rng.below(lines.len())).map(|l| l.0).unwrap_or(0);
            text.insert_str(at, &chunk);
        }
        13 => {
            // A generated program's lines, indented like the target line.
            let gen = kalam_props::generated_program(rng.next_u64(), skin);
            let lines = line_spans(text);
            let (a, b) = lines[rng.below(lines.len())];
            let indent = text[a..b].len() - text[a..b].trim_start_matches(' ').len();
            let pad = " ".repeat(indent);
            let block: String = gen
                .lines()
                .take(1 + rng.below(12))
                .map(|l| format!("{pad}{l}\n"))
                .collect();
            text.insert_str(a, &block);
        }
        14 => {
            // Deep nesting around a token or at a position.
            let n = 1 + {
                let big = rng.chance(20);
                rng.below(if big { 400 } else { 80 })
            };
            let (open, close) =
                *rng.pick(&[("(", ")"), ("[", "]"), ("{", "}"), ("-", ""), ("not ", "")]);
            let (a, b) = range(rng, text, 12);
            let inner = text[a..b].to_string();
            let wrapped = format!("{}{inner}{}", open.repeat(n), close.repeat(n));
            text.replace_range(a..b, &wrapped);
        }
        15 => {
            // Deeply nested blocks.
            let depth = 1 + {
                let big = rng.chance(20);
                rng.below(if big { 200 } else { 40 })
            };
            let kw = KeywordTable::builtin().spelling(kalam_skins::Concept::KwIf, skin);
            let mut s = String::new();
            for d in 0..depth {
                s.push_str(&format!("{}{kw} x:\n", " ".repeat(4 * (d + 1))));
            }
            s.push_str(&format!("{}x()\n", " ".repeat(4 * (depth + 1))));
            let lines = line_spans(text);
            let at = lines.get(rng.below(lines.len())).map(|l| l.1).unwrap_or(0);
            text.insert_str(at, &s);
        }
        16 => {
            // Long chains: `a.b.c…`, `f()()…`, `1 + 1 + …`.
            let n = 1 + {
                let big = rng.chance(20);
                rng.below(if big { 3000 } else { 200 })
            };
            let unit = *rng.pick(&[".x", "()", " + 1", "?", "[0]", " and y", " or z"]);
            let p = pos(rng, text);
            text.insert_str(p, &format!("x{}", unit.repeat(n)));
        }
        17 => {
            // Re-skin: swap ASCII punctuation and digits for Arabic glyphs and back.
            let swaps: &[(&str, &str)] = &[
                (",", "\u{060C}"),
                ("?", "\u{061F}"),
                ("1", "\u{0661}"),
                ("0", "\u{0660}"),
                (".", "\u{066B}"),
                ("\u{060C}", ","),
                ("\u{0660}", "0"),
            ];
            let (from, to) = *rng.pick(swaps);
            if let Some(i) = text.match_indices(from).map(|(i, _)| i).nth(rng.below(8)) {
                text.replace_range(i..i + from.len(), to);
            }
        }
        18 => {
            // Unfinished or odd strings.
            let p = pos(rng, text);
            let s = *rng.pick(&[
                "\"{",
                "\"\"\"\n",
                "\"{\"}\"",
                "\"\\u{110000}\"",
                "\"{x{y}}\"",
                "html\"<a href={u}>\"",
                "\"\\",
                "\"{fn(x) => x}\"",
            ]);
            text.insert_str(p, s);
        }
        _ => {
            let p = pos(rng, text);
            let t = random_token(rng);
            text.insert_str(p, &format!(" {t} "));
        }
    }
}

/// A stacked ("havoc") mutation of `base`.
pub fn mutate(rng: &mut Rng, base: &Input, other: &Input) -> Input {
    let mut text = base.text.clone();
    let rounds = 1 + {
        let big = rng.chance(20);
        rng.below(if big { 16 } else { 4 })
    };
    for _ in 0..rounds {
        mutate_once(rng, &mut text, base.skin, &other.text);
        if text.len() > MAX_LEN {
            let cut = boundary_at_or_before(&text, MAX_LEN);
            text.truncate(cut);
        }
    }
    let skin = if rng.chance(8) {
        *rng.pick(&Skin::ALL)
    } else {
        base.skin
    };
    Input { skin, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutations_keep_utf8_and_size() {
        let mut rng = Rng::new(9);
        let base = Input {
            skin: Skin::Ar,
            text: "دالة رئيسي():\n    اطبع(\"سلام\"، ١٢)\n".into(),
        };
        let other = Input {
            skin: Skin::En,
            text: "fn main():\n    print(1)\n".into(),
        };
        let mut cur = base.clone();
        for _ in 0..3000 {
            cur = mutate(&mut rng, &cur, &other);
            assert!(cur.text.len() <= MAX_LEN);
            if cur.text.len() > 8000 {
                cur = base.clone();
            }
        }
    }
}
