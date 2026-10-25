//! Property suites for the Kalam front end (scorecard metric `D1.property_tests`, M1
//! checklist items 12 and 16).
//!
//! - [`gen`]: a random AST generator (spec §8.2 grammar, explicit parentheses where §10.6
//!   precedence needs them);
//! - [`print`]: an independent reference printer from AST to text in any skin;
//! - [`soup`]: hostile text for the lexer and parser;
//! - [`props`]: the properties and the report (`eval/props/report.json`).
//!
//! Run with `cargo test -p kalam-props --release`. `KALAM_PROPS_CASES` (default 10 000),
//! `KALAM_PROPS_SEED`, `KALAM_PROPS_CASE` (one case, to reproduce a failure) and
//! `KALAM_PROPS_THREADS` configure the run; `KALAM_PROPS_REPORT` overrides the report path
//! (`-` writes none).

pub mod gen;
pub mod print;
pub mod props;
pub mod rng;
pub mod soup;

pub use props::{run, Report, Settings};

/// A generated program for `seed`, printed in `skin` (used by `kalam-fuzz` for seeds and
/// grammar-aware mutations).
pub fn generated_program(seed: u64, skin: kalam_base::Skin) -> String {
    let ast = gen::Gen::new(seed).program();
    print::print(&ast, skin, rng::splitmix(seed ^ 1), rng::splitmix(seed ^ 2))
}
