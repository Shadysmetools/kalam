fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    for skin in kalam_base::Skin::ALL {
        println!("--- {skin}\n{}", kalam_props::generated_program(seed, skin));
    }
}
