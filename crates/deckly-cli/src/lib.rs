//! The Deckly plugin tool: create a plugin project, build it into a plugin
//! folder, run it in a running Deckly while you edit it, check it, and pack
//! it into one `.decklyPlugin` file.
//!
//! It depends on `deckly-plugin-protocol` only: the manifest, its checks
//! and the development link file are the same code Deckly uses, so a plugin
//! that passes here installs there. It never links the SDK, the plugin host
//! or the app. `cargo` and `git` are run as commands.
//!
//! Deckly is an independent app and is not made, approved or supported by
//! Elgato or Corsair.

mod args;
mod build;
mod config;
mod control;
mod create;
mod dev;
mod link;
mod log;
mod pack;
mod project;
mod prompt;
mod templates;
mod term;
mod validate;

use args::Command;
use std::ffi::OsString;
use std::time::{Duration, Instant};

/// This tool's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The `deckly-sdk` version new projects use from crates.io.
pub const SDK_VERSION: &str = "0.0.1";

/// Runs the tool with its arguments (without the program name) and returns
/// the exit status: 0 done, 1 failed, 2 wrong arguments.
pub fn main(args: impl Iterator<Item = OsString>) -> i32 {
    let command = match args::parse(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("deckly-cli: {e}\nRun `deckly-cli help` for the commands.");
            return 2;
        }
    };
    match run(command) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("deckly-cli: {e}");
            1
        }
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Help(text) => {
            print!("{text}");
            Ok(())
        }
        Command::Version => {
            println!("deckly-cli {VERSION} (new projects use deckly-sdk {SDK_VERSION})");
            Ok(())
        }
        Command::Create(a) => create::run(a),
        Command::Build(c) => {
            let project = project::Project::find(c.package.as_deref())?;
            let built = build::build(&project, c.release)?;
            println!("The plugin folder is {}", built.dir.display());
            Ok(())
        }
        Command::Dev { common, log } => dev::run(&common, log),
        Command::Link(c) => link::link_command(&c),
        Command::Unlink { common, id } => link::unlink_command(&common, id),
        Command::List(c) => link::list_command(&c),
        Command::Validate { common, start } => validate::command(&common, start),
        Command::Pack { common, merge, out } => pack::command(&common, merge, out),
        Command::Log { common, follow, id } => log::command(&common, follow, id),
    }
}

/// Prints one step's line once it is done: what it did and how long it
/// took.
pub(crate) fn step<T>(what: &str, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let began = Instant::now();
    let result = work();
    let took = seconds(began.elapsed());
    match &result {
        Ok(_) => println!("  done    {what} ({took})"),
        Err(_) => println!("  failed  {what} ({took})"),
    }
    result
}

pub(crate) fn seconds(d: Duration) -> String {
    format!("{:.1} s", d.as_secs_f32())
}
