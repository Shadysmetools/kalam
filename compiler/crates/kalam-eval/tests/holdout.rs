//! The sealed held-out runner, run as a child process on a synthetic store: it must print
//! aggregate counts only, never a task name, case id, file name or source text.

use kalam_eval::holdout::{self, Held, Runner};
use std::fs;
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kalam-eval-it-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

const SECRET_TASK: &str = "t05-zebra-secret-name";
const SECRET_TEXT: &str = "zebra-secret-greeting";

fn store() -> PathBuf {
    let s = temp_dir("store");
    let en = format!("fn main():\n    print(\"{SECRET_TEXT}\")\n");
    let ar = format!("دالة رئيسي():\n    اطبع(\"{SECRET_TEXT}\")\n");
    let fr = format!("dala ra2isi():\n    ektb(\"{SECRET_TEXT}\")\n");
    let t = format!("corpus/{SECRET_TASK}");
    write(&s, &format!("{t}/en.kalam"), &en);
    write(&s, &format!("{t}/ar.kalam"), &ar);
    write(&s, &format!("{t}/franco.kalam"), &fr);
    write(
        &s,
        &format!("{t}/expected.txt"),
        &format!("kind: run\n--- stdout\n{SECRET_TEXT}\n"),
    );
    write(
        &s,
        "suites/trojan-source/zebra-bad/expect.json",
        r#"{"must_reject": true, "codes": ["SEC0110"], "lines": [2]}"#,
    );
    write(
        &s,
        "suites/trojan-source/zebra-bad/main.kl",
        "fn main():\n    let s = \"a\u{202E}b\"\n    print(s)\n",
    );
    write(
        &s,
        "suites/trojan-source/zebra-good/expect.json",
        r#"{"must_reject": false}"#,
    );
    write(&s, "suites/trojan-source/zebra-good/main.kl", &en);
    s
}

#[test]
fn child_prints_aggregates_only() {
    let s = store();
    let runner = Runner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_kalam-eval")),
        store: s.clone(),
        include_m2: false,
    };
    let held = holdout::run_child(&runner, &repo_root());
    let text = held.to_json().to_string();
    let _ = fs::remove_dir_all(&s);
    let v = match &held {
        Held::Ran(v) => v.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(v["schema"], holdout::SCHEMA);
    assert_eq!(v["status"], "measured");
    for secret in [SECRET_TASK, SECRET_TEXT, "zebra", "main.kl", "t05"] {
        assert!(!text.contains(secret), "leaked {secret}: {text}");
    }
    // the one task is out of... no: T05 is in the M1 scope
    assert_eq!(v["m1_syntax"]["tasks_total"], 1, "{text}");
    assert_eq!(v["parity"]["P1"]["total"], 1, "{text}");
    assert_eq!(v["trojan"]["bad_total"], 1, "{text}");
    assert_eq!(v["trojan"]["good_total"], 1, "{text}");
    assert_eq!(v["trojan"]["bad_rejected"], 1, "{text}");
    assert_eq!(v["leak"]["hits"], 0, "{text}");
    assert!(v["manifest_sha256"].as_str().is_some_and(|h| h.len() == 64));
    assert!(v["diag_recovery"].is_null());
}

#[test]
fn missing_and_encrypted_stores_measure_nothing() {
    let runner = Runner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_kalam-eval")),
        store: PathBuf::from("/no/such/store"),
        include_m2: false,
    };
    assert!(matches!(
        holdout::run_child(&runner, &repo_root()),
        Held::Missing(_)
    ));
    let s = temp_dir("enc");
    write(&s, "store.age", "age-encryption.org/v1 ...");
    let runner = Runner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_kalam-eval")),
        store: s.clone(),
        include_m2: false,
    };
    let held = holdout::run_child(&runner, &repo_root());
    let _ = fs::remove_dir_all(&s);
    assert!(held.aggregate().is_none(), "{held:?}");
    assert!(held.why_not().contains("encrypted"), "{held:?}");
}

/// The held-out author's layout: tasks and suite splits side by side at the store root
/// (`S/tNN-slug/`, `S/trojan-source/<case>/`, `S/seeded-errors/<case>/`), plus non-task
/// directories. Suite cases must feed `trojan` and `diag_recovery` only, never the corpus
/// totals; `[M2]` cases are counted as deferred, never judged.
#[test]
fn flat_store_feeds_trojan_and_recovery_not_the_corpus() {
    let s = temp_dir("flat");
    let en = format!("fn main():\n    print(\"{SECRET_TEXT}\")\n");
    write(&s, &format!("{SECRET_TASK}/en.kalam"), &en);
    write(
        &s,
        &format!("{SECRET_TASK}/expected.txt"),
        &format!("kind: run\n--- stdout\n{SECRET_TEXT}\n"),
    );
    write(&s, "tools/zebra.kl", "fn main(:\n");
    let bad = "fn main():\n    let s = \"a\u{202E}b\"\n    print(s)\n";
    write(
        &s,
        "trojan-source/zebra-bad/expect.json",
        r#"{"must_reject": true, "codes": ["SEC0110"], "lines": [2]}"#,
    );
    write(&s, "trojan-source/zebra-bad/main.kl", bad);
    write(
        &s,
        "trojan-source/zebra-good/expect.json",
        r#"{"must_reject": false}"#,
    );
    write(&s, "trojan-source/zebra-good/main.kl", &en);
    write(
        &s,
        "trojan-source/zebra-m2/expect.json",
        r#"{"must_reject": true, "codes": ["SEC0104"], "notes": "[M2] zebra"}"#,
    );
    write(&s, "trojan-source/zebra-m2/main.kl", &en);
    let seeded = "fn main():\n    let x = 007\n    print(\"{x}\")\n";
    write(
        &s,
        "seeded-errors/zebra-zero/expect.json",
        r#"{"must_reject": true, "codes": ["K0125"], "lines": [2]}"#,
    );
    write(&s, "seeded-errors/zebra-zero/main.kl", seeded);
    write(&s, "seeded-errors/zebra-zero/other.kl", seeded);
    write(
        &s,
        "seeded-errors/zebra-m2/expect.json",
        r#"{"must_reject": true, "codes": ["K0401"], "lines": [2], "notes": "[M2] zebra"}"#,
    );
    write(&s, "seeded-errors/zebra-m2/main.kl", seeded);
    let runner = Runner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_kalam-eval")),
        store: s.clone(),
        include_m2: false,
    };
    let held = holdout::run_child(&runner, &repo_root());
    let text = held.to_json().to_string();
    let _ = fs::remove_dir_all(&s);
    let v = match &held {
        Held::Ran(v) => v.clone(),
        other => panic!("{other:?}"),
    };
    for secret in [SECRET_TASK, SECRET_TEXT, "zebra", "main.kl", "t05", "tools"] {
        assert!(!text.contains(secret), "leaked {secret}: {text}");
    }
    // only the task feeds the corpus side (one en file)
    assert_eq!(v["m1_syntax"]["tasks_total"], 1, "{text}");
    assert_eq!(v["parity"]["P1"]["total"], 1, "{text}");
    assert_eq!(v["fmt"]["total"], 1, "{text}");
    assert_eq!(v["trojan"]["bad_total"], 1, "{text}");
    assert_eq!(v["trojan"]["bad_rejected"], 1, "{text}");
    assert_eq!(v["trojan"]["good_total"], 1, "{text}");
    assert_eq!(v["trojan"]["deferred_m2"], 1, "{text}");
    assert_eq!(v["diag_recovery"]["files"], 2, "{text}");
    assert_eq!(v["diag_recovery"]["deferred_m2"], 1, "{text}");
    assert_eq!(v["diag_recovery"]["value"], 1.0, "{text}");
}

/// From tier M2 on (FOUNDER-QUEUE Q10) the parent asks the child to judge the `[M2]`
/// Trojan Source cases instead of deferring them.
#[test]
fn from_m2_the_child_judges_m2_trojan_cases() {
    let s = temp_dir("m2");
    let en = format!("fn main():\n    print(\"{SECRET_TEXT}\")\n");
    write(
        &s,
        "trojan-source/zebra-bad/expect.json",
        r#"{"must_reject": true, "codes": ["SEC0110"], "lines": [2]}"#,
    );
    write(
        &s,
        "trojan-source/zebra-bad/main.kl",
        "fn main():\n    let s = \"a\u{202E}b\"\n    print(s)\n",
    );
    write(
        &s,
        "trojan-source/zebra-m2/expect.json",
        r#"{"must_reject": true, "codes": ["SEC0104"], "notes": "[M2] zebra"}"#,
    );
    write(&s, "trojan-source/zebra-m2/main.kl", &en);
    let run = |include_m2: bool| {
        let runner = Runner {
            exe: PathBuf::from(env!("CARGO_BIN_EXE_kalam-eval")),
            store: s.clone(),
            include_m2,
        };
        match holdout::run_child(&runner, &repo_root()) {
            Held::Ran(v) => v,
            other => panic!("{other:?}"),
        }
    };
    let (m1, m2) = (run(false), run(true));
    let _ = fs::remove_dir_all(&s);
    assert_eq!(m1["trojan"]["deferred_m2"], 1, "{m1}");
    assert_eq!(m1["trojan"]["bad_total"], 1, "{m1}");
    assert_eq!(m2["trojan"]["deferred_m2"], 0, "{m2}");
    assert_eq!(m2["trojan"]["bad_total"], 2, "{m2}");
    // a clean file is not a SEC0104 collision: the [M2] case is judged, and fails
    assert_eq!(m2["trojan"]["bad_rejected"], 1, "{m2}");
    assert!(!m2.to_string().contains("zebra"), "{m2}");
}
