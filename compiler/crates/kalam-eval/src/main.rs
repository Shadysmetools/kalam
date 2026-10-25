//! ```text
//! kalam-eval [--root DIR] [--target M1..M10|beta|1.0|mvp] [--cargo-test] [--run-props] [--out FILE]
//!            [--holdout DIR | --no-holdout]
//!            [--append --timestamp TS [--history FILE] [--git-sha SHA] [--outcome O] [--notes TEXT]]
//! kalam-eval freeze    [--root DIR] [--target TIER] [--timestamp TS] [--holdout DIR]
//! kalam-eval integrity [--root DIR] [--holdout DIR]
//! kalam-eval holdout   --root DIR --store DIR
//! ```
//!
//! The default command prints the JSON scorecard on stdout. With `--append`, it also appends
//! one line to `eval/history.jsonl` (or `--history FILE`). The timestamp is always passed in,
//! never read from the clock, so a run is reproducible.
//!
//! `--target` is a milestone tier from `scorecard.json` -> `milestone_tiers.order` (`mvp` is
//! an alias of the M5 scorecard tier). Without it, the target is `eval/FREEZE.json`
//! `target_milestone`, or `M1` before the freeze.
//!
//! - `freeze` writes `eval/FREEZE.json` and `eval/MANIFEST.sha256` (LOOP.md §2 step 4). Only
//!   the reviewer runs it; the fixer never repairs a manifest (LOOP.md §3 step 1).
//! - `integrity` runs G-INT alone and exits 1 when it fails.
//! - `holdout` is the sealed held-out runner. It prints one line of aggregate JSON and
//!   nothing else; the scorecard run starts it as a child process (`--holdout`, default
//!   `eval/holdout`).

use kalam_eval::history::{self, AppendArgs};
use kalam_eval::report::{self, Options};
use kalam_eval::scorecard::Scorecard;
use kalam_eval::{holdout, integrity, parity};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage:
  kalam-eval [--root DIR] [--target M1..M10|beta|1.0|mvp] [--cargo-test] [--run-props] [--out FILE] [--holdout DIR | --no-holdout]
             [--append --timestamp TS [--history FILE] [--git-sha SHA] [--outcome OUTCOME] [--notes TEXT]]
  kalam-eval freeze    [--root DIR] [--target TIER] [--timestamp TS] [--holdout DIR]
  kalam-eval integrity [--root DIR] [--holdout DIR]
  kalam-eval holdout   --root DIR --store DIR";

#[derive(Default)]
struct Args {
    command: Option<String>,
    root: Option<PathBuf>,
    target: Option<String>,
    cargo_test: bool,
    run_props: bool,
    out: Option<PathBuf>,
    append: bool,
    timestamp: Option<String>,
    history: Option<PathBuf>,
    git_sha: Option<String>,
    outcome: Option<String>,
    notes: Option<String>,
    holdout: Option<PathBuf>,
    no_holdout: bool,
    store: Option<PathBuf>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = argv.iter().peekable();
    if let Some(cmd) = it.peek().filter(|c| !c.starts_with('-')) {
        match cmd.as_str() {
            "freeze" | "integrity" | "holdout" => a.command = Some(cmd.to_string()),
            other => return Err(format!("unknown command `{other}`\n{USAGE}")),
        }
        it.next();
    }
    while let Some(flag) = it.next() {
        let mut val = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--root" => a.root = Some(val()?.into()),
            "--target" => a.target = Some(val()?),
            "--cargo-test" => a.cargo_test = true,
            "--run-props" => a.run_props = true,
            "--out" => a.out = Some(val()?.into()),
            "--append" => a.append = true,
            "--timestamp" => a.timestamp = Some(val()?),
            "--history" => a.history = Some(val()?.into()),
            "--git-sha" => a.git_sha = Some(val()?),
            "--outcome" => a.outcome = Some(val()?),
            "--notes" => a.notes = Some(val()?),
            "--holdout" => a.holdout = Some(val()?.into()),
            "--no-holdout" => a.no_holdout = true,
            "--store" => a.store = Some(val()?.into()),
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    if a.append && a.timestamp.is_none() {
        return Err("--append needs --timestamp <ISO-8601 UTC>".into());
    }
    if a.command.as_deref() == Some("holdout") && (a.root.is_none() || a.store.is_none()) {
        return Err("holdout needs --root and --store".into());
    }
    Ok(a)
}

/// Walks up from `start` to the directory that holds `eval/scorecard.json`.
fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|d| d.join("eval/scorecard.json").is_file())
        .map(Path::to_path_buf)
}

/// `eval/FREEZE.json` `target_milestone`, or `M1` (the first tier, LOOP.md) before the freeze.
fn default_target(root: &Path) -> String {
    std::fs::read_to_string(root.join(integrity::FREEZE))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["target_milestone"].as_str().map(str::to_string))
        .unwrap_or_else(|| "M1".to_string())
}

/// `git rev-parse HEAD` in `root`, or `None` (never invented).
fn git_head(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

/// `true` if `git status --porcelain` lists any change (the scores then describe HEAD plus
/// uncommitted edits, not HEAD), `None` if git is unavailable.
fn git_dirty(root: &Path) -> Option<bool> {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| !out.stdout.iter().all(u8::is_ascii_whitespace))
}

fn load_scorecard(root: &Path) -> Result<Scorecard, String> {
    std::fs::read_to_string(root.join("eval/scorecard.json"))
        .map_err(|e| e.to_string())
        .and_then(|t| Scorecard::parse(&t))
}

/// The sealed child: aggregate JSON on stdout, nothing on stderr, even on a panic.
fn holdout_child(root: &Path, store: &Path) -> ExitCode {
    std::panic::set_hook(Box::new(|_| {}));
    let Ok(sc) = load_scorecard(root) else {
        return ExitCode::from(1);
    };
    let scope = sc
        .tiers
        .get("M1")
        .and_then(|t| t.checks.iter().find(|c| c.id == "D1.corpus_syntax_pass"))
        .map(|c| parity::scope_tasks(&c.scope))
        .unwrap_or_default();
    let hook = parity::default_hook();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        holdout::aggregate(store, root, hook.as_ref(), &scope)
    })) {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(_) => ExitCode::from(1),
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if args.command.as_deref() == Some("holdout") {
        let (Some(root), Some(store)) = (&args.root, &args.store) else {
            return ExitCode::from(2);
        };
        return holdout_child(root, store);
    }
    let root = match args
        .root
        .clone()
        .or_else(|| std::env::current_dir().ok().and_then(|d| find_root(&d)))
    {
        Some(r) => r,
        None => {
            eprintln!(
                "could not find eval/scorecard.json above the current directory; pass --root"
            );
            return ExitCode::from(2);
        }
    };
    let sc = match load_scorecard(&root) {
        Ok(sc) => sc,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let target = args.target.clone().unwrap_or_else(|| default_target(&root));
    let store = args
        .holdout
        .clone()
        .unwrap_or_else(|| PathBuf::from(integrity::DEFAULT_HOLDOUT));

    match args.command.as_deref() {
        Some("freeze") => {
            let Some(tier) = sc.tiers.resolve(&target) else {
                eprintln!("--target must be one of {}", sc.tiers.order.join(", "));
                return ExitCode::from(2);
            };
            let o = integrity::FreezeOptions {
                eval_version: &sc.eval_version,
                target_milestone: &tier.id,
                timestamp: args.timestamp.as_deref(),
                holdout_store: &store,
            };
            return match integrity::freeze(&root, &o) {
                Ok(f) => {
                    eprintln!(
                        "wrote {} ({} frozen files) and {} ({} lines); the reviewer signs it in {}",
                        integrity::FREEZE,
                        f.freeze["frozen_files"].as_array().map_or(0, Vec::len),
                        integrity::MANIFEST,
                        f.manifest.lines().count(),
                        integrity::REVIEWS
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            };
        }
        Some("integrity") => {
            let r = integrity::run(
                &root,
                &integrity::CheckOptions {
                    eval_version: &sc.eval_version,
                    holdout_store: &store,
                    holdout_reported: None,
                },
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&r.to_json()).expect("JSON serializes")
            );
            return if r.state() == integrity::State::Fail {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            };
        }
        _ => {}
    }

    if sc.tiers.resolve(&target).is_none() {
        eprintln!(
            "--target must be one of {} (or mvp)",
            sc.tiers.order.join(", ")
        );
        return ExitCode::from(2);
    }
    // The evaluated tree: HEAD plus any uncommitted changes (LOOP.md §8 `commit`, A12).
    let git_sha = args.git_sha.clone().or_else(|| git_head(&root));
    let runner = match (args.no_holdout, std::env::current_exe()) {
        (false, Ok(exe)) => Some(holdout::Runner { exe, store }),
        _ => None,
    };
    let mut run = match report::run(
        &sc,
        &Options {
            root: root.clone(),
            tier: target,
            run_cargo_test: args.cargo_test,
            hook: parity::default_hook(),
            git_sha: git_sha.clone(),
            holdout: runner,
            run_props: args.run_props,
        },
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    run.json["git_sha"] = serde_json::json!(git_sha);
    run.json["git_dirty"] = serde_json::json!(git_dirty(&root));
    let text = serde_json::to_string_pretty(&run.json).expect("JSON serializes");
    if let Some(out) = &args.out {
        if let Err(e) = std::fs::write(out, format!("{text}\n")) {
            eprintln!("{}: {e}", out.display());
            return ExitCode::from(1);
        }
    }
    println!("{text}");

    if args.append {
        let path = args
            .history
            .clone()
            .unwrap_or_else(|| root.join("eval/history.jsonl"));
        let append_args = AppendArgs {
            timestamp: args.timestamp.as_deref().unwrap_or_default(),
            git_sha,
            outcome: args.outcome.as_deref(),
            notes: args.notes.as_deref(),
        };
        match history::append(&path, &run.json, &append_args) {
            Ok(rec) => eprintln!(
                "appended iteration {} to {}",
                rec["iteration"],
                path.display()
            ),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn append_requires_timestamp() {
        assert!(parse_args(&argv(&["--append"])).is_err());
        let a = parse_args(&argv(&[
            "--append",
            "--timestamp",
            "2026-10-25T00:00:00Z",
            "--notes",
            "n",
        ]))
        .unwrap();
        assert!(a.append);
        assert_eq!(a.notes.as_deref(), Some("n"));
        assert!(parse_args(&argv(&["--bogus"])).is_err());
        assert!(parse_args(&argv(&["--root"])).is_err());
    }

    #[test]
    fn subcommands() {
        let a = parse_args(&argv(&["freeze", "--timestamp", "t"])).unwrap();
        assert_eq!(a.command.as_deref(), Some("freeze"));
        assert_eq!(
            parse_args(&argv(&["integrity"]))
                .unwrap()
                .command
                .as_deref(),
            Some("integrity")
        );
        assert!(parse_args(&argv(&["holdout", "--root", "r"])).is_err());
        assert!(parse_args(&argv(&["holdout", "--root", "r", "--store", "s"])).is_ok());
        assert!(parse_args(&argv(&["unfreeze"])).is_err());
        assert!(parse_args(&argv(&["--no-holdout"])).unwrap().no_holdout);
    }

    #[test]
    fn default_target_is_the_frozen_tier_or_m1() {
        let dir = std::env::temp_dir().join(format!("kalam-eval-freeze-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("eval")).unwrap();
        assert_eq!(default_target(&dir), "M1");
        std::fs::write(
            dir.join("eval/FREEZE.json"),
            r#"{"target_milestone": "M3"}"#,
        )
        .unwrap();
        let t = default_target(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(t, "M3");
    }

    #[test]
    fn finds_root_from_a_nested_directory() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = find_root(here).expect("repo root");
        assert!(root.join("eval/scorecard.json").is_file());
    }
}
