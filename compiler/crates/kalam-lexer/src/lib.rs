//! The Kalam lexer: source-text security checks, skin-aware keyword recognition, numbers,
//! strings, comments and layout (INDENT / DEDENT / NEWLINE).
//!
//! Spec references: §5 (lexical structure), §6 (D7: RTL, digits, punctuation), §7 (keywords).
//! ARCHITECTURE.md §5 describes the design and §5.4 lists what this scaffold still defers:
//! leading-dot continuation, NFC verification, exact XID and single-script checks, the
//! emoji ZWJ and variation-selector exceptions, interpolation sub-lexing, and K0201 for
//! another skin's word inside an interpolation hole.
//!
//! K0201 (spec §5.4): another skin's keyword, core name or Franco variant is reported where
//! reading it as a name is a syntax error (`word()`: it is followed by another word) or an
//! undefined name (`cross_skin_names()`: a file-local declaration lookup, FOUNDER-QUEUE Q9).
//!
//! The lexer scans by `char` index and keeps byte offsets for spans, so it never slices a
//! string inside a multi-byte Arabic character.

use std::collections::HashSet;

use kalam_base::{FileId, Skin, Span};
use kalam_diagnostics::{Diagnostic, Fix};
use kalam_skins::{is_tatweel_or_haraka, Concept, KeywordTable, Kind, SkinTable, Spelling};

/// ASCII operators and punctuation (spec §5.9). `،` and `؟` in `ar` files lex as
/// [`Op::Comma`] and [`Op::Question`] with [`TokenFlags::arabic_glyph`] set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Plus,
    Minus,
    Star,
    Slash,
    SlashSlash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    Arrow,
    FatArrow,
    DotDot,
    DotDotEq,
    Question,
    Dot,
    Comma,
    Colon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    At,
    Pipe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// An identifier, including raw identifiers written in backticks.
    Ident,
    /// A translated keyword or a fixed-ASCII keyword (`unsafe`, `extern`).
    Keyword(Concept),
    /// A core name (`Int`, `print`, `Some`, `main`, …): reserved, translated per skin.
    Core(Concept),
    Int,
    Float,
    Str,
    /// `TAG"…"`: the token span covers the tag and the string.
    TaggedStr,
    Op(Op),
    Newline,
    Indent,
    Dedent,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TokenFlags {
    /// A keyword written with an accepted non-canonical spelling (Arabic hamza/alef form,
    /// Franco variant, or with tatweel/harakat). `fmt` rewrites it to the canonical form.
    pub variant_spelling: bool,
    /// A number written with Arabic-Indic digits, or `،` / `؟`.
    pub arabic_glyph: bool,
    /// An identifier written in backticks.
    pub raw_ident: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub flags: TokenFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Comment,
    DocComment,
}

/// Comments are trivia: kept for the lossless CST, never seen by the grammar.
/// Spaces and newlines need no trivia records; they are the gaps between spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub trivia: Vec<Trivia>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LexOutput {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }

    pub fn kinds(&self) -> Vec<TokenKind> {
        self.tokens.iter().map(|t| t.kind).collect()
    }
}

/// Lexes one file with the compiler's embedded keyword table.
pub fn lex(text: &str, skin: Skin, file: FileId) -> LexOutput {
    lex_with(text, skin, file, KeywordTable::builtin())
}

pub fn lex_with(text: &str, skin: Skin, file: FileId, table: &KeywordTable) -> LexOutput {
    let lexer = Lexer {
        text,
        chars: text.char_indices().collect(),
        i: 0,
        skin,
        table,
        words: table.skin(skin),
        file,
        out: LexOutput::default(),
        indents: vec![0],
        depth: 0,
        line_has_token: false,
    };
    lexer.run()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Code,
    Comment,
    Str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Ok,
    Bom,
    Control,
    NonAsciiSpace,
    Bidi,
    Invisible,
    TagOrPrivate,
    LineSeparator,
}

/// Spec §5.1 character classes that are rejected (or escape-only) everywhere.
fn classify(c: char) -> Class {
    match c {
        '\u{FEFF}' => Class::Bom,
        '\u{202A}'..='\u{202E}'
        | '\u{2066}'..='\u{2069}'
        | '\u{200E}'
        | '\u{200F}'
        | '\u{061C}' => Class::Bidi,
        '\u{2028}' | '\u{2029}' => Class::LineSeparator,
        '\u{200B}'
        | '\u{200C}'
        | '\u{200D}'
        | '\u{2060}'..='\u{2064}'
        | '\u{180E}'
        | '\u{00AD}'
        | '\u{034F}'
        | '\u{115F}'
        | '\u{1160}'
        | '\u{3164}'
        | '\u{FFA0}'
        | '\u{FE00}'..='\u{FE0F}'
        | '\u{E0100}'..='\u{E01EF}' => Class::Invisible,
        '\u{E0000}'..='\u{E007F}'
        | '\u{E000}'..='\u{F8FF}'
        | '\u{F0000}'..='\u{FFFFD}'
        | '\u{100000}'..='\u{10FFFD}' => Class::TagOrPrivate,
        '\u{00A0}'
        | '\u{1680}'
        | '\u{2000}'..='\u{200A}'
        | '\u{202F}'
        | '\u{205F}'
        | '\u{3000}' => Class::NonAsciiSpace,
        '\u{0000}'..='\u{0008}'
        | '\u{000B}'
        | '\u{000C}'
        | '\u{000E}'..='\u{001F}'
        | '\u{007F}'..='\u{009F}' => Class::Control,
        _ => Class::Ok,
    }
}

/// A short Unicode name for the characters diagnostics mention most often.
fn char_name(c: char) -> &'static str {
    match c {
        '\u{FEFF}' => "ZERO WIDTH NO-BREAK SPACE (BOM)",
        '\u{202A}' => "LEFT-TO-RIGHT EMBEDDING",
        '\u{202B}' => "RIGHT-TO-LEFT EMBEDDING",
        '\u{202C}' => "POP DIRECTIONAL FORMATTING",
        '\u{202D}' => "LEFT-TO-RIGHT OVERRIDE",
        '\u{202E}' => "RIGHT-TO-LEFT OVERRIDE",
        '\u{2066}' => "LEFT-TO-RIGHT ISOLATE",
        '\u{2067}' => "RIGHT-TO-LEFT ISOLATE",
        '\u{2068}' => "FIRST STRONG ISOLATE",
        '\u{2069}' => "POP DIRECTIONAL ISOLATE",
        '\u{200E}' => "LEFT-TO-RIGHT MARK",
        '\u{200F}' => "RIGHT-TO-LEFT MARK",
        '\u{061C}' => "ARABIC LETTER MARK",
        '\u{200B}' => "ZERO WIDTH SPACE",
        '\u{200C}' => "ZERO WIDTH NON-JOINER",
        '\u{200D}' => "ZERO WIDTH JOINER",
        '\u{00A0}' => "NO-BREAK SPACE",
        '\u{2028}' => "LINE SEPARATOR",
        '\u{2029}' => "PARAGRAPH SEPARATOR",
        '\u{0640}' => "ARABIC TATWEEL",
        '\t' => "CHARACTER TABULATION",
        _ => "",
    }
}

fn code_point(c: char) -> String {
    format!("U+{:04X}", c as u32)
}

fn display_name(c: char) -> String {
    match char_name(c) {
        "" => String::new(),
        name => format!("({name})"),
    }
}

pub fn is_arabic_indic_digit(c: char) -> bool {
    ('\u{0660}'..='\u{0669}').contains(&c)
}

pub fn is_persian_digit(c: char) -> bool {
    ('\u{06F0}'..='\u{06F9}').contains(&c)
}

fn is_any_digit(c: char) -> bool {
    c.is_ascii_digit() || is_arabic_indic_digit(c) || is_persian_digit(c)
}

/// Stand-in for `XID_Start` until `unicode-ident` is added (ARCHITECTURE.md §5.4).
fn is_ident_start(c: char) -> bool {
    c == '_' || (c.is_alphabetic() && !is_tatweel_or_haraka(c))
}

/// Characters consumed as part of a word run. Tatweel, harakat and non-ASCII digits are
/// consumed so they can be reported (SEC0106, SEC0124) instead of splitting the word.
fn is_ident_continue(c: char) -> bool {
    c == '_'
        || c.is_alphabetic()
        || c.is_ascii_digit()
        || is_arabic_indic_digit(c)
        || is_persian_digit(c)
        || is_tatweel_or_haraka(c)
}

/// ZWNJ / ZWJ. Inside a word run they are reported (SEC0111) but do not split the word,
/// so one stray joiner gives one diagnostic instead of a cascade of parse errors.
fn is_joiner(c: char) -> bool {
    c == '\u{200C}' || c == '\u{200D}'
}

/// Keywords that can legally follow a name on the same line (`x in xs`, `a and b`,
/// `v as T`, `case n if n > 0`, `if c: a else: b`). Any other word after a name is a
/// syntax error, which is what makes a cross-skin keyword detectable in the lexer.
fn may_follow_a_name(concept: Concept) -> bool {
    matches!(
        concept,
        Concept::KwIn
            | Concept::KwAnd
            | Concept::KwOr
            | Concept::KwAs
            | Concept::KwElse
            | Concept::KwIf
            | Concept::KwNot
    )
}

/// A token after which an identifier would be a second operand side by side with the
/// first (a syntax error), while an infix keyword would be fine.
fn ends_an_operand(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident
            | TokenKind::Core(_)
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Str
            | TokenKind::TaggedStr
            | TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace | Op::Question)
            | TokenKind::Keyword(Concept::KwTrue | Concept::KwFalse | Concept::KwSelf)
    )
}

/// Every name the file declares or imports, by its text (tatweel, harakat and joiners
/// removed). Scope-insensitive on purpose: a name declared anywhere counts as declared
/// everywhere, so a doubtful position never produces K0201.
fn declared_names(tokens: &[Token], word: &dyn Fn(&Token) -> String) -> HashSet<String> {
    let mut out = HashSet::new();
    let kind = |j: usize| tokens.get(j).map(|t| t.kind);
    let is = |j: usize, op: Op| kind(j) == Some(TokenKind::Op(op));
    let mut add = |j: usize, out: &mut HashSet<String>| {
        if kind(j) == Some(TokenKind::Ident) && !(j > 0 && is(j - 1, Op::Dot)) {
            out.insert(word(&tokens[j]));
        }
    };
    // Identifiers directly inside the bracket group opened at `open` that start an item
    // (after the opening bracket or a comma): parameters, generic parameters.
    let group = |open: usize,
                 out: &mut HashSet<String>,
                 add: &mut dyn FnMut(usize, &mut HashSet<String>)|
     -> usize {
        let mut depth = 0usize;
        let mut j = open;
        while let Some(k) = kind(j) {
            match k {
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return j + 1;
                    }
                }
                TokenKind::Newline | TokenKind::Eof => return j,
                TokenKind::Ident if depth == 1 && (j == open + 1 || is(j - 1, Op::Comma)) => {
                    add(j, out)
                }
                _ => {}
            }
            j += 1;
        }
        j
    };
    // Identifiers from `from` up to (not including) a stop token at bracket depth 0. In a
    // pattern (`pattern`), an identifier right after a complete operand ends it too: that is
    // another skin's keyword (`for x in xs` in an Arabic file), not a binding.
    let until = |from: usize,
                 stop: &dyn Fn(TokenKind) -> bool,
                 pattern: bool,
                 out: &mut HashSet<String>,
                 add: &mut dyn FnMut(usize, &mut HashSet<String>)| {
        let mut depth = 0usize;
        let mut j = from;
        while let Some(k) = kind(j) {
            match k {
                TokenKind::Newline | TokenKind::Eof => break,
                _ if depth == 0 && stop(k) => break,
                TokenKind::Ident
                    if pattern
                        && depth == 0
                        && j > from
                        && kind(j - 1).is_some_and(ends_an_operand) =>
                {
                    break
                }
                TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace) => depth += 1,
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    depth = depth.saturating_sub(1)
                }
                TokenKind::Ident => add(j, out),
                _ => {}
            }
            j += 1;
        }
    };
    for (i, t) in tokens.iter().enumerate() {
        let TokenKind::Keyword(c) = t.kind else {
            continue;
        };
        match c {
            Concept::KwLet | Concept::KwVar => until(
                i + 1,
                &|k| matches!(k, TokenKind::Op(Op::Eq | Op::Colon)),
                true,
                &mut out,
                &mut add,
            ),
            Concept::KwFor => until(
                i + 1,
                &|k| k == TokenKind::Keyword(Concept::KwIn),
                true,
                &mut out,
                &mut add,
            ),
            Concept::KwCase => until(
                i + 1,
                &|k| {
                    matches!(
                        k,
                        TokenKind::Op(Op::Colon) | TokenKind::Keyword(Concept::KwIf)
                    )
                },
                true,
                &mut out,
                &mut add,
            ),
            Concept::KwUse => until(i + 1, &|_| false, false, &mut out, &mut add),
            Concept::KwAs => add(i + 1, &mut out),
            Concept::KwFn
            | Concept::KwType
            | Concept::KwEnum
            | Concept::KwTrait
            | Concept::KwImpl => {
                let mut j = i + 1;
                if kind(j) == Some(TokenKind::Ident) {
                    add(j, &mut out);
                    j += 1;
                }
                if is(j, Op::LBracket) {
                    j = group(j, &mut out, &mut add);
                }
                if c == Concept::KwFn && is(j, Op::LParen) {
                    group(j, &mut out, &mut add);
                }
            }
            _ => {}
        }
    }
    out
}

/// Inserts `new` (in source order) among `existing` (in report order) by span start,
/// keeping the existing relative order.
fn merge_by_position(existing: &mut Vec<Diagnostic>, new: Vec<Diagnostic>) {
    for d in new {
        let at = existing
            .iter()
            .position(|e| e.span.start > d.span.start)
            .unwrap_or(existing.len());
        existing.insert(at, d);
    }
}

fn is_dec(c: char) -> bool {
    c.is_ascii_digit()
}

fn is_hex(c: char) -> bool {
    c.is_ascii_hexdigit()
}

fn is_bin(c: char) -> bool {
    c == '0' || c == '1'
}

fn is_oct(c: char) -> bool {
    ('0'..='7').contains(&c)
}

/// `D { [ "_" ] D }`: non-empty, starts and ends with a digit, no doubled underscore.
fn digits_ok(s: &str, pred: fn(char) -> bool) -> bool {
    s.chars().next().is_some_and(pred)
        && s.chars().last().is_some_and(pred)
        && !s.contains("__")
        && s.chars().all(|c| c == '_' || pred(c))
}

/// Validates a run made of ASCII digits and letters against `INT_LIT | FLOAT_LIT` (§5.6).
fn validate_ascii_number(s: &str) -> Result<TokenKind, &'static str> {
    if s.contains('\u{066B}') {
        return Err("SEC0120"); // Arabic decimal separator with ASCII digits
    }
    if let Some(rest) = s.strip_prefix("0x") {
        return if digits_ok(rest, is_hex) {
            Ok(TokenKind::Int)
        } else {
            Err("K0127")
        };
    }
    if let Some(rest) = s.strip_prefix("0b") {
        return if digits_ok(rest, is_bin) {
            Ok(TokenKind::Int)
        } else {
            Err("K0127")
        };
    }
    if let Some(rest) = s.strip_prefix("0o") {
        return if digits_ok(rest, is_oct) {
            Ok(TokenKind::Int)
        } else {
            Err("K0127")
        };
    }
    let (mantissa, exponent) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let (int_part, frac) = match mantissa.find('.') {
        Some(i) => (&mantissa[..i], Some(&mantissa[i + 1..])),
        None => (mantissa, None),
    };
    if !digits_ok(int_part, is_dec) {
        return Err("K0127");
    }
    if let Some(f) = frac {
        if !digits_ok(f, is_dec) {
            return Err("K0127");
        }
    }
    if let Some(e) = exponent {
        let digits = e.strip_prefix(|c: char| c == '+' || c == '-').unwrap_or(e);
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err("K0127");
        }
    }
    if int_part.len() > 1 && int_part.starts_with('0') {
        return Err("K0125");
    }
    Ok(if frac.is_some() || exponent.is_some() {
        TokenKind::Float
    } else {
        TokenKind::Int
    })
}

/// Validates an Arabic-Indic literal: `DEC_ARABIC [ "٫" digits ]`, no prefix, no exponent.
fn validate_arabic_number(s: &str) -> Result<TokenKind, &'static str> {
    if s.contains('.') {
        return Err("SEC0120"); // ASCII decimal point with Arabic-Indic digits
    }
    if s.chars().any(|c| c.is_alphabetic()) {
        let looks_like_form = s.chars().any(|c| matches!(c, 'e' | 'E' | 'x' | 'b' | 'o'));
        return Err(if looks_like_form { "K0128" } else { "K0127" });
    }
    let (int_part, frac) = match s.find('\u{066B}') {
        Some(i) => (&s[..i], Some(&s[i + '\u{066B}'.len_utf8()..])),
        None => (s, None),
    };
    if !digits_ok(int_part, is_arabic_indic_digit) {
        return Err("K0127");
    }
    if let Some(f) = frac {
        if !digits_ok(f, is_arabic_indic_digit) {
            return Err("K0127");
        }
    }
    if int_part.chars().count() > 1 && int_part.starts_with('\u{0660}') {
        return Err("K0125");
    }
    Ok(if frac.is_some() {
        TokenKind::Float
    } else {
        TokenKind::Int
    })
}

/// The exact value of an integer-literal token's text (decimal with ASCII or Arabic-Indic
/// digits, `0x`, `0b`, `0o`, with `_` separators). `None` if the text is not a valid
/// `INT_LIT` or does not fit in `u128`.
///
/// The lexer never range-checks a literal itself: an integer literal may be a `BigInt`
/// (any size), and `-9223372036854775808` reaches the lexer without its sign. The type
/// checker reports K0126 in `Int` context using [`int_literal_fits_int`] (spec §5.6, D18).
pub fn int_literal_value(text: &str) -> Option<u128> {
    // Mirror the lexer's acceptance rules (one digit system per literal, no Persian digits,
    // Arabic-Indic forms validated on their own) before normalizing to ASCII.
    let has_ascii = text.chars().any(|c| c.is_ascii_digit());
    let has_arabic = text.chars().any(is_arabic_indic_digit);
    if text.chars().any(is_persian_digit) || (has_ascii && has_arabic) {
        return None;
    }
    if has_arabic && validate_arabic_number(text) != Ok(TokenKind::Int) {
        return None;
    }
    let ascii = to_ascii_digits(text);
    if validate_ascii_number(&ascii) != Ok(TokenKind::Int) {
        return None;
    }
    let (digits, radix) = if let Some(rest) = ascii.strip_prefix("0x") {
        (rest, 16)
    } else if let Some(rest) = ascii.strip_prefix("0b") {
        (rest, 2)
    } else if let Some(rest) = ascii.strip_prefix("0o") {
        (rest, 8)
    } else {
        (ascii.as_str(), 10)
    };
    let mut value: u128 = 0;
    for c in digits.chars().filter(|&c| c != '_') {
        let d = c.to_digit(radix)? as u128;
        value = value.checked_mul(radix as u128)?.checked_add(d)?;
    }
    Some(value)
}

/// True if a literal of this magnitude is an `Int`: `Int` is a true 64-bit checked
/// integer on every backend, [−2⁶³, 2⁶³−1] (DECISIONS.md D18). `negated` is true when
/// the literal is the direct operand of unary minus, so `-9223372036854775808` fits.
pub fn int_literal_fits_int(magnitude: u128, negated: bool) -> bool {
    let limit = if negated {
        1u128 << 63
    } else {
        (1u128 << 63) - 1
    };
    magnitude <= limit
}

/// Rewrites Arabic-Indic and Persian digits to ASCII and `٫` to `.`.
pub fn to_ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| {
            if is_arabic_indic_digit(c) {
                char::from(b'0' + (c as u32 - 0x0660) as u8)
            } else if is_persian_digit(c) {
                char::from(b'0' + (c as u32 - 0x06F0) as u8)
            } else if c == '\u{066B}' {
                '.'
            } else {
                c
            }
        })
        .collect()
}

/// Removes leading zeros for the K0125 fix (`007` → `7`, `00.5` → `0.5`).
fn strip_leading_zeros(s: &str) -> String {
    let trimmed = s.trim_start_matches(['0', '\u{0660}', '_']);
    if trimmed.is_empty() || trimmed.starts_with('.') || trimmed.starts_with('\u{066B}') {
        let zero = if s.starts_with('\u{0660}') {
            '\u{0660}'
        } else {
            '0'
        };
        format!("{zero}{trimmed}")
    } else {
        trimmed.to_string()
    }
}

struct Lexer<'a> {
    text: &'a str,
    chars: Vec<(usize, char)>,
    i: usize,
    skin: Skin,
    table: &'a KeywordTable,
    words: &'a SkinTable,
    file: FileId,
    out: LexOutput,
    /// Indentation widths of the open blocks; always starts with 0.
    indents: Vec<u32>,
    /// Bracket depth; newlines and indentation are ignored inside brackets.
    depth: u32,
    /// Whether the current logical line has produced a significant token.
    line_has_token: bool,
}

impl<'a> Lexer<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).map(|&(_, c)| c)
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.i + n).map(|&(_, c)| c)
    }

    fn offset(&self, i: usize) -> u32 {
        match self.chars.get(i) {
            Some(&(off, _)) => off as u32,
            None => self.text.len() as u32,
        }
    }

    fn pos(&self) -> u32 {
        self.offset(self.i)
    }

    /// Span of the single character at the current position.
    fn char_span(&self) -> Span {
        Span::new(self.pos(), self.offset(self.i + 1))
    }

    fn diag(&self, code: &'static str, span: Span) -> Diagnostic {
        Diagnostic::new(code, self.file, span)
    }

    fn report(&mut self, d: Diagnostic) {
        self.out.diagnostics.push(d);
    }

    fn last_is_dot(&self) -> bool {
        matches!(
            self.out.tokens.last(),
            Some(Token {
                kind: TokenKind::Op(Op::Dot),
                ..
            })
        )
    }

    fn push(&mut self, kind: TokenKind, start: u32, flags: TokenFlags) {
        let span = Span::new(start, self.pos());
        self.out.tokens.push(Token { kind, span, flags });
        self.line_has_token = true;
    }

    fn push_layout(&mut self, kind: TokenKind, span: Span) {
        self.out.tokens.push(Token {
            kind,
            span,
            flags: TokenFlags::default(),
        });
    }

    fn run(mut self) -> LexOutput {
        self.line_start();
        while let Some(c) = self.peek() {
            match c {
                '\n' => self.newline(1),
                '\r' => {
                    if self.peek_at(1) == Some('\n') {
                        self.newline(2);
                    } else {
                        let d = self
                            .diag("SEC0113", self.char_span())
                            .arg("cp", code_point(c))
                            .arg("name", "(CARRIAGE RETURN)");
                        self.report(d);
                        self.i += 1;
                    }
                }
                ' ' => self.i += 1,
                '#' => self.comment(),
                '"' => self.string(None),
                '\'' => self.single_quoted(),
                '`' => self.raw_ident(),
                c if is_any_digit(c) => self.number(),
                c if is_ident_start(c) => self.word(),
                c => self.punct(c),
            }
        }
        let end = self.text.len() as u32;
        if self.line_has_token {
            self.push_layout(TokenKind::Newline, Span::new(end, end));
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push_layout(TokenKind::Dedent, Span::new(end, end));
        }
        self.push_layout(TokenKind::Eof, Span::new(end, end));
        // A one-line fragment is interpolation-hole code re-lexed on its own (by the
        // parser, KCF and convert): it cannot see the file's declarations, so the
        // undefined-name half of K0201 is decided by the file's own lex instead.
        if self.text.contains('\n') {
            self.cross_skin_names();
        }
        self.out
    }

    /// The undefined-name half of K0201 (spec §5.4 bullet 3, §21.4 rule 6; eval P8).
    ///
    /// Another skin's keyword, core name or Franco variant that the file lexed as an
    /// identifier is K0201, with a machine fix to this skin's spelling, unless it is
    /// legitimately a name:
    /// - it is declared in this file (by `let`/`var`, `fn` and its parameters, `for` and
    ///   `case` patterns, `type`/`enum`/`trait` names, generic parameters, `use` items and
    ///   `as` aliases); spec §5.4 bullet 2, §4.3. Without glob imports or globals (§21.2)
    ///   every name a file uses is declared or imported in it, so a name that is not is
    ///   undefined. The lookup ignores scopes, so it errs towards "declared";
    /// - it is in a §5.3 rule 8 position: after `.` or `@`, a label (`f(match: 1)`), or a
    ///   field name in a `type` body or a variant name in an `enum` body;
    /// - it is written in backticks (§5.3 rule 7).
    ///
    /// Where reading the word as a name would be a syntax error, the token becomes the
    /// keyword (so the parser recovers without a cascade): at the start of a line, before
    /// `:`, after an operand (`a we (b)`), and always for core names and the value keywords
    /// `true`/`false`/`self`. Elsewhere (`x = law`) it stays an identifier.
    fn cross_skin_names(&mut self) {
        let text = self.text;
        let tokens = &self.out.tokens;
        let word = |t: &Token| -> String {
            text[t.span.range()]
                .chars()
                .filter(|&c| !is_tatweel_or_haraka(c) && !is_joiner(c))
                .collect()
        };
        let declared = declared_names(tokens, &word);
        let mut found: Vec<(usize, Concept, Option<TokenKind>)> = Vec::new();
        let mut brackets: Vec<Op> = Vec::new();
        let mut bodies: Vec<Option<Concept>> = Vec::new();
        let mut line_head: Option<Concept> = None;
        let mut prev_line_head: Option<Concept> = None;
        for (i, t) in tokens.iter().enumerate() {
            let prev = i.checked_sub(1).map(|p| tokens[p].kind);
            let next = tokens.get(i + 1).map(|n| n.kind);
            let line_start = matches!(
                prev,
                None | Some(TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent)
            );
            match t.kind {
                TokenKind::Newline => {
                    prev_line_head = line_head.take();
                    continue;
                }
                TokenKind::Indent => {
                    bodies.push(prev_line_head);
                    continue;
                }
                TokenKind::Dedent => {
                    bodies.pop();
                    continue;
                }
                TokenKind::Op(op @ (Op::LParen | Op::LBracket | Op::LBrace)) => {
                    brackets.push(op);
                }
                TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace) => {
                    brackets.pop();
                }
                TokenKind::Keyword(c)
                    if line_head.is_none()
                        && !matches!(c, Concept::KwExport | Concept::KwOpaque) =>
                {
                    line_head = Some(c);
                }
                _ => {}
            }
            if t.kind != TokenKind::Ident || t.flags.raw_ident {
                continue;
            }
            if matches!(prev, Some(TokenKind::Op(Op::Dot | Op::At))) {
                continue;
            }
            let colon_next = next == Some(TokenKind::Op(Op::Colon));
            let label = colon_next
                && matches!(prev, Some(TokenKind::Op(Op::LParen | Op::Comma)))
                && brackets.last() == Some(&Op::LParen);
            let body = bodies.last().copied().flatten();
            let field = line_start && colon_next && body == Some(Concept::KwType);
            let variant = line_start && body == Some(Concept::KwEnum);
            if label || field || variant {
                continue;
            }
            let w = word(t);
            if declared.contains(&w) {
                continue;
            }
            let Some(m) = Skin::ALL
                .into_iter()
                .filter(|&s| s != self.skin)
                .filter_map(|s| self.table.skin(s).lookup_word(&w))
                .find(|m| matches!(m.kind, Kind::Keyword | Kind::CoreName))
            else {
                continue;
            };
            let keyword_reading = m.kind == Kind::CoreName
                || matches!(
                    m.concept,
                    Concept::KwTrue | Concept::KwFalse | Concept::KwSelf
                )
                || line_start
                || colon_next
                || prev.is_some_and(ends_an_operand);
            let as_kind = match m.kind {
                _ if !keyword_reading => None,
                Kind::CoreName => Some(TokenKind::Core(m.concept)),
                _ => Some(TokenKind::Keyword(m.concept)),
            };
            found.push((i, m.concept, as_kind));
        }
        let mut new_diags = Vec::new();
        for (i, concept, as_kind) in found {
            let span = self.out.tokens[i].span;
            let written = &text[span.range()];
            let expected = self.words.spelling(concept).to_string();
            new_diags.push(
                self.diag("K0201", span)
                    .arg("found", written)
                    .arg("expected", expected.clone())
                    .fix(Fix::machine(format!("write `{expected}`"), span, expected)),
            );
            if let Some(kind) = as_kind {
                self.out.tokens[i].kind = kind;
            }
        }
        merge_by_position(&mut self.out.diagnostics, new_diags);
    }

    fn newline(&mut self, len: usize) {
        let start = self.pos();
        self.i += len;
        if self.depth == 0 {
            if self.line_has_token {
                let span = Span::new(start, self.pos());
                self.push_layout(TokenKind::Newline, span);
            }
            self.line_has_token = false;
            self.line_start();
        }
    }

    /// Measures indentation at the start of a physical line and emits INDENT/DEDENT.
    /// Blank and comment-only lines do not affect layout.
    fn line_start(&mut self) {
        let start = self.pos();
        let mut width = 0u32;
        loop {
            match self.peek() {
                Some(' ') => {
                    width += 1;
                    self.i += 1;
                }
                Some('\t') => {
                    let span = self.char_span();
                    let d = self.diag("K0103", span).fix(Fix::machine(
                        "replace the tab with 4 spaces",
                        span,
                        "    ",
                    ));
                    self.report(d);
                    width += 4;
                    self.i += 1;
                }
                _ => break,
            }
        }
        if matches!(self.peek(), None | Some('\n') | Some('\r') | Some('#')) {
            return;
        }
        let span = Span::new(start, self.pos());
        let top = *self.indents.last().expect("indent stack is never empty");
        if width > top {
            if width != top + 4 || width % 4 != 0 {
                let d = self.diag("K0120", span);
                self.report(d);
            }
            self.indents.push(width);
            self.push_layout(TokenKind::Indent, span);
        } else if width < top {
            let here = self.pos();
            while width < *self.indents.last().expect("indent stack is never empty") {
                self.indents.pop();
                self.push_layout(TokenKind::Dedent, Span::new(here, here));
            }
            if width != *self.indents.last().expect("indent stack is never empty") {
                let d = self.diag("K0122", span);
                self.report(d);
            }
        }
    }

    /// Reports a §5.1 character class problem for a character in a comment or string.
    /// Returns true if the character was reported.
    fn check_text_char(&mut self, c: char, ctx: Context) -> bool {
        let code = match classify(c) {
            Class::Ok => return false,
            Class::Bom => "SEC0100",
            Class::Control => "SEC0102",
            Class::Bidi => "SEC0110",
            Class::Invisible => "SEC0111",
            Class::TagOrPrivate => "SEC0112",
            Class::LineSeparator => "SEC0113",
            Class::NonAsciiSpace => {
                if ctx == Context::Str {
                    return false; // allowed raw in strings (lint W0114, not yet implemented)
                }
                "SEC0103"
            }
        };
        let span = self.char_span();
        let mut d = self
            .diag(code, span)
            .arg("cp", code_point(c))
            .arg("name", display_name(c));
        if ctx == Context::Str {
            d = d.fix(Fix::machine(
                "write it as an escape",
                span,
                format!("\\u{{{:X}}}", c as u32),
            ));
        } else if code == "SEC0103" {
            d = d.fix(Fix::machine("replace with a space", span, " "));
        }
        self.report(d);
        true
    }

    fn comment(&mut self) {
        let start = self.pos();
        let kind = if self.peek_at(1) == Some('#') {
            TriviaKind::DocComment
        } else {
            TriviaKind::Comment
        };
        while let Some(c) = self.peek() {
            if c == '\n' || c == '\r' {
                break;
            }
            if c != '\t' {
                self.check_text_char(c, Context::Comment); // tabs are allowed in comments
            }
            self.i += 1;
        }
        let span = Span::new(start, self.pos());
        self.out.trivia.push(Trivia { kind, span });
    }

    /// A `"…"` or `"""…"""` string. `tag_start` is set for tagged literals (`sql"…"`).
    fn string(&mut self, tag_start: Option<u32>) {
        let start = tag_start.unwrap_or_else(|| self.pos());
        let triple = self.peek_at(1) == Some('"') && self.peek_at(2) == Some('"');
        self.i += if triple { 3 } else { 1 };
        let mut hole_depth = 0u32;
        let mut reported_hole_string = false;
        loop {
            let Some(c) = self.peek() else {
                let d = self.diag("K0129", Span::new(start, self.pos()));
                self.report(d);
                break;
            };
            if triple {
                if c == '"'
                    && hole_depth == 0
                    && self.peek_at(1) == Some('"')
                    && self.peek_at(2) == Some('"')
                {
                    self.i += 3;
                    break;
                }
                if c == '\n' {
                    self.i += 1;
                    continue;
                }
                if c == '\r' && self.peek_at(1) == Some('\n') {
                    self.i += 2;
                    continue;
                }
            } else {
                if c == '\n' || c == '\r' {
                    let d = self.diag("K0129", Span::new(start, self.pos()));
                    self.report(d);
                    break; // the newline is left for the main loop
                }
                if c == '"' && hole_depth == 0 {
                    self.i += 1;
                    break;
                }
            }
            match c {
                '\\' => self.escape(),
                '{' => {
                    hole_depth += 1;
                    self.i += 1;
                }
                '}' => {
                    hole_depth = hole_depth.saturating_sub(1);
                    self.i += 1;
                }
                '"' => {
                    if hole_depth > 0 && !reported_hole_string {
                        let kw_let = self.table.spelling(Concept::KwLet, self.skin).to_string();
                        let d = self.diag("K0131", self.char_span()).arg("kw_let", kw_let);
                        self.report(d);
                        reported_hole_string = true;
                    }
                    self.i += 1;
                }
                '\t' => {
                    let span = self.char_span();
                    let d = self.diag("K0103", span).fix(Fix::machine(
                        "write the tab as \\t",
                        span,
                        "\\t",
                    ));
                    self.report(d);
                    self.i += 1;
                }
                _ => {
                    self.check_text_char(c, Context::Str);
                    self.i += 1;
                }
            }
        }
        let kind = if tag_start.is_some() {
            TokenKind::TaggedStr
        } else {
            TokenKind::Str
        };
        self.push(kind, start, TokenFlags::default());
    }

    /// One escape sequence inside a string (spec §5.7).
    fn escape(&mut self) {
        let start = self.pos();
        self.i += 1; // the backslash
        match self.peek() {
            Some('n' | 't' | 'r' | '\\' | '"' | '{' | '}' | '0') => self.i += 1,
            Some('u') => {
                self.i += 1;
                let mut ok = false;
                if self.peek() == Some('{') {
                    self.i += 1;
                    let hex_start = self.i;
                    while matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                        self.i += 1;
                    }
                    let count = self.i - hex_start;
                    if self.peek() == Some('}') && (1..=6).contains(&count) {
                        let hex: String = self.chars[hex_start..self.i]
                            .iter()
                            .map(|&(_, c)| c)
                            .collect();
                        self.i += 1;
                        let value = u32::from_str_radix(&hex, 16).unwrap_or(u32::MAX);
                        let escape = self.text[start as usize..self.pos() as usize].to_string();
                        if (0xD800..=0xDFFF).contains(&value) {
                            let d = self
                                .diag("K0132", Span::new(start, self.pos()))
                                .arg("escape", escape);
                            self.report(d);
                        } else if char::from_u32(value).is_none() {
                            let d = self
                                .diag("K0130", Span::new(start, self.pos()))
                                .arg("escape", escape);
                            self.report(d);
                        }
                        ok = true;
                    }
                }
                if !ok {
                    let escape = self.text[start as usize..self.pos() as usize].to_string();
                    let d = self
                        .diag("K0130", Span::new(start, self.pos()))
                        .arg("escape", escape);
                    self.report(d);
                }
            }
            Some(c) if c != '\n' && c != '\r' => {
                self.i += 1;
                let escape = self.text[start as usize..self.pos() as usize].to_string();
                let d = self
                    .diag("K0130", Span::new(start, self.pos()))
                    .arg("escape", escape);
                self.report(d);
            }
            _ => {
                let d = self
                    .diag("K0130", Span::new(start, self.pos()))
                    .arg("escape", "\\");
                self.report(d);
            }
        }
    }

    /// `'text'` is K0924. It is lexed as a string so later stages see one token.
    fn single_quoted(&mut self) {
        let start = self.pos();
        self.i += 1;
        let content_start = self.i;
        while matches!(self.peek(), Some(c) if c != '\'' && c != '\n' && c != '\r') {
            self.i += 1;
        }
        let content: String = self.chars[content_start..self.i]
            .iter()
            .map(|&(_, c)| c)
            .collect();
        if self.peek() == Some('\'') {
            self.i += 1;
        }
        let span = Span::new(start, self.pos());
        let mut d = self.diag("K0924", span);
        if !content.contains('"') && !content.contains('\\') {
            d = d.fix(Fix::machine(
                "use double quotes",
                span,
                format!("\"{content}\""),
            ));
        }
        self.report(d);
        self.push(TokenKind::Str, start, TokenFlags::default());
    }

    /// `` `name` ``: a raw identifier (DR-17).
    fn raw_ident(&mut self) {
        let start = self.pos();
        let backtick = self.i;
        self.i += 1;
        if matches!(self.peek(), Some(c) if is_ident_start(c)) {
            while matches!(self.peek(), Some(c) if is_ident_continue(c)) {
                self.i += 1;
            }
            if self.peek() == Some('`') {
                self.i += 1;
                let flags = TokenFlags {
                    raw_ident: true,
                    ..TokenFlags::default()
                };
                self.push(TokenKind::Ident, start, flags);
                return;
            }
        }
        // Not a well-formed raw identifier: report the backtick and lex what follows normally.
        self.i = backtick;
        let d = self
            .diag("K0105", self.char_span())
            .arg("cp", code_point('`'))
            .arg("name", "(GRAVE ACCENT)");
        self.report(d);
        self.i = backtick + 1;
    }

    /// A word run: keyword, core name, identifier or the tag of a tagged literal.
    fn word(&mut self) {
        let src = self.text;
        let start_i = self.i;
        let start = self.pos();
        loop {
            match self.peek() {
                Some(c) if is_ident_continue(c) => self.i += 1,
                Some(c)
                    if is_joiner(c)
                        && matches!(self.peek_at(1), Some(n) if is_ident_continue(n)) =>
                {
                    self.i += 1
                }
                _ => break,
            }
        }
        let text = &src[start as usize..self.pos() as usize];

        // `sql"…"`: tags are lowercase ASCII and recognized only directly before a quote.
        if self.peek() == Some('"') && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            self.string(Some(start));
            return;
        }

        for k in start_i..self.i {
            let (off, c) = self.chars[k];
            let span = Span::new(off as u32, self.offset(k + 1));
            if is_tatweel_or_haraka(c) {
                let d = self
                    .diag("SEC0106", span)
                    .arg("cp", code_point(c))
                    .arg("name", display_name(c))
                    .fix(Fix::machine("remove the mark", span, ""));
                self.report(d);
            } else if is_arabic_indic_digit(c) || is_persian_digit(c) {
                let ascii = to_ascii_digits(&c.to_string());
                let d = self
                    .diag("SEC0124", span)
                    .arg("cp", code_point(c))
                    .fix(Fix::machine("use an ASCII digit", span, ascii));
                self.report(d);
            } else if is_joiner(c) {
                let d = self
                    .diag("SEC0111", span)
                    .arg("cp", code_point(c))
                    .arg("name", display_name(c))
                    .fix(Fix::machine("remove the invisible character", span, ""));
                self.report(d);
            }
        }

        // Contextual freedom (§5.3 rule 8): a word right after `.` is always a member name.
        if !self.last_is_dot() {
            let cleaned: String = text
                .chars()
                .filter(|&c| !is_tatweel_or_haraka(c) && !is_joiner(c))
                .collect();
            if let Some(m) = self.words.lookup_word(&cleaned) {
                let kind = match m.kind {
                    Kind::CoreName => TokenKind::Core(m.concept),
                    _ => TokenKind::Keyword(m.concept),
                };
                let flags = TokenFlags {
                    variant_spelling: m.spelling == Spelling::Variant || cleaned != text,
                    ..TokenFlags::default()
                };
                self.push(kind, start, flags);
                return;
            }
            if let Some(concept) = self.cross_skin_keyword(&cleaned) {
                let expected = self.words.spelling(concept).to_string();
                let span = Span::new(start, self.pos());
                let d = self
                    .diag("K0201", span)
                    .arg("found", text)
                    .arg("expected", expected.clone())
                    .fix(Fix::machine(format!("write `{expected}`"), span, expected));
                self.report(d);
                self.push(TokenKind::Keyword(concept), start, TokenFlags::default());
                return;
            }
        }
        self.push(TokenKind::Ident, start, TokenFlags::default());
    }

    /// K0201 (spec §5.4 bullet 3, §21.4 rule 6): `word` is a keyword of another skin, used
    /// where reading it as a name would be a syntax error. Another skin's word used as a
    /// declared name stays legal (§5.4 bullet 2), so the lexer only fires on the one
    /// position it can judge alone: the word is directly followed on the same line by
    /// another word, number or string, and that next word is not a keyword that may follow
    /// a name (`for law in xs`). Two names side by side never parse.
    ///
    /// Not covered here (needs parser position or name resolution): a cross-skin keyword
    /// followed by an operator or `:` (`else:`, `return -1`), and core names (`print(…)`
    /// in an Arabic file is an undefined name, so it belongs to the resolver).
    fn cross_skin_keyword(&self, word: &str) -> Option<Concept> {
        let concept = Skin::ALL
            .into_iter()
            .filter(|&s| s != self.skin)
            .filter_map(|s| self.table.skin(s).lookup_word(word))
            .find(|m| m.kind == Kind::Keyword)?
            .concept;

        let mut j = self.i;
        while self.chars.get(j).is_some_and(|&(_, c)| c == ' ') {
            j += 1;
        }
        let next = self.chars.get(j).map(|&(_, c)| c)?;
        if next == '"' || next == '`' || is_any_digit(next) {
            return Some(concept);
        }
        if !is_ident_start(next) {
            return None;
        }
        let next_word: String = self.chars[j..]
            .iter()
            .map(|&(_, c)| c)
            .take_while(|&c| is_ident_continue(c) || is_joiner(c))
            .filter(|&c| !is_tatweel_or_haraka(c) && !is_joiner(c))
            .collect();
        match self.words.lookup_word(&next_word) {
            Some(m) if may_follow_a_name(m.concept) => None,
            _ => Some(concept),
        }
    }

    /// "A digit always starts a number" (§5.6, DR-03).
    fn number(&mut self) {
        let src = self.text;
        let start = self.pos();
        let first = self.peek().expect("number() is called on a digit");

        // After a member-access `.`, a digit run is a decimal member index only (DR-26).
        if self.last_is_dot() && first.is_ascii_digit() {
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
            self.push(TokenKind::Int, start, TokenFlags::default());
            return;
        }

        let prefixed = first == '0' && matches!(self.peek_at(1), Some('x' | 'b' | 'o'));
        let mut plain_int = true;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                if !(is_any_digit(c) || c == '_') {
                    plain_int = false;
                }
                self.i += 1;
            } else if (c == '.' || c == '\u{066B}')
                && plain_int
                && matches!(self.peek_at(1), Some(d) if is_any_digit(d))
            {
                plain_int = false;
                self.i += 1;
            } else if (c == '+' || c == '-')
                && !prefixed
                && matches!(self.chars[self.i - 1].1, 'e' | 'E')
                && matches!(self.peek_at(1), Some(d) if d.is_ascii_digit())
            {
                self.i += 1;
            } else {
                break;
            }
        }

        let span = Span::new(start, self.pos());
        let text = &src[span.range()];
        let has_ascii = text.chars().any(|c| c.is_ascii_digit());
        let has_arabic = text.chars().any(is_arabic_indic_digit);
        let has_persian = text.chars().any(is_persian_digit);
        let mut flags = TokenFlags::default();

        let result = if has_persian {
            Err("SEC0121")
        } else if has_arabic && has_ascii {
            Err("SEC0120")
        } else if has_arabic {
            flags.arabic_glyph = true;
            if self.skin != Skin::Ar {
                let fixed = to_ascii_digits(text);
                let d = self
                    .diag("SEC0122", span)
                    .arg("literal", text)
                    .arg("fixed", fixed.clone())
                    .fix(Fix::machine("use ASCII digits", span, fixed));
                self.report(d);
            }
            validate_arabic_number(text)
        } else {
            validate_ascii_number(text)
        };

        let kind = match result {
            Ok(kind) => kind,
            Err(code) => {
                let mut d = self.diag(code, span);
                match code {
                    "K0127" => {
                        d = d.arg("word", text);
                        if let Some(concept) = self.table.did_you_mean(text) {
                            let spelling = self.table.spelling(concept, self.skin).to_string();
                            d = d.fix(Fix::machine(
                                format!("did you mean `{spelling}`?"),
                                span,
                                spelling,
                            ));
                        }
                    }
                    "K0125" => {
                        let fixed = strip_leading_zeros(text);
                        d = d
                            .arg("literal", text)
                            .arg("fixed", fixed.clone())
                            .fix(Fix::machine("remove the leading zeros", span, fixed));
                    }
                    _ => d = d.arg("literal", text),
                }
                self.report(d);
                TokenKind::Int // recovery: later stages see a literal
            }
        };
        self.push(kind, start, flags);
    }

    fn punct(&mut self, c: char) {
        let start = self.pos();
        let n1 = self.peek_at(1);
        let n2 = self.peek_at(2);

        // Characters with special handling first.
        match c {
            '\t' => {
                let span = self.char_span();
                let d = self.diag("K0103", span).fix(Fix::machine(
                    "replace the tab with 4 spaces",
                    span,
                    "    ",
                ));
                self.report(d);
                self.i += 1;
                return;
            }
            ';' | '\u{061B}' => {
                let span = self.char_span();
                let d =
                    self.diag("K0904", span)
                        .fix(Fix::machine("remove the semicolon", span, ""));
                self.report(d);
                self.i += 1;
                return;
            }
            '\u{060C}' | '\u{061F}' => {
                let op = if c == '\u{060C}' {
                    Op::Comma
                } else {
                    Op::Question
                };
                if self.skin != Skin::Ar {
                    let ascii = if c == '\u{060C}' { "," } else { "?" };
                    let span = self.char_span();
                    let d = self
                        .diag("K0201", span)
                        .arg("found", c.to_string())
                        .arg("expected", ascii)
                        .fix(Fix::machine("use the ASCII form", span, ascii));
                    self.report(d);
                }
                self.i += 1;
                let flags = TokenFlags {
                    arabic_glyph: true,
                    ..TokenFlags::default()
                };
                self.push(TokenKind::Op(op), start, flags);
                return;
            }
            '\u{066C}' => {
                let span = self.char_span();
                let d = self.diag("SEC0123", span).fix(Fix::machine(
                    "use `_` to group digits",
                    span,
                    "_",
                ));
                self.report(d);
                self.i += 1;
                return;
            }
            _ => {}
        }

        // Known mistakes from other languages (§21.3): `&&`, `||`, `!x`. Each is reported
        // and lexed as the keyword it stands for, so parsing continues normally.
        let mistake = match (c, n1) {
            ('&', Some('&')) => Some(("&&", Concept::KwAnd, 2)),
            ('|', Some('|')) => Some(("||", Concept::KwOr, 2)),
            ('!', next) if next != Some('=') => Some(("!", Concept::KwNot, 1)),
            _ => None,
        };
        if let Some((found, concept, len)) = mistake {
            let span = Span::new(start, self.offset(self.i + len));
            let expected = self.table.spelling(concept, self.skin).to_string();
            let insert = if concept == Concept::KwNot {
                format!("{expected} ")
            } else {
                expected.clone()
            };
            let d = self
                .diag("K0911", span)
                .arg("found", found)
                .arg("expected", expected)
                .fix(Fix::machine("use the keyword", span, insert));
            self.report(d);
            self.i += len;
            self.push(TokenKind::Keyword(concept), start, TokenFlags::default());
            return;
        }

        let (op, len) = match (c, n1) {
            ('/', Some('/')) => (Op::SlashSlash, 2),
            ('/', Some('=')) => (Op::SlashEq, 2),
            ('=', Some('=')) => (Op::EqEq, 2),
            ('!', Some('=')) => (Op::NotEq, 2),
            ('<', Some('=')) => (Op::Le, 2),
            ('>', Some('=')) => (Op::Ge, 2),
            ('+', Some('=')) => (Op::PlusEq, 2),
            ('-', Some('=')) => (Op::MinusEq, 2),
            ('*', Some('=')) => (Op::StarEq, 2),
            ('-', Some('>')) => (Op::Arrow, 2),
            ('=', Some('>')) => (Op::FatArrow, 2),
            ('.', Some('.')) => {
                if n2 == Some('=') {
                    (Op::DotDotEq, 3)
                } else {
                    (Op::DotDot, 2)
                }
            }
            ('+', _) => (Op::Plus, 1),
            ('-', _) => (Op::Minus, 1),
            ('*', _) => (Op::Star, 1),
            ('/', _) => (Op::Slash, 1),
            ('%', _) => (Op::Percent, 1),
            ('=', _) => (Op::Eq, 1),
            ('<', _) => (Op::Lt, 1),
            ('>', _) => (Op::Gt, 1),
            ('?', _) => (Op::Question, 1),
            ('.', _) => (Op::Dot, 1),
            (',', _) => (Op::Comma, 1),
            (':', _) => (Op::Colon, 1),
            ('(', _) => (Op::LParen, 1),
            (')', _) => (Op::RParen, 1),
            ('[', _) => (Op::LBracket, 1),
            (']', _) => (Op::RBracket, 1),
            ('{', _) => (Op::LBrace, 1),
            ('}', _) => (Op::RBrace, 1),
            ('@', _) => (Op::At, 1),
            ('|', _) => (Op::Pipe, 1),
            _ => {
                // Not an operator: a §5.1 class problem, or simply unexpected.
                if !self.check_text_char(c, Context::Code) {
                    let d = self
                        .diag("K0105", self.char_span())
                        .arg("cp", code_point(c))
                        .arg("name", display_name(c));
                    self.report(d);
                }
                self.i += 1;
                return;
            }
        };
        match op {
            Op::LParen | Op::LBracket | Op::LBrace => self.depth += 1,
            Op::RParen | Op::RBracket | Op::RBrace => self.depth = self.depth.saturating_sub(1),
            _ => {}
        }
        self.i += len;
        self.push(TokenKind::Op(op), start, TokenFlags::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_EN: &str = include_str!("../../../testdata/skins/hello.kl");
    const HELLO_AR: &str = include_str!("../../../testdata/skins/hello.ar.kl");
    const HELLO_FRANCO: &str = include_str!("../../../testdata/skins/hello.franco.kl");
    const FIZZ_EN: &str = include_str!("../../../testdata/skins/fizzbuzz.kl");
    const FIZZ_AR: &str = include_str!("../../../testdata/skins/fizzbuzz.ar.kl");
    const FIZZ_FRANCO: &str = include_str!("../../../testdata/skins/fizzbuzz.franco.kl");

    fn kinds(text: &str, skin: Skin) -> Vec<TokenKind> {
        let out = lex(text, skin, FileId(0));
        assert!(
            out.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            out.diagnostics
        );
        out.kinds()
    }

    fn codes(text: &str, skin: Skin) -> Vec<&'static str> {
        lex(text, skin, FileId(0))
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect()
    }

    use Op::*;
    use TokenKind as T;

    #[test]
    fn hello_world_tokens() {
        assert_eq!(
            kinds(HELLO_EN, Skin::En),
            vec![
                T::Keyword(Concept::KwFn),
                T::Core(Concept::CoreMain),
                T::Op(LParen),
                T::Op(RParen),
                T::Op(Colon),
                T::Newline,
                T::Indent,
                T::Core(Concept::CorePrint),
                T::Op(LParen),
                T::Str,
                T::Op(RParen),
                T::Newline,
                T::Dedent,
                T::Eof,
            ]
        );
    }

    #[test]
    fn three_skins_give_identical_token_kinds() {
        let en = kinds(HELLO_EN, Skin::En);
        assert_eq!(kinds(HELLO_AR, Skin::Ar), en);
        assert_eq!(kinds(HELLO_FRANCO, Skin::Franco), en);

        let en = kinds(FIZZ_EN, Skin::En);
        assert_eq!(kinds(FIZZ_AR, Skin::Ar), en);
        assert_eq!(kinds(FIZZ_FRANCO, Skin::Franco), en);
    }

    #[test]
    fn ranges_and_member_indices() {
        assert_eq!(
            kinds("1..=100\n", Skin::En),
            vec![T::Int, T::Op(DotDotEq), T::Int, T::Newline, T::Eof]
        );
        assert_eq!(
            kinds("t.0.1\n", Skin::En),
            vec![
                T::Ident,
                T::Op(Dot),
                T::Int,
                T::Op(Dot),
                T::Int,
                T::Newline,
                T::Eof
            ]
        );
        assert_eq!(
            kinds("0x1e+5\n", Skin::En),
            vec![T::Int, T::Op(Plus), T::Int, T::Newline, T::Eof]
        );
        assert_eq!(kinds("1e5\n", Skin::En), vec![T::Float, T::Newline, T::Eof]);
        assert_eq!(
            kinds("1.5e-3\n", Skin::En),
            vec![T::Float, T::Newline, T::Eof]
        );
    }

    #[test]
    fn digit_leading_words_are_k0127_with_did_you_mean() {
        assert_eq!(codes("3ala\n", Skin::Franco), vec!["K0127"]);
        let out = lex("5ali x = 1\n", Skin::Franco, FileId(0));
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code, "K0127");
        assert_eq!(out.diagnostics[0].fixes[0].edits[0].insert, "khali");
        for bad in ["7f\n", "1e5x\n", "0x1Fg\n"] {
            assert_eq!(codes(bad, Skin::En), vec!["K0127"], "{bad}");
        }
    }

    #[test]
    fn numeric_literal_rules() {
        assert_eq!(codes("007\n", Skin::En), vec!["K0125"]);
        assert_eq!(codes("\u{0661}\u{0662}\n", Skin::En), vec!["SEC0122"]);
        assert_eq!(codes("\u{0661}\u{0662}\n", Skin::Ar), Vec::<&str>::new());
        assert_eq!(codes("1\u{0662}3\n", Skin::Ar), vec!["SEC0120"]);
        assert_eq!(codes("\u{06F1}\n", Skin::Ar), vec!["SEC0121"]);
        assert_eq!(codes("1\u{066C}5\n", Skin::En), vec!["SEC0123"]);
        assert_eq!(
            kinds("\u{0661}\u{066B}\u{0665}\n", Skin::Ar),
            vec![T::Float, T::Newline, T::Eof]
        );
    }

    #[test]
    fn int_literal_values_and_the_i64_range() {
        // D18: `Int` is a true 64-bit integer; K0126 uses these helpers in `Int` context.
        assert_eq!(int_literal_value("0"), Some(0));
        assert_eq!(int_literal_value("1_000_000"), Some(1_000_000));
        assert_eq!(int_literal_value("\u{0661}\u{0662}"), Some(12));
        assert_eq!(int_literal_value("\u{0661}_\u{0662}"), Some(12));
        assert_eq!(int_literal_value("0xff"), Some(255));
        assert_eq!(int_literal_value("0b1010"), Some(10));
        assert_eq!(int_literal_value("0o17"), Some(15));
        assert_eq!(int_literal_value("1.5"), None);
        assert_eq!(int_literal_value("1e5"), None);
        assert_eq!(int_literal_value("3ala"), None);
        // Tokens the lexer rejects have no value: Persian digits, mixed digit systems,
        // and a radix prefix after an Arabic-Indic zero.
        assert_eq!(int_literal_value("\u{06F1}\u{06F2}"), None);
        assert_eq!(int_literal_value("1\u{0662}3"), None);
        assert_eq!(int_literal_value("\u{0660}xff"), None);
        assert_eq!(int_literal_value("\u{0660}"), Some(0));
        // Larger than u128: no value, so it can only be a K0126 (or a BigInt via typeck).
        assert_eq!(int_literal_value(&"9".repeat(40)), None);

        let max = int_literal_value("9223372036854775807").unwrap();
        let min_mag = int_literal_value("9223372036854775808").unwrap();
        assert!(int_literal_fits_int(max, false));
        assert!(int_literal_fits_int(max, true));
        assert!(!int_literal_fits_int(min_mag, false)); // 2^63 is out of range...
        assert!(int_literal_fits_int(min_mag, true)); // ...but -2^63 is i64::MIN
        assert!(!int_literal_fits_int(min_mag + 1, true));
        assert!(int_literal_fits_int(
            int_literal_value("0x7fff_ffff_ffff_ffff").unwrap(),
            false
        ));
        assert!(!int_literal_fits_int(
            int_literal_value("0x8000_0000_0000_0000").unwrap(),
            false
        ));
        // The old 53-bit limit is gone: 2^53 is an ordinary Int literal.
        assert!(int_literal_fits_int(
            int_literal_value("9007199254740992").unwrap(),
            false
        ));
        // The lexer itself never rejects a large literal (it may be a BigInt).
        assert_eq!(
            codes("99999999999999999999999\n", Skin::En),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn persian_and_urdu_letters_are_folded_not_rejected() {
        // D19: ی ک ە ھ ۀ are ordinary Arabic-script letters in identifiers...
        for word in [
            "\u{06A9}تاب",
            "عل\u{06CC}",
            "\u{06BE}ذا",
            "\u{06D5}",
            "خان\u{06C0}",
        ] {
            let text = format!("{word}\n");
            assert_eq!(
                kinds(&text, Skin::Ar),
                vec![T::Ident, T::Newline, T::Eof],
                "{word}"
            );
            assert_eq!(
                kinds(&text, Skin::En),
                vec![T::Ident, T::Newline, T::Eof],
                "{word}"
            );
        }
        // ...and in an Arabic file they match keywords after folding, flagged for `fmt`.
        let out = lex("ل\u{06A9}ل س ف\u{06CC} ق\n", Skin::Ar, FileId(0));
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(out.tokens[0].kind, T::Keyword(Concept::KwFor));
        assert!(out.tokens[0].flags.variant_spelling);
        assert_eq!(out.tokens[2].kind, T::Keyword(Concept::KwIn));
        assert!(out.tokens[2].flags.variant_spelling);
    }

    #[test]
    fn trojan_source_characters_are_rejected_everywhere() {
        assert_eq!(codes("x = 1 # \u{202E}note\n", Skin::En), vec!["SEC0110"]);
        assert_eq!(codes("x = \"a\u{2067}b\"\n", Skin::En), vec!["SEC0110"]);
        assert_eq!(codes("x\u{200B}y\n", Skin::En), vec!["SEC0111"]);
        assert_eq!(codes("\u{FEFF}x\n", Skin::En), vec!["SEC0100"]);
        assert_eq!(codes("x\u{00A0}= 1\n", Skin::En), vec!["SEC0103"]);
    }

    #[test]
    fn layout_and_bracket_suppression() {
        assert_eq!(
            kinds("if x:\n    y\nz\n", Skin::En),
            vec![
                T::Keyword(Concept::KwIf),
                T::Ident,
                T::Op(Colon),
                T::Newline,
                T::Indent,
                T::Ident,
                T::Newline,
                T::Dedent,
                T::Ident,
                T::Newline,
                T::Eof,
            ]
        );
        assert_eq!(
            kinds("f(\n    1,\n    2,\n)\n", Skin::En),
            vec![
                T::Ident,
                T::Op(LParen),
                T::Int,
                T::Op(Comma),
                T::Int,
                T::Op(Comma),
                T::Op(RParen),
                T::Newline,
                T::Eof,
            ]
        );
        assert_eq!(codes("if x:\n\ty\n", Skin::En), vec!["K0103"]);
        assert_eq!(codes("if x:\n  y\n", Skin::En), vec!["K0120"]);
    }

    #[test]
    fn skins_keep_their_own_words() {
        // `law` is Franco for `if`; in an English file that declares it, it is an ordinary
        // identifier.
        assert_eq!(
            kinds("let law = 1\n", Skin::En),
            vec![
                T::Keyword(Concept::KwLet),
                T::Ident,
                T::Op(Op::Eq),
                T::Int,
                T::Newline,
                T::Eof
            ]
        );
        assert_eq!(
            kinds("law\n", Skin::Franco),
            vec![T::Keyword(Concept::KwIf), T::Newline, T::Eof]
        );
        // After `.`, a reserved word is a member name in every skin.
        assert_eq!(
            kinds("contract.law\n", Skin::Franco),
            vec![T::Ident, T::Op(Dot), T::Ident, T::Newline, T::Eof]
        );
        // Arabic matching uses arabic_fold, and the variant is flagged for `fmt`.
        let out = lex("اذا\n", Skin::Ar, FileId(0));
        assert_eq!(out.tokens[0].kind, T::Keyword(Concept::KwIf));
        assert!(out.tokens[0].flags.variant_spelling);
        // `،` is a comma in Arabic files only.
        assert_eq!(
            kinds("f(a\u{060C} b)\n", Skin::Ar),
            vec![
                T::Ident,
                T::Op(LParen),
                T::Ident,
                T::Op(Comma),
                T::Ident,
                T::Op(RParen),
                T::Newline,
                T::Eof
            ]
        );
        assert_eq!(codes("f(a\u{060C} b)\n", Skin::En), vec!["K0201"]);
    }

    /// The single K0201 for `text`, or a panic with all diagnostics.
    fn one_k0201(text: &str, skin: Skin) -> (LexOutput, Diagnostic) {
        let out = lex(text, skin, FileId(0));
        let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, vec!["K0201"], "{text:?}");
        let d = out.diagnostics[0].clone();
        (out, d)
    }

    fn arg<'d>(d: &'d Diagnostic, name: &str) -> &'d str {
        d.args
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }

    #[test]
    fn keyword_from_another_skin_is_k0201_with_a_machine_fix() {
        // Spec §5.4 bullet 3, §21.4 rule 6. Each case: (source, skin, found, expected).
        let cases = [
            ("if صح:\n", Skin::Ar, "if", "إذا"),
            ("law صح:\n", Skin::Ar, "law", "إذا"),
            ("إذا x:\n", Skin::En, "إذا", "if"),
            ("إذا sa7:\n", Skin::Franco, "إذا", "law"),
            ("if x:\n", Skin::Franco, "if", "law"),
            ("for x في xs:\n", Skin::Ar, "for", "لكل"),
        ];
        for (src, skin, found, expected) in cases {
            let (out, d) = one_k0201(src, skin);
            assert_eq!(arg(&d, "found"), found, "{src:?}");
            assert_eq!(arg(&d, "expected"), expected, "{src:?}");
            // The fix replaces exactly the word, and is safe to apply without review.
            assert_eq!(d.fixes.len(), 1);
            let fix = &d.fixes[0];
            assert_eq!(fix.applicability, kalam_diagnostics::Applicability::Machine);
            assert_eq!(fix.edits.len(), 1);
            assert_eq!(&src[fix.edits[0].span.range()], found);
            assert_eq!(fix.edits[0].insert, expected);
            // The word lexes as the keyword, so the parser recovers without a cascade.
            assert!(
                matches!(
                    out.tokens[0].kind,
                    T::Keyword(Concept::KwIf | Concept::KwFor)
                ),
                "{src:?}"
            );
        }
        // Every English word in an Arabic file gets its own fix.
        let out = lex("for x in xs:\n", Skin::Ar, FileId(0));
        let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, vec!["K0201", "K0201"]);
        assert_eq!(out.tokens[2].kind, T::Keyword(Concept::KwIn));
        // The message renders the file-skin spelling.
        let (_, d) = one_k0201("if صح:\n", Skin::Ar);
        assert!(d.message(Skin::En).contains("إذا"));
    }

    /// THE RULE (spec §5.4 bullets 2–3, §5.3 rules 7–8, §4.3, §21.4 rule 6; eval P8).
    ///
    /// Another skin's keyword, core name or Franco variant is K0201 wherever reading it as a
    /// name would be a syntax error or an undefined name. It is a legal identifier when:
    /// 1. the file declares it: `let`/`var`, `fn` name and parameters, `for` and `case`
    ///    patterns, `type`/`enum`/`trait` names, generic parameters, `use` items, `as`
    ///    aliases. (No globs or globals, §21.2, so an undeclared name is undefined.)
    /// 2. it is in a §5.3 rule 8 position: after `.` or `@`, a label `(word: …)`, a field
    ///    name in a `type` body, a variant name in an `enum` body;
    /// 3. it is written in backticks (§5.3 rule 7);
    /// 4. it is inside a string or a comment (text, not code).
    ///
    /// Interpolation-hole code is not checked by this rule (see
    /// `holes_are_not_checked_for_undefined_names`).
    #[test]
    fn another_skins_word_used_as_a_name_is_legal() {
        for (src, skin) in [
            // 1. declared
            ("let law = 1\nprint(law)\n", Skin::En),
            ("var law = 1\nlaw = law + 1\n", Skin::En),
            ("fn law(x: Int):\n    law(x)\n", Skin::En),
            ("fn f(law: Int, fi: Int):\n    print(law + fi)\n", Skin::En),
            ("let g = fn(law) => law\n", Skin::En),
            ("for law in xs:\n    print(law)\n", Skin::En),
            ("let (we, aw) = t\nprint(we + aw)\n", Skin::En),
            ("match x:\n    case Some(law): print(law)\n", Skin::En),
            ("type law:\n    a: Int\n", Skin::En),
            ("enum hat:\n    A\n", Skin::En),
            ("fn f[ka](x: ka) -> ka:\n    return x\n", Skin::En),
            ("use std/io.{law}\nlaw()\n", Skin::En),
            ("use std/io as law\nlaw.f()\n", Skin::En),
            ("let law = 1\nlet y = law\n", Skin::En),
            ("ليكن if = 1\nاطبع(if)\n", Skin::Ar),
            ("دالة print(t: نص):\n    اطبع(t)\nprint(\"x\")\n", Skin::Ar),
            ("khali let = 1\nektb(let)\n", Skin::Franco),
            ("let إذا = 1\nprint(إذا)\n", Skin::En),
            // 2. rule 8 positions
            ("contract.law\n", Skin::En),
            ("contract.if x\n", Skin::Ar),
            ("f(if: 1, law: 2)\n", Skin::Ar),
            ("@law\nfn f():\n    pass\n", Skin::En),
            ("type R:\n    law: Int\n    walla: Int\n", Skin::En),
            ("enum E:\n    law\n    fot(x: Int)\n", Skin::En),
            // 3. backticks
            ("`law` صح\n", Skin::Ar),
            ("let x = `law`\n", Skin::En),
            // 4. strings and comments
            ("# if for law\nprint(\"if x: law\")\n", Skin::En),
        ] {
            let out = lex(src, skin, FileId(0));
            assert!(
                out.diagnostics.iter().all(|d| d.code != "K0201"),
                "{src:?}: {:?}",
                out.diagnostics
            );
        }
    }

    #[test]
    fn every_other_skin_word_is_k0201_in_every_direction() {
        // (source, skin, found, expected): keywords, core names and Franco variants, used
        // where they are not declared names.
        let cases = [
            // Franco in English
            ("fn main():\n    kfaya\n", Skin::En, "kfaya", "break"),
            ("fn main():\n    khalas\n", Skin::En, "khalas", "break"),
            ("fn main():\n    let x = sa7\n", Skin::En, "sa7", "true"),
            ("fn main():\n    ektb(\"hi\")\n", Skin::En, "ektb", "print"),
            ("fn main():\n    let x = mafeesh\n", Skin::En, "mafeesh", "None"),
            ("fn f() -> adad:\n    return 1\n", Skin::En, "adad", "Int"),
            ("fn main():\n    foot\n", Skin::En, "foot", "pass"),
            // English in Franco
            ("dala ra2isi():\n    khali x = true\n", Skin::Franco, "true", "sa7"),
            ("dala ra2isi():\n    pass\n", Skin::Franco, "pass", "fot"),
            ("dala ra2isi():\n    print(\"x\")\n", Skin::Franco, "print", "ektb"),
            ("dala f(n: String):\n    fot\n", Skin::Franco, "String", "nass"),
            ("dala ra2isi():\n    motghayer i = 0\n    tool_ma i < 3:\n        kfaya\n    else:\n        fot\n", Skin::Franco, "else", "walla"),
            // English and Franco in Arabic
            ("دالة رئيسي():\n    print(\"x\")\n", Skin::Ar, "print", "اطبع"),
            ("دالة رئيسي():\n    ليكن x: Int = 1\n", Skin::Ar, "Int", "عدد"),
            ("دالة رئيسي():\n    ليكن x = Some(1)\n", Skin::Ar, "Some", "موجود"),
            ("دالة رئيسي():\n    ليكن x = true\n", Skin::Ar, "true", "صح"),
            ("دالة رئيسي():\n    break\n", Skin::Ar, "break", "اخرج"),
            ("دالة رئيسي():\n    ektb(1)\n", Skin::Ar, "ektb", "اطبع"),
            ("دالة رئيسي():\n    ليكن x = sa7\n", Skin::Ar, "sa7", "صح"),
            // Arabic in English and Franco
            ("fn main():\n    اطبع(\"x\")\n", Skin::En, "اطبع", "print"),
            ("fn main():\n    let x = صح\n", Skin::En, "صح", "true"),
            ("dala ra2isi():\n    اطبع(1)\n", Skin::Franco, "اطبع", "ektb"),
        ];
        for (src, skin, found, expected) in cases {
            let out = lex(src, skin, FileId(0));
            let k: Vec<_> = out
                .diagnostics
                .iter()
                .filter(|d| d.code == "K0201")
                .collect();
            assert_eq!(k.len(), 1, "{src:?} ({skin:?}): {:?}", out.diagnostics);
            assert_eq!(arg(k[0], "found"), found, "{src:?}");
            assert_eq!(arg(k[0], "expected"), expected, "{src:?}");
            let fix = &k[0].fixes[0];
            assert_eq!(fix.applicability, kalam_diagnostics::Applicability::Machine);
            assert_eq!(&src[fix.edits[0].span.range()], found);
            assert_eq!(fix.edits[0].insert, expected);
        }
        // Core names and statement keywords become the concept, so parsing recovers.
        let out = lex("دالة رئيسي():\n    print(\"x\")\n", Skin::Ar, FileId(0));
        assert!(out.kinds().contains(&T::Core(Concept::CorePrint)));
        let out = lex("fn main():\n    kfaya\n", Skin::En, FileId(0));
        assert!(out.kinds().contains(&T::Keyword(Concept::KwBreak)));
        // In operand position (`x = law`) the word stays a name: one error, no cascade.
        let out = lex("fn main():\n    let x = law\n", Skin::En, FileId(0));
        assert_eq!(
            codes("fn main():\n    let x = law\n", Skin::En),
            vec!["K0201"]
        );
        assert!(!out.kinds().contains(&T::Keyword(Concept::KwIf)));
        // A foreign `in`/`if` that ends a `for`/`case` pattern is a keyword, not a binding of
        // that pattern: reported and converted, so the parser does not cascade.
        let out = lex(
            "دالة رئيسي():\n    for x in [1، 2]:\n        اطبع(x)\n",
            Skin::Ar,
            FileId(0),
        );
        assert_eq!(
            out.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["K0201", "K0201"]
        );
        assert!(out.kinds().contains(&T::Keyword(Concept::KwIn)));
        let src = "dala ra2isi():\n    hasab v:\n        lama x if x > 1: fot\n";
        assert_eq!(codes(src, Skin::Franco), vec!["K0201"]);
        // Diagnostics stay in source order.
        let out = lex("fn main():\n    x = 007\n    kfaya\n", Skin::En, FileId(0));
        let order: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(order, vec!["K0125", "K0201"]);
    }

    #[test]
    fn holes_are_not_checked_for_undefined_names() {
        // Hole code is re-lexed alone (by the parser, KCF and convert) and cannot see the
        // file's declarations, so a declared `law` used in `"{law}"` must not be K0201.
        // One-line fragments therefore skip the undefined-name check. KNOWN GAP: an
        // undeclared other-skin word inside a hole (`"{kfaya}"`) is not reported yet.
        assert!(codes("law", Skin::En).is_empty());
        assert!(codes("x + law", Skin::En).is_empty());
        assert!(codes(
            "fn main():\n    let law = 1\n    print(\"{law}\")\n",
            Skin::En
        )
        .is_empty());
        assert!(codes("fn main():\n    print(\"{kfaya}\")\n", Skin::En).is_empty());
    }

    #[test]
    fn zero_width_joiners_inside_a_name_keep_one_token() {
        for (src, cp) in [
            ("let a\u{200D}b = 1\n", "U+200D"),
            ("let a\u{200C}b = 1\n", "U+200C"),
        ] {
            let out = lex(src, Skin::En, FileId(0));
            let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
            assert_eq!(codes, vec!["SEC0111"], "{src:?}");
            let d = &out.diagnostics[0];
            assert_eq!(arg(d, "cp"), cp);
            assert_eq!(
                d.fixes[0].applicability,
                kalam_diagnostics::Applicability::Machine
            );
            assert_eq!(d.fixes[0].edits[0].insert, "");
            assert_eq!(&src[d.fixes[0].edits[0].span.range()], &src[5..8]);
            assert_eq!(
                out.kinds(),
                vec![
                    T::Keyword(Concept::KwLet),
                    T::Ident,
                    T::Op(Op::Eq),
                    T::Int,
                    T::Newline,
                    T::Eof
                ]
            );
            assert_eq!(&src[out.tokens[1].span.range()], &src[4..9]);
        }
        // Arabic words join letters with ZWNJ in some typing habits: still one name.
        let out = lex("خلي ك\u{200C}تاب = 1\n", Skin::Ar, FileId(0));
        assert_eq!(
            out.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["SEC0111"]
        );
        assert_eq!(out.tokens[1].kind, T::Ident);
        assert_eq!(out.tokens[2].kind, T::Op(Op::Eq));
        // A joiner hidden inside a keyword still reads as the keyword it looks like.
        let out = lex("i\u{200D}f x:\n", Skin::En, FileId(0));
        assert_eq!(
            out.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["SEC0111"]
        );
        assert_eq!(out.tokens[0].kind, T::Keyword(Concept::KwIf));
        assert!(out.tokens[0].flags.variant_spelling);
        // A trailing joiner (not between name characters) is reported on its own.
        let out = lex("a\u{200D} = 1\n", Skin::En, FileId(0));
        assert_eq!(
            out.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec!["SEC0111"]
        );
        assert_eq!(
            out.kinds(),
            vec![T::Ident, T::Op(Op::Eq), T::Int, T::Newline, T::Eof]
        );
    }

    #[test]
    fn strings_comments_and_tags() {
        assert_eq!(
            kinds("let q = sql\"SELECT 1\" # trailing\n", Skin::En),
            vec![
                T::Keyword(Concept::KwLet),
                T::Ident,
                T::Op(Op::Eq),
                T::TaggedStr,
                T::Newline,
                T::Eof,
            ]
        );
        assert_eq!(codes("\"open\n", Skin::En), vec!["K0129"]);
        assert_eq!(codes("\"a\\qb\"\n", Skin::En), vec!["K0130"]);
        assert_eq!(codes("x = 1;\n", Skin::En), vec!["K0904"]);
        assert_eq!(codes("a && b\n", Skin::En), vec!["K0911"]);
    }
}
