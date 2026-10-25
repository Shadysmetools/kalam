//! `kalam`: the single Kalam binary (D10, spec §21.5, ARCHITECTURE.md §19).
//!
//! Exit codes: 0 success; 1 diagnostics, failed tests, or `main` returned `Err`; 2 usage
//! errors and commands that are not implemented yet; 101 a panic in `kalam run`.
//! The message language comes from `KALAM_LANG` (`en`, `ar` or `franco`; default `en`).
//!
//! Argument parsing is hand-written in the scaffold; `clap` replaces it in Phase 1.

use std::process::ExitCode;

use kalam_base::Skin;
use kalam_diagnostics::{catalog, Lang};
use kalam_fmt::FmtConfig;
use kalam_skins::KeywordTable;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Commands that exist in the design but not yet in code, with the phase that adds them.
const PLANNED: &[(&str, &str)] = &[
    ("new", "Phase 1"),
    ("build", "Phase 1"),
    ("run", "Phase 1"),
    ("view", "Phase 0"),
    ("rename", "Phase 1"),
    ("diff", "Phase 1"),
    ("fix", "Phase 1"),
    ("audit", "Phase 1"),
    ("caps", "Phase 1"),
    ("bind", "Phase 2"),
    ("add", "Phase 2"),
    ("update", "Phase 2"),
    ("pkg", "Phase 2"),
    ("db", "Phase 2"),
];

fn usage() -> &'static str {
    "kalam: the Kalam toolchain\n\
     \n\
     usage:\n\
     \x20 kalam check [--json] FILE           check a file (scaffold: source text and lexing)\n\
     \x20 kalam fmt [--check|--stdout] FILE...  format files in canonical layout (in place)\n\
     \x20 kalam convert --to en|ar|franco [--write] FILE\n\
     \x20                                    print FILE in another skin (--write: rename it)\n\
     \x20 kalam explain CODE                  explain a diagnostic code (KALAM_LANG sets the language)\n\
     \x20 kalam test | lsp | mcp              planned (Phase 1)\n\
     \x20 kalam version\n\
     \n\
     See compiler/ARCHITECTURE.md §19 for the full command list.\n"
}

fn message_lang() -> Lang {
    std::env::var("KALAM_LANG")
        .ok()
        .and_then(|id| Skin::from_id(&id))
        .unwrap_or(Skin::En)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        eprint!("{}", usage());
        return ExitCode::from(2);
    };
    let rest = &args[1..];
    match command.as_str() {
        "version" | "--version" => {
            let table = KeywordTable::builtin();
            println!(
                "kalam {VERSION} (spec v0.1, keyword table: {} concepts, Unicode 16.0 pinned)",
                table.rows().len()
            );
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            print!("{}", usage());
            ExitCode::SUCCESS
        }
        "check" => check(rest),
        "fmt" => fmt(rest),
        "convert" => convert(rest),
        "explain" => explain(rest),
        "test" => report_unavailable(
            kalam_test_runner::run(
                ".",
                &kalam_test_runner::TestOptions {
                    filter: None,
                    update_snapshots: false,
                    reporter: kalam_test_runner::Reporter::Human,
                },
            )
            .map(|_| ()),
        ),
        "lsp" => report_unavailable(kalam_lsp::run_stdio()),
        "mcp" => report_unavailable(kalam_mcp::run_stdio()),
        other => match PLANNED.iter().find(|(name, _)| *name == other) {
            Some((name, phase)) => {
                eprintln!("`kalam {name}` is designed but not implemented yet ({phase}).");
                ExitCode::from(2)
            }
            None => {
                eprintln!("unknown command `{other}`\n");
                eprint!("{}", usage());
                ExitCode::from(2)
            }
        },
    }
}

fn report_unavailable(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn read_file(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("cannot read `{path}`: {e}");
        ExitCode::from(1)
    })
}

fn check(args: &[String]) -> ExitCode {
    let json = args.iter().any(|a| a == "--json");
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: kalam check [--json] FILE");
        return ExitCode::from(2);
    };
    let text = match read_file(path) {
        Ok(text) => text,
        Err(code) => return code,
    };
    let checked = match kalam_driver::check_source(path, &text) {
        Ok(checked) => checked,
        Err(_) => {
            eprintln!("`{path}` is not a Kalam file (expected .kl, .ar.kl or .franco.kl)");
            return ExitCode::from(2);
        }
    };
    if json {
        println!("{}", checked.render_json());
    } else if !checked.diagnostics.is_empty() {
        eprint!("{}", checked.render_human(message_lang()));
    }
    if checked.has_errors() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn report_source_error(path: &str, err: kalam_driver::SourceError) -> ExitCode {
    match err {
        kalam_driver::SourceError::NotKalam(_) => {
            eprintln!("`{path}` is not a Kalam file (expected .kl, .ar.kl or .franco.kl)");
            ExitCode::from(2)
        }
        kalam_driver::SourceError::Diagnostics(checked) => {
            eprint!("{}", checked.render_human(message_lang()));
            eprintln!("`{path}` has errors, so it was left unchanged.");
            ExitCode::from(1)
        }
    }
}

fn write_file(path: &str, text: &str) -> Result<(), ExitCode> {
    std::fs::write(path, text).map_err(|e| {
        eprintln!("cannot write `{path}`: {e}");
        ExitCode::from(1)
    })
}

/// `kalam fmt [--check|--stdout] FILE...` (spec §21.1). `--check` changes nothing and exits
/// 1 if a file is not in canonical form (CI); `--stdout` prints instead of writing.
fn fmt(args: &[String]) -> ExitCode {
    let check = args.iter().any(|a| a == "--check");
    let stdout = args.iter().any(|a| a == "--stdout");
    let paths: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if paths.is_empty() {
        eprintln!("usage: kalam fmt [--check|--stdout] FILE...");
        return ExitCode::from(2);
    }
    let config = FmtConfig::default();
    let mut status = ExitCode::SUCCESS;
    for path in paths {
        let text = match read_file(path) {
            Ok(text) => text,
            Err(code) => return code,
        };
        let formatted = match kalam_driver::format_source(path, &text, &config) {
            Ok(formatted) => {
                if let Some(why) = &formatted.layout_kept {
                    eprintln!("note: {path}: layout kept, spellings normalized only ({why})");
                }
                formatted.text
            }
            Err(err) => {
                status = report_source_error(path, err);
                continue;
            }
        };
        if stdout {
            print!("{formatted}");
        } else if formatted != text {
            if check {
                println!("would reformat {path}");
                status = ExitCode::from(1);
            } else if let Err(code) = write_file(path, &formatted) {
                return code;
            }
        }
    }
    status
}

/// `kalam convert --to SKIN [--write] FILE` (spec §4.3): prints the converted file, or with
/// `--write` renames it to the new skin's suffix (`a.kl` → `a.ar.kl`).
fn convert(args: &[String]) -> ExitCode {
    let mut to: Option<Skin> = None;
    let mut path: Option<&String> = None;
    let mut write = false;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--to" {
            to = args.get(i + 1).and_then(|id| Skin::from_id(id));
            i += 2;
        } else if args[i] == "--write" {
            write = true;
            i += 1;
        } else {
            path = Some(&args[i]);
            i += 1;
        }
    }
    let (Some(to), Some(path)) = (to, path) else {
        eprintln!("usage: kalam convert --to en|ar|franco [--write] FILE");
        return ExitCode::from(2);
    };
    let text = match read_file(path) {
        Ok(text) => text,
        Err(code) => return code,
    };
    let converted = match kalam_driver::convert_source(path, &text, to, &FmtConfig::default()) {
        Ok(converted) => converted,
        Err(err) => return report_source_error(path, err),
    };
    if !write {
        print!("{}", converted.text);
        return ExitCode::SUCCESS;
    }
    if converted.path != *path && std::path::Path::new(&converted.path).exists() {
        eprintln!("`{}` already exists; not overwriting it.", converted.path);
        return ExitCode::from(1);
    }
    if let Err(code) = write_file(&converted.path, &converted.text) {
        return code;
    }
    if converted.path != *path {
        if let Err(e) = std::fs::remove_file(path) {
            eprintln!("wrote `{}` but cannot remove `{path}`: {e}", converted.path);
            return ExitCode::from(1);
        }
    }
    println!("{path} -> {}", converted.path);
    ExitCode::SUCCESS
}

fn explain(args: &[String]) -> ExitCode {
    let Some(code) = args.first() else {
        eprintln!("usage: kalam explain CODE");
        return ExitCode::from(2);
    };
    let code = code.to_ascii_uppercase();
    match catalog::lookup(&code) {
        Some(entry) => {
            let lang = message_lang();
            println!("{}  {}\n", entry.code, entry.title[lang.index()]);
            println!("{}", entry.message[lang.index()]);
            if entry.proposed {
                println!("\n(This code is proposed and not yet listed in spec §27.)");
            }
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("no explanation for `{code}` yet");
            ExitCode::from(1)
        }
    }
}
