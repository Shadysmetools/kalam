//! ```text
//! kalam-eval [--root DIR] [--target M1..M10|beta|1.0|mvp] [--cargo-test]
//!            [--run-props | --props-report FILE] [--out FILE] [--holdout DIR | --no-holdout]
//!            [--append --timestamp TS [--history FILE] [--git-sha SHA] [--outcome O] [--notes TEXT]]
//! kalam-eval freeze    [--root DIR] [--target TIER] [--timestamp TS] [--holdout DIR]
//! kalam-eval integrity [--root DIR] [--holdout DIR]
//! kalam-eval holdout   --root DIR --store DIR [--include-m2] [--kalam PATH]
//! kalam-eval clauses   [--root DIR] [--write | --check]
//! kalam-eval judge-sample [--root DIR] [--check]
//! kalam-eval judge-ingest [--root DIR] --in FILE [--merge]
//! kalam-eval judge-stale [--root DIR]
//! kalam-eval judge-calibration-kit [--root DIR]
//! kalam-eval judge-calibrate [--root DIR] --grader NAME CSV
//! ```
//!
//! `--kalam PATH` (or `KALAM_BIN`) names the `kalam` binary used by the M2 checkpoints and
//! by conformance cases that run the interpreter. The harness never builds it.
//!
//! `--props-report FILE` reads the kalam-props report from FILE instead of
//! `eval/props/report.json`, for a report written in a scratch target directory
//! (`KALAM_PROPS_REPORT=FILE cargo test -p kalam-props --release` with any
//! `CARGO_TARGET_DIR`). Its `commit` must still be the evaluated commit (HEAD); it excludes
//! `--run-props`.
//!
//! - `clauses` prints the spec clause index (`eval/conformance/CLAUSES.json`, src/clauses.rs);
//!   `--write` writes it, `--check` exits 1 when the committed file is stale.
//! - `judge-sample` writes the pinned judge sample `eval/judge/sample.json` (src/judge.rs);
//!   `--check` exits 1 when the committed sample is stale.
//! - `judge-ingest --in FILE` validates externally produced judge scores against the pinned
//!   sample and rubric and writes `eval/judge/report.json`. With `--merge`, FILE scores only
//!   the re-judged items (at least every stale and unjudged one) and the other rows are kept.
//! - `judge-stale` prints the report items whose rendered golden (or rubric) changed since
//!   judging, the sampled items the report does not score and the judged items no longer
//!   sampled (FOUNDER-QUEUE Q43); it exits 1 while any item is stale or unjudged.
//! - `judge-calibration-kit` writes `eval/judge/calibration-kit.csv`: 60 judged, non-stale
//!   sample items for a human to grade blind (empty `human_score` column).
//! - `judge-calibrate --grader NAME CSV` reads the graded kit (>= 50 grades) and writes
//!   `eval/judge/calibration.json`, printing the mean absolute error against the judge.
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
use kalam_eval::{clauses, goldens, holdout, integrity, judge, parity};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage:
  kalam-eval [--root DIR] [--target M1..M10|beta|1.0|mvp] [--cargo-test] [--run-props | --props-report FILE] [--out FILE] [--holdout DIR | --no-holdout]
             [--append --timestamp TS [--history FILE] [--git-sha SHA] [--outcome OUTCOME] [--notes TEXT]]
  kalam-eval freeze    [--root DIR] [--target TIER] [--timestamp TS] [--holdout DIR]
  kalam-eval integrity [--root DIR] [--holdout DIR]
  kalam-eval holdout   --root DIR --store DIR [--include-m2] [--kalam PATH]
  kalam-eval clauses   [--root DIR] [--write | --check]
  kalam-eval judge-sample [--root DIR] [--check]
  kalam-eval judge-ingest [--root DIR] --in FILE [--merge]
  kalam-eval judge-stale [--root DIR]
  kalam-eval judge-calibration-kit [--root DIR]
  kalam-eval judge-calibrate [--root DIR] --grader NAME CSV
  (scorecard runs also take --kalam PATH: the kalam binary for the M2 checkpoints)";

#[derive(Default)]
struct Args {
    command: Option<String>,
    root: Option<PathBuf>,
    target: Option<String>,
    cargo_test: bool,
    run_props: bool,
    props_report: Option<PathBuf>,
    include_m2: bool,
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
    kalam: Option<PathBuf>,
    write: bool,
    check: bool,
    input: Option<PathBuf>,
    merge: bool,
    grader: Option<String>,
    /// The positional CSV of `judge-calibrate`.
    csv: Option<PathBuf>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = argv.iter().peekable();
    if let Some(cmd) = it.peek().filter(|c| !c.starts_with('-')) {
        match cmd.as_str() {
            "freeze"
            | "integrity"
            | "holdout"
            | "clauses"
            | "judge-sample"
            | "judge-ingest"
            | "judge-stale"
            | "judge-calibration-kit"
            | "judge-calibrate" => a.command = Some(cmd.to_string()),
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
            "--props-report" => a.props_report = Some(val()?.into()),
            "--include-m2" => a.include_m2 = true,
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
            "--kalam" => a.kalam = Some(val()?.into()),
            "--write" => a.write = true,
            "--check" => a.check = true,
            "--in" => a.input = Some(val()?.into()),
            "--merge" => a.merge = true,
            "--grader" => a.grader = Some(val()?),
            "-h" | "--help" => return Err(USAGE.into()),
            csv if a.command.as_deref() == Some("judge-calibrate")
                && !csv.starts_with('-')
                && a.csv.is_none() =>
            {
                a.csv = Some(csv.into())
            }
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    if a.append && a.timestamp.is_none() {
        return Err("--append needs --timestamp <ISO-8601 UTC>".into());
    }
    if a.command.as_deref() == Some("judge-ingest") && a.input.is_none() {
        return Err("judge-ingest needs --in FILE".into());
    }
    if a.command.as_deref() == Some("judge-calibrate") && (a.csv.is_none() || a.grader.is_none()) {
        return Err(
            "judge-calibrate needs --grader NAME (the human who graded) and the graded CSV".into(),
        );
    }
    if a.merge && a.command.as_deref() != Some("judge-ingest") {
        return Err("--merge belongs to judge-ingest".into());
    }
    if a.run_props && a.props_report.is_some() {
        return Err("--run-props and --props-report exclude each other".into());
    }
    if a.write && a.check {
        return Err("--write and --check exclude each other".into());
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
fn holdout_child(root: &Path, store: &Path, include_m2: bool, kalam: Option<&Path>) -> ExitCode {
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
        holdout::aggregate(store, root, hook.as_ref(), &scope, include_m2, kalam)
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
        return holdout_child(root, store, args.include_m2, args.kalam.as_deref());
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
    match args.command.as_deref() {
        Some("clauses") => return clauses_cmd(&root, &args),
        Some("judge-sample") => return judge_sample_cmd(&root, &args),
        Some("judge-ingest") => return judge_ingest_cmd(&root, &args),
        Some("judge-stale") => return judge_stale_cmd(&root),
        Some("judge-calibration-kit") => return judge_kit_cmd(&root),
        Some("judge-calibrate") => return judge_calibrate_cmd(&root, &args),
        _ => {}
    }
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
        // `include_m2` is set by `report::run` from the resolved target tier
        (false, Ok(exe)) => Some(holdout::Runner {
            exe,
            store,
            include_m2: false,
            kalam: args.kalam.clone(),
        }),
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
            props_report: args.props_report.clone(),
            kalam: args.kalam.clone(),
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

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(1)
}

/// Writes `text` to `root/rel`, creating the parent directory.
fn write_rel(root: &Path, rel: &str, text: &str) -> Result<(), String> {
    let p = root.join(rel);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
}

/// `kalam-eval clauses [--write | --check]`.
fn clauses_cmd(root: &Path, args: &Args) -> ExitCode {
    let spec = match std::fs::read_to_string(root.join(clauses::SPEC_PATH)) {
        Ok(s) => s,
        Err(e) => return fail(format!("{}: {e}", clauses::SPEC_PATH)),
    };
    let units = clauses::index(&spec);
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&clauses::to_json(&units, &spec)).expect("JSON serializes")
    );
    if args.check {
        let committed = std::fs::read_to_string(root.join(clauses::CLAUSES_PATH)).ok();
        return match clauses::load_checked(committed.as_deref(), &spec) {
            Ok(u) => {
                eprintln!("{} is current ({} units)", clauses::CLAUSES_PATH, u.len());
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        };
    }
    if args.write {
        return match write_rel(root, clauses::CLAUSES_PATH, &text) {
            Ok(()) => {
                eprintln!(
                    "wrote {} ({} units, {} clauses)",
                    clauses::CLAUSES_PATH,
                    units.len(),
                    units.iter().map(|u| u.clauses.len()).sum::<usize>()
                );
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        };
    }
    print!("{text}");
    ExitCode::SUCCESS
}

fn current_sample(root: &Path) -> Result<serde_json::Value, String> {
    let goldens = goldens::load(&root.join(goldens::GOLDENS_DIR))?
        .ok_or_else(|| format!("{} does not exist", goldens::GOLDENS_DIR))?;
    let rubric = std::fs::read_to_string(root.join(judge::RUBRIC_PATH))
        .map_err(|e| format!("{}: {e}", judge::RUBRIC_PATH))?;
    judge::sample(&goldens, &rubric)
}

/// `kalam-eval judge-sample [--check]`.
fn judge_sample_cmd(root: &Path, args: &Args) -> ExitCode {
    let sample = match current_sample(root) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    if args.check {
        let committed = std::fs::read_to_string(root.join(judge::SAMPLE_PATH))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        return match committed {
            Some(c) if c == sample => {
                eprintln!("{} is current ({} items)", judge::SAMPLE_PATH, sample["n"]);
                ExitCode::SUCCESS
            }
            _ => fail(format!("{} is missing or stale", judge::SAMPLE_PATH)),
        };
    }
    // FOUNDER-QUEUE Q43: before the scored sample is replaced, the report's rows keep the
    // text they were judged on (`rendered_sha256`), so only changed items become stale.
    let old = std::fs::read_to_string(root.join(judge::SAMPLE_PATH))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let report = std::fs::read_to_string(root.join(judge::REPORT_PATH))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    if let (Some(old), Some(report)) = (old.filter(|o| *o != sample), report) {
        if let Some(pinned) = judge::pin_hashes(&report, &old) {
            if let Err(e) = write_rel(root, judge::REPORT_PATH, &json_text(&pinned)) {
                return fail(e);
            }
            eprintln!(
                "pinned the judged rendering of every row of {} (scores unchanged)",
                judge::REPORT_PATH
            );
        }
    }
    let text = json_text(&sample);
    match write_rel(root, judge::SAMPLE_PATH, &text) {
        Ok(()) => {
            eprintln!(
                "wrote {} ({} items, sample_sha256 {})",
                judge::SAMPLE_PATH,
                sample["n"],
                sample["sample_sha256"]
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

/// `kalam-eval judge-ingest --in FILE`.
fn judge_ingest_cmd(root: &Path, args: &Args) -> ExitCode {
    let Some(input) = &args.input else {
        return ExitCode::from(2);
    };
    let pinned = match std::fs::read_to_string(root.join(judge::SAMPLE_PATH))
        .map_err(|e| {
            format!(
                "{}: {e} (run `kalam-eval judge-sample` first)",
                judge::SAMPLE_PATH
            )
        })
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    match current_sample(root) {
        Ok(cur) if cur == pinned => {}
        Ok(_) => {
            return fail(format!(
                "{} is stale (the goldens or the rubric changed); re-run `kalam-eval judge-sample` and the judge",
                judge::SAMPLE_PATH
            ))
        }
        Err(e) => return fail(e),
    }
    let scores = match std::fs::read_to_string(input)
        .map_err(|e| format!("{}: {e}", input.display()))
        .and_then(|t| {
            serde_json::from_str::<serde_json::Value>(&t)
                .map_err(|e| format!("{}: {e}", input.display()))
        }) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let report = if args.merge {
        let existing = std::fs::read_to_string(root.join(judge::REPORT_PATH)).ok();
        match judge::assess(&pinned, None, existing.as_deref())
            .and_then(|(old, st)| judge::ingest_merge(&pinned, &old, &st, &scores))
        {
            Ok(r) => r,
            Err(e) => return fail(format!("rejected: {e}")),
        }
    } else {
        match judge::ingest(&pinned, &scores) {
            Ok(r) => r,
            Err(e) => return fail(format!("rejected: {e}")),
        }
    };
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&report).expect("JSON serializes")
    );
    match write_rel(root, judge::REPORT_PATH, &text) {
        Ok(()) => {
            eprintln!(
                "wrote {}: model {}, {} items, mean {}",
                judge::REPORT_PATH,
                report["model"],
                report["n"],
                report["mean"]
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn json_text(v: &serde_json::Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(v).expect("JSON serializes")
    )
}

/// The current sample, the committed report and their item-by-item comparison
/// (FOUNDER-QUEUE Q43).
fn judge_state(
    root: &Path,
) -> Result<(serde_json::Value, serde_json::Value, judge::Staleness), String> {
    let current = current_sample(root)?;
    let committed = std::fs::read_to_string(root.join(judge::SAMPLE_PATH)).ok();
    let report = std::fs::read_to_string(root.join(judge::REPORT_PATH)).ok();
    let (report, st) = judge::assess(&current, committed.as_deref(), report.as_deref())?;
    Ok((current, report, st))
}

/// `kalam-eval judge-stale`: the stale, unjudged and removed items as JSON; exit 1 while any
/// item is stale or unjudged.
fn judge_stale_cmd(root: &Path) -> ExitCode {
    let (_, _, st) = match judge_state(root) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    print!("{}", json_text(&st.to_json()));
    if st.is_clean() {
        eprintln!(
            "every one of the {} sampled items is judged on its current rendering",
            st.current.len()
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "{} stale and {} unjudged item(s): re-judge them, then `kalam-eval judge-ingest --merge --in FILE`",
            st.stale.len(),
            st.unjudged.len()
        );
        ExitCode::from(1)
    }
}

/// `kalam-eval judge-calibration-kit`: writes the blind grading kit.
fn judge_kit_cmd(root: &Path) -> ExitCode {
    let (current, _, st) = match judge_state(root) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    match judge::kit_csv(&current, &st) {
        Ok((csv, n)) => match write_rel(root, judge::KIT_PATH, &csv) {
            Ok(()) => {
                eprintln!(
                    "wrote {} ({n} items to grade with {}; fill `human_score` with 0-5, then \
                     `kalam-eval judge-calibrate --grader NAME {}`)",
                    judge::KIT_PATH,
                    judge::RUBRIC_PATH,
                    judge::KIT_PATH
                );
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        Err(e) => fail(e),
    }
}

/// `kalam-eval judge-calibrate --grader NAME CSV`: writes `eval/judge/calibration.json`.
fn judge_calibrate_cmd(root: &Path, args: &Args) -> ExitCode {
    let (Some(csv), Some(grader)) = (&args.csv, &args.grader) else {
        return ExitCode::from(2);
    };
    let (_, report, st) = match judge_state(root) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let text = match std::fs::read_to_string(csv) {
        Ok(t) => t,
        Err(e) => return fail(format!("{}: {e}", csv.display())),
    };
    let (grades, skipped) = match judge::read_grades(&text, &st) {
        Ok(g) => g,
        Err(e) => return fail(format!("rejected, nothing written: {e}")),
    };
    for s in &skipped {
        eprintln!("left out: {s}");
    }
    let (doc, mae) = match judge::calibration_doc(grader, &grades, &st, &report) {
        Ok(d) => d,
        Err(e) => return fail(format!("rejected, nothing written: {e}")),
    };
    if !st.is_clean() {
        eprintln!(
            "note: {} judge item(s) are stale or unjudged, so D5.judge_score stays not measurable until they are re-judged (`kalam-eval judge-stale`)",
            st.stale.len() + st.unjudged.len()
        );
    }
    if let Err(e) = write_rel(root, judge::CALIBRATION_PATH, &json_text(&doc)) {
        return fail(e);
    }
    eprintln!(
        "wrote {}: {} grades by {grader:?}, mean absolute error {mae:.3} against the judge ({})",
        judge::CALIBRATION_PATH,
        doc["n"],
        if mae <= judge::CALIBRATION_MAE {
            format!("<= {}: the judge is calibrated", judge::CALIBRATION_MAE)
        } else {
            format!("> {}: the judge is not trusted", judge::CALIBRATION_MAE)
        }
    );
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
        assert!(parse_args(&argv(&["clauses", "--write"])).unwrap().write);
        assert!(parse_args(&argv(&["clauses", "--write", "--check"])).is_err());
        assert!(parse_args(&argv(&["judge-ingest"])).is_err());
        let a = parse_args(&argv(&["judge-ingest", "--in", "s.json"])).unwrap();
        assert_eq!(a.command.as_deref(), Some("judge-ingest"));
        assert!(
            parse_args(&argv(&["judge-ingest", "--in", "s.json", "--merge"]))
                .unwrap()
                .merge
        );
        assert!(parse_args(&argv(&["judge-stale", "--merge"])).is_err());
        assert_eq!(
            parse_args(&argv(&["judge-stale"]))
                .unwrap()
                .command
                .as_deref(),
            Some("judge-stale")
        );
        assert!(parse_args(&argv(&["judge-calibration-kit"])).is_ok());
        // judge-calibrate takes the CSV as its one positional argument and a named grader
        let a = parse_args(&argv(&["judge-calibrate", "--grader", "Shady", "kit.csv"])).unwrap();
        assert_eq!(a.csv, Some(PathBuf::from("kit.csv")));
        assert_eq!(a.grader.as_deref(), Some("Shady"));
        assert!(parse_args(&argv(&["judge-calibrate", "kit.csv"])).is_err());
        assert!(parse_args(&argv(&["judge-calibrate", "--grader", "x"])).is_err());
        assert!(parse_args(&argv(&[
            "judge-calibrate",
            "--grader",
            "x",
            "a.csv",
            "b.csv"
        ]))
        .is_err());
        assert!(parse_args(&argv(&["judge-stale", "kit.csv"])).is_err());
        let a = parse_args(&argv(&["--kalam", "k.exe"])).unwrap();
        assert_eq!(a.kalam, Some(PathBuf::from("k.exe")));
        let a = parse_args(&argv(&["--props-report", "E:/tmp/r.json"])).unwrap();
        assert_eq!(a.props_report, Some(PathBuf::from("E:/tmp/r.json")));
        assert!(parse_args(&argv(&["--props-report", "r.json", "--run-props"])).is_err());
        assert!(parse_args(&argv(&["--props-report"])).is_err());
        let a = parse_args(&argv(&[
            "holdout",
            "--root",
            "r",
            "--store",
            "s",
            "--include-m2",
        ]))
        .unwrap();
        assert!(a.include_m2);
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
