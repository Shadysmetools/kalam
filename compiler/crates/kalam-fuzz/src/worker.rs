//! The worker process (`kalam-fuzz --worker`) and the one-shot reproducer
//! (`kalam-fuzz --repro FILE`).
//!
//! The worker reads inputs from stdin and runs [`crate::target::exercise`] on its **main
//! thread**, like `kalam` itself, so a stack overflow the CLI would hit happens here too.
//! Panics are caught and reported; a stack overflow or an abort kills the process, and the
//! supervisor, which still holds the input it sent, records it.

use std::io::{self, BufReader, BufWriter};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Mutex;

use kalam_base::Skin;

use crate::proto::{read_request, write_reply, Reply};
use crate::target::{exercise, with_budget, INTERP_BUDGET};

/// Where the last panic happened. Process-wide, not per thread: the interpreter runs on its
/// own threads, and its panic is resumed on the main thread without running the hook again.
static PANIC_AT: Mutex<String> = Mutex::new(String::new());

fn set_panic_at(at: String) {
    *PANIC_AT.lock().unwrap_or_else(|e| e.into_inner()) = at;
}

fn panic_at() -> String {
    PANIC_AT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Inputs that start with this prefix trigger deliberate faults when
/// `KALAM_FUZZ_SELFTEST=1`, to test the supervisor's crash handling end to end.
pub const SELFTEST_PREFIX: &str = "#!selftest:";

/// Keeps Windows from showing an error dialog when a worker crashes (the supervisor
/// handles it), and from blocking the fuzz run on that dialog.
fn quiet_crashes() {
    #[cfg(windows)]
    {
        extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }
        // SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX
        // SAFETY: SetErrorMode only changes this process's error-mode flags.
        unsafe {
            SetErrorMode(0x0001 | 0x0002 | 0x8000);
        }
    }
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file().replace('\\', "/"), l.line()))
            .unwrap_or_else(|| "unknown".into());
        set_panic_at(at);
    }));
}

#[inline(never)]
fn overflow(n: u64) -> u64 {
    let buf = [n as u8; 4096];
    std::hint::black_box(&buf);
    if n == u64::MAX {
        return 0;
    }
    overflow(n + 1).wrapping_add(u64::from(buf[(n % 4096) as usize]))
}

/// Deliberate faults for the supervisor's tests; `true` if the worker must retire.
fn selftest(text: &str) -> bool {
    if std::env::var("KALAM_FUZZ_SELFTEST").as_deref() != Ok("1") {
        return false;
    }
    let Some(what) = text.strip_prefix(SELFTEST_PREFIX) else {
        return false;
    };
    match what.trim() {
        // A budgeted computation that never ends, like an interpreted infinite loop.
        "spin" => {
            return with_budget(INTERP_BUDGET, || loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
            })
            .is_none();
        }
        // A panic on a budgeted thread is still a panic, located where it happened.
        "thread-panic" => {
            let _ = with_budget(INTERP_BUDGET, || -> u8 { panic!("selftest thread panic") });
        }
        "panic" => panic!("selftest panic"),
        "overflow" => {
            std::hint::black_box(overflow(0));
        }
        "abort" => std::process::abort(),
        "hang" => loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        },
        _ => {}
    }
    false
}

fn payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "non-string panic payload".into())
}

/// Runs one input and describes the result.
pub fn run_one(skin: Skin, text: &str) -> Reply {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if selftest(text) {
            return crate::target::Observed {
                retire: true,
                ..Default::default()
            };
        }
        exercise(text, skin)
    }));
    match r {
        Ok(obs) => {
            let reply = match obs.oracle {
                None => Reply::Ok {
                    features: obs.features,
                },
                Some((name, detail)) => Reply::Oracle {
                    name: name.to_string(),
                    detail,
                    features: obs.features,
                },
            };
            if obs.retire {
                Reply::Retire(Box::new(reply))
            } else {
                reply
            }
        }
        Err(payload) => Reply::Panic {
            message: payload_message(payload.as_ref()),
            location: panic_at(),
        },
    }
}

/// `kalam-fuzz --worker`
pub fn worker_main() -> ExitCode {
    quiet_crashes();
    install_panic_hook();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut r = BufReader::new(stdin.lock());
    let mut w = BufWriter::new(stdout.lock());
    loop {
        match read_request(&mut r) {
            Ok(Some((skin, text))) => {
                let reply = run_one(skin, &text);
                if write_reply(&mut w, &reply).is_err() || matches!(reply, Reply::Retire(_)) {
                    // Retiring: returning from `main` ends the process and the thread that
                    // outlived its budget.
                    return ExitCode::SUCCESS;
                }
            }
            Ok(None) => return ExitCode::SUCCESS,
            Err(_) => return ExitCode::from(2),
        }
    }
}

/// `kalam-fuzz --repro FILE [--skin S]`: exit 0 if the input runs cleanly (an oracle
/// violation is printed but is not a crash), 101 on a panic; a stack overflow or an abort
/// ends the process the way it would end `kalam`.
pub fn repro_main(path: &Path, skin: Option<Skin>) -> ExitCode {
    quiet_crashes();
    install_panic_hook();
    let Some(skin) = skin.or_else(|| crate::corpus::skin_of(path)) else {
        eprintln!("kalam-fuzz: cannot tell the skin of {}", path.display());
        return ExitCode::from(2);
    };
    let text = match std::fs::read(path) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => {
            eprintln!("kalam-fuzz: {}: {e}", path.display());
            return ExitCode::from(2);
        }
    };
    let reply = match run_one(skin, &text) {
        Reply::Retire(inner) => {
            println!("note: a budgeted step (the interpreter) ran out of time");
            *inner
        }
        r => r,
    };
    match reply {
        Reply::Retire(_) | Reply::Ok { .. } => {
            println!("ok: no crash");
            ExitCode::SUCCESS
        }
        Reply::Oracle { name, detail, .. } => {
            println!("oracle violation {name}: {detail}");
            ExitCode::SUCCESS
        }
        Reply::Panic { message, location } => {
            eprintln!("panic at {location}: {message}");
            ExitCode::from(101)
        }
    }
}
