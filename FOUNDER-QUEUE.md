# Founder review queue (for the M1 milestone stop)

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
