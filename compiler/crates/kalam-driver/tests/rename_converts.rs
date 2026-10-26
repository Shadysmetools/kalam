//! FOUNDER-QUEUE Q51: K0405's rename fix uses one suffix rule (`_2`, `_3`, …) in every skin,
//! so fixing and converting commute: `convert(fix(x), X→Y) == fix(convert(x, X→Y))`, and a
//! fixed file converts to a fixed file (`kalam fix` on it changes nothing).

use kalam_base::Skin;
use kalam_driver::{convert_source, fix_file, FmtConfig};

fn path(skin: Skin) -> String {
    format!("main{}", skin.extension())
}

fn fixed(skin: Skin, text: &str) -> String {
    let f = fix_file(&path(skin), text.as_bytes()).unwrap();
    assert!(f.changed(), "nothing fixed in {skin}:\n{text}");
    assert!(
        f.applied.iter().any(|a| a.code == "K0405"),
        "{:?}",
        f.applied
    );
    f.fixed
}

fn convert(text: &str, from: Skin, to: Skin) -> String {
    convert_source(&path(from), text, to, &FmtConfig::default())
        .unwrap_or_else(|e| panic!("{from}->{to}: {e:?}\n{text}"))
        .text
}

fn commutes(from: Skin, src: &str) {
    let fixed_here = fixed(from, src);
    for to in Skin::ALL {
        if to == from {
            continue;
        }
        let fix_then_convert = convert(&fixed_here, from, to);
        let convert_then_fix = fixed(to, &convert(src, from, to));
        assert_eq!(
            fix_then_convert, convert_then_fix,
            "{from}->{to}: fix∘convert != convert∘fix"
        );
        let again = fix_file(&path(to), fix_then_convert.as_bytes()).unwrap();
        assert!(
            !again.changed(),
            "{to}: the converted fixed file is not fixed"
        );
    }
}

#[test]
fn the_k0405_goldens_fix_the_same_way_in_every_skin() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../eval/goldens/K0405");
    for skin in Skin::ALL {
        let src = std::fs::read_to_string(dir.join(format!("{}.kl", skin.id()))).unwrap();
        commutes(skin, &src);
        assert!(fixed(skin, &src).contains("total_2"), "{skin}");
    }
}

#[test]
fn renames_of_arabic_franco_and_type_names_commute_with_convert() {
    commutes(
        Skin::Ar,
        "هيكل شكل:\n    ضلع: عدد\n\nدالة رئيسي():\n    ليكن شكل = 1\n    اطبع(\"{شكل}\")\n",
    );
    commutes(
        Skin::Franco,
        "dala ra2isi():\n    khali kotob = 1\n    law kotob > 0:\n        khali kotob = 2\n        ektb(\"{kotob}\")\n",
    );
    commutes(
        Skin::En,
        "type Shape:\n    a: Int\n\nfn Shape() -> Int => 1\n\nfn main():\n    print(\"x\")\n",
    );
    // `total_2` is taken, so the suffix is `_3` in every skin.
    commutes(
        Skin::En,
        "fn main():\n    let total = 1\n    if total > 0:\n        let total = 2\n        print(\"{total}\")\n    let total_2 = 5\n    print(\"{total} {total_2}\")\n",
    );
}
