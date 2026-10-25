//! Prints the lexer and parser diagnostics of a file (skin from its extension):
//! `cargo run -p kalam-props --example codes -- FILE`. Handy when a property fails.
fn main() {
    let path = std::env::args().nth(1).expect("FILE");
    let skin = kalam_base::Skin::from_path(&path).expect("a .kl, .ar.kl or .franco.kl file");
    let text = std::fs::read_to_string(&path).expect("read");
    let out = kalam_parser::parse_source(&text, skin, kalam_base::FileId(0));
    for d in &out.diagnostics {
        let at = text
            .get(d.span.start as usize..d.span.end as usize)
            .unwrap_or("?");
        println!("{} at {}: {at:?}", d.code, d.span.start);
    }
    println!("{} diagnostic(s)", out.diagnostics.len());
}
