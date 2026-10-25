//! Deep but valid programs (long operator and postfix chains, up to the parser's height cap)
//! are formatted and converted on a 1 MiB stack, the Windows main-thread default, without
//! overflowing it.

mod common;

use common::*;
use kalam_base::Skin;

fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn deep_chains_format_and_convert_on_a_one_mebibyte_stack() {
    let programs = [
        format!("fn main():\n    let x = 1{}\n", " + 1".repeat(900)),
        format!("fn main():\n    let x = a{}\n", ".b".repeat(900)),
        format!("fn main():\n    let x = f{}\n", "()".repeat(900)),
        format!("fn main():\n    let x = a{}\n", ".b(1)".repeat(400)),
        format!("fn main():\n    print(\"{{a{}}}\")\n", " and b".repeat(500)),
        format!(
            "fn main():\n    let x = {}1{}\n",
            "(".repeat(60),
            ")".repeat(60)
        ),
    ];
    for src in programs {
        let out = on_small_stack(move || {
            let f = kalam_fmt::format_strict(&src, Skin::En, &cfg()).map_err(|e| e.to_string());
            let ar =
                kalam_fmt::convert(&src, Skin::En, Skin::Ar, &cfg()).map_err(|d| format!("{d:?}"));
            (src, f, ar)
        });
        let (src, f, ar) = out;
        let f = f.unwrap_or_else(|e| panic!("{e}"));
        assert!(ar.is_ok(), "convert failed");
        assert_eq!(
            parse_clean(&f, Skin::En).map(|a| tree(&a)),
            parse_clean(&src, Skin::En).map(|a| tree(&a))
        );
    }
}
