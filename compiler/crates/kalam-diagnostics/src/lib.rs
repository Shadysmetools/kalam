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
pub mod code;
pub mod explain;
pub mod labels;

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

    /// The message body in `lang`. Keyword placeholders the emitter did not fill are spelled
    /// in `lang`'s own skin; renderers that know the file use [`Diagnostic::message_in`].
    pub fn message(&self, lang: Lang) -> String {
        self.message_in(lang, lang)
    }

    /// The message body in `lang`, with code (keyword placeholders) in `code_skin`, the skin
    /// of the file the diagnostic points into (spec §21.4 rule 1).
    pub fn message_in(&self, lang: Lang, code_skin: Skin) -> String {
        match catalog::lookup(self.code) {
            Some(entry) => {
                catalog::render_in_skin(entry.message[lang.index()], &self.args, code_skin)
            }
            None => format!("{} (no catalog entry yet)", self.code),
        }
    }

    /// How to fix it, in `lang` (keyword placeholders in `lang`'s own skin).
    pub fn help(&self, lang: Lang) -> Option<String> {
        self.help_in(lang, lang)
    }

    /// How to fix it, in `lang`, with code in `code_skin`. `None` for an unknown code.
    pub fn help_in(&self, lang: Lang, code_skin: Skin) -> Option<String> {
        catalog::lookup(self.code)
            .map(|entry| catalog::render_in_skin(entry.help[lang.index()], &self.args, code_skin))
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

/// Renders the human-readable terminal form (spec §21.4): the header, the message, the
/// excerpt with the primary span underlined (`^`) and every label (Elm and Rust style; see
/// `render_excerpt`), one `Try:` per fix edit (an edit of several lines as an indented
/// block), the help and the `kalam explain` footer.
///
/// For `ar` files the spec asks for a second excerpt re-printed in the English skin
/// (§6.1 rule 5). That needs the CST printer, so the caller passes it in once it exists.
pub fn render_human(
    diag: &Diagnostic,
    src: &SourceView<'_>,
    lang: Lang,
    en_excerpt: Option<&str>,
) -> String {
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
    out.push_str(&diag.message_in(lang, src.skin));
    out.push_str("\n\n");
    out.push_str(&render_excerpt(diag, src, lang, en_excerpt));

    let try_word = catalog::ui(lang, catalog::Ui::Try);
    for fix in &diag.fixes {
        for (i, edit) in fix.edits.iter().enumerate() {
            // A rename writes the same text at every use: show it once.
            if edit.insert.is_empty() || fix.edits[..i].iter().any(|e| e.insert == edit.insert) {
                continue;
            }
            match code_block(src, edit) {
                // An edit of several lines is shown as the code it inserts, indented under
                // `Try:` (spec §21.4 example), never as one backtick span across lines.
                Some(lines) => {
                    out.push_str(&format!("\n{try_word}\n\n"));
                    for l in lines {
                        if l.is_empty() {
                            out.push('\n');
                        } else {
                            out.push_str(&format!("    {l}\n"));
                        }
                    }
                }
                None => out.push_str(&format!("\n{try_word} `{}`\n", edit.insert)),
            }
        }
    }
    if let Some(help) = diag.help_in(lang, src.skin) {
        out.push_str(&format!(
            "\n{} {}\n",
            catalog::ui(lang, catalog::Ui::Help),
            help
        ));
    }
    out.push_str(&format!(
        "\n{} kalam explain {}\n",
        catalog::ui(lang, catalog::Ui::LearnMore),
        diag.code
    ));
    out
}

/// One underline in the excerpt: the primary span (`^`) or a label (`-`), with its text.
struct Marker {
    line: usize,
    col: usize,
    width: usize,
    primary: bool,
    text: Option<String>,
}

/// The marker for `span`: its first line, and the columns it covers on that line (a span
/// that runs over several lines is underlined to the end of its first line).
fn marker(src: &SourceView<'_>, span: Span, primary: bool, text: Option<String>) -> Marker {
    let start = (span.start as usize).min(src.text.len());
    let line = src.lines.line_of(start as u32);
    let range = src.lines.line_range(src.text, line);
    let start = start.max(range.start);
    let col = src.text[range.start..start].chars().count();
    let end = (span.end as usize).min(range.end).max(start);
    let width = src.text[start..end].chars().count().max(1);
    Marker {
        line,
        col,
        width,
        primary,
        text,
    }
}

/// The source excerpt: every line that holds the primary span or a label, in line order,
/// each followed by its underlines. A label on exactly the primary span is written after
/// the carets; any other label gets its own `-` underline under its own line (Rust and Elm
/// style). Labels are in the message language ([`labels::localize`]); one that has no
/// translation is left out of an Arabic or Franco message.
fn render_excerpt(
    diag: &Diagnostic,
    src: &SourceView<'_>,
    lang: Lang,
    en_excerpt: Option<&str>,
) -> String {
    let mut inline: Vec<String> = Vec::new();
    let mut markers: Vec<Marker> = Vec::new();
    for l in &diag.labels {
        let Some(text) = labels::localize(&l.text, lang, src.skin) else {
            continue;
        };
        if l.span == diag.span {
            inline.push(text);
        } else {
            markers.push(marker(src, l.span, false, Some(text)));
        }
    }
    let primary_text = (!inline.is_empty()).then(|| inline.join("; "));
    let primary = marker(src, diag.span, true, primary_text);
    let primary_line = primary.line;
    markers.push(primary);
    // Line order; on one line the primary span first, then labels left to right.
    markers.sort_by_key(|m| (m.line, !m.primary, m.col));
    let gutter = markers
        .iter()
        .map(|m| (m.line + 1).to_string().len())
        .max()
        .unwrap_or(1);

    let mut out = String::new();
    let mut i = 0;
    while i < markers.len() {
        let line = markers[i].line;
        let range = src.lines.line_range(src.text, line);
        let number = format!("{:>gutter$}", line + 1);
        out.push_str(&format!("{number}\u{2502} {}\n", &src.text[range]));
        while i < markers.len() && markers[i].line == line {
            let m = &markers[i];
            let mark = if m.primary { "^" } else { "-" };
            out.push_str(&" ".repeat(gutter + 2 + m.col));
            out.push_str(&mark.repeat(m.width));
            if let Some(text) = &m.text {
                out.push(' ');
                out.push_str(text);
            }
            out.push('\n');
            i += 1;
        }
        if line == primary_line {
            if let Some(excerpt) = en_excerpt {
                out.push_str(&format!("{number}\u{2502} {excerpt}\n"));
            }
        }
    }
    out
}

/// The lines a multi-line edit inserts, for display: blank lines at either end dropped and
/// the common indentation removed. The first line's indentation is the column the edit
/// starts at plus its own leading spaces. `None` for an edit of one line.
fn code_block(src: &SourceView<'_>, edit: &Edit) -> Option<Vec<String>> {
    if !edit.insert.contains('\n') {
        return None;
    }
    let start = (edit.span.start as usize).min(src.text.len());
    let line_start = src.text[..start].rfind('\n').map_or(0, |i| i + 1);
    let first_col = src.text[line_start..start].chars().count();
    let mut lines: Vec<(usize, &str)> = edit
        .insert
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            let l = l.trim_end();
            let base = if i == 0 { first_col } else { 0 };
            (base + l.len() - l.trim_start_matches(' ').len(), l)
        })
        .collect();
    while lines.first().is_some_and(|(_, l)| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|(_, l)| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return None;
    }
    let indent = lines
        .iter()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(n, _)| *n)
        .min()
        .unwrap_or(0);
    Some(
        lines
            .iter()
            .map(|(n, l)| {
                if l.trim().is_empty() {
                    String::new()
                } else {
                    let body = l.trim_start_matches(' ');
                    format!("{}{body}", " ".repeat(n - indent))
                }
            })
            .collect(),
    )
}

/// Renders one diagnostic as the JSON object of spec §21.4. All three message languages
/// are included, so an agent or LSP client can choose. `help` (how to fix it, in all three
/// languages, `null` for a code with no catalog entry) is an addition to the §21.4 example.
pub fn render_json(diag: &Diagnostic, src: &SourceView<'_>) -> serde_json::Value {
    let help = match catalog::lookup(diag.code) {
        Some(_) => json!({
            "en": diag.help_in(Skin::En, src.skin),
            "ar": diag.help_in(Skin::Ar, src.skin),
            "franco": diag.help_in(Skin::Franco, src.skin),
        }),
        None => serde_json::Value::Null,
    };
    let pos = |offset: u32| {
        let lc = src
            .lines
            .line_col(src.text, offset.min(src.text.len() as u32));
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
            "en": diag.message_in(Skin::En, src.skin),
            "ar": diag.message_in(Skin::Ar, src.skin),
            "franco": diag.message_in(Skin::Franco, src.skin),
        },
        "help": help,
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
        assert!(out.contains("Help: Write `7`."), "{out}");
    }

    /// The excerpt and `Try:` part of a rendering: from the first gutter line to `Help:`.
    fn body(out: &str) -> &str {
        let start = out
            .find('\u{2502}')
            .map_or(0, |i| out[..i].rfind("\n\n").unwrap() + 2);
        let end = out
            .find("\nHelp:")
            .or_else(|| out.find("\nLearn more:"))
            .unwrap();
        &out[start..end]
    }

    #[test]
    fn multi_line_inserts_print_as_an_indented_block() {
        // K1001's fix appends an arm after the last one (a leading newline, then the arm at
        // the arms' column).
        let text =
            "fn f(l: Light):\n    match l:\n        case .Red:\n            print(\"stop\")\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.kl", text, Skin::En, &lines);
        let end = text.len() as u32 - 1;
        let d = Diagnostic::new("K1001", FileId(0), Span::new(20, 28))
            .arg("missing", ".Amber")
            .fix(Fix::machine(
                "add the missing cases",
                Span::new(end, end),
                "\n        case .Amber: panic(\"TODO\")\n        case .Green: panic(\"TODO\")",
            ));
        let out = render_human(&d, &src, Skin::En, None);
        assert_eq!(
            body(&out),
            "2\u{2502}     match l:\n       ^^^^^^^^\n\nTry:\n\n    case .Amber: panic(\"TODO\")\n    case .Green: panic(\"TODO\")\n",
            "{out}"
        );
        assert!(!out.contains("Try: `"), "{out}");
    }

    #[test]
    fn a_block_that_starts_mid_line_is_dedented_from_its_column() {
        // K0925 replaces `switch n {` (column 4) with a `match` and its arms (column 8).
        let text = "fn f(n: Int):\n    switch n {\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.kl", text, Skin::En, &lines);
        let d = Diagnostic::new("K0925", FileId(0), Span::new(18, 28)).fix(Fix::machine(
            "use match",
            Span::new(18, 28),
            "match n:\n        case 0: return \"zero\"",
        ));
        let out = render_human(&d, &src, Skin::En, None);
        assert!(
            out.contains("\nTry:\n\n    match n:\n        case 0: return \"zero\"\n\nHelp:"),
            "{out}"
        );
    }

    #[test]
    fn one_line_edits_keep_one_try_line_each() {
        let text = "let x = a === b\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.kl", text, Skin::En, &lines);
        let d = Diagnostic::new("K0901", FileId(0), Span::new(10, 13)).fix(Fix {
            title: "two edits".into(),
            applicability: Applicability::Machine,
            edits: vec![
                Edit {
                    span: Span::new(10, 13),
                    insert: "==".into(),
                },
                Edit {
                    span: Span::new(0, 0),
                    insert: String::new(),
                },
                Edit {
                    span: Span::new(14, 15),
                    insert: "c".into(),
                },
            ],
        });
        let out = render_human(&d, &src, Skin::En, None);
        assert!(out.contains("\nTry: `==`\n\nTry: `c`\n"), "{out}");
        // A rename inserts the same name at each use; it is shown once.
        let rename = Fix {
            title: "rename".into(),
            applicability: Applicability::Machine,
            edits: vec![
                Edit {
                    span: Span::new(4, 5),
                    insert: "y".into(),
                },
                Edit {
                    span: Span::new(8, 9),
                    insert: "y".into(),
                },
            ],
        };
        let d = Diagnostic::new("K0405", FileId(0), Span::new(4, 5)).fix(rename);
        let out = render_human(&d, &src, Skin::En, None);
        assert_eq!(out.matches("Try: `y`").count(), 1, "{out}");
    }

    #[test]
    fn a_label_on_the_primary_span_follows_the_carets() {
        let text = "fn main():\n    let x: Int = \"hi\"\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.kl", text, Skin::En, &lines);
        let span = Span::new(28, 32);
        let d = Diagnostic::new("K0600", FileId(0), span)
            .arg("expected", "Int")
            .arg("found", "String")
            .label(span, "expected `Int`, found `String`");
        let out = render_human(&d, &src, Skin::En, None);
        assert_eq!(
            body(&out),
            "2\u{2502}     let x: Int = \"hi\"\n                    ^^^^ expected `Int`, found `String`\n",
            "{out}"
        );
        // In Arabic, through the label table; the types are already in the file's skin.
        let text = "دالة رئيسي():\n    ليكن x: عدد = \"hi\"\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.ar.kl", text, Skin::Ar, &lines);
        let start = text.find("\"hi\"").unwrap() as u32;
        let span = Span::new(start, start + 4);
        let d = Diagnostic::new("K0600", FileId(0), span).label(span, "expected `عدد`, found `نص`");
        let out = render_human(&d, &src, Skin::Ar, None);
        assert!(out.contains("^^^^ المطلوب `عدد`، والموجود `نص`\n"), "{out}");
    }

    #[test]
    fn labels_elsewhere_get_their_own_line_and_a_shared_gutter() {
        let mut text = String::new();
        for i in 1..=8 {
            text.push_str(&format!("# line {i}\n"));
        }
        text.push_str("let total = 1\n# line 10\nlet total = 2\n");
        let first = text.find("total").unwrap() as u32;
        let second = text.rfind("total").unwrap() as u32;
        let lines = LineIndex::new(&text);
        let src = SourceView::new("a.kl", &text, Skin::En, &lines);
        let d = Diagnostic::new("K0405", FileId(0), Span::new(second, second + 5))
            .arg("name", "`total`")
            .label(
                Span::new(first, first + 5),
                "`total` is already declared here",
            );
        let out = render_human(&d, &src, Skin::En, None);
        assert_eq!(
            body(&out),
            " 9\u{2502} let total = 1\n        ----- `total` is already declared here\n11\u{2502} let total = 2\n        ^^^^^\n",
            "{out}"
        );
        // Arabic message: translated; a label without a translation is left out.
        let out = render_human(&d, &src, Skin::Ar, None);
        assert!(out.contains("----- هنا عُرِّف `total` أول مرة\n"), "{out}");
        let d = Diagnostic::new("K0405", FileId(0), Span::new(second, second + 5))
            .label(Span::new(first, first + 5), "`total` was moved here");
        let out = render_human(&d, &src, Skin::Ar, None);
        assert!(
            !out.contains("\n        -----") && !out.contains("moved"),
            "{out}"
        );
        assert!(out.contains("11\u{2502} let total = 2\n"), "{out}");
        let out = render_human(&d, &src, Skin::En, None);
        assert!(out.contains("----- `total` was moved here\n"), "{out}");
    }

    #[test]
    fn keywords_in_messages_follow_the_file_skin() {
        // K0731's message names `if` and `else`: an Arabic reader of a Franco file sees the
        // Franco keywords (code stays in the file's skin, spec §21.4 rule 1).
        let text = "x\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.franco.kl", text, Skin::Franco, &lines);
        let d = Diagnostic::new("K0731", FileId(0), Span::new(0, 1));
        let out = render_human(&d, &src, Skin::Ar, None);
        assert!(out.contains("`law`") && out.contains("`walla`"), "{out}");
        assert!(!out.contains("{kw_"), "{out}");
        let v = render_json(&d, &src);
        assert!(v["message"]["en"].as_str().unwrap().contains("`walla`"));
        // Without a file, keywords follow the message language.
        assert!(d.message(Skin::En).contains("`else`"));
        assert!(d.help(Skin::Ar).unwrap().contains("`وإلا:`"));
    }

    #[test]
    fn json_render_has_all_three_languages() {
        let text = "x\n";
        let lines = LineIndex::new(text);
        let src = SourceView::new("a.kl", text, Skin::En, &lines);
        let d = Diagnostic::new("K0904", FileId(0), Span::new(0, 1));
        let v = render_json(&d, &src);
        assert_eq!(v["code"], "K0904");
        assert!(!v["message"]["ar"].as_str().unwrap().is_empty());
        assert!(v["help"]["ar"].as_str().unwrap().contains("؛"));
        assert!(v["help"]["en"].as_str().unwrap().contains("kalam fix"));
        assert_eq!(v["span"]["start"]["line"], 1);
        let unknown = Diagnostic::new("K9999", FileId(0), Span::new(0, 1));
        assert!(render_json(&unknown, &src)["help"].is_null());
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
