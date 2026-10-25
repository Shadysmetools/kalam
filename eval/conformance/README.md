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

- §10.1.2 lists core names among the names a binding may not reuse and gives K0405. §7.2 and §27 give K0413 for a declared or shadowed core name. No case is filed until the spec picks one. *Update (M2 iteration 2):* FOUNDER-QUEUE Q21 (binding provisional) says "Only the file's own skin spelling of a core name is K0413", using a `let` binding as its example. On that basis, `7.2.1/core-name-as-let-binding` expects K0413. The §10.1.2 text still needs the erratum.

## Divergences seen at authoring time (2026-10-25)

- `use std/http.*` reports K0250 (the generic parse error), not K1203 (§13.1.2).
- A top-level `var` reports K0255 (the generic expected-item error), not K1202 (§13.4.2).

Both cases fail until the parser reports the spec code.

## M2 iteration 2 additions (2026-10-25)

The suite now has 152 cases (86 must-reject, 66 accepted) over 82 clause directories, each in all three skins. They cover 45 of the 95 units: §7.2, §9.3, §9.6, §9.7, §10.2–§10.8, §11, §12.1–§12.3, §14.1–§14.4, §15.1–§15.4, §15.6, §16.1, §17.1–§17.6, §18.1 and §18.2 were added. They were written the same way as the initial cases: English by hand, `ar` and `franco` by `kalam convert`, and a valid carrier plus the hazard put back by hand where the parser rejects the program (`impl Show for Money:`, `async fn`, `try:`, `1 < x < 10`, value-position `if` without `else`, `range(3)`, a `Float` pattern, `.unwrap()`).

- **Q25 (literal typing):** `9.1.2/no-implicit-numeric-conversion` now adds two variables (`count + ratio`, K0601). The new twin `9.1.2/int-literal-takes-float-type` accepts `1 + ratio`. Open fork: Q25 says "binary operators", and `==` is one. But §9.8.1, `9.8.1/int-eq-float` and the harness's typeck probe all require `1 == 1.0` to be K0602. These cases assume Q25 does not reach the comparison operators.
- **Q21 (another skin's core names):** `7.2.1/other-skin-core-name-is-ordinary` is written by hand in each skin, because conversion would translate the name. The English file binds `عدد`, and the Arabic and Franco files bind `print`. The K0413 cases use the file's own spelling.
- **Lines left unchecked (`null`):** the spec does not say where K1001, K1101, K1401, K1402 and K0612 point, so those cases check only the code.
- **Weaker citations:** `10.5.2/python-range-loop` (K0926) cites §10.5.2 through §21.3. `18.2.1/seeded-rand-has-no-token` reads §18.2's "type error by omission" as K0420, the unknown-method code (§21.3).

Spec gaps found (no diagnostic code, so no must-reject case):

- §9.3.1: a tuple with 1 element or more than 8 elements;
- §9.6.2: an unsatisfied trait bound (`largest[T: Ord]` called on `List[Float]`);
- §10.5.4: `break` or `continue` outside a loop;
- §12.1.4: a default value that refers to another parameter;
- §13.1.4: `use` with a string path;
- §15.6.3 and §16.1.12: a `Cell` or a capability stored in a module-level constant;
- §17.6.3: a regex that RE2 rejects (backreference, lookaround);
- §18.3.1: a hand-written `impl X as Decode`.

Not authorable in the shared suite format:

- **K0418 (Q23, item is private)**, **K1201** (import cycle) and **K0301** need two modules of one package. The format judges each file on its own. `stdlib/` has no non-`export` item a single file could import, and a stub would not be spec-derived anyway.
- **K1502** (`Screen` on a non-browser target) and **K1504** (library `print` without `console = true`) need a target or a manifest.
- **W0403, W1606, W1622, W0421, W0710, W0720** are warnings. The runner counts only errors, so a must-reject case on them could never pass.

Divergences seen at authoring time (debug `kalam`, typeck not wired into `kalam check` yet):

- `11.0.6/float-literal-pattern`: the parser reports K0250, not K1005.
- `7.2.1/core-name-as-let-binding`: the resolver reports nothing for `let print = 1`. K0413 is expected. (`type Option:` does report K0413.)
