# Eval suites

Case-per-directory test suites for the M1 checks that need a curated set of inputs. All cases
were written from the spec (`spec/KALAM-SPEC.md` §4–§8, §21, §27) and the research reports
(`research/04-security-by-design.md` §4.1–§4.2 and §5.2, `research/02-arabic-and-franco-languages.md`
§6–§7). They were **not** written from current compiler behaviour, so some cases fail today on
purpose. Where research and spec disagree, the spec wins (spec §3). DECISIONS.md wins over both
(for example D19: Persian/Urdu letters are folded, not rejected).

| Suite | Cases | Feeds | Main spec references |
|---|---|---|---|
| [`trojan-source/`](trojan-source) | 123 (10 marked `[M2]`) | `D2.trojan_source_suite` → G-SEC | §5.1, §5.2, §5.3, §5.4, §6.2, §6.3; 04 §4.1, U1–U5; 02 §6.2, §6.5, §7.5 |
| [`lexing-hazards/`](lexing-hazards) | 48 (1 `[M2]`) | `D2.lexing_hazard_suite` → G-SEC | §5.3, §5.4, §5.6, §6.2–§6.4, §7.1–§7.3, §21.4 rule 6; 02 §6.7, §7.4, §7.6 |
| [`seeded-errors/`](seeded-errors) | 45 (43 in all three skins; 131 files) | `D5.error_recovery_precision` | §5.6–§5.9, §6.4, §8.2–§8.4, §21.3, §27 |
| [`mixed-skin/`](mixed-skin) | 39 | `G-PAR.P8` (K0201 at M1) | §4.3, §5.3 rule 8, §5.4, §6.4, §21.4 rule 6 |
| [`conformance/`](conformance) | 36 | `D7.formatter_idempotence`, `kcf(fmt(x)) == kcf(x)` | §4.1, §4.4, §5.8, §8.2, §21.1 |

## Format (shared by every agent)

```
eval/suites/<suite>/<case-id>/
    case.kl | case.ar.kl | case.franco.kl     one or more source files; the extension is the skin (§4.1)
    expect.json
```

```json
{"must_reject": true, "codes": ["SEC0110"], "lines": [2], "notes": "why"}
```

- `codes`: diagnostic codes that must all be reported, in any order. Codes not listed are
  allowed: a case states what must appear, not everything that may appear.
- `lines` (optional): parallel to `codes`. `lines[i]` is the 1-based line where the **first**
  diagnostic with code `codes[i]` must point. Lines are counted by LF only, so a lone CR
  (SEC0113) does not start a new line. When `lines` is absent, the position is not checked.
  This happens for SEC0104, because it may point at either declaration.
- The expectation applies to **every** source file in the case directory. Multi-file cases hold
  the same program in several skins, with the error on the same line.

**Pass rule** (what a harness should implement):

| `must_reject` | The case passes when, for every source file |
|---|---|
| `true` | at least one error-severity diagnostic is reported (`K…` or `SEC…`), every code in `codes` is present, and each `lines[i]` matches |
| `false` | no error-severity diagnostic is reported (`W…`, `H…` and `L…` are allowed, for example W0114 or H0202). For `conformance/`, `kalam fmt` also succeeds, `fmt(fmt(x)) == fmt(x)`, and `kcf(fmt(x)) == kcf(x)` |

**`[M2]` marker.** A case whose `notes` start with `[M2]` needs name resolution, either for
SEC0104 skeleton collisions or for K0401 on an undefined name. The scorecard counts those from
tier M2 (SCORECARD.md §10.1 check 4: "the SEC0104 post-fold collision cases need name
resolution and count from M2"). An M1 harness should report them separately and leave them out
of the M1 pass/fail. Every other case is in M1 scope.

**Byte-exactness.** Sources are raw UTF-8 with no BOM (except the BOM case) and LF endings
(except the CRLF and lone-CR cases). Several of them contain raw bidi controls, invisible
characters and invalid bytes on purpose. A `.gitattributes` (`* -text`) in each of the five
suite directories stops git from rewriting them. Do not open and re-save these files in an editor. `notes` never contains raw hazardous
characters. It names them as `U+XXXX` instead.

## trojan-source

The source-text rules of §5.1, plus the identifier and digit rules that defend against
display spoofing:

- **Bidi** (SEC0110). Each of the 12 controls (U+202A–202E, U+2066–2069, LRM U+200E, RLM U+200F,
  ALM U+061C) is tested raw in a string and raw in a comment. Six are also tested between code
  tokens. Then come three attack shapes (early return, comment-out, a *paired* isolate in an
  Arabic string) and mirrors of corpus T50 (`t50-main`, `t50-good`, `t50-homoglyph`).
- **Invisible and format characters** (SEC0111): ZWSP, WJ, invisible times, soft hyphen, MVS, CGJ,
  the Hangul fillers (including the invisible-identifier backdoor), ZWNJ/ZWJ in strings, emoji ZWJ
  in a **comment** (the emoji exception covers strings only), and misplaced variation selectors.
- **SEC0112** (tag characters and ASCII smuggling, private use, noncharacters, code points
  unassigned in Unicode 16.0); **SEC0100** (BOM at offset 0 and mid-file, invalid and overlong
  UTF-8); **SEC0101** (not NFC, including decomposed hamza); **SEC0102** (C0/C1 controls);
  **SEC0103** (non-ASCII whitespace in code and comments); **SEC0113** (U+2028, U+2029, lone CR);
  **K0103** (tab).
- **Identifiers:** mixed script (SEC0105); tatweel, harakat, superscript alef, Quranic marks and
  presentation forms in identifiers **and keywords** (SEC0106).
- **Post-fold collisions** `[M2]` (SEC0104): ي/ى, ك/ک, ي/ی, ة/ه, ه/ە, أ/ا, Latin/Cyrillic and `l`/`1`.
  A use spelled with Persian letters for an Arabic declaration gives K0401 (FOUNDER-QUEUE Q7).
- **Digits:** Persian digits in every skin (SEC0121), mixed systems (SEC0120), Arabic-Indic digits
  in `en`/`franco` (SEC0122), U+066C (SEC0123) and Arabic-Indic digits inside identifiers (SEC0124).
- **Good cases** (`must_reject: false`), required by the metric ("every good case accepted"):
  escaped controls, ordinary Arabic text with harakat and tatweel in strings, an emoji ZWJ
  sequence and VS16 in a string, VS16 after an emoji in a comment, NBSP in a string (W0114 only),
  a tab in a comment, Arabic-Indic digits in an English string, and Persian letters in an
  identifier used with one spelling (D19: SEC0107 is retired).

The suite holds about twice the "~60" target in SCORECARD.md, because every control appears in
every position. The holdout author can take the required 30% from it.

## lexing-hazards

- **Franco digit-leading words** (K0127): `3ala`, `7aga`, and the `did_you_mean` spellings `7asab`,
  `2ktb`, `5alas`, `8alat`, `5ali`, whose message must suggest the canonical word. Positive
  cases cover canonical words with digits *inside* them (`rag3`, `sa7`, `naga7`), closed-list
  variants, and `_3ala`.
- **Numeric edges** (§5.6): `7f`, `1e5x`, `0x1Fg`, `0X1F`, `1__000`, `1_`, `1e`, `1e+`, `0b102`, `0o8`
  and `0x_1F` are K0127. `007` and `00` are K0125. Valid edges: `0x1e+5` (= `0x1e + 5`), `1.5e-3`,
  `2E+10`, `1..10`, `t.0.1` and `5.to_string()`.
- **Arabic-Indic** in `ar` files: valid integers, `٫` floats, `_` groups and ranges. K0128 for
  hex and exponents. K0125 for `٠٠٧`. K0127 when digits are glued to letters.
- **Arabic punctuation:** `(1،5)` is a tuple, never a decimal. `؟` and `،` are valid in `ar`.
  `؛` is K0904.
- **Attached و:** `وزن` is a legal identifier, while `وصح` is K0401 with the split hint `[M2]`.
- **Keyword orthography:** hamza-less, ة/ه, ى/ي, and Persian ک ی ە in keywords fold (D19, no
  SEC0107). Harakat or tatweel in a keyword is SEC0106.
- **Contextual freedom and raw identifiers** (§5.3 rules 7–8): `law` and `type` work as field,
  label and member names, and `` `match` `` and `` `law` `` work as raw identifiers.

## seeded-errors

There is exactly one deliberate error per case, at a known line. Most cases ship the same
program in all three skins, with the error on the same line, which satisfies the ">= 30 files x 3
skins" scope of `D5.error_recovery_precision`. A few are skin-specific: K0128 and the Arabic `؛`
case are `ar` only, and the K0201 cases use a different foreign word in each skin. Each file is
otherwise clean. The four unmodified base programs, in all three skins, produce no lexer or
parser diagnostic today, and they were written to type-check as well. So a precise compiler
reports only the seeded `(code, line)`.

The codes are spec-defined (§27) and detectable by the lexer or parser: K0103, K0105, K0120,
K0122, K0125, K0127–K0132, K0201, K0710, K0720, K0731, K0901–K0926 (§21.3, in their obvious
syntactic positions), SEC0110 and SEC0103. The provisional K025x parse codes (FOUNDER-QUEUE Q1)
are deliberately not used as expectations, and neither are type or name errors (K04xx, K06xx),
which M1 cannot find. When the compiler emits a generic K0250 where the spec defines a targeted
K09xx code, that counts as a miss.

**Path note.** SCORECARD and LOOP name `eval/diagnostics/broken/` as the suite location. That
path is protected, so this suite lives here, and the reviewer promotes it there (or points the
harness here).

## mixed-skin

Spec §5.4 says another skin's keyword, core name or alias is **K0201**, but only where that
word, read as an identifier, would be a syntax error or an undefined name. 27 cases cover
English words in `ar`/`franco`, Franco and Arabic words in `en`, Franco variants (`lw`, `khalli`),
the hamza-less `اذا` in `en`, and `،`/`؟` outside `ar` (§6.4). 12 legal twins check that a declared
identifier which merely equals another skin's reserved word, or a reserved word in field, label
or member position or inside strings and comments, gives **no error** (H0202 at most). They also
check that ASCII `,`/`?` are accepted in `ar`. The "undefined name" cases need only a file-local
declaration lookup, since no case has a `use`. K0301 (two skin files for one module) is M2 and
is not covered.

## conformance

These are 36 valid programs with non-canonical layout:

- trailing commas (call, params, list, tuple, map);
- a multi-line list without a trailing comma;
- 0 or 3 blank lines between items, extra blank lines in a body, at the start and at EOF;
- one-line `if`/`for`/`fn` blocks and `=>` bodies;
- a line longer than 100 columns;
- tight and loose spacing, spaces before `(` and `:`;
- CRLF in all three skins, no final newline, trailing spaces;
- comments at odd indentation;
- bracket-suppressed odd indentation and uneven leading-dot chains;
- odd spacing in inline `if` and match arms;
- redundant parentheses and backticks;
- Franco variants, Arabic orthographic variants and Persian letters in keywords;
- ASCII punctuation and Arabic-Indic digits in `ar`.

All of them have `must_reject: false`.

**Known spec tension (founder fork).** §4.4 keeps blank lines in KCF "byte for byte", and the
current KCF also serializes trailing commas. But §21.1 makes `fmt` cap and insert blank lines
(rule 4) and add or drop trailing commas (rule 1). So `kcf(fmt(x)) == kcf(x)` cannot hold for
the blank-line and trailing-comma cases unless KCF stops recording that layout. The notes of
those cases say so. The cases are kept, because they are exactly what the property must decide.

## Baseline: current compiler (informational)

Run on 2026-10-25 with `compiler/target/debug/kalam.exe`, built 2026-10-25 03:17 from the iter-3
tree. Per file, the runner used `kalam check --json` (source text and lexing) plus
`kalam fmt --stdout` (parsing), and for conformance it added fmt idempotence. Expectations were
not changed to match the compiler.

| Suite | M1-scope pass | `[M2]` |
|---|---|---|
| trojan-source | 98 / 113 | 0 / 10 |
| lexing-hazards | 47 / 47 | 0 / 1 |
| seeded-errors | 27 / 45 | — |
| mixed-skin | 31 / 39 | — |
| conformance | 35 / 36 (14 of 38 files also differ in `kcf(fmt(x))` vs `kcf(x)`) | — |

Main gaps:

- **trojan-source:**
  - SEC0101 (NFC), SEC0105 (mixed script), SEC0112 for noncharacters and unassigned code points, and SEC0106 for presentation forms are not implemented.
  - Invalid UTF-8 aborts with "cannot read" instead of SEC0100.
  - An emoji ZWJ sequence and VS16 after an emoji are wrongly rejected (SEC0111).
  - U+3164 used as an identifier is not flagged in code.
- **seeded-errors:** most K09xx codes come out as generic K0250/K0255, and `f"…"` is accepted as a tag.
- **mixed-skin:** K0201 for core names in undefined-name position (`print`, `Int`, `Some`, `true`, `ektb`, `اطبع`) is missing.
- **conformance:** an uneven leading-dot chain is rejected (K0120/K0122).
