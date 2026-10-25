//! Spec §28: 8 programs × 3 skins.
//! - Every block within the 100-column limit is canonical text, so `fmt` is a fixpoint on
//!   it; on every block `fmt` is idempotent and keeps the AST.
//! - `[converter]` blocks (§28.1, §28.2) are exactly what `kalam convert` prints from the
//!   English block, in every direction.
//! - Every block round-trips through every pair and 3-hop cycle to `fmt(block)`.

mod common;

use common::*;

#[test]
fn spec_28_blocks_format_to_themselves_and_round_trip() {
    let blocks = spec_section_28();
    assert_eq!(blocks.len(), 24, "8 programs × 3 skins");
    let mut failures = Vec::new();
    let mut fixpoints = 0;
    for b in &blocks {
        let Some(before) = parse_clean(&b.body, b.skin) else {
            failures.push(format!("§{} {}: does not parse", b.section, b.file));
            continue;
        };
        let f1 = match kalam_fmt::format_strict(&b.body, b.skin, &cfg()) {
            Ok(f) => f,
            Err(e) => {
                failures.push(format!("§{} {}: fmt failed: {e}", b.section, b.file));
                continue;
            }
        };
        // A block with a line over 100 columns breaks §21.1 rule 1 itself (§28.7 has a
        // 105-column line; the §28.8 Franco glossary view is wider than its source), so only
        // blocks within the limit must already be in canonical layout.
        let within_limit = b.body.lines().all(|l| l.chars().count() <= 100);
        if within_limit {
            fixpoints += 1;
            if f1 != b.body {
                failures.push(format!(
                    "§{} {}: not a fmt fixpoint\n{f1}",
                    b.section, b.file
                ));
            }
        }
        if kalam_fmt::format_strict(&f1, b.skin, &cfg()).ok().as_ref() != Some(&f1) {
            failures.push(format!("§{} {}: fmt not idempotent", b.section, b.file));
        }
        let after = parse_clean(&f1, b.skin).expect("formatted text parses");
        if tree(&after) != tree(&before) || comments(&after) != comments(&before) {
            failures.push(format!("§{} {}: fmt changed the AST", b.section, b.file));
        }
        for path in round_trip_paths(b.skin) {
            match convert_path(&f1, &path) {
                Ok(steps) if steps.last() == Some(&f1) => {}
                Ok(_) => {
                    failures.push(format!("§{} {}: {path:?} not identical", b.section, b.file))
                }
                Err(e) => failures.push(format!("§{} {}: {path:?}: {e}", b.section, b.file)),
            }
        }
    }
    for chunk in blocks.chunks(3) {
        if chunk.iter().any(|b| b.glossary) {
            continue;
        }
        for from in chunk {
            for to in chunk {
                match kalam_fmt::convert(&from.body, from.skin, to.skin, &cfg()) {
                    Ok(got) if got == to.body => {}
                    other => failures.push(format!(
                        "§{}: {} -> {}: {:?}",
                        from.section, from.file, to.file, other
                    )),
                }
            }
        }
    }
    assert!(
        fixpoints >= 19,
        "only {fixpoints} of 24 blocks are within 100 columns"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
