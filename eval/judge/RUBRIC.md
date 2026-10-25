# Diagnostic judge rubric (`D5.judge_score`)

rubric_version: `becker-2026.1`

**Who uses it:** the orchestrator's judge agent (FOUNDER-QUEUE Q20). The harness never calls a model; it pins this file by SHA-256 in `eval/judge/sample.json` and rejects scores made against any other version (`kalam-eval judge-ingest`).

**Source:** the guidelines of Becker et al., "Compiler Error Messages Considered Unhelpful" (ITiCSE-WGR 2019), restated for Kalam's template (spec §21.4).

## What the judge sees

One item per line of `eval/judge/sample.json` → `items`: the source file (`source`, in the skin `skin`), and the diagnostic exactly as `kalam check` prints it in that skin's message language (`rendered`). Judge the **rendered diagnostic only**, as a reader of that language would see it. Do not judge the program.

## Score: one number from 0 to 5 per item

Start from the ten checks below. Each one that holds earns credit; the score is the closest whole or half number.

| # | Check (Becker et al.) | Holds when |
|---|---|---|
| 1 | Readable | Plain words in the message language, short sentences, no compiler jargon the reader must look up |
| 2 | Low cognitive load | One problem per message; the root cause, not a symptom |
| 3 | Context | It points at the exact place (line excerpt and caret on the right characters) |
| 4 | Positive tone | No blame and none of "illegal", "invalid", "fatal", "bad" (spec §21.4 rule 3); first person ("I expected…") is fine |
| 5 | Example | It shows what correct code looks like when that helps |
| 6 | Solution | It gives a concrete fix (`Try:` / `Help:`), written in the file's skin |
| 7 | Further help | It names `kalam explain CODE` |
| 8 | Logical argument | It says why the rule exists or why the code is wrong, not only that it is |
| 9 | Accurate | Everything it says about this program is true |
| 10 | Right language | Message text in the message language; code in the file's skin (spec §21.4 rule 1) |

Anchors:

- **5**: all ten hold; a newcomer fixes the code from the message alone.
- **4**: one minor gap (for example no example where one would help).
- **3**: correct and polite, but the fix or the reason is vague.
- **2**: correct, but hard to act on (jargon, missing fix, caret in the wrong place).
- **1**: misleading in part, or in the wrong language.
- **0**: wrong, or no usable information.

## Output the judge returns

```json
{"model": "<exact model id>", "rubric_version": "becker-2026.1",
 "sample_sha256": "<from sample.json>",
 "scores": [{"id": "K0901/franco", "score": 4.5, "rationale": "one short sentence"}]}
```

Every item id exactly once; scores in 0–5. Then run `kalam-eval judge-ingest --in <file>`.

## Calibration (scorecard.json)

The metric is unavailable until the judge is calibrated: a human grades at least 50 sample items with this rubric and writes `eval/judge/calibration.json`:

```json
{"schema": "kalam-judge-calibration/1", "grader": "<name>", "scores": [{"id": "K0901/franco", "score": 4.0}]}
```

The harness computes the mean absolute error between those grades and the judge's scores for the same items; it must be ≤ 0.5.
