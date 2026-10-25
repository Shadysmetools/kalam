//! `kalam-stdlib-index [--check] [STDLIB_DIR]`: regenerates `STDLIB_DIR/api-index.json`
//! (default: the repository's `stdlib/`). With `--check`, writes nothing and exits 1 when
//! the file is stale.

use std::path::PathBuf;
use std::process::ExitCode;

use kalam_stdlib_index::{default_stdlib_root, generate, INDEX_FILE};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let root = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(default_stdlib_root);
    let index = match generate(&root) {
        Ok(index) => index,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let json = index.to_json();
    let target = root.join(INDEX_FILE);
    if check {
        let current = std::fs::read_to_string(&target).unwrap_or_default();
        if current != json {
            eprintln!("{} is stale; run kalam-stdlib-index", target.display());
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }
    if let Err(e) = std::fs::write(&target, json) {
        eprintln!("cannot write {}: {e}", target.display());
        return ExitCode::from(1);
    }
    println!("wrote {} ({} items)", target.display(), index.items.len());
    ExitCode::SUCCESS
}
