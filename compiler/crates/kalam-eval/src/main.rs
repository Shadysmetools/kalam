//! `kalam-eval [--root DIR] [--target M1..M10|beta|1.0|mvp] [--cargo-test] [--out FILE]
//!             [--append --timestamp TS [--history FILE] [--git-sha SHA] [--outcome O] [--notes TEXT]]`
//!
//! Prints the JSON scorecard on stdout. With `--append`, also appends one line to
//! `eval/history.jsonl` (or `--history FILE`). The timestamp is always passed in, never read
//! from the clock, so a run is reproducible.
//!
//! `--target` is a milestone tier from `scorecard.json` -> `milestone_tiers.order` (`mvp` is
//! an alias of the M5 scorecard tier). Without it, the target is `eval/FREEZE.json`
//! `target_milestone`, or `M1` before the freeze.

use kalam_eval::history::{self, AppendArgs};
use kalam_eval::parity;
use kalam_eval::report::{self, Options};
use kalam_eval::scorecard::Scorecard;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str =
    "usage: kalam-eval [--root DIR] [--target M1..M10|beta|1.0|mvp] [--cargo-test] [--out FILE] \
[--append --timestamp TS [--history FILE] [--git-sha SHA] [--outcome OUTCOME] [--notes TEXT]]";

#[derive(Default)]
struct Args {
    root: Option<PathBuf>,
    target: Option<String>,
    cargo_test: bool,
    out: Option<PathBuf>,
    append: bool,
    timestamp: Option<String>,
    history: Option<PathBuf>,
    git_sha: Option<String>,
    outcome: Option<String>,
    notes: Option<String>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = argv.iter();
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
            "--out" => a.out = Some(val()?.into()),
            "--append" => a.append = true,
            "--timestamp" => a.timestamp = Some(val()?),
            "--history" => a.history = Some(val()?.into()),
            "--git-sha" => a.git_sha = Some(val()?),
            "--outcome" => a.outcome = Some(val()?),
            "--notes" => a.notes = Some(val()?),
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    if a.append && a.timestamp.is_none() {
        return Err("--append needs --timestamp <ISO-8601 UTC>".into());
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
    std::fs::read_to_string(root.join("eval/FREEZE.json"))
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

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
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
    let target = args.target.clone().unwrap_or_else(|| default_target(&root));
    let sc = match std::fs::read_to_string(root.join("eval/scorecard.json"))
        .map_err(|e| e.to_string())
        .and_then(|t| Scorecard::parse(&t))
    {
        Ok(sc) => sc,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    if sc.tiers.resolve(&target).is_none() {
        eprintln!(
            "--target must be one of {} (or mvp)",
            sc.tiers.order.join(", ")
        );
        return ExitCode::from(2);
    }
    let mut run = match report::run(
        &sc,
        &Options {
            root: root.clone(),
            tier: target,
            run_cargo_test: args.cargo_test,
            hook: parity::default_hook(),
        },
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    // The evaluated tree: HEAD plus any uncommitted changes (LOOP.md §8 `commit`, A12).
    let git_sha = args.git_sha.clone().or_else(|| git_head(&root));
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
