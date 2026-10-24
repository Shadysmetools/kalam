//! Foundation types shared by every Kalam crate: skins, file ids, spans and line index.
//!
//! This crate has no dependencies and performs no I/O, so it builds for `wasm32` unchanged.

use std::fmt;

/// A keyword skin. One file is written in exactly one skin, declared by its extension
/// (spec §4.1, DR-18). Skin IDs are `en`, `ar` and `franco` everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Skin {
    En,
    Ar,
    Franco,
}

impl Skin {
    pub const ALL: [Skin; 3] = [Skin::En, Skin::Ar, Skin::Franco];

    /// The stable skin ID used by `KALAM_LANG`, `kalam.toml`, `keywords.json` and CLI flags.
    pub fn id(self) -> &'static str {
        match self {
            Skin::En => "en",
            Skin::Ar => "ar",
            Skin::Franco => "franco",
        }
    }

    pub fn from_id(id: &str) -> Option<Skin> {
        match id {
            "en" => Some(Skin::En),
            "ar" => Some(Skin::Ar),
            "franco" => Some(Skin::Franco),
            _ => None,
        }
    }

    /// The file extension that declares this skin.
    pub fn extension(self) -> &'static str {
        match self {
            Skin::En => ".kl",
            Skin::Ar => ".ar.kl",
            Skin::Franco => ".franco.kl",
        }
    }

    /// Reads the skin from a file name (DR-18: the extension is the only declaration).
    /// The longer suffixes are checked first, because every Kalam file ends in `.kl`.
    pub fn from_path(path: &str) -> Option<Skin> {
        if path.ends_with(".ar.kl") {
            Some(Skin::Ar)
        } else if path.ends_with(".franco.kl") {
            Some(Skin::Franco)
        } else if path.ends_with(".kl") {
            Some(Skin::En)
        } else {
            None
        }
    }

    /// Base paragraph direction used by first-party renderers (spec §6.1 rule 2).
    pub fn is_rtl(self) -> bool {
        matches!(self, Skin::Ar)
    }

    /// Index into per-skin arrays (`[en, ar, franco]`).
    pub fn index(self) -> usize {
        match self {
            Skin::En => 0,
            Skin::Ar => 1,
            Skin::Franco => 2,
        }
    }
}

impl fmt::Display for Skin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// Identifies one source file inside a compilation session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct FileId(pub u32);

/// A half-open byte range `[start, end)` into one file's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Span {
        debug_assert!(start <= end);
        Span { start, end }
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The smallest span covering both.
    pub fn cover(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// A span together with the file it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct FileSpan {
    pub file: FileId,
    pub span: Span,
}

impl FileSpan {
    pub fn new(file: FileId, span: Span) -> FileSpan {
        FileSpan { file, span }
    }
}

/// A 1-based line and column. Columns count Unicode scalar values; the LSP converts
/// them to UTF-16 when the client negotiates that encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Maps byte offsets to lines and columns.
#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        LineIndex { line_starts }
    }

    /// The 0-based line number containing `offset`.
    pub fn line_of(&self, offset: u32) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    /// Byte range of a 0-based line, excluding its line terminator.
    pub fn line_range(&self, text: &str, line: usize) -> std::ops::Range<usize> {
        let start = self.line_starts[line] as usize;
        let mut end = match self.line_starts.get(line + 1) {
            Some(next) => *next as usize - 1,
            None => text.len(),
        };
        if end > start && text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        start..end
    }

    /// 1-based line and column of `offset`. `offset` must be on a char boundary.
    pub fn line_col(&self, text: &str, offset: u32) -> LineCol {
        let line = self.line_of(offset);
        let start = self.line_starts[line] as usize;
        let end = (offset as usize).min(text.len());
        let col = text[start..end].chars().count() as u32 + 1;
        LineCol {
            line: line as u32 + 1,
            col,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_from_path_checks_long_suffixes_first() {
        assert_eq!(Skin::from_path("src/users.kl"), Some(Skin::En));
        assert_eq!(Skin::from_path("src/users.ar.kl"), Some(Skin::Ar));
        assert_eq!(Skin::from_path("src/users.franco.kl"), Some(Skin::Franco));
        assert_eq!(Skin::from_path("src/users.ts"), None);
    }

    #[test]
    fn line_col_counts_scalar_values() {
        let text = "ab\nإذا x\n";
        let idx = LineIndex::new(text);
        assert_eq!(idx.line_col(text, 0), LineCol { line: 1, col: 1 });
        let x = text.find('x').unwrap() as u32;
        assert_eq!(idx.line_col(text, x), LineCol { line: 2, col: 5 });
    }
}
