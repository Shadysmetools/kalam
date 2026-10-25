//! `sql"…"` (spec §17.3): quotes, comments and statements, and each hole's position.
//!
//! The scanner tracks SQL quote and comment state across holes, and a coarse clause state
//! from the last clause keyword, which decides identifier positions:
//!
//! - after `FROM`, `JOIN`, `INTO`, `UPDATE`, `TABLE` and `ORDER`/`GROUP`/`PARTITION BY`, a
//!   hole names a table or a column (or is `ASC`/`DESC`): K1603 unless it is an `Sql`
//!   fragment (`sql.ident`), until the next value clause (`SELECT`, `WHERE`, `SET`,
//!   `VALUES`, `LIMIT`, `OFFSET`, `HAVING`, `ON`, `RETURNING`);
//! - after a comparison operator (`=`, `<`, `>`) a hole is a value, even in those clauses;
//! - a hole right after `.` or `AS`, right before `.`, or before `=` in a `SET` clause is
//!   an identifier too;
//! - a hole right after `LIKE`/`ILIKE` is a pattern (W1606 for a raw `String`);
//! - every other hole is a bound parameter.
//!
//! A hole inside `'…'` is K1602; inside `"…"` or `` `…` `` (quoted identifiers) K1603;
//! inside `--` or `/* */` comments K1605. A `;` outside quotes and comments is K1604, even
//! a trailing one: each `Sql` value is exactly one statement.

use crate::{HoleRequirement, Site, TemplateCheck};

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Code,
    Single,
    Double,
    Backtick,
    LineComment,
    BlockComment,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Clause {
    Value,
    Ident,
    Set,
}

struct Scan {
    state: State,
    clause: Clause,
    word: String,
    last_word: String,
    /// The last significant character in code (not whitespace, quotes or comments).
    last_sig: Option<char>,
    /// The previous character, for `--`, `/*` and `*/`.
    prev: Option<char>,
    semicolon: bool,
}

impl Scan {
    fn end_word(&mut self) {
        if self.word.is_empty() {
            return;
        }
        let w = std::mem::take(&mut self.word).to_ascii_uppercase();
        match w.as_str() {
            "FROM" | "JOIN" | "INTO" | "UPDATE" | "TABLE" => self.clause = Clause::Ident,
            "BY" if matches!(self.last_word.as_str(), "ORDER" | "GROUP" | "PARTITION") => {
                self.clause = Clause::Ident
            }
            "SELECT" | "WHERE" | "VALUES" | "LIMIT" | "OFFSET" | "HAVING" | "ON" | "RETURNING" => {
                self.clause = Clause::Value
            }
            "SET" => self.clause = Clause::Set,
            _ => {}
        }
        self.last_word = w;
    }

    fn feed(&mut self, c: char) {
        let prev = self.prev.replace(c);
        match self.state {
            State::Single if c == '\'' => self.state = State::Code,
            State::Double if c == '"' => self.state = State::Code,
            State::Backtick if c == '`' => self.state = State::Code,
            State::LineComment if c == '\n' => self.state = State::Code,
            State::BlockComment if prev == Some('*') && c == '/' => {
                self.state = State::Code;
                // `*/` must not also start a comment with the next `*`.
                self.prev = None;
            }
            State::Code => self.code(c, prev),
            _ => {}
        }
    }

    fn code(&mut self, c: char, prev: Option<char>) {
        if c.is_alphanumeric() || c == '_' {
            self.word.push(c);
            self.last_sig = Some(c);
            return;
        }
        self.end_word();
        match c {
            '\'' => self.state = State::Single,
            '"' => self.state = State::Double,
            '`' => self.state = State::Backtick,
            '-' if prev == Some('-') => {
                self.state = State::LineComment;
                // The first `-` was taken as code; it was half of the comment opener.
                self.last_sig = None;
            }
            '*' if prev == Some('/') => {
                self.state = State::BlockComment;
                self.prev = None;
                self.last_sig = None;
            }
            ';' => self.semicolon = true,
            c if c.is_whitespace() => {}
            c => self.last_sig = Some(c),
        }
        if self.state != State::Code {
            self.last_word.clear();
        }
    }

    /// Classifies the hole between the text so far and `next` (the text after it).
    fn hole(&mut self, next: &str) -> Result<HoleRequirement, &'static str> {
        match self.state {
            State::Single => return Err("K1602"),
            State::Double | State::Backtick => return Err("K1603"),
            State::LineComment | State::BlockComment => return Err("K1605"),
            State::Code => {}
        }
        self.end_word();
        // A hole ends any `--`/`/*` opener in progress.
        self.prev = None;
        let after = next.trim_start();
        // After a comparison (`CASE WHEN s = {v}` in `ORDER BY`) a hole is a value.
        let compared = matches!(self.last_sig, Some('=' | '<' | '>'));
        let ident = (self.clause == Clause::Ident && !compared)
            || self.last_sig == Some('.')
            || self.last_word == "AS"
            || after.starts_with('.')
            || (self.clause == Clause::Set && after.starts_with('=') && !after.starts_with("=="));
        let req = if ident {
            HoleRequirement::SqlIdent
        } else if matches!(self.last_word.as_str(), "LIKE" | "ILIKE") {
            HoleRequirement::SqlLike
        } else {
            HoleRequirement::SqlParam
        };
        // The hole is a value: what follows is not glued to the keyword before it.
        self.last_word.clear();
        self.last_sig = Some('?');
        Ok(req)
    }
}

pub(crate) fn check(parts: &[String], site: &Site<'_>) -> TemplateCheck {
    let mut s = Scan {
        state: State::Code,
        clause: Clause::Value,
        word: String::new(),
        last_word: String::new(),
        last_sig: None,
        prev: None,
        semicolon: false,
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
        let next = parts.get(i + 1).map(String::as_str).unwrap_or("");
        match s.hole(next) {
            Ok(req) => holes.push(req),
            Err(code) => {
                diagnostics.push(site.hole(code, i));
                holes.push(HoleRequirement::Forbidden);
            }
        }
    }
    if s.semicolon {
        diagnostics.push(site.literal("K1604"));
    }
    TemplateCheck { holes, diagnostics }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{codes, holes};
    use crate::HoleRequirement as R;
    use crate::Tag;

    fn sql(t: &str) -> Vec<&'static str> {
        codes(Tag::Sql, t)
    }

    #[test]
    fn conformance_positive_twin() {
        // 17.3.1: values, `IN {ids}`, a trailing comment, and `;` inside quotes.
        let t = "SELECT id FROM users WHERE name = {name} AND id IN {ids} -- active users";
        assert!(sql(t).is_empty());
        assert_eq!(holes(Tag::Sql, t), vec![R::SqlParam, R::SqlParam]);
        assert!(sql("SELECT id FROM users WHERE note = 'a;b'").is_empty());
        assert!(sql("DELETE FROM notes").is_empty());
    }

    #[test]
    fn hole_inside_quotes_is_k1602() {
        assert_eq!(
            sql("SELECT id FROM users WHERE name = '{name}'"),
            vec!["K1602"]
        );
        // `''` is an escaped quote: still inside the string.
        assert_eq!(sql("SELECT 'it''s {x}'"), vec!["K1602"]);
        assert!(sql("SELECT 'it''s' WHERE a = {x}").is_empty());
    }

    #[test]
    fn identifier_positions() {
        let t = "SELECT id FROM users ORDER BY {name}";
        assert_eq!(holes(Tag::Sql, t), vec![R::SqlIdent]);
        assert_eq!(
            holes(Tag::Sql, "SELECT id FROM {t} WHERE a = {v}"),
            vec![R::SqlIdent, R::SqlParam]
        );
        assert_eq!(
            holes(Tag::Sql, "SELECT id FROM t ORDER BY name {dir} LIMIT {n}"),
            vec![R::SqlIdent, R::SqlParam]
        );
        assert_eq!(
            holes(Tag::Sql, "SELECT u.{col} FROM users u"),
            vec![R::SqlIdent]
        );
        assert_eq!(
            holes(Tag::Sql, "SELECT {t}.id AS {alias} FROM t"),
            vec![R::SqlIdent, R::SqlIdent]
        );
        assert_eq!(
            holes(Tag::Sql, "UPDATE t SET {col} = {v} WHERE id = {id}"),
            vec![R::SqlIdent, R::SqlParam, R::SqlParam]
        );
        assert_eq!(
            holes(Tag::Sql, "INSERT INTO t ({col}) VALUES ({v}, 0)"),
            vec![R::SqlIdent, R::SqlParam]
        );
        assert_eq!(
            holes(
                Tag::Sql,
                "SELECT a FROM t JOIN u ON u.id = {v} GROUP BY {g}"
            ),
            vec![R::SqlParam, R::SqlIdent]
        );
        // Quoted identifiers: a hole there names something too.
        assert_eq!(sql("SELECT \"{c}\" FROM t"), vec!["K1603"]);
        assert_eq!(sql("SELECT `{c}` FROM t"), vec!["K1603"]);
    }

    #[test]
    fn corpus_queries_are_values() {
        let t = "\n        UPDATE todos SET title = {a}, done = {b} WHERE id = {id}\n    ";
        assert!(sql(t).is_empty());
        assert_eq!(holes(Tag::Sql, t), vec![R::SqlParam; 3]);
        let t = "\n        INSERT INTO todos (title, done) VALUES ({t}, 0)\n        RETURNING id, title, done\n    ";
        assert_eq!(holes(Tag::Sql, t), vec![R::SqlParam]);
        let t = "\n        SELECT id, name, email, balance\n        FROM customers\n        WHERE city = {city} AND balance > 0\n        ORDER BY balance DESC\n        LIMIT {limit}\n    ";
        assert_eq!(holes(Tag::Sql, t), vec![R::SqlParam, R::SqlParam]);
        assert!(
            sql("CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT, city TEXT)").is_empty()
        );
    }

    #[test]
    fn multiple_statements_are_k1604() {
        assert_eq!(
            sql("DELETE FROM users WHERE name = {name}; DELETE FROM notes"),
            vec!["K1604"]
        );
        assert_eq!(sql("SELECT 1;"), vec!["K1604"]);
        assert!(sql("SELECT 1 -- a; b").is_empty());
        assert!(sql("SELECT 1 /* a; b */").is_empty());
        assert!(sql("SELECT \"a;b\" FROM t").is_empty());
    }

    #[test]
    fn hole_inside_comment_is_k1605() {
        assert_eq!(sql("SELECT id FROM users /* {name} */"), vec!["K1605"]);
        assert_eq!(sql("SELECT id FROM users -- {name}"), vec!["K1605"]);
        // The comment ends at the newline.
        assert!(sql("SELECT id -- note\nFROM t WHERE a = {x}").is_empty());
        // `*/` closes; `/*/` does not.
        assert_eq!(sql("SELECT /*/ {x} */ 1"), vec!["K1605"]);
        assert!(sql("SELECT /**/ {x}").is_empty());
        // A minus next to a hole is not a comment.
        assert!(sql("SELECT -{x} - {y}").is_empty());
    }

    #[test]
    fn comparisons_in_identifier_clauses_take_values() {
        assert_eq!(
            holes(
                Tag::Sql,
                "SELECT a FROM t ORDER BY CASE WHEN s = {v} THEN 0 ELSE 1 END, {col}"
            ),
            vec![R::SqlParam, R::SqlIdent]
        );
    }

    #[test]
    fn like_pattern_holes() {
        assert_eq!(
            holes(
                Tag::Sql,
                "SELECT id FROM t WHERE name LIKE {p} AND a ILIKE {q}"
            ),
            vec![R::SqlLike, R::SqlLike]
        );
    }

    #[test]
    fn arabic_text_and_odd_input_never_panic() {
        assert!(sql("SELECT 'مرحبا' WHERE الاسم = {x}").is_empty());
        for t in [
            "", "'", "\"", "--", "/*", "{x}", "{a}{b}", "*/;", "-{x}-", "/{x}*",
        ] {
            let _ = sql(t);
        }
    }
}
