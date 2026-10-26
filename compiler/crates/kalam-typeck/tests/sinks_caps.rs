//! Capabilities (spec §16), sink holes of tagged literals (§17) and `Secret` (§18), from
//! source text: parse → resolve (with the stdlib stubs) → check. One test per code, with
//! its positive twin. The rules live in `kalam-sinks`; these tests check the wiring.

use kalam_base::{FileId, Skin};
use kalam_resolve::{resolve_program, ModulePath, Options, SourceModule};

fn stdlib() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../stdlib/src");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("stdlib/src") {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "kl") {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            out.push((format!("src/{name}"), std::fs::read_to_string(&p).unwrap()));
        }
    }
    out.sort();
    out
}

/// `(code, line)` of every finding (warnings included) in the one-file program `path`.
fn codes_in(path: &str, src: &str) -> Vec<(&'static str, usize)> {
    let mut texts = vec![(format!("src/{path}"), false, src.to_string())];
    texts.extend(stdlib().into_iter().map(|(p, t)| (p, true, t)));
    let mut parsed = Vec::new();
    for (i, (path, _, text)) in texts.iter().enumerate() {
        let skin = Skin::from_path(path).expect("a Kalam path");
        let out = kalam_parser::parse_source(text, skin, FileId(i as u32));
        let errors: Vec<_> = out.diagnostics.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "{path} must parse: {errors:?}");
        parsed.push((skin, out));
    }
    let sources: Vec<SourceModule<'_>> = texts
        .iter()
        .zip(&parsed)
        .enumerate()
        .map(|(i, ((path, std, _), (skin, out)))| SourceModule {
            path: ModulePath::from_file(if *std { "std" } else { "app" }, path),
            file: FileId(i as u32),
            skin: *skin,
            ast: &out.ast,
            std: *std,
        })
        .collect();
    let options = Options {
        package: "app".into(),
        whole_package: true,
        std_loaded: true,
    };
    let (program, diags) = resolve_program(&sources, &options);
    let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "resolve must be clean: {errors:#?}");
    kalam_typeck::check_resolved(&program, &sources)
        .diagnostics()
        .iter()
        .map(|d| {
            assert_eq!(d.file, FileId(0), "a finding in the stdlib: {d:?}");
            let line = src[..(d.span.start as usize).min(src.len())]
                .matches('\n')
                .count()
                + 1;
            (d.code, line)
        })
        .collect()
}

fn codes(src: &str) -> Vec<(&'static str, usize)> {
    codes_in("main.kl", src)
}

fn clean(src: &str) {
    assert_eq!(codes(src), vec![], "{src}");
}

/// The program `fn main():` with `body` (one statement per line, indented by the caller's
/// four spaces) after `lets`.
fn in_main(body: &str) -> String {
    format!("fn main():\n    let name = \"Sara\"\n{body}")
}

// ---- §16 capabilities -------------------------------------------------------------------

#[test]
fn capabilities_come_from_main_and_print_is_ambient() {
    // §16.1 rule 1, rule 4 (`<Net>` is `Show`), D20: `print` needs no capability, even in
    // a function that has none.
    clean("fn has_config(fs: Fs) -> Bool => fs.exists(path\"config.toml\")\n\nfn note(n: Int):\n    print(\"{n}\")\n\nfn main(world: World):\n    let found = has_config(world.fs)\n    note(1)\n    print(\"{found} {world.net}\")\n");
}

#[test]
fn constructing_world_is_k1501_in_every_skin() {
    assert_eq!(
        codes("fn main():\n    let root = World()\n    print(\"x\")\n"),
        vec![("K1501", 2)]
    );
    assert_eq!(
        codes_in(
            "main.ar.kl",
            "دالة رئيسي():\n    ليكن root = عالم()\n    اطبع(\"x\")\n"
        ),
        vec![("K1501", 2)]
    );
    assert_eq!(
        codes_in(
            "main.franco.kl",
            "dala ra2isi():\n    khali root = donya()\n    ektb(\"x\")\n"
        ),
        vec![("K1501", 2)]
    );
}

#[test]
fn decoding_world_is_k1501() {
    assert_eq!(
        codes("use std/json.{DecodeError}\n\nfn load(s: String) -> Result[World, DecodeError] => json.decode(s)\n\nfn main():\n    pass\n"),
        vec![("K1501", 3)]
    );
}

#[test]
fn other_capabilities_have_no_constructor_either() {
    assert_eq!(
        codes("fn main():\n    let n = Net()\n    print(\"{n}\")\n"),
        vec![("K0600", 2)]
    );
}

#[test]
fn capabilities_are_not_eq() {
    assert_eq!(
        codes("fn main(world: World):\n    let same = world.net == world.net\n    print(\"{same}\")\n"),
        vec![("K0602", 2)]
    );
}

#[test]
fn secret_looking_env_key_is_k1503() {
    assert_eq!(
        codes("fn key(env: Env) -> Option[String] => env.get(\"STRIPE_API_KEY\")\n\nfn main():\n    pass\n"),
        vec![("K1503", 1)]
    );
    assert_eq!(
        codes("fn main(world: World):\n    let p = world.env.get(\"DB_PASSWORD\")\n    print(\"{p}\")\n"),
        vec![("K1503", 2)]
    );
    clean("fn port(env: Env) -> Option[String] => env.get(\"PORT\")\n\nfn key(env: Env) -> Option[Secret[String]] => env.secret(\"STRIPE_API_KEY\")\n\nfn main():\n    pass\n");
}

// ---- §17.3 sql --------------------------------------------------------------------------

#[test]
fn sql_holes_are_parameters() {
    clean(&in_main("    let ids = [1, 2]\n    let _ = sql\"SELECT id FROM users WHERE name = {name} AND id IN {ids} -- active users\"\n    let _ = sql\"SELECT id FROM users WHERE note = 'a;b'\"\n"));
}

#[test]
fn sql_hole_in_quotes_is_k1602() {
    assert_eq!(
        codes(&in_main(
            "    let _ = sql\"SELECT id FROM users WHERE name = '{name}'\"\n"
        )),
        vec![("K1602", 3)]
    );
}

#[test]
fn sql_identifier_hole_is_k1603_unless_it_is_sql() {
    assert_eq!(
        codes(&in_main(
            "    let _ = sql\"SELECT id FROM users ORDER BY {name}\"\n"
        )),
        vec![("K1603", 3)]
    );
    // `sql.ident` returns an `Sql` fragment: identifiers come only from it.
    clean("use std/sql.{self, DbError}\n\nfn q(col: String) -> Result[Sql, DbError]:\n    let c = sql.ident(col, allowed: [\"name\", \"id\"])?\n    return Ok(sql\"SELECT id FROM users ORDER BY {c}\")\n\nfn main():\n    pass\n");
}

#[test]
fn sql_two_statements_is_k1604() {
    assert_eq!(
        codes(&in_main(
            "    let _ = sql\"DELETE FROM users WHERE name = {name}; DELETE FROM notes\"\n"
        )),
        vec![("K1604", 3)]
    );
}

#[test]
fn sql_hole_in_comment_is_k1605() {
    assert_eq!(
        codes(&in_main(
            "    let _ = sql\"SELECT id FROM users /* {name} */\"\n"
        )),
        vec![("K1605", 3)]
    );
}

#[test]
fn raw_string_like_pattern_is_w1606() {
    assert_eq!(
        codes(&in_main(
            "    let _ = sql\"SELECT id FROM users WHERE name LIKE {name}\"\n"
        )),
        vec![("W1606", 3)]
    );
    clean("use std/sql\n\nfn main():\n    let p = sql.like_contains(\"Sa\")\n    let _ = sql\"SELECT id FROM users WHERE name LIKE {p}\"\n");
}

#[test]
fn sql_hole_must_be_a_sql_param() {
    // A record is not an `SqlParam` (§17.3), and neither is a `Secret` (§18.1: no
    // implicit reveal).
    assert_eq!(
        codes("type User:\n    id: Int\n\nfn main():\n    let u = User(id: 1)\n    let _ = sql\"SELECT id FROM users WHERE id = {u}\"\n"),
        vec![("K0603", 6)]
    );
    assert_eq!(
        codes("fn main():\n    let pw = Secret.new(\"x\")\n    let _ = sql\"SELECT id FROM users WHERE hash = {pw}\"\n"),
        vec![("K0603", 3)]
    );
}

// ---- §17.4 html -------------------------------------------------------------------------

#[test]
fn html_holes_in_allowed_contexts() {
    clean(&in_main("    let count = 3\n    let home = url\"https://example.com/home\"\n    let _ = html\"\"\"<p class=\"{name}\" title=\"{count}\">{name}</p><a href=\"{home}\">home</a>\"\"\"\n"));
    // `Html`, `List[Html]` and a generic `Show` value in text.
    clean("fn item[T: Show](x: T) -> Html => html\"<li>{x}</li>\"\n\nfn list(items: List[Html]) -> Html => html\"<ul>{items}</ul>\"\n\nfn main():\n    let _ = list([item(1), item(\"a\")])\n");
}

#[test]
fn html_malformed_is_k1610() {
    assert_eq!(
        codes("fn main():\n    let _ = html\"<p><b>hi</p>\"\n"),
        vec![("K1610", 2)]
    );
}

#[test]
fn html_forbidden_contexts() {
    let one = |lit: &str| codes(&in_main(&format!("    let _ = html\"\"\"{lit}\"\"\"\n")));
    assert_eq!(one("<p class={name}>hi</p>"), vec![("K1611", 3)]);
    assert_eq!(one("<script>{name}</script>"), vec![("K1612", 3)]);
    assert_eq!(one("<p onclick=\"go({name})\">hi</p>"), vec![("K1612", 3)]);
    assert_eq!(one("<{name}>hi</{name}>"), vec![("K1614", 3), ("K1614", 3)]);
    assert_eq!(one("<p>hi<!-- {name} --></p>"), vec![("K1615", 3)]);
    assert_eq!(
        one("<button k-frobnicate=\"go\">{name}</button>"),
        vec![("K1617", 3)]
    );
}

#[test]
fn html_url_attribute_needs_url_k1613() {
    assert_eq!(
        codes(&in_main(
            "    let _ = html\"\"\"<a href=\"{name}\">home</a>\"\"\"\n"
        )),
        vec![("K1613", 3)]
    );
}

#[test]
fn html_event_hole_needs_message_k1616() {
    let src = "@derive(Message)\nenum Msg:\n    Save\n\nfn main():\n    let n = 1\n    let _ = html\"\"\"<button k-click=\"{Msg.Save}\">ok</button>\"\"\"\n    let _ = html\"\"\"<button k-click=\"{n}\">ok</button>\"\"\"\n";
    assert_eq!(codes(src), vec![("K1616", 8)]);
}

#[test]
fn html_hole_types_follow_the_context() {
    // Text needs `Show` (a function has none); an attribute takes text-like values.
    assert_eq!(
        codes("fn f() -> Int => 1\n\nfn main():\n    let _ = html\"<p>{f}</p>\"\n"),
        vec![("K0603", 4)]
    );
    assert_eq!(
        codes("fn main():\n    let xs = [\"a\"]\n    let _ = html\"\"\"<p class=\"{xs}\">x</p>\"\"\"\n"),
        vec![("K0600", 3)]
    );
}

// ---- §17.5 cmd --------------------------------------------------------------------------

#[test]
fn cmd_literal_program_with_holes() {
    clean("fn main():\n    let count = 5\n    let file = \"notes.txt\"\n    let _ = cmd\"git log --oneline -n {count} -- {file}\"\n");
}

#[test]
fn cmd_program_from_hole_is_k1621() {
    assert_eq!(
        codes("fn main():\n    let program = \"git\"\n    let _ = cmd\"{program} --version\"\n"),
        vec![("K1621", 3)]
    );
}

#[test]
fn cmd_shell_metacharacter_is_k1620() {
    assert_eq!(
        codes("fn main():\n    let _ = cmd\"ls -la | wc -l\"\n"),
        vec![("K1620", 2)]
    );
}

#[test]
fn cmd_option_like_hole_is_w1622() {
    assert_eq!(
        codes("fn main():\n    let file = \"notes.txt\"\n    let _ = cmd\"rm {file}\"\n"),
        vec![("W1622", 3)]
    );
    clean("fn main():\n    let file = \"notes.txt\"\n    let _ = cmd\"rm -- {file}\"\n");
}

// ---- §17.6 url, path, regex -------------------------------------------------------------

#[test]
fn url_host_from_hole_is_k1631() {
    assert_eq!(
        codes(
            "fn main():\n    let host = \"example.com\"\n    let _ = url\"https://{host}/users\"\n"
        ),
        vec![("K1631", 3)]
    );
}

#[test]
fn url_path_and_regex_literals() {
    clean("fn upload(name: PathSegment) -> Path => path\"uploads/{name}\"\n\nfn main():\n    let id = 7\n    let term = \"kalam\"\n    let _ = url\"https://api.example.com/users/{id}?q={term}\"\n    let _ = url\"/{term}\"\n    let _ = regex\"^[a-z0-9_]\\{3,20\\}$\"\n");
}

#[test]
fn path_hole_is_still_k1641_once() {
    assert_eq!(
        codes("fn main():\n    let name = \"a.txt\"\n    let _ = path\"uploads/{name}\"\n"),
        vec![("K1641", 3)]
    );
}

// ---- §18.1 Secret -----------------------------------------------------------------------

#[test]
fn secret_has_no_encode_eq_or_ord() {
    assert_eq!(
        codes("@derive(Encode)\ntype Login:\n    user: String\n    password: Secret[String]\n\nfn main():\n    pass\n")
            .into_iter()
            .map(|(c, _)| c)
            .collect::<Vec<_>>(),
        vec!["K0612"]
    );
    assert_eq!(
        codes("fn main():\n    let a = Secret.new(\"x\")\n    let b = Secret.new(\"x\")\n    let same = a == b\n    print(\"{same}\")\n"),
        vec![("K0602", 4)]
    );
    assert_eq!(
        codes("fn main():\n    let a = Secret.new(1)\n    let b = Secret.new(2)\n    let less = a < b\n    print(\"{less}\")\n"),
        vec![("K0603", 4)]
    );
}

#[test]
fn secret_is_shown_redacted_and_revealed_explicitly() {
    // §18.1: `Show` prints `[REDACTED]`; `.expose()` is the explicit reveal.
    clean("fn main():\n    let a = Secret.new(\"x\")\n    let b = Secret.new(\"x\")\n    let same = a.expose() == b.expose()\n    print(\"{a} {same}\")\n    let _ = html\"<p>{a}</p>\"\n");
}

/// FOUNDER-QUEUE Q53: the parser accepts exactly the tags kalam-sinks types (K0260 for any
/// other), and the K0260 message lists them all.
#[test]
fn the_parser_and_the_sinks_agree_on_the_built_in_tags() {
    for tag in kalam_parser::BUILTIN_TAGS {
        assert!(kalam_sinks::Tag::from_name(tag).is_some(), "{tag}");
    }
    for unknown in ["f", "sqll", "shell", "raw", "return"] {
        assert!(kalam_sinks::Tag::from_name(unknown).is_none(), "{unknown}");
        assert!(!kalam_parser::BUILTIN_TAGS.contains(&unknown), "{unknown}");
    }
    let entry = kalam_diagnostics::catalog::lookup("K0260").expect("K0260 is in the catalog");
    for msg in entry.message {
        for tag in kalam_parser::BUILTIN_TAGS {
            assert!(msg.contains(&format!("`{tag}`")), "{tag}: {msg}");
        }
    }
}
