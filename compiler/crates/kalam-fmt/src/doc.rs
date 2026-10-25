//! A small Wadler/Prettier-style document and its layout engine.
//!
//! Groups break outermost first: a group is printed flat when it fits in the remaining
//! width (with whatever follows it up to the next possible line break), otherwise its own
//! line breaks become newlines and its child groups are decided again.
//!
//! Widths are **skin-independent**: every [`Doc::Text`] carries the width of its English-skin
//! spelling, so one AST gets the same line breaks in `en`, `ar` and `franco`, and a converted
//! formatted file is itself formatted (see the crate docs).

pub(crate) const WIDTH: usize = 100;
pub(crate) const INDENT: usize = 4;

#[derive(Clone, Debug)]
pub(crate) enum Doc {
    /// Text and its English-skin width. Only literal string text may contain `\n`
    /// (`"""` strings); its parts are the same in every skin.
    Text(String, usize),
    /// A space when flat, a newline when broken.
    Line,
    /// Nothing when flat, a newline when broken.
    Soft,
    /// Text printed only when the enclosing group is broken (the trailing comma).
    BreakOnly(String),
    Group(Vec<Doc>),
    Indent(Vec<Doc>),
    Concat(Vec<Doc>),
    /// A call whose last argument is a bracketed literal: printed like `full` when that fits
    /// on the line; otherwise, when `prefix` (everything up to and including the literal's
    /// opening bracket, flat) fits, the call "hugs" the literal (`f(a, [` … `])`) and only the
    /// literal breaks; otherwise like `full`.
    Hug {
        prefix: Vec<Doc>,
        last: Box<Doc>,
        suffix: Vec<Doc>,
        full: Box<Doc>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Flat,
    Break,
}

/// Text whose width is its character count (operators, literal text, layout).
pub(crate) fn text(s: impl Into<String>) -> Doc {
    let s = s.into();
    let w = s.chars().count();
    Doc::Text(s, w)
}

pub(crate) fn concat(parts: Vec<Doc>) -> Doc {
    Doc::Concat(parts)
}

/// The width of `doc` printed flat, or `None` if it contains a newline.
pub(crate) fn flat_width(doc: &Doc) -> Option<usize> {
    match doc {
        Doc::Text(s, w) => (!s.contains('\n')).then_some(*w),
        Doc::Line => Some(1),
        Doc::Soft | Doc::BreakOnly(_) => Some(0),
        Doc::Group(c) | Doc::Indent(c) | Doc::Concat(c) => c
            .iter()
            .try_fold(0usize, |acc, d| flat_width(d).map(|w| acc + w)),
        Doc::Hug { full, .. } => flat_width(full),
    }
}

/// Prints `doc` flat (holes inside string literals are always one line).
pub(crate) fn flat_text(doc: &Doc, out: &mut String) {
    match doc {
        Doc::Text(s, _) => out.push_str(s),
        Doc::Line => out.push(' '),
        Doc::Soft | Doc::BreakOnly(_) => {}
        Doc::Group(c) | Doc::Indent(c) | Doc::Concat(c) => {
            for d in c {
                flat_text(d, out);
            }
        }
        Doc::Hug { full, .. } => flat_text(full, out),
    }
}

/// Lays `doc` out starting at column `indent` (the caller has written the indentation).
pub(crate) fn layout(doc: &Doc, indent: usize, out: &mut String) {
    let mut pos = indent;
    let mut stack: Vec<(usize, Mode, &Doc)> = vec![(indent, Mode::Break, doc)];
    while let Some((ind, mode, d)) = stack.pop() {
        match d {
            Doc::Text(s, w) => {
                out.push_str(s);
                match s.rfind('\n') {
                    Some(p) => pos = s[p + 1..].chars().count(),
                    None => pos += w,
                }
            }
            Doc::Line | Doc::Soft => {
                if mode == Mode::Break {
                    out.push('\n');
                    out.extend(std::iter::repeat(' ').take(ind));
                    pos = ind;
                } else if matches!(d, Doc::Line) {
                    out.push(' ');
                    pos += 1;
                }
            }
            Doc::BreakOnly(s) => {
                if mode == Mode::Break {
                    out.push_str(s);
                    pos += 1;
                }
            }
            Doc::Concat(c) => stack.extend(c.iter().rev().map(|x| (ind, mode, x))),
            Doc::Indent(c) => stack.extend(c.iter().rev().map(|x| (ind + INDENT, mode, x))),
            Doc::Group(c) => {
                let m = if mode == Mode::Flat || fits(WIDTH as isize - pos as isize, d, &stack) {
                    Mode::Flat
                } else {
                    Mode::Break
                };
                stack.extend(c.iter().rev().map(|x| (ind, m, x)));
            }
            Doc::Hug {
                prefix,
                last,
                suffix,
                full,
            } => {
                let rem = WIDTH as isize - pos as isize;
                // The prefix and the literal's opening bracket must fit on this line.
                let prefix_fits =
                    flat_width(&Doc::Concat(prefix.clone())).is_some_and(|w| (w as isize) < rem);
                if mode == Mode::Flat || fits(rem, full, &stack) || !prefix_fits {
                    stack.push((ind, mode, full));
                } else {
                    stack.extend(suffix.iter().rev().map(|x| (ind, Mode::Break, x)));
                    stack.push((ind, Mode::Break, last));
                    stack.extend(prefix.iter().rev().map(|x| (ind, Mode::Flat, x)));
                }
            }
        }
    }
}

/// Whether `doc` printed flat, followed by the rest of the line, fits in `rem` columns.
fn fits(mut rem: isize, doc: &Doc, rest: &[(usize, Mode, &Doc)]) -> bool {
    let mut work: Vec<(Mode, &Doc)> = vec![(Mode::Flat, doc)];
    let mut rest_i = rest.len();
    loop {
        if rem < 0 {
            return false;
        }
        let (mode, d) = match work.pop() {
            Some(x) => x,
            None => {
                if rest_i == 0 {
                    return true;
                }
                rest_i -= 1;
                (rest[rest_i].1, rest[rest_i].2)
            }
        };
        match d {
            Doc::Text(s, w) => match s.find('\n') {
                Some(p) => return rem >= s[..p].chars().count() as isize,
                None => rem -= *w as isize,
            },
            Doc::Line | Doc::Soft => {
                if mode == Mode::Break {
                    return true;
                }
                if matches!(d, Doc::Line) {
                    rem -= 1;
                }
            }
            Doc::BreakOnly(_) => {
                if mode == Mode::Break {
                    rem -= 1;
                }
            }
            Doc::Group(c) | Doc::Indent(c) | Doc::Concat(c) => {
                work.extend(c.iter().rev().map(|x| (mode, x)));
            }
            Doc::Hug { full, .. } => work.push((mode, full)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Doc {
        let mut inner = vec![Doc::Soft];
        for (i, it) in items.iter().enumerate() {
            if i > 0 {
                inner.push(text(","));
                inner.push(Doc::Line);
            }
            inner.push(text(*it));
        }
        inner.push(Doc::BreakOnly(",".into()));
        Doc::Group(vec![text("["), Doc::Indent(inner), Doc::Soft, text("]")])
    }

    #[test]
    fn a_group_that_fits_stays_flat() {
        let mut out = String::new();
        layout(&list(&["a", "b"]), 0, &mut out);
        assert_eq!(out, "[a, b]");
    }

    #[test]
    fn a_long_group_breaks_one_item_per_line_with_a_trailing_comma() {
        let long = "x".repeat(60);
        let mut out = String::new();
        layout(&list(&[&long, &long]), 4, &mut out);
        assert_eq!(out, format!("[\n        {long},\n        {long},\n    ]"));
    }

    #[test]
    fn flat_width_and_text() {
        let d = list(&["ab", "c"]);
        assert_eq!(flat_width(&d), Some(7));
        let mut s = String::new();
        flat_text(&d, &mut s);
        assert_eq!(s, "[ab, c]");
        assert_eq!(flat_width(&text("a\nb")), None);
    }
}
