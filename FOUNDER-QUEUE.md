# Founder review queue

**Q1–Q18 APPROVED by founder 2026-10-25 (M1 review).** New items from M2 on start at Q19.

Minor design forks the loop decided provisionally. Each one is reversible. Confirm or override them at the M1 review.

| # | Fork | Provisional choice | Source |
|---|---|---|---|
| Q1 | Parse-error code range | Keep K0250–K0269 (provisional) instead of moving them to K07xx | iter 2 parser, PARSE-CODES.md |
| Q2 | Spec §28.3–28.8 glossary blocks translate identifiers too | Accept a comparison with identifier names erased for glossary blocks; strict same-AST parity applies to converter output | iter 2 |
| Q3 | Separate `kalam-ast` tree next to the CST (ARCHITECTURE §6 said typed views over the CST) | Keep two trees, and record it in ARCHITECTURE.md | iter 2 |
| Q4 | K0259 "nesting too deep" | Implementation limit, not a spec code | iter 2 |
| Q5 | Unknown tag before a string literal (`foo"..."`) | Make it a parse error | iter 2 |
| Q6 | Do the 19 translated core names count against the keyword budget (50 reserved spellings per skin, against a limit of 40)? | Report the count only; it is not a gate | iter 1 verifier |
| Q7 | D19 fold semantics: a name written with Persian letters does not silently resolve to a name declared with Arabic letters; it gets K0401 plus an automatic fix | As stated | iter 1 |
| Q8 | KCF and layout: spec §4.4 kept blank lines "byte for byte", but §21.1 lets `fmt` cap and insert blank lines and add or drop trailing commas, so `kcf(fmt(x)) == kcf(x)` could not hold | KCF2: KCF records no layout `fmt` may change. A blank-line run counts as one blank line, and it is recorded only between statements or members of one block. It is not recorded at block start, after an attribute, before `else`, inside brackets, at the top level or at EOF. Trailing commas are dropped except in `(x,)`. A one-line block is serialized like its indented form. The header is `KCF2` (every hash changes once). Known limit: if `fmt` later removes redundant parentheses or `pass` (§21.1 rule 7) or sorts `use` lines (rule 3), KCF must normalize those too | iter 4 (orchestrator decision; spec §4.4 erratum 2026-10-25) |
| Q9 | K0201 for another skin's word used as an undeclared name (§5.4 "undefined name" half), decided in the lexer before name resolution exists | It is decided by a file-local, scope-insensitive declaration lookup (let/var, fn and parameters, for/case patterns, type/enum/trait names, generics, use items, `as` aliases). The word is legal where it is declared, in a §5.3 rule 8 position or in backticks. Interpolation holes are not checked yet (holes are re-lexed alone). Random-program generators must declare such names | iter 4 |
| Q10 | `[M2]` marker on suite cases that need the name resolver | The harness honors `[M2]` in every suite. Those cases are excluded at M1 and checked from M2 on | iter 4 verifier |
| Q11 | Help text for P6 and diagnostic completeness | Strict reading: every code needs a message, help text and an explain page in en/ar/franco | iter 4 |
| Q12 | Spec errata on security codes | Presentation forms → SEC0106; ZWJ/ZWNJ in an identifier → SEC0111; a dedent to an unknown column → K0122 | iter 4 |
| Q13 | Where the sealed held-out store lives | Locally under eval/holdout, gitignored except MANIFEST.json and README.md, so contents never enter git history. Moving to the age-encrypted store outside the repo (LOOP §6.2.2) is deferred until Beta | iter 4 |
| Q14 | Path of the seeded-errors suite | eval/suites/seeded-errors is canonical; update scorecard.json and LOOP.md to match | iter 4 |
| Q15 | FREEZE `holdout.split_seed` (pending_founder) | Provisionally set to the M0 sign-off commit `d3abd2d`, a value no agent picked. **Founder: override if you want your own seed** | iter 4 |
| Q16 | Backticks around another skin's reserved word that the file does not declare | `fmt` and `convert` keep them, and add them where needed, so their output always passes their own lexer | iter 5 verifier (fuzz found convert producing K0201) |
| Q17 | Measure diagnostics through the user-facing `kalam check` or through the harness parse hook | Through `kalam check`, which must now run the parser too (it is lex-only today) | iter 5 |
| Q18 | Unicode data source for the NFC and general-category checks (SEC0101, SEC0112) | Tables generated from Python `unicodedata` (Unicode 15.x on this machine) and vendored with the version noted; upgrade to 16.0 when a source is available | iter 6 |
