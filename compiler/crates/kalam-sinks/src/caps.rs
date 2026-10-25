//! Capability rules of spec §16 that the type checker applies at a call site (K15xx).
//!
//! - K1501: `World` cannot be constructed or decoded (§16.1 rule 1). Typeck reports it at
//!   a call of the type `World` ([`construct_world`]) and for a `Decode` requirement on it.
//! - K1503: a literal key that looks secret, passed to `env.get` (§16.1 `Env` row, §20.1):
//!   such keys come back as `Secret[String]` from `env.secret` ([`env_get_secret_key`]).
//!
//! `print` and `log.*` need no capability (D20); a *library* that calls them must declare
//! `console = true` (K1504), which needs the package manifest and is checked with it, not
//! here.

use kalam_base::{FileId, Span};
use kalam_diagnostics::Diagnostic;

/// K1501 at a construction of `World`.
pub fn construct_world(file: FileId, span: Span) -> Diagnostic {
    Diagnostic::new("K1501", file, span)
}

/// True for an environment key that names a secret: `*_KEY`, `*_SECRET`, `*_TOKEN`, or
/// any key containing `PASSWORD` (case-insensitive, like the platforms that set them).
pub fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    ["_KEY", "_SECRET", "_TOKEN"]
        .iter()
        .any(|s| k.ends_with(s) && k.len() > s.len())
        || k.contains("PASSWORD")
}

/// K1503 when `env.get` is called with the literal key `key` and the key looks secret.
pub fn env_get_secret_key(key: &str, file: FileId, span: Span) -> Option<Diagnostic> {
    is_secret_key(key).then(|| Diagnostic::new("K1503", file, span))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_looking_keys() {
        for k in [
            "STRIPE_API_KEY",
            "JWT_SECRET",
            "GITHUB_TOKEN",
            "PASSWORD",
            "DB_PASSWORD",
            "db_password",
            "Session_Secret",
        ] {
            assert!(is_secret_key(k), "{k}");
        }
        for k in [
            "PORT",
            "HOME",
            "KEY",
            "_KEY",
            "MONKEY",
            "TOKENS",
            "DATABASE_URL",
            "",
        ] {
            assert!(!is_secret_key(k), "{k}");
        }
    }

    #[test]
    fn codes() {
        let at = Span::new(1, 2);
        assert_eq!(construct_world(FileId(0), at).code, "K1501");
        assert_eq!(
            env_get_secret_key("API_KEY", FileId(0), at).map(|d| d.code),
            Some("K1503")
        );
        assert_eq!(env_get_secret_key("PORT", FileId(0), at), None);
    }
}
