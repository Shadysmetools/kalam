//! Suspension inference (spec §15.2–§15.4, ARCHITECTURE.md §9.1).
//!
//! **Input.** The type checker gives every function type a bit ([`Suspension`]) and records a
//! [`kalam_typeck::unify::SuspFlow`] `from ≤ to` wherever a function value flows into a
//! slot; it never solves them. Inside one body, effect variables `Var(k)` are numbered by
//! that body's inference context, and lambdas and local `fn`s share it. HIR drops the
//! internal stdlib attributes, so [`base_attrs`] reads them from the AST.
//!
//! **Model.** A bit is a [`SuspBit`] `always ∨ ⋁ params` over the effect parameters of the
//! declaration that owns the body (spec §15.3: `map` suspends exactly when its `f` does).
//! Per body, the nodes are the owner's own bit, each effect variable, each effect parameter,
//! and one node per stored function value (`Suspending`, DR-13). The edges are the flows and
//! one edge per call, from the callee's bit to the bit of the enclosing closure: a call inside
//! a lambda or a local `fn` makes *that* closure suspend, not the function around it.
//! Each instantiation of a declaration `g` (a call `g(…)`, a method call, or `g` used as a
//! value) defines `g`'s own bit there as `summary(g)` with its effect parameters replaced
//! by the bits of the arguments.
//!
//! **Fixpoint.** Summaries start at "sync" and only grow, so a worklist over the call graph
//! (callees first; a body is solved again when a callee's summary grows) reaches the least
//! fixpoint. That is the SCC fixpoint of spec §15.2: mutual recursion stays sync unless a
//! cycle member reaches a suspending call.
//!
//! **Base set and stubs.** `@suspends` stdlib primitives and `@js(async: true)` externs
//! always suspend. A body-less or `@builtin` declaration is never inferred from its
//! placeholder body: `@stores_callbacks` makes it sync, and otherwise it suspends exactly
//! when one of its non-`@sync` function parameters does (the `api-index.json` rule).
//!
//! **Trait methods (provisional, a founder fork).** A call that dispatches through a
//! trait's method (a generic receiver `T: Tr`) suspends if the trait's default body or any
//! impl of the method in the program does.
//!
//! **Checks.** K1401: a flow into a `@sync` slot whose source may suspend. K1402: a `@sync`
//! declaration whose body may suspend. A flow into `@sync` from an effect parameter (or a
//! `@sync` declaration calling one) is not an error at the definition: the parameter
//! becomes sync-required, and K1401 is reported where a suspending function is passed for
//! it. W0421: a value read with `Cell.get()` before a suspension point and used after it, in
//! straight-line code of one closure. Each diagnostic carries the shortest-cause call chain.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use kalam_ast as ast;
use kalam_base::{FileId, Span};
use kalam_diagnostics::Diagnostic;
use kalam_resolve::{DefId, Program, SourceModule};
use kalam_typeck::hir::{
    Arg, BodyValue, Expr, ExprId, ExprKind, FnDecl, HirProgram, ItemKind, LocalId, PatKind, Stmt,
    StmtKind, StrPart, TypeKind,
};
use kalam_typeck::traits::TraitId;
use kalam_typeck::{Scheme, SuspBit, Suspension, Suspensions, Ty, TypeckResults};

// ---------------------------------------------------------------------------------------
// Base attributes
// ---------------------------------------------------------------------------------------

/// The declarations whose bit is fixed by an attribute instead of inferred from a body.
#[derive(Clone, Debug, Default)]
pub struct BaseAttrs {
    /// `@suspends` stdlib primitives and `@js(async: true)` externs (spec §15.2).
    pub suspends: HashSet<DefId>,
    /// `@stores_callbacks`: function parameters are stored, never called (sync).
    pub stores_callbacks: HashSet<DefId>,
    /// `@builtin`: the body is a placeholder; the bit comes from the signature.
    pub builtin: HashSet<DefId>,
}

/// Reads [`BaseAttrs`] from the modules given to `kalam_resolve::resolve_program`.
/// `@suspends`, `@stores_callbacks` and `@builtin` are internal to the stdlib and count only
/// in stdlib modules; `@js(async: true)` counts on any `extern fn`.
pub fn base_attrs(program: &Program, sources: &[SourceModule<'_>]) -> BaseAttrs {
    let mut out = BaseAttrs::default();
    for src in sources {
        let mut note = |name: Span, attrs: &[ast::Attribute], is_extern: bool| {
            let Some(def) = program.decl_at(src.file, name) else {
                return;
            };
            if src.std && has_attr(attrs, "suspends") || is_extern && js_async(attrs) {
                out.suspends.insert(def);
            }
            if src.std && has_attr(attrs, "stores_callbacks") {
                out.stores_callbacks.insert(def);
            }
            if src.std && has_attr(attrs, "builtin") {
                out.builtin.insert(def);
            }
        };
        for item in &src.ast.file.items {
            match &item.kind {
                ast::ItemKind::Fn(f) => note(f.sig.name.span, &item.attrs, false),
                ast::ItemKind::Impl(imp) => {
                    for m in &imp.members {
                        note(m.func.sig.name.span, &m.attrs, false);
                    }
                }
                ast::ItemKind::Extern(ast::ExternDecl::Fn(sig)) => {
                    note(sig.name.span, &item.attrs, true)
                }
                _ => {}
            }
        }
    }
    out
}

fn has_attr(attrs: &[ast::Attribute], name: &str) -> bool {
    attrs
        .iter()
        .any(|a| a.path.len() == 1 && a.path[0].name == name)
}

/// `@js(…, async: true, …)` (spec §19.1).
fn js_async(attrs: &[ast::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|a| a.path.len() == 1 && a.path[0].name == "js")
        .flat_map(|a| a.args.iter().flatten())
        .any(|arg| {
            arg.label.as_ref().is_some_and(|l| l.name == "async")
                && matches!(arg.value.kind, ast::ExprKind::Bool(true))
        })
}

// ---------------------------------------------------------------------------------------
// Per-body facts
// ---------------------------------------------------------------------------------------

/// Why a node suspends in every instance (the first cause found), for the chains of K1401,
/// K1402 and W0421.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    /// Flowed in from another node of the same body.
    From(usize),
    /// An instantiation of a declaration that always suspends.
    Calls(DefId),
    /// A function value stored in data (suspending unless `@sync`, DR-13).
    Stored,
}

/// One use of a declaration's scheme in a body: its own bit and the bits given to its
/// effect parameters there.
struct Inst {
    def: DefId,
    span: Span,
    /// The node of the declaration's own bit at this use (`None` for a `@sync` one).
    own: Option<usize>,
    /// Effect parameter index → node of the argument's bit.
    args: Vec<(u32, usize)>,
}

/// A call that may be a suspension point.
struct Call {
    expr: ExprId,
    span: Span,
    callee: usize,
    closure: usize,
}

/// `let x = cell.get()` (W0421).
struct CellRead {
    local: LocalId,
    end: u32,
    closure: usize,
}

/// Everything about one body that does not depend on other functions' summaries.
struct BodyFacts {
    owner: DefId,
    file: FileId,
    nodes: usize,
    /// Node → display name for chains (local `fn`s, lambdas, instantiated callees).
    names: HashMap<usize, String>,
    /// Effect parameter seeds `(node, i)`.
    params: Vec<(usize, u32)>,
    /// Stored-function seeds.
    stored: Vec<usize>,
    edges: Vec<(usize, usize)>,
    insts: Vec<Inst>,
    /// Nodes that must be sync (a flow into a `@sync` slot), with the flow's span.
    sync_checks: Vec<(usize, Span)>,
    calls: Vec<Call>,
    lambdas: Vec<(ExprId, usize)>,
    local_fns: Vec<(LocalId, usize)>,
    cell_reads: Vec<CellRead>,
    /// Uses of locals: `(local, span, closure)`.
    uses: Vec<(LocalId, Span, usize)>,
}

/// The node of the owner's own bit.
const OWN: usize = 0;

struct FactsBuilder<'a> {
    hir: &'a HirProgram,
    res: &'a TypeckResults,
    facts: BodyFacts,
    vars: HashMap<u32, usize>,
    param_nodes: HashMap<u32, usize>,
    local_names: HashMap<LocalId, String>,
}

impl<'a> FactsBuilder<'a> {
    fn new(
        hir: &'a HirProgram,
        res: &'a TypeckResults,
        owner: DefId,
        file: FileId,
        local_names: HashMap<LocalId, String>,
    ) -> Self {
        FactsBuilder {
            hir,
            res,
            facts: BodyFacts {
                owner,
                file,
                nodes: 1,
                names: HashMap::new(),
                params: Vec::new(),
                stored: Vec::new(),
                edges: Vec::new(),
                insts: Vec::new(),
                sync_checks: Vec::new(),
                calls: Vec::new(),
                lambdas: Vec::new(),
                local_fns: Vec::new(),
                cell_reads: Vec::new(),
                uses: Vec::new(),
            },
            vars: HashMap::new(),
            param_nodes: HashMap::new(),
            local_names,
        }
    }

    fn fresh(&mut self) -> usize {
        self.facts.nodes += 1;
        self.facts.nodes - 1
    }

    /// The node of a bit in this body; `None` for `Sync` (a constant).
    fn node(&mut self, s: Suspension) -> Option<usize> {
        match s {
            Suspension::Sync => None,
            Suspension::Suspending => {
                let n = self.fresh();
                self.facts.stored.push(n);
                Some(n)
            }
            Suspension::Param(i) => Some(match self.param_nodes.get(&i) {
                Some(&n) => n,
                None => {
                    let n = self.fresh();
                    self.param_nodes.insert(i, n);
                    self.facts.params.push((n, i));
                    n
                }
            }),
            Suspension::Var(k) => Some(match self.vars.get(&k) {
                Some(&n) => n,
                None => {
                    let n = self.fresh();
                    self.vars.insert(k, n);
                    n
                }
            }),
        }
    }

    fn ty(&self, e: &Expr) -> Option<&'a Ty> {
        self.res.expr_types.get(&(self.facts.owner, e.id))
    }

    /// The bit of a function-typed expression's value.
    fn fn_bit(&self, e: &Expr) -> Option<Suspension> {
        match self.ty(e) {
            Some(Ty::Fn { suspension, .. }) => Some(*suspension),
            _ => None,
        }
    }

    /// A node for a closure (lambda or local `fn`) whose type is `ty`. A type that is not a
    /// function with a variable bit (poisoned code) still gets its own node, so calls inside
    /// never leak into the enclosing closure.
    fn closure_node(&mut self, ty: Option<&Ty>) -> usize {
        match ty {
            Some(Ty::Fn {
                suspension: Suspension::Var(k),
                ..
            }) => match self.node(Suspension::Var(*k)) {
                Some(n) => n,
                None => self.fresh(),
            },
            _ => self.fresh(),
        }
    }

    fn add_flows(&mut self, flows: &[kalam_typeck::unify::SuspFlow]) {
        for f in flows {
            match f.to {
                Suspension::Sync => {
                    if let Some(n) = self.node(f.from) {
                        self.facts.sync_checks.push((n, f.span));
                    }
                }
                // Always satisfied; and an effect parameter is an input, not a sink.
                Suspension::Suspending | Suspension::Param(_) => {}
                Suspension::Var(_) => {
                    if let (Some(a), Some(b)) = (self.node(f.from), self.node(f.to)) {
                        self.facts.edges.push((a, b));
                    }
                }
            }
        }
    }

    /// An instantiation of `def`'s scheme whose instantiated type is `inst`.
    fn inst_from_type(&mut self, def: DefId, inst: &Ty, span: Span) -> Option<usize> {
        let scheme = self.res.signatures.get(&def)?;
        let (
            Ty::Fn {
                params: sp,
                suspension: so,
                ..
            },
            Ty::Fn {
                params: ip,
                suspension: io,
                ..
            },
        ) = (&scheme.ty, inst)
        else {
            return None;
        };
        let own = match so {
            Suspension::Param(_) => self.node(*io),
            _ => None,
        };
        let mut args = Vec::new();
        for (a, b) in sp.iter().zip(ip) {
            if let (
                Ty::Fn {
                    suspension: Suspension::Param(i),
                    ..
                },
                Ty::Fn { suspension: s, .. },
            ) = (a, b)
            {
                if let Some(n) = self.node(*s) {
                    args.push((*i, n));
                }
            }
        }
        self.push_inst(def, span, own, args);
        own
    }

    /// A method call of `def`: the instantiated type is not recorded, so the arguments are
    /// matched to the declared parameters (positional first, then by label) and each
    /// argument's own bit stands for the effect parameter it instantiates.
    fn inst_from_args(&mut self, def: DefId, args: &[Arg], span: Span) -> usize {
        let own = self.fresh();
        let Some(scheme) = self.res.signatures.get(&def) else {
            return own;
        };
        let Ty::Fn { params, .. } = &scheme.ty else {
            return own;
        };
        let names: Vec<&str> = match self.hir.item(def).map(|i| &i.kind) {
            Some(ItemKind::Fn(f)) => f.params.iter().map(|p| p.name.as_str()).collect(),
            _ => Vec::new(),
        };
        let mut by_param: Vec<Option<&Expr>> = vec![None; params.len()];
        let mut next = 1; // skip `self`
        for a in args {
            match &a.label {
                None => {
                    if next < by_param.len() {
                        by_param[next] = Some(&a.value);
                        next += 1;
                    }
                }
                Some(l) => {
                    if let Some(j) = names.iter().position(|n| n == l) {
                        if let Some(slot) = by_param.get_mut(j) {
                            *slot = Some(&a.value);
                        }
                    }
                }
            }
        }
        let mut inst_args = Vec::new();
        for (p, arg) in params.iter().zip(by_param) {
            if let (
                Ty::Fn {
                    suspension: Suspension::Param(i),
                    ..
                },
                Some(arg),
            ) = (p, arg)
            {
                if let Some(n) = self.fn_bit(arg).and_then(|s| self.node(s)) {
                    inst_args.push((*i, n));
                }
            }
        }
        self.push_inst(def, span, Some(own), inst_args);
        own
    }

    fn push_inst(&mut self, def: DefId, span: Span, own: Option<usize>, args: Vec<(u32, usize)>) {
        if let Some(n) = own {
            let name = format!("`{}`", self.res.name_of(def));
            self.facts.names.entry(n).or_insert(name);
        }
        self.facts.insts.push(Inst {
            def,
            span,
            own,
            args,
        });
    }

    fn call(&mut self, e: &Expr, callee: usize, closure: usize) {
        self.facts.edges.push((callee, closure));
        self.facts.calls.push(Call {
            expr: e.id,
            span: e.span,
            callee,
            closure,
        });
    }

    /// True if `e` is `cell.get()` on the stdlib's `Cell` (W0421).
    fn is_cell_get(&self, e: &Expr) -> bool {
        let ExprKind::MethodCall { name, .. } = &e.kind else {
            return false;
        };
        if name != "get" {
            return false;
        }
        let Some(m) = self.res.method_picks.get(&(self.facts.owner, e.id)) else {
            return false;
        };
        let Some(item) = self.hir.item(*m) else {
            return false;
        };
        if item.module.0 != "std/prelude" {
            return false;
        }
        let ItemKind::Fn(FnDecl {
            parent: Some(p), ..
        }) = &item.kind
        else {
            return false;
        };
        match self.hir.item(*p).map(|i| &i.kind) {
            Some(ItemKind::Impl(imp)) => matches!(
                &imp.self_ty.kind,
                TypeKind::Def(d, _) if self.hir.item(*d).is_some_and(|c| c.name == "Cell")
            ),
            _ => false,
        }
    }

    /// Walks a body (iteratively: HIR can be as deep as the parser allows).
    fn walk(&mut self, roots: Vec<Node<'a>>) {
        let mut stack: Vec<(Node<'a>, usize)> = roots.into_iter().map(|n| (n, OWN)).collect();
        stack.reverse();
        while let Some((node, closure)) = stack.pop() {
            match node {
                Node::Stmts(stmts) => {
                    for s in stmts.iter().rev() {
                        stack.push((Node::Stmt(s), closure));
                    }
                }
                Node::Stmt(s) => self.stmt(s, closure, &mut stack),
                Node::Expr(e) => self.expr(e, closure, &mut stack),
            }
        }
    }

    fn stmt(&mut self, s: &'a Stmt, closure: usize, stack: &mut Vec<(Node<'a>, usize)>) {
        let push = |n: Node<'a>, stack: &mut Vec<(Node<'a>, usize)>| stack.push((n, closure));
        match &s.kind {
            StmtKind::Expr(e) => push(Node::Expr(e), stack),
            StmtKind::Let { pat, value, .. } => {
                if let PatKind::Bind(l) = pat.kind {
                    if self.is_cell_get(value) {
                        self.facts.cell_reads.push(CellRead {
                            local: l,
                            end: s.span.end,
                            closure,
                        });
                    }
                }
                push(Node::Expr(value), stack);
            }
            StmtKind::Var { value, .. } => push(Node::Expr(value), stack),
            StmtKind::Assign { place, value, .. } => {
                // Evaluation order: the value, then the place.
                push(Node::Expr(place), stack);
                push(Node::Expr(value), stack);
            }
            StmtKind::Return(Some(e)) => push(Node::Expr(e), stack),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
            StmtKind::For { iter, body, .. } => {
                push(Node::Stmts(&body.stmts), stack);
                push(Node::Expr(iter), stack);
            }
            StmtKind::While { cond, body } => {
                push(Node::Stmts(&body.stmts), stack);
                push(Node::Expr(cond), stack);
            }
            StmtKind::LocalFn { local, decl, .. } => {
                let ty = self.res.local_types.get(&(self.facts.owner, *local));
                let inner = self.closure_node(ty);
                let name = self
                    .local_names
                    .get(local)
                    .map_or_else(|| "a local function".to_string(), |n| format!("`{n}`"));
                self.facts.names.insert(inner, name);
                self.facts.local_fns.push((*local, inner));
                if let Some(body) = &decl.body {
                    match &body.value {
                        BodyValue::Block(b) => stack.push((Node::Stmts(&b.stmts), inner)),
                        BodyValue::Expr(e) => stack.push((Node::Expr(e), inner)),
                    }
                }
                for p in decl.params.iter().rev() {
                    if let Some(d) = &p.default {
                        stack.push((Node::Expr(d), inner));
                    }
                }
            }
        }
    }

    fn expr(&mut self, e: &'a Expr, closure: usize, stack: &mut Vec<(Node<'a>, usize)>) {
        // Children are pushed in reverse, so they are visited in evaluation order.
        let mut kids: Vec<&'a Expr> = Vec::new();
        let mut blocks: Vec<&'a [Stmt]> = Vec::new();
        match &e.kind {
            ExprKind::Local(l) => self.facts.uses.push((*l, e.span, closure)),
            ExprKind::Def(d) => {
                if let Some(t) = self.ty(e) {
                    self.inst_from_type(*d, t, e.span);
                }
            }
            ExprKind::AssocFn { .. } => {
                let pick = self
                    .res
                    .method_picks
                    .get(&(self.facts.owner, e.id))
                    .copied();
                if let (Some(m), Some(t)) = (pick, self.ty(e)) {
                    self.inst_from_type(m, t, e.span);
                }
            }
            ExprKind::Interp(parts) => {
                for p in parts {
                    if let StrPart::Hole(h) = p {
                        kids.push(h);
                    }
                }
            }
            ExprKind::Tagged { holes, .. } => kids.extend(holes.iter()),
            ExprKind::Call { callee, args } => {
                if let Some(n) = self.fn_bit(callee).and_then(|s| self.node(s)) {
                    self.call(e, n, closure);
                }
                kids.push(callee);
                kids.extend(args.iter().map(|a| &a.value));
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                let pick = self
                    .res
                    .method_picks
                    .get(&(self.facts.owner, e.id))
                    .copied();
                if let Some(m) = pick {
                    let own = self.inst_from_args(m, args, e.span);
                    self.call(e, own, closure);
                }
                kids.push(receiver);
                kids.extend(args.iter().map(|a| &a.value));
            }
            ExprKind::Field { base, .. } | ExprKind::TupleField { base, .. } => kids.push(base),
            ExprKind::Index { base, index } => {
                kids.push(base);
                kids.push(index);
            }
            ExprKind::Unary { operand, .. } => kids.push(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                kids.push(lhs);
                kids.push(rhs);
            }
            ExprKind::Range { lo, hi, .. } => {
                kids.push(lo);
                kids.push(hi);
            }
            ExprKind::Tuple(xs) | ExprKind::List(xs) => kids.extend(xs.iter()),
            ExprKind::Map(pairs) => {
                for (k, v) in pairs {
                    kids.push(k);
                    kids.push(v);
                }
            }
            ExprKind::Try(x) => kids.push(x),
            ExprKind::Lambda { body, .. } => {
                let inner = self.closure_node(self.ty(e));
                self.facts.names.insert(inner, "the lambda".to_string());
                self.facts.lambdas.push((e.id, inner));
                stack.push((Node::Expr(body), inner));
            }
            ExprKind::If {
                branches,
                else_body,
            } => {
                for b in branches {
                    kids.push(&b.cond);
                    blocks.push(&b.body.stmts);
                }
                if let Some(eb) = else_body {
                    blocks.push(&eb.stmts);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                kids.push(scrutinee);
                for a in arms {
                    if let Some(g) = &a.guard {
                        kids.push(g);
                    }
                    blocks.push(&a.body.stmts);
                }
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::Unit
            | ExprKind::Lang(_)
            | ExprKind::ImplicitMember(_)
            | ExprKind::Unsafe(_)
            | ExprKind::TypeName(_)
            | ExprKind::Error => {}
        }
        // Blocks after the expressions (an approximation of evaluation order that only
        // W0421's straight-line rule reads).
        for b in blocks.into_iter().rev() {
            stack.push((Node::Stmts(b), closure));
        }
        for k in kids.into_iter().rev() {
            stack.push((Node::Expr(k), closure));
        }
    }
}

#[derive(Clone, Copy)]
enum Node<'a> {
    Stmts(&'a [Stmt]),
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

// ---------------------------------------------------------------------------------------
// Solving
// ---------------------------------------------------------------------------------------

/// What makes a function always suspend: a rendered call chain.
#[derive(Clone, Debug, Default)]
struct Chain {
    /// Display names after the function itself (`` `save` ``, `` `exec` ``).
    steps: Vec<String>,
    /// How it ends: a base primitive, or a stored function value.
    stored: bool,
}

#[derive(Clone, Debug, Default)]
struct FnState {
    summary: SuspBit,
    sync_params: BTreeSet<u32>,
    /// Set the first time `summary.always` becomes true.
    chain: Option<Chain>,
    /// Fixed by an attribute or the signature (never re-solved).
    fixed: bool,
}

struct Solution {
    values: Vec<SuspBit>,
    why: Vec<Option<Why>>,
}

fn solve(facts: &BodyFacts, fns: &HashMap<DefId, FnState>) -> Solution {
    let n = facts.nodes;
    let mut values = vec![SuspBit::sync(); n];
    let mut why: Vec<Option<Why>> = vec![None; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in &facts.edges {
        adj[a].push(b);
    }
    for &(node, i) in &facts.params {
        values[node].join(&SuspBit::param(i));
    }
    for &node in &facts.stored {
        values[node].always = true;
        why[node] = Some(Why::Stored);
    }
    for inst in &facts.insts {
        let (Some(own), Some(g)) = (inst.own, fns.get(&inst.def)) else {
            continue;
        };
        if g.summary.always && !values[own].always {
            values[own].always = true;
            why[own] = Some(Why::Calls(inst.def));
        }
        for &(i, arg) in &inst.args {
            if g.summary.params.contains(&i) {
                adj[arg].push(own);
            }
        }
    }
    let mut work: VecDeque<usize> = (0..n).filter(|&v| values[v].may_suspend()).collect();
    while let Some(u) = work.pop_front() {
        let from = values[u].clone();
        for &v in &adj[u] {
            let was = values[v].always;
            if values[v].join(&from) {
                if !was && values[v].always {
                    why[v] = Some(Why::From(u));
                }
                work.push_back(v);
            }
        }
    }
    Solution { values, why }
}

/// The chain that makes `node` always suspend (empty steps if none is known).
fn chain_of(
    facts: &BodyFacts,
    sol: &Solution,
    fns: &HashMap<DefId, FnState>,
    res: &TypeckResults,
    node: usize,
) -> Chain {
    let mut chain = Chain::default();
    let push = |chain: &mut Chain, s: &str| {
        if chain.steps.last().map(String::as_str) != Some(s) {
            chain.steps.push(s.to_string());
        }
    };
    let mut at = node;
    let mut seen = HashSet::new();
    while seen.insert(at) {
        if at != node {
            if let Some(name) = facts.names.get(&at) {
                push(&mut chain, name);
            }
        }
        match sol.why.get(at).copied().flatten() {
            Some(Why::From(u)) => at = u,
            Some(Why::Calls(def)) => {
                push(&mut chain, &format!("`{}`", res.name_of(def)));
                if let Some(c) = fns.get(&def).and_then(|f| f.chain.as_ref()) {
                    for s in &c.steps {
                        push(&mut chain, s);
                    }
                    chain.stored = c.stored;
                }
                break;
            }
            Some(Why::Stored) => {
                chain.stored = true;
                break;
            }
            None => break,
        }
    }
    chain
}

/// "`check` calls `present`, which calls `exists` (suspends)".
fn render_chain(first: &str, chain: &Chain) -> String {
    let mut out = first.to_string();
    for (i, s) in chain.steps.iter().enumerate() {
        out.push_str(if i == 0 { " calls " } else { ", which calls " });
        out.push_str(s);
    }
    if chain.stored {
        out.push_str(if chain.steps.is_empty() {
            " calls a function value stored in data (suspending unless its type is `@sync`)"
        } else {
            ", which calls a function value stored in data (suspending unless its type is `@sync`)"
        });
    } else {
        out.push_str(" (suspends)");
    }
    out
}

// ---------------------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------------------

/// Effect parameter indices of `scheme`: `(fn-typed parameters, own bit)`.
fn effect_params(scheme: &Scheme) -> (Vec<u32>, Option<u32>) {
    let Ty::Fn {
        params, suspension, ..
    } = &scheme.ty
    else {
        return (Vec::new(), None);
    };
    let ps = params
        .iter()
        .filter_map(|p| match p {
            Ty::Fn {
                suspension: Suspension::Param(i),
                ..
            } => Some(*i),
            _ => None,
        })
        .collect();
    let own = match suspension {
        Suspension::Param(i) => Some(*i),
        _ => None,
    };
    (ps, own)
}

/// Solves suspension for a type-checked program: fills `res.suspension` and returns the
/// K1401, K1402 and W0421 diagnostics. Diagnostics are dropped when type checking reported
/// an error (poisoned types make the analysis unreliable; root cause first).
pub fn analyze(hir: &HirProgram, res: &mut TypeckResults, attrs: &BaseAttrs) -> Vec<Diagnostic> {
    let flows = {
        let mut by_owner: HashMap<DefId, Vec<kalam_typeck::unify::SuspFlow>> = HashMap::new();
        for (owner, f) in &res.flows {
            by_owner.entry(*owner).or_default().push(*f);
        }
        by_owner
    };
    let r: &TypeckResults = res;

    // Trait method → the impl methods that implement it.
    let mut impls_of: HashMap<DefId, Vec<DefId>> = HashMap::new();
    let mut trait_ids: Vec<&TraitId> = r.traits.traits.keys().collect();
    trait_ids.sort_by_key(|t| format!("{t:?}"));
    for tid in trait_ids {
        let decl = &r.traits.traits[tid];
        for (name, m) in &decl.methods {
            for imp in &r.traits.impls {
                if imp.trait_ref.as_ref().is_some_and(|t| t.id == *tid) {
                    if let Some((_, im)) = imp.methods.iter().find(|(n, _)| n == name) {
                        impls_of.entry(*m).or_default().push(*im);
                    }
                }
            }
        }
    }

    // Function states and body facts.
    let mut fns: HashMap<DefId, FnState> = HashMap::new();
    let mut bodies: Vec<BodyFacts> = Vec::new();
    let mut body_of: HashMap<DefId, usize> = HashMap::new();
    let mut sync_decl: Vec<(DefId, FileId, Span)> = Vec::new();
    for item in hir.items() {
        let local_names = |locals: &[kalam_typeck::hir::LocalDecl]| {
            locals
                .iter()
                .enumerate()
                .map(|(i, l)| (LocalId(i as u32), l.name.clone()))
                .collect::<HashMap<_, _>>()
        };
        let empty = Vec::new();
        let owner_flows = flows.get(&item.def).unwrap_or(&empty);
        match &item.kind {
            ItemKind::Fn(f) => {
                let Some(scheme) = r.signatures.get(&item.def) else {
                    continue;
                };
                let (callbacks, _) = effect_params(scheme);
                let is_trait_method = matches!(
                    f.parent.and_then(|p| hir.item(p)).map(|i| &i.kind),
                    Some(ItemKind::Trait(_))
                );
                let stub = f.body.is_none() || attrs.builtin.contains(&item.def);
                let mut state = FnState::default();
                if attrs.suspends.contains(&item.def) {
                    state.summary = SuspBit::suspending();
                    state.chain = Some(Chain::default());
                    state.fixed = true;
                } else if attrs.stores_callbacks.contains(&item.def) {
                    state.fixed = true;
                } else if stub && !is_trait_method {
                    for i in callbacks {
                        state.summary.join(&SuspBit::param(i));
                    }
                    state.fixed = true;
                }
                if f.sync_attr {
                    sync_decl.push((item.def, item.file, item.span));
                }
                if !state.fixed && !stub {
                    if let Some(body) = &f.body {
                        let mut b = FactsBuilder::new(
                            hir,
                            r,
                            item.def,
                            item.file,
                            local_names(&body.locals),
                        );
                        b.add_flows(owner_flows);
                        let mut roots: Vec<Node> = f
                            .params
                            .iter()
                            .filter_map(|p| p.default.as_ref().map(Node::Expr))
                            .collect();
                        roots.push(match &body.value {
                            BodyValue::Block(bl) => Node::Stmts(&bl.stmts),
                            BodyValue::Expr(e) => Node::Expr(e),
                        });
                        b.walk(roots);
                        body_of.insert(item.def, bodies.len());
                        bodies.push(b.facts);
                    }
                }
                fns.insert(item.def, state);
            }
            ItemKind::Record(rec) => {
                let defaults: Vec<Node> = rec
                    .fields
                    .iter()
                    .filter_map(|f| f.default.as_ref().map(Node::Expr))
                    .collect();
                if defaults.is_empty() {
                    continue;
                }
                let mut b = FactsBuilder::new(hir, r, item.def, item.file, HashMap::new());
                b.add_flows(owner_flows);
                b.walk(defaults);
                bodies.push(b.facts);
            }
            ItemKind::Const(c) => {
                let mut b = FactsBuilder::new(hir, r, item.def, item.file, HashMap::new());
                b.add_flows(owner_flows);
                b.walk(vec![Node::Expr(&c.value)]);
                bodies.push(b.facts);
            }
            ItemKind::Test(body) => {
                let mut b =
                    FactsBuilder::new(hir, r, item.def, item.file, local_names(&body.locals));
                b.add_flows(owner_flows);
                b.walk(vec![match &body.value {
                    BodyValue::Block(bl) => Node::Stmts(&bl.stmts),
                    BodyValue::Expr(e) => Node::Expr(e),
                }]);
                bodies.push(b.facts);
            }
            _ => {}
        }
    }

    // Callers of each function (for re-solving), and trait methods as joins of their impls.
    let mut callers: HashMap<DefId, Vec<DefId>> = HashMap::new();
    for b in &bodies {
        if body_of.contains_key(&b.owner) {
            let mut seen = HashSet::new();
            for inst in &b.insts {
                if seen.insert(inst.def) {
                    callers.entry(inst.def).or_default().push(b.owner);
                }
            }
        }
    }
    for (m, impls) in &impls_of {
        for im in impls {
            callers.entry(*im).or_default().push(*m);
        }
    }

    // Worklist, callees first (post-order of the call graph).
    let order = post_order(&bodies, &body_of, &impls_of, &fns);
    let mut queued: HashSet<DefId> = order.iter().copied().collect();
    let mut work: VecDeque<DefId> = order.into_iter().collect();
    let mut solutions: Vec<Option<Solution>> = (0..bodies.len()).map(|_| None).collect();
    while let Some(f) = work.pop_front() {
        queued.remove(&f);
        let mut summary = SuspBit::sync();
        let mut sync_params = BTreeSet::new();
        let mut chain = None;
        if let Some(&bi) = body_of.get(&f) {
            let facts = &bodies[bi];
            let sol = solve(facts, &fns);
            summary = sol.values[OWN].clone();
            if summary.always {
                chain = Some(chain_of(facts, &sol, &fns, r, OWN));
            }
            collect_sync_params(facts, &sol, &fns, &mut sync_params);
            solutions[bi] = Some(sol);
        }
        if let Some(impls) = impls_of.get(&f) {
            let own = r.signatures.get(&f).and_then(|s| effect_params(s).1);
            for im in impls {
                let Some(st) = fns.get(im) else { continue };
                let mut bit = st.summary.clone();
                bit.params.retain(|&i| Some(i) != own);
                if bit.always && !summary.always {
                    chain = Some(Chain {
                        steps: std::iter::once(format!("`{}`", r.name_of(*im)))
                            .chain(st.chain.iter().flat_map(|c| c.steps.clone()))
                            .collect(),
                        stored: st.chain.as_ref().is_some_and(|c| c.stored),
                    });
                }
                summary.join(&bit);
                sync_params.extend(st.sync_params.iter().copied());
            }
        }
        let state = fns.entry(f).or_default();
        if state.fixed {
            continue;
        }
        // `@sync` (provisional fork): the callbacks it calls must be sync.
        if hir
            .item(f)
            .is_some_and(|i| matches!(&i.kind, ItemKind::Fn(d) if d.sync_attr))
        {
            sync_params.extend(summary.params.iter().copied());
        }
        let before = state.sync_params.len();
        let mut grew = state.summary.join(&summary);
        if state.chain.is_none() && state.summary.always {
            state.chain = chain;
        }
        state.sync_params.extend(sync_params);
        grew |= state.sync_params.len() > before;
        if grew {
            for c in callers.get(&f).into_iter().flatten() {
                if queued.insert(*c) {
                    work.push_back(*c);
                }
            }
        }
    }
    // Bodies that are not functions (constants, record defaults, tests) are solved once.
    for (bi, facts) in bodies.iter().enumerate() {
        if solutions[bi].is_none() {
            solutions[bi] = Some(solve(facts, &fns));
        }
    }

    // Outputs.
    let mut out = Suspensions {
        solved: true,
        ..Suspensions::default()
    };
    let mut diags = Vec::new();
    let mut reported: HashSet<(FileId, Span)> = HashSet::new();
    for (facts, sol) in bodies.iter().zip(&solutions) {
        let Some(sol) = sol else { continue };
        for c in &facts.calls {
            let bit = &sol.values[c.callee];
            if bit.may_suspend() {
                out.calls.insert((facts.owner, c.expr), bit.clone());
            }
        }
        for (e, n) in &facts.lambdas {
            out.lambdas
                .insert((facts.owner, *e), sol.values[*n].clone());
        }
        for (l, n) in &facts.local_fns {
            out.local_fns
                .insert((facts.owner, *l), sol.values[*n].clone());
        }
        // K1401: flows into `@sync`, and arguments for sync-required effect parameters.
        let mut checks: Vec<(usize, Span)> = facts.sync_checks.clone();
        for inst in &facts.insts {
            if let Some(g) = fns.get(&inst.def) {
                for &(i, arg) in &inst.args {
                    if g.sync_params.contains(&i) {
                        checks.push((arg, inst.span));
                    }
                }
            }
        }
        for (node, span) in checks {
            if !sol.values[node].always || !reported.insert((facts.file, span)) {
                continue;
            }
            let chain = chain_of(facts, sol, &fns, r, node);
            let first = facts
                .names
                .get(&node)
                .cloned()
                .unwrap_or_else(|| "this function".to_string());
            diags.push(
                Diagnostic::new("K1401", facts.file, span)
                    .label(span, render_chain(&first, &chain)),
            );
        }
        diags.extend(w0421(facts, sol));
    }
    for (def, file, span) in sync_decl {
        let Some(st) = fns.get(&def) else { continue };
        if !st.summary.always {
            continue;
        }
        let chain = st.chain.clone().unwrap_or_default();
        let first = format!("`{}`", r.name_of(def));
        diags.push(Diagnostic::new("K1402", file, span).label(span, render_chain(&first, &chain)));
    }
    for (def, st) in &fns {
        out.fns.insert(*def, st.summary.clone());
        if !st.sync_params.is_empty() {
            out.sync_params
                .insert(*def, st.sync_params.iter().copied().collect());
        }
    }
    let poisoned = !res.errors.is_empty();
    res.suspension = out;
    if poisoned {
        return Vec::new();
    }
    diags.sort_by_key(|d| (d.file, d.span.start, d.span.end, d.code));
    diags
}

/// Effect parameters of the owner that flow into `@sync` slots.
fn collect_sync_params(
    facts: &BodyFacts,
    sol: &Solution,
    fns: &HashMap<DefId, FnState>,
    out: &mut BTreeSet<u32>,
) {
    for &(node, _) in &facts.sync_checks {
        out.extend(sol.values[node].params.iter().copied());
    }
    for inst in &facts.insts {
        if let Some(g) = fns.get(&inst.def) {
            for &(i, arg) in &inst.args {
                if g.sync_params.contains(&i) {
                    out.extend(sol.values[arg].params.iter().copied());
                }
            }
        }
    }
}

/// Function bodies and trait methods in post-order of the call graph (callees first),
/// computed without recursion.
fn post_order(
    bodies: &[BodyFacts],
    body_of: &HashMap<DefId, usize>,
    impls_of: &HashMap<DefId, Vec<DefId>>,
    fns: &HashMap<DefId, FnState>,
) -> Vec<DefId> {
    let succ = |f: DefId| -> Vec<DefId> {
        let mut out: Vec<DefId> = body_of
            .get(&f)
            .map(|&bi| bodies[bi].insts.iter().map(|i| i.def).collect())
            .unwrap_or_default();
        out.extend(impls_of.get(&f).into_iter().flatten().copied());
        out
    };
    let mut roots: Vec<DefId> = body_of.keys().copied().collect();
    roots.extend(impls_of.keys().copied());
    roots.sort();
    roots.dedup();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        if !seen.insert(root) {
            continue;
        }
        let mut stack: Vec<(DefId, Vec<DefId>)> = vec![(root, succ(root))];
        while let Some((f, rest)) = stack.last_mut() {
            match rest.pop() {
                Some(g) => {
                    let interesting = body_of.contains_key(&g) || impls_of.contains_key(&g);
                    if interesting && seen.insert(g) {
                        let s = succ(g);
                        stack.push((g, s));
                    }
                }
                None => {
                    let f = *f;
                    stack.pop();
                    if !fns.get(&f).is_some_and(|s| s.fixed) {
                        out.push(f);
                    }
                }
            }
        }
    }
    out
}

/// W0421 (§15.6): `let x = cell.get()`, then a call that may suspend, then a use of `x`, all
/// in straight-line source order inside one closure. One warning per read, at the first
/// such use.
fn w0421(facts: &BodyFacts, sol: &Solution) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for read in &facts.cell_reads {
        let waits: Vec<&Call> = facts
            .calls
            .iter()
            .filter(|c| {
                c.closure == read.closure
                    && c.span.start >= read.end
                    && sol.values[c.callee].may_suspend()
            })
            .collect();
        let first = facts
            .uses
            .iter()
            .filter(|(l, span, closure)| {
                *l == read.local
                    && *closure == read.closure
                    && waits.iter().any(|w| w.span.end <= span.start)
            })
            .min_by_key(|(_, span, _)| span.start);
        if let Some((_, span, _)) = first {
            out.push(Diagnostic::new("W0421", facts.file, *span));
        }
    }
    out
}
