//! Construct-level tests for the §8.2 grammar, the §8.3 disambiguation rules, precedence
//! (§10.6), interpolation holes (§5.7), leading-dot continuation (§5.8) and trivia.

mod common;

use common::*;
use kalam_ast::{ItemKind, TriviaKind};
use kalam_base::Skin;

/// Parses an English program and asserts it produced no parser diagnostics.
fn ok(src: &str) -> String {
    let (out, _) = parse_text(src, Skin::En);
    assert!(
        out.diagnostics.is_empty(),
        "unexpected parser diagnostics for\n{src}\n{:?}",
        out.diagnostics
    );
    assert_eq!(out.cst.text(), src, "CST must be lossless");
    shape_of(&out)
}

fn has(shape: &str, frag: &str) {
    assert!(shape.contains(frag), "missing\n  {frag}\nin\n{shape}");
}

fn in_main(body: &str) -> String {
    let mut src = String::from("fn main():\n");
    for line in body.lines() {
        src.push_str("    ");
        src.push_str(line);
        src.push('\n');
    }
    ok(&src)
}

#[test]
fn precedence_follows_spec_10_6() {
    let s = in_main("let r = a or b and not c == d + e * -f.g(1)?");
    has(
        &s,
        "(let (bind \"r\") - (or \"a\" (and \"b\" (not (== \"c\" (+ \"d\" (* \"e\" (neg (? (call (. \"f\" \"g\") (args (arg (int 1))))))))))))))",
    );
    let s = in_main("let r = 1..=n + 1\nlet q = a - b - c\nlet m = a // b % c");
    has(&s, "(..= (int 1) (+ \"n\" (int 1)))");
    has(&s, "(- (- \"a\" \"b\") \"c\")");
    has(&s, "(% (// \"a\" \"b\") \"c\")");
}

#[test]
fn if_disambiguation_8_3() {
    let s = in_main("if x < 0: return Err(.Neg)\nlet s = if done: \"done\" else: \"open\"\nreturn if a > b: a else: b");
    has(&s, "(expr (if (branch (< \"x\" (int 0)) (line (return (call CORE_ERR (args (arg (dot \"Neg\")))))))))");
    has(&s, "(let (bind \"s\") - (if-inline (branch \"done\" (line (expr (str \"done\")))) (else (line (expr (str \"open\"))))))");
    has(&s, "(return (if-inline (branch (> \"a\" \"b\") (line (expr \"a\"))) (else (line (expr \"b\")))))");

    let s = in_main("if a:\n    f()\nelse if b: g()\nelse:\n    h()");
    has(&s, "(expr (if (branch \"a\" (block (expr (call \"f\" (args))))) (branch \"b\" (line (expr (call \"g\" (args))))) (else (block (expr (call \"h\" (args)))))))");

    // An inline `if` is valid in argument position (§8.3 rule 2).
    let s = in_main("print(if same: \"a\" else: \"b\")");
    has(&s, "(call CORE_PRINT (args (arg (if-inline");
}

#[test]
fn value_forms_after_binding_heads() {
    let s = in_main("let fee = match plan:\n    case .Free: 0\n    case .Pro(seats):\n        let base = 10 * seats\n        base - base // 10\nprint(fee)");
    has(&s, "(let (bind \"fee\") - (match \"plan\" (case (. \"Free\") (line (expr (int 0)))) (case (. \"Pro\" ((pf (bind \"seats\")))) (block (let (bind \"base\") - (* (int 10) \"seats\")) (expr (- \"base\" (// \"base\" (int 10))))))))");
    has(&s, "(expr (call CORE_PRINT (args (arg \"fee\"))))");
    let s = in_main("return match x:\n    case _: 1");
    has(&s, "(return (match \"x\" (case _ (line (expr (int 1))))))");
    let s = in_main("var y = 0\ny = if c:\n    1\nelse:\n    2");
    has(
        &s,
        "(= \"y\" (if (branch \"c\" (block (expr (int 1)))) (else (block (expr (int 2))))))",
    );
}

#[test]
fn functions_lambdas_and_module_items() {
    let s = ok("let WIDTH: Int = 80\n\nfn covers(top: Int, y: Int) -> Bool => y >= top and y < top + 5\n\nfn main(world: World) -> Result[(), AnyError]:\n    let f = fn(t) => if t.id == id: todo else: t\n    let g = fn(_, x: Int) -> Int => x\n    return Ok(())\n");
    has(&s, "(item (const \"WIDTH\" (t CORE_INT) (int 80)))");
    has(&s, "(fn (sig \"covers\" ((param \"top\" (t CORE_INT) -)(param \"y\" (t CORE_INT) -)) -> (t CORE_BOOL)) => (and (>= \"y\" \"top\") (< \"y\" (+ \"top\" (int 5)))))");
    has(&s, "(sig CORE_MAIN ((param \"world\" (t CORE_WORLD) -)) -> (t CORE_RESULT [ () (t \"AnyError\")]))");
    has(
        &s,
        "(lambda ((lp \"t\" -)) - (if-inline (branch (== (. \"t\" \"id\") \"id\")",
    );
    has(
        &s,
        "(lambda ((lp \"_\" -)(lp \"x\" (t CORE_INT))) -> (t CORE_INT) \"x\")",
    );
    has(&s, "(return (call CORE_OK (args (arg ()))))");
}

#[test]
fn local_fn_vs_lambda_statement() {
    let s = in_main("fn helper(x: Int) -> Int:\n    return x\nhelper(1)");
    has(&s, "(fn (sig \"helper\"");
}

#[test]
fn declarations_cover_the_item_grammar() {
    let src = "use std/http.{self, Request as Req}\nexport use app/db as database\n\n@derive(Encode, Decode)\nexport opaque type Pair[A, B: Show + Eq]:\n    @name(\"first\")\n    first: A\n    second: B = default()\n    type: Int\n\ntype Ids = List[Int]\n\nenum Shape:\n    Circle(radius: Float)\n    Rect(Float, Float)\n    Empty\n\ntrait Show:\n    fn show(self) -> String\n    fn twice(self) -> String => self.show() + self.show()\n\nimpl[T] Box[T] as Show:\n    export fn show(self) -> String:\n        return \"box\"\n\nextern fn now() -> Int\nextern type Element[T]\n\ntest \"adds\":\n    assert(1 + 1 == 2)\n\nfn apply(f: @sync fn(Int) -> Int, pair: (Int, String)) -> ():\n    pass\n";
    let s = ok(src);
    has(&s, "(use \"std/http\" { self \"Request\" as \"Req\" })");
    has(&s, "(export-use \"app/db\" as \"database\")");
    has(&s, "(item export-opaque (@derive (args (arg \"Encode\") (arg \"Decode\"))) (type \"Pair\" [(gp \"A\")(gp \"B\" (t \"Show\") (t \"Eq\"))] (field (@name (args (arg (str \"first\")))) \"first\" (t \"A\") -) (field \"second\" (t \"B\") (call \"default\" (args))) (field \"type\" (t CORE_INT) -)))");
    has(&s, "(type \"Ids\" = (t CORE_LIST [ (t CORE_INT)]))");
    has(&s, "(enum \"Shape\" (variant \"Circle\" ((vf \"radius\" (t CORE_FLOAT)))) (variant \"Rect\" ((vf - (t CORE_FLOAT))(vf - (t CORE_FLOAT)))) (variant \"Empty\"))");
    has(&s, "(trait \"Show\" (fn (sig \"show\" ((self)) -> (t CORE_STRING)) required) (fn (sig \"twice\" ((self)) -> (t CORE_STRING)) => (+ (call (. self \"show\") (args)) (call (. self \"show\") (args)))))");
    has(&s, "(impl [(gp \"T\")] (t \"Box\" [ (t \"T\")]) as (t \"Show\") (member export (fn (sig \"show\" ((self)) -> (t CORE_STRING)) (block (return (str \"box\"))))))");
    has(&s, "(extern (sig \"now\" () -> (t CORE_INT)))");
    has(&s, "(extern type \"Element\" [(gp \"T\")])");
    has(&s, "(test (str \"adds\") (block (expr (call CORE_ASSERT (args (arg (== (+ (int 1) (int 1)) (int 2))))))))");
    has(&s, "(param \"f\" (sync-fn ( (t CORE_INT)) -> (t CORE_INT)) -)(param \"pair\" (tuple-t (t CORE_INT) (t CORE_STRING)) -)) -> ()");
}

#[test]
fn patterns_cover_the_pattern_grammar() {
    let s = in_main("match v:\n    case Some(x) if x > 0: pass\n    case None | Err(_): pass\n    case -5..=5: pass\n    case 7: pass\n    case \"hi\": pass\n    case true: pass\n    case [first, .., last]: pass\n    case [..rest]: pass\n    case (a, b): pass\n    case Shape.Circle(radius: r) as c: pass\n    case Point(x: 0, ..): pass\n    case other: pass");
    has(
        &s,
        "(case (CORE_SOME (bind \"x\")) if (> \"x\" (int 0)) (line (pass)))",
    );
    has(&s, "(case (or (CORE_NONE) (CORE_ERR _)) (line (pass)))");
    has(&s, "(case (range= -5 5)");
    has(&s, "(case (int 7)");
    has(&s, "(case (str \"hi\")");
    has(&s, "(case true");
    has(&s, "(case (list (bind \"first\") (..) (bind \"last\"))");
    has(&s, "(case (list (.. \"rest\"))");
    has(&s, "(case (tuple (bind \"a\") (bind \"b\"))");
    has(
        &s,
        "(case (as (path \"Shape\" \"Circle\" ((pf \"radius\": (bind \"r\")))) \"c\")",
    );
    has(&s, "(case (path \"Point\" ((pf \"x\": (int 0)) ..))");
    has(&s, "(case (bind \"other\")");
}

#[test]
fn statements_and_collections() {
    let s = in_main("for (name, city) in [(\"A\", \"B\")]:\n    let _ = db.exec(name)?\nwhile running(g):\n    g.tick += 1\n    xs[0] = {k: 1, \"z\": 2}\n    break\nvar t: Int = ()\ncontinue\nlet u = (1)\nlet e = {}\nlet x = unsafe.raw\nlet z = t.0.1");
    has(&s, "(for (tuple (bind \"name\") (bind \"city\")) (list (tuple (str \"A\") (str \"B\"))) (block (let _ - (? (call (. \"db\" \"exec\") (args (arg \"name\")))))))");
    has(&s, "(while (call \"running\" (args (arg \"g\"))) (block (+= (. \"g\" \"tick\") (int 1)) (= (index \"xs\" (int 0)) (map (kv \"k\" (int 1)) (kv (str \"z\") (int 2)))) (break)))");
    has(&s, "(var \"t\" (t CORE_INT) ())");
    has(&s, "(continue)");
    has(&s, "(let (bind \"u\") - (paren (int 1)))");
    has(&s, "(let (bind \"e\") - (map))");
    has(&s, "(let (bind \"x\") - (unsafe \"raw\"))");
    has(&s, "(let (bind \"z\") - (. (. \"t\" 0) 1))");
}

#[test]
fn keyword_labels_and_fields_are_identifiers_rule_8() {
    let s = in_main("f(match: 1, type: 2)\nlet r = contract.law");
    has(
        &s,
        "(args (arg \"match\": (int 1)) (arg \"type\": (int 2)))",
    );
}

#[test]
fn leading_dot_continuation_5_8() {
    let s = ok("fn routes() -> Router[App]:\n    return http.router()\n        .get(\"/a\", a, access: .Public)\n        .post(\"/b\", b)\n\nfn f():\n    pass\n");
    has(&s, "(return (call (. (call (. (call (. \"http\" \"router\") (args)) \"get\") (args (arg (str \"/a\")) (arg \"a\") (arg \"access\": (dot \"Public\")))) \"post\") (args (arg (str \"/b\")) (arg \"b\"))))");
    has(&s, "(sig \"f\"");
}

#[test]
fn interpolation_holes_are_parsed_code() {
    let s = in_main("print(\"a {x.y} b {f(1, 2)}\")\nprint(\"\\{not a hole\\}\")\nlet q = sql\"SELECT {id}\"\nlet t = \"\"\"\n    {a}\n\"\"\"");
    has(&s, "(str \"a \" (hole (. \"x\" \"y\")) \" b \" (hole (call \"f\" (args (arg (int 1)) (arg (int 2))))))");
    has(&s, "(str \"\\\\{not a hole\\\\}\")");
    has(&s, "(tagged sql (str \"SELECT \" (hole \"id\")))");
    // in_main indents every line, including the string body and its closing quotes.
    has(&s, "(str3 \"\\n        \" (hole \"a\") \"\\n    \")");
}

#[test]
fn holes_convert_like_code_across_skins() {
    let en = parse_text(
        "fn main():\n    print(\"{not x}, {f(a, Some(1))}\")\n",
        Skin::En,
    )
    .0;
    let ar = parse_text(
        "دالة رئيسي():\n    اطبع(\"{ليس x}, {f(a، موجود(1))}\")\n",
        Skin::Ar,
    )
    .0;
    let fr = parse_text(
        "dala ra2isi():\n    ektb(\"{mesh x}, {f(a, mawgood(1))}\")\n",
        Skin::Franco,
    )
    .0;
    assert!(en.diagnostics.is_empty() && ar.diagnostics.is_empty() && fr.diagnostics.is_empty());
    assert_eq!(shape_of(&en), shape_of(&ar));
    assert_eq!(shape_of(&en), shape_of(&fr));
    has(&shape_of(&en), "(hole (not \"x\"))");
}

#[test]
fn numbers_are_skin_free() {
    let en = parse_text("fn main():\n    let x = 12 + 1.5\n", Skin::En).0;
    let ar = parse_text("دالة رئيسي():\n    ليكن x = ١٢ + ١٫٥\n", Skin::Ar).0;
    assert!(ar.diagnostics.is_empty(), "{:?}", ar.diagnostics);
    assert_eq!(shape_of(&en), shape_of(&ar));
}

#[test]
fn trivia_keeps_comments_and_blank_lines() {
    let src = "# header\n## doc\nfn main():\n\n    pass # trailing\n";
    let (out, _) = parse_text(src, Skin::En);
    assert!(out.diagnostics.is_empty());
    let kinds: Vec<_> = out
        .ast
        .trivia
        .iter()
        .map(|t| (t.kind, t.line, t.text.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (TriviaKind::Comment, 1, "# header"),
            (TriviaKind::DocComment, 2, "## doc"),
            (TriviaKind::BlankLine, 4, ""),
            (TriviaKind::Comment, 5, "# trailing"),
        ]
    );
    assert_eq!(out.cst.text(), src);
}

#[test]
fn spans_point_into_the_source() {
    let src = "fn main():\n    print(\"hi\")\n";
    let (out, _) = parse_text(src, Skin::En);
    let item = &out.ast.file.items[0];
    assert_eq!(&src[item.span.range()], "fn main():\n    print(\"hi\")");
    match &item.kind {
        ItemKind::Fn(f) => {
            assert_eq!(&src[f.sig.span.range()], "fn main()");
            assert_eq!(&src[f.sig.name.span.range()], "main");
        }
        other => panic!("{other:?}"),
    }
}
