//! `kalam-fuzz`: see the crate documentation and `kalam-fuzz --help`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use kalam_base::Skin;
use kalam_fuzz::supervisor::{run, Options};
use kalam_fuzz::worker::{repro_main, worker_main};

const USAGE: &str = "\
kalam-fuzz: mutational fuzzer for the Kalam lexer, parser, KCF, fmt, convert, type checker
and reference interpreter (status.json `targets`)

USAGE:
    kalam-fuzz [--hours H | --minutes M | --seconds S] [OPTIONS]
    kalam-fuzz --repro FILE [--skin en|ar|franco]

OPTIONS:
    --hours H          fuzz for H hours (default 1)
    --minutes M        fuzz for M minutes
    --seconds S        fuzz for S seconds
    --jobs N           worker processes (default: a quarter of the cores)
    --seed N           mutation seed (default: the clock)
    --out DIR          output directory (default: eval/fuzz)
    --corpus DIR       seed directory, repeatable (default: eval/corpus and eval/suites)
    --timeout-ms N     a hang after N ms without a result (default 10000)
    --max-execs N      stop after N executions
    --generated N      generated seed programs (default 150)
    --oracle-examples N  inputs saved per oracle under OUT/oracle (default 3; all are counted)
    --no-replay        do not re-run saved crash inputs first
    --no-mutate        run the seeds once and stop
    --quiet            no progress output

The crash-free clock in OUT/status.json accumulates across runs only while the fuzz target
binary's SHA-256 is unchanged and no crash is open; it resets otherwise.
";

fn num<T: std::str::FromStr>(v: Option<&String>, flag: &str) -> Result<T, String> {
    v.and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("{flag} needs a number"))
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options::defaults();
    let mut corpus: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let v = args.get(i + 1);
        let mut takes = true;
        match a {
            "--hours" => o.duration = Duration::from_secs_f64(num::<f64>(v, a)? * 3600.0),
            "--minutes" => o.duration = Duration::from_secs_f64(num::<f64>(v, a)? * 60.0),
            "--seconds" => o.duration = Duration::from_secs_f64(num::<f64>(v, a)?),
            "--jobs" => o.jobs = num::<usize>(v, a)?.max(1),
            "--seed" => o.seed = num(v, a)?,
            "--out" => o.out = PathBuf::from(v.ok_or("--out needs a directory")?),
            "--corpus" => corpus.push(PathBuf::from(v.ok_or("--corpus needs a directory")?)),
            "--timeout-ms" => o.timeout = Duration::from_millis(num(v, a)?),
            "--max-execs" => o.max_execs = Some(num(v, a)?),
            "--generated" => o.generated = num(v, a)?,
            "--oracle-examples" => o.oracle_examples = num(v, a)?,
            "--no-replay" => {
                o.replay = false;
                takes = false;
            }
            "--no-mutate" => {
                o.mutate = false;
                takes = false;
            }
            "--quiet" => {
                o.quiet = true;
                takes = false;
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
        i += if takes { 2 } else { 1 };
    }
    if !corpus.is_empty() {
        o.corpus_dirs = corpus;
    }
    Ok(o)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--worker") => return worker_main(),
        Some("--repro") => {
            let Some(file) = args.get(1) else {
                eprintln!("--repro needs a file");
                return ExitCode::from(2);
            };
            let skin = match (args.get(2).map(String::as_str), args.get(3)) {
                (Some("--skin"), Some(s)) => Skin::ALL.into_iter().find(|k| k.id() == s),
                _ => None,
            };
            return repro_main(std::path::Path::new(file), skin);
        }
        Some("--help" | "-h" | "help") => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("kalam-fuzz: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(opts) {
        Ok(s) => {
            let st = &s.status;
            println!(
                "kalam-fuzz: {} execs in {:.0}s ({:.0}/s); crashes {}; crash-free {:.3} h; corpus {}; features {}",
                s.run_execs,
                s.run_secs,
                s.run_execs as f64 / s.run_secs.max(1e-9),
                st.crashes.len(),
                st.crash_free_secs / 3600.0,
                st.corpus_size,
                st.features,
            );
            for c in &st.crashes {
                println!(
                    "  crash {} x{} -> {} (reproducible: {:?})",
                    c.signature, c.count, c.file, c.reproducible
                );
            }
            for (name, o) in &st.oracle {
                println!("  oracle {name} x{}: {}", o.count, o.detail);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("kalam-fuzz: {e}");
            ExitCode::FAILURE
        }
    }
}
