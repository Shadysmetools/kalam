//! One pass over a corpus directory (the dev corpus, or the held-out corpus inside the
//! held-out runner): lex-level checks, the token round-trip probe, G-PAR P1-P3,
//! `D1.corpus_syntax_pass` and the formatter check of every file.

use crate::checks::{self, LexResult, RoundTrip};
use crate::corpus::{self, Case};
use crate::parity::{self, ParityCase, ParityReport, ParseHook};
use crate::suites::FmtTally;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub struct CorpusRun {
    pub cases: Vec<Case>,
    pub records: Vec<Value>,
    pub lex_pairs: usize,
    pub lex_pass: usize,
    /// Case-skin pairs with lex-stage (source-text security) expectations.
    pub sec_pairs: usize,
    pub sec_pass: usize,
    pub roundtrip: RoundTrip,
    pub parity: ParityReport,
    /// `D7.formatter_idempotence` over every file (files of a case that is meant to be
    /// rejected before parsing are excluded).
    pub fmt: FmtTally,
}

impl CorpusRun {
    pub fn tasks(&self) -> Vec<&str> {
        let mut t: Vec<&str> = self.cases.iter().map(|c| c.task.as_str()).collect();
        t.dedup();
        t
    }
}

pub fn run(corpus_dir: &Path, hook: &dyn ParseHook) -> Result<CorpusRun, String> {
    // Only task directories (`tNN-slug`): a held-out store may hold suites next to them.
    let cases: Vec<Case> = corpus::discover(corpus_dir)
        .map_err(|e| format!("{}: {e}", corpus_dir.display()))?
        .into_iter()
        .filter(|c| parity::task_number(&c.task).is_some())
        .collect();
    let mut r = CorpusRun {
        cases: Vec::new(),
        records: Vec::new(),
        lex_pairs: 0,
        lex_pass: 0,
        sec_pairs: 0,
        sec_pass: 0,
        roundtrip: RoundTrip::default(),
        parity: parity::measure(hook, &[]),
        fmt: FmtTally::default(),
    };
    let mut parity_cases = Vec::new();
    let format_ok = hook.probe().is_ok() && hook.probe_format().is_ok();
    for case in &cases {
        let mut sources = BTreeMap::new();
        let mut lexed: Vec<LexResult> = Vec::new();
        for f in &case.files {
            let text =
                fs::read_to_string(&f.path).map_err(|e| format!("{}: {e}", f.path.display()))?;
            let l = checks::lex_case(&text, f.skin, &f.staged, &case.expect);
            r.lex_pairs += 1;
            r.lex_pass += l.pass as usize;
            if l.lex_expectations > 0 {
                r.sec_pairs += 1;
                r.sec_pass += l.pass as usize;
            }
            lexed.push(l);
            sources.insert(f.skin, text);
        }
        // Outside P1-P3 only when it is meant to fail at lex level AND did not lex
        // cleanly in any skin (T50 main); a case that lexes still goes through parity.
        let lex_stage_rejection =
            lexed.iter().any(|l| l.lex_expectations > 0) && lexed.iter().all(|l| !l.clean);
        if format_ok {
            for (skin, text) in &sources {
                if lex_stage_rejection {
                    r.fmt.excluded += 1;
                } else {
                    let id = format!("{}/{}/{}", case.task, case.case, skin.id());
                    r.fmt.check(hook, &id, text, *skin);
                }
            }
        }
        parity_cases.push(ParityCase {
            task: case.task.clone(),
            case: case.case.clone(),
            sources: sources.clone(),
            lex_stage_rejection,
            lex_rejection_matched: lexed.len() == 3 && lexed.iter().all(|l| l.pass),
        });
        let rt = checks::round_trip(&sources);
        r.roundtrip.checks += rt.checks;
        r.roundtrip.failures.extend(
            rt.failures
                .iter()
                .map(|f| format!("{}/{}: {f}", case.task, case.case)),
        );
        r.records.push(checks::case_record(case, &lexed, &rt));
    }
    r.parity = parity::measure(hook, &parity_cases);
    r.cases = cases;
    Ok(r)
}
