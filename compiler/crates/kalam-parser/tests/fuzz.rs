//! Fuzz-safety property: the parser never panics, on arbitrary token streams (including
//! bogus spans, unbalanced INDENT/DEDENT and no EOF) and on mutated real programs.
//!
//! `proptest` is not available offline, so this is a deterministic xorshift generator with
//! fixed seeds; a failure prints the seed and the input so it can be turned into a test.

mod common;

use std::panic::{catch_unwind, AssertUnwindSafe};

use common::*;
use kalam_base::{FileId, Skin, Span};
use kalam_lexer::{LexOutput, Op, Token, TokenFlags, TokenKind, Trivia, TriviaKind};
use kalam_parser::parse;
use kalam_skins::Concept;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

const OPS: &[Op] = &[
    Op::Plus,
    Op::Minus,
    Op::Star,
    Op::Slash,
    Op::SlashSlash,
    Op::Percent,
    Op::EqEq,
    Op::NotEq,
    Op::Lt,
    Op::Le,
    Op::Gt,
    Op::Ge,
    Op::Eq,
    Op::PlusEq,
    Op::MinusEq,
    Op::StarEq,
    Op::SlashEq,
    Op::Arrow,
    Op::FatArrow,
    Op::DotDot,
    Op::DotDotEq,
    Op::Question,
    Op::Dot,
    Op::Comma,
    Op::Colon,
    Op::LParen,
    Op::RParen,
    Op::LBracket,
    Op::RBracket,
    Op::LBrace,
    Op::RBrace,
    Op::At,
    Op::Pipe,
];

fn random_kind(rng: &mut Rng) -> TokenKind {
    match rng.below(12) {
        0 | 1 => TokenKind::Ident,
        2 | 3 => TokenKind::Op(OPS[rng.below(OPS.len())]),
        4 => {
            let c = Concept::ALL[rng.below(Concept::ALL.len())];
            if c.name().starts_with("CORE") {
                TokenKind::Core(c)
            } else {
                TokenKind::Keyword(c)
            }
        }
        5 => TokenKind::Int,
        6 => TokenKind::Float,
        7 => TokenKind::Str,
        8 => TokenKind::TaggedStr,
        9 => TokenKind::Newline,
        10 => {
            if rng.chance(50) {
                TokenKind::Indent
            } else {
                TokenKind::Dedent
            }
        }
        _ => {
            if rng.chance(10) {
                TokenKind::Eof
            } else {
                TokenKind::Op(Op::Colon)
            }
        }
    }
}

const PIECES: &[&str] = &[
    "x", " ", "\n", "    ", "(", ")", "[", "]", "{", "}", ":", ",", ".", "\"", "\"\"\"", "#",
    "إذا", "ليكن", "١٢", "if", "fn", "let", "match", "case", "else", "=>", "->", "?", "`", "\\{",
    "@", "|", "..", "law", "dala", "٫", "،", "؟", "é", "\u{202E}",
];

fn random_text(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..rng.below(60) {
        s.push_str(PIECES[rng.below(PIECES.len())]);
    }
    s
}

#[test]
fn parser_never_panics_on_random_token_streams() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let iterations = 20_000;
    for iter in 0..iterations {
        let text = random_text(&mut rng);
        let len = text.len() as u32;
        let n = rng.below(120);
        let mut tokens = Vec::with_capacity(n);
        let mut pos = 0u32;
        for _ in 0..n {
            let kind = random_kind(&mut rng);
            let span = if rng.chance(85) {
                // Plausible, monotonic span (possibly not on a char boundary).
                let start = pos.min(len);
                let end = (start + rng.below(6) as u32).min(len);
                pos = end;
                Span { start, end }
            } else {
                // Bogus: reversed, out of range, or anywhere.
                Span {
                    start: rng.below(len as usize + 20) as u32,
                    end: rng.below(len as usize + 20) as u32,
                }
            };
            tokens.push(Token {
                kind,
                span,
                flags: TokenFlags::default(),
            });
        }
        if rng.chance(70) {
            tokens.push(Token {
                kind: TokenKind::Eof,
                span: Span {
                    start: len,
                    end: len,
                },
                flags: TokenFlags::default(),
            });
        }
        let mut trivia = Vec::new();
        if rng.chance(20) {
            trivia.push(Trivia {
                kind: TriviaKind::Comment,
                span: Span {
                    start: rng.below(len as usize + 3) as u32,
                    end: rng.below(len as usize + 3) as u32,
                },
            });
        }
        let lexed = LexOutput {
            tokens,
            trivia,
            diagnostics: Vec::new(),
        };
        let skin = Skin::ALL[rng.below(3)];
        let r = catch_unwind(AssertUnwindSafe(|| parse(&text, &lexed, skin, FileId(0))));
        if r.is_err() {
            panic!(
                "iteration {iter}: parser panicked\ntext: {text:?}\ntokens: {:?}",
                lexed.tokens
            );
        }
    }
    eprintln!("fuzz: {iterations} random token streams, 0 panics");
}

#[test]
fn parser_never_panics_on_mutated_programs_and_stays_lossless() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let mut seeds = Vec::new();
    for case in corpus_cases() {
        for (skin, path) in case.files {
            seeds.push((skin, std::fs::read_to_string(path).unwrap()));
        }
    }
    for b in spec_section_28() {
        seeds.push((b.skin, b.body));
    }
    let iterations = 4_000;
    let mut runs = 0;
    let mut lexer_panics: Vec<String> = Vec::new();
    for iter in 0..iterations {
        let (skin, seed) = &seeds[rng.below(seeds.len())];
        let mut chars: Vec<char> = seed.chars().collect();
        for _ in 0..1 + rng.below(8) {
            if chars.is_empty() {
                break;
            }
            let at = rng.below(chars.len());
            match rng.below(4) {
                0 => {
                    chars.remove(at);
                }
                1 => {
                    let end = (at + rng.below(40)).min(chars.len());
                    chars.drain(at..end);
                }
                2 => {
                    let piece: Vec<char> = PIECES[rng.below(PIECES.len())].chars().collect();
                    for (k, c) in piece.into_iter().enumerate() {
                        chars.insert(at + k, c);
                    }
                }
                _ => {
                    let end = (at + rng.below(30)).min(chars.len());
                    let dup: Vec<char> = chars[at..end].to_vec();
                    for (k, c) in dup.into_iter().enumerate() {
                        chars.insert(at + k, c);
                    }
                }
            }
        }
        let text: String = chars.into_iter().collect();
        // The lexer is another crate's scope: a lexer panic is counted and reported, and
        // only the parser is held to the never-panic property here.
        let lexed = match catch_unwind(AssertUnwindSafe(|| {
            kalam_lexer::lex(&text, *skin, FileId(0))
        })) {
            Ok(l) => l,
            Err(_) => {
                lexer_panics.push(text);
                continue;
            }
        };
        let r = catch_unwind(AssertUnwindSafe(|| {
            let out = parse(&text, &lexed, *skin, FileId(0));
            out.cst.text() == text
        }));
        match r {
            Ok(true) => runs += 1,
            Ok(false) => panic!("iteration {iter}: CST not lossless for {text:?}"),
            Err(_) => panic!("iteration {iter}: parser panic for {skin} input {text:?}"),
        }
    }
    eprintln!(
        "fuzz: {runs} mutated programs parsed, 0 parser panics, all lossless; {} lexer panics (not parser scope){}",
        lexer_panics.len(),
        lexer_panics.first().map(|t| format!(", first: {t:?}")).unwrap_or_default()
    );
}
