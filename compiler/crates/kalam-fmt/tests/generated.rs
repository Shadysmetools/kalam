//! Property test: random programs, written directly in a random skin, through `fmt` and every
//! skin-conversion round trip.
//!
//! The generator (a deterministic xorshift, as in the parser's fuzz test, because `proptest`
//! is not available offline) writes source text straight from the keyword table, never via
//! the converter under test. It varies everything the converter and formatter must handle:
//! Franco spelling variants, Arabic hamza/alef forms and Persian/Urdu letters (D19), `،`/`؟`
//! and Arabic-Indic digits, identifiers that are reserved in some skin (escaped where needed,
//! plain in label/member/field positions, with needless backticks too), interpolation holes
//! holding keywords, core names, `,` and `?`, broken and unbroken brackets, leading-dot
//! method chains, one-line and indented blocks, blank lines, own-line, trailing and in-bracket
//! comments, and long lines.
//!
//! For every program `src` in skin X, with `f = fmt(src)`:
//! - `src` parses cleanly (otherwise the generator is wrong);
//! - `fmt` succeeds without its fallback, `fmt(f) == f`, and `f` has `src`'s AST and comments;
//! - every ordered skin pair and both 3-hop cycles from `f` come back to `f` byte for byte,
//!   every intermediate text parses to the same AST and comments, and is itself formatted
//!   (`fmt_Y(convert_X→Y(f)) == convert_X→Y(f)`);
//! - every round trip from the unformatted `src` keeps the AST, and formats to `f`.
//!
//! The case count is `KALAM_FMT_PROP_CASES` (default 2,000; the M1 gate needs 10,000).
//! A failure prints the seed and the program.

mod common;

use common::*;
use kalam_base::Skin;
use kalam_skins::{Concept, KeywordTable};

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
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

const PLAIN: &[&str] = &[
    "x", "y", "total", "items", "count", "user", "name", "value", "n1", "_tmp", "قيمة", "اسم",
];
/// Words that are reserved in at least one skin (keywords, core names, Franco variants,
/// Arabic fold variants, Persian/Urdu letter forms).
const CROSSY: &[&str] = &[
    "law",
    "walla",
    "fi",
    "lekol",
    "hasab",
    "lama",
    "khali",
    "dala",
    "ektb",
    "adad",
    "lw",
    "fe",
    "wala",
    "low",
    "sa7",
    "if",
    "for",
    "in",
    "match",
    "print",
    "let",
    "else",
    "إذا",
    "اذا",
    "لكل",
    "في",
    "فى",
    "عدد",
    "اطبع",
    "و",
    "ليكن",
    "ل\u{06A9}ل",
    "ف\u{06CC}",
    "mafeesh",
    "None",
];
const TYPES: &[&str] = &[
    "Shape", "User", "Point", "Box", "Item", "Int", "adad", "نوع",
];
const VARIANTS: &[&str] = &["Circle", "Empty", "Rect", "law", "Some", "حالة"];
const TEXT: &[&str] = &[
    "hello",
    " ",
    "سلام عليكم",
    "a, b",
    "x?",
    "\\n",
    "\\{",
    "\\}",
    "\\u{41}",
    "if",
    "law",
    ": ",
    "١٢٣",
];

struct Gen<'t> {
    rng: Rng,
    skin: Skin,
    table: &'t KeywordTable,
    comments: u32,
    /// Non-zero while generating a block header, signature or field default: the formatter keeps
    /// comments inside brackets only in simple statements and `let` constants.
    quiet: u32,
    /// Words reserved in some skin that the program uses as plain names. `program` declares
    /// them all in one `use` line: an undeclared one is K0201 (FOUNDER-QUEUE Q9).
    declare: Vec<&'static str>,
}

impl Gen<'_> {
    // ----- spellings ----------------------------------------------------------------------

    fn kw(&mut self, c: Concept) -> String {
        let canonical = self.table.spelling(c, self.skin).to_string();
        match self.skin {
            Skin::Franco if self.rng.chance(35) => {
                let aliases = &self.table.row(c).franco_aliases;
                if aliases.is_empty() {
                    canonical
                } else {
                    aliases[self.rng.below(aliases.len())].clone()
                }
            }
            Skin::Ar if self.rng.chance(35) => {
                let v: String = canonical
                    .chars()
                    .map(|ch| match ch {
                        'أ' | 'إ' | 'آ' => 'ا',
                        'ي' => *self.rng.pick(&['ي', '\u{06CC}', 'ى']),
                        'ك' => *self.rng.pick(&['ك', '\u{06A9}']),
                        'ة' => *self.rng.pick(&['ة', 'ه', '\u{06D5}']),
                        'ه' => *self.rng.pick(&['ه', '\u{06BE}']),
                        other => other,
                    })
                    .collect();
                let same = self
                    .table
                    .skin(Skin::Ar)
                    .lookup_word(&v)
                    .is_some_and(|m| m.concept == c);
                if same {
                    v
                } else {
                    canonical
                }
            }
            _ => canonical,
        }
    }

    fn boolean(&mut self) -> String {
        let c = if self.rng.chance(50) {
            Concept::KwTrue
        } else {
            Concept::KwFalse
        };
        self.kw(c)
    }

    fn comma(&mut self) -> String {
        if self.skin == Skin::Ar && self.rng.chance(60) {
            "\u{060C}".into()
        } else {
            ",".into()
        }
    }

    fn try_op(&mut self) -> String {
        if self.skin == Skin::Ar && self.rng.chance(60) {
            "\u{061F}".into()
        } else {
            "?".into()
        }
    }

    fn sp(&mut self) -> &'static str {
        if self.rng.chance(15) {
            "  "
        } else {
            " "
        }
    }

    /// A comma and the space after it (sometimes none, sometimes two).
    fn sep(&mut self) -> String {
        let c = self.comma();
        let s = *self.rng.pick(&["", " ", " ", " ", "  "]);
        format!("{c}{s}")
    }

    fn int(&mut self) -> String {
        let n = *self.rng.pick(&[0u64, 1, 2, 7, 15, 42, 100, 1000, 65535]);
        if self.skin == Skin::Ar && self.rng.chance(30) {
            n.to_string()
                .chars()
                .map(|c| char::from_u32(0x0660 + (c as u32 - '0' as u32)).unwrap())
                .collect()
        } else if self.rng.chance(10) {
            "0x1F".into()
        } else if self.rng.chance(10) {
            "1_000".into()
        } else {
            n.to_string()
        }
    }

    fn float(&mut self) -> String {
        if self.skin == Skin::Ar && self.rng.chance(30) {
            "\u{0661}\u{066B}\u{0665}".into()
        } else {
            self.rng.pick(&["1.5", "0.25", "2e3", "3.0e-2"]).to_string()
        }
    }

    fn raw_name(&mut self) -> &'static str {
        let name = if self.rng.chance(35) {
            self.rng.pick(CROSSY)
        } else {
            self.rng.pick(PLAIN)
        };
        self.note_reserved(name);
        name
    }

    /// Records a name that is reserved in some skin, so `program` declares it.
    fn note_reserved(&mut self, name: &'static str) {
        let reserved = Skin::ALL
            .into_iter()
            .any(|s| self.table.skin(s).is_reserved(name));
        if reserved && !self.declare.contains(&name) {
            self.declare.push(name);
        }
    }

    /// An identifier where a reserved word must be escaped.
    fn bound(&mut self, name: &str) -> String {
        if self.table.skin(self.skin).is_reserved(name) || (self.rng.chance(8) && name != "_") {
            format!("`{name}`")
        } else {
            name.to_string()
        }
    }

    /// An identifier in a §5.3 rule 8 position: a reserved word needs no escape.
    fn free(&mut self, name: &str) -> String {
        if self.rng.chance(8) {
            format!("`{name}`")
        } else {
            name.to_string()
        }
    }

    fn var(&mut self) -> String {
        let n = self.raw_name();
        self.bound(n)
    }

    fn label(&mut self) -> String {
        let n = self.raw_name();
        self.free(n)
    }

    fn type_name(&mut self) -> String {
        let n = *self.rng.pick(TYPES);
        self.note_reserved(n);
        self.bound(n)
    }

    fn comment(&mut self, doc: bool) -> String {
        self.comments += 1;
        let words = *self
            .rng
            .pick(&["note", "TODO: law fi lekol", "تعليق", "if x: y", "a, b?"]);
        format!(
            "{} c{} {words}",
            if doc { "##" } else { "#" },
            self.comments
        )
    }

    // ----- expressions --------------------------------------------------------------------

    /// A bracketed, comma-separated list; sometimes broken over lines (bracket newlines and
    /// indentation are ignored), sometimes with a comment inside.
    fn bracket(
        &mut self,
        open: &str,
        items: Vec<String>,
        close: &str,
        ind: usize,
        hole: bool,
    ) -> String {
        if items.is_empty() {
            return format!("{open}{close}");
        }
        let broken = !hole && self.rng.chance(20);
        let mut s = String::from(open);
        let n = items.len();
        for (i, it) in items.into_iter().enumerate() {
            if broken {
                s.push('\n');
                s.push_str(&" ".repeat(ind + 4 + self.rng.below(3)));
            } else if i > 0 {
                s.push_str(self.rng.pick(&["", " "]));
            }
            s.push_str(&it);
            if i + 1 < n || (broken && self.rng.chance(50)) {
                s.push_str(&self.comma());
                if broken && self.quiet == 0 && self.rng.chance(15) {
                    let c = self.comment(false);
                    s.push_str(&format!("  {c}"));
                }
            }
        }
        if broken {
            s.push('\n');
            s.push_str(&" ".repeat(ind));
        }
        s.push_str(close);
        s
    }

    fn string(&mut self, depth: u32, ind: usize, tag: Option<&str>) -> String {
        let triple = self.rng.chance(15);
        let q = if triple { "\"\"\"" } else { "\"" };
        let mut s = format!("{}{q}", tag.unwrap_or(""));
        if triple && self.rng.chance(40) {
            s.push('\n');
            s.push_str(&" ".repeat(ind + 4));
        }
        for _ in 0..1 + self.rng.below(4) {
            if self.rng.chance(45) {
                let pad = if self.rng.chance(20) { " " } else { "" };
                let e = self.expr(depth.min(2), Ctx::Hole, ind);
                s.push_str(&format!("{{{pad}{e}{pad}}}"));
            } else {
                s.push_str(self.rng.pick(TEXT));
            }
        }
        if triple && s.contains('\n') {
            s.push('\n');
            s.push_str(&" ".repeat(ind));
        }
        s.push_str(q);
        s
    }

    fn atom(&mut self, ctx: Ctx, ind: usize) -> String {
        match self.rng.below(if ctx.in_hole() { 6 } else { 10 }) {
            0 => self.int(),
            1 => self.float(),
            2 => self.boolean(),
            3 | 4 => self.var(),
            5 => {
                let c = *self.rng.pick(&[
                    Concept::CorePrint,
                    Concept::CoreNone,
                    Concept::CoreInt,
                    Concept::CoreMain,
                ]);
                self.kw(c)
            }
            6 => format!(".{}", {
                let v = *self.rng.pick(VARIANTS);
                self.free(v)
            }),
            7 => "()".into(),
            8 => {
                let tag = *self.rng.pick(&["html", "sql", "url"]);
                self.string(1, ind, Some(tag))
            }
            _ => self.string(1, ind, None),
        }
    }

    fn args(&mut self, depth: u32, ind: usize, hole: bool) -> String {
        let n = self.rng.below(4);
        let named_from = self.rng.below(n + 1);
        let mut items = Vec::new();
        for i in 0..n {
            let v = self.expr(depth - 1, if hole { Ctx::Hole } else { Ctx::Top }, ind + 4);
            if i >= named_from {
                let l = self.label();
                let sp = if self.rng.chance(15) { "" } else { " " };
                items.push(format!("{l}:{sp}{v}"));
            } else {
                items.push(v);
            }
        }
        self.bracket("(", items, ")", ind, hole)
    }

    fn postfix(&mut self, depth: u32, ctx: Ctx, ind: usize) -> String {
        let hole = ctx.in_hole();
        let mut s = if self.rng.chance(70) || depth <= 1 {
            self.var()
        } else {
            let inner = self.expr(depth - 1, if hole { Ctx::Hole } else { Ctx::Top }, ind);
            format!("({inner})")
        };
        for _ in 0..1 + self.rng.below(3) {
            match self.rng.below(5) {
                0 | 1 => {
                    let m = self.label();
                    s.push('.');
                    s.push_str(&m);
                }
                2 => {
                    let a = self.args(depth.max(1), ind, hole);
                    s.push_str(&a);
                }
                3 => {
                    let i = self.expr(depth - 1, if hole { Ctx::Hole } else { Ctx::Top }, ind);
                    s.push_str(&format!("[{i}]"));
                }
                _ => {
                    let t = self.try_op();
                    s.push_str(&t);
                }
            }
        }
        s
    }

    fn expr(&mut self, depth: u32, ctx: Ctx, ind: usize) -> String {
        if depth == 0 {
            return self.atom(ctx, ind);
        }
        let hole = ctx.in_hole();
        let kinds = if hole { 9 } else { 15 };
        let (text, compound) = match self.rng.below(kinds) {
            0 | 1 => (self.atom(ctx, ind), false),
            2 | 3 => (self.postfix(depth, ctx, ind), false),
            4 => {
                let op = self.rng.below(8);
                let l = self.expr(depth - 1, Ctx::Operand(hole), ind);
                let r = self.expr(depth - 1, Ctx::Operand(hole), ind);
                let op = match op {
                    0 => self.kw(Concept::KwAnd),
                    1 => self.kw(Concept::KwOr),
                    2 => "==".into(),
                    3 => "<".into(),
                    4 => "+".into(),
                    5 => "*".into(),
                    6 => "//".into(),
                    _ => "-".into(),
                };
                let (a, b) = (self.sp(), self.sp());
                (format!("{l}{a}{op}{b}{r}"), true)
            }
            5 => {
                let e = self.expr(depth - 1, Ctx::Operand(hole), ind);
                let n = self.kw(Concept::KwNot);
                (format!("{n} {e}"), true)
            }
            6 => {
                let e = self.expr(depth - 1, Ctx::Operand(hole), ind);
                (format!("-{e}"), true)
            }
            7 => {
                let items = (0..self.rng.below(4))
                    .map(|_| self.expr(depth - 1, if hole { Ctx::Hole } else { Ctx::Top }, ind + 4))
                    .collect();
                (self.bracket("[", items, "]", ind, hole), false)
            }
            8 => {
                let n = 2 + self.rng.below(2);
                let items = (0..n)
                    .map(|_| self.expr(depth - 1, if hole { Ctx::Hole } else { Ctx::Top }, ind + 4))
                    .collect();
                (self.bracket("(", items, ")", ind, hole), false)
            }
            9 => {
                let items = (0..self.rng.below(3))
                    .map(|_| {
                        let k = self.expr(depth - 1, Ctx::Top, ind + 4);
                        let v = self.expr(depth - 1, Ctx::Top, ind + 4);
                        format!("{k}: {v}")
                    })
                    .collect();
                (self.bracket("{", items, "}", ind, false), false)
            }
            10 => {
                let lo = self.expr(depth - 1, Ctx::Operand(false), ind);
                let hi = self.expr(depth - 1, Ctx::Operand(false), ind);
                let op = if self.rng.chance(50) { ".." } else { "..=" };
                (format!("{lo}{op}{hi}"), true)
            }
            11 => {
                let params = (0..self.rng.below(3))
                    .map(|_| {
                        let p = if self.rng.chance(20) {
                            "_".to_string()
                        } else {
                            self.var()
                        };
                        if self.rng.chance(30) {
                            let t = self.ty(1);
                            format!("{p}: {t}")
                        } else {
                            p
                        }
                    })
                    .collect();
                let ps = self.bracket("(", params, ")", ind, false);
                let body = self.expr(depth - 1, Ctx::Top, ind);
                let f = self.kw(Concept::KwFn);
                (format!("{f}{ps} => {body}"), true)
            }
            12 => (self.inline_if(depth, ind), true),
            13 => {
                let inner = self.expr(depth - 1, Ctx::Top, ind);
                (format!("({inner})"), false)
            }
            _ => {
                let u = self.kw(Concept::KwUnsafe);
                let m = self.label();
                (format!("{u}.{m}"), false)
            }
        };
        match ctx {
            Ctx::Operand(_) | Ctx::Cond if compound => format!("({text})"),
            _ => text,
        }
    }

    /// An expression in a block header (no comments inside its brackets).
    fn head_expr(&mut self, depth: u32, ind: usize) -> String {
        self.quiet += 1;
        let e = self.expr(depth, Ctx::Cond, ind);
        self.quiet -= 1;
        e
    }

    fn inline_if(&mut self, depth: u32, ind: usize) -> String {
        let (i, e) = (self.kw(Concept::KwIf), self.kw(Concept::KwElse));
        let c = self.expr(depth - 1, Ctx::Cond, ind);
        let a = self.expr(depth - 1, Ctx::Top, ind);
        let mut s = format!("{i} {c}: {a}");
        if self.rng.chance(30) {
            let i = self.kw(Concept::KwIf);
            let c = self.expr(depth - 1, Ctx::Cond, ind);
            let a = self.expr(depth - 1, Ctx::Top, ind);
            let e = self.kw(Concept::KwElse);
            s.push_str(&format!(" {e} {i} {c}: {a}"));
        }
        let b = self.expr(depth - 1, Ctx::Top, ind);
        s.push_str(&format!(" {e}: {b}"));
        s
    }

    /// A method chain written over leading-dot continuation lines.
    fn chain(&mut self, ind: usize) -> String {
        let mut s = self.var();
        for _ in 0..2 + self.rng.below(2) {
            let m = self.label();
            let a = self.args(2, ind + 4, false);
            s.push_str(&format!("\n{}.{m}{a}", " ".repeat(ind + 4)));
        }
        s
    }

    // ----- patterns and types -------------------------------------------------------------

    fn pat_fields(&mut self, depth: u32) -> String {
        let n = self.rng.below(3);
        let mut items: Vec<String> = (0..n)
            .map(|_| {
                let p = self.pat(depth - 1);
                if self.rng.chance(40) {
                    let l = self.label();
                    format!("{l}: {p}")
                } else {
                    p
                }
            })
            .collect();
        if self.rng.chance(25) {
            items.push("..".into());
        }
        let c = self.sep();
        format!("({})", items.join(&c))
    }

    fn pat(&mut self, depth: u32) -> String {
        self.pat_k(depth, 13)
    }

    /// `kinds` = 12 excludes `p as name` (an `as` pattern cannot be the left side of `as`).
    fn pat_k(&mut self, depth: u32, kinds: usize) -> String {
        let k = if depth == 0 {
            self.rng.below(5)
        } else {
            self.rng.below(kinds)
        };
        match k {
            0 => "_".into(),
            1 => self.var(),
            2 => {
                let n = self.int();
                if self.rng.chance(20) {
                    format!("-{n}")
                } else {
                    n
                }
            }
            3 => self.boolean(),
            4 => format!("\"{}\"", self.rng.pick(&["a", "law", "سلام"])),
            5 => {
                let (a, b) = (self.int(), self.int());
                format!("{a}..={b}")
            }
            6 => {
                let v = *self.rng.pick(VARIANTS);
                let v = self.free(v);
                let f = if self.rng.chance(50) {
                    self.pat_fields(depth)
                } else {
                    String::new()
                };
                format!(".{v}{f}")
            }
            7 => {
                let t = self.type_name();
                let v = *self.rng.pick(VARIANTS);
                let v = self.free(v);
                let f = if self.rng.chance(50) {
                    self.pat_fields(depth)
                } else {
                    String::new()
                };
                format!("{t}.{v}{f}")
            }
            8 => {
                let c = *self
                    .rng
                    .pick(&[Concept::CoreSome, Concept::CoreOk, Concept::CoreErr]);
                let c = self.kw(c);
                let p = self.pat(depth - 1);
                format!("{c}({p})")
            }
            9 => self.kw(Concept::CoreNone),
            10 => {
                let (a, b) = (self.pat(depth - 1), self.pat(depth - 1));
                let c = self.sep();
                format!("({a}{c}{b})")
            }
            11 => {
                let a = self.pat(depth - 1);
                let rest = if self.rng.chance(50) {
                    let r = self.var();
                    format!("..{r}")
                } else {
                    "..".into()
                };
                let c = self.sep();
                format!("[{a}{c}{rest}]")
            }
            _ => {
                let a = self.pat_k(depth - 1, 12);
                let as_kw = self.kw(Concept::KwAs);
                let n = self.var();
                format!("{a} {as_kw} {n}")
            }
        }
    }

    /// A top-level pattern: sometimes an or-pattern.
    fn pattern(&mut self, depth: u32) -> String {
        let p = self.pat(depth);
        if self.rng.chance(15) {
            let q = self.pat(depth);
            format!("{p} | {q}")
        } else {
            p
        }
    }

    fn ty(&mut self, depth: u32) -> String {
        let k = if depth == 0 {
            self.rng.below(3)
        } else {
            self.rng.below(8)
        };
        match k {
            0 => {
                let c = *self
                    .rng
                    .pick(&[Concept::CoreInt, Concept::CoreString, Concept::CoreBool]);
                self.kw(c)
            }
            1 => self.type_name(),
            2 => "()".into(),
            3 => {
                let l = self.kw(Concept::CoreList);
                let t = self.ty(depth - 1);
                format!("{l}[{t}]")
            }
            4 => {
                let r = self.kw(Concept::CoreResult);
                let (a, b) = (self.ty(depth - 1), self.ty(depth - 1));
                let c = self.sep();
                format!("{r}[{a}{c}{b}]")
            }
            5 => {
                let (a, b) = (self.ty(depth - 1), self.ty(depth - 1));
                let c = self.sep();
                format!("({a}{c}{b})")
            }
            6 => {
                let t = self.type_name();
                let m = self.label();
                format!("{t}.{m}")
            }
            _ => {
                let f = self.kw(Concept::KwFn);
                let (a, b) = (self.ty(depth - 1), self.ty(depth - 1));
                let sync = if self.rng.chance(20) { "@sync " } else { "" };
                format!("{sync}{f}({a}) -> {b}")
            }
        }
    }

    // ----- statements ---------------------------------------------------------------------

    fn trailing(&mut self, line: &mut String) {
        if self.rng.chance(12) {
            let c = self.comment(false);
            let pad = *self.rng.pick(&[" ", "  ", "    "]);
            line.push_str(&format!("{pad}{c}"));
        }
    }

    /// Blank lines and own-line comments before a line at `ind`.
    fn spacing(&mut self, out: &mut Vec<String>, ind: usize) {
        for _ in 0..self.rng.below(3) {
            match self.rng.below(5) {
                0 => out.push(String::new()),
                1 => out.push(" ".repeat(self.rng.below(6))),
                2 => {
                    let col = *self
                        .rng
                        .pick(&[ind, ind, ind.saturating_sub(4), ind + 4, 0]);
                    let c = self.comment(false);
                    out.push(format!("{}{c}", " ".repeat(col)));
                }
                _ => {}
            }
        }
    }

    fn simple(&mut self, depth: u32, ind: usize) -> String {
        match self.rng.below(10) {
            0 | 1 => {
                let (l, p) = (self.kw(Concept::KwLet), self.pattern(1));
                let ty = if self.rng.chance(30) {
                    format!(": {}", self.ty(1))
                } else {
                    String::new()
                };
                let v = self.expr(depth, Ctx::Top, ind);
                format!("{l} {p}{ty} = {v}")
            }
            2 => {
                let (k, n) = (self.kw(Concept::KwVar), self.var());
                let v = self.expr(depth, Ctx::Top, ind);
                format!("{k} {n} = {v}")
            }
            3 => {
                let mut place = self.var();
                if self.rng.chance(40) {
                    let m = self.label();
                    place.push_str(&format!(".{m}"));
                }
                if self.rng.chance(20) {
                    let i = self.expr(1, Ctx::Top, ind);
                    place.push_str(&format!("[{i}]"));
                }
                let op = *self.rng.pick(&["=", "+=", "-=", "*=", "/="]);
                let v = self.expr(depth, Ctx::Top, ind);
                format!("{place} {op} {v}")
            }
            4 => {
                let r = self.kw(Concept::KwReturn);
                if self.rng.chance(20) {
                    r
                } else {
                    let v = self.expr(depth, Ctx::Top, ind);
                    format!("{r} {v}")
                }
            }
            5 => {
                let c = *self
                    .rng
                    .pick(&[Concept::KwBreak, Concept::KwContinue, Concept::KwPass]);
                self.kw(c)
            }
            6 => {
                let (l, n) = (self.kw(Concept::KwLet), self.var());
                let c = self.chain(ind);
                format!("{l} {n} = {c}")
            }
            _ => {
                let p = self.kw(Concept::CorePrint);
                let a = self.args(depth.max(1), ind, false);
                format!("{p}{a}")
            }
        }
    }

    /// `head:` plus a block: one line (`head: stmt`) or indented statements.
    fn block(&mut self, out: &mut Vec<String>, head: String, ind: usize, depth: u32) {
        if self.rng.chance(30) {
            let s = self.simple(depth.min(2), ind);
            let mut line = format!("{}{head}: {s}", " ".repeat(ind));
            self.trailing(&mut line);
            out.push(line);
            return;
        }
        let mut line = format!("{}{head}:", " ".repeat(ind));
        self.trailing(&mut line);
        out.push(line);
        let n = 1 + self.rng.below(3);
        for i in 0..n {
            if i > 0 {
                self.spacing(out, ind + 4);
            }
            self.stmt(out, ind + 4, depth.saturating_sub(1));
        }
        if self.rng.chance(15) {
            self.spacing(out, ind + 4);
        }
    }

    fn stmt(&mut self, out: &mut Vec<String>, ind: usize, depth: u32) {
        let pad = " ".repeat(ind);
        let k = if depth == 0 { 0 } else { self.rng.below(9) };
        match k {
            0..=3 => {
                let mut line = format!("{pad}{}", self.simple(depth.max(1), ind));
                self.trailing(&mut line);
                out.push(line);
            }
            4 => self.if_form(out, String::new(), ind, depth, false),
            5 => {
                let (l, n) = (self.kw(Concept::KwLet), self.var());
                if self.rng.chance(50) {
                    self.if_form(out, format!("{l} {n} = "), ind, depth, true);
                } else {
                    self.match_form(out, format!("{l} {n} = "), ind, depth);
                }
            }
            6 => self.match_form(out, String::new(), ind, depth),
            7 => {
                let (f, i) = (self.kw(Concept::KwFor), self.kw(Concept::KwIn));
                let p = self.pat(1);
                let e = self.head_expr(1, ind);
                self.block(out, format!("{f} {p} {i} {e}"), ind, depth);
            }
            _ => {
                let w = self.kw(Concept::KwWhile);
                let c = self.head_expr(2, ind);
                self.block(out, format!("{w} {c}"), ind, depth);
            }
        }
    }

    fn if_form(
        &mut self,
        out: &mut Vec<String>,
        prefix: String,
        ind: usize,
        depth: u32,
        value: bool,
    ) {
        let i = self.kw(Concept::KwIf);
        let c = self.head_expr(2, ind);
        self.block(out, format!("{prefix}{i} {c}"), ind, depth);
        for _ in 0..self.rng.below(2) {
            let (e, i) = (self.kw(Concept::KwElse), self.kw(Concept::KwIf));
            let c = self.head_expr(2, ind);
            self.block(out, format!("{e} {i} {c}"), ind, depth);
        }
        if value || self.rng.chance(50) {
            let e = self.kw(Concept::KwElse);
            self.block(out, e, ind, depth);
        }
    }

    fn match_form(&mut self, out: &mut Vec<String>, prefix: String, ind: usize, depth: u32) {
        let m = self.kw(Concept::KwMatch);
        let e = self.head_expr(2, ind);
        let mut line = format!("{}{prefix}{m} {e}:", " ".repeat(ind));
        self.trailing(&mut line);
        out.push(line);
        for i in 0..1 + self.rng.below(3) {
            if i > 0 {
                self.spacing(out, ind + 4);
            }
            let c = self.kw(Concept::KwCase);
            let p = self.pattern(2);
            let guard = if self.rng.chance(20) {
                let i = self.kw(Concept::KwIf);
                let g = self.head_expr(1, ind);
                format!(" {i} {g}")
            } else {
                String::new()
            };
            self.block(
                out,
                format!("{c} {p}{guard}"),
                ind + 4,
                depth.saturating_sub(1),
            );
        }
    }

    // ----- items --------------------------------------------------------------------------

    fn sig(&mut self, with_self: bool) -> String {
        self.quiet += 1;
        let s = self.sig_inner(with_self);
        self.quiet -= 1;
        s
    }

    fn sig_inner(&mut self, with_self: bool) -> String {
        let f = self.kw(Concept::KwFn);
        let name = if self.rng.chance(15) {
            self.kw(Concept::CoreMain)
        } else {
            self.var()
        };
        let generics = if self.rng.chance(15) {
            let t = self.bound("T");
            format!("[{t}: {}]", self.type_name())
        } else {
            String::new()
        };
        let mut params = Vec::new();
        if with_self {
            params.push(self.kw(Concept::KwSelf));
        }
        for _ in 0..self.rng.below(3) {
            let (n, t) = (self.var(), self.ty(1));
            let d = if self.rng.chance(20) {
                format!(" = {}", self.int())
            } else {
                String::new()
            };
            params.push(format!("{n}: {t}{d}"));
        }
        let ps = self.bracket("(", params, ")", 0, false);
        let ret = if self.rng.chance(50) {
            format!(" -> {}", self.ty(2))
        } else {
            String::new()
        };
        format!("{f} {name}{generics}{ps}{ret}")
    }

    fn fn_item(&mut self, out: &mut Vec<String>, prefix: &str, ind: usize, with_self: bool) {
        let sig = self.sig(with_self);
        if self.rng.chance(20) {
            let e = self.expr(2, Ctx::Top, ind);
            let mut line = format!("{}{prefix}{sig} => {e}", " ".repeat(ind));
            self.trailing(&mut line);
            out.push(line);
        } else {
            self.block(out, format!("{prefix}{sig}"), ind, 3);
        }
    }

    fn attrs(&mut self, out: &mut Vec<String>, ind: usize) {
        if self.rng.chance(25) {
            let c = self.sep();
            out.push(format!("{}@derive(Encode{c}Decode)", " ".repeat(ind)));
        }
    }

    fn item(&mut self, out: &mut Vec<String>) {
        if self.rng.chance(20) {
            let c = self.comment(true);
            out.push(c);
        }
        let export = if self.rng.chance(15) {
            format!("{} ", self.kw(Concept::KwExport))
        } else {
            String::new()
        };
        match self.rng.below(9) {
            0..=2 => self.fn_item(out, &export, 0, false),
            3 => {
                self.attrs(out, 0);
                let (t, n) = (self.kw(Concept::KwType), self.type_name());
                let mut line = format!("{export}{t} {n}:");
                self.trailing(&mut line);
                out.push(line);
                for _ in 0..1 + self.rng.below(3) {
                    if self.rng.chance(20) {
                        out.push("    @name(\"wire\")".into());
                    }
                    let (f, ty) = (self.label(), self.ty(1));
                    let d = if self.rng.chance(25) {
                        format!(" = {}", self.head_expr(1, 4))
                    } else {
                        String::new()
                    };
                    let mut line = format!("    {f}: {ty}{d}");
                    self.trailing(&mut line);
                    out.push(line);
                }
            }
            4 => {
                self.attrs(out, 0);
                let (e, n) = (self.kw(Concept::KwEnum), self.type_name());
                out.push(format!("{export}{e} {n}:"));
                for _ in 0..1 + self.rng.below(3) {
                    let v = *self.rng.pick(VARIANTS);
                    let v = self.bound(v);
                    let fields = match self.rng.below(3) {
                        0 => String::new(),
                        1 => {
                            let (l, t) = (self.label(), self.ty(1));
                            format!("({l}: {t})")
                        }
                        _ => {
                            let (a, b) = (self.ty(1), self.ty(1));
                            let c = self.sep();
                            format!("({a}{c}{b})")
                        }
                    };
                    out.push(format!("    {v}{fields}"));
                }
            }
            5 => {
                let (l, n, t) = (self.kw(Concept::KwLet), self.var(), self.ty(1));
                let v = self.expr(2, Ctx::Top, 0);
                let mut line = format!("{export}{l} {n}: {t} = {v}");
                self.trailing(&mut line);
                out.push(line);
            }
            6 => {
                let (i, t) = (self.kw(Concept::KwImpl), self.type_name());
                let tr = if self.rng.chance(40) {
                    format!(" {} {}", self.kw(Concept::KwAs), self.type_name())
                } else {
                    String::new()
                };
                out.push(format!("{i} {t}{tr}:"));
                for k in 0..1 + self.rng.below(2) {
                    if k > 0 {
                        self.spacing(out, 4);
                    }
                    let ex = if self.rng.chance(30) {
                        format!("{} ", self.kw(Concept::KwExport))
                    } else {
                        String::new()
                    };
                    self.fn_item(out, &ex, 4, true);
                }
            }
            7 => {
                let (tr, n) = (self.kw(Concept::KwTrait), self.type_name());
                out.push(format!("{tr} {n}:"));
                let sig = self.sig(true);
                out.push(format!("    {sig}"));
                if self.rng.chance(50) {
                    self.fn_item(out, "", 4, true);
                }
            }
            _ => {
                if self.rng.chance(50) {
                    let x = self.kw(Concept::KwExtern);
                    let sig = self.sig(false);
                    out.push(format!("{x} {sig}"));
                } else {
                    let t = self.kw(Concept::KwTest);
                    let s = self.string(1, 0, None);
                    self.block(out, format!("{t} {s}"), 0, 2);
                }
            }
        }
    }

    fn program(&mut self) -> String {
        let mut out = Vec::new();
        if self.rng.chance(30) {
            let c = self.comment(false);
            out.push(c);
        }
        for _ in 0..self.rng.below(3) {
            let u = self.kw(Concept::KwUse);
            let seg = self.label();
            let tail = match self.rng.below(3) {
                0 => String::new(),
                1 => {
                    let a = self.kw(Concept::KwAs);
                    let n = self.var();
                    format!(" {a} {n}")
                }
                _ => {
                    let s = self.kw(Concept::KwSelf);
                    let n = self.var();
                    let c = self.sep();
                    format!(".{{{s}{c}{n}}}")
                }
            };
            out.push(format!("{u} std/{seg}{tail}"));
        }
        for i in 0..1 + self.rng.below(4) {
            if i > 0 || self.rng.chance(50) {
                self.spacing(&mut out, 0);
            }
            self.item(&mut out);
        }
        if self.rng.chance(20) {
            self.spacing(&mut out, 0);
        }
        // Declare every reserved-in-some-skin word used as a plain name (Q9), before the
        // other `use` lines and after a leading comment.
        if !self.declare.is_empty() {
            let u = self.kw(Concept::KwUse);
            let names: Vec<String> = std::mem::take(&mut self.declare)
                .into_iter()
                .map(|n| self.bound(n))
                .collect();
            let c = self.sep();
            let at = usize::from(out.first().is_some_and(|l| l.starts_with('#')));
            out.insert(at, format!("{u} std/names.{{{}}}", names.join(&c)));
        }
        let mut s = out.join("\n");
        if self.rng.chance(90) {
            s.push('\n');
        }
        s
    }
}

impl Ctx {
    fn in_hole(self) -> bool {
        matches!(self, Ctx::Hole | Ctx::Operand(true))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// Any expression (values, arguments, list items).
    Top,
    /// An operand: compound expressions get parentheses. `true` inside a hole.
    Operand(bool),
    /// An `if`/`while` condition, `for` iterable, `match` scrutinee or guard.
    Cond,
    /// Inside an interpolation hole: one line, no strings.
    Hole,
}

/// The KCF bytes of `text` parsed in `skin` (explicit skin, no inference).
fn kcf_in(text: &str, skin: Skin) -> Vec<u8> {
    let out = kalam_parser::parse_source(text, skin, kalam_base::FileId(0));
    kalam_syntax::kcf::serialize_in(&out.cst, skin)
}

fn check(seed: u64, skin: Skin, src: &str) -> Result<(), String> {
    let before = parse_clean(src, skin).ok_or_else(|| {
        let out = kalam_parser::parse_source(src, skin, kalam_base::FileId(0));
        let codes: Vec<String> = out
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| format!("{}@{:?}", d.code, &src[d.span.range()]))
            .collect();
        format!("the generated program does not parse: {codes:?}")
    })?;
    let f = kalam_fmt::format_strict(src, skin, &cfg()).map_err(|e| format!("fmt: {e}"))?;
    let f2 = kalam_fmt::format_strict(&f, skin, &cfg()).map_err(|e| format!("fmt(fmt): {e}"))?;
    if f2 != f {
        return Err(format!(
            "fmt is not idempotent\n--- fmt\n{f}\n--- fmt(fmt)\n{f2}"
        ));
    }
    let after = parse_clean(&f, skin).ok_or("formatted text does not parse")?;
    if tree(&after) != tree(&before) || comments(&after) != comments(&before) {
        return Err(format!("fmt changed the AST\n--- fmt\n{f}"));
    }
    // P1/P2 on KCF (spec §4.4), not only on the AST: every conversion step, read in its own
    // skin, has the KCF of the text it came from. (`kcf(fmt(src)) == kcf(src)` is not
    // checked: fmt's blank-line, trailing-comma and one-line-block normalization changes KCF
    // today; see the verifier's report.)
    let kcf_f = kcf_in(&f, skin);
    for path in round_trip_paths(skin) {
        let steps = convert_path(&f, &path)?;
        for (i, step) in steps.iter().enumerate() {
            if kcf_in(step, path[i + 1]) != kcf_f {
                return Err(format!("{path:?}: step {i} has another KCF\n{step}"));
            }
        }
        if steps.last() != Some(&f) {
            return Err(format!(
                "{path:?} from fmt(src) is not byte-identical\n--- fmt\n{f}\n--- back\n{}",
                steps.last().unwrap()
            ));
        }
        for (i, step) in steps.iter().enumerate() {
            let s = path[i + 1];
            let a = parse_clean(step, s)
                .ok_or_else(|| format!("{path:?}: {s} text does not parse\n{step}"))?;
            if tree(&a) != tree(&before) || comments(&a) != comments(&before) {
                return Err(format!("{path:?}: {s} text has another AST\n{step}"));
            }
            let g =
                kalam_fmt::format_strict(step, s, &cfg()).map_err(|e| format!("fmt {s}: {e}"))?;
            if &g != step {
                return Err(format!(
                    "{path:?}: converted formatted text is not formatted in {s}\n--- converted\n{step}\n--- fmt\n{g}"
                ));
            }
        }
    }
    // From the unformatted source (one path per program, rotating): the AST survives, and
    // the result formats to `fmt(src)`.
    let paths = round_trip_paths(skin);
    let path = &paths[(seed as usize) % paths.len()];
    let raw = convert_path(src, path)?;
    let kcf_src = kcf_in(src, skin);
    for (i, step) in raw.iter().enumerate() {
        if kcf_in(step, path[i + 1]) != kcf_src {
            return Err(format!(
                "{path:?} from src: step {i} has another KCF\n{step}"
            ));
        }
    }
    let back = raw.last().unwrap();
    let a = parse_clean(back, skin).ok_or("round trip of the source does not parse")?;
    if tree(&a) != tree(&before) || comments(&a) != comments(&before) {
        return Err(format!("{path:?} from src changed the AST\n{back}"));
    }
    let g = kalam_fmt::format_strict(back, skin, &cfg()).map_err(|e| format!("fmt back: {e}"))?;
    if g != f {
        return Err(format!(
            "{path:?} from src does not format to fmt(src)\n{back}"
        ));
    }
    Ok(())
}

#[test]
fn random_programs_format_idempotently_and_round_trip_through_every_skin() {
    let cases: u64 = std::env::var("KALAM_FMT_PROP_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_000);
    let table = KeywordTable::builtin();
    let mut failures = Vec::new();
    let mut lines = 0usize;
    for seed in 1..=cases {
        let skin = Skin::ALL[(seed % 3) as usize];
        let mut g = Gen {
            rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            skin,
            table,
            comments: 0,
            quiet: 0,
            declare: Vec::new(),
        };
        let src = g.program();
        lines += src.lines().count();
        if let Err(why) = check(seed, skin, &src) {
            failures.push(format!("seed {seed} ({skin}): {why}\n--- program\n{src}"));
            if failures.len() >= 3 {
                break;
            }
        }
    }
    eprintln!("{cases} generated programs, {lines} lines");
    assert!(failures.is_empty(), "{}", failures.join("\n\n=====\n\n"));
}
