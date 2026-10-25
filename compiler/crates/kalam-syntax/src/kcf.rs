//! Kalam Canonical Form (spec §4.4): a skin-free serialization of the CST. Its SHA-256
//! ([`KcfHash`]) is the unit of identity: it is signed, recorded in `kalam.lock`, stored by
//! the registry, and compared by parity gate P1.
//!
//! Two files that are conversions of each other (spec §4.3) have byte-identical KCF
//! (guarantee G1), and so does a file and its `kalam fmt` output (`kcf(fmt(x)) == kcf(x)`).
//!
//! # What is recorded
//!
//! - keywords and core names → their concept ID (`KW_IF`, `CORE_PRINT`), whatever the
//!   spelling (canonical, hamza/alef variant, Franco alias);
//! - a reserved word in a §5.3 rule 8 position (field or method after `.`, a named-argument
//!   or field-pattern label, a record field name, a `use` path segment or item, an attribute
//!   name) → an identifier with its own text, exactly as the parser's AST reads it;
//! - identifiers → their exact code points (no folding), without raw-identifier backticks;
//! - `،`/`,` → `PUNCT_COMMA`, `؟`/`?` → `PUNCT_TRY`, other punctuation as ASCII;
//! - numbers → kind (int with its radix, or float) plus exact value: digit glyphs, `_`
//!   separators, hex digit case and an exponent's `+` are not recorded;
//! - string and tagged literals → the text outside interpolation holes byte for byte
//!   (tag, quotes and escapes included); the code inside each `{…}` hole is re-lexed in the
//!   file's skin and serialized like any other code;
//! - comments and doc comments → byte for byte (trailing spaces trimmed), each marked as on
//!   its own line or at the end of a code line;
//! - logical-line structure → the parser's `NEWLINE`/`INDENT`/`DEDENT` tokens; a one-line
//!   block (`if x: f()`, `fn f(): return 1`, `case 1: g()`) is serialized exactly like its
//!   indented form (`:` `NL` `IN` … `NL` `DE`);
//! - blank lines → only "at least one blank line here" (a run of blank lines is one `B`), and
//!   only where `fmt` keeps them: between two statements or members of one block, and between
//!   comments there.
//!
//! Not recorded, because `fmt` may change it and it carries no meaning (spec §4.4, KCF2,
//! FOUNDER-QUEUE Q8): spaces between tokens, indentation bytes (the INDENT/DEDENT tokens
//! carry the structure), `\r`, line breaks inside brackets (§5.8 bracket suppression), a
//! missing final newline, and:
//!
//! - blank lines inside brackets, at the start of a block (before its first comment or
//!   statement), after an attribute line, before an `else`, at the top level of the file
//!   (`fmt` puts exactly one between items itself) and at the end of the file;
//! - the number of blank lines in a run;
//! - a trailing comma before a closing bracket (`f(a, b,)`, `[1, 2,]`), except in a
//!   one-element tuple (`(x,)`), where the comma is what makes it a tuple;
//! - whether a block was written on its header line or indented (an inline `if`
//!   *expression*, `if c: a else: b`, is not a block and keeps its form).
//!
//! The order of a comment and the INDENT/DEDENT records next to it is not recorded either:
//! the comment's column decides which block `fmt` puts it in, and KCF has no columns. Layout
//! records are written as they come; own-line comments and blank lines wait for the next
//! code token, which decides which blank lines count.
//!
//! CST node boundaries are not recorded: they are implied by the tokens, and leaving them
//! out keeps hashes stable when the CST gets finer.
//!
//! # Byte format (version 2)
//!
//! UTF-8. The magic [`MAGIC`] (`KCF2\n`), then one record per line: a tag, then for most
//! tags a space and a payload. Payloads that hold source text are length-prefixed
//! (`<byte length>:<bytes>`), so identifier or string text can never be mistaken for
//! structure, even when it contains a newline.
//!
//! | Record | Meaning |
//! |---|---|
//! | `K KW_IF` | keyword or core name (concept ID) |
//! | `I 4:name` | identifier |
//! | `P (` · `P PUNCT_COMMA` | punctuation |
//! | `N i 16 31` | integer literal: radix, exact decimal value |
//! | `N f 1.5e-3` | float literal: ASCII digits, no `_`, lowercase `e`, no exponent `+` |
//! | `N ? 3:3ab` | a malformed number (only in files with lexer errors) |
//! | `S` … `/S` | a string or tagged literal, holding `T` and `H` records |
//! | `T 7:"Hello ` | literal text outside holes, byte for byte |
//! | `H` … `/H` | one interpolation hole, holding its code's records |
//! | `R 3:a b` | hole code nested too deep to re-lex (only in invalid files) |
//! | `# own 6:# note` · `# end …` | comment on its own line, or after code |
//! | `## own 8:## docs` | doc comment |
//! | `NL` · `IN` · `DE` | logical NEWLINE, INDENT, DEDENT |
//! | `B` | at least one blank line |
//! | `X 1:;` | source bytes that are neither tokens, comments nor spaces (lexer errors) |
//!
//! Everything is computed with explicit loops over a flat token list (no recursion over the
//! tree), so a tree at the parser's height cap serializes on a 1 MiB stack.

use std::fmt;

use kalam_base::{FileId, Skin};
use kalam_lexer::{TokenKind, TriviaKind};
use kalam_skins::{is_tatweel_or_haraka, Concept, KeywordTable};

use crate::{Cst, GreenNode, SyntaxKind};

/// The first bytes of every KCF serialization (format version 2: layout `fmt` may change is
/// not recorded, spec §4.4).
pub const MAGIC: &[u8] = b"KCF2\n";

/// Strings nest inside interpolation holes only in invalid files (a string literal inside a
/// hole is K0131); past this depth the hole code is recorded raw.
const MAX_HOLE_DEPTH: u32 = 16;

/// SHA-256 of the KCF bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KcfHash(pub [u8; 32]);

impl KcfHash {
    /// The hash of already serialized KCF bytes.
    pub fn of(kcf: &[u8]) -> KcfHash {
        KcfHash(crate::sha256::digest(kcf))
    }

    /// Lowercase hex (64 digits), as `kalam.lock` and review tools show it.
    pub fn to_hex(&self) -> String {
        crate::sha256::hex(&self.0)
    }
}

impl fmt::Display for KcfHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Serializes a CST skin-free. The file's skin (needed only to re-lex interpolation holes)
/// is inferred from its keyword, core-name, punctuation and digit spellings; use
/// [`serialize_in`] when the skin is known.
pub fn serialize(cst: &Cst) -> Vec<u8> {
    let leaves = leaves(&cst.root);
    let skin = infer_skin_of(&leaves);
    serialize_leaves(&leaves, skin)
}

/// Serializes a CST whose file is written in `skin`.
pub fn serialize_in(cst: &Cst, skin: Skin) -> Vec<u8> {
    serialize_leaves(&leaves(&cst.root), skin)
}

/// The KCF hash of a CST (SHA-256 of [`serialize`]).
pub fn hash(cst: &Cst) -> KcfHash {
    KcfHash::of(&serialize(cst))
}

/// The skin a CST was parsed in, as far as its spellings tell: a keyword or core-name
/// spelling, `،`/`؟`, or an Arabic-Indic digit. A file with none of these (no keywords at
/// all) is reported as `en`; its KCF does not depend on the skin unless a hole holds a
/// keyword, which then would also be the only keyword in the file.
pub fn infer_skin(cst: &Cst) -> Skin {
    infer_skin_of(&leaves(&cst.root))
}

// ----- flat token list --------------------------------------------------------------------

/// One CST token with the kind of the node that holds it.
#[derive(Clone, Copy)]
struct Leaf<'a> {
    kind: SyntaxKind,
    text: &'a str,
    parent: SyntaxKind,
}

fn leaves(root: &GreenNode) -> Vec<Leaf<'_>> {
    root.tokens()
        .into_iter()
        .map(|(parent, t)| Leaf {
            kind: t.kind,
            text: &t.text,
            parent,
        })
        .collect()
}

/// Tokens that take part in the grammar: everything except spaces, comments and the empty
/// tokens the parser inserts for missing input. Layout tokens are zero-width but count.
fn significant(l: &Leaf) -> bool {
    match l.kind {
        SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::DocComment => false,
        SyntaxKind::Newline | SyntaxKind::Indent | SyntaxKind::Dedent => true,
        _ => !l.text.is_empty(),
    }
}

fn is_punct(l: Option<&Leaf>, text: &str) -> bool {
    matches!(l, Some(l) if l.kind == SyntaxKind::Punct && l.text == text)
}

fn is_comma(l: Option<&Leaf>) -> bool {
    is_punct(l, ",") || is_punct(l, "\u{060C}")
}

fn is_layout(l: Option<&Leaf>) -> bool {
    matches!(
        l,
        Some(l) if matches!(l.kind, SyntaxKind::Newline | SyntaxKind::Indent | SyntaxKind::Dedent)
    )
}

/// For every leaf: true if it is a keyword or core-name token in a §5.3 rule 8 position,
/// where the parser reads it as an identifier (`word_ident`). These positions mirror the
/// parser's `at_word` call sites.
fn rule_8_identifiers(leaves: &[Leaf]) -> Vec<bool> {
    let sig: Vec<usize> = (0..leaves.len())
        .filter(|&i| significant(&leaves[i]))
        .collect();
    let mut out = vec![false; leaves.len()];
    let mut open: Vec<&str> = Vec::new();
    for (k, &i) in sig.iter().enumerate() {
        let l = &leaves[i];
        if matches!(l.kind, SyntaxKind::Keyword(_) | SyntaxKind::Core(_)) {
            let prev = k.checked_sub(1).map(|p| &leaves[sig[p]]);
            let next = sig.get(k + 1).map(|&n| &leaves[n]);
            out[i] = is_rule_8(l, prev, next, open.last().copied());
        }
        if l.kind == SyntaxKind::Punct {
            match l.text {
                "(" | "[" | "{" => open.push(l.text),
                ")" | "]" | "}" => {
                    open.pop();
                }
                _ => {}
            }
        }
    }
    out
}

fn is_rule_8(l: &Leaf, prev: Option<&Leaf>, next: Option<&Leaf>, open: Option<&str>) -> bool {
    // A member name after `.` (the lexer already makes these identifiers).
    if is_punct(prev, ".") {
        return true;
    }
    let colon_next = is_punct(next, ":");
    match l.parent {
        // `use a/b/c`, `use a/{b, c}`: path segments and item names (but `self`).
        SyntaxKind::UseDecl => {
            let after_use =
                matches!(prev, Some(p) if p.kind == SyntaxKind::Keyword(Concept::KwUse));
            if after_use || is_punct(prev, "/") {
                return true;
            }
            if (is_punct(prev, "{") || is_comma(prev))
                && l.kind != SyntaxKind::Keyword(Concept::KwSelf)
            {
                return true;
            }
        }
        // `@name` (later segments follow `.`).
        SyntaxKind::Attribute if is_punct(prev, "@") => return true,
        // A record field name: first on its line (after any attributes), before `:`.
        SyntaxKind::TypeDecl => {
            let line_start =
                is_layout(prev) || matches!(prev, Some(p) if p.parent == SyntaxKind::Attribute);
            if colon_next && line_start {
                return true;
            }
        }
        _ => {}
    }
    // A label inside `(…)`: named argument, variant field, field pattern.
    open == Some("(") && colon_next && (is_punct(prev, "(") || is_comma(prev))
}

/// For every leaf: true if it is a trailing comma that KCF leaves out, i.e. a comma directly
/// before a closing bracket (comments and line breaks between them do not count), except the
/// comma of a one-element tuple `(x,)`.
fn trailing_commas(leaves: &[Leaf]) -> Vec<bool> {
    let mut out = vec![false; leaves.len()];
    // Per open bracket: (index of the opening leaf, commas directly inside it).
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut prev: Option<usize> = None;
    let mut prev_of: Vec<Option<usize>> = vec![None; leaves.len()];
    for (i, l) in leaves.iter().enumerate() {
        if !significant(l) || is_layout(Some(l)) {
            continue;
        }
        prev_of[i] = prev;
        if l.kind == SyntaxKind::Punct {
            match l.text {
                "(" | "[" | "{" => open.push((i, 0)),
                "," | "\u{060C}" => {
                    if let Some(top) = open.last_mut() {
                        top.1 += 1;
                    }
                }
                ")" | "]" | "}" => {
                    if let (Some((o, commas)), Some(p)) = (open.pop(), prev) {
                        let one_tuple = leaves[o].text == "("
                            && commas == 1
                            && !opens_a_call(prev_of[o].map(|q| &leaves[q]));
                        if is_comma(Some(&leaves[p])) && !one_tuple {
                            out[p] = true;
                        }
                    }
                }
                _ => {}
            }
        }
        prev = Some(i);
    }
    out
}

/// Whether a `(` after `prev` opens an argument or parameter list (a call, a declaration's
/// parameters, a variant's fields, a lambda's parameters) rather than a tuple or a
/// parenthesized expression.
fn opens_a_call(prev: Option<&Leaf>) -> bool {
    let Some(p) = prev else { return false };
    match p.kind {
        SyntaxKind::Ident
        | SyntaxKind::Core(_)
        | SyntaxKind::IntLit
        | SyntaxKind::FloatLit
        | SyntaxKind::StrLit
        | SyntaxKind::TaggedStrLit => true,
        SyntaxKind::Keyword(c) => matches!(c, Concept::KwFn | Concept::KwSelf),
        SyntaxKind::Punct => matches!(p.text, ")" | "]" | "}" | "?" | "\u{061F}"),
        _ => false,
    }
}

/// For every leaf: true if it is the `:` of a one-line block, which KCF writes as if the
/// block were indented. Those are a `Block` colon followed by code on the same line, and the
/// colon of a statement `if x: stmt` (parent `IfForm`). The colons of an inline `if`
/// *expression* (`if c: a else: b`) also have parent `IfForm`; they are told apart by the
/// `else` that follows on the same line, or precedes the colon.
fn one_line_block_colons(leaves: &[Leaf]) -> Vec<bool> {
    let sig: Vec<usize> = (0..leaves.len())
        .filter(|&i| significant(&leaves[i]))
        .collect();
    let mut out = vec![false; leaves.len()];
    for (k, &i) in sig.iter().enumerate() {
        let l = &leaves[i];
        if !(is_punct(Some(l), ":") && matches!(l.parent, SyntaxKind::Block | SyntaxKind::IfForm)) {
            continue;
        }
        let Some(&n) = sig.get(k + 1) else { continue };
        if is_layout(Some(&leaves[n])) {
            continue; // an indented block
        }
        if l.parent == SyntaxKind::IfForm {
            let is_inline_else = |j: usize| {
                leaves[j].kind == SyntaxKind::Keyword(Concept::KwElse)
                    && leaves[j].parent == SyntaxKind::IfForm
            };
            let after_else = k.checked_sub(1).is_some_and(|p| is_inline_else(sig[p]));
            let else_follows = sig[k + 1..]
                .iter()
                .take_while(|&&j| leaves[j].kind != SyntaxKind::Newline)
                .any(|&j| is_inline_else(j));
            if after_else || else_follows {
                continue;
            }
        }
        out[i] = true;
    }
    out
}

// ----- skin inference ---------------------------------------------------------------------

fn skin_index(skin: Skin) -> usize {
    match skin {
        Skin::En => 0,
        Skin::Ar => 1,
        Skin::Franco => 2,
    }
}

fn infer_skin_of(leaves: &[Leaf]) -> Skin {
    let table = KeywordTable::builtin();
    let ar_only = [false, true, false];
    let mut candidates = [true; 3];
    for l in leaves {
        let mask = match l.kind {
            SyntaxKind::Keyword(c) | SyntaxKind::Core(c) => {
                let word: String = l
                    .text
                    .chars()
                    .filter(|&ch| !is_tatweel_or_haraka(ch))
                    .collect();
                Skin::ALL.map(|s| table.skin(s).lookup_word(&word).map(|m| m.concept) == Some(c))
            }
            SyntaxKind::Punct if matches!(l.text, "\u{060C}" | "\u{061F}") => ar_only,
            SyntaxKind::IntLit | SyntaxKind::FloatLit
                if l.text.chars().any(kalam_lexer::is_arabic_indic_digit) =>
            {
                ar_only
            }
            _ => continue,
        };
        let narrowed = [0, 1, 2].map(|i| candidates[i] && mask[i]);
        // A token no skin explains (only in a broken file) does not wipe out the evidence.
        if narrowed.contains(&true) {
            candidates = narrowed;
        }
        if candidates.iter().filter(|&&c| c).count() == 1 {
            break;
        }
    }
    Skin::ALL
        .into_iter()
        .find(|&s| candidates[skin_index(s)])
        .unwrap_or(Skin::En)
}

// ----- writer -----------------------------------------------------------------------------

/// An own-line comment or a blank line, waiting for the next code token.
enum GapItem {
    Blank,
    /// A serialized `#`/`##` record.
    Comment(Vec<u8>),
}

/// Which blank lines of a gap (the own-line comments and blank lines before a code token)
/// KCF records. These mirror the formatter's rules (spec §21.1 rule 4), so that
/// `kcf(fmt(x)) == kcf(x)`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Blanks {
    /// Between statements or members of one block: `fmt` keeps one blank line wherever
    /// there was at least one.
    Keep,
    /// At the start of a block, and after an attribute line: `fmt` drops the blank lines
    /// before the first comment or code line, and keeps the others.
    DropFirst,
    /// At the top level, before `else` and at the end of the file: `fmt` decides alone.
    /// Which of them it keeps depends on comment columns, which KCF does not record.
    DropAll,
}

struct Ser {
    out: Vec<u8>,
    skin: Skin,
    /// The current physical line already has a token or a comment.
    line_has_content: bool,
    /// Own-line comments and blank lines since the last code token.
    gap: Vec<GapItem>,
    /// INDENT depth, including the blocks that one-line blocks are written as.
    indent: usize,
    /// Bracket depth: blank lines inside brackets are not recorded.
    brackets: usize,
    /// An INDENT was written and no code token has followed it yet.
    block_start: bool,
    /// The next code token is the first of its logical line.
    at_line_start: bool,
    /// The current logical line starts with `@`.
    line_is_attr: bool,
    /// The previous logical line started with `@`.
    after_attr: bool,
    /// One-line blocks opened on the current line: each gets a DEDENT after its NEWLINE.
    open_one_line: usize,
}

fn serialize_leaves(leaves: &[Leaf], skin: Skin) -> Vec<u8> {
    let mut s = Ser {
        out: MAGIC.to_vec(),
        skin,
        line_has_content: false,
        gap: Vec::new(),
        indent: 0,
        brackets: 0,
        block_start: false,
        at_line_start: true,
        line_is_attr: false,
        after_attr: false,
        open_one_line: 0,
    };
    s.leaves(leaves, 0);
    s.flush(Blanks::DropAll);
    if s.open_one_line > 0 {
        // A one-line block on a last line without a NEWLINE (only in broken input).
        s.newline();
    }
    s.out
}

impl Ser {
    fn rec(&mut self, tag: &str) {
        self.out.extend_from_slice(tag.as_bytes());
        self.out.push(b'\n');
    }

    fn word(&mut self, tag: &str, word: &str) {
        self.out.extend_from_slice(tag.as_bytes());
        self.out.push(b' ');
        self.out.extend_from_slice(word.as_bytes());
        self.out.push(b'\n');
    }

    fn text(&mut self, tag: &str, text: &str) {
        self.out.extend_from_slice(tag.as_bytes());
        self.out.push(b' ');
        self.out
            .extend_from_slice(text.len().to_string().as_bytes());
        self.out.push(b':');
        self.out.extend_from_slice(text.as_bytes());
        self.out.push(b'\n');
    }

    /// Writes the waiting gap, keeping the blank lines `policy` allows (at most one per run).
    fn flush(&mut self, policy: Blanks) {
        let mut first = true;
        let mut blank = false;
        for item in std::mem::take(&mut self.gap) {
            match item {
                GapItem::Blank => blank = true,
                GapItem::Comment(bytes) => {
                    self.blank(policy, first, blank);
                    self.out.extend_from_slice(&bytes);
                    first = false;
                    blank = false;
                }
            }
        }
        if policy != Blanks::DropAll {
            self.blank(policy, first, blank);
        }
    }

    fn blank(&mut self, policy: Blanks, first: bool, blank: bool) {
        let keep = match policy {
            Blanks::Keep => true,
            Blanks::DropFirst => !first,
            Blanks::DropAll => false,
        };
        if blank && keep {
            self.rec("B");
        }
    }

    /// Before a code token: writes the gap, and tracks the logical line.
    fn code(&mut self, is_else: bool, is_at: bool) {
        let policy = if self.indent == 0 || is_else {
            Blanks::DropAll
        } else if self.block_start || (self.at_line_start && self.after_attr) {
            Blanks::DropFirst
        } else {
            Blanks::Keep
        };
        self.flush(policy);
        if self.at_line_start {
            self.line_is_attr = is_at;
            self.at_line_start = false;
        }
        self.block_start = false;
        self.line_has_content = true;
    }

    fn newline(&mut self) {
        self.rec("NL");
        for _ in 0..std::mem::take(&mut self.open_one_line) {
            self.rec("DE");
            self.indent = self.indent.saturating_sub(1);
        }
        self.after_attr = std::mem::take(&mut self.line_is_attr);
        self.at_line_start = true;
    }

    fn leaves(&mut self, leaves: &[Leaf], depth: u32) {
        let rule_8 = rule_8_identifiers(leaves);
        let dropped_comma = trailing_commas(leaves);
        let one_line_colon = one_line_block_colons(leaves);
        for (i, l) in leaves.iter().enumerate() {
            match l.kind {
                SyntaxKind::Whitespace => self.whitespace(l.text),
                SyntaxKind::Comment | SyntaxKind::DocComment => {
                    let own = !self.line_has_content;
                    self.line_has_content = true;
                    let tag = if l.kind == SyntaxKind::Comment {
                        "#"
                    } else {
                        "##"
                    };
                    let place = if own { "own" } else { "end" };
                    let text = l.text.trim_end_matches([' ', '\r']);
                    let start = self.out.len();
                    self.out.extend_from_slice(tag.as_bytes());
                    self.out.push(b' ');
                    self.text(place, text);
                    if own {
                        let record = self.out.split_off(start);
                        self.gap.push(GapItem::Comment(record));
                    }
                }
                SyntaxKind::Newline => self.newline(),
                SyntaxKind::Indent => {
                    self.rec("IN");
                    self.indent += 1;
                    self.block_start = true;
                    self.at_line_start = true;
                }
                SyntaxKind::Dedent => {
                    self.rec("DE");
                    self.indent = self.indent.saturating_sub(1);
                    self.at_line_start = true;
                }
                _ if l.text.is_empty() => {}
                SyntaxKind::Keyword(c) | SyntaxKind::Core(c) => {
                    self.code(c == Concept::KwElse && !rule_8[i], false);
                    if rule_8[i] {
                        self.text("I", strip_backticks(l.text));
                    } else {
                        self.word("K", c.name());
                    }
                }
                SyntaxKind::Ident => {
                    self.code(false, false);
                    self.text("I", strip_backticks(l.text));
                }
                SyntaxKind::IntLit | SyntaxKind::FloatLit => {
                    self.code(false, false);
                    self.number(l.kind == SyntaxKind::IntLit, l.text);
                }
                SyntaxKind::StrLit | SyntaxKind::TaggedStrLit => {
                    self.code(false, false);
                    self.string(l.text, depth);
                }
                SyntaxKind::Punct => {
                    self.code(false, l.text == "@");
                    match l.text {
                        "(" | "[" | "{" => self.brackets += 1,
                        ")" | "]" | "}" => self.brackets = self.brackets.saturating_sub(1),
                        _ => {}
                    }
                    if dropped_comma[i] {
                        continue;
                    }
                    let p = match l.text {
                        "," | "\u{060C}" => Concept::PunctComma.name(),
                        "?" | "\u{061F}" => Concept::PunctTry.name(),
                        other => other,
                    };
                    self.word("P", p);
                    if one_line_colon[i] {
                        // Written as the indented block it is equivalent to.
                        self.rec("NL");
                        self.rec("IN");
                        self.indent += 1;
                        self.open_one_line += 1;
                        self.block_start = true;
                        self.at_line_start = true;
                    }
                }
                _ => {
                    self.code(false, false);
                    self.text("X", l.text);
                }
            }
        }
    }

    /// Spaces and `\r` are layout. A `\n` ends a line, which is blank if it had no content
    /// (blank lines inside brackets are not recorded). Any other byte (left between tokens
    /// by a lexer error) is recorded as `X`.
    fn whitespace(&mut self, text: &str) {
        let mut junk_start = None;
        for (i, c) in text.char_indices() {
            let layout = matches!(c, ' ' | '\r' | '\n');
            if layout {
                if let Some(s) = junk_start.take() {
                    self.code(false, false);
                    self.text("X", &text[s..i]);
                }
            } else if junk_start.is_none() {
                junk_start = Some(i);
            }
            if c == '\n' {
                if !self.line_has_content && self.brackets == 0 {
                    self.gap.push(GapItem::Blank);
                }
                self.line_has_content = false;
            }
        }
        if let Some(s) = junk_start {
            self.code(false, false);
            self.text("X", &text[s..]);
        }
    }

    fn number(&mut self, int: bool, text: &str) {
        let ascii: String = kalam_lexer::to_ascii_digits(text)
            .chars()
            .filter(|&c| c != '_')
            .collect();
        if int {
            let (radix, digits) = match ascii.get(..2) {
                Some("0x") => (16, &ascii[2..]),
                Some("0b") => (2, &ascii[2..]),
                Some("0o") => (8, &ascii[2..]),
                _ => (10, ascii.as_str()),
            };
            if let Some(value) = to_decimal(digits, radix) {
                self.word("N", &format!("i {radix} {value}"));
                return;
            }
        } else {
            let canonical = ascii.replace('E', "e").replace("e+", "e");
            let well_formed = canonical.starts_with(|c: char| c.is_ascii_digit())
                && canonical
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | '-'));
            if well_formed {
                self.word("N", &format!("f {canonical}"));
                return;
            }
        }
        self.out.extend_from_slice(b"N ");
        self.text("?", &ascii);
    }

    fn string(&mut self, text: &str, depth: u32) {
        self.rec("S");
        for seg in split_holes(text) {
            match seg {
                Segment::Text(t) => self.text("T", t),
                Segment::Hole(code) => self.hole(code, depth + 1),
            }
        }
        self.rec("/S");
    }

    /// Hole code is code (§4.4): it is re-lexed in the file's skin as a fragment, exactly as
    /// the parser does (`lex_fragment`: a hole cannot see the file's declarations, so the
    /// undefined-name half of K0201 must not turn a name into a keyword here, even in a hole
    /// that spans lines), and serialized like the rest of the file.
    fn hole(&mut self, code: &str, depth: u32) {
        if depth > MAX_HOLE_DEPTH {
            self.text("R", code);
            return;
        }
        let lexed = kalam_lexer::lex_fragment(code, self.skin, FileId(0));
        let slice = |start: u32, end: u32| -> &str {
            let (a, b) = (start.min(end) as usize, start.max(end) as usize);
            code.get(a..b).unwrap_or("")
        };
        let mut items: Vec<(u32, Leaf)> = Vec::new();
        for t in &lexed.tokens {
            let kind = match t.kind {
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => {
                    continue
                }
                TokenKind::Ident => SyntaxKind::Ident,
                TokenKind::Keyword(c) => SyntaxKind::Keyword(c),
                TokenKind::Core(c) => SyntaxKind::Core(c),
                TokenKind::Int => SyntaxKind::IntLit,
                TokenKind::Float => SyntaxKind::FloatLit,
                TokenKind::Str => SyntaxKind::StrLit,
                TokenKind::TaggedStr => SyntaxKind::TaggedStrLit,
                TokenKind::Op(_) => SyntaxKind::Punct,
            };
            let text = slice(t.span.start, t.span.end);
            items.push((t.span.start, leaf(kind, text)));
        }
        for t in &lexed.trivia {
            let kind = match t.kind {
                TriviaKind::Comment => SyntaxKind::Comment,
                TriviaKind::DocComment => SyntaxKind::DocComment,
            };
            items.push((t.span.start, leaf(kind, slice(t.span.start, t.span.end))));
        }
        items.sort_by_key(|(start, _)| *start);
        let hole_leaves: Vec<Leaf> = items.into_iter().map(|(_, l)| l).collect();
        self.rec("H");
        let brackets = std::mem::take(&mut self.brackets);
        self.leaves(&hole_leaves, depth);
        self.flush(Blanks::DropAll);
        self.brackets = brackets;
        self.rec("/H");
    }
}

fn leaf(kind: SyntaxKind, text: &str) -> Leaf<'_> {
    Leaf {
        kind,
        text,
        parent: SyntaxKind::Expr,
    }
}

fn strip_backticks(text: &str) -> &str {
    text.strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(text)
}

/// The exact value of a digit string in `radix`, in decimal. `None` if it is empty or holds
/// a non-digit. Arbitrary precision: an integer literal may be a `BigInt` (§5.6).
fn to_decimal(digits: &str, radix: u32) -> Option<String> {
    const BASE: u64 = 1_000_000_000;
    if digits.is_empty() {
        return None;
    }
    // Little-endian limbs in base 10^9.
    let mut limbs: Vec<u64> = vec![0];
    for c in digits.chars() {
        let mut carry = u64::from(c.to_digit(radix)?);
        for limb in limbs.iter_mut() {
            let v = *limb * u64::from(radix) + carry;
            *limb = v % BASE;
            carry = v / BASE;
        }
        while carry > 0 {
            limbs.push(carry % BASE);
            carry /= BASE;
        }
    }
    let mut out = String::new();
    let mut iter = limbs.iter().rev();
    if let Some(top) = iter.next() {
        out.push_str(&top.to_string());
    }
    for limb in iter {
        out.push_str(&format!("{limb:09}"));
    }
    Some(out)
}

enum Segment<'a> {
    Text(&'a str),
    Hole(&'a str),
}

/// Splits a string or tagged-literal token into the literal text outside holes (tag, quotes
/// and escapes included) and the trimmed code of each hole. Mirrors the parser's
/// `str_lit`: `\` escapes the next character, `\u{…}` braces belong to the escape, and a
/// hole's braces may nest. An unclosed hole is literal text.
fn split_holes(text: &str) -> Vec<Segment<'_>> {
    let tag_len = text.find('"').unwrap_or(0);
    let body = &text[tag_len..];
    let triple = body.starts_with("\"\"\"");
    let q = (if triple { 3 } else { 1 }).min(body.len());
    let closed = if triple {
        body.len() >= 6 && body.ends_with("\"\"\"")
    } else {
        body.len() >= 2 && body.ends_with('"')
    };
    let inner_end = if closed { body.len() - q } else { body.len() };
    let inner = body.get(q..inner_end).unwrap_or("");
    let base = tag_len + q;

    let mut out = Vec::new();
    let mut cursor = 0usize; // into `text`
    let mut chars = inner.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, n)) = chars.next() {
                    if n == 'u' && matches!(chars.peek(), Some(&(_, '{'))) {
                        for (_, d) in chars.by_ref() {
                            if d == '}' {
                                break;
                            }
                        }
                    }
                }
            }
            '{' => {
                let mut depth = 1usize;
                let mut end = None;
                for (j, d) in inner[i + 1..].char_indices() {
                    match d {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = Some(i + 1 + j);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let Some(end) = end else { break };
                if base + i > cursor {
                    out.push(Segment::Text(&text[cursor..base + i]));
                }
                out.push(Segment::Hole(inner[i + 1..end].trim()));
                cursor = base + end + 1;
                while matches!(chars.peek(), Some(&(k, _)) if k <= end) {
                    chars.next();
                }
            }
            _ => {}
        }
    }
    if cursor < text.len() {
        out.push(Segment::Text(&text[cursor..]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(text: &str) -> Vec<String> {
        split_holes(text)
            .into_iter()
            .map(|s| match s {
                Segment::Text(t) => format!("T{t}"),
                Segment::Hole(h) => format!("H{h}"),
            })
            .collect()
    }

    #[test]
    fn holes_split_like_the_parser() {
        assert_eq!(segs("\"a {x} b\""), ["T\"a ", "Hx", "T b\""]);
        assert_eq!(segs("\"{ a }{b}\""), ["T\"", "Ha", "Hb", "T\""]);
        assert_eq!(segs("\"\\{x\\}\""), ["T\"\\{x\\}\""]);
        assert_eq!(segs("\"\\u{41}{x}\""), ["T\"\\u{41}", "Hx", "T\""]);
        assert_eq!(segs("\"{ {1: 2}[1] }\""), ["T\"", "H{1: 2}[1]", "T\""]);
        assert_eq!(segs("sql\"select {x}\""), ["Tsql\"select ", "Hx", "T\""]);
        assert_eq!(
            segs("\"\"\"\n{x}\n\"\"\""),
            ["T\"\"\"\n", "Hx", "T\n\"\"\""]
        );
        assert_eq!(segs("\"open {x"), ["T\"open {x"]);
        assert_eq!(segs("\"\""), ["T\"\""]);
        assert_eq!(segs("\""), ["T\""]);
    }

    #[test]
    fn decimal_values_are_exact() {
        assert_eq!(to_decimal("0", 10).as_deref(), Some("0"));
        assert_eq!(to_decimal("1f", 16).as_deref(), Some("31"));
        assert_eq!(to_decimal("1F", 16).as_deref(), Some("31"));
        assert_eq!(to_decimal("101", 2).as_deref(), Some("5"));
        assert_eq!(to_decimal("777", 8).as_deref(), Some("511"));
        assert_eq!(
            to_decimal("ffffffffffffffffffffffffffffffffff", 16).as_deref(),
            Some("87112285931760246646623899502532662132735")
        );
        assert_eq!(
            to_decimal("123456789012345678901234567890", 10).as_deref(),
            Some("123456789012345678901234567890")
        );
        assert_eq!(to_decimal("", 10), None);
        assert_eq!(to_decimal("12a", 10), None);
    }
}
