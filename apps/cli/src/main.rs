//! `swapdeck-cli`: vedi `lib.rs` e `swapdeck-cli help`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(swapdeck_cli::main_with(&args));
}
