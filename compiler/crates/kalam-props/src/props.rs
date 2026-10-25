//! The property runner: every property, on `cases` generated inputs each, with a report.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;
use std::time::Instant;

use kalam_ast::{Ast, TriviaKind};
use kalam_base::{FileId, Skin};
use kalam_fmt::{normalized_shape, FmtConfig};
use kalam_syntax::kcf::{self, KcfHash};
use serde_json::{json, Value};

use crate::gen::Gen;
use crate::print::print;
use crate::rng::splitmix;
use crate::soup::soup;

/// What D1.property_tests needs per property (scorecard.json, M1 checklist item 12).
pub const M1_MIN_CASES: u64 = 10_000;

/// Default case count in debug builds.
pub const DEBUG_CASES: u64 = 300;

/// Default seed when `KALAM_PROPS_SEED` is unset ("KALAM" in ASCII).
pub const DEFAULT_SEED: u64 = 0x4B_414C_414D;

/// The properties, in report order: (name, gating, the G-PAR checks it runs on every
/// generated program, what it checks).
///
/// The parity list is what kalam-eval's `G-PAR.P4` counts (scorecard: ">= 10k generated
/// programs per run through P1-P3"), so it names only checks the property's code performs:
/// P1 = equal KCF across the three skin printouts (and the hash of it); P2 = the 6 ordered
/// pairs and the 3-hop cycles give equal KCF and byte-identical text back; P3 =
/// `print_Y(parse(print_Y(parse_X(s)))) == print_Y(parse_X(s))` for every ordered pair.
pub const PROPERTIES: &[(&str, bool, &[&str], &str)] = &[
    (
        "ast_print_parse_roundtrip",
        true,
        &[],
        "random AST printed in each skin (en, ar, franco) parses cleanly to the same AST \
         (kalam_fmt::normalized_shape)",
    ),
    (
        "fmt_idempotence",
        true,
        &[],
        "fmt succeeds without its fallback, fmt(fmt(x)) == fmt(x), and fmt keeps the AST and \
         the comments, in every skin",
    ),
    (
        "convert_roundtrip",
        true,
        &["P2"],
        "all 6 ordered skin pairs (X->Y->X) and all six 3-hop cycles come back byte-identical; \
         every intermediate parses to the same AST and comments; converting fmt output gives \
         formatted output (G2 fixpoint)",
    ),
    (
        "kcf_cross_skin_equality",
        true,
        &["P1", "P2"],
        "KCF bytes of the same program are equal across the three skin printouts and across \
         every conversion step (spec §4.4 G1); a program accepted in some skins but not \
         others fails (G-PAR scope rule), one rejected in all three is unsupported (not run)",
    ),
    (
        "kcf_hash_stability",
        true,
        &["P1"],
        "KCF hash == SHA-256(KCF bytes), skin inference agrees with the file's skin, re-parsing \
         gives the same hash, hashes are equal across skins and stable under re-formatting",
    ),
    (
        "source_fixpoint",
        true,
        &["P3"],
        "P3: for every ordered skin pair X->Y, converting the X printout to Y and then Y->Y          again gives byte-identical text (print_Y(parse(print_Y(parse_X(s)))) ==          print_Y(parse_X(s)))",
    ),
    (
        "kcf_hash_golden",
        true,
        &[],
        "pinned KCF hashes of fixed programs (any change to KCF needs a format version bump)",
    ),
    (
        "lexer_parser_never_panic",
        true,
        &[],
        "lex + parse never panic on hostile text (bidi, ZWJ, tatweel, BOM, NUL, CR, tabs, \
         deep brackets, unfinished strings) in any skin",
    ),
    (
        "cst_lossless",
        true,
        &[],
        "the CST of any input, valid or not, reproduces the input byte for byte",
    ),
    (
        "kcf_fmt_invariance",
        false,
        &[],
        "report only: kcf(fmt(x)) == kcf(x) for generated programs (known open: fmt's \
         blank-line, trailing-comma and one-line-block normalization changes KCF)",
    ),
];

/// The gating properties judged once per generated program. A run keeps drawing programs
/// (up to [`MAX_DRAW_FACTOR`] x `cases`) until each has run on at least `cases` of them:
/// those after the round trip skip programs that do not parse in all three skins.
pub const PER_PROGRAM: &[&str] = &[
    "ast_print_parse_roundtrip",
    "fmt_idempotence",
    "convert_roundtrip",
    "kcf_cross_skin_equality",
    "kcf_hash_stability",
    "source_fixpoint",
];

/// At most this many times `cases` programs are drawn to fill [`PER_PROGRAM`].
pub const MAX_DRAW_FACTOR: u64 = 4;

const MAX_EXAMPLES: usize = 3;
const MAX_TEXT: usize = 6000;

/// Run settings (environment: `KALAM_PROPS_CASES`, `KALAM_PROPS_SEED`, `KALAM_PROPS_CASE`,
/// `KALAM_PROPS_THREADS`).
#[derive(Clone, Debug)]
pub struct Settings {
    pub cases: u64,
    pub seed: u64,
    /// Run only this case index (to reproduce a failure).
    pub only_case: Option<u64>,
    pub threads: usize,
}

fn parse_u64(s: &str) -> Option<u64> {
    let s = s.trim().replace('_', "");
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

impl Settings {
    pub fn from_env() -> Settings {
        let var = |k: &str| std::env::var(k).ok().and_then(|v| parse_u64(&v));
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(16);
        Settings {
            // Debug builds (a plain workspace `cargo test`) run a smoke-sized suite; the
            // metric run is `cargo test -p kalam-props --release` (10 000 cases).
            cases: var("KALAM_PROPS_CASES").unwrap_or(if cfg!(debug_assertions) {
                DEBUG_CASES
            } else {
                M1_MIN_CASES
            }),
            seed: var("KALAM_PROPS_SEED").unwrap_or(DEFAULT_SEED),
            only_case: var("KALAM_PROPS_CASE"),
            threads: var("KALAM_PROPS_THREADS")
                .map(|t| t.max(1) as usize)
                .unwrap_or(threads),
        }
    }
}

/// One recorded failure.
#[derive(Clone, Debug)]
pub struct Failure {
    pub case: u64,
    pub case_seed: u64,
    pub skin: Option<Skin>,
    pub detail: String,
    pub input: String,
}

#[derive(Clone, Debug)]
pub struct PropStats {
    pub name: &'static str,
    pub gating: bool,
    /// The G-PAR checks (`P1`..`P3`) this property runs ([`PROPERTIES`]).
    pub parity: &'static [&'static str],
    pub description: &'static str,
    pub cases: u64,
    pub failures: u64,
    pub examples: Vec<Failure>,
    /// Summed time spent in this property's checks, across threads.
    pub busy_secs: f64,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub settings: Settings,
    /// Generated programs drawn (case indices `0..cases_drawn`).
    pub cases_drawn: u64,
    /// Drawn programs that no skin accepts: unsupported (G-PAR scope rule), judged only by
    /// `ast_print_parse_roundtrip`.
    pub unsupported: u64,
    /// `git rev-parse HEAD` of the tree the suite ran on (`None` when git is unavailable):
    /// kalam-eval accepts the report only for the commit it evaluates.
    pub commit: Option<String>,
    pub properties: Vec<PropStats>,
    pub elapsed_secs: f64,
}

impl Report {
    /// Failures of gating properties.
    pub fn failures(&self) -> u64 {
        self.properties
            .iter()
            .filter(|p| p.gating)
            .map(|p| p.failures)
            .sum()
    }

    /// Every gating property passed on at least [`M1_MIN_CASES`] cases (D1.property_tests).
    pub fn meets_m1(&self) -> bool {
        self.failures() == 0
            && self
                .properties
                .iter()
                .filter(|p| p.gating && p.name != "kcf_hash_golden")
                .all(|p| p.cases >= M1_MIN_CASES)
    }

    pub fn to_json(&self) -> Value {
        let props: Vec<Value> = self
            .properties
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "gating": p.gating,
                    "parity": p.parity,
                    "description": p.description,
                    "cases": p.cases,
                    "failures": p.failures,
                    "passed": p.failures == 0,
                    "busy_secs": (p.busy_secs * 10.0).round() / 10.0,
                    "examples": p.examples.iter().map(|f| json!({
                        "case": f.case,
                        "case_seed": format!("{:#x}", f.case_seed),
                        "skin": f.skin.map(|s| s.id()),
                        "detail": truncate(&f.detail),
                        "input": truncate(&f.input),
                        "reproduce": format!(
                            "KALAM_PROPS_SEED={:#x} KALAM_PROPS_CASE={} cargo test -p kalam-props --release",
                            self.settings.seed, f.case
                        ),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({
            "suite": "kalam-props",
            "metric": "D1.property_tests",
            "commit": self.commit,
            "version": env!("CARGO_PKG_VERSION"),
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "cases": self.settings.cases,
            "cases_drawn": self.cases_drawn,
            "unsupported": self.unsupported,
            "seed": self.settings.seed,
            "seed_hex": format!("{:#x}", self.settings.seed),
            "only_case": self.settings.only_case,
            "failures": self.failures(),
            "report_only_failures": self.properties.iter().filter(|p| !p.gating).map(|p| p.failures).sum::<u64>(),
            "min_cases_per_property": M1_MIN_CASES,
            "passed": self.failures() == 0,
            "meets_m1": self.meets_m1(),
            "elapsed_secs": (self.elapsed_secs * 10.0).round() / 10.0,
            "properties": props,
        })
    }

    /// One line per property.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "kalam-props: seed {:#x}, {} cases, {} gating failure(s), {:.1}s\n",
            self.settings.seed,
            self.settings.cases,
            self.failures(),
            self.elapsed_secs
        );
        for p in &self.properties {
            s.push_str(&format!(
                "  {:<28} {:>6} cases {:>5} failures {:>7.1}s{}\n",
                p.name,
                p.cases,
                p.failures,
                p.busy_secs,
                if p.gating { "" } else { "  (report only)" }
            ));
            for f in &p.examples {
                s.push_str(&format!(
                    "    case {} ({}): {}\n",
                    f.case,
                    f.skin.map(|s| s.id()).unwrap_or("-"),
                    first_lines(&f.detail, 6)
                ));
            }
        }
        s
    }
}

fn truncate(s: &str) -> String {
    if s.len() <= MAX_TEXT {
        return s.to_string();
    }
    let mut end = MAX_TEXT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[{} more bytes]", &s[..end], s.len() - end)
}

fn first_lines(s: &str, n: usize) -> String {
    s.lines().take(n).collect::<Vec<_>>().join(" | ")
}

// ----- helpers ----------------------------------------------------------------------------

type Outcome = Result<(), (Option<Skin>, String)>;

fn cfg() -> FmtConfig {
    FmtConfig::default()
}

fn parse(text: &str, skin: Skin) -> kalam_parser::ParseOutput {
    kalam_parser::parse_source(text, skin, FileId(0))
}

/// The AST if `text` lexes and parses without errors; otherwise the error codes with the
/// text they point at.
fn parse_clean(text: &str, skin: Skin) -> Result<Ast, String> {
    let out = parse(text, skin);
    let errs: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| {
            let r = d.span.start as usize..(d.span.end as usize).min(text.len());
            let at = text.get(r).unwrap_or("?");
            format!("{}@{}:{at:?}", d.code, d.span.start)
        })
        .collect();
    if errs.is_empty() {
        Ok(out.ast)
    } else {
        Err(format!("does not parse in {skin}: {}", errs.join(", ")))
    }
}

fn comments(ast: &Ast) -> Vec<String> {
    ast.trivia
        .iter()
        .filter(|t| t.kind != TriviaKind::BlankLine)
        .map(|t| t.text.clone())
        .collect()
}

/// Where two strings first differ, with some context.
fn diff(a: &str, b: &str) -> String {
    let at = a
        .char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map(|((i, _), _)| i)
        .unwrap_or(a.len().min(b.len()));
    let ctx = |s: &str| {
        let mut lo = at.saturating_sub(60);
        while !s.is_char_boundary(lo) {
            lo -= 1;
        }
        let mut hi = (at + 100).min(s.len());
        while !s.is_char_boundary(hi) {
            hi += 1;
        }
        s.get(lo..hi).unwrap_or("").to_string()
    };
    format!(
        "first difference at byte {at}\n  expected: …{}…\n  actual:   …{}…",
        ctx(a),
        ctx(b)
    )
}

fn kcf_in(text: &str, skin: Skin) -> Vec<u8> {
    kcf::serialize_in(&parse(text, skin).cst, skin)
}

/// The ordered pairs and both 3-hop cycles starting at `from`, as skin paths.
fn round_trip_paths(from: Skin) -> Vec<Vec<Skin>> {
    let others: Vec<Skin> = Skin::ALL.into_iter().filter(|s| *s != from).collect();
    vec![
        vec![from, others[0], from],
        vec![from, others[1], from],
        vec![from, others[0], others[1], from],
        vec![from, others[1], others[0], from],
    ]
}

fn convert_path(text: &str, path: &[Skin]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = text.to_string();
    for w in path.windows(2) {
        cur = kalam_fmt::convert(&cur, w[0], w[1], &cfg()).map_err(|d| {
            let codes: Vec<&str> = d.iter().map(|d| d.code).collect();
            format!("convert {}->{} refused: {codes:?}", w[0], w[1])
        })?;
        out.push(cur.clone());
    }
    Ok(out)
}

/// P3 for one ordered pair: `c = convert(text, from -> to)` must be a fixpoint of
/// `reprint` (normally `convert(_, to -> to)`).
fn fixpoint_check(
    text: &str,
    from: Skin,
    to: Skin,
    reprint: impl Fn(&str) -> Result<String, String>,
) -> Result<(), String> {
    let c = kalam_fmt::convert(text, from, to, &cfg())
        .map_err(|_| format!("convert {from}->{to} refused"))?;
    let again = reprint(&c)?;
    if again == c {
        Ok(())
    } else {
        Err(format!(
            "P3 {from}->{to}: re-printing the converted text in {to} changed it
{}",
            diff(&c, &again)
        ))
    }
}

/// The commit the suite runs on, for the report (`None` without git).
fn head_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|c| !c.is_empty())
}

fn path_name(path: &[Skin]) -> String {
    path.iter().map(|s| s.id()).collect::<Vec<_>>().join("->")
}

// ----- one generated program --------------------------------------------------------------

struct Program {
    expected: String,
    texts: Vec<(Skin, String)>,
}

impl Program {
    fn new(case_seed: u64) -> Program {
        let ast = Gen::new(case_seed).program();
        let expected = normalized_shape(&ast);
        let layout = splitmix(case_seed ^ 0x1A70);
        let texts = Skin::ALL
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                (
                    s,
                    print(&ast, s, layout, splitmix(case_seed ^ (0x5E11 + i as u64))),
                )
            })
            .collect();
        Program { expected, texts }
    }

    fn all_text(&self) -> String {
        let mut s = String::new();
        for (skin, t) in &self.texts {
            s.push_str(&format!("--- {skin}\n{t}"));
        }
        s
    }

    /// P1: print → parse is the identity on ASTs.
    fn roundtrip(&self) -> Outcome {
        for (skin, text) in &self.texts {
            let ast = parse_clean(text, *skin).map_err(|e| (Some(*skin), e))?;
            let got = normalized_shape(&ast);
            if got != self.expected {
                return Err((
                    Some(*skin),
                    format!(
                        "parsed AST differs from the generated AST\n{}",
                        diff(&self.expected, &got)
                    ),
                ));
            }
        }
        Ok(())
    }

    fn fmt_idempotence(&self) -> Outcome {
        for (skin, text) in &self.texts {
            let s = Some(*skin);
            let before = parse_clean(text, *skin).map_err(|e| (s, e))?;
            let f = kalam_fmt::format_strict(text, *skin, &cfg())
                .map_err(|e| (s, format!("fmt failed: {e}")))?;
            let f2 = kalam_fmt::format_strict(&f, *skin, &cfg())
                .map_err(|e| (s, format!("fmt(fmt) failed: {e}\n--- fmt\n{f}")))?;
            if f2 != f {
                return Err((
                    s,
                    format!("fmt is not idempotent\n{}\n--- fmt\n{f}", diff(&f, &f2)),
                ));
            }
            let after = parse_clean(&f, *skin).map_err(|e| (s, format!("fmt output {e}")))?;
            if normalized_shape(&after) != self.expected {
                return Err((
                    s,
                    format!(
                        "fmt changed the AST\n{}",
                        diff(&self.expected, &normalized_shape(&after))
                    ),
                ));
            }
            if comments(&after) != comments(&before) {
                return Err((s, "fmt changed the comments".into()));
            }
        }
        Ok(())
    }

    fn convert_roundtrip(&self, case: u64) -> Outcome {
        for (skin, text) in &self.texts {
            let s = Some(*skin);
            let before = parse_clean(text, *skin).map_err(|e| (s, e))?;
            // Byte identity (G2) holds from canonical spelling: `convert X->X` re-spells
            // Franco/Arabic variants, `,`/`،` and needless backticks and keeps the layout.
            let canon = kalam_fmt::convert(text, *skin, *skin, &cfg())
                .map_err(|_| (s, format!("convert {skin}->{skin} refused")))?;
            let a = parse_clean(&canon, *skin).map_err(|e| (s, format!("respelled: {e}")))?;
            if normalized_shape(&a) != self.expected || comments(&a) != comments(&before) {
                return Err((s, format!("respelling in {skin} changed the AST\n{canon}")));
            }
            let paths = round_trip_paths(*skin);
            for (k, path) in paths.iter().enumerate() {
                let path = path.clone();
                let name = path_name(&path);
                // Every step keeps the AST: from the respelled text on every path, from the
                // printed text (variant spellings) on one rotating path per case.
                let raw = if k == (case as usize) % paths.len() {
                    convert_path(text, &path).map_err(|e| (s, format!("{name}: {e}")))?
                } else {
                    Vec::new()
                };
                let steps = convert_path(&canon, &path).map_err(|e| (s, format!("{name}: {e}")))?;
                for (j, step) in steps.iter().chain(raw.iter()).enumerate() {
                    let i = j % steps.len();
                    let to = path[i + 1];
                    let a = parse_clean(step, to)
                        .map_err(|e| (s, format!("{name} step {i}: {e}\n--- step\n{step}")))?;
                    if normalized_shape(&a) != self.expected || comments(&a) != comments(&before) {
                        return Err((s, format!("{name} step {i} changed the AST\n{step}")));
                    }
                }
                let back = steps.last().map(String::as_str).unwrap_or("");
                if back != canon {
                    return Err((
                        s,
                        format!("{name} is not byte-identical\n{}", diff(&canon, back)),
                    ));
                }
            }
            // G2 from the formatted text, one rotating path per case.
            let f = kalam_fmt::format_strict(text, *skin, &cfg())
                .map_err(|e| (s, format!("fmt failed: {e}")))?;
            let path = &paths[(case as usize + 1) % paths.len()];
            let name = path_name(path);
            let steps = convert_path(&f, path).map_err(|e| (s, format!("fmt {name}: {e}")))?;
            for (i, step) in steps.iter().enumerate() {
                let to = path[i + 1];
                let g = kalam_fmt::format_strict(step, to, &cfg())
                    .map_err(|e| (s, format!("fmt {name} step {i}: fmt failed: {e}")))?;
                if &g != step {
                    return Err((
                        s,
                        format!(
                            "fmt {name} step {i}: converted formatted text is not formatted in {to}\n{}",
                            diff(step, &g)
                        ),
                    ));
                }
            }
            if steps.last() != Some(&f) {
                return Err((s, format!("fmt {name} is not byte-identical")));
            }
        }
        Ok(())
    }

    /// The G-PAR scope rule on one generated program: `None` when no skin accepts it
    /// (unsupported, not a parity case), `Some(Err)` when some skins accept it and others
    /// do not (a parity failure), `Some(Ok)` when every skin accepts it.
    fn skin_parse_parity(&self) -> Option<Outcome> {
        let verdicts: Vec<(Skin, Result<Ast, String>)> = self
            .texts
            .iter()
            .map(|(skin, text)| (*skin, parse_clean(text, *skin)))
            .collect();
        let accepted: Vec<&str> = verdicts
            .iter()
            .filter(|(_, v)| v.is_ok())
            .map(|(s, _)| s.id())
            .collect();
        if accepted.is_empty() {
            return None;
        }
        Some(match verdicts.iter().find(|(_, v)| v.is_err()) {
            None => Ok(()),
            Some((skin, Err(e))) => Err((
                Some(*skin),
                format!(
                    "accepted in {} but not in {skin} (a parity failure): {e}",
                    accepted.join(", ")
                ),
            )),
            Some((_, Ok(_))) => unreachable!("found an Err"),
        })
    }

    /// P3 over every ordered skin pair.
    fn source_fixpoint(&self) -> Outcome {
        for (skin, text) in &self.texts {
            for to in Skin::ALL.into_iter().filter(|t| t != skin) {
                fixpoint_check(text, *skin, to, |c| {
                    kalam_fmt::convert(c, to, to, &cfg())
                        .map_err(|_| format!("convert {to}->{to} refused"))
                })
                .map_err(|e| (Some(*skin), e))?;
            }
        }
        Ok(())
    }

    fn kcf_cross_skin(&self) -> Outcome {
        let (skin0, text0) = &self.texts[0];
        let k0 = kcf_in(text0, *skin0);
        for (skin, text) in &self.texts[1..] {
            let k = kcf_in(text, *skin);
            if k != k0 {
                return Err((
                    Some(*skin),
                    format!(
                        "KCF differs from the {skin0} printout\n{}",
                        diff(&String::from_utf8_lossy(&k0), &String::from_utf8_lossy(&k))
                    ),
                ));
            }
        }
        for (skin, text) in &self.texts {
            for to in Skin::ALL {
                if to == *skin {
                    continue;
                }
                let c = kalam_fmt::convert(text, *skin, to, &cfg())
                    .map_err(|_| (Some(*skin), format!("convert {skin}->{to} refused")))?;
                let k = kcf_in(&c, to);
                if k != k0 {
                    return Err((
                        Some(*skin),
                        format!(
                            "KCF of convert {skin}->{to} differs\n{}",
                            diff(&String::from_utf8_lossy(&k0), &String::from_utf8_lossy(&k))
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn hash_stability(&self) -> Outcome {
        let mut first: Option<KcfHash> = None;
        for (skin, text) in &self.texts {
            let s = Some(*skin);
            let cst = parse(text, *skin).cst;
            let bytes = kcf::serialize(&cst);
            let h = kcf::hash(&cst);
            if h != KcfHash::of(&bytes) {
                return Err((s, "kcf::hash != KcfHash::of(kcf::serialize)".into()));
            }
            if kcf::infer_skin(&cst) != *skin || bytes != kcf::serialize_in(&cst, *skin) {
                return Err((
                    s,
                    format!(
                        "skin inference says {} for a {skin} file",
                        kcf::infer_skin(&cst)
                    ),
                ));
            }
            let again = kcf::hash(&parse(text, *skin).cst);
            if again != h {
                return Err((s, "re-parsing gave another hash".into()));
            }
            if h.to_hex().len() != 64 {
                return Err((s, format!("hash hex is not 64 digits: {}", h.to_hex())));
            }
            match first {
                None => first = Some(h),
                Some(h0) if h0 != h => {
                    return Err((s, format!("hash {h} differs from the en printout's {h0}")));
                }
                _ => {}
            }
            let f = kalam_fmt::format_strict(text, *skin, &cfg())
                .map_err(|e| (s, format!("fmt failed: {e}")))?;
            let f2 = kalam_fmt::format_strict(&f, *skin, &cfg())
                .map_err(|e| (s, format!("fmt(fmt) failed: {e}")))?;
            if kcf::hash(&parse(&f, *skin).cst) != kcf::hash(&parse(&f2, *skin).cst) {
                return Err((s, "hash(fmt(fmt(x))) != hash(fmt(x))".into()));
            }
        }
        Ok(())
    }

    fn kcf_fmt_invariance(&self) -> Outcome {
        for (skin, text) in &self.texts {
            let s = Some(*skin);
            let f = kalam_fmt::format_strict(text, *skin, &cfg())
                .map_err(|e| (s, format!("fmt failed: {e}")))?;
            let (a, b) = (kcf_in(text, *skin), kcf_in(&f, *skin));
            if a != b {
                return Err((
                    s,
                    format!(
                        "kcf(fmt(x)) != kcf(x)\n{}",
                        diff(&String::from_utf8_lossy(&a), &String::from_utf8_lossy(&b))
                    ),
                ));
            }
        }
        Ok(())
    }
}

// ----- golden hashes ----------------------------------------------------------------------

/// The KCF format version the [`GOLDEN`] hashes were pinned for.
pub const GOLDEN_MAGIC: &[u8] = b"KCF2\n";

/// Fixed programs and their pinned KCF hashes (each also checked through conversion to
/// every skin). A KCF change must bump `kcf::MAGIC`; then re-pin these and
/// [`GOLDEN_MAGIC`] (the failure message prints the new hashes).
pub const GOLDEN: &[(&str, Skin, &str, &str)] = &[
    (
        "hello",
        Skin::En,
        "fn main():\n    print(\"hello\")\n",
        "d988871b9207c82f074b9d300985ba788951c362a8a4055c5c6fa0df92a14aeb",
    ),
    (
        "fizzbuzz",
        Skin::En,
        "fn main():\n    for n in 1..=100:\n        if n % 15 == 0:\n            print(\"FizzBuzz\")\n        else:\n            print(\"{n}\")\n",
        "22f7507af1790f551c57574ba8906636d980ee50e8c3be24655f91e259ae9158",
    ),
    (
        "shapes",
        Skin::En,
        "enum Shape:\n    Rect(w: Int, h: Int)\n    Empty\n\nfn area(s: Shape) -> Int:\n    return match s:\n        case .Rect(w, h): w * h\n        case .Empty: 0\n",
        "29af4c6929d11ce4c0f38af35de5a6237e25e6300580067ac64fcb561b387c33",
    ),
];

fn golden_check(name: &str, skin: Skin, src: &str, want: &str) -> Outcome {
    let s = Some(skin);
    if kcf::MAGIC != GOLDEN_MAGIC {
        return Err((
            s,
            format!(
                "KCF format is now {:?}, the golden hashes are for {:?}: re-pin them",
                String::from_utf8_lossy(kcf::MAGIC).trim(),
                String::from_utf8_lossy(GOLDEN_MAGIC).trim()
            ),
        ));
    }
    parse_clean(src, skin).map_err(|e| (s, format!("{name}: {e}")))?;
    let h = kcf::hash(&parse(src, skin).cst).to_hex();
    if h != want {
        return Err((
            s,
            format!("{name}: KCF hash {h}, pinned {want} (same format version: KCF changed without a version bump)"),
        ));
    }
    for to in Skin::ALL {
        let c = kalam_fmt::convert(src, skin, to, &cfg())
            .map_err(|_| (s, format!("{name}: convert to {to} refused")))?;
        let hc = kcf::hash(&parse(&c, to).cst).to_hex();
        if hc != want {
            return Err((s, format!("{name}: {to} conversion hashes to {hc}")));
        }
    }
    Ok(())
}

// ----- runner -----------------------------------------------------------------------------

thread_local! {
    static LAST_PANIC: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f`, turning a panic into a failure that names the panic.
fn guarded(f: impl FnOnce() -> Outcome) -> Outcome {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "non-string panic".into());
            let at = LAST_PANIC
                .with(|p| p.borrow_mut().take())
                .unwrap_or_default();
            Err((None, format!("PANIC: {msg} {at}")))
        }
    }
}

struct Acc {
    stats: Vec<PropStats>,
    /// Programs no skin accepts.
    unsupported: u64,
    /// When the check now being recorded started (the previous `record` or `restart`).
    mark: Instant,
}

impl Acc {
    fn new() -> Acc {
        Acc {
            stats: PROPERTIES
                .iter()
                .map(|(name, gating, parity, description)| PropStats {
                    name,
                    gating: *gating,
                    parity,
                    description,
                    cases: 0,
                    failures: 0,
                    examples: Vec::new(),
                    busy_secs: 0.0,
                })
                .collect(),
            unsupported: 0,
            mark: Instant::now(),
        }
    }

    fn record(&mut self, prop: usize, case: u64, case_seed: u64, r: Outcome, input: &str) {
        let now = Instant::now();
        let p = &mut self.stats[prop];
        p.busy_secs += now.duration_since(self.mark).as_secs_f64();
        self.mark = now;
        p.cases += 1;
        if let Err((skin, detail)) = r {
            p.failures += 1;
            if p.examples.len() < MAX_EXAMPLES {
                p.examples.push(Failure {
                    case,
                    case_seed,
                    skin,
                    detail,
                    input: input.to_string(),
                });
            }
        }
    }

    /// Starts timing the next check.
    fn restart(&mut self) {
        self.mark = Instant::now();
    }

    fn merge(&mut self, other: Acc) {
        self.unsupported += other.unsupported;
        for (a, b) in self.stats.iter_mut().zip(other.stats) {
            a.cases += b.cases;
            a.failures += b.failures;
            a.busy_secs += b.busy_secs;
            a.examples.extend(b.examples);
            a.examples.sort_by_key(|f| f.case);
            a.examples.truncate(MAX_EXAMPLES);
        }
    }
}

fn prop_index(name: &str) -> usize {
    PROPERTIES
        .iter()
        .position(|(n, _, _, _)| *n == name)
        .expect("known property")
}

/// The seed of case `case` in a run seeded with `seed`.
pub fn case_seed(seed: u64, case: u64) -> u64 {
    splitmix(seed ^ splitmix(case))
}

fn run_case(acc: &mut Acc, seed: u64, case: u64) {
    let cs = case_seed(seed, case);
    acc.restart();
    // Generated programs.
    let prog = match catch_unwind(AssertUnwindSafe(|| Program::new(cs))) {
        Ok(p) => p,
        Err(_) => {
            acc.record(
                prop_index("ast_print_parse_roundtrip"),
                case,
                cs,
                Err((None, "the generator or printer panicked".into())),
                "",
            );
            return;
        }
    };
    let text = prog.all_text();
    let p1 = guarded(|| prog.roundtrip());
    let parsed = p1.is_ok();
    acc.record(prop_index("ast_print_parse_roundtrip"), case, cs, p1, &text);
    if parsed {
        let r = guarded(|| prog.fmt_idempotence());
        acc.record(prop_index("fmt_idempotence"), case, cs, r, &text);
        let r = guarded(|| prog.convert_roundtrip(case));
        acc.record(prop_index("convert_roundtrip"), case, cs, r, &text);
        let r = guarded(|| prog.source_fixpoint());
        acc.record(prop_index("source_fixpoint"), case, cs, r, &text);
        let r = guarded(|| prog.kcf_cross_skin());
        acc.record(prop_index("kcf_cross_skin_equality"), case, cs, r, &text);
        let r = guarded(|| prog.hash_stability());
        acc.record(prop_index("kcf_hash_stability"), case, cs, r, &text);
        let r = guarded(|| prog.kcf_fmt_invariance());
        acc.record(prop_index("kcf_fmt_invariance"), case, cs, r, &text);
    } else {
        // P1 still judges a program the round trip rejected (G-PAR scope rule).
        let kcf = prop_index("kcf_cross_skin_equality");
        match catch_unwind(AssertUnwindSafe(|| prog.skin_parse_parity())).unwrap_or(None) {
            None => acc.unsupported += 1,
            Some(Ok(())) => {
                let r = guarded(|| prog.kcf_cross_skin());
                acc.record(kcf, case, cs, r, &text);
            }
            Some(Err(e)) => acc.record(kcf, case, cs, Err(e), &text),
        }
    }
    // Hostile text.
    let input = match catch_unwind(|| soup(cs)) {
        Ok(s) => s,
        Err(_) => return,
    };
    acc.restart();
    for skin in Skin::ALL {
        let r = guarded(|| {
            let lexed = kalam_lexer::lex(&input, skin, FileId(0));
            let out = kalam_parser::parse(&input, &lexed, skin, FileId(0));
            let _ = kcf::hash(&out.cst);
            Ok(())
        });
        let r = r.map_err(|(_, d)| (Some(skin), d));
        acc.record(prop_index("lexer_parser_never_panic"), case, cs, r, &input);
        let r = guarded(|| {
            let out = parse(&input, skin);
            let back = out.cst.text();
            if back == input {
                Ok(())
            } else {
                Err((
                    Some(skin),
                    format!("CST text differs from the input\n{}", diff(&input, &back)),
                ))
            }
        })
        .map_err(|(s, d)| (s.or(Some(skin)), d));
        acc.record(prop_index("cst_lossless"), case, cs, r, &input);
    }
}

/// Runs every property. Panics inside the compiler are caught and recorded as failures.
pub fn run(settings: &Settings) -> Report {
    let start = Instant::now();
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        let at = info
            .location()
            .map(|l| format!("at {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        LAST_PANIC.with(|p| *p.borrow_mut() = Some(at));
    }));

    let mut total = Acc::new();
    for (name, skin, src, want) in GOLDEN {
        let r = guarded(|| golden_check(name, *skin, src, want));
        total.record(prop_index("kcf_hash_golden"), 0, 0, r, src);
    }

    let mut drawn = 0;
    match settings.only_case {
        Some(c) => {
            total.merge(run_batch(settings, &[c]));
            drawn = 1;
        }
        None => {
            let limit = settings.cases.saturating_mul(MAX_DRAW_FACTOR);
            let mut want = settings.cases;
            while want > 0 {
                let next = drawn + want.min(limit - drawn);
                let batch: Vec<u64> = (drawn..next).collect();
                total.merge(run_batch(settings, &batch));
                drawn = next;
                // top up: draw about as many programs again as the thinnest property lacks,
                // scaled by the share of programs it actually ran on
                let thinnest = total
                    .stats
                    .iter()
                    .filter(|p| PER_PROGRAM.contains(&p.name))
                    .map(|p| p.cases)
                    .min()
                    .unwrap_or(settings.cases);
                want = top_up(settings.cases, drawn, thinnest, limit);
            }
        }
    }
    std::panic::set_hook(previous_hook);

    let mut settings = settings.clone();
    if let Some(_c) = settings.only_case {
        settings.cases = 1;
    }
    Report {
        settings,
        cases_drawn: drawn,
        unsupported: total.unsupported,
        commit: head_commit(),
        properties: total.stats,
        elapsed_secs: start.elapsed().as_secs_f64(),
    }
}

/// How many more programs to draw when `drawn` programs gave the thinnest per-program
/// property `thinnest` cases and each needs `target`: the deficit scaled by the observed
/// yield, never past `limit` drawn in all.
fn top_up(target: u64, drawn: u64, thinnest: u64, limit: u64) -> u64 {
    let missing = target.saturating_sub(thinnest);
    if missing == 0 || drawn >= limit {
        return 0;
    }
    (missing.saturating_mul(drawn))
        .div_ceil(thinnest.max(1))
        .min(limit - drawn)
}

/// Runs the generated programs `cases` (and their hostile-text twins) on worker threads.
fn run_batch(settings: &Settings, cases: &[u64]) -> Acc {
    let threads = settings.threads.max(1).min(cases.len().max(1));
    let merged = Mutex::new(Acc::new());
    std::thread::scope(|scope| {
        for t in 0..threads {
            let cases = &cases;
            let merged = &merged;
            let seed = settings.seed;
            std::thread::Builder::new()
                .name(format!("kalam-props-{t}"))
                .stack_size(32 << 20)
                .spawn_scoped(scope, move || {
                    let mut acc = Acc::new();
                    for &case in cases.iter().skip(t).step_by(threads) {
                        run_case(&mut acc, seed, case);
                    }
                    merged.lock().unwrap_or_else(|e| e.into_inner()).merge(acc);
                })
                .expect("spawn a property thread");
        }
    });
    merged.into_inner().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_program_passes_every_property() {
        let p = Program::new(case_seed(DEFAULT_SEED, 3));
        assert_eq!(p.roundtrip(), Ok(()));
        assert_eq!(p.fmt_idempotence(), Ok(()));
        assert_eq!(p.kcf_cross_skin(), Ok(()));
    }

    #[test]
    fn the_roundtrip_property_detects_a_wrong_tree() {
        let mut p = Program::new(case_seed(DEFAULT_SEED, 3));
        p.expected.push_str(" (extra)");
        assert!(p.roundtrip().is_err());
        let mut p = Program::new(case_seed(DEFAULT_SEED, 4));
        p.texts[1].1.push_str("\n)\n");
        assert!(p.roundtrip().is_err());
    }

    #[test]
    fn a_panic_is_a_failure_not_a_crash() {
        let r = guarded(|| panic!("boom"));
        assert!(
            matches!(r, Err((None, ref d)) if d.contains("boom")),
            "{r:?}"
        );
    }

    #[test]
    fn the_report_names_its_commit_and_each_property_parity_checks() {
        let settings = Settings {
            cases: 2,
            seed: DEFAULT_SEED,
            only_case: None,
            threads: 1,
        };
        let v = run(&settings).to_json();
        // the commit ties the report to a tree (kalam-eval rejects a report without one)
        let head = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        assert_eq!(
            v["commit"].as_str().map(str::to_string),
            head,
            "{}",
            v["commit"]
        );
        // every property lists the G-PAR checks it runs; P1-P3 each have a gating property
        let props = v["properties"].as_array().unwrap();
        for p in props {
            assert!(p["parity"].is_array(), "{}", p["name"]);
        }
        for check in ["P1", "P2", "P3"] {
            assert!(
                props.iter().any(|p| p["gating"] == true
                    && p["parity"].as_array().unwrap().iter().any(|c| c == check)),
                "no gating property runs {check}"
            );
        }
        // lexer-only properties run no parity check
        let soup = props.iter().find(|p| p["name"] == "cst_lossless").unwrap();
        assert!(soup["parity"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_program_parsing_in_some_skins_only_is_a_parity_failure() {
        let p = Program::new(case_seed(DEFAULT_SEED, 3));
        assert_eq!(p.skin_parse_parity(), Some(Ok(())));
        // franco broken only: accepted in some skins, not others (G-PAR scope rule)
        let mut some = Program::new(case_seed(DEFAULT_SEED, 3));
        some.texts[2].1.push_str("\n)\n");
        match some.skin_parse_parity() {
            Some(Err((Some(Skin::Franco), d))) => assert!(d.contains("franco"), "{d}"),
            o => panic!("{o:?}"),
        }
        // rejected in every skin: unsupported, not a parity failure
        let mut none = Program::new(case_seed(DEFAULT_SEED, 3));
        for t in &mut none.texts {
            t.1.push_str("\n)\n");
        }
        assert_eq!(none.skin_parse_parity(), None);
    }

    #[test]
    fn every_per_program_property_gets_the_configured_case_count() {
        let settings = Settings {
            cases: 12,
            seed: DEFAULT_SEED,
            only_case: None,
            threads: 2,
        };
        let r = run(&settings);
        for p in r
            .properties
            .iter()
            .filter(|p| PER_PROGRAM.contains(&p.name))
        {
            assert!(p.cases >= 12, "{} ran {} cases", p.name, p.cases);
        }
        let v = r.to_json();
        assert!(v["cases_drawn"].as_u64().unwrap() >= 12, "{v}");
        assert!(v["unsupported"].is_u64(), "{v}");
        // a single-case reproduction draws exactly that case
        let one = run(&Settings {
            only_case: Some(5),
            ..settings
        });
        assert_eq!(one.to_json()["cases_drawn"], 1);
    }

    #[test]
    fn top_up_scales_the_deficit_by_the_yield_and_stops_at_the_cap() {
        assert_eq!(top_up(10_000, 10_000, 10_000, 40_000), 0);
        // 6716 of 10000 programs reached the thinnest property: draw ~4890 more
        assert_eq!(top_up(10_000, 10_000, 6_716, 40_000), 4_890);
        // nothing ran yet: draw up to the cap
        assert_eq!(top_up(10, 10, 0, 40), 30);
        assert_eq!(top_up(10, 40, 3, 40), 0);
    }

    #[test]
    fn source_fixpoint_holds_and_detects_a_non_fixpoint() {
        let p = Program::new(case_seed(DEFAULT_SEED, 3));
        assert_eq!(p.source_fixpoint(), Ok(()));
        assert!(
            fixpoint_check("fn main():\n    print(\"x\")\n", Skin::En, Skin::Ar, |c| {
                Ok(format!("{c}\n"))
            })
            .is_err()
        );
    }

    #[test]
    fn printouts_are_deterministic() {
        let a = Program::new(77);
        let b = Program::new(77);
        assert_eq!(a.texts, b.texts);
        assert_ne!(a.texts[0].1, a.texts[1].1);
    }
}
