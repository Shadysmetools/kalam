//! The worker process (`kalam-fuzz --worker`) and the one-shot reproducer
//! (`kalam-fuzz --repro FILE`).
//!
//! The worker reads inputs from stdin and runs [`crate::target::exercise`] on its **main
//! thread**, like `kalam` itself, so a stack overflow the CLI would hit happens here too.
//! Panics are caught and reported; a stack overflow or an abort kills the process, and the
//! supervisor, which still holds the input it sent, records it.

use std::cell::RefCell;
use std::io::{self, BufReader, BufWriter};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::process::ExitCode;

use kalam_base::Skin;

use crate::proto::{read_request, write_reply, Reply};
use crate::target::exercise;

thread_local! {
    static PANIC_AT: RefCell<String> = const { RefCell::new(String::new()) };
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
        PANIC_AT.with(|p| *p.borrow_mut() = at);
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

fn selftest(text: &str) {
    if std::env::var("KALAM_FUZZ_SELFTEST").as_deref() != Ok("1") {
        return;
    }
    let Some(what) = text.strip_prefix(SELFTEST_PREFIX) else {
        return;
    };
    match what.trim() {
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
        selftest(text);
        exercise(text, skin)
    }));
    match r {
        Ok(obs) => match obs.oracle {
            None => Reply::Ok {
                features: obs.features,
            },
            Some((name, detail)) => Reply::Oracle {
                name: name.to_string(),
                detail,
                features: obs.features,
            },
        },
        Err(payload) => Reply::Panic {
            message: payload_message(payload.as_ref()),
            location: PANIC_AT.with(|p| p.borrow().clone()),
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
                if write_reply(&mut w, &reply).is_err() {
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
    match run_one(skin, &text) {
        Reply::Ok { .. } => {
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
