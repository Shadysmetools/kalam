//! Prints why an input fails the convert round-trip oracle: `kalam-fuzz` saves such inputs
//! in `eval/fuzz/oracle/`. Usage: `cargo run -p kalam-fuzz --example oracle_diff -- FILE`.
use kalam_base::Skin;
use kalam_driver::FmtConfig;

fn main() {
    let path = std::env::args().nth(1).expect("FILE");
    let skin = kalam_fuzz::corpus::skin_of(std::path::Path::new(&path)).expect("a skin suffix");
    let text = std::fs::read_to_string(&path).expect("read");
    let cfg = FmtConfig::default();
    let canon = kalam_fmt::convert(&text, skin, skin, &cfg).expect("respell");
    for to in Skin::ALL {
        if to == skin {
            continue;
        }
        let there = kalam_fmt::convert(&text, skin, to, &cfg).expect("convert");
        let back = kalam_fmt::convert(&there, to, skin, &cfg).expect("convert back");
        if back != canon {
            let i = canon
                .char_indices()
                .zip(back.chars())
                .find(|((_, a), b)| a != b)
                .map(|((i, _), _)| i)
                .unwrap_or(canon.len().min(back.len()));
            let lo = canon[..i].rfind('\n').map(|k| k + 1).unwrap_or(0);
            let line = |s: &str| s[lo..].lines().next().unwrap_or("").to_string();
            println!("{skin}->{to}->{skin} differs at byte {i}");
            println!("  respelled: {:?}", line(&canon));
            println!("  via {to}:  {:?}", line(&there[lo.min(there.len())..]));
            println!("  back:      {:?}", line(&back));
        }
    }
}
