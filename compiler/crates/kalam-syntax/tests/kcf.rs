//! KCF (spec §4.4): the skin-free canonical serialization of the CST and its SHA-256.
//!
//! - Every eval corpus case gives byte-identical KCF in `en`, `ar` and `franco` (parity
//!   gate P1).
//! - Hand-written triplets cover what the corpus does not: `،`/`؟`, Arabic-Indic digits,
//!   keyword spelling variants, raw identifiers, §5.3 rule 8 positions and keywords inside
//!   interpolation holes.
//! - Layout that `fmt` may change (spaces, CRLF, a final newline, line breaks inside
//!   brackets) never changes the KCF; everything that changes meaning or that §4.4 keeps
//!   byte for byte (identifiers, numbers, strings, comments, blank lines) does.

use std::path::{Path, PathBuf};

use kalam_base::{FileId, Skin};
use kalam_syntax::kcf::{self, KcfHash};
use kalam_syntax::Cst;

fn parse(text: &str, skin: Skin) -> kalam_parser::ParseOutput {
    kalam_parser::parse_source(text, skin, FileId(0))
}

fn cst(text: &str, skin: Skin) -> Cst {
    parse(text, skin).cst
}

fn kcf_of(text: &str, skin: Skin) -> Vec<u8> {
    kcf::serialize(&cst(text, skin))
}

fn show(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Asserts that the three skins of one program parse cleanly and give one KCF.
#[track_caller]
fn same_kcf(en: &str, ar: &str, franco: &str) -> Vec<u8> {
    let mut first: Option<Vec<u8>> = None;
    for (skin, text) in [(Skin::En, en), (Skin::Ar, ar), (Skin::Franco, franco)] {
        let out = parse(text, skin);
        let errors: Vec<_> = out
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| d.code)
            .collect();
        assert!(
            errors.is_empty(),
            "{skin:?} does not parse: {errors:?}\n{text}"
        );
        let k = kcf::serialize(&out.cst);
        assert_eq!(
            kcf::serialize_in(&out.cst, skin),
            k,
            "{skin:?}: the inferred skin gives the explicit-skin KCF"
        );
        match &first {
            None => first = Some(k),
            Some(f) => assert_eq!(show(&k), show(f), "{skin:?} KCF differs from en"),
        }
    }
    first.unwrap()
}

#[track_caller]
fn differ(a: &str, b: &str, skin: Skin) {
    assert_ne!(
        show(&kcf_of(a, skin)),
        show(&kcf_of(b, skin)),
        "{a:?} and {b:?} must not share a KCF"
    );
}

#[track_caller]
fn equal(a: &str, b: &str, skin: Skin) {
    assert_eq!(
        show(&kcf_of(a, skin)),
        show(&kcf_of(b, skin)),
        "{a:?} and {b:?} must share a KCF"
    );
}

// ----- corpus (P1) ------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root")
}

/// Every `eval/corpus/<task>[/<case>]/` directory holding `{en,ar,franco}.kalam`.
fn corpus_cases() -> Vec<(String, Vec<(Skin, String)>)> {
    let root = repo_root().join("eval").join("corpus");
    let mut dirs = Vec::new();
    for task in std::fs::read_dir(&root).expect("eval/corpus") {
        let task = task.unwrap().path();
        if task.is_dir() {
            dirs.push(task.clone());
            for sub in std::fs::read_dir(&task).unwrap() {
                let sub = sub.unwrap().path();
                if sub.is_dir() {
                    dirs.push(sub);
                }
            }
        }
    }
    dirs.sort();
    let mut cases = Vec::new();
    for d in dirs {
        let files: Vec<(Skin, String)> = Skin::ALL
            .iter()
            .filter_map(|&skin| {
                let p = d.join(format!("{}.kalam", skin.id()));
                std::fs::read_to_string(p).ok().map(|t| (skin, t))
            })
            .collect();
        if !files.is_empty() {
            let name = d
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            cases.push((name, files));
        }
    }
    cases
}

#[test]
fn every_corpus_case_has_one_kcf_in_all_three_skins() {
    let cases = corpus_cases();
    assert!(cases.len() >= 14, "found {} corpus cases", cases.len());
    let mut checked = 0;
    let mut failures = Vec::new();
    for (name, files) in &cases {
        assert_eq!(files.len(), 3, "{name}: all three skins exist");
        let mut kcfs = Vec::new();
        let mut parses = true;
        for (skin, text) in files {
            let out = parse(text, *skin);
            parses &= !out.diagnostics.iter().any(|d| d.is_error());
            // Must not panic, even on a file the lexer rejects (T50 main).
            let k = kcf::serialize(&out.cst);
            assert_eq!(
                k,
                kcf::serialize_in(&out.cst, *skin),
                "{name}/{skin:?}: skin inference"
            );
            assert!(k.starts_with(kcf::MAGIC));
            kcfs.push((*skin, k));
        }
        if !parses {
            continue; // P1 scope: only cases that parse (G-PAR scope rule).
        }
        checked += 1;
        let (_, en) = &kcfs[0];
        for (skin, k) in &kcfs[1..] {
            if k != en {
                failures.push(format!(
                    "{name}: {skin:?} KCF differs from en\n--- en\n{}\n--- {skin:?}\n{}",
                    show(en),
                    show(k)
                ));
            } else {
                assert_eq!(KcfHash::of(k), KcfHash::of(en));
            }
        }
    }
    eprintln!("KCF P1: {checked} parsing corpus cases checked");
    assert!(checked >= 14, "only {checked} corpus cases parse");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn distinct_corpus_programs_have_distinct_hashes() {
    let mut seen = std::collections::HashMap::new();
    for (name, files) in corpus_cases() {
        let h = kcf::hash(&cst(&files[0].1, files[0].0));
        if let Some(other) = seen.insert(h, name.clone()) {
            panic!("{name} and {other} share a KCF hash");
        }
    }
}

// ----- cross-skin triplets ------------------------------------------------------------------

#[test]
fn arabic_comma_and_try_mark_are_the_ascii_concepts() {
    same_kcf(
        "fn f(a: Int, b: Int) -> Result[Int, String]:\n    let x = g(a, b)?\n    return Ok(x)\n",
        "دالة f(a: عدد، b: عدد) -> نتيجة[عدد، نص]:\n    ليكن x = g(a، b)؟\n    أرجع نجاح(x)\n",
        "dala f(a: adad, b: adad) -> natiga[adad, nass]:\n    khali x = g(a, b)?\n    rag3 naga7(x)\n",
    );
}

#[test]
fn numbers_are_values_not_glyphs() {
    let k = same_kcf(
        "fn main():\n    let x = 1_000 + 25 * 0x1F\n    let y = 3.5\n",
        "دالة رئيسي():\n    ليكن x = ١_٠٠٠ + ٢٥ * 0x1F\n    ليكن y = ٣٫٥\n",
        "dala ra2isi():\n    khali x = 1_000 + 25 * 0x1F\n    khali y = 3.5\n",
    );
    let text = show(&k);
    assert!(text.contains("N i 10 1000\n"), "{text}");
    assert!(text.contains("N i 16 31\n"), "{text}");
    assert!(text.contains("N f 3.5\n"), "{text}");
}

#[test]
fn keyword_spelling_variants_are_one_concept() {
    // `اذا` (no hamza) is an accepted variant of `إذا`; `fmt` rewrites it.
    same_kcf(
        "fn main():\n    if true:\n        pass\n",
        "دالة رئيسي():\n    اذا صح:\n        تخطى\n",
        "dala ra2isi():\n    law sa7:\n        fot\n",
    );
}

#[test]
fn keywords_inside_interpolation_holes_are_concepts() {
    // §4.4: hole code is serialized like any other code.
    let k = same_kcf(
        "fn main():\n    let a = true\n    print(\"{a and not a}, {f(1, 2)}\")\n",
        "دالة رئيسي():\n    ليكن a = صح\n    اطبع(\"{a و ليس a}, {f(١، ٢)}\")\n",
        "dala ra2isi():\n    khali a = sa7\n    ektb(\"{a we mesh a}, {f(1, 2)}\")\n",
    );
    let text = show(&k);
    assert!(text.contains("K KW_AND\n"), "{text}");
    // The literal text between holes is kept byte for byte (the ASCII `, ` here).
    assert!(text.contains("T 2:, \n"), "{text}");
}

#[test]
fn raw_identifiers_lose_their_backticks() {
    // `law` is an English identifier; in Franco it is the keyword `if`, so the converter
    // writes it in backticks (§4.3).
    same_kcf(
        "fn main():\n    let law = 1\n    print(law)\n",
        "دالة رئيسي():\n    ليكن law = 1\n    اطبع(law)\n",
        "dala ra2isi():\n    khali `law` = 1\n    ektb(`law`)\n",
    );
}

#[test]
fn reserved_words_in_rule_8_positions_are_identifiers() {
    // §5.3 rule 8: after `.`, as a named-argument label and as a record field name, a
    // reserved word is an ordinary identifier. Conversion copies it unchanged, so the
    // English `match` stays `match` in the Arabic and Franco files.
    let k = same_kcf(
        "type Rule:\n    match: Int\n\nfn main():\n    let r = Rule(match: 1)\n    print(r.match)\n",
        "هيكل Rule:\n    match: عدد\n\nدالة رئيسي():\n    ليكن r = Rule(match: 1)\n    اطبع(r.match)\n",
        "haykal Rule:\n    match: adad\n\ndala ra2isi():\n    khali r = Rule(match: 1)\n    ektb(r.match)\n",
    );
    let text = show(&k);
    assert!(!text.contains("KW_MATCH"), "{text}");
    assert_eq!(text.matches("I 5:match\n").count(), 3, "{text}");
}

#[test]
fn rule_8_keywords_in_use_paths_and_attributes_are_identifiers() {
    // A use path is never translated (§4.3), so `test` stays `test` in every skin.
    let en = "use std/test\n\n@test\nfn f():\n    pass\n";
    let ar = "استخدم std/test\n\n@test\nدالة f():\n    تخطى\n";
    let franco = "hat std/test\n\n@test\ndala f():\n    fot\n";
    let k = same_kcf(en, ar, franco);
    assert!(!show(&k).contains("KW_TEST"), "{}", show(&k));
    // A real keyword in the same skin is still a concept.
    assert!(show(&k).contains("K KW_USE\n"));
}

#[test]
fn rule_8_does_not_swallow_block_keywords() {
    // `else:` is a keyword followed by `:`, not a field name or label.
    let k = kcf_of(
        "fn main():\n    if true:\n        pass\n    else:\n        pass\n",
        Skin::En,
    );
    assert!(show(&k).contains("K KW_ELSE\n"), "{}", show(&k));
    let k = kcf_of("fn main():\n    let m = {true: 1, false: 2}\n", Skin::En);
    assert!(show(&k).contains("K KW_FALSE\n"), "{}", show(&k));
}

// ----- what never changes the KCF ---------------------------------------------------------

#[test]
fn spacing_line_endings_and_final_newline_do_not_change_kcf() {
    let base = "fn main():\n    let x = f(1, 2) + 3\n    print(x)\n";
    equal(
        base,
        "fn main():\n    let x = f( 1,2 )+3   \n    print(x)\n",
        Skin::En,
    );
    equal(base, &base.replace('\n', "\r\n"), Skin::En);
    equal(base, base.trim_end_matches('\n'), Skin::En);
    // A line break inside brackets is layout too (bracket suppression, §5.8).
    equal(
        base,
        "fn main():\n    let x = f(\n        1,\n        2\n    ) + 3\n    print(x)\n",
        Skin::En,
    );
    // Trailing blank lines at the end of the file.
    equal(base, &format!("{base}\n\n"), Skin::En);
    // Trailing spaces after a comment.
    equal(
        "# note\nfn main():\n    pass\n",
        "# note   \nfn main():\n    pass\n",
        Skin::En,
    );
}

// ----- what always changes the KCF --------------------------------------------------------

#[test]
fn meaning_comments_and_blank_lines_change_kcf() {
    let base = "fn main():\n    let x = 1\n    print(\"hi {x}\")\n";
    differ(base, &base.replace("x", "y"), Skin::En);
    differ(base, &base.replace("1", "2"), Skin::En);
    differ(base, &base.replace("hi", "Hi"), Skin::En);
    differ(base, &base.replace("{x}", "{x + 1}"), Skin::En);
    differ(base, &base.replace("let", "var"), Skin::En);
    differ(base, &format!("# a comment\n{base}"), Skin::En);
    differ(
        &format!("# one\n{base}"),
        &format!("# two\n{base}"),
        Skin::En,
    );
    differ(
        &format!("## doc\n{base}"),
        &format!("# doc\n{base}"),
        Skin::En,
    );
    // A blank line between two statements of a block (KCF2 records "at least one").
    differ(base, &base.replacen("= 1\n", "= 1\n\n", 1), Skin::En);
    // A trailing comment and a comment on its own line are different layouts.
    differ(
        "fn main():\n    let x = 1 # c\n    pass\n",
        "fn main():\n    let x = 1\n    # c\n    pass\n",
        Skin::En,
    );
    // Indentation is structure.
    differ(
        "fn main():\n    if true:\n        f()\n    g()\n",
        "fn main():\n    if true:\n        f()\n        g()\n",
        Skin::En,
    );
    // Radix is part of a number's kind.
    differ("let x = 16\n", "let x = 0x10\n", Skin::En);
    // A string's text, quotes and escapes are kept byte for byte.
    differ("let x = \"a\\{b\"\n", "let x = \"a{b}\"\n", Skin::En);
}

// ----- KCF2: layout `fmt` may change is not recorded (spec §4.4, FOUNDER-QUEUE Q8) -------

#[test]
fn header_is_kcf2() {
    assert_eq!(kcf::MAGIC, b"KCF2\n");
    assert!(kcf_of("fn main():\n    pass\n", Skin::En).starts_with(b"KCF2\n"));
}

#[test]
fn blank_line_runs_are_at_least_one_blank_line() {
    let one = "fn main():\n    let x = 1\n\n    print(x)\n";
    let three = "fn main():\n    let x = 1\n\n\n\n    print(x)\n";
    equal(one, three, Skin::En);
    assert_eq!(show(&kcf_of(one, Skin::En)).matches("\nB\n").count(), 1);
    // Between comments inside a block, too.
    equal(
        "fn main():\n    f()\n\n    # a\n\n    # b\n    g()\n",
        "fn main():\n    f()\n\n\n    # a\n\n\n\n    # b\n    g()\n",
        Skin::En,
    );
    differ(
        "fn main():\n    f()\n    # a\n\n    g()\n",
        "fn main():\n    f()\n\n    # a\n    g()\n",
        Skin::En,
    );
}

#[test]
fn blank_lines_fmt_decides_are_not_recorded() {
    let base = "fn main():\n    f(a, b)\n    # c\n    g()\n";
    // At the start of a block, before its first comment or statement.
    equal(base, &base.replacen(":\n", ":\n\n\n", 1), Skin::En);
    // Inside brackets.
    equal(
        "fn main():\n    f(\n        a,\n        b,\n    )\n",
        "fn main():\n    f(\n\n        a,\n\n\n        b,\n\n    )\n",
        Skin::En,
    );
    // At the top level: none, one or three between items, at the start and at the end.
    let items = "fn a():\n    pass\n\nfn b():\n    pass\n";
    equal(items, &items.replace("\n\n", "\n"), Skin::En);
    equal(items, &items.replace("\n\n", "\n\n\n\n"), Skin::En);
    equal(items, &format!("\n\n{items}\n\n\n"), Skin::En);
    equal(
        "use std/io\n\n# head\nfn a():\n    pass\n",
        "use std/io\n# head\n\nfn a():\n    pass\n",
        Skin::En,
    );
    // Before `else`.
    equal(
        "fn main():\n    if x:\n        f()\n    else:\n        g()\n",
        "fn main():\n    if x:\n        f()\n\n    else:\n        g()\n",
        Skin::En,
    );
    equal(
        "fn main():\n    if x:\n        f()\n    # c\n    else:\n        g()\n",
        "fn main():\n    if x:\n        f()\n\n    # c\n\n    else:\n        g()\n",
        Skin::En,
    );
    // After an attribute line.
    equal(
        "impl A:\n    @inline\n    fn f():\n        pass\n",
        "impl A:\n    @inline\n\n    fn f():\n        pass\n",
        Skin::En,
    );
}

#[test]
fn trailing_commas_are_not_recorded_except_in_a_one_element_tuple() {
    equal("let x = f(a, b,)\n", "let x = f(a, b)\n", Skin::En);
    equal("let x = [1, 2,]\n", "let x = [1, 2]\n", Skin::En);
    equal("let x = {1: 2,}\n", "let x = {1: 2}\n", Skin::En);
    equal("let x = (1, 2,)\n", "let x = (1, 2)\n", Skin::En);
    equal(
        "fn f(a: Int,):\n    pass\n",
        "fn f(a: Int):\n    pass\n",
        Skin::En,
    );
    equal("let g = fn(x,) => x\n", "let g = fn(x) => x\n", Skin::En);
    equal("let x = f(a,)\n", "let x = f(a)\n", Skin::En);
    equal("let x = xs[0](a,)\n", "let x = xs[0](a)\n", Skin::En);
    equal("use a/{b, c,}\n", "use a/{b, c}\n", Skin::En);
    // The Arabic comma is the same concept.
    equal(
        "دالة رئيسي():\n    ليكن x = f(a، b،)\n",
        "دالة رئيسي():\n    ليكن x = f(a، b)\n",
        Skin::Ar,
    );
    // `(x,)` is a one-element tuple and `(x)` is just `x`.
    differ("let t = (x,)\n", "let t = (x)\n", Skin::En);
    differ(
        "fn f() -> (Int,):\n    pass\n",
        "fn f() -> (Int):\n    pass\n",
        Skin::En,
    );
    differ(
        "fn main():\n    match t:\n        case (x,): pass\n",
        "fn main():\n    match t:\n        case (x): pass\n",
        Skin::En,
    );
    // A comma between items is always recorded.
    differ("let x = f(a, b)\n", "let x = f(ab)\n", Skin::En);
}

#[test]
fn one_line_blocks_are_their_indented_form() {
    for (one, indented) in [
        (
            "fn main():\n    if x: f()\n",
            "fn main():\n    if x:\n        f()\n",
        ),
        (
            "fn main():\n    if x: f()\n    else if y: g()\n    else: h()\n",
            "fn main():\n    if x:\n        f()\n    else if y:\n        g()\n    else:\n        h()\n",
        ),
        (
            "fn main():\n    for n in xs: print(n)\n    g()\n",
            "fn main():\n    for n in xs:\n        print(n)\n    g()\n",
        ),
        (
            "fn main():\n    while c: f()  # note\n",
            "fn main():\n    while c:\n        f()  # note\n",
        ),
        ("fn f() -> Int: return 1\n", "fn f() -> Int:\n    return 1\n"),
        (
            "fn main():\n    match x:\n        case 1: f()\n        case _: g()\n",
            "fn main():\n    match x:\n        case 1:\n            f()\n        case _:\n            g()\n",
        ),
    ] {
        equal(one, indented, Skin::En);
    }
    // An inline `if` expression is not a block: it keeps its form.
    differ(
        "fn main():\n    if c: f() else: g()\n",
        "fn main():\n    if c:\n        f()\n    else:\n        g()\n",
        Skin::En,
    );
    differ(
        "fn main():\n    let v = if c: a else: b\n",
        "fn main():\n    let v = if c:\n        a\n    else:\n        b\n",
        Skin::En,
    );
    // ... and inside a one-line block it stays inline.
    equal(
        "fn main():\n    if c: return if a: b else: d\n",
        "fn main():\n    if c:\n        return if a: b else: d\n",
        Skin::En,
    );
    // Where the next statement goes is still structure.
    differ(
        "fn main():\n    if x: f()\n    g()\n",
        "fn main():\n    if x:\n        f()\n        g()\n",
        Skin::En,
    );
}

#[test]
fn identifiers_are_exact_code_points() {
    // No folding: `علي` and `على` are different identifiers (§5.3, §4.4).
    differ("let علي = 1\n", "let على = 1\n", Skin::En);
}

// ----- robustness -------------------------------------------------------------------------

#[test]
fn never_panics_on_broken_or_odd_input() {
    let inputs = [
        "",
        "\n\n\n",
        "# only a comment",
        "fn main(:\n    let = \n",
        "let s = \"unterminated {x",
        "let s = \"{}{ }{a b}{\"\n",
        "let s = \"\"\"\nmulti {line}\n  text\n\"\"\"\n",
        "let s = sql\"select {x}\"\n",
        "x = 99999999999999999999999999999999999999999999999 + 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\n",
        "let x = 3ala + 1٢3 + ۱\n",
        "fn f():\n\tpass\n",
        "let s = \"{\"{\"{\"{x}\"}\"}\"}\"\n",
        ")(][}{:=>->,.?\n",
        "let x = 1 ; let y = 2\n",
        "\u{202E}let x = 1\n",
    ];
    for text in inputs {
        for skin in Skin::ALL {
            let c = cst(text, skin);
            let k = kcf::serialize(&c);
            assert!(k.starts_with(kcf::MAGIC), "{text:?}");
            let _ = kcf::hash(&c);
        }
    }
    // Every prefix of a real program.
    let text =
        std::fs::read_to_string(repo_root().join("eval/corpus/t11-user-record/ar.kalam")).unwrap();
    for (i, _) in text.char_indices().step_by(7) {
        kcf::serialize(&cst(&text[..i], Skin::Ar));
    }
}

#[test]
fn kcf_of_a_tree_at_the_parsers_height_cap_fits_a_1mib_stack() {
    let src = format!("fn main():\n    let x = 1{}\n", " + 1".repeat(3000));
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let out = parse(&src, Skin::En);
            let k = kcf::serialize(&out.cst);
            assert_eq!(out.cst.text(), src);
            k.len()
        })
        .unwrap()
        .join()
        .expect("KCF within 1 MiB");
}

#[test]
fn hash_is_sha256_of_the_kcf_bytes() {
    let c = cst("fn main():\n    print(\"hi\")\n", Skin::En);
    let h = kcf::hash(&c);
    assert_eq!(h, KcfHash::of(&kcf::serialize(&c)));
    assert_eq!(h.to_hex().len(), 64);
    assert_eq!(h.to_string(), h.to_hex());
}
