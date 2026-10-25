//! The package manager behind `kalam add`, `kalam update` and `kalam pkg …`
//! (spec §13.5, report 04 §5.4, ARCHITECTURE.md §15).
//!
//! - `kalam.toml` manifest and `kalam.lock` lockfile (KCF hashes for Kalam packages,
//!   integrity hashes for npm tarballs). CI installs are frozen.
//! - PubGrub resolution; the suspension bit and capability set are part of a package's API.
//! - npm tarballs are fetched and integrity-checked directly. **No lifecycle script ever
//!   runs** (04 P1). New versions have a 48-hour release-age cooldown (04 P9).
//! - `add`/`update` show a capability diff over the whole tree; additions need approval
//!   (04 P6). Publishing checks namespaces and cross-script typosquats (04 P7).
//!
//! Status: data types only (Phase 2).

use std::collections::BTreeMap;

use kalam_base::Skin;

/// `kalam.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub targets: Vec<String>,
    /// `[skin.ar] digits` (the only skin setting, spec §6.2).
    pub ar_digits_arabic_indic: bool,
    pub capabilities: ManifestCapabilities,
    pub dependencies: BTreeMap<String, String>,
    pub npm: BTreeMap<String, NpmDependency>,
}

/// `[capabilities]`: what this package's externs and console use may touch (spec §16.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManifestCapabilities {
    pub net: Vec<String>,
    pub fs: Vec<String>,
    pub env: Vec<String>,
    /// Libraries that call `print`/`log.*` must set this (deviation X9).
    pub console: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpmDependency {
    pub version: String,
    /// For example `net:api.stripe.com`.
    pub caps: Vec<String>,
}

/// One `kalam.lock` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockedPackage {
    Kalam {
        name: String,
        version: String,
        kcf_sha256: String,
    },
    Npm {
        name: String,
        version: String,
        integrity: String,
    },
}

/// A change in capabilities between two resolutions, shown by `kalam add`/`update`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilityDiff {
    pub added: Vec<(String, String)>,
    pub removed: Vec<(String, String)>,
}

impl CapabilityDiff {
    /// Additions need explicit approval (04 P6).
    pub fn needs_approval(&self) -> bool {
        !self.added.is_empty()
    }
}

/// The skin a registry page is rendered in is a viewer choice; packages are stored as KCF.
pub fn registry_render_skins() -> [Skin; 3] {
    Skin::ALL
}

/// The 48-hour default release-age cooldown for new dependency versions (04 P9).
pub const RELEASE_AGE_COOLDOWN_HOURS: u32 = 48;
