//! `cmd"…"` (spec §17.5): the literal is split into argv words at whitespace outside
//! double quotes, at compile time. Kalam never runs a shell.
//!
//! - The program (the first word) must be literal text: K1621 when a hole is in it, or when
//!   the literal has no word at all.
//! - A shell metacharacter outside double quotes is K1620 (the list of §17.5): `|`, `;`,
//!   `&`, `>`, `<`, `)`, `` ` ``, `*`, `?`, `~`, `!`, and `$(`.
//! - Every hole is one argv element, or part of one when glued to literal text
//!   (`--author={name}`). A whole-word hole with no literal `--` word before it gets the
//!   option-like guard ([`HoleRequirement::Argv`], W1622 for text values).

use crate::{HoleRequirement, Site, TemplateCheck};

const META: &[char] = &['|', ';', '&', '>', '<', ')', '`', '*', '?', '~', '!'];

struct Scan {
    quoted: bool,
    /// Literal text of the current word (quotes removed), and whether it has a hole.
    word: String,
    word_hole: bool,
    /// The current word was started (by text, a quote or a hole).
    in_word: bool,
    words: usize,
    double_dash: bool,
    meta: bool,
    prev: Option<char>,
}

impl Scan {
    fn end_word(&mut self) {
        if self.in_word {
            if !self.word_hole && self.word == "--" {
                self.double_dash = true;
            }
            self.words += 1;
        }
        self.word.clear();
        self.word_hole = false;
        self.in_word = false;
    }

    fn feed(&mut self, c: char) {
        let prev = self.prev.replace(c);
        if c == '"' {
            self.quoted = !self.quoted;
            self.in_word = true;
            return;
        }
        if !self.quoted {
            if c.is_whitespace() {
                self.end_word();
                return;
            }
            if META.contains(&c) || (c == '(' && prev == Some('$')) {
                self.meta = true;
            }
        }
        self.in_word = true;
        self.word.push(c);
    }
}

pub(crate) fn check(parts: &[String], site: &Site<'_>) -> TemplateCheck {
    let mut s = Scan {
        quoted: false,
        word: String::new(),
        word_hole: false,
        in_word: false,
        words: 0,
        double_dash: false,
        meta: false,
        prev: None,
    };
    let mut holes = Vec::new();
    let mut diagnostics = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        for c in part.chars() {
            s.feed(c);
        }
        if i + 1 == parts.len() {
            break;
        }
        s.prev = None;
        if s.words == 0 {
            diagnostics.push(site.hole("K1621", i));
            holes.push(HoleRequirement::Forbidden);
        } else {
            let next = parts.get(i + 1).map(String::as_str).unwrap_or("");
            let starts_word = s.word.is_empty() && !s.word_hole;
            let ends_word = match next.chars().next() {
                None => i + 2 == parts.len(),
                Some(c) => c.is_whitespace() || (c == '"' && s.quoted),
            };
            holes.push(HoleRequirement::Argv {
                whole_word: starts_word && ends_word,
                guarded: s.double_dash,
            });
        }
        s.in_word = true;
        s.word_hole = true;
    }
    s.end_word();
    if s.words == 0 && parts.len() == 1 {
        diagnostics.push(site.literal("K1621"));
    }
    if s.meta {
        diagnostics.push(site.literal("K1620"));
    }
    TemplateCheck { holes, diagnostics }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{codes, holes};
    use crate::HoleRequirement as R;
    use crate::Tag;

    fn cmd(t: &str) -> Vec<&'static str> {
        codes(Tag::Cmd, t)
    }

    fn argv(whole_word: bool, guarded: bool) -> R {
        R::Argv {
            whole_word,
            guarded,
        }
    }

    #[test]
    fn conformance_positive_twin() {
        // 17.5.1, the spec's own example.
        let t = "git log --oneline -n {count} -- {file}";
        assert!(cmd(t).is_empty());
        assert_eq!(
            holes(Tag::Cmd, t),
            vec![argv(true, false), argv(true, true)]
        );
    }

    #[test]
    fn glued_and_quoted_holes() {
        assert_eq!(
            holes(Tag::Cmd, "git log --author={name} {rev}x"),
            vec![argv(false, false), argv(false, false)]
        );
        assert_eq!(
            holes(Tag::Cmd, "git commit -m \"fix: {msg}\""),
            vec![argv(false, false)]
        );
        // A quoted hole alone is still a whole argv element.
        assert_eq!(
            holes(Tag::Cmd, "git commit -m \"{msg}\""),
            vec![argv(true, false)]
        );
        assert_eq!(holes(Tag::Cmd, "rm -- {a}"), vec![argv(true, true)]);
        assert_eq!(
            holes(Tag::Cmd, "rm --{a} {b}"),
            vec![argv(false, false), argv(true, false)]
        );
    }

    #[test]
    fn program_must_be_literal() {
        assert_eq!(cmd("{program} --version"), vec!["K1621"]);
        assert_eq!(cmd("  {program}"), vec!["K1621"]);
        assert_eq!(cmd("{dir}/bin/git status"), vec!["K1621"]);
        assert_eq!(cmd(""), vec!["K1621"]);
        assert_eq!(cmd("   "), vec!["K1621"]);
        assert!(cmd("/usr/bin/git status").is_empty());
    }

    #[test]
    fn shell_metacharacters_are_k1620() {
        for t in [
            "ls -la | wc -l",
            "ls; rm x",
            "a && b",
            "a > out.txt",
            "cat < in",
            "echo $(id)",
            "echo `id`",
            "ls *.kl",
            "ls ?",
            "ls ~",
            "echo hi!",
            "run (x)",
        ] {
            assert_eq!(cmd(t), vec!["K1620"], "{t}");
        }
        // Inside double quotes they are plain text.
        assert!(cmd("git log \"--format=%h | %s\" -- {f}").is_empty());
        assert!(cmd("echo $HOME").is_empty());
    }

    #[test]
    fn arabic_text_and_odd_input_never_panic() {
        assert!(cmd("echo \"مرحبا\" {x}").is_empty());
        for t in ["\"", "{a}{b}", "a \"{b}", "a {b}\"", "$", "$({x})"] {
            let _ = cmd(t);
        }
    }
}
