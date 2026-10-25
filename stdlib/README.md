# Kalam standard library (M2: signature stubs)

At M2 the stdlib exists only as **signature stubs**: types, capabilities and suspension
bits (ROADMAP M2). Programs that use it type-check, but only the prelude core (`print`,
`panic`, `assert`, and the core-type methods the reference interpreter implements natively)
runs before M4.

The canonical text is the English skin (spec §21.1). Stdlib member names are English in
every skin, and the 19 core names are translated in every skin (spec §7.2, D21).

## Layout

One file is one module (spec §13.1). The package is `std`, so `src/http.kl` is `std/http`.

| File | Module | Contents |
|---|---|---|
| `src/prelude.kl` | prelude | core types, `Ordering`, built-in traits, `print`/`panic`/`assert`, `Int`/`Float`/`Bytes` methods, `World` and the capability types (`Net` and `Stdin` methods), `Cell`, `Secret`, `AnyError`, the safe string kinds (`Html`, `Sql`, `Command`, `Url`, `Path`, `PathSegment`, `Regex`), `StaticParts`, `TemplateHole`; re-exports `std/json` and `std/log` |
| `src/string.kl` | `std/string` | `String` methods, `ParseError` |
| `src/list.kl`, `src/map.kl`, `src/set.kl` | `std/list`, `std/map`, `std/set` | collection methods |
| `src/option.kl`, `src/result.kl` | `std/option`, `std/result` | `Option` and `Result` methods |
| `src/math.kl`, `src/decimal.kl`, `src/bigint.kl` | `std/math`, `std/decimal`, `std/bigint` | numbers, `Rounding` |
| `src/json.kl`, `src/log.kl` | `std/json`, `std/log` | schema-first JSON; ambient structured logs |
| `src/time.kl` | `std/time` | `Instant`, `Date`, `Duration`, `Zone`, `Clock` methods |
| `src/env.kl`, `src/fs.kl`, `src/path.kl`, `src/proc.kl` | `std/env`, `std/fs`, `std/path`, `std/proc` | `Env`, `Fs`, `Path`/`PathSegment` and `Proc` methods |
| `src/random.kl` | `std/random` | `Rand` methods, `SeededRand` |
| `src/http.kl` | `std/http` | routes, requests, responses, server, client |
| `src/sql.kl` | `std/sql` | `Db`, `sqlite`, `postgres`, `DbError` |

**Where methods live.** Inherent methods of a prelude type are declared in the module that
spec §20.1 names for them (`impl String:` is in `std/string`). Method resolution never
depends on imports (DR-27), so `name.trim()` works without `use std/string`. The stdlib is
one privileged package: it may write inherent impls for prelude types outside the prelude.

## Stub conventions

| Marker | Meaning |
|---|---|
| `@builtin` | Provided by the compiler and runtime. A builtin function or method has the body `=> panic("builtin")` and nothing else; the generator rejects a mismatch either way. A `@builtin` record (`World`, `Request`) has readable fields but no public constructor. |
| `extern type X` | A built-in opaque type with no fields (the grammar has no empty record). Never used for functions. |
| `@suspends` | A base suspending primitive (spec §15.2, ARCHITECTURE §9.1): network, files, database, `Stdin.read_line`, `serve`. |
| `@capability` | A capability type (spec §16.1). |
| `@handle` | A handle built from a capability (`Db`, spec §16.1 rule 5). |
| `@console` | Ambient console output (D20). A library calling it must declare `console = true`. |
| `@stores_callbacks` | Function-typed parameters are stored, not called, so the function does not suspend (`Router.get`). |
| `@positional` | Spec §12.1: conventional argument order, exempt from K1104. |
| `@sync fn(…)` | A callback that must not suspend (spec §15.4). |
| `##` | Doc comment, copied into the index. |

A function with a non-`@sync` function-typed parameter is effect-polymorphic (spec §15.3):
it suspends exactly when its callback does.

The core constructors `Some`, `None`, `Ok` and `Err` cannot be declared as enum variants
(they are core-name tokens), and the marker traits `Eq`, `Hash`, `Ord`, `Encode`, `Decode`,
`Row`, `Message`, `SqlParam` and `Iterable` cannot be declared without a member. The index
generator adds both groups itself.

## `api-index.json`

`api-index.json` lists every public item: `path`, `module`, `kind`, `name`, `owner`,
`receiver` (the impl header), `signature` (one line, canonical English), `capability`
(capability and handle types reachable from the receiver and parameters, plus `console`),
`suspends` (`yes`, `no` or `if_callback`; absent for non-functions), `attributes`,
`spellings` (en/ar/franco, core names only) and `doc`. It feeds the K0420
hallucinated-API guard and `llms.txt`.

Paths: prelude items are bare (`print`, `Int`); other module items are `std/http.serve`;
members are `Owner.member` (`String.trim`, `HttpError.NotFound`). A member of a trait impl
whose trait takes type arguments carries it: `HttpError.from@From[DbError]`.

Regenerate it after editing a stub (from `compiler/`):

```
cargo run -p kalam-stdlib-index            # writes stdlib/api-index.json
cargo run -p kalam-stdlib-index -- --check # exits 1 if it is stale
```

`cargo test -p kalam-stdlib-index` checks that the index is current, that every stub is a
`kalam fmt` fixed point and converts to every skin, and that the API the dev corpus uses
exists with the expected shapes.
