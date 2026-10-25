# Held-out store (SEALED)

This directory holds the sealed 30% hold-out of the Kalam corpus for the current eval version (eval/LOOP.md §6.2, eval/corpus/INDEX.md §7).

## Sealing rule

- **Do not read anything in this directory except this README and `MANIFEST.json`.** This applies to the fixer (`/loop`) and reviewer sessions and to every other agent that is not the holdout author or the holdout runner. Do not open, list, grep, diff, hash-compare line by line, summarize, copy or quote any file under `eval/holdout/<task>/` or `eval/holdout/tools/`.
- Only two roles may touch the contents:
  - the **holdout author** session, which writes them from the INDEX.md titles, `spec/KALAM-SPEC.md` and `spec/keywords.json` only, and never fixes compiler code;
  - the **holdout runner**, a separate process that is given a built `kalam` binary, runs the same runners as for the dev corpus, and returns **aggregates only** (`m1_syntax`, `parity`, and so on; LOOP.md §6.2 item 4). It never returns names, diffs, diagnostics or stdout.
- A sealed task never becomes visible during its own eval version. At the next eval version, tasks already disclosed in a milestone report move to the dev corpus, and the holdout author writes fresh ones (LOOP.md §6.2 item 6).
- Compiler, runtime and stdlib code must not mention these task directories, their paths or any of their contents (LOOP.md §6.1 rule A5). The G-INT corpus-leak scan covers this.
- Changing any file here changes `MANIFEST.json`. Its SHA-256 is pinned in `eval/FREEZE.json`, and a mismatch fails G-INT. Only the holdout author regenerates it, with `tools/seal.py`.

## What is here

- `MANIFEST.json`: the task ids and the SHA-256 of every file in this directory, but none of their contents.
- One directory per held-out task. Each uses the dev-task layout of INDEX.md §2 (`task.toml` with `split = "heldout"`, `en.kalam`, `ar.kalam`, `franco.kalam`, `ref.py`, `expected.txt`).
- `tools/`: the holdout author's converter, its expected-output generator and its sealer. These are holdout-author tools; they are not part of the harness.

## Known deviation from LOOP.md §6.2

LOOP.md §6.2 puts the sealed store outside the repository, at `E:\knowcap-dev\6-R and D\kalam-holdout\`, encrypted at rest with `age`. This copy lives inside the repository, as the current workflow step instructed. It is therefore protected only by this rule, not by encryption or file permissions. Whether to move it there and encrypt it is an open founder decision.
