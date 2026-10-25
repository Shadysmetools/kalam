//! The type checker is a fuzz target: the deepest programs the parser accepts must not
//! overflow the stack. Each program runs through `check_source` (lex, parse, resolve, type
//! check) on a thread with a 1 MiB stack, the Windows main-thread default. The parser
//! accepts bracket nesting up to 64 levels and operator or postfix chains up to 1024 links.

fn on_small_stack(src: String) -> Vec<&'static str> {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            kalam_driver::check_source("main.kl", &src)
                .unwrap()
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect()
        })
        .unwrap()
        .join()
        .expect("no stack overflow")
}

#[test]
fn a_long_string_chain_into_a_sink() {
    let mut chain = String::from("\"a\"");
    for i in 0..450 {
        chain.push_str(&format!(" + x{} + \"b\"", i % 2));
    }
    let src = format!(
        "use std/sql.{{Db, DbError}}\n\nfn f(db: Db, x0: String, x1: String) -> Result[Int, DbError]:\n    return db.exec({chain})\n\nfn main():\n    pass\n"
    );
    assert_eq!(on_small_stack(src), vec!["K1601"]);
}

#[test]
fn long_operator_and_method_chains() {
    let sum = vec!["1"; 1000].join(" + ");
    assert!(on_small_stack(format!(
        "fn main():\n    let n = {sum}\n    print(\"{{n}}\")\n"
    ))
    .is_empty());
    // A call is two postfix links (member, call): 450 calls stay under 1024.
    let calls = ".trim()".repeat(450);
    let codes = on_small_stack(format!(
        "fn main():\n    let s = \" a \"{calls}\n    print(s)\n"
    ));
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn deeply_nested_values_patterns_and_types() {
    let depth = 60;
    let list = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
    assert!(on_small_stack(format!(
        "fn main():\n    let xs = {list}\n    print(\"{{xs.len()}}\")\n"
    ))
    .is_empty());
    let some = format!("{}1{}", "Some(".repeat(depth), ")".repeat(depth));
    let pat = format!("{}n{}", "Some(".repeat(depth), ")".repeat(depth));
    assert!(on_small_stack(format!(
        "fn main():\n    let v = {some}\n    match v:\n        case {pat}:\n            print(\"{{n}}\")\n        case _:\n            pass\n"
    ))
    .is_empty());
    let ty = format!("{}Int{}", "List[".repeat(depth), "]".repeat(depth));
    let codes = on_small_stack(format!(
        "fn f(x: {ty}) -> {ty} => x\n\nfn main():\n    pass\n"
    ));
    assert!(codes.is_empty(), "{codes:?}");
}
