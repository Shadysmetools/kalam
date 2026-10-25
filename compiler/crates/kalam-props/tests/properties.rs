//! D1.property_tests: every property on `KALAM_PROPS_CASES` (default 10 000) cases each,
//! with the report written to `eval/props/report.json`.

use std::path::PathBuf;

use kalam_props::{run, Settings};

fn report_path(settings: &Settings) -> Option<PathBuf> {
    match std::env::var("KALAM_PROPS_REPORT") {
        Ok(p) if p == "-" => None,
        Ok(p) => Some(PathBuf::from(p)),
        // A single-case reproduction, or a debug-build smoke run (a plain workspace
        // `cargo test`), never overwrites the suite's report.
        Err(_) if settings.only_case.is_some() || cfg!(debug_assertions) => None,
        Err(_) => {
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../eval/props/report.json"))
        }
    }
}

#[test]
fn property_suites_hold() {
    let settings = Settings::from_env();
    let report = run(&settings);
    if let Some(path) = report_path(&settings) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("create the report directory");
        }
        let text = serde_json::to_string_pretty(&report.to_json()).expect("serialize") + "\n";
        std::fs::write(&path, text).expect("write the report");
    }
    eprintln!("{}", report.summary());
    assert_eq!(report.failures(), 0, "\n{}", report.summary());
}
