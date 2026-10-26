# The diagnostic judge (`D5.judge_score`)

The rubric is `RUBRIC.md`. Do not edit it casually: `sample.json` pins its SHA-256, and any change makes every judged item stale (below).

| File | Written by | What it is |
|---|---|---|
| `RUBRIC.md` | hand (pinned) | The Becker-rubric the judge and the human grader use |
| `sample.json` | `kalam-eval judge-sample` | One rendered golden per code, the items to judge |
| `report.json` | `kalam-eval judge-ingest` | The judge's scores; each row records the rendered text it judged (`rendered_sha256`) |
| `calibration-kit.csv` | `kalam-eval judge-calibration-kit` | 60 items for a human to grade blind |
| `calibration.json` | `kalam-eval judge-calibrate` | The human grades and their mean absolute error (MAE) against the judge |

## Stale items (FOUNDER-QUEUE Q43)

When a golden's rendering, or the rubric, changes after judging, its item is **stale**. A sampled item that the report does not score is **unjudged** (a new golden code, for example). While any item is stale or unjudged, `D5.judge_score` is not measurable. The scorecard shows the lists under `checks.m2.judge_items`.

1. `kalam-eval judge-stale` lists the stale, unjudged and removed items. It exits 1 while any item is stale or unjudged.
2. `kalam-eval judge-sample` rewrites the sample. Before it replaces the sample the report scored, it pins the judged text into the report rows. The scores do not change.
3. The judge grades **only** the listed items with `RUBRIC.md`, using the same model and output format.
4. `kalam-eval judge-ingest --merge --in <file>` keeps every fresh row and replaces the re-judged ones. A different judge model is refused, because one report holds one model.

## Calibration (for the founder)

A step-by-step version, with the exact command, is in `HOW-TO-CALIBRATE.md`. It has you grade a copy of the kit, so regenerating the kit can't overwrite your grades.

The judge's score counts only after a human has graded at least 50 items and the MAE between the human and the judge is 0.5 or less.

1. Open `calibration-kit.csv` in a spreadsheet. It is UTF-8 with a BOM, so the Arabic displays correctly. It does not contain the judge's scores.
   - `rendered` is the diagnostic to grade.
   - `source` is the program it is about, for rubric check 9 ("Accurate").
   - A text cell that would start with `=`, `+`, `-` or `@` has one extra leading space, so the spreadsheet does not read it as a formula.
2. Grade each item from 0 to 5 with `RUBRIC.md`. Half points are fine, and `4,5` is read as `4.5`. Put the grade in `human_score`. Grade at least 50 rows; leave a row empty to skip it. Write anything you like in `notes`.
3. Save as CSV. Comma, semicolon or tab separators all work.
4. Run `kalam-eval judge-calibrate --grader "<your name>" eval/judge/calibration-kit.csv`. It writes `calibration.json` and prints the MAE.

Rows are matched on `id`.
- A bad score, an unknown id or a repeated id rejects the whole file, and nothing is written.
- A grade is left out (and listed) if its item is stale now or its rendering changed after the kit was written.

Each scorecard run recomputes the MAE, using only the grades whose rendering is still current. `kalam-eval judge-calibration-kit` regenerates the kit with the same deterministic choice of items (`sha256(seed + "/calibration/" + id)` over the judged, non-stale items).
