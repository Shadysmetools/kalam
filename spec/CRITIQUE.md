# Kalam v0.1: completeness critique

**Date:** 2026-10-24. **Reviewed:** `spec/KALAM-SPEC.md`, `spec/keywords.json`, `spec/llms.txt`, `compiler/ARCHITECTURE.md` (plus the scaffold layout), `eval/SCORECARD.md`, `eval/scorecard.json`, `eval/LOOP.md`, `eval/corpus/` (all 10 written tasks), `ROADMAP.md`, `README.md`, `DECISIONS.md`. Checked against research 02 (§6–§7) and 04 (§5 charter).

**Verdict.** The language design is coherent and well traced. The keyword tables in §7 match `keywords.json` exactly. The EBNF has 75 productions with no undefined or unused nonterminal. The corpus programs conform to the spec, apart from the gaps the corpus already logs. **The project is not ready to start the M1 build loop.** The blockers are infrastructure and process (§3, B1–B4), not language design. Six spec contradictions were small enough to fix directly (§2). The rest are listed by priority.

How the checks were run: scripts compared §7 with `keywords.json`, the §27 index with every code used in the repo, the Franco table under the §7.3 fold, and the EBNF for defined and used nonterminals. Every corpus `en.kalam` was read against the grammar and the rules, and the `ar`/`franco` conversions were spot-checked.

---

## 1. What is sound (no action)

- `keywords.json` ↔ §7.1/§7.2: 0 mismatches (spellings and every Franco variant).
- §7.4 rules 1–4 hold under the lexer's lookup keys (see C2 for the wording fix).
- §8.2: 75 productions, all defined, all reachable.
- D1–D17 are respected everywhere. No contradiction with a locked decision was found. D7 is resolved in spec §6 consistently with 02 §6.3/§7.5 and 04 U1/U5, with 04 winning where they conflict (LRM/RLM, Persian digits).
- Corpus T01, T02, T12, T14, T15, T32, T45, T48, T49 and T50 parse and type under the spec as written. Line numbers in the negative `expected.txt` files are correct (T48 line 12, T49 lines 9/12/15, T50 lines 5/7). No corpus identifier collides with a Franco reserved spelling, so "no backtick escapes needed" holds.

## 2. Fixed directly in this pass (logged in the spec's new "Errata log")

| ID | Where | Problem | Fix |
|---|---|---|---|
| C1 | spec §5.4 | Bullet 2 said other-skin reserved words are ordinary identifiers. Bullet 3 said an English keyword in an `ar`/`franco` file is K0201. ARCHITECTURE §5.4 flagged the clash | K0201 now fires only where the word would otherwise be a syntax error or an undefined name |
| C2 | spec §7.4 rule 1 | "Share a fold key" was undefined. Under the §7.3 Franco fold, `we`/`wi` (and) and `aw` (or) all fold to `w`, so the "0 conflicts" claim was false as written | Rule 1 now uses the lexer lookup key (exact for `en`/`franco`, `arabic_fold` for `ar`), and the ambiguity of the diagnostic fold is noted |
| C3 | spec §4.3 | Escaping on conversion checked only "a keyword or core name of the target skin". An English identifier `low`/`foot`/`wala`, or an Arabic identifier `اذا`/`فى` in an `.kl` file, would not be escaped and would silently become a keyword after conversion, which breaks G1 | Escaping now uses the target skin's full lookup (Franco variants, `arabic_fold`) |
| C4 | spec §4.4 | KCF kept "strings byte for byte", but §4.3 converts code inside `{…}` holes. `"{a and b}"` and `"{a و b}"` would hash differently, so P1 would fail | KCF serializes hole code as code, and only literal text byte for byte |
| C5 | spec §17.6, §20.2, §28.5 ×3, `llms.txt` | `regex"…{3,20}…"` and `.get("/notes/{id}", …)` are interpolation holes by §5.7 (corpus gaps G1/G2). The Arabic glossary view even showed the hole untranslated | Escaped as `\{…\}`, a note added in §20.2, and route syntax opened as Q15 |
| C6 | spec §27 / ARCHITECTURE | K0129 and K1504 were used by ARCHITECTURE and `catalog.rs` but missing from §27 | Adopted into §27. ARCHITECTURE §9.2, §12.2 and A5 updated |
| S1 | spec §16.1 | 04 S12 (MUST) "TLS verification is always on" was not in the spec | Added to the `Net` row |
| S2 | spec §24 | Traceability omitted 04 S15, P3, P4, P5, P8, P11, P12, P13 and G1–G4 | Rows added that map them to ROADMAP M7/M10/M0 (registry and process obligations, not language semantics) |
| R1 | ROADMAP header | Said `eval/SCORECARD.md` is "pending" | Now points to the existing draft |
| R2 | ROADMAP M0 | Crate names (`kalam-lex`, `kalam-parse`, `kalam-types`, `kalam-js`) and paths (`corpus/`, `spec-tests/`) contradicted ARCHITECTURE §3 and `eval/corpus/` | Aligned |
| R3 | README | The repository table omitted `compiler/`, `eval/` and this file | Rows added |

**Side effects to handle (protected paths, so not edited here):**
- `eval/corpus/INDEX.md` §8 records the old spec SHA-256 (`1c69d6…`). It is now stale.
- INDEX §6 gaps G1 and G2 are now resolved: the spec adopted the corpus's own escaped form (`\{id\}`, `\{3,20\}`).
- The reviewer should update both. `keywords.json` was **not** changed, so the `ar`/`franco` corpus files are still valid.

---

## 3. Remaining issues, prioritized

### P0: blockers for starting the M1 build loop

- **B1. No Rust toolchain, so the scaffold has never compiled.**
  - `cargo`/`rustc` are absent on this machine, and ARCHITECTURE §22 confirms `cargo check` was never run.
  - LOOP step 6 needs `cargo fmt`, `clippy` and `test`.
  - A12 needs a `rust-toolchain.toml`, which does not exist.
  - *Action:* install rustup, add `compiler/rust-toolchain.toml`, then run `cargo check --workspace && cargo test --workspace` and fix what fails before iteration 1.
- **B2. The loop has no M1 stop.**
  - DECISIONS D17 and ROADMAP §0.1 require a founder stop at **every** milestone (M1 … M10).
  - `SCORECARD.md` §6, `scorecard.json` (`milestones: [mvp, beta, 1.0]`) and LOOP §5 rule 1 only know the three scorecard tiers.
  - With target `mvp`, LOOP §4 will drive past M1 into S3/S4 work (the corpus tasks need the type checker and the JS backend) and never stop at the M1 exit.
  - *Action (founder plus reviewer, protected paths):*
    - add checkpoint milestones `M1`…`M4` to `scorecard.json`/`FREEZE.json`, each with the gate list from its ROADMAP exit criteria;
    - make LOOP §4 restrict slices to that checkpoint's stages (M1: S0, S1, S2, S10);
    - make LOOP §5 rule 1 fire on the checkpoint's exit list.
- **B3. M1's measurement system does not exist.** M1's hard gates cannot be measured, and by rule A2 "unmeasured = 0", until the following exist:
  - `eval/harness/` (S0, `kalam-eval`);
  - `eval/FREEZE.json`, `eval/MANIFEST.sha256`, `questions.jsonl` and `reviews.jsonl` (`history.jsonl` exists but is empty);
  - `conformance/` (§25), including `conformance/keywords/`, an M0 deliverable;
  - the Trojan Source suite (about 60 cases);
  - the Arabic/Franco lexing-hazard suite (G-SEC);
  - `eval/diagnostics/broken/` (30 or more seeded files ×3, for the D5 recovery metric at M1 exit);
  - the P4 program generator (10k programs per run, an M1 hard gate);
  - the `kalam-props` crate (named by `D1.property_tests`, absent from the workspace);
  - the sealed holdout store.

  M0 itself is also open: founder sign-off on F1–F8, D7 recorded, eval frozen.
- **B4. No parser-error diagnostic codes.** §27 has no code for "unexpected token / expected X", an unclosed bracket at EOF, a missing block after `:`, an unterminated `"""`, an unknown tag before `"…"`, or a `"""` used as a hole. The parser cannot emit a stable code, and D5 completeness, recovery precision and P6 at M1 all depend on these. *Action:* reserve a parse-error family (for example K0700–K0709) in §27.

### P1: spec ambiguities that the M1 lexer, parser and KCF will hit

1. **Core constructors after `.`.**
   - §5.3 rule 8 makes any reserved word an identifier after `.`, and ARCHITECTURE §4.3 implements that in the lexer.
   - So `.None`, `Option.None` and `Result.Ok(x)` carry `None`/`Ok` as **identifiers**. They are never translated (an Arabic file keeps `.None`, a mixed-script line), and the KCF of `.None` (identifier) differs from `.لا_شيء`. P1 breaks.
   - The spec itself uses `.None` (§5.8, DR-05, the §25 layout test).
   - *Decide:* either forbid dotted core constructors (a K0917-style machine fix to bare `None`), or keep core names as `CORE` tokens after `.`.
2. **Map-literal keys vs labels.** DR-34 point 4 and §8.4 item 4 say that in map literals "a bare `IDENT` followed by `:` becomes a label". But `map_lit` is `expr ":" expr`, and §9.3 treats keys as values. So is `{name: 1}` the string key `"name"` (the JS prior) or the value of the variable `name` (the Python prior)? This is an LLM trap either way. *Decide,* and add K09xx for the rejected reading.
3. **The LL(1) claim fails at keyword labels.**
   - §5.3 rule 8 allows `f(match: 1)`, `User(type: t)` and keyword record fields.
   - Recognizing a label that is a keyword token needs two tokens of lookahead (keyword, then `:`). Example: `f(if: 1)` against `f(if c: a else: b)`.
   - *Action:* state LL(2) at that point in DR-34 and §8.4, or forbid keyword labels.
4. **Pattern-path overlap.** In `prim_pat`, `type_path "." IDENT [(…)]` (a variant) and `type_path "(" … ")"` (a record) overlap. `a.B(x)` can be variant `B` of type `a`, or record `B` from module `a`. *Action:* one production `path [ "(" pat_fields ")" ]`, with name resolution deciding.
5. **Inline-`if` edges.**
   - §8.3 reads a `simple_line` and then requires `if_inline`'s `or_expr`. A then-branch that is a lambda, or a nested inline `if` without parentheses (`if a: if b: x else: y else: z`), is unspecified.
   - Whether a value-position block's *last* statement may itself be an `if_form`/`match_form` (nested value forms) is also unspecified.
6. **String-literal corners.**
   - Are holes allowed in a `STRING` pattern and in a `test "…"` name? Both should be forbidden.
   - Is a one-line `"""…"""` legal? T15 and §28.7 use it, but §5.7 describes only the multi-line form.
   - How do braces nest inside a hole (`"{ {k: 1}[k] }"`)?
   - Is `{` legal in tagged-literal text, such as CSS in `html"<style>…"`?
7. **Lexer word runs.** `LETTER_ARABIC` excludes tatweel, harakat and the Persian letters. The spec must say the word-run scanner still *includes* them, so that it reports SEC0106/SEC0107 on the whole word and then looks up the fixed spelling as a keyword (for example `متغیر` with Persian ی). Otherwise the result is K0105, or an "identifier" message for something the user meant as a keyword.

### P1: Arabic and Franco lexing hazards not handled

| # | Hazard | Example | Needed |
|---|---|---|---|
| H1 | A multi-word keyword split by a space | `و إلا`, `لا شيء`, `tool ma`, `le kol` | a targeted K0201-style diagnostic with a machine fix |
| H2 | Hamza on the line (ء) is not in `arabic_fold` | Egyptian spelling `لا_شئ` folds to `لا_شي` ≠ `لا_شيء` (None), so None typed the common way is "undefined" | fold ئ/ء pairs for keywords, or pick a None word without ء |
| H3 | `،` between digits | `f(1،5)` in an `ar` file silently passes **two** arguments, where the writer meant 1.5 | an error or lint when `،` sits between two digits with no space, with fixes `٫`/`.` or `، ` |
| H4 | Letter case of keywords | `Law`, `Walla`, `If` | state that matching is case-sensitive and add a did-you-mean diagnostic |
| H5 | Codes not assigned | `٫` with ASCII digits (`1٫5`), Arabic percent `٪` used as `%` | assign codes and fixes (the §25 numerals row already expects a rejection) |
| H6 | Attached `و` before a keyword | `وليس`, `وصح` | **fixed** in §5.4 |
| H7 | Franco reserved words that are common English identifiers. 02 §7.6 rule 3 ("avoid the top 2,000 English identifiers, checked in CI") was not adopted | canonical `law`, `hat`, `mesh`, `we`, `fi`, `ka`, `lama`, `nass`; variants `low`, `foot`, `fe`, `wi`, `ao`, `kamel`, `nafs` | add a Franco/English 2,000-word CI rule beside DR-36, and drop at least `low`, `foot` and `ao` (variants, so dropping them is cheap) |
| H8 | §7.4 rule 5 (near-collision in the same position) fails for Franco under its own fold, but only the Arabic `و`/`أو` pair is a recorded exception | `we`/`aw` (both infix); `mesh`/`sa7` and `mesh`/`mafeesh` (both operand-start); `wala`/`we`/`aw` (all after an operand) | record them as exceptions or change the spellings; add them to the F3 survey |
| H9 | `عدد` (Int) is among the most common Arabic identifier words ("count") but is not in DR-36's justification list | `let عدد = …` gives K0413 | add the justification (users write `العدد`), or reconsider |

### P2: charter deviations not recorded in spec §3

- **04 S4** says decoders run "no user hooks", but `@decode_with` runs a user validator (§9.4, §18.3). It is a reasonable, restricted exception, but it needs an X-row in §3.
- **04 U6** asks for a *warning* when an identifier equals another skin's keyword. The spec gives only H0202, an LSP hint. Record it as a deviation, or make it a W-code.

### P2: cross-document inconsistencies

- **DECISIONS.md is stale.** D7 still reads "Deferred", and the line "research/02, 04, 05, 06 — pending" is out of date. Only the founder may edit the file (LOOP §1), so this is an M0 action (F2).
- **ROADMAP M2** lists T14 among the "prelude-only" tasks that run on the reference interpreter. But T14 needs `String.to_int` (`std/string`) and `map_err` (`std/result`), which M2 provides only as signature stubs.
- **Corpus file names.** The corpus uses `en.kalam`/`ar.kalam`/`franco.kalam` and needs a staging step (E13) because the spec sets the skin by extension. Naming the files `main.kl`, `main.ar.kl` and `main.franco.kl` would remove the step and one source of harness bugs.
- **`keywords.json` has no version field,** although adding a variant is a minor version and removing one is an edition change (§7.3). Add `{"version": …}`, or a sibling file.
- **Unspecified output formats.** `Show` for `Float` and `Bool`, `json.encode` bytes, HTML entity choice, URL encoding set, the `HttpError` body, and the exit code of `kalam check` (corpus G3–G7, G11, G17) need working defaults before M3/M4, not M1.

### P3: notes

- §28.7 imports `Access` and never uses it. Unused-import behaviour (warning or none) is unspecified.
- A local `fn` cannot carry attributes, because the `stmt` production takes `fn_decl` without `{attribute}`. So `@sync` on a local callback is impossible.
- 02 §7.4 step 5 suggests folding Persian ی/ک into keywords. The spec instead rejects them with a fix, which is safer. Keep it, but see P1 item 7 for the message wording.

---

## 4. Suggested order before iteration 1

1. Founder M0: approve the spec, answer F1–F8, record D7 in `DECISIONS.md`, and decide B2 (checkpoint milestones).
2. Design team: decide P1 items 1–3 (dotted core constructors, map keys, keyword labels) and H1–H3, because they change the lexer, parser and KCF that M1 builds. Reserve the parse-error codes (B4).
3. Install Rust and get the scaffold compiling (B1).
4. Reviewer: update INDEX §6/§8 for this pass, freeze `eval-v0.1` with an M1 target, and build S0 (B3). Only then start `/loop`.
