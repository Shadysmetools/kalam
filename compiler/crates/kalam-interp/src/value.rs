//! Runtime values. Every value is immutable (§9.1), so compound values share their parts
//! through `Rc` and "updates" build new values.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use kalam_ir::FnId;

use crate::num::{BigInt, Decimal};

#[derive(Clone, Debug)]
pub enum Value {
    Int(i64),
    Float(f64),
    Decimal(Rc<Decimal>),
    BigInt(Rc<BigInt>),
    Bool(bool),
    Str(Rc<str>),
    Unit,
    Tuple(Rc<[Value]>),
    /// A record's fields in declaration order.
    Record(Rc<[Value]>),
    /// An enum, `Option` or `Result` value: the variant number and its payload.
    Variant(u32, Rc<[Value]>),
    List(Rc<Vec<Value>>),
    Map(Rc<OrderedMap>),
    /// A set is a map from each element to `()`.
    Set(Rc<OrderedMap>),
    Fn(FnId),
    Closure(Rc<Closure>),
    /// A capability (`World` fields); no operation of the M2 interpreter uses one.
    Cap(&'static str),
}

/// A lambda or local `fn` (by its index in the interpreter's lambda table) with the
/// enclosing frame it captured, by value (§12.3).
#[derive(Debug)]
pub struct Closure {
    pub lambda: usize,
    pub env: Vec<Option<Value>>,
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }

    pub fn variant(tag: u32, fields: Vec<Value>) -> Value {
        Value::Variant(tag, Rc::from(fields))
    }

    pub fn none() -> Value {
        Value::variant(0, Vec::new())
    }

    pub fn some(v: Value) -> Value {
        Value::variant(1, vec![v])
    }

    pub fn ok(v: Value) -> Value {
        Value::variant(0, vec![v])
    }

    pub fn err(v: Value) -> Value {
        Value::variant(1, vec![v])
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(items))
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Decimal(_) => "Decimal",
            Value::BigInt(_) => "BigInt",
            Value::Bool(_) => "Bool",
            Value::Str(_) => "String",
            Value::Unit => "()",
            Value::Tuple(_) => "tuple",
            Value::Record(_) => "record",
            Value::Variant(..) => "variant",
            Value::List(_) => "List",
            Value::Map(_) => "Map",
            Value::Set(_) => "Set",
            Value::Fn(_) | Value::Closure(_) => "function",
            Value::Cap(_) => "capability",
        }
    }

    /// Whether this value can be a `Map` key or `Set` element (`Eq + Hash`, §9.3).
    pub fn hashable(&self) -> bool {
        match self {
            Value::Float(_) | Value::Fn(_) | Value::Closure(_) | Value::Cap(_) => false,
            Value::Tuple(vs) | Value::Record(vs) | Value::Variant(_, vs) => {
                vs.iter().all(Value::hashable)
            }
            Value::List(vs) => vs.iter().all(Value::hashable),
            Value::Map(m) | Value::Set(m) => {
                m.entries.iter().all(|(k, v)| k.hashable() && v.hashable())
            }
            _ => true,
        }
    }
}

/// Structural equality (§9.8, §11 rule 11: never runs user code). `Float` compares with
/// IEEE `==`; functions and capabilities are never equal (typeck rejects comparing them).
impl PartialEq for Value {
    fn eq(&self, o: &Value) -> bool {
        match (self, o) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Decimal(a), Value::Decimal(b)) => a == b,
            (Value::BigInt(a), Value::BigInt(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Tuple(a), Value::Tuple(b)) | (Value::Record(a), Value::Record(b)) => a == b,
            (Value::Variant(t, a), Value::Variant(u, b)) => t == u && a == b,
            (Value::List(a), Value::List(b)) => a == b,
            // Insertion order is not part of a map's or set's value.
            (Value::Map(a), Value::Map(b)) | (Value::Set(a), Value::Set(b)) => {
                a.len() == b.len()
                    && a.entries
                        .iter()
                        .all(|(k, v)| b.get(k).is_some_and(|w| w == v))
            }
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, h: &mut H) {
        std::mem::discriminant(self).hash(h);
        match self {
            Value::Int(i) => i.hash(h),
            Value::Float(f) => f.to_bits().hash(h),
            Value::Decimal(d) => d.canonical().hash(h),
            Value::BigInt(b) => b.hash(h),
            Value::Bool(b) => b.hash(h),
            Value::Str(s) => s.hash(h),
            Value::Tuple(vs) | Value::Record(vs) => vs.hash(h),
            Value::Variant(t, vs) => {
                t.hash(h);
                vs.hash(h);
            }
            Value::List(vs) => vs.hash(h),
            // Order-independent, like equality.
            Value::Map(m) | Value::Set(m) => m.len().hash(h),
            Value::Unit | Value::Fn(_) | Value::Closure(_) | Value::Cap(_) => {}
        }
    }
}

/// The order of `Ord` values (§9.7 `@derive(Ord)`: lexicographic by declaration order,
/// enums by variant order). `None` for values with no order.
pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    Some(match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Decimal(x), Value::Decimal(y)) => x.cmp(y),
        (Value::BigInt(x), Value::BigInt(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Unit, Value::Unit) => Ordering::Equal,
        (Value::Tuple(x), Value::Tuple(y)) | (Value::Record(x), Value::Record(y)) => {
            lexicographic(x, y)?
        }
        (Value::List(x), Value::List(y)) => lexicographic(x, y)?,
        (Value::Variant(t, x), Value::Variant(u, y)) => match t.cmp(u) {
            Ordering::Equal => lexicographic(x, y)?,
            other => other,
        },
        _ => return None,
    })
}

fn lexicographic(x: &[Value], y: &[Value]) -> Option<Ordering> {
    for (a, b) in x.iter().zip(y) {
        match compare(a, b)? {
            Ordering::Equal => {}
            other => return Some(other),
        }
    }
    Some(x.len().cmp(&y.len()))
}

/// An insertion-ordered map with structural keys (§9.3). Removing a key keeps the order of
/// the others.
#[derive(Clone, Debug, Default)]
pub struct OrderedMap {
    entries: Vec<(Value, Value)>,
    index: HashMap<Value, usize>,
}

impl OrderedMap {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, k: &Value) -> Option<&Value> {
        self.index.get(k).map(|&i| &self.entries[i].1)
    }

    /// Replaces the value of an existing key in place; a new key goes last.
    pub fn insert(&mut self, k: Value, v: Value) {
        match self.index.get(&k) {
            Some(&i) => self.entries[i].1 = v,
            None => {
                self.index.insert(k.clone(), self.entries.len());
                self.entries.push((k, v));
            }
        }
    }

    pub fn remove(&mut self, k: &Value) {
        if let Some(i) = self.index.remove(k) {
            self.entries.remove(i);
            for (j, (key, _)) in self.entries.iter().enumerate().skip(i) {
                self.index.insert(key.clone(), j);
            }
        }
    }

    pub fn entries(&self) -> &[(Value, Value)] {
        &self.entries
    }

    pub fn keys(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().map(|(k, _)| k)
    }
}
