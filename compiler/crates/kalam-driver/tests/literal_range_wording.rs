//! K0126 names the literal's actual type and that type's range, in every message language:
//! a `Float` or `Decimal` literal is never explained with `Int`'s 64-bit range (the M2
//! iteration-6 review). The type-specific reason is the label under the literal.

use kalam_base::Skin;
use kalam_driver::check_source;

const INT_MAX: &str = "9223372036854775807";

/// `kalam check` of `src` (in `path`) rendered in `lang`: K0126 must be its only finding.
fn rendered(path: &str, src: &str, lang: Skin) -> String {
    let checked = check_source(path, src).unwrap();
    let codes: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["K0126"], "{src}");
    checked.render_human(lang)
}

#[test]
fn an_int_literal_is_explained_with_the_int_range() {
    let src = "fn main():\n    let big = 9223372036854775808\n    print(\"{big}\")\n";
    let en = rendered("main.kl", src, Skin::En);
    assert!(en.contains("doesn't fit in `Int`"), "{en}");
    assert!(en.contains(INT_MAX), "{en}");
    let ar_src = "دالة رئيسي():\n    ليكن big = 9223372036854775808\n    اطبع(\"{big}\")\n";
    let ar = rendered("main.ar.kl", ar_src, Skin::Ar);
    assert!(ar.contains("`عدد`") && ar.contains(INT_MAX), "{ar}");
    assert!(!ar.contains("`Int`"), "{ar}");
    let franco_src = "dala ra2isi():\n    khali big = 9223372036854775808\n    ektb(\"{big}\")\n";
    let franco = rendered("main.franco.kl", franco_src, Skin::Franco);
    assert!(
        franco.contains("`adad`") && franco.contains(INT_MAX),
        "{franco}"
    );
}

#[test]
fn float_and_decimal_literals_name_their_own_type() {
    for (ty, lit, core, why) in [
        ("Float", "1e7000", "Float", "1.8e308"),
        (
            "Decimal",
            "1.00000000000000000000000000000000001",
            "Decimal",
            "34",
        ),
        ("Decimal", "1e6145", "Decimal", "exponent"),
    ] {
        let src = format!("fn main():\n    let x: {ty} = {lit}\n    print(\"{{x}}\")\n");
        let en = rendered("main.kl", &src, Skin::En);
        assert!(en.contains(&format!("doesn't fit in `{core}`")), "{en}");
        assert!(en.contains(why), "{en}");
        // Never `Int`'s range, and never a nudge to `BigInt` for whole numbers alone.
        for lang in Skin::ALL {
            let text = rendered("main.kl", &src, lang);
            assert!(!text.contains(INT_MAX), "{lang:?}: {text}");
            assert!(
                !text.contains("`Int`") && !text.contains("`عدد`"),
                "{lang:?}: {text}"
            );
        }
    }
}
