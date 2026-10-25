//! Integration tests over the real stdlib stubs in `stdlib/src` and the checked-in
//! `stdlib/api-index.json`.

use std::collections::BTreeSet;
use std::fs;

use kalam_base::Skin;
use kalam_stdlib_index::{default_stdlib_root, generate, stub_files, ApiIndex, INDEX_FILE};

fn index() -> ApiIndex {
    generate(&default_stdlib_root()).expect("the stdlib stubs parse and follow the conventions")
}

#[test]
fn checked_in_index_is_up_to_date() {
    let root = default_stdlib_root();
    let fresh = index().to_json();
    let on_disk = fs::read_to_string(root.join(INDEX_FILE)).unwrap_or_default();
    assert!(
        on_disk == fresh,
        "stdlib/{INDEX_FILE} is stale: run `cargo run -p kalam-stdlib-index` to regenerate it"
    );
}

#[test]
fn every_stub_file_is_a_formatter_fixed_point() {
    let root = default_stdlib_root();
    let files = stub_files(&root).expect("stdlib/src is readable");
    assert!(
        files.len() >= 15,
        "expected the M2 module set, found {}",
        files.len()
    );
    for rel in files {
        let text = fs::read_to_string(root.join(&rel)).unwrap();
        let formatted = kalam_fmt::format(&text, Skin::En, &kalam_fmt::FmtConfig::default())
            .unwrap_or_else(|d| panic!("{rel} does not parse: {d:?}"));
        assert!(
            formatted == text,
            "{rel} is not in canonical `kalam fmt` form"
        );
    }
}

#[test]
fn every_stub_converts_to_every_skin() {
    let root = default_stdlib_root();
    for rel in stub_files(&root).unwrap() {
        let text = fs::read_to_string(root.join(&rel)).unwrap();
        for to in [Skin::Ar, Skin::Franco] {
            let out = kalam_fmt::convert(&text, Skin::En, to, &kalam_fmt::FmtConfig::default());
            assert!(out.is_ok(), "{rel} does not convert to {to:?}");
        }
    }
}

#[test]
fn paths_are_unique_and_sorted() {
    let idx = index();
    let mut seen = BTreeSet::new();
    for e in &idx.items {
        assert!(seen.insert(e.path.clone()), "duplicate path {}", e.path);
    }
    let paths: Vec<&str> = idx.items.iter().map(|e| e.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
}

fn entry<'a>(idx: &'a ApiIndex, path: &str) -> &'a kalam_stdlib_index::Entry {
    idx.find(path)
        .unwrap_or_else(|| panic!("missing stdlib item {path}"))
}

#[test]
fn api_used_by_the_dev_corpus_exists() {
    let idx = index();
    // Members by owner type (method resolution never depends on imports, DR-27).
    for path in [
        "String.trim",
        "String.is_empty",
        "String.len_chars",
        "String.split",
        "String.to_int",
        "String.to_lower",
        "String.replace",
        "String.arabic_fold",
        "String.chars",
        "String.contains",
        "List.append",
        "List.len",
        "List.is_empty",
        "List.get",
        "List.map",
        "List.filter",
        "List.find",
        "List.any",
        "Map.get",
        "Map.len",
        "Map.insert",
        "Int.to_float",
        "Float.to_int",
        "Option.expect",
        "Option.or",
        "Result.map_err",
        "Stdin.read_line",
        "Rand.seeded",
        "SeededRand.int",
        "Cell.new",
        "Cell.get",
        "Cell.set",
        "Html.render",
        "Request.app",
        "Request.path_int",
        "Request.json",
        "Request.query_param",
        "Response.json",
        "Response.text",
        "Response.html",
        "Response.status",
        "Router.get",
        "Router.post",
        "Router.put",
        "Router.delete",
        "Db.all",
        "Db.one",
        "Db.exec",
        "Fs.read_text",
        "Proc.run",
        "PathSegment.parse",
        "Regex.matches",
        "Secret.new",
        "World.stdin",
        "World.net",
        "World.fs",
        "std/math.round",
        "std/json.encode",
        "std/json.decode",
        "std/json.DecodeError",
        "std/http.router",
        "std/http.serve",
        "std/http.HttpError",
        "std/sql.sqlite",
        "std/sql.Db",
        "std/fs.FsError",
        "std/proc.Output",
        "std/proc.ProcError",
        "print",
        "panic",
        "assert",
        "World",
        "AnyError",
        "Some",
        "None",
        "Ok",
        "Err",
    ] {
        entry(&idx, path);
    }
}

#[test]
fn sinks_take_safe_string_kinds_never_string() {
    let idx = index();
    for path in ["Db.all", "Db.one", "Db.exec"] {
        assert!(entry(&idx, path).signature.contains("query: Sql"), "{path}");
    }
    assert!(entry(&idx, "Response.html").signature.contains(": Html"));
    assert!(entry(&idx, "Fs.read_text").signature.contains(": Path"));
    assert!(entry(&idx, "Proc.run").signature.contains(": Command"));
}

#[test]
fn router_access_has_no_default() {
    let idx = index();
    for path in ["Router.get", "Router.post", "Router.put", "Router.delete"] {
        let sig = &entry(&idx, path).signature;
        assert!(sig.contains("access: Access)"), "{path}: {sig}");
    }
}

#[test]
fn question_mark_conversions_into_http_error_exist() {
    let idx = index();
    let froms: Vec<&str> = idx
        .items
        .iter()
        .filter(|e| e.owner.as_deref() == Some("HttpError") && e.name == "from")
        .filter_map(|e| e.receiver.as_deref())
        .collect();
    for source in ["DbError", "DecodeError", "AnyError"] {
        assert!(
            froms.iter().any(|r| r.contains(&format!("From[{source}]"))),
            "missing impl HttpError as From[{source}]: {froms:?}"
        );
    }
}

#[test]
fn suspension_bits() {
    let idx = index();
    let bit = |p: &str| entry(&idx, p).suspends.clone();
    assert_eq!(bit("Fs.read_text").as_deref(), Some("yes"));
    assert_eq!(bit("Db.all").as_deref(), Some("yes"));
    assert_eq!(bit("std/http.serve").as_deref(), Some("yes"));
    assert_eq!(bit("Stdin.read_line").as_deref(), Some("yes"));
    assert_eq!(bit("String.trim").as_deref(), Some("no"));
    assert_eq!(bit("print").as_deref(), Some("no"));
    // Effect-polymorphic: suspends exactly when the callback does (spec §15.3).
    assert_eq!(bit("List.map").as_deref(), Some("if_callback"));
    assert_eq!(bit("Result.map_err").as_deref(), Some("if_callback"));
    // `@sync` callbacks never suspend; stored callbacks are not called here.
    assert_eq!(bit("List.sort_by").as_deref(), Some("no"));
    assert_eq!(bit("Cell.update").as_deref(), Some("no"));
    assert_eq!(bit("Router.get").as_deref(), Some("no"));
    // Not a function.
    assert_eq!(bit("std/http.HttpError"), None);
}

#[test]
fn capabilities_come_from_the_signature() {
    let idx = index();
    let caps = |p: &str| entry(&idx, p).capability.clone();
    assert_eq!(caps("Fs.read_text"), vec!["Fs"]);
    assert_eq!(caps("std/sql.sqlite"), vec!["Fs"]);
    assert_eq!(caps("std/sql.postgres"), vec!["Net"]);
    assert_eq!(caps("Db.exec"), vec!["Db"]);
    assert_eq!(caps("std/http.serve"), vec!["Net"]);
    assert_eq!(caps("Stdin.read_line"), vec!["Stdin"]);
    assert_eq!(caps("Rand.int"), vec!["Rand"]);
    // D20: console output is ambient but recorded, so libraries can declare it.
    assert_eq!(caps("print"), vec!["console"]);
    assert_eq!(caps("std/log.info"), vec!["console"]);
    // No capability: pure code, and the deterministic generator (spec §18.2).
    assert!(caps("String.trim").is_empty());
    assert!(caps("Rand.seeded").is_empty());
    assert!(caps("SeededRand.int").is_empty());
    assert!(caps("std/json.decode").is_empty());
}

#[test]
fn core_names_carry_their_skin_spellings() {
    let idx = index();
    let int = entry(&idx, "Int");
    let sp = int.spellings.as_ref().expect("Int is a core name");
    assert_eq!(sp.en, "Int");
    assert!(!sp.ar.is_empty() && !sp.franco.is_empty());
    assert!(entry(&idx, "Some").spellings.is_some());
    assert!(entry(&idx, "print").spellings.is_some());
    assert!(entry(&idx, "Cell").spellings.is_none());
}

#[test]
fn rand_seeded_has_no_secret_methods() {
    let idx = index();
    assert!(idx.find("SeededRand.token").is_none());
    assert!(idx.find("SeededRand.uuid_v4").is_none());
    assert!(idx.find("Rand.token").is_some());
}

#[test]
fn marker_traits_are_listed() {
    let idx = index();
    for t in [
        "Eq", "Hash", "Ord", "Encode", "Decode", "Row", "Message", "SqlParam",
    ] {
        assert_eq!(entry(&idx, t).kind, "trait", "{t}");
    }
    assert_eq!(entry(&idx, "Show.show").kind, "trait_method");
}

#[test]
fn private_items_are_not_listed() {
    let idx = index();
    assert!(idx
        .items
        .iter()
        .all(|e| !e.path.contains("panic(\"builtin\")")));
    // Every listed function or method is a builtin stub at M2.
    for e in &idx.items {
        if matches!(e.kind.as_str(), "fn" | "method" | "assoc_fn") {
            assert!(e.attributes.iter().any(|a| a == "builtin"), "{}", e.path);
        }
    }
}

#[test]
fn prelude_modules_are_marked_ambient() {
    let idx = index();
    let ambient = |p: &str| entry(&idx, p).attributes.iter().any(|a| a == "prelude");
    assert!(ambient("std/prelude"));
    assert!(ambient("std/json"));
    assert!(ambient("std/log"));
    assert!(!ambient("std/http"));
    assert!(!ambient("std/math"));
}
