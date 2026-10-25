//! The type representation (spec §9) shared by the checker, `kalam-effects` and KIR.
//!
//! - `Int` is a true 64-bit checked integer on every backend (DECISIONS D18). `Float` is
//!   IEEE binary64 and is **not** `Eq`/`Ord`/`Hash`; `==` and `<` on two `Float`s are
//!   built-in IEEE operators (DR-23, §9.8). `Decimal` is exact base 10 and has no `/`.
//! - Function types carry a suspension bit (§15.3). Inside a [`Scheme`], a fn-typed
//!   parameter has an effect parameter [`Suspension::Param`]; after instantiation it is an
//!   effect variable [`Suspension::Var`] that `kalam-effects` solves.
//! - Capability types (§16), sink types for tagged literals (§17) and `Secret[T]` (§18) are
//!   built in, because the spec's trait exclusions (§9.7) are facts about them.
//! - Generic parameters of the item being checked are rigid [`Ty::Param`]s; inference
//!   variables are [`Ty::Var`]s owned by [`crate::unify::InferCtx`].

use std::fmt;

use kalam_resolve::DefId;

use crate::traits::TraitRef;

/// Capability types (spec §16.1). They have no public constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Capability {
    World,
    Net,
    Fs,
    Env,
    Clock,
    Rand,
    Proc,
    Stdin,
    Screen,
}

impl Capability {
    /// The fixed ASCII type name (spec §7.2: capability types are never translated, except
    /// `World`, whose spelling the printer takes from the skin table).
    pub fn name(self) -> &'static str {
        match self {
            Capability::World => "World",
            Capability::Net => "Net",
            Capability::Fs => "Fs",
            Capability::Env => "Env",
            Capability::Clock => "Clock",
            Capability::Rand => "Rand",
            Capability::Proc => "Proc",
            Capability::Stdin => "Stdin",
            Capability::Screen => "Screen",
        }
    }
}

/// Safe string kinds (spec §17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SinkType {
    Sql,
    Html,
    Command,
    Url,
    Path,
    PathSegment,
    Regex,
}

impl SinkType {
    pub fn name(self) -> &'static str {
        match self {
            SinkType::Sql => "Sql",
            SinkType::Html => "Html",
            SinkType::Command => "Command",
            SinkType::Url => "Url",
            SinkType::Path => "Path",
            SinkType::PathSegment => "PathSegment",
            SinkType::Regex => "Regex",
        }
    }

    /// The type a built-in tagged literal produces (`sql"…"` → `Sql`, `cmd"…"` → `Command`).
    pub fn of_tag(tag: kalam_sinks::Tag) -> SinkType {
        use kalam_sinks::Tag;
        match tag {
            Tag::Sql => SinkType::Sql,
            Tag::Html => SinkType::Html,
            Tag::Cmd => SinkType::Command,
            Tag::Url => SinkType::Url,
            Tag::Path => SinkType::Path,
            Tag::Regex => SinkType::Regex,
        }
    }
}

/// The suspension bit of a function type (spec §15.3).
///
/// - `Sync`: written `@sync fn(…)`, or a function known not to suspend.
/// - `Suspending`: may suspend. Function types stored in data are `Suspending` unless
///   written `@sync` (DR-13).
/// - `Param(i)`: the `i`-th effect parameter of a [`Scheme`]. Every fn-typed *parameter* of a
///   declaration gets one, so the declaration is effect-polymorphic.
/// - `Var(i)`: an effect variable created by instantiation or for a lambda. Typeck never
///   solves it; it records [`crate::unify::SuspFlow`] constraints for `kalam-effects`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Suspension {
    Sync,
    Suspending,
    Param(u32),
    Var(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    BigInt,
    Float,
    Decimal,
    Bool,
    String,
    Bytes,
    Unit,
    Never,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Tuple(Vec<Ty>),
    /// `a..b` and `a..=b` (spec §10.5). Always `Range[Int]` in v0.1.
    Range(Box<Ty>),
    Fn {
        params: Vec<Ty>,
        ret: Box<Ty>,
        suspension: Suspension,
    },
    /// A user record or enum, with type arguments. Aliases are expanded before this point.
    Adt(DefId, Vec<Ty>),
    Capability(Capability),
    Sink(SinkType),
    Secret(Box<Ty>),
    AnyError,
    /// An opaque JS value from `extern` code (spec §19.2).
    JsValue,
    /// The `i`-th generic parameter of the enclosing item (rigid inside its body).
    Param(u32),
    /// An inference variable.
    Var(u32),
    /// Poison: unifies with everything and suppresses follow-on diagnostics.
    Error,
}

impl Ty {
    pub fn list(t: Ty) -> Ty {
        Ty::List(Box::new(t))
    }

    pub fn option(t: Ty) -> Ty {
        Ty::Option(Box::new(t))
    }

    pub fn result(t: Ty, e: Ty) -> Ty {
        Ty::Result(Box::new(t), Box::new(e))
    }

    pub fn func(params: Vec<Ty>, ret: Ty, suspension: Suspension) -> Ty {
        Ty::Fn {
            params,
            ret: Box::new(ret),
            suspension,
        }
    }

    /// `Int`, `BigInt`, `Float` or `Decimal`.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int | Ty::BigInt | Ty::Float | Ty::Decimal)
    }

    /// Direct children, in a fixed order.
    pub fn children(&self) -> Vec<&Ty> {
        match self {
            Ty::List(t) | Ty::Set(t) | Ty::Option(t) | Ty::Range(t) | Ty::Secret(t) => vec![t],
            Ty::Map(a, b) | Ty::Result(a, b) => vec![a, b],
            Ty::Tuple(ts) | Ty::Adt(_, ts) => ts.iter().collect(),
            Ty::Fn { params, ret, .. } => params.iter().chain(std::iter::once(&**ret)).collect(),
            _ => Vec::new(),
        }
    }

    /// True if `pred` holds for this type or any type inside it.
    pub fn any(&self, pred: &mut dyn FnMut(&Ty) -> bool) -> bool {
        if pred(self) {
            return true;
        }
        self.children().into_iter().any(|c| c.any(pred))
    }

    pub fn has_vars(&self) -> bool {
        self.any(&mut |t| matches!(t, Ty::Var(_)))
    }

    pub fn contains_secret(&self) -> bool {
        self.any(&mut |t| matches!(t, Ty::Secret(_)))
    }

    /// Rebuilds the type bottom-up, replacing each node with `f(node)` when it returns
    /// `Some`. Children of a replaced node are not visited.
    pub fn map(&self, f: &mut dyn FnMut(&Ty) -> Option<Ty>) -> Ty {
        if let Some(t) = f(self) {
            return t;
        }
        let b = |t: &Ty, f: &mut dyn FnMut(&Ty) -> Option<Ty>| Box::new(t.map(f));
        match self {
            Ty::List(t) => Ty::List(b(t, f)),
            Ty::Set(t) => Ty::Set(b(t, f)),
            Ty::Option(t) => Ty::Option(b(t, f)),
            Ty::Range(t) => Ty::Range(b(t, f)),
            Ty::Secret(t) => Ty::Secret(b(t, f)),
            Ty::Map(k, v) => {
                let k = b(k, f);
                Ty::Map(k, b(v, f))
            }
            Ty::Result(t, e) => {
                let t = b(t, f);
                Ty::Result(t, b(e, f))
            }
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|t| t.map(f)).collect()),
            Ty::Adt(d, ts) => Ty::Adt(*d, ts.iter().map(|t| t.map(f)).collect()),
            Ty::Fn {
                params,
                ret,
                suspension,
            } => {
                let params = params.iter().map(|t| t.map(f)).collect();
                Ty::Fn {
                    params,
                    ret: b(ret, f),
                    suspension: *suspension,
                }
            }
            other => other.clone(),
        }
    }

    /// Replaces `Param(i)` with `args[i]` (parameters without an argument stay).
    pub fn subst(&self, args: &[Ty]) -> Ty {
        self.map(&mut |t| match t {
            Ty::Param(i) => args.get(*i as usize).cloned(),
            _ => None,
        })
    }

    /// Renders the type with English-skin names. `name` gives the declared name of an ADT.
    pub fn display(&self, name: &dyn Fn(DefId) -> String) -> String {
        let args = |ts: &[Ty]| {
            ts.iter()
                .map(|t| t.display(name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Ty::Int => "Int".into(),
            Ty::BigInt => "BigInt".into(),
            Ty::Float => "Float".into(),
            Ty::Decimal => "Decimal".into(),
            Ty::Bool => "Bool".into(),
            Ty::String => "String".into(),
            Ty::Bytes => "Bytes".into(),
            Ty::Unit => "()".into(),
            Ty::Never => "Never".into(),
            Ty::List(t) => format!("List[{}]", t.display(name)),
            Ty::Set(t) => format!("Set[{}]", t.display(name)),
            Ty::Option(t) => format!("Option[{}]", t.display(name)),
            Ty::Range(t) => format!("Range[{}]", t.display(name)),
            Ty::Secret(t) => format!("Secret[{}]", t.display(name)),
            Ty::Map(k, v) => format!("Map[{}, {}]", k.display(name), v.display(name)),
            Ty::Result(t, e) => format!("Result[{}, {}]", t.display(name), e.display(name)),
            Ty::Tuple(ts) => format!("({})", args(ts)),
            Ty::Fn {
                params,
                ret,
                suspension,
            } => {
                let sync = if *suspension == Suspension::Sync {
                    "@sync "
                } else {
                    ""
                };
                format!("{sync}fn({}) -> {}", args(params), ret.display(name))
            }
            Ty::Adt(d, ts) if ts.is_empty() => name(*d),
            Ty::Adt(d, ts) => format!("{}[{}]", name(*d), args(ts)),
            Ty::Capability(c) => c.name().into(),
            Ty::Sink(s) => s.name().into(),
            Ty::AnyError => "AnyError".into(),
            Ty::JsValue => "JsValue".into(),
            Ty::Param(i) => format!("T{i}"),
            Ty::Var(_) => "_".into(),
            Ty::Error => "?".into(),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display(&|d| format!("#{}", d.0)))
    }
}

/// A generic parameter of a declaration, with its trait bounds (`T: Eq + Hash`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<TraitRef>,
}

/// A declared, possibly generic type: the only source of polymorphism in Kalam.
///
/// There is no let-generalization (§9.9 rule 2: local `let` is monomorphic; module
/// constants infer monomorphically). A scheme is built from an annotated signature: its
/// `generics` are the declared type parameters (referenced as `Ty::Param(i)`), and
/// `effect_params` counts the `Suspension::Param(i)` bits of its fn-typed parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub generics: Vec<GenericParam>,
    pub effect_params: u32,
    pub ty: Ty,
}

impl Scheme {
    pub fn mono(ty: Ty) -> Scheme {
        Scheme {
            generics: Vec::new(),
            effect_params: 0,
            ty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subst_replaces_params_and_display_is_readable() {
        let t = Ty::func(
            vec![Ty::list(Ty::Param(0)), Ty::Param(1)],
            Ty::option(Ty::Param(1)),
            Suspension::Param(0),
        );
        let s = t.subst(&[Ty::Int, Ty::String]);
        assert_eq!(s.to_string(), "fn(List[Int], String) -> Option[String]");
        let sync = Ty::func(vec![], Ty::Unit, Suspension::Sync);
        assert_eq!(sync.to_string(), "@sync fn() -> ()");
    }

    #[test]
    fn tagged_literal_types() {
        assert_eq!(SinkType::of_tag(kalam_sinks::Tag::Cmd), SinkType::Command);
        assert_eq!(SinkType::of_tag(kalam_sinks::Tag::Sql), SinkType::Sql);
    }

    #[test]
    fn contains_secret_looks_inside() {
        let t = Ty::Tuple(vec![Ty::Int, Ty::option(Ty::Secret(Box::new(Ty::String)))]);
        assert!(t.contains_secret());
        assert!(!Ty::list(Ty::Int).contains_secret());
    }
}
