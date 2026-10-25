//! Seed inputs: `eval/corpus` (`en.kalam`, `ar.kalam`, `franco.kalam`), `eval/suites`
//! (`*.kl`, `*.ar.kl`, `*.franco.kl`; created by another agent, so it may be missing), saved
//! crash inputs, and generated programs from `kalam-props`.

use std::path::{Path, PathBuf};

use kalam_base::Skin;

use crate::mutate::{Input, MAX_LEN};

/// The skin a seed file is written in: the `.kl` extensions (DR-18), or the corpus naming
/// `<skin>.kalam`.
pub fn skin_of(path: &Path) -> Option<Skin> {
    let name = path.file_name()?.to_str()?;
    if let Some(s) = Skin::from_path(name) {
        return Some(s);
    }
    let stem = name.strip_suffix(".kalam")?;
    let last = stem.rsplit(['.', '-', '_']).next().unwrap_or(stem);
    Skin::ALL.into_iter().find(|s| s.id() == last)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out, depth + 1);
        } else {
            out.push(p);
        }
    }
}

/// Every Kalam source file under `dirs` (missing directories are skipped).
pub fn load(dirs: &[PathBuf]) -> Vec<(PathBuf, Input)> {
    let mut files = Vec::new();
    for d in dirs {
        walk(d, &mut files, 0);
    }
    let mut out = Vec::new();
    for f in files {
        let Some(skin) = skin_of(&f) else { continue };
        let Ok(bytes) = std::fs::read(&f) else {
            continue;
        };
        if bytes.len() > MAX_LEN {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        out.push((f, Input { skin, text }));
    }
    out
}

/// `n` generated programs, spread over the skins.
pub fn generated(n: u64, seed: u64) -> Vec<Input> {
    (0..n)
        .map(|i| {
            let skin = Skin::ALL[(i % 3) as usize];
            Input {
                skin,
                text: kalam_props::generated_program(seed ^ (i.wrapping_mul(0x9E37_79B9)), skin),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skins_from_names() {
        assert_eq!(skin_of(Path::new("a/en.kalam")), Some(Skin::En));
        assert_eq!(skin_of(Path::new("a/ar.kalam")), Some(Skin::Ar));
        assert_eq!(skin_of(Path::new("franco.kalam")), Some(Skin::Franco));
        assert_eq!(skin_of(Path::new("x/case.ar.kl")), Some(Skin::Ar));
        assert_eq!(skin_of(Path::new("x/case.franco.kl")), Some(Skin::Franco));
        assert_eq!(skin_of(Path::new("x/case.kl")), Some(Skin::En));
        assert_eq!(skin_of(Path::new("x/expect.json")), None);
        assert_eq!(skin_of(Path::new("x/ref.py")), None);
    }

    #[test]
    fn the_repo_corpus_loads() {
        let root = crate::util::repo_root();
        let seeds = load(&[root.join("eval/corpus")]);
        assert!(seeds.len() >= 30, "only {} corpus seeds", seeds.len());
        assert!(seeds.iter().any(|(_, s)| s.skin == Skin::Ar));
    }
}
