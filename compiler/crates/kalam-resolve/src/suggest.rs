//! "Did you mean" for undefined names (K0401): edit distance over names and skeletons.

use crate::skeleton::skeleton;

/// Optimal-string-alignment (restricted Damerau–Levenshtein) distance over chars.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = v;
        }
    }
    d[n][m]
}

/// The largest distance worth suggesting for a name of `len` chars.
fn budget(len: usize) -> usize {
    match len {
        0..=2 => 0,
        3..=5 => 1,
        _ => 2,
    }
}

/// The closest candidate to `name`, by edit distance over the spellings and over the
/// skeletons (so `prnit` finds `print` and `اكتب` finds `أكتب`). Ties go to the earlier
/// candidate, so the result is deterministic for a deterministic candidate order. `None` when
/// nothing is close enough, or when `name` itself is a candidate.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let len = name.chars().count();
    let limit = budget(len);
    if limit == 0 {
        return None;
    }
    let skel = skeleton(name);
    let mut best: Option<(usize, &'a str)> = None;
    for c in candidates {
        if c == name {
            return None;
        }
        let d = edit_distance(name, c).min(edit_distance(&skel, &skeleton(c)));
        let better = match best {
            None => true,
            Some((bd, _)) => d < bd,
        };
        if d <= limit && better {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(edit_distance("print", "print"), 0);
        assert_eq!(edit_distance("prnit", "print"), 1); // transposition
        assert_eq!(edit_distance("totl", "total"), 1);
        assert_eq!(edit_distance("", "ab"), 2);
    }

    #[test]
    fn closest_picks_the_nearest_name() {
        let names = ["total", "items", "describe"];
        assert_eq!(closest("totl", names), Some("total"));
        assert_eq!(closest("descibe", names), Some("describe"));
        assert_eq!(closest("zzzzzz", names), None);
        assert_eq!(closest("ab", ["ac"]), None, "too short to guess");
    }
}
