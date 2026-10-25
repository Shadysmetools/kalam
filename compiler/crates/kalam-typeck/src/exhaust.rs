//! Exhaustiveness and usefulness of patterns (spec §11), after Maranget, "Warnings for
//! pattern matching" (JFP 2007).
//!
//! A `match` is a matrix with one row per unguarded arm (§11 rule 2: guarded arms do not
//! count toward exhaustiveness). An arm is **unreachable** (K1002) when its pattern is not
//! *useful* against the rows above it; the match is **non-exhaustive** (K1001) when the
//! wildcard is useful against all rows, and the witnesses of that are the missing cases.
//! The same test decides whether a `let` or `for` pattern is refutable (K1003).
//!
//! Constructors per type: `Bool` has `true`/`false`; `Option` `Some`/`None`; `Result`
//! `Ok`/`Err`; an enum its variants; tuples, records and `()` one constructor each. `Int`,
//! `String`, `Decimal`, `BigInt` and `Float` literals come from infinite domains, so only a
//! wildcard or binding covers them (§11 rule 6). Lists are split by length the way rustc
//! does: lengths `0..L` one by one plus "`L` or more", where `L` is one more than the
//! longest length any pattern of the column needs, so `[]` + `[x, ..rest]` is exhaustive.
//!
//! Range coverage is conservative: a range arm is reported unreachable only when a single
//! earlier range or wildcard covers it (no false K1002).

use std::collections::HashMap;

use kalam_resolve::DefId;

use crate::adt::{AdtDef, AdtKind};
use crate::hir::{LangItem, Pat, PatKind};
use crate::ty::Ty;

/// A constructor of a value.
#[derive(Clone, Debug, PartialEq)]
pub enum Ctor {
    Bool(bool),
    Some,
    None,
    Ok,
    Err,
    /// The `i`-th variant of an enum.
    Variant(usize),
    /// The single constructor of a tuple, a record or `()`.
    Single,
    Int(i128),
    /// An inclusive `Int` range.
    Range(i128, i128),
    Str(String),
    Float(String),
    /// A list of exactly this length.
    Len(usize),
    /// A list of at least this length (its first `n` elements are the fields).
    LenAtLeast(usize),
}

/// A pattern after deconstruction: or-patterns are kept only at this level.
#[derive(Clone, Debug, PartialEq)]
pub enum DPat {
    Wild,
    Ctor(Ctor, Vec<DPat>),
    /// A list pattern before length splitting: `before`, optional `..`, `after`.
    List {
        before: Vec<DPat>,
        rest: bool,
        after: Vec<DPat>,
    },
    Or(Vec<DPat>),
}

/// Everything the algorithm needs about types.
pub struct Cx<'a> {
    pub adts: &'a HashMap<DefId, AdtDef>,
}

/// The report for one `match`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatchReport {
    /// Missing cases as source text (`.Amber`, `Some(_)`, `_`), empty when exhaustive.
    pub missing: Vec<String>,
    /// Indices of unreachable arms.
    pub unreachable: Vec<usize>,
}

const MAX_WITNESSES: usize = 16;

impl Cx<'_> {
    fn enum_of(&self, ty: &Ty) -> Option<(&AdtDef, Vec<Ty>)> {
        match ty {
            Ty::Adt(d, args) => self.adts.get(d).map(|a| (a, args.clone())),
            _ => None,
        }
    }

    /// Converts a checked pattern. `ty` is the resolved type of the value it matches.
    pub fn lower(&self, p: &Pat, ty: &Ty) -> DPat {
        kalam_ast::stack::deeper(|| self.lower_inner(p, ty))
    }

    fn lower_inner(&self, p: &Pat, ty: &Ty) -> DPat {
        match &p.kind {
            PatKind::Wild | PatKind::Bind(_) | PatKind::Error => DPat::Wild,
            PatKind::As { pat, .. } => self.lower(pat, ty),
            PatKind::Bool(b) => DPat::Ctor(Ctor::Bool(*b), Vec::new()),
            PatKind::Int { negative, value } => {
                let v = (*value).min(i128::MAX as u128) as i128;
                DPat::Ctor(Ctor::Int(if *negative { -v } else { v }), Vec::new())
            }
            PatKind::Range { lo, hi, inclusive } => {
                let hi = if *inclusive {
                    *hi
                } else {
                    hi.saturating_sub(1)
                };
                DPat::Ctor(Ctor::Range(*lo, hi), Vec::new())
            }
            PatKind::Str(s) => DPat::Ctor(Ctor::Str(s.clone()), Vec::new()),
            PatKind::Float(f) => DPat::Ctor(Ctor::Float(f.clone()), Vec::new()),
            PatKind::Tuple(ps) => {
                let tys = match ty {
                    Ty::Tuple(ts) if ts.len() == ps.len() => ts.clone(),
                    _ => vec![Ty::Error; ps.len()],
                };
                let fields = ps.iter().zip(&tys).map(|(p, t)| self.lower(p, t)).collect();
                DPat::Ctor(Ctor::Single, fields)
            }
            PatKind::Lang { item, inner } => {
                let (ctor, inner_ty) = match (item, ty) {
                    (LangItem::Some, Ty::Option(t)) => (Ctor::Some, (**t).clone()),
                    (LangItem::None, _) => (Ctor::None, Ty::Error),
                    (LangItem::Ok, Ty::Result(t, _)) => (Ctor::Ok, (**t).clone()),
                    (LangItem::Err, Ty::Result(_, e)) => (Ctor::Err, (**e).clone()),
                    (LangItem::Some, _) => (Ctor::Some, Ty::Error),
                    (LangItem::Ok, _) => (Ctor::Ok, Ty::Error),
                    (LangItem::Err, _) => (Ctor::Err, Ty::Error),
                    _ => return DPat::Wild,
                };
                let fields = match ctor {
                    Ctor::None => Vec::new(),
                    _ => vec![inner
                        .as_ref()
                        .map_or(DPat::Wild, |p| self.lower(p, &inner_ty))],
                };
                DPat::Ctor(ctor, fields)
            }
            PatKind::Variant {
                def,
                name,
                fields,
                rest: _,
            } => {
                let Some((adt, args)) = self.enum_of(ty) else {
                    return DPat::Wild;
                };
                let AdtKind::Enum(variants) = &adt.kind else {
                    return DPat::Wild;
                };
                let idx = match def {
                    Some(d) => variants.iter().position(|v| v.def == *d),
                    None => variants.iter().position(|v| v.name == *name),
                };
                let Some(idx) = idx else {
                    return DPat::Wild;
                };
                let v = &variants[idx];
                let mut out = vec![DPat::Wild; v.fields.len()];
                for (i, f) in fields.iter().enumerate() {
                    let slot = match &f.label {
                        Some(l) => v
                            .fields
                            .iter()
                            .position(|vf| vf.label.as_deref() == Some(l)),
                        None => (i < v.fields.len()).then_some(i),
                    };
                    if let Some(s) = slot {
                        out[s] = self.lower(&f.pat, &v.fields[s].ty.subst(&args));
                    }
                }
                DPat::Ctor(Ctor::Variant(idx), out)
            }
            PatKind::Record { fields, .. } => {
                let Some((adt, args)) = self.enum_of(ty) else {
                    return DPat::Wild;
                };
                let AdtKind::Record(decl) = &adt.kind else {
                    return DPat::Wild;
                };
                let mut out = vec![DPat::Wild; decl.len()];
                for f in fields {
                    let slot = f
                        .label
                        .as_ref()
                        .and_then(|l| decl.iter().position(|d| &d.name == l));
                    if let Some(s) = slot {
                        out[s] = self.lower(&f.pat, &decl[s].ty.subst(&args));
                    }
                }
                DPat::Ctor(Ctor::Single, out)
            }
            PatKind::List {
                before,
                rest,
                after,
            } => {
                let elem = match ty {
                    Ty::List(t) => (**t).clone(),
                    _ => Ty::Error,
                };
                DPat::List {
                    before: before.iter().map(|p| self.lower(p, &elem)).collect(),
                    rest: rest.is_some(),
                    after: after.iter().map(|p| self.lower(p, &elem)).collect(),
                }
            }
            PatKind::Or(alts) => DPat::Or(alts.iter().map(|p| self.lower(p, ty)).collect()),
        }
    }

    /// The field types of `c` for a value of type `ty`.
    fn fields(&self, ty: &Ty, c: &Ctor) -> Vec<Ty> {
        match (c, ty) {
            (Ctor::Some, Ty::Option(t)) | (Ctor::Ok, Ty::Result(t, _)) => vec![(**t).clone()],
            (Ctor::Err, Ty::Result(_, e)) => vec![(**e).clone()],
            (Ctor::Some | Ctor::Ok | Ctor::Err, _) => vec![Ty::Error],
            (Ctor::Single, Ty::Tuple(ts)) => ts.clone(),
            (Ctor::Single, Ty::Adt(..)) | (Ctor::Variant(_), Ty::Adt(..)) => {
                let Some((adt, args)) = self.enum_of(ty) else {
                    return Vec::new();
                };
                match (&adt.kind, c) {
                    (AdtKind::Record(fs), Ctor::Single) => {
                        fs.iter().map(|f| f.ty.subst(&args)).collect()
                    }
                    (AdtKind::Enum(vs), Ctor::Variant(i)) => vs
                        .get(*i)
                        .map(|v| v.fields.iter().map(|f| f.ty.subst(&args)).collect())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                }
            }
            (Ctor::Len(n) | Ctor::LenAtLeast(n), Ty::List(t)) => vec![(**t).clone(); *n],
            (Ctor::Len(n) | Ctor::LenAtLeast(n), _) => vec![Ty::Error; *n],
            _ => Vec::new(),
        }
    }

    /// Every constructor of `ty`, when the set is finite.
    fn all_ctors(&self, ty: &Ty) -> Option<Vec<Ctor>> {
        Some(match ty {
            Ty::Bool => vec![Ctor::Bool(true), Ctor::Bool(false)],
            Ty::Option(_) => vec![Ctor::None, Ctor::Some],
            Ty::Result(_, _) => vec![Ctor::Ok, Ctor::Err],
            Ty::Tuple(_) | Ty::Unit => vec![Ctor::Single],
            Ty::Adt(..) => match self.enum_of(ty).map(|(a, _)| &a.kind) {
                Some(AdtKind::Enum(vs)) => (0..vs.len()).map(Ctor::Variant).collect(),
                Some(AdtKind::Record(_)) => vec![Ctor::Single],
                _ => return None,
            },
            _ => return None,
        })
    }
}

/// Expands top-level or-patterns of a row.
fn expand(row: &[DPat]) -> Vec<Vec<DPat>> {
    match row.first() {
        Some(DPat::Or(alts)) => alts
            .iter()
            .flat_map(|a| {
                let mut r = vec![a.clone()];
                r.extend_from_slice(&row[1..]);
                expand(&r)
            })
            .collect(),
        _ => vec![row.to_vec()],
    }
}

fn head_covers(row_head: &Ctor, c: &Ctor) -> bool {
    match (row_head, c) {
        (Ctor::Range(lo, hi), Ctor::Int(v)) => lo <= v && v <= hi,
        (Ctor::Range(lo, hi), Ctor::Range(a, b)) => lo <= a && b <= hi,
        (Ctor::Int(v), Ctor::Range(a, b)) => a == b && a == v,
        _ => row_head == c,
    }
}

/// The list length constructors a column needs (rustc's split).
fn list_ctors(rows: &[Vec<DPat>]) -> Vec<Ctor> {
    let mut max = 0;
    for r in rows {
        if let Some(DPat::List {
            before,
            rest,
            after,
        }) = r.first()
        {
            let n = before.len() + after.len();
            max = max.max(if *rest { n } else { n + 1 });
        }
    }
    let mut out: Vec<Ctor> = (0..max).map(Ctor::Len).collect();
    out.push(Ctor::LenAtLeast(max));
    out
}

/// The fields a list pattern has under a length constructor, or `None` if it does not
/// match that length.
fn list_fields(before: &[DPat], rest: bool, after: &[DPat], c: &Ctor) -> Option<Vec<DPat>> {
    let (n, at_least) = match c {
        Ctor::Len(n) => (*n, false),
        Ctor::LenAtLeast(n) => (*n, true),
        _ => return None,
    };
    let need = before.len() + after.len();
    if !rest {
        return (!at_least && need == n).then(|| [before, after].concat());
    }
    if need > n {
        return None;
    }
    let mut out = before.to_vec();
    out.extend(std::iter::repeat(DPat::Wild).take(n - need));
    out.extend_from_slice(after);
    Some(out)
}

/// The rows that match constructor `c` in their first column, specialized.
fn specialize(rows: &[Vec<DPat>], c: &Ctor, arity: usize) -> Vec<Vec<DPat>> {
    let mut out = Vec::new();
    for row in rows {
        let Some((head, rest)) = row.split_first() else {
            continue;
        };
        let fields = match head {
            DPat::Wild => Some(vec![DPat::Wild; arity]),
            DPat::Ctor(rc, args) if head_covers(rc, c) => {
                let mut a = args.clone();
                a.resize(arity, DPat::Wild);
                Some(a)
            }
            DPat::Ctor(..) => None,
            DPat::List {
                before,
                rest: r,
                after,
            } => list_fields(before, *r, after, c),
            DPat::Or(_) => None,
        };
        if let Some(mut f) = fields {
            f.extend_from_slice(rest);
            out.push(f);
        }
    }
    out
}

/// Rows whose first column is a wildcard, without that column.
fn default_rows(rows: &[Vec<DPat>]) -> Vec<Vec<DPat>> {
    rows.iter()
        .filter(|r| matches!(r.first(), Some(DPat::Wild)))
        .map(|r| r[1..].to_vec())
        .collect()
}

fn head_ctors(rows: &[Vec<DPat>]) -> Vec<Ctor> {
    let mut out: Vec<Ctor> = Vec::new();
    for r in rows {
        if let Some(DPat::Ctor(c, _)) = r.first() {
            if !out.contains(c) {
                out.push(c.clone());
            }
        }
    }
    out
}

fn has_list_head(rows: &[Vec<DPat>]) -> bool {
    rows.iter()
        .any(|r| matches!(r.first(), Some(DPat::List { .. })))
}

impl Cx<'_> {
    fn expand_all(rows: &[Vec<DPat>]) -> Vec<Vec<DPat>> {
        rows.iter().flat_map(|r| expand(r)).collect()
    }

    /// Is `q` useful with respect to `rows`? (Maranget's `U`.)
    pub fn useful(&self, rows: &[Vec<DPat>], q: &[DPat], tys: &[Ty]) -> bool {
        let rows = Self::expand_all(rows);
        self.useful_rec(&rows, q, tys, 0)
    }

    fn useful_rec(&self, rows: &[Vec<DPat>], q: &[DPat], tys: &[Ty], depth: u32) -> bool {
        // Pathological nesting cannot recurse without bound: assume useful (no K1002).
        if depth > 256 {
            return true;
        }
        let Some((qh, qr)) = q.split_first() else {
            return rows.is_empty();
        };
        let ty = tys.first().cloned().unwrap_or(Ty::Error);
        let rest_tys = tys.get(1..).unwrap_or(&[]);
        match qh {
            DPat::Or(alts) => alts.iter().any(|a| {
                let mut q2 = vec![a.clone()];
                q2.extend_from_slice(qr);
                self.useful_rec(rows, &q2, tys, depth + 1)
            }),
            DPat::Ctor(c, args) => {
                let fields = self.fields(&ty, c);
                let arity = args.len().max(fields.len());
                let mut q2 = args.clone();
                q2.resize(arity, DPat::Wild);
                q2.extend_from_slice(qr);
                let mut tys2 = fields;
                tys2.resize(arity, Ty::Error);
                tys2.extend_from_slice(rest_tys);
                let spec = Self::expand_all(&specialize(rows, c, arity));
                self.useful_rec(&spec, &q2, &tys2, depth + 1)
            }
            DPat::List {
                before,
                rest,
                after,
            } => {
                let mut all = rows.to_vec();
                all.push(q.to_vec());
                list_ctors(&all).iter().any(|c| {
                    let Some(fields) = list_fields(before, *rest, after, c) else {
                        return false;
                    };
                    let mut q2 = fields;
                    q2.extend_from_slice(qr);
                    let arity = q2.len() - qr.len();
                    let mut tys2 = self.fields(&ty, c);
                    tys2.resize(arity, Ty::Error);
                    tys2.extend_from_slice(rest_tys);
                    let spec = Self::expand_all(&specialize(rows, c, arity));
                    self.useful_rec(&spec, &q2, &tys2, depth + 1)
                })
            }
            DPat::Wild => {
                let ctors = if has_list_head(rows) {
                    Some(list_ctors(rows))
                } else {
                    self.all_ctors(&ty)
                };
                let used = head_ctors(rows);
                let complete = match &ctors {
                    Some(all) => {
                        has_list_head(rows)
                            || (!all.is_empty() && all.iter().all(|c| used.contains(c)))
                    }
                    None => false,
                };
                if complete {
                    let all = ctors.unwrap_or_default();
                    all.iter().any(|c| {
                        let fields = self.fields(&ty, c);
                        let arity = fields.len();
                        let mut q2 = vec![DPat::Wild; arity];
                        q2.extend_from_slice(qr);
                        let mut tys2 = fields;
                        tys2.extend_from_slice(rest_tys);
                        let spec = Self::expand_all(&specialize(rows, c, arity));
                        self.useful_rec(&spec, &q2, &tys2, depth + 1)
                    })
                } else {
                    self.useful_rec(&default_rows(rows), qr, rest_tys, depth + 1)
                }
            }
        }
    }

    /// The patterns no row matches (each a vector of one pattern per column), at most
    /// [`MAX_WITNESSES`].
    pub fn missing(&self, rows: &[Vec<DPat>], tys: &[Ty]) -> Vec<Vec<DPat>> {
        let rows = Self::expand_all(rows);
        let mut out = Vec::new();
        self.witnesses(&rows, tys, 0, &mut out);
        out
    }

    fn witnesses(&self, rows: &[Vec<DPat>], tys: &[Ty], depth: u32, out: &mut Vec<Vec<DPat>>) {
        if out.len() >= MAX_WITNESSES || depth > 256 {
            return;
        }
        let Some(ty) = tys.first() else {
            if rows.is_empty() {
                out.push(Vec::new());
            }
            return;
        };
        let rest_tys = &tys[1..];
        let list = has_list_head(rows) || matches!(ty, Ty::List(_)) && has_list_head(rows);
        let ctors = if list {
            Some(list_ctors(rows))
        } else {
            self.all_ctors(ty)
        };
        let used = head_ctors(rows);
        let complete = match &ctors {
            Some(all) => list || (!all.is_empty() && all.iter().all(|c| used.contains(c))),
            None => false,
        };
        if complete {
            for c in ctors.unwrap_or_default() {
                let fields = self.fields(ty, &c);
                let arity = fields.len();
                let mut tys2 = fields;
                tys2.extend_from_slice(rest_tys);
                let spec = Self::expand_all(&specialize(rows, &c, arity));
                let mut sub = Vec::new();
                self.witnesses(&spec, &tys2, depth + 1, &mut sub);
                for w in sub {
                    if out.len() >= MAX_WITNESSES {
                        return;
                    }
                    let (args, rest) = w.split_at(arity.min(w.len()));
                    let mut row = vec![DPat::Ctor(c.clone(), args.to_vec())];
                    row.extend_from_slice(rest);
                    out.push(row);
                }
            }
            return;
        }
        let mut sub = Vec::new();
        self.witnesses(&default_rows(rows), rest_tys, depth + 1, &mut sub);
        if sub.is_empty() {
            return;
        }
        // The heads to report: every missing constructor of a finite type (with wildcard
        // fields), or `_` for an infinite one.
        let heads: Vec<DPat> = match &ctors {
            Some(all) if !used.is_empty() => all
                .iter()
                .filter(|c| !used.contains(c))
                .map(|c| {
                    let n = self.fields(ty, c).len();
                    DPat::Ctor(c.clone(), vec![DPat::Wild; n])
                })
                .collect(),
            Some(all) if all.len() > 1 => all
                .iter()
                .map(|c| {
                    let n = self.fields(ty, c).len();
                    DPat::Ctor(c.clone(), vec![DPat::Wild; n])
                })
                .collect(),
            _ => vec![DPat::Wild],
        };
        for w in sub {
            for h in &heads {
                if out.len() >= MAX_WITNESSES {
                    return;
                }
                let mut row = vec![h.clone()];
                row.extend_from_slice(&w);
                out.push(row);
            }
        }
    }

    /// Source text for a witness of type `ty` (English core-name spellings).
    pub fn show(&self, p: &DPat, ty: &Ty) -> String {
        kalam_ast::stack::deeper(|| self.show_inner(p, ty))
    }

    fn show_inner(&self, p: &DPat, ty: &Ty) -> String {
        match p {
            DPat::Wild => "_".to_string(),
            DPat::Or(alts) => alts
                .iter()
                .map(|a| self.show(a, ty))
                .collect::<Vec<_>>()
                .join(" | "),
            DPat::List { .. } => "[..]".to_string(),
            DPat::Ctor(c, args) => {
                let tys = self.fields(ty, c);
                let shown: Vec<String> = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| self.show(a, tys.get(i).unwrap_or(&Ty::Error)))
                    .collect();
                let paren = |name: &str| {
                    if shown.is_empty() {
                        name.to_string()
                    } else {
                        format!("{name}({})", shown.join(", "))
                    }
                };
                match c {
                    Ctor::Bool(b) => b.to_string(),
                    Ctor::Some => paren("Some"),
                    Ctor::None => "None".into(),
                    Ctor::Ok => paren("Ok"),
                    Ctor::Err => paren("Err"),
                    Ctor::Variant(i) => {
                        let name = self
                            .enum_of(ty)
                            .and_then(|(a, _)| match &a.kind {
                                AdtKind::Enum(vs) => vs.get(*i).map(|v| v.name.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| "_".into());
                        paren(&format!(".{name}"))
                    }
                    Ctor::Single => match ty {
                        Ty::Adt(..) => {
                            let name = self.enum_of(ty).map(|(a, _)| a.name.clone());
                            match name {
                                Some(n) => format!("{n}(..)"),
                                None => "_".into(),
                            }
                        }
                        Ty::Unit => "()".into(),
                        _ => format!("({})", shown.join(", ")),
                    },
                    Ctor::Int(v) => v.to_string(),
                    Ctor::Range(a, b) => format!("{a}..={b}"),
                    Ctor::Str(s) => format!("{s:?}"),
                    Ctor::Float(f) => f.clone(),
                    Ctor::Len(_) => format!("[{}]", shown.join(", ")),
                    Ctor::LenAtLeast(_) => {
                        let mut parts = shown;
                        parts.push("..".into());
                        format!("[{}]", parts.join(", "))
                    }
                }
            }
        }
    }

    /// Exhaustiveness and usefulness of one `match`: `arms` are the lowered patterns, each
    /// with whether it has a guard.
    pub fn check_match(&self, scrut: &Ty, arms: &[(DPat, bool)]) -> MatchReport {
        let mut report = MatchReport::default();
        let mut rows: Vec<Vec<DPat>> = Vec::new();
        for (i, (p, guarded)) in arms.iter().enumerate() {
            if !self.useful(&rows, std::slice::from_ref(p), std::slice::from_ref(scrut)) {
                report.unreachable.push(i);
            }
            if !guarded {
                rows.push(vec![p.clone()]);
            }
        }
        for w in self.missing(&rows, std::slice::from_ref(scrut)) {
            if let Some(p) = w.first() {
                let text = self.show(p, scrut);
                if !report.missing.contains(&text) {
                    report.missing.push(text);
                }
            }
        }
        report
    }

    /// True when `p` matches every value of `ty` (an irrefutable `let`/`for` pattern).
    pub fn irrefutable(&self, p: &DPat, ty: &Ty) -> bool {
        let rows = vec![vec![p.clone()]];
        !self.useful(&rows, &[DPat::Wild], std::slice::from_ref(ty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adt::VariantDef;
    use kalam_resolve::ModulePath;

    fn light() -> (HashMap<DefId, AdtDef>, Ty) {
        let mut adts = HashMap::new();
        let vs = ["Red", "Amber", "Green"]
            .iter()
            .enumerate()
            .map(|(i, n)| VariantDef {
                def: DefId(10 + i as u32),
                name: n.to_string(),
                fields: Vec::new(),
            })
            .collect();
        adts.insert(
            DefId(1),
            AdtDef {
                def: DefId(1),
                name: "Light".into(),
                module: ModulePath("m".into()),
                generics: Vec::new(),
                kind: AdtKind::Enum(vs),
                opaque: false,
                derives: Vec::new(),
            },
        );
        (adts, Ty::Adt(DefId(1), Vec::new()))
    }

    fn v(i: usize) -> DPat {
        DPat::Ctor(Ctor::Variant(i), Vec::new())
    }

    #[test]
    fn missing_variants_and_unreachable_arms() {
        let (adts, ty) = light();
        let cx = Cx { adts: &adts };
        let r = cx.check_match(&ty, &[(v(0), false), (v(2), false)]);
        assert_eq!(r.missing, vec![".Amber".to_string()]);
        assert!(r.unreachable.is_empty());
        let r = cx.check_match(&ty, &[(DPat::Wild, false), (v(0), false)]);
        assert!(r.missing.is_empty());
        assert_eq!(r.unreachable, vec![1]);
        // A guarded arm does not cover.
        let r = cx.check_match(&ty, &[(DPat::Wild, true), (v(0), false)]);
        assert_eq!(r.missing, vec![".Amber".to_string(), ".Green".to_string()]);
        assert!(r.unreachable.is_empty());
    }

    #[test]
    fn nested_options_and_infinite_domains() {
        let adts = HashMap::new();
        let cx = Cx { adts: &adts };
        let ty = Ty::option(Ty::Bool);
        let some = |b: bool| DPat::Ctor(Ctor::Some, vec![DPat::Ctor(Ctor::Bool(b), vec![])]);
        let none = DPat::Ctor(Ctor::None, vec![]);
        let r = cx.check_match(&ty, &[(some(true), false), (none.clone(), false)]);
        assert_eq!(r.missing, vec!["Some(false)".to_string()]);
        let ints = Ty::Int;
        let one = DPat::Ctor(Ctor::Int(1), vec![]);
        let r = cx.check_match(&ints, &[(one.clone(), false), (one, false)]);
        assert_eq!(r.missing, vec!["_".to_string()]);
        assert_eq!(r.unreachable, vec![1]);
        let range = DPat::Ctor(Ctor::Range(1, 9), vec![]);
        let five = DPat::Ctor(Ctor::Int(5), vec![]);
        let r = cx.check_match(&ints, &[(range, false), (five, false), (DPat::Wild, false)]);
        assert_eq!(r.unreachable, vec![1], "5 is inside 1..=9");
    }

    #[test]
    fn list_lengths_split() {
        let adts = HashMap::new();
        let cx = Cx { adts: &adts };
        let ty = Ty::list(Ty::Int);
        let empty = DPat::List {
            before: vec![],
            rest: false,
            after: vec![],
        };
        let cons = DPat::List {
            before: vec![DPat::Wild],
            rest: true,
            after: vec![],
        };
        let r = cx.check_match(&ty, &[(empty.clone(), false), (cons, false)]);
        assert!(r.missing.is_empty(), "{r:?}");
        let r = cx.check_match(&ty, &[(empty, false)]);
        assert_eq!(r.missing.len(), 1, "{r:?}");
        assert!(cx.irrefutable(&DPat::Wild, &ty));
    }
}
