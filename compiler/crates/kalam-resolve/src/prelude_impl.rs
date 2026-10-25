//! The implicit prelude (spec §13.1, §20.1 "prelude" row, §9.7 built-in traits).
//!
//! The 19 core names (§7.2) are prelude items too, but they are spelled per skin and reach
//! the resolver as concepts, so they are not listed here. Every name below is fixed ASCII
//! in every skin (§7.2 "Fixed ASCII"). Tags (`sql"…"`, `html"…"`, …) are not names.

/// Prelude types, traits and capability types, in symbol-id order.
pub const PRELUDE: &[&str] = &[
    // §13.1 fixed-ASCII types.
    "BigInt",
    "Bytes",
    "Never",
    "Self",
    "Html",
    "Sql",
    "Command",
    "Url",
    "Path",
    "PathSegment",
    "Regex",
    "Secret",
    "AnyError",
    "Cell",
    // §13.1 capability types (§16.1); `World` is a core name.
    "Net",
    "Fs",
    "Env",
    "Clock",
    "Rand",
    "Proc",
    "Stdin",
    "Screen",
    // §9.7 built-in traits (used in bounds, `impl T as Show:` and `@derive(…)`).
    "Eq",
    "Hash",
    "Ord",
    "Show",
    "Encode",
    "Decode",
    "Row",
    "Message",
    "Error",
    "From",
    "Template",
    // §10.5: the types of `a..b` and of anything `for` iterates. Not in the §13.1 list,
    // but written in signatures; provisional (FOUNDER fork, see the M2 report).
    "Range",
    "Iterable",
];

/// Prelude modules (§13.1 "the modules `log` and `json`"): the name and the module path.
pub const PRELUDE_MODULES: &[(&str, &str)] = &[("log", "std/log"), ("json", "std/json")];
