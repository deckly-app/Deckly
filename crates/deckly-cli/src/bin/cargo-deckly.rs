//! `cargo deckly ...`: the same program as `deckly-cli`, run by Cargo, which
//! passes `deckly` as the first argument.

fn main() {
    let mut args = std::env::args_os().skip(1).peekable();
    if args.peek().is_some_and(|a| a == "deckly") {
        args.next();
    }
    std::process::exit(deckly_cli::main(args));
}
