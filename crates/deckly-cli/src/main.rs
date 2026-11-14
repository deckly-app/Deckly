//! `deckly-cli`: create, build, run and package Deckly plugins.

fn main() {
    std::process::exit(deckly_cli::main(std::env::args_os().skip(1)));
}
