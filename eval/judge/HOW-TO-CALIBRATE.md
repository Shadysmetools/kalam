# How to calibrate the judge (for the founder)

An AI judge scores every compiler error message from 0 to 5 (`D5.judge_score`). Its scores count only after **you** have graded at least 50 of the same messages and your grades and the judge's grades are close: the average gap (the MAE) must be 0.5 or less. This page walks you through it.

You never see the judge's scores while grading. The kit leaves them out on purpose, so they can't nudge you.

## 1. Make your own copy of the kit

The kit is `eval/judge/calibration-kit.csv`: 60 error messages, one per row. Copy it somewhere outside the repo and grade the copy. The repo file gets rewritten whenever the kit is regenerated, and that would wipe your grades.

```powershell
Copy-Item "E:\knowcap-dev\6-R and D\new-language\eval\judge\calibration-kit.csv" "$HOME\Documents\kalam-grades.csv"
```

Open the copy in Excel. It is saved as UTF-8 with a BOM, so the Arabic shows correctly.

## 2. What each column is

| Column | What to do with it |
|---|---|
| `id` | The message's name, such as `K0255/en`. **Don't edit it**, because grades are matched on it. |
| `code`, `skin` | The error code and the file's language (`en`, `ar` or `franco`). The message is written in that same language. |
| `human_score` | **Your grade goes here**, 0 to 5. |
| `notes` | Optional. Anything you want to remember, such as "caret is on the wrong word". |
| `rendered` | **The message you grade**, exactly as `kalam check` prints it. |
| `source` | The program the message is about. Use it to check that what the message says is true. |
| `rubric`, `rendered_sha256` | Bookkeeping. Leave them alone. |

Some text cells start with an extra space (for example ` -- STATEMENT OUTSIDE A FUNCTION ...`). That stops Excel from reading a leading `-`, `=`, `+` or `@` as a formula. Ignore it.

## 3. Grade each message from 0 to 5

The rules are in `eval/judge/RUBRIC.md`, and that file is the authority. Here they are in plain English. Read the `rendered` message as a newcomer who knows only that message's language would, and go through these ten questions:

1. **Readable.** Is it plain words and short sentences, with no compiler jargon to look up?
2. **One problem.** Does it name the real cause, not a side effect?
3. **Right place.** Does the excerpt show the right line, with the `^^^` carets under the right characters?
4. **Kind tone.** Is there no blame, and none of "illegal", "invalid", "fatal" or "bad"? ("I expected..." is fine.)
5. **Example.** Does it show what correct code looks like, when that would help?
6. **Fix.** Does it give a concrete fix (the `Help:` line), written in the file's own language?
7. **More help.** Does it end with `kalam explain CODE`?
8. **Reason.** Does it say *why* the code is wrong, not only *that* it is?
9. **True.** Is everything it says about the program in `source` actually true?
10. **Right language.** Are the words in the message language, and the code in the file's language (Arabic keywords in an Arabic file, for example)?

Then pick a score using these anchors:

| Score | Means |
|---|---|
| **5** | All ten hold. A newcomer could fix the code from this message alone. |
| **4** | One small gap, such as no example where one would have helped. |
| **3** | Correct and polite, but the fix or the reason is vague. |
| **2** | Correct, but hard to act on: jargon, no fix, or the caret in the wrong place. |
| **1** | Partly misleading, or in the wrong language. |
| **0** | Wrong, or no usable information. |

Some tips:
- Half points are fine (`3.5`). A comma works too: `4,5` is read as `4.5`.
- Grade **at least 50** rows. You can skip up to 10 by leaving `human_score` empty.
- Don't look up the judge's scores in `report.json` before you finish. Calibration only works if you grade blind.
- If you can't read a row's language well, skip it rather than guess.

## 4. Save as "CSV UTF-8"

In Excel use **File > Save As > "CSV UTF-8 (Comma delimited) (*.csv)"**. Don't pick plain "CSV (Comma delimited)". That format ruins the Arabic and the `│` characters, and the ingest command may refuse to read the file. Comma, semicolon or tab separators all work.

## 5. Ingest your grades

Run this in PowerShell, putting your name after `--grader` and the path of the file you saved at the end:

```powershell
$env:PATH = "$HOME\.cargo\bin;$env:PATH"
$env:CARGO_TARGET_DIR = "E:/tmp/claude/kalam-m2-target"
Set-Location "E:\knowcap-dev\6-R and D\new-language\compiler"
cargo run -q -p kalam-eval -- judge-calibrate --root "E:\knowcap-dev\6-R and D\new-language" --grader "Shady" "$HOME\Documents\kalam-grades.csv"
```

`CARGO_TARGET_DIR` keeps this build out of `compiler/target`, which holds the fuzz binary of a long fuzz run. Don't drop that line while a fuzz run is going.

What you will see:
- **Success:** it writes `eval/judge/calibration.json` and prints the MAE. 0.5 or less means the judge is calibrated and its score starts counting. More than 0.5 means the judge and you disagree too much, so its score stays not measurable. Tell the loop, so it can look at the rows where you disagreed most.
- **`note: N judge item(s) are stale or unjudged, so D5.judge_score stays not measurable ...`:** your grades and `calibration.json` are still saved. The judge score just waits until the loop re-scores those few messages. Nothing more is needed from you.
- **`rejected, nothing written: N item(s) are graded; calibration needs >= 50 human grades`:** grade more rows and run it again.
- **`rejected, nothing written: row R ...`:** one score isn't a number from 0 to 5, an `id` was edited, or a row appears twice. Fix that row and run it again. Nothing was saved.
- **`left out: <id>: ...`:** that message has changed since the kit was made, or the judge hasn't re-scored it yet. Your grade for it is dropped, because there is no matching judge score to compare it with. That is fine as long as at least 50 grades remain.

You can run the command as many times as you like. Each run replaces `calibration.json`.

## Why the kit can change

`kalam-eval judge-calibration-kit` regenerates the kit. It picks 60 messages with a fixed rule from the ones the judge has scored on their current text. When a message's text changes (a fixed caret, for example), that message drops out of the kit and another takes its place. If you're partway through grading, keep using your copy. Grades on rows that are still current all count.
