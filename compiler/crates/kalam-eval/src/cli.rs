//! The `kalam` binary as a subprocess: `kalam check --json FILE` and `kalam run --interp
//! FILE` (the M2 reference interpreter, ROADMAP.md M2).
//!
//! The M2 checkpoints (`roadmap.M2.typecheck/interpreter/negative`) and the interpreter part
//! of conformance cases with `stdout` go through the user-facing binary, so they measure
//! exactly what a user runs. The binary is never built here: it is passed with `--kalam
//! PATH` (or `KALAM_BIN`). Without one, everything that needs it is `not_measurable`.
//!
//! Each call stages the source under its spec file name (`main.kl`, `main.ar.kl`,
//! `main.franco.kl`) in a fresh temporary directory, because the extension is the skin
//! declaration (spec §4.1, DR-18), and removes the directory afterwards.

use kalam_base::Skin;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One error-severity diagnostic from `kalam check --json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliDiag {
    pub code: String,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliCheck {
    pub exit: Option<i32>,
    /// Error-severity diagnostics only.
    pub errors: Vec<CliDiag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliRun {
    pub exit: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// What the probes ask of `kalam run --interp`: a prelude-only program and its output.
pub const INTERP_PROBE: (&str, &str) = (
    "fn main():\n    print(\"kalam interp probe\")\n",
    "kalam interp probe\n",
);

/// A program the type checker must reject with K0602 (spec §9.8: "`1 == 1.0` is K0602").
pub const TYPECK_PROBE: &str = "fn main():\n    let same = 1 == 1.0\n    print(\"{same}\")\n";
/// A program name resolution must reject with K0401 (spec §27: name not defined).
pub const RESOLVE_PROBE: &str = "fn main():\n    print(kalam_probe_undefined_name)\n";

pub struct KalamCli {
    pub exe: PathBuf,
}

/// The spec file name for a skin (`main.kl`, `main.ar.kl`, `main.franco.kl`).
pub fn spec_name(skin: Skin) -> String {
    format!("main{}", skin.extension())
}

fn temp_dir() -> std::io::Result<PathBuf> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!(
        "kalam-eval-cli-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d)?;
    Ok(d)
}

/// Parses the `kalam check --json` array (spec §21.4) into its error diagnostics.
pub fn parse_check_json(stdout: &str) -> Result<Vec<CliDiag>, String> {
    let v: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("`kalam check --json` printed no JSON array ({e})"))?;
    let arr = v
        .as_array()
        .ok_or("`kalam check --json` printed JSON that is not an array")?;
    Ok(arr
        .iter()
        .filter(|d| d["severity"] == "error")
        .map(|d| CliDiag {
            code: d["code"].as_str().unwrap_or("").to_string(),
            line: d["span"]["start"]["line"].as_u64().unwrap_or(0) as usize,
        })
        .collect())
}

impl KalamCli {
    /// `--kalam PATH`, else `KALAM_BIN`. `Err(why)` when neither names an existing file.
    pub fn locate(explicit: Option<&Path>) -> Result<KalamCli, String> {
        let exe = explicit
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("KALAM_BIN").map(PathBuf::from))
            .ok_or("no `kalam` binary was given (pass --kalam PATH or set KALAM_BIN; the harness never builds it)")?;
        if exe.is_file() {
            Ok(KalamCli { exe })
        } else {
            Err(format!(
                "the `kalam` binary {} does not exist",
                exe.display()
            ))
        }
    }

    fn run(
        &self,
        args: &[&str],
        skin: Skin,
        source: &[u8],
        stdin: Option<&[u8]>,
    ) -> Result<CliRun, String> {
        let dir = temp_dir().map_err(|e| format!("temp dir: {e}"))?;
        let name = spec_name(skin);
        let out = (|| {
            fs::write(dir.join(&name), source).map_err(|e| format!("staging {name}: {e}"))?;
            let mut cmd = Command::new(&self.exe);
            cmd.args(args)
                .arg(&name)
                .current_dir(&dir)
                .env("KALAM_LANG", "en")
                .stdin(if stdin.is_some() {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = cmd
                .spawn()
                .map_err(|e| format!("cannot start {}: {e}", self.exe.display()))?;
            if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
                let _ = pipe.write_all(input);
            }
            let o = child
                .wait_with_output()
                .map_err(|e| format!("{}: {e}", self.exe.display()))?;
            Ok(CliRun {
                exit: o.status.code(),
                stdout: String::from_utf8_lossy(&o.stdout).replace("\r\n", "\n"),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            })
        })();
        let _ = fs::remove_dir_all(&dir);
        out
    }

    /// `kalam check --json FILE`.
    pub fn check(&self, skin: Skin, source: &[u8]) -> Result<CliCheck, String> {
        let r = self.run(&["check", "--json"], skin, source, None)?;
        Ok(CliCheck {
            exit: r.exit,
            errors: parse_check_json(&r.stdout)?,
        })
    }

    /// `kalam run --interp FILE`, stdin from `stdin`.
    pub fn run_interp(
        &self,
        skin: Skin,
        source: &[u8],
        stdin: Option<&[u8]>,
    ) -> Result<CliRun, String> {
        self.run(&["run", "--interp"], skin, source, stdin)
    }

    /// `Ok` when `kalam check` runs the type checker ([`TYPECK_PROBE`] gives K0602).
    pub fn typeck_available(&self) -> Result<(), String> {
        let r = self.check(Skin::En, TYPECK_PROBE.as_bytes())?;
        if r.errors.iter().any(|d| d.code == "K0602") {
            Ok(())
        } else {
            Err(format!(
                "`kalam check` does not run the type checker yet: `1 == 1.0` (spec §9.8) gives {:?}, not K0602",
                r.errors.iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
            ))
        }
    }

    /// `Ok` when `kalam run --interp` runs a prelude-only program with the right output.
    pub fn interp_available(&self) -> Result<(), String> {
        let r = self.run_interp(Skin::En, INTERP_PROBE.0.as_bytes(), None)?;
        if r.exit == Some(0) && r.stdout == INTERP_PROBE.1 {
            Ok(())
        } else {
            let said: String = r
                .stderr
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(160)
                .collect();
            Err(format!(
                "`kalam run --interp` is not available yet: a prelude-only `print` program exited {:?} \
                 with stdout {:?} (stderr: {said:?})",
                r.exit, r.stdout
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_check_json_errors_only() {
        let j = r#"[{"code":"K0901","severity":"error","span":{"start":{"line":2,"col":1}}},
                    {"code":"W0114","severity":"warning","span":{"start":{"line":3,"col":1}}}]"#;
        assert_eq!(
            parse_check_json(j).unwrap(),
            vec![CliDiag {
                code: "K0901".into(),
                line: 2
            }]
        );
        assert_eq!(parse_check_json("[]").unwrap(), vec![]);
        assert!(parse_check_json("usage: kalam").is_err());
    }

    #[test]
    fn locate_needs_an_existing_binary() {
        let missing = Path::new("definitely/not/here/kalam.exe");
        assert!(KalamCli::locate(Some(missing))
            .err()
            .unwrap()
            .contains("does not exist"));
        assert_eq!(spec_name(Skin::Ar), "main.ar.kl");
    }
}
