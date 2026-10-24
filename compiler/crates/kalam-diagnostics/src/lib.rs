//! Diagnostics: the data model, the three-language message catalog and the renderers.
//!
//! Design (ARCHITECTURE.md §12):
//! - A [`Diagnostic`] stores a stable code, spans, arguments and fixes, never rendered prose.
//! - Prose is rendered late from the [`catalog`], in the message language (`KALAM_LANG`:
//!   `en`, `ar` or `franco`). Code inside a message stays in the *file's* skin. In this
//!   scaffold, arguments are pre-rendered strings and the producer renders code in the file's
//!   skin; Phase 1 replaces them with typed arguments (types, concepts, KCF fragments).
//! - The JSON renderer follows spec §21.4 and carries all three languages.

pub mod catalog;

use kalam_base::{FileId, LineIndex, Skin, Span};
use serde_json::json;

/// The message language. It uses the same three IDs as skins (spec §21.4 rule 1), but it is
/// chosen by the user (`KALAM_LANG`), not by the file.
pub type Lang = Skin;

/// Diagnostic severity, derived from the code prefix (spec §27).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Hint,
    Lint,
}

impl Severity {
    /// `SEC` and `K` codes are errors, `W` warnings, `H` hints and `L` lints.
    pub fn from_code(code: &str) -> Severity {
        if code.starts_with('W') {
            Severity::Warning
        } else if code.starts_with('H') {
            Severity::Hint
        } else if code.starts_with('L') {
            Severity::Lint
        } else {
            Severity::Error
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Hint => "hint",
            Severity::Lint => "lint",
        }
    }
}

/// Whether a fix may be applied without review (`kalam fix`, agents) or needs a choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applicability {
    Machine,
    Suggestion,
}

impl Applicability {
    pub fn as_str(self) -> &'static str {
        match self {
            Applicability::Machine => "machine",
            Applicability::Suggestion => "suggestion",
        }
    }
}

/// Replace `span` (in the diagnostic's file) with `insert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub insert: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    /// Short English description, used in JSON output and LSP code-action titles.
    pub title: String,
    pub applicability: Applicability,
    pub edits: Vec<Edit>,
}

impl Fix {
    /// A single-edit, machine-applicable fix.
    pub fn machine(title: impl Into<String>, span: Span, insert: impl Into<String>) -> Fix {
        Fix {
            title: title.into(),
            applicability: Applicability::Machine,
            edits: vec![Edit {
                span,
                insert: insert.into(),
            }],
        }
    }

    pub fn suggestion(title: impl Into<String>, span: Span, insert: impl Into<String>) -> Fix {
        Fix {
            applicability: Applicability::Suggestion,
            ..Fix::machine(title, span, insert)
        }
    }
}

/// A secondary span with a short explanation ("`price` was inferred as `Float` here").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub file: FileId,
    pub span: Span,
    pub args: Vec<(&'static str, String)>,
    pub labels: Vec<Label>,
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    pub fn new(code: &'static str, file: FileId, span: Span) -> Diagnostic {
        Diagnostic {
            code,
            severity: Severity::from_code(code),
            file,
            span,
            args: Vec::new(),
            labels: Vec::new(),
            fixes: Vec::new(),
        }
    }

    pub fn arg(mut self, name: &'static str, value: impl Into<String>) -> Diagnostic {
        self.args.push((name, value.into()));
        self
    }

    pub fn label(mut self, span: Span, text: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            text: text.into(),
        });
        self
    }

    pub fn fix(mut self, fix: Fix) -> Diagnostic {
        self.fixes.push(fix);
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// The headline in `lang` (spec §21.4 template).
    pub fn title(&self, lang: Lang) -> String {
        match catalog::lookup(self.code) {
            Some(entry) => entry.title[lang.index()].to_string(),
            None => self.code.to_string(),
        }
    }

    /// The message body in `lang`.
    pub fn message(&self, lang: Lang) -> String {
        match catalog::lookup(self.code) {
            Some(entry) => catalog::render_template(entry.message[lang.index()], &self.args),
            None => format!("{} (no catalog entry yet)", self.code),
        }
    }
}

/// What a renderer needs to know about the file a diagnostic points into.
pub struct SourceView<'a> {
    pub path: &'a str,
    pub text: &'a str,
    pub skin: Skin,
    pub lines: &'a LineIndex,
}

impl<'a> SourceView<'a> {
    pub fn new(path: &'a str, text: &'a str, skin: Skin, lines: &'a LineIndex) -> SourceView<'a> {
        SourceView {
            path,
            text,
            skin,
            lines,
        }
    }
}

/// Renders the human-readable terminal form (spec §21.4).
///
/// For `ar` files the spec asks for a second excerpt re-printed in the English skin
/// (§6.1 rule 5). That needs the CST printer, so the caller passes it in once it exists.
pub fn render_human(diag: &Diagnostic, src: &SourceView<'_>, lang: Lang, en_excerpt: Option<&str>) -> String {
    let mut out = String::new();
    let title = diag.title(lang);
    let used = 3 + title.chars().count() + 1 + 1 + src.path.chars().count() + 2 + diag.code.len();
    let dashes = if used < 80 { 80 - used } else { 3 };
    out.push_str(&format!(
        "-- {} {} {}  {}\n\n",
        title,
        "-".repeat(dashes),
        src.path,
        diag.code
    ));
    out.push_str(&diag.message(lang));
    out.push_str("\n\n");

    let start = (diag.span.start as usize).min(src.text.len()) as u32;
    let line = src.lines.line_of(start);
    let range = src.lines.line_range(src.text, line);
    let line_text = &src.text[range.clone()];
    let number = (line + 1).to_string();
    out.push_str(&format!("{}\u{2502} {}\n", number, line_text));

    let col = src.text[range.start..start as usize].chars().count();
    let end = (diag.span.end as usize).min(range.end).max(start as usize);
    let width = src.text[start as usize..end].chars().count().max(1);
    out.push_str(&" ".repeat(number.len() + 2 + col));
    out.push_str(&"^".repeat(width));
    out.push('\n');

    if let Some(excerpt) = en_excerpt {
        out.push_str(&format!("{}\u{2502} {}\n", number, excerpt));
    }

    for fix in &diag.fixes {
        for edit in &fix.edits {
            if !edit.insert.is_empty() {
                out.push_str(&format!("\n{} `{}`\n", catalog::ui(lang, catalog::Ui::Try), edit.insert));
            }
        }
    }
    out.push_str(&format!(
        "\n{} kalam explain {}\n",
        catalog::ui(lang, catalog::Ui::LearnMore),
        diag.code
    ));
    out
}

/// Renders one diagnostic as the JSON object of spec §21.4. All three message languages
/// are included, so an agent or LSP client can choose.
pub fn render_json(diag: &Diagnostic, src: &SourceView<'_>) -> serde_json::Value {
    let pos = |offset: u32| {
        let lc = src.lines.line_col(src.text, offset.min(src.text.len() as u32));
        json!({"line": lc.line, "col": lc.col})
    };
    let span = |s: Span| json!({"start": pos(s.start), "end": pos(s.end)});
    let labels: Vec<_> = diag
        .labels
        .iter()
        .map(|l| json!({"span": span(l.span), "text": l.text}))
        .collect();
    let fixes: Vec<_> = diag
        .fixes
        .iter()
        .map(|f| {
            let edits: Vec<_> = f
                .edits
                .iter()
                .map(|e| json!({"span": span(e.span), "insert": e.insert}))
                .collect();
            json!({"title": f.title, "applicability": f.applicability.as_str(), "edits": edits})
        })
        .collect();
    json!({
        "code": diag.code,
        "severity": diag.severity.as_str(),
        "message": {
            "en": diag.message(Skin::En),
            "ar": diag.message(Skin::Ar),
            "franco": diag.message(Skin::Franco),
        },
        "file": src.path,
        "skin": src.skin.id(),
        "span": span(diag.span),
        "labels": labels,
        "fixes": fixes,
        "explain": format!("https://kalam.dev/errors/{}", diag.code),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_from_code_prefix() {
        assert_eq!(Severity::from_code("SEC0110"), Severity::Error);
        assert_eq!(Severity::from_code("K0127"), Severity::Error);
        assert_eq!(Severity::from_code("W0403"), Severity::Warning);
        assert_eq!(Severity::from_code("H0202"), Severity::Hint);
    }

    #[test]
    fn human_render_has_code_excerpt_and_caret() {
        let text = "let x = 007\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.kl", text, Skin::En, &lines);
        let d = Diagnostic::new("K0125", FileId(0), Span::new(8, 11))
            .arg("literal", "007")
            .arg("fixed", "7")
            .fix(Fix::machine("remove leading zeros", Span::new(8, 11), "7"));
        let out = render_human(&d, &src, Skin::En, None);
        assert!(out.contains("K0125"));
        assert!(out.contains("1\u{2502} let x = 007"));
        assert!(out.contains("^^^"));
        assert!(out.contains("kalam explain K0125"));
    }

    #[test]
    fn json_render_has_all_three_languages() {
        let text = "x\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.kl", text, Skin::En, &lines);
        let d = Diagnostic::new("K0904", FileId(0), Span::new(0, 1));
        let v = render_json(&d, &src);
        assert_eq!(v["code"], "K0904");
        assert!(v["message"]["ar"].as_str().unwrap().len() > 0);
        assert_eq!(v["span"]["start"]["line"], 1);
    }

    #[test]
    fn every_catalog_entry_has_three_languages() {
        for entry in catalog::CATALOG {
            for text in entry.title.iter().chain(entry.message.iter()) {
                assert!(!text.is_empty(), "{} has an empty translation", entry.code);
            }
        }
    }
}
