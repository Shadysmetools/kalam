//! Small helpers: wall-clock timestamps without a date crate, file hashing.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a Unix time (UTC).
pub fn iso8601(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

/// SHA-256 (hex) of a file's bytes.
pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(kalam_syntax::sha256::hex(&kalam_syntax::sha256::digest(
        &bytes,
    )))
}

/// The hash that identifies this fuzz target build: SHA-256 of the running executable
/// (the worker processes run the same file).
pub fn target_version() -> String {
    std::env::current_exe()
        .and_then(|p| file_sha256(&p))
        .unwrap_or_else(|_| "unknown".into())
}

/// The `new-language` repository root (this crate lives in `compiler/crates/kalam-fuzz`).
pub fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso8601(1_793_016_000), "2026-10-26T12:00:00Z");
    }
}
