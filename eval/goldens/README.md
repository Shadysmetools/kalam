# Diagnostic goldens

These goldens feed `D5.golden_coverage`. They are also half of the corpus for `D5.machine_fix_rate`. The runner is `compiler/crates/kalam-eval/src/goldens.rs`, and the rules are in [`../HISTORY-SCHEMA.md`](../HISTORY-SCHEMA.md) ("Goldens").

```text
<CODE>/en.kl      en.expected       rendered with KALAM_LANG=en
       ar.kl      ar.expected       rendered with KALAM_LANG=ar
       franco.kl  franco.expected   rendered with KALAM_LANG=franco
```

- **What is compared.** Each source is checked as `main.kl`, `main.ar.kl` or `main.franco.kl`. Its first error must be `<CODE>`. That diagnostic's rendering must equal the `.expected` file byte for byte, plus one final newline.
- **No bless mode.** The runner only compares. A new or changed golden is written and reviewed by hand (LOOP.md A3).
- **Byte-exact files.** `.gitattributes` keeps the bytes exact. Several sources contain raw hazards on purpose: a BOM, bidi controls, invisible characters.

## Provenance (M2 start, 2026-10-25)

65 codes have goldens in all three skins.

- **Sources.** Most sources come from the spec-derived seeded-error suite. A case is used when it exists in all three skins and its first error is the code; it is named `eval/suites/seeded-errors/<code>-*`. Every other source is a small English program converted with `kalam convert`, with the hazard inserted identically in each skin.
- **Renderings** come from `kalam check` at authoring time. Each one was checked against spec §21.4:
  - the first error is the code;
  - the header follows the `-- TITLE --- file  CODE` template;
  - it ends with a `kalam explain CODE` footer;
  - the English prose never says "illegal", "invalid", "fatal" or "bad" (rule 3);
  - in the `ar` and `franco` goldens, every `Try:` fix and every quoted Kalam code is in the file's skin (rule 1). Fixed-ASCII stdlib member names such as `.pow(` and `expect("…")` stay in ASCII (§7.2). The quotes of foreign syntax that a K09xx message explains, such as `else if (…) {` or `let mut`, were checked one by one and kept.

  A code that failed any of these checks got no golden.
- **Arabic excerpt.** The `ar` goldens record the excerpt as it is printed today, with no English-skin copy underneath. Spec §21.4 rule 1 asks for that copy on terminals without bidi support, and `kalam check` does not print it yet.

## Added in M2 iteration 3 (2026-10-25)

24 codes from name resolution, the type checker and the parser's module rules: K0413, K0416, K0417, K0602, K0612, K0650, K0730, K0801, K0802, K0803, K0922, K1001, K1002, K1003, K1004, K1101, K1103, K1104, K1106, K1202, K1203, K1301, K1302, K1303. That makes 89 of the 141 catalog codes.

- **Sources.** Each English program is written from the spec, mostly from the type checker's own spec tests (`compiler/crates/kalam-typeck/tests/source.rs`). The `ar` and `franco` sources are `kalam convert` output. K1202 and K1203 are parse errors, which `convert` refuses, so their sources convert the corrected program and then put `var` / `.*` back with the skin's spelling.
- **Renderings** come from `kalam check` and were reviewed by hand with the same checks as at M2 start, plus two more:
  - a help text that says "`kalam fix` does this" needs a fix with `applicability: machine` in the JSON;
  - in `ar` and `franco` renderings, a type in a message is checked like any other quoted code: a core type name (`Int`, `List`, `String`, ...) must be in the file's skin (spec §21.4 rule 1, D21).
- **Example choice.** K0420, K0602 and K0730 use record types (`Point`, `Size`), so the message names no core type. Their template is right; the core-name problem is the type printer's and is listed below for K0126, K0601 and K1601, which cannot avoid a core type.
- **Multi-edit fixes** print one `Try:` line per edit (as in the M2-start K0910, K0914 and K0923 goldens): K0416, K1104.

## Added in M2 iteration 4 (2026-10-25)

25 codes: K0126, K0420, K0600, K0601, K1110, K1204, K1401, K1402, K1501, K1503, K1601, K1602, K1603, K1604, K1605, K1610, K1612, K1613, K1614, K1615, K1616, K1617, K1620, K1621, K1631. K1001, K1301 and K1302 match again. That makes 114 of the 145 catalog codes with goldens; the catalog grew by K0804, K1108 and K1204 this iteration. At the end of the iteration 113 match (77.9 %), because K0405 is changing under a fix that is still being written (below).

- **Renderer changes** (`kalam_diagnostics::render_human`), so these older goldens were re-blessed after a line-by-line review:
  - **Fixes of several lines** are shown as the code they insert, indented four spaces under `Try:` (the spec §21.4 example), with blank lines at either end dropped and the common indentation removed. Before, one backtick span ran across lines. Re-blessed: K0925, K1001, K1301. K1001, K1301 and K1302 also gained their `Try:` lines because the type checker now attaches those fixes.
  - **Labels** (secondary spans) are printed in the excerpt. A label on exactly the primary span follows the carets (`^^^^^ expected `Int`, found `Float``). Any other label gets its own source line and a `-` underline. Line numbers share one right-aligned gutter. Re-blessed: K0303, K0405, SEC0104.
  - **Label language.** A label's text is written in English by the stage that found the problem. The renderer translates it through `kalam_diagnostics::labels`, keyed by the English text (gettext style), and spells keywords and core names in the file's skin. A label with no translation is shown in English messages only, never inside an Arabic or Franco one.
  - **One `Try:` per distinct text.** A fix that writes the same text in several places, such as a rename, shows it once.
- **Types in messages follow the file's skin** (spec §21.4 rule 1, D21). The type checker prints them with `Ty::display_in` and `kalam_diagnostics::code::TypeSpelling`, the same spellings `kalam convert` uses: the 11 core type names, `fn` and the Arabic comma. `List[Int]` is `قائمة[عدد]` in an `ar` file and `qayma[adad]` in a `franco` file. Names that are not core names (`BigInt`, `Secret`, `Sql`, `Net`, user types) stay as written. This unblocked K0126, K0601 and K1601.
- **Catalog wording fixed before blessing:**
  - K0601 and K0602 (en) say "a value of type `Int` with one of type `Float`", which removes "a `Int`". K0602 was re-blessed.
  - K0420's help no longer promises suggestions that are never attached.
  - K0600's help points at the type label and no longer pushes `.to_float()` for every mismatch.
  - K1503's help spells `Secret[{core_string}]` in the file's skin.
- **Sources** are English programs written from the type checker's and kalam-sinks' spec tests (`kalam-typeck/tests/sinks_caps.rs`, `kalam-effects/src/tests.rs`). The `ar` and `franco` sources are `kalam convert` output. Each rendering was reviewed by hand with the checks above. The checks were also scripted: the first error is the code; no rule-3 word appears in English prose; no English core type name appears inside backticks in an `ar` or `franco` rendering; any "`kalam fix` …" claim has a machine fix in the JSON.
- **K1204** is brand new (FOUNDER-QUEUE Q33, in progress in another slice this iteration). Its golden records today's text. Its labels are in the label table, so `ar`/`franco` show them.
- **K0405** has a rename fix from name resolution that is still being written this iteration (`total_2` in every skin). Its golden was not re-blessed for that change: it fails until the fix settles (the renaming convention per skin is still open in that slice). It was re-blessed in iteration 5 (below).

## Changed in M2 iteration 5 (2026-10-25)

- **K0405 re-blessed** (en, ar, franco). Name resolution's rename fix has settled, and each rendering gained one `Try:` line with the new name. The en and ar renderings show `total_2`. The franco rendering shows `el_total`, following the skin's value convention (spec §10.1, DR-28). The en and ar sources declare an ASCII `total`, so no Arabic article is added. The rest of each rendering is unchanged. It was reviewed line by line with the checks above:
  - the first error is K0405;
  - the header and footer follow the template;
  - the prose has no rule-3 word;
  - every `Try:` is in the file's skin;
  - the fix is `applicability: machine` in the JSON.
- **Each fix leaves a clean file.** `kalam fix` applies the fix in all three skins, and the result re-checks with no diagnostics. With K0405 matching, all 114 goldens match: 114 of the 145 catalog codes, 78.6 %.
- **The `kalam fix` acceptance rule** (FOUNDER-QUEUE Q42) is now tested over every golden source (`compiler/crates/kalam-driver/tests/fix_rule.rs`). A fixed golden re-checks with fewer errors and no new error code, and a second `kalam fix` run changes nothing.

## Changed in M2 iteration 6 (2026-10-25)

- **K0405 franco re-blessed** (one line). FOUNDER-QUEUE Q51: the rename fix now uses one suffix rule in every skin, so the franco `Try:` line reads `total_2`, not `el_total`, as en and ar already did. Nothing else in the rendering changed. The help line still describes the skins' naming convention for values; it is advice to the person, not the fix (`compiler/crates/kalam-driver/tests/rename_converts.rs` checks that fixing and converting commute).

## Changed in M2 iteration 6 review (2026-10-26)

- **K0260 added** (en, ar, franco). FOUNDER-QUEUE Q53: an unknown string tag (`sqll"…"`) is now its own code, K0260 "unknown string tag", instead of the generic parse error K0250. The English source is a small program written from the parser test (`compiler/crates/kalam-parser/tests/adversarial.rs`). `kalam convert` refuses a file with a parse error, so the clean program (with `sql"…"`) was converted, and then `sql` was changed to `sqll` in each skin, as was done for K1202 and K1203. Tags are ASCII and identical in every skin, so the `ar` and `franco` messages list the built-in tags in ASCII; the help's example `{kw_return} "done"` is in the file's skin (`أرجع`, `rag3`). There is no fix (a space after an unknown word usually leaves a parse error), and the help does not claim one.
- **K0126 re-blessed** (en, ar, franco). The message used to name the literal's type and then explain `Int`'s 64-bit range, which was wrong for a `Float` or `Decimal` literal ("doesn't fit in `Float`. For `Int`, the range is …"). The message now names only the literal's type. The type's range or the reason it does not fit is the label under the literal: `Int`'s range (new, translated in `kalam_diagnostics::labels`), "beyond the largest `Float`", "more than 34 significant digits" or "`Decimal` exponent out of range". The title is now "NUMBER OUT OF RANGE FOR ITS TYPE", because a `Decimal` literal can be out of range by being too precise or too small. The help no longer pushes `BigInt` alone. `compiler/crates/kalam-driver/tests/literal_range_wording.rs` checks the wording for all three types in every message language.
- **Both reviewed line by line** with the checks above: the first error is the code; the header and footer follow the template; the prose has no rule-3 word; in `ar` and `franco` every quoted Kalam code and every core type name is in the file's skin (`عدد`, `adad`; `BigInt` is not a core name); neither help text claims a `kalam fix`.
- **Count.** The catalog grew by K0260 to 146 codes, and 115 of them have goldens (78.8 %). All 115 match the current `kalam check` byte for byte.
- **Judge.** K0126/ar is a judged sample item, so its new rendering makes it stale (FOUNDER-QUEUE Q43), and K0260 is a new, unjudged code. `D5.judge_score` is not measurable until both are judged (`kalam-eval judge-stale` lists them).

## Catalog codes without goldens, and why

### Rendered text is wrong: not blessed

| Code | What is wrong in the rendering |
|---|---|
| K0405 | Not a wording problem: name resolution is adding a rename fix this iteration, so the blessed text (no `Try:`) no longer matches. Re-bless once the per-skin naming of that fix is settled |
| K0603 | The message does not name the missing trait (for `json.decode` into a type without `Decode`, it talks about `Ord`/`Show`), and the help always suggests `impl T as Show:` |
| K0916 | The help says "`kalam fix` does this for you", but the diagnostic has no fix (spec §21.3 lists one) |
| K1005 | Not reported: the parser rejects `case 2.5:` with K0250 "I expected a pattern here" (spec §11 rule 6 and §27 say K1005) |
| K1105 | `ar`/`franco`: the `detail` argument is English prose inside the translated sentence ("... معاملات الدالة: expected at most 1 argument(s).") |
| K1107 | `ar`/`franco`: the same problem as K1105. The `detail` text ("`name` returns `Int` here, but `String` in the trait") is English, and its types are printed in English skin (`kalam_typeck::conform` does not use `Ty::display_in` yet) |
| K1611 | The help says "`kalam fix` adds them" (spec §17.4: "fix-it adds quotes"), but kalam-sinks attaches no fix |
| K1641 | The help says "`kalam fix` inserts it", but the only fix is `applicability: suggestion`. Its `ar` edit also inserts `?` where the Arabic file spells it `؟` |

### Cannot be a three-skin golden today

| Code | Why |
|---|---|
| K0128 | It can only occur in `ar` files: Arabic-Indic digits in other skins are SEC0122 |
| SEC0122 | It cannot occur in an `ar` file, where Arabic-Indic digits are allowed |
| K0301, K1201 | They need two files of one package, and a golden is a single file |
| SEC0108 | It needs a `names.toml` display glossary |
| H0202, L0114, W0114, W0133, W0403, W0421, W0710, W0720, W1606, W1622 | The runner compares the file's first **error**, so a hint, lint or warning can never match (W0421, W1606 and W1622 are reported by `kalam check` now) |
| K0418 | Needs a second module (a private item of a user module; the stdlib stubs' private items are not reachable from one file) |
| K0804, K1108 | New this iteration and still being implemented in the type-checker slice; not authored yet |
| K1502, K1504, K1618, K1650, K1701, K1702 | Not reported by any compiler stage yet. K1504 and K1701 also need a package manifest, which a single-file golden does not have |
