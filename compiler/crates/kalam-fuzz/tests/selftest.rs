//! End-to-end checks of the supervisor: every kind of crash is caught, saved and confirmed
//! (`KALAM_FUZZ_SELFTEST=1` makes the worker fault on `#!selftest:` inputs), and the
//! crash-free clock resumes on the same binary and resets on a new one.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kalam-fuzz"))
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kalam-fuzz-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("seeds")).unwrap();
    d
}

fn fuzz(dir: &Path, extra: &[&str]) -> Value {
    let out = Command::new(exe())
        .arg("--corpus")
        .arg(dir.join("seeds"))
        .arg("--out")
        .arg(dir.join("out"))
        .args(["--generated", "0", "--jobs", "1", "--quiet"])
        .args(extra)
        .env("KALAM_FUZZ_SELFTEST", "1")
        .output()
        .expect("run kalam-fuzz");
    assert!(
        out.status.success(),
        "kalam-fuzz failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(dir.join("out/status.json")).expect("status.json");
    serde_json::from_str(&text).expect("valid JSON")
}

#[test]
fn panics_overflows_aborts_and_hangs_are_caught_saved_and_reproducible() {
    let dir = scratch("faults");
    for (name, body) in [
        ("panic", "#!selftest:panic"),
        ("overflow", "#!selftest:overflow"),
        ("abort", "#!selftest:abort"),
        ("hang", "#!selftest:hang"),
        ("fine", "fn main():\n    print(\"ok\")\n"),
    ] {
        std::fs::write(dir.join("seeds").join(format!("{name}.kl")), body).unwrap();
    }
    let st = fuzz(
        &dir,
        &["--no-mutate", "--timeout-ms", "3000", "--seconds", "120"],
    );
    assert_eq!(st["crashes"], 4, "{st:#}");
    assert_eq!(st["crash_free_hours"], 0.0);
    let list = st["crash_list"].as_array().unwrap();
    let mut kinds: Vec<&str> = list.iter().map(|c| c["kind"].as_str().unwrap()).collect();
    kinds.sort_unstable();
    assert_eq!(
        kinds,
        ["abort", "panic", "stack_overflow", "timeout"],
        "{st:#}"
    );
    for c in list {
        assert_eq!(c["reproducible"], true, "{c:#}");
        let file = dir.join("out").join(c["file"].as_str().unwrap());
        assert!(file.exists(), "{}", file.display());
    }
    let panic = list.iter().find(|c| c["kind"] == "panic").unwrap();
    assert!(
        panic["signature"]
            .as_str()
            .unwrap()
            .starts_with("panic@crates/kalam-fuzz/src/worker.rs:"),
        "{panic:#}"
    );

    // A second run re-checks the saved crashes first: they still crash on this binary.
    std::fs::remove_dir_all(dir.join("seeds")).unwrap();
    std::fs::create_dir_all(dir.join("seeds")).unwrap();
    std::fs::write(dir.join("seeds/fine.kl"), "fn main():\n    pass\n").unwrap();
    let st = fuzz(&dir, &["--no-mutate", "--timeout-ms", "3000"]);
    assert_eq!(st["replay_checked"], 4, "{st:#}");
    assert_eq!(st["replay_open"], 4, "{st:#}");
    assert_eq!(st["crash_free_hours"], 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A budgeted step that never ends (an interpreted infinite loop) retires the worker; it is
/// not a crash or a hang, and the next input runs on a fresh worker. A panic on a budgeted
/// thread is a panic located where it happened.
#[test]
fn an_over_budget_step_retires_the_worker_without_a_crash() {
    let dir = scratch("retire");
    for (name, body) in [
        ("a-spin", "#!selftest:spin"),
        ("b-fine", "fn main():\n    print(\"ok\")\n"),
        ("c-spin", "#!selftest:spin"),
        ("d-thread-panic", "#!selftest:thread-panic"),
        ("e-fine", "fn main():\n    print(1)\n"),
    ] {
        std::fs::write(dir.join("seeds").join(format!("{name}.kl")), body).unwrap();
    }
    let st = fuzz(
        &dir,
        &["--no-mutate", "--timeout-ms", "8000", "--seconds", "120"],
    );
    assert_eq!(st["total_execs"], 5, "{st:#}");
    assert_eq!(st["crashes"], 1, "{st:#}");
    let c = &st["crash_list"][0];
    assert_eq!(c["kind"], "panic", "{c:#}");
    assert!(
        c["signature"]
            .as_str()
            .unwrap()
            .starts_with("panic@crates/kalam-fuzz/src/worker.rs:"),
        "{c:#}"
    );
    assert!(st["targets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "typeck"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_clock_resumes_on_the_same_binary_and_resets_on_a_new_one() {
    let dir = scratch("clock");
    std::fs::write(dir.join("seeds/a.kl"), "fn main():\n    print(1)\n").unwrap();
    std::fs::write(dir.join("seeds/b.ar.kl"), "دالة رئيسي():\n    اطبع(١)\n").unwrap();
    let secs = |st: &Value| st["crash_free_secs"].as_f64().unwrap();

    let a = fuzz(&dir, &["--no-mutate"]);
    assert_eq!(a["crashes"], 0, "{a:#}");
    assert!(secs(&a) > 0.0, "{a:#}");
    assert_eq!(a["total_execs"], 2);

    let b = fuzz(&dir, &["--no-mutate"]);
    assert_eq!(b["version"], a["version"]);
    assert!(
        secs(&b) > secs(&a),
        "the clock must resume: {} then {}",
        secs(&a),
        secs(&b)
    );
    assert_eq!(b["total_execs"], 4);
    assert_eq!(b["runs"], 2);

    // Pretend the previous runs were on another binary.
    let path = dir.join("out/status.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["version"] = Value::from("0000-older-binary");
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

    let c = fuzz(&dir, &["--no-mutate"]);
    assert_eq!(c["previous_version"], "0000-older-binary");
    assert!(
        secs(&c) < secs(&b),
        "the clock must reset: {} then {}",
        secs(&b),
        secs(&c)
    );
    assert_eq!(c["version_execs"], 2);
    assert_eq!(c["total_execs"], 6);
    let _ = std::fs::remove_dir_all(&dir);
}
