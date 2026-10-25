# Spec conformance cases

Feeds `D1.spec_clause_coverage` and `D1.conformance_pass_rate`. The runner is `compiler/crates/kalam-eval/src/conformance.rs`, and the measurement rules are in [`../HISTORY-SCHEMA.md`](../HISTORY-SCHEMA.md) ("How each M2 tier check and checkpoint is measured").

## Layout

```text
CLAUSES.json                        spec §4–§22 split into units (### subsections, plus §N.0) and clauses §N.M.k
<clause-id>/<case>/case.kl          the same program once per skin
                   case.ar.kl
                   case.franco.kl
                   expect.json      {"must_reject", "codes", "lines", "notes", "stdout"?, "stage"?}
pending/                            fixer additions (LOOP.md §1); never counted until the reviewer promotes them
```

- `<clause-id>` is a clause (`13.1.2`) or a unit (`13.1`). The leading `§` is optional.
- `expect.json` is the shared suite format (`eval/suites/README.md`), with two optional fields:
  - `stdout`: the program also runs on `kalam run --interp` and must print exactly this;
  - `stage`: the compiler stage the case needs (`lex`, `parse`, `resolve`, `typeck`, `interp`). The default is `interp` with `stdout`, otherwise `parse` for §4–§8, `resolve` for §10.1 and §13, and `typeck` for every other section.
- A case whose stage `kalam check` does not run yet **fails**. It never passes by default.
- Regenerate the index with `kalam-eval clauses --write` after a spec edit, then re-check every case filed under a clause whose id moved. `kalam-eval clauses --check` fails while the index is stale.

## The initial cases (M2 start)

There are 57 cases (31 must-reject, 26 accepted), each in all three skins. They cover name resolution (§5.3 skeleton and fold rules, §10.1, §13.1–§13.4) and the basics of the type system (§9.1, §9.2, §9.4, §9.5, §9.8, §9.9).

- **Source:** every case was derived from the spec text, not from compiler behaviour. Each `notes` field quotes the clause it tests. The English file was written by hand. The `ar` and `franco` files were produced with `kalam convert`, which translates keywords, core names and punctuation only. For a program the parser rejects, the conversion ran on a valid carrier, and the hazard was then put back identically in every skin.
- **Must-reject cases** only use rules for which the spec names a code. Accepted twins are prelude-only programs.

## Spec gaps found while writing the cases

These rules have no diagnostic code in the spec, so they have no must-reject case. Each one is a question for the spec:

- §9.1.2: no truthiness ("conditions must be `Bool`");
- §9.2.4: `Decimal` has no `/`;
- §9.3.3: `Map` keys must be `Eq + Hash`;
- §9.7.2: coherence and the orphan rule;
- §10.1.1 and §13.3: two module items with one name in one namespace (for example `type Point` plus `fn Point`). §10.1.2 (K0405) covers bindings, not items;
- §13.1.2: relative imports;
- §13.4.1: a module-level `let` that is not a constant expression.

One clause conflict:

- §10.1.2 lists core names among the names a binding may not reuse and gives K0405. §7.2 and §27 give K0413 for a declared or shadowed core name. No case is filed until the spec picks one.

## Divergences seen at authoring time (2026-10-25)

- `use std/http.*` reports K0250 (the generic parse error), not K1203 (§13.1.2).
- A top-level `var` reports K0255 (the generic expected-item error), not K1202 (§13.4.2).

Both cases fail until the parser reports the spec code.
