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

## Catalog codes without goldens, and why

### Rendered text is wrong: not blessed

| Code | What is wrong in the rendering |
|---|---|
| K0126 | `ar`/`franco`: "النوع `Int`" / "no3 `Int`". The type checker prints types with English core names (`kalam_typeck::Ty::display`); rule 1 needs `عدد` / `adad`. The same sentence then says "`عدد`", so the message names one type two ways |
| K0420 | The help says "pick one of the names I suggest", but no suggestion is ever attached (no `Try:` line, no fix, for `xs.length()` or `p.z`) |
| K0600 | The message never names the expected and found types (the `expected`/`found` arguments exist, the template ignores them), and the help always suggests `.to_float()`, even for `let x: Int = "hi"`. Also FOUNDER-QUEUE Q31: `let x: Int = 7 / 2` reports K0601, not K0600 |
| K0601 | `en`: "I can't combine a `Int` with a `Float`" ("an `Int`"; K0602 has the same template problem). `ar`/`franco`: `Int`/`Float` in English skin (as K0126). Number types are always core names, so no example avoids it |
| K0603 | The message does not name the missing trait (for `json.decode` into a type without `Decode`, it talks about `Ord`/`Show`), and the help always suggests `impl T as Show:` |
| K0916 | The help says "`kalam fix` does this for you", but the diagnostic has no fix |
| K1005 | Not reported: the parser rejects `case 2.5:` with K0250 "I expected a pattern here" (spec §11 rule 6 and §27 say K1005) |
| K1105 | `ar`/`franco`: the detail is English inside the translated sentence ("... معاملات الدالة: expected at most 1 argument(s).") |
| K1110 | The help says "`kalam fix` inserts the copy", but the diagnostic has no fix |
| K1601 | `ar`/`franco`: "`String`" in English skin (as K0126); every K1601 is about a `String` |
| K1641 | The help says "`kalam fix` inserts it", but the only fix is `applicability: suggestion`, which `kalam fix` does not apply |

### Cannot be a three-skin golden today

| Code | Why |
|---|---|
| K0128 | It can only occur in `ar` files: Arabic-Indic digits in other skins are SEC0122 |
| SEC0122 | It cannot occur in an `ar` file, where Arabic-Indic digits are allowed |
| K0301, K1201 | They need two files of one package, and a golden is a single file |
| SEC0108 | It needs a `names.toml` display glossary |
| H0202, L0114, W0114, W0133, W0403, W0421, W0710, W0720, W1606, W1622 | The runner compares the file's first **error**, so a hint, lint or warning can never match. None of them is reported by `kalam check` yet either |
| K0418 | Needs a second module (a private item of a user module; the stdlib stubs' private items are not reachable from one file) |
| K1401, K1402, K1501–K1504, K1602–K1650 (except K1601, K1641), K1701, K1702 | `kalam check` does not run the effects, capability and sink checks on a single file yet |
