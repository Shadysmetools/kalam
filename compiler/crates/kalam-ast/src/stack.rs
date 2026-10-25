//! Stack safety for recursive walks over deep expression chains.
//!
//! The parser caps tree height at `MAX_HEIGHT` (1024, kalam-parser), but a postfix, unary or
//! binary chain (`1 + 1 + … + 1`, `a.b.c…`, `f()()…`) still nests one [`Expr`] per link.
//! A recursive walk with a debug-build frame of 1–2 KiB per level then overflows a 1 MiB
//! stack (the Windows main-thread default) well below the cap, and a stack overflow aborts
//! the process: no `catch_unwind` can catch it.
//!
//! [`deeper`] wraps one level of such a walk. It counts nesting per thread; past
//! [`SPAWN_AT`] levels it moves the rest of the walk to a helper thread with a
//! [`BIG_STACK`] stack, where it runs inline from then on. Shallow walks (every realistic
//! program) never leave the caller's thread. Where threads are unavailable (wasm32), the
//! walk continues inline on the caller's stack.
//!
//! [`Expr`]: crate::Expr

use std::cell::Cell;

/// Nesting depth after which a walk moves to a big-stack thread. At 1–2 KiB per level in
/// debug builds this uses at most ~256 KiB of the caller's stack.
pub const SPAWN_AT: u32 = 128;

/// Stack size of the helper thread: room for ~30 000 levels in a debug build, far above the
/// parser's height cap. Only touched pages are committed.
pub const BIG_STACK: usize = 64 << 20;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    /// Set on a helper thread (it has the big stack), or after a failed spawn (no threads on
    /// this target): either way, never try to move again from this thread.
    static INLINE_ONLY: Cell<bool> = const { Cell::new(false) };
}

/// Restores the depth counter on scope exit, including on unwind.
struct Level(u32);

impl Drop for Level {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(self.0));
    }
}

/// True if the next [`deeper`] call would move to a helper thread.
pub fn must_move() -> bool {
    !INLINE_ONLY.with(Cell::get) && DEPTH.with(Cell::get) >= SPAWN_AT
}

/// Runs one level of a recursive walk: inline while the walk is shallow or already on a big
/// stack, otherwise on a helper thread with a [`BIG_STACK`] stack. A panic in `f` is
/// re-raised on the calling thread.
pub fn deeper<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    if !must_move() {
        return level(f);
    }
    let mut slot = Some(f);
    let slot_ref = &mut slot;
    let moved = std::thread::scope(|s| {
        let spawned = std::thread::Builder::new()
            .name("kalam-deep-walk".into())
            .stack_size(BIG_STACK)
            .spawn_scoped(s, move || {
                INLINE_ONLY.with(|b| b.set(true));
                slot_ref.take().map(|f| f())
            });
        match spawned {
            Ok(handle) => match handle.join() {
                Ok(r) => r,
                Err(payload) => std::panic::resume_unwind(payload),
            },
            Err(_) => None,
        }
    });
    match moved {
        Some(r) => r,
        None => {
            // The thread could not start, so `f` was never taken: run it here.
            INLINE_ONLY.with(|b| b.set(true));
            let f = slot.take().expect("f runs exactly once");
            level(f)
        }
    }
}

/// Runs one level of a recursive walk on the current thread, counting it toward [`SPAWN_AT`].
/// For walks whose state cannot cross threads (a `Formatter`): check [`must_move`] first.
pub fn level<R>(f: impl FnOnce() -> R) -> R {
    let guard = Level(DEPTH.with(|d| {
        let n = d.get();
        d.set(n.saturating_add(1));
        n
    }));
    let r = f();
    drop(guard);
    r
}
