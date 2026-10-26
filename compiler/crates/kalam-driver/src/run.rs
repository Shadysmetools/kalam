//! `kalam run --interp FILE`: one file, checked exactly as `kalam check FILE` checks it,
//! then compiled against the embedded stdlib signature stubs, lowered to KIR and run on the
//! reference interpreter (`kalam-interp`, ROADMAP M2).
//!
//! The stdlib's `@builtin` bodies are dropped before type checking, so every stdlib call
//! lowers to a runtime intrinsic; a program that needs one the M2 interpreter does not
//! provide is refused before it starts. Nothing runs unless the whole program lowered.

use std::io::Write;

use kalam_base::{FileId, LineIndex, Skin};
use kalam_skins::Concept;

use crate::{check_bytes, embedded_stdlib, resolve_with_std, with_std, CheckedFile, NotKalamFile};

/// Stack of the thread that type-checks, lowers and interprets (only touched pages are
/// committed). Interpretation recurses per Kalam call and per nested expression.
const RUN_STACK: usize = 1 << 30;

/// Nested Kalam calls before a run panics with "stack overflow" (§9.10).
pub const MAX_CALL_DEPTH: usize = 10_000;

/// What `kalam run --interp` did.
pub enum RunOutcome {
    /// The file has errors (lexer through type checker): nothing ran.
    Rejected(CheckedFile),
    /// The program uses something the M2 interpreter cannot run: nothing ran.
    Unsupported(String),
    /// `main` returned normally.
    Done,
    /// `main` returned `Err` (§14.3), with the error's `message()` when it has one.
    MainErr(Option<String>),
    /// A Kalam panic (§14.4), at a 1-based line and column of the file when it is there.
    Panicked {
        message: String,
        at: Option<(u32, u32)>,
    },
    /// A compiler or interpreter bug (a value of the wrong shape, an ill-formed KIR).
    Internal(String),
    /// Writing the program's output failed.
    Io(String),
}

impl RunOutcome {
    /// The process exit code (spec §14.3, §14.4; `kalam` CLI conventions): 0 done, 1
    /// diagnostics or `Err` from `main`, 2 unsupported, 101 a panic, 70 an internal error.
    pub fn exit_code(&self) -> u8 {
        match self {
            RunOutcome::Done => 0,
            RunOutcome::Rejected(_) | RunOutcome::MainErr(_) | RunOutcome::Io(_) => 1,
            RunOutcome::Unsupported(_) => 2,
            RunOutcome::Panicked { .. } => 101,
            RunOutcome::Internal(_) => 70,
        }
    }

    /// What goes to stderr (empty for [`RunOutcome::Done`]; the caller renders the
    /// diagnostics of [`RunOutcome::Rejected`] itself). A panic is
    /// `panic: <message>` plus ` --> <path>:<line>:<col>` (the Rust/Elm diagnostic
    /// location form of spec §21.4).
    pub fn stderr_text(&self, path: &str) -> String {
        match self {
            RunOutcome::Done | RunOutcome::Rejected(_) => String::new(),
            RunOutcome::Unsupported(why) => format!("cannot run `{path}`: {why}\n"),
            RunOutcome::MainErr(Some(m)) => format!("error: {m}\n"),
            RunOutcome::MainErr(None) => "error: `main` returned `Err`\n".to_string(),
            RunOutcome::Panicked { message, at } => match at {
                Some((line, col)) => format!("panic: {message}\n  --> {path}:{line}:{col}\n"),
                None => format!("panic: {message}\n"),
            },
            RunOutcome::Internal(why) => format!("internal error in `kalam run --interp`: {why}\n"),
            RunOutcome::Io(why) => format!("cannot write the program's output: {why}\n"),
        }
    }
}

/// `kalam run --interp`: checks `bytes` as `kalam check` does, then runs it, writing the
/// program's standard output to `out`.
pub fn run_interp(
    path: &str,
    bytes: &[u8],
    out: &mut (dyn Write + Send),
) -> Result<RunOutcome, NotKalamFile> {
    let checked = check_bytes(path, bytes)?;
    if checked.has_errors() {
        return Ok(RunOutcome::Rejected(checked));
    }
    let (skin, text) = (checked.skin, checked.text.as_str());
    let outcome = std::thread::scope(|s| {
        let spawned = std::thread::Builder::new()
            .name("kalam-run".into())
            .stack_size(RUN_STACK)
            .spawn_scoped(s, || compile_and_run(skin, text, out));
        match spawned {
            Ok(handle) => match handle.join() {
                Ok(r) => r,
                Err(payload) => std::panic::resume_unwind(payload),
            },
            Err(e) => RunOutcome::Internal(format!("cannot start the interpreter thread: {e}")),
        }
    });
    Ok(outcome)
}

fn compile_and_run(skin: Skin, text: &str, out: &mut (dyn Write + Send)) -> RunOutcome {
    let lines = LineIndex::new(text);
    let at = |offset: u32| {
        // Defensive: a span from another file must never panic here (`line_col` needs a
        // char boundary). Lowering reports a stdlib default at its call site.
        let mut offset = (offset as usize).min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let lc = lines.line_col(text, offset as u32);
        (lc.line, lc.col)
    };
    let parsed = kalam_parser::parse_source(text, skin, FileId(0));
    let Some(std) = embedded_stdlib() else {
        return RunOutcome::Internal("the embedded stdlib stubs do not parse".into());
    };
    // The same resolution `kalam check FILE` makes (FOUNDER-QUEUE Q34), which reported any
    // error it finds before this point: one found here means the two disagree.
    let sources = with_std(skin, &parsed.ast, std);
    let (program, resolved) = resolve_with_std(&sources);
    let errors: Vec<&str> = resolved
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code)
        .collect();
    if !errors.is_empty() {
        return RunOutcome::Internal(format!(
            "resolving the file against the stdlib stubs found {} that `kalam check` did not report",
            errors.join(", ")
        ));
    }
    // Signatures only: the stubs' `panic("builtin")` bodies would hide the intrinsics.
    let hir = kalam_typeck::lower::lower_program_with(&program, &sources, false);
    let tc = kalam_typeck::check_program(&hir);
    let entry = entry_point(&parsed.ast, &program);
    let kir = match kalam_ir::lower::lower_program_entry(&hir, &tc, "main", entry) {
        Ok(k) => k,
        Err(kalam_ir::lower::LowerError::Unsupported { what, span }) => {
            let (line, _) = at(span.start);
            return RunOutcome::Unsupported(format!(
                "line {line}: {what} is not supported by the M2 reference interpreter"
            ));
        }
        Err(kalam_ir::lower::LowerError::TypeErrors(n)) => {
            return RunOutcome::Internal(format!(
                "{n} type error(s) found only when compiling against the stdlib stubs"
            ))
        }
    };
    let problems = kalam_ir::verify::verify(&kir);
    if let Some(p) = problems.first() {
        return RunOutcome::Internal(format!("ill-formed KIR: {p}"));
    }
    let options = kalam_interp::Options {
        max_depth: MAX_CALL_DEPTH,
    };
    match kalam_interp::run(&kir, out, &options) {
        Ok(kalam_interp::Exit::Done) => RunOutcome::Done,
        Ok(kalam_interp::Exit::MainErr(m)) => RunOutcome::MainErr(m),
        Err(kalam_interp::InterpError::Panic {
            message,
            file,
            span,
        }) => RunOutcome::Panicked {
            message,
            at: (file == FileId(0)).then(|| at(span.start)),
        },
        Err(kalam_interp::InterpError::Unsupported(why)) => RunOutcome::Unsupported(why),
        Err(kalam_interp::InterpError::Internal(why)) => RunOutcome::Internal(why),
        Err(kalam_interp::InterpError::Io(why)) => RunOutcome::Io(why),
    }
}

/// The file's `main` (the core name `main`, spelled per skin, §7.2), as resolve declared it.
fn entry_point(
    ast: &kalam_ast::Ast,
    program: &kalam_resolve::Program,
) -> Option<kalam_resolve::DefId> {
    ast.file.items.iter().find_map(|item| match &item.kind {
        kalam_ast::ItemKind::Fn(f)
            if f.sig.name.kind == kalam_ast::NameKind::Core(Concept::CoreMain) =>
        {
            program.decl_at(FileId(0), f.sig.name.span)
        }
        _ => None,
    })
}
