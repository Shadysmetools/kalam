//! `kalam`: the single Kalam binary (D10, spec §21.5, ARCHITECTURE.md §19).
//!
//! Exit codes: 0 success; 1 diagnostics, failed tests, or `main` returned `Err`; 2 usage
//! errors, commands that are not implemented yet, and programs `kalam run --interp` cannot
//! run yet; 101 a panic in `kalam run`; 70 an internal error of the interpreter.
//! The message language comes from `KALAM_LANG` (`en`, `ar` or `franco`; default `en`).
//!
//! Argument parsing is hand-written in the scaffold; `clap` replaces it in Phase 1.

use std::process::ExitCode;

use kalam_base::Skin;
use kalam_diagnostics::Lang;
use kalam_fmt::FmtConfig;
use kalam_skins::KeywordTable;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Commands that exist in the design but not yet in code, with the phase that adds them.
const PLANNED: &[(&str, &str)] = &[
    ("new", "Phase 1"),
    ("build", "Phase 1"),
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
     \x20 kalam check [--json] FILE           check a file (source text, lexing, parsing, names)\n\
     \x20 kalam check [--json] [--std DIR] PROJECT\n\
     \x20                                    check a package: every file under PROJECT/src as one\n\
     \x20                                    module graph, with the stdlib at DIR (default: the\n\
     \x20                                    repository stdlib/)\n\
     \x20 kalam fmt [--check|--stdout] FILE...  format files in canonical layout (in place)\n\
     \x20 kalam convert --to en|ar|franco [--write] FILE\n\
     \x20                                    print FILE in another skin (--write: rename it)\n\
     \x20 kalam run --interp FILE             run FILE's `main` on the reference interpreter\n\
     \x20                                    (prelude only in M2; `kalam run` on JS is Phase 1)\n\
     \x20 kalam explain CODE                  explain a diagnostic code (KALAM_LANG sets the language)\n\
     \x20 kalam explain --catalog [--json]    list every code (--json: all codes, en/ar/franco)\n\
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
        "run" => run(rest),
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

/// `kalam run --interp FILE`: the reference interpreter (ROADMAP M2). Program output goes
/// to stdout as it is printed; diagnostics, a panic message or an `Err` from `main` go to
/// stderr. Plain `kalam run` (the JS backend) is not implemented yet.
fn run(args: &[String]) -> ExitCode {
    let interp = args.iter().any(|a| a == "--interp");
    let paths: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if !interp {
        eprintln!("`kalam run` on the JS backend is designed but not implemented yet (Phase 1); use `kalam run --interp FILE`.");
        return ExitCode::from(2);
    }
    let [path] = paths.as_slice() else {
        eprintln!("usage: kalam run --interp FILE");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(path.as_str()) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read `{path}`: {e}");
            return ExitCode::from(1);
        }
    };
    // Buffered, and flushed whatever the outcome: output printed before a panic survives.
    let mut out = std::io::BufWriter::new(std::io::stdout());
    let outcome = match kalam_driver::run_interp(path, &bytes, &mut out) {
        Ok(outcome) => outcome,
        Err(_) => {
            eprintln!("`{path}` is not a Kalam file (expected .kl, .ar.kl or .franco.kl)");
            return ExitCode::from(2);
        }
    };
    let _ = std::io::Write::flush(&mut out);
    match &outcome {
        kalam_driver::RunOutcome::Rejected(checked) => {
            eprint!("{}", checked.render_human(message_lang()));
        }
        other => eprint!("{}", other.stderr_text(path)),
    }
    ExitCode::from(outcome.exit_code())
}

fn check(args: &[String]) -> ExitCode {
    let mut json = false;
    let mut std_dir: Option<&String> = None;
    let mut path: Option<&String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--std" => {
                i += 1;
                std_dir = args.get(i);
                if std_dir.is_none() {
                    eprintln!("--std needs a directory");
                    return ExitCode::from(2);
                }
            }
            a if !a.starts_with("--") && path.is_none() => path = Some(&args[i]),
            _ => {}
        }
        i += 1;
    }
    let Some(path) = path else {
        eprintln!("usage: kalam check [--json] FILE | kalam check [--json] [--std DIR] PROJECT");
        return ExitCode::from(2);
    };
    if std::path::Path::new(path).is_dir() {
        return check_dir(path, std_dir.map(String::as_str), json);
    }
    if std_dir.is_some() {
        eprintln!("--std applies to a PROJECT directory, not to a single file");
        return ExitCode::from(2);
    }
    // Bytes, not text: invalid UTF-8 is a finding (SEC0100), not an unreadable file.
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read `{path}`: {e}");
            return ExitCode::from(1);
        }
    };
    let checked = match kalam_driver::check_bytes(path, &bytes) {
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

/// The repository stdlib (`stdlib/` next to `compiler/`), the default for `--std`.
fn default_std_dir() -> String {
    format!("{}/../../../stdlib", env!("CARGO_MANIFEST_DIR"))
}

/// A path in the form `kalam_driver::check_project` expects: `/` separators and no
/// trailing `/` (its module paths come from `strip_prefix(dir)` and then `/`).
fn slash_path(p: &str) -> String {
    let p = p.replace('\\', "/");
    match p.trim_end_matches('/') {
        "" => p,
        t => t.to_string(),
    }
}

/// The real file system for `check_project`. `list` is recursive and returns
/// `{dir}/{relative path}` with `/`, built from the exact `dir` string it was given.
struct DiskVfs;

impl kalam_driver::Vfs for DiskVfs {
    fn read(&self, path: &str) -> Option<String> {
        // Project mode reads text: an invalid UTF-8 run reads as U+FFFD (the single-file
        // `kalam check FILE` reports it as SEC0100).
        std::fs::read(path)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    fn list(&self, dir: &str) -> Vec<String> {
        fn walk(dir: &str, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = format!("{dir}/{}", e.file_name().to_string_lossy());
                match e.file_type() {
                    Ok(t) if t.is_dir() => walk(&p, out),
                    Ok(_) => out.push(p),
                    Err(_) => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(dir.trim_end_matches('/'), &mut out);
        out.sort();
        out
    }
}

/// The package's short name (spec §13.1, §13.5): `[package] name` in `kalam.toml` without
/// its scope (`@team/notes` → `notes`), else the directory's name, else `app`.
fn package_name(root: &str) -> String {
    let manifest = std::fs::read_to_string(format!("{root}/kalam.toml")).unwrap_or_default();
    let mut in_package = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_package) else {
            continue;
        };
        if key.trim() == "name" {
            let value = value
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"');
            let short = value
                .rsplit('/')
                .next()
                .unwrap_or(value)
                .trim_start_matches('@');
            if !short.is_empty() {
                return short.to_string();
            }
        }
    }
    std::path::Path::new(root)
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "app".to_string())
}

/// `kalam check [--json] [--std DIR] PROJECT`: every Kalam file under `PROJECT/src` is one
/// module (spec §13.1), resolved as one package together with the stdlib modules under DIR
/// (`kalam_driver::check_project`), so a module-level finding such as K0301 (two skin files
/// for one module, spec §4.1) is reported. `--json` prints one array over every file; each
/// diagnostic names its `file`. A project without a source file, or a stdlib directory that
/// does not exist, is a usage error (exit 2), never an empty pass.
fn check_dir(root: &str, std_dir: Option<&str>, json: bool) -> ExitCode {
    let root = slash_path(root);
    let std_dir = std_dir.map_or_else(default_std_dir, str::to_string);
    if !std::path::Path::new(&std_dir).is_dir() {
        eprintln!("the stdlib directory `{std_dir}` does not exist (pass --std DIR)");
        return ExitCode::from(2);
    }
    let vfs = DiskVfs;
    let src = format!("{root}/src");
    if !kalam_driver::Vfs::list(&vfs, &src)
        .iter()
        .any(|p| Skin::from_path(p).is_some())
    {
        eprintln!("`{src}` holds no Kalam file (.kl, .ar.kl or .franco.kl)");
        return ExitCode::from(2);
    }
    let options = kalam_driver::ProjectOptions {
        package: package_name(&root),
        std_dir: Some(slash_path(&std_dir)),
    };
    let checked = kalam_driver::check_project(&vfs, &root, &options);
    if json {
        // one flat array: the inside of each file's array, joined
        let items: Vec<String> = checked
            .iter()
            .map(|c| c.render_json())
            .map(|a| a[1..a.len() - 1].to_string())
            .filter(|a| !a.is_empty())
            .collect();
        println!("[{}]", items.join(","));
    } else {
        for c in checked.iter().filter(|c| !c.diagnostics.is_empty()) {
            eprintln!("{}", c.render_human(message_lang()));
        }
    }
    if checked.iter().any(|c| c.has_errors()) {
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

/// `kalam explain CODE`, `kalam explain --catalog [--json]` (logic in kalam-driver).
fn explain(args: &[String]) -> ExitCode {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--catalog", "--json"] | ["--json", "--catalog"] => {
            println!("{}", kalam_driver::explain_catalog_json());
            ExitCode::SUCCESS
        }
        ["--catalog"] => {
            print!("{}", kalam_driver::explain_catalog(message_lang()));
            ExitCode::SUCCESS
        }
        [code] if !code.starts_with("--") => match kalam_driver::explain(code, message_lang()) {
            Some(page) => {
                print!("{page}");
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("no explanation for `{}` yet", code.to_ascii_uppercase());
                ExitCode::from(1)
            }
        },
        _ => {
            eprintln!("usage: kalam explain CODE | kalam explain --catalog [--json]");
            ExitCode::from(2)
        }
    }
}
