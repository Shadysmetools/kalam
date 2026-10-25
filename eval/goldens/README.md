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

## Catalog codes without goldens, and why

| Code | Why |
|---|---|
| K0126 | The type checker reports it, and `kalam check` does not run the type checker yet |
| K0128 | It can only occur in `ar` files: Arabic-Indic digits in other skins are SEC0122 |
| K0301, K1201 | They need two files of one package, and a golden is a single file |
| K0413, K0417 | `kalam check` does not report them yet for a plain program (K0413: `let Int = 3`; K0417: a field named `with`) |
| K0916, K0922 | `print(a, b)` and `f[T](x)` are not reported yet |
| SEC0108 | It needs a `names.toml` display glossary |
| SEC0122 | It cannot occur in an `ar` file, where Arabic-Indic digits are allowed |
