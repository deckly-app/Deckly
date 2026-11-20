//! The command line.

use lexopt::{Arg, Parser, ValueExt};
use std::ffi::OsString;
use std::path::PathBuf;

pub const HELP: &str = "\
deckly-cli: create, build, run and package Deckly plugins

Usage: deckly-cli <command> [options]
       cargo deckly <command> [options]

Commands:
  create [name]   Make a new plugin project in a new folder (alias: new)
  dev             Build, link into Deckly, and rebuild and restart on every save
  build           Build the plugin folder target/deckly/<plugin id>
  link            Show this plugin's folder to the local Deckly
  unlink [id]     Remove the link (this plugin's, or the plugin with that id)
  list            List the linked plugins
  validate        Check the plugin folder and start the plugin once
  pack            Validate, then make target/deckly/<id>-<version>-<target>.decklyPlugin
  log             Print the plugin's log (-f to follow it)
  help [command]  Show this, or a command's options

Every command except create works in the plugin's folder or any folder
inside it. Run `deckly-cli help <command>` for its options.
";

const CREATE_HELP: &str = "\
deckly-cli create [name] [options]   (alias: new)

Asks a few questions, then makes a ready plugin project in ./<name>: the
code with an example action and its tests, icons, a README, a licence, a git
repository and a release workflow; builds it and links it into Deckly.

Every question has a flag. With --yes, questions without a flag take their
default. Without a terminal, missing answers are an error, never a prompt.

  --author <name>         Author (default: git user.name, else the login name)
  --id <id>               Plugin id, reverse-DNS (default: com.<author>.<name>)
  --description <text>    One line about the plugin
  --example <kind>        counter (a key with a setting and a face) or minimal
  --license <licence>     mit, apache-2.0, mit-or-apache-2.0 or none
  --dir <folder>          Where to make the project folder (default: here)
  -y, --yes               Take the defaults; do not ask
  --no-git                Do not make a git repository
  --no-ci                 Leave out the check and release workflows
  --no-build              Do not build the plugin
  --no-link               Do not link it into Deckly
  --sdk-path <folder>     Use deckly-sdk from a local folder (also DECKLY_SDK_PATH)
  --sdk-git <url>         Use deckly-sdk from a git repository (also DECKLY_SDK_GIT)
  --sdk-branch <branch>   The branch for --sdk-git
  --config-dir <folder>   Deckly's settings folder (also DECKLY_CONFIG_DIR)
";

const COMMON_HELP: &str = "
Options:
  -p, --package <name>    The plugin's package, in a workspace with several
  --release               Build with optimizations
  --config-dir <folder>   Deckly's settings folder (also DECKLY_CONFIG_DIR)
";

/// Options most commands share.
#[derive(Clone, Debug, Default)]
pub struct Common {
    pub config_dir: Option<PathBuf>,
    pub package: Option<String>,
    pub release: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Example {
    #[default]
    Counter,
    Minimal,
}

impl Example {
    pub const ALL: [Example; 2] = [Example::Counter, Example::Minimal];

    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "counter" => Some(Example::Counter),
            "minimal" => Some(Example::Minimal),
            _ => None,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Example::Counter => {
                "Counter: a key that counts presses, with a setting, a face and tests"
            }
            Example::Minimal => "Minimal: one key that shows OK when pressed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum License {
    #[default]
    Mit,
    Apache,
    MitOrApache,
    None,
}

impl License {
    pub const ALL: [License; 4] = [
        License::Mit,
        License::Apache,
        License::MitOrApache,
        License::None,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "mit" => Some(License::Mit),
            "apache-2.0" | "apache" => Some(License::Apache),
            "mit-or-apache-2.0" | "mit or apache-2.0" | "mit-or-apache" => {
                Some(License::MitOrApache)
            }
            "none" => Some(License::None),
            _ => None,
        }
    }

    /// The SPDX expression for `Cargo.toml`; empty for none.
    pub fn spdx(self) -> &'static str {
        match self {
            License::Mit => "MIT",
            License::Apache => "Apache-2.0",
            License::MitOrApache => "MIT OR Apache-2.0",
            License::None => "",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            License::Mit => "MIT",
            License::Apache => "Apache-2.0",
            License::MitOrApache => "MIT OR Apache-2.0",
            License::None => "None (all rights reserved)",
        }
    }
}

/// Where a new project gets `deckly-sdk` from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sdk {
    CratesIo,
    Path(PathBuf),
    Git { url: String, branch: Option<String> },
}

#[derive(Clone, Debug)]
pub struct CreateArgs {
    pub name: Option<String>,
    pub author: Option<String>,
    pub id: Option<String>,
    pub description: Option<String>,
    pub example: Option<Example>,
    pub license: Option<License>,
    pub dir: Option<PathBuf>,
    pub yes: bool,
    pub git: bool,
    pub ci: bool,
    pub build: bool,
    pub link: bool,
    pub sdk: Sdk,
    pub config_dir: Option<PathBuf>,
}

impl Default for CreateArgs {
    fn default() -> Self {
        Self {
            name: None,
            author: None,
            id: None,
            description: None,
            example: None,
            license: None,
            dir: None,
            yes: false,
            git: true,
            ci: true,
            build: true,
            link: true,
            sdk: Sdk::CratesIo,
            config_dir: None,
        }
    }
}

#[derive(Debug)]
pub enum Command {
    Help(String),
    Version,
    Create(CreateArgs),
    Dev {
        common: Common,
        log: bool,
    },
    Build(Common),
    Link(Common),
    Unlink {
        common: Common,
        id: Option<String>,
    },
    List(Common),
    Validate {
        common: Common,
        start: bool,
    },
    Pack {
        common: Common,
        merge: Vec<PathBuf>,
        out: Option<PathBuf>,
    },
    Log {
        common: Common,
        follow: bool,
        id: Option<String>,
    },
}

fn help_for(command: &str) -> Result<String, String> {
    let (usage, extra) = match command {
        "create" | "new" => return Ok(CREATE_HELP.to_owned()),
        "dev" => (
            "deckly-cli dev [options]\n\nBuilds the plugin, links it into Deckly and restarts it there; then\n\
             rebuilds and restarts it on every save of src/, assets/ or Cargo.toml,\n\
             and prints its log. A failed build leaves the running plugin alone.\n\
             Ctrl+C stops the tool; the plugin stays linked (deckly-cli unlink).\n",
            "  --no-log                Do not print the plugin's log\n",
        ),
        "build" => (
            "deckly-cli build [options]\n\nBuilds the program and assembles the plugin folder\n\
             target/deckly/<plugin id>: the program, assets/ and deckly-plugin.json.\n",
            "",
        ),
        "link" => (
            "deckly-cli link [options]\n\nBuilds the plugin and links its folder into Deckly: Deckly runs it from\n\
             there as a development plugin, from a fresh copy at each restart.\n",
            "",
        ),
        "unlink" => (
            "deckly-cli unlink [plugin id] [options]\n\nRemoves a link; without an id, this plugin's.\n",
            "",
        ),
        "list" => (
            "deckly-cli list [options]\n\nLists the linked plugins.\n",
            "",
        ),
        "validate" => (
            "deckly-cli validate [options]\n\nBuilds the plugin, checks its folder as Deckly will, then starts it once\n\
             as Deckly does: it must answer hello, and exit when its input closes.\n",
            "  --no-start              Check the folder only\n",
        ),
        "pack" => (
            "deckly-cli pack [options]\n       deckly-cli pack --merge <file>... [-o <file>]\n\n\
             Builds with optimizations, validates, and makes one .decklyPlugin file in\n\
             target/deckly. --merge joins files packed on different systems into one.\n",
            "  --merge <file>...       Join .decklyPlugin files of the same plugin and version\n  \
             -o, --output <file>     Where to write the file\n",
        ),
        "log" => (
            "deckly-cli log [plugin id] [options]\n\nPrints the plugin's log from Deckly's logs folder.\n",
            "  -f, --follow            Keep printing new lines\n",
        ),
        other => return Err(format!("there is no command {other:?}")),
    };
    Ok(format!("{usage}{COMMON_HELP}{extra}"))
}

pub fn parse(args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    parse_inner(Parser::from_args(args)).map_err(|e| e.to_string())
}

fn string(v: OsString) -> Result<String, lexopt::Error> {
    v.string()
}

fn parse_inner(mut p: Parser) -> Result<Command, lexopt::Error> {
    let command = match p.next()? {
        None => return Ok(Command::Help(HELP.to_owned())),
        Some(Arg::Short('h') | Arg::Long("help")) => return Ok(Command::Help(HELP.to_owned())),
        Some(Arg::Short('V') | Arg::Long("version")) => return Ok(Command::Version),
        Some(Arg::Value(v)) => string(v)?,
        Some(other) => return Err(other.unexpected()),
    };
    if command == "help" {
        return Ok(Command::Help(match p.next()? {
            Some(Arg::Value(v)) => help_for(&string(v)?).map_err(lexopt::Error::from)?,
            _ => HELP.to_owned(),
        }));
    }
    if command == "create" || command == "new" {
        return parse_create(p);
    }
    help_for(&command).map_err(lexopt::Error::from)?;
    let mut common = Common::default();
    let mut positional: Option<String> = None;
    let (mut log, mut start, mut follow) = (true, true, false);
    let mut merge = Vec::new();
    let mut merging = false;
    let mut out = None;
    while let Some(arg) = p.next()? {
        match (command.as_str(), arg) {
            (_, Arg::Short('h') | Arg::Long("help")) => {
                return Ok(Command::Help(
                    help_for(&command).map_err(lexopt::Error::from)?,
                ));
            }
            (_, Arg::Long("config-dir")) => common.config_dir = Some(p.value()?.into()),
            (_, Arg::Short('p') | Arg::Long("package")) => {
                common.package = Some(string(p.value()?)?);
            }
            (_, Arg::Long("release")) => common.release = true,
            ("dev", Arg::Long("no-log")) => log = false,
            ("validate", Arg::Long("no-start")) => start = false,
            ("log", Arg::Short('f') | Arg::Long("follow")) => follow = true,
            ("pack", Arg::Long("merge")) => merging = true,
            ("pack", Arg::Short('o') | Arg::Long("output")) => out = Some(p.value()?.into()),
            ("pack", Arg::Value(v)) if merging => merge.push(PathBuf::from(v)),
            ("unlink" | "log", Arg::Value(v)) if positional.is_none() => {
                positional = Some(string(v)?);
            }
            (_, other) => return Err(other.unexpected()),
        }
    }
    if merging && merge.len() < 2 {
        return Err("--merge needs two or more .decklyPlugin files".into());
    }
    Ok(match command.as_str() {
        "dev" => Command::Dev { common, log },
        "build" => Command::Build(common),
        "link" => Command::Link(common),
        "unlink" => Command::Unlink {
            common,
            id: positional,
        },
        "list" => Command::List(common),
        "validate" => Command::Validate { common, start },
        "pack" => Command::Pack { common, merge, out },
        _ => Command::Log {
            common,
            follow,
            id: positional,
        },
    })
}

fn parse_create(mut p: Parser) -> Result<Command, lexopt::Error> {
    let mut a = CreateArgs::default();
    let (mut sdk_path, mut sdk_git, mut sdk_branch) = (None, None, None);
    while let Some(arg) = p.next()? {
        match arg {
            Arg::Short('h') | Arg::Long("help") => {
                return Ok(Command::Help(CREATE_HELP.to_owned()));
            }
            Arg::Value(v) if a.name.is_none() => a.name = Some(string(v)?),
            Arg::Long("author") => a.author = Some(string(p.value()?)?),
            Arg::Long("id") => a.id = Some(string(p.value()?)?),
            Arg::Long("description") => a.description = Some(string(p.value()?)?),
            Arg::Long("example") => {
                let v = string(p.value()?)?;
                a.example = Some(Example::parse(&v).ok_or_else(|| {
                    lexopt::Error::from(format!("--example is counter or minimal, not {v:?}"))
                })?);
            }
            Arg::Long("license" | "licence") => {
                let v = string(p.value()?)?;
                a.license = Some(License::parse(&v).ok_or_else(|| {
                    lexopt::Error::from(format!(
                        "--license is mit, apache-2.0, mit-or-apache-2.0 or none, not {v:?}"
                    ))
                })?);
            }
            Arg::Long("dir") => a.dir = Some(p.value()?.into()),
            Arg::Short('y') | Arg::Long("yes") => a.yes = true,
            Arg::Long("no-git") => a.git = false,
            Arg::Long("no-ci") => a.ci = false,
            Arg::Long("no-build") => a.build = false,
            Arg::Long("no-link") => a.link = false,
            Arg::Long("sdk-path") => sdk_path = Some(PathBuf::from(p.value()?)),
            Arg::Long("sdk-git") => sdk_git = Some(string(p.value()?)?),
            Arg::Long("sdk-branch") => sdk_branch = Some(string(p.value()?)?),
            Arg::Long("config-dir") => a.config_dir = Some(p.value()?.into()),
            other => return Err(other.unexpected()),
        }
    }
    let env = |name| std::env::var_os(name).filter(|v| !v.is_empty());
    sdk_path = sdk_path.or_else(|| env("DECKLY_SDK_PATH").map(PathBuf::from));
    sdk_git = sdk_git.or_else(|| env("DECKLY_SDK_GIT").and_then(|v| v.into_string().ok()));
    a.sdk = match (sdk_path, sdk_git) {
        (Some(_), Some(_)) => {
            return Err("give either --sdk-path or --sdk-git, not both".into());
        }
        (Some(path), None) => Sdk::Path(path),
        (None, Some(url)) => Sdk::Git {
            url,
            branch: sdk_branch,
        },
        (None, None) if sdk_branch.is_some() => {
            return Err("--sdk-branch goes with --sdk-git".into());
        }
        (None, None) => Sdk::CratesIo,
    };
    Ok(Command::Create(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn commands_and_their_options() {
        assert!(matches!(parse_str(&[]), Ok(Command::Help(_))));
        assert!(matches!(parse_str(&["--version"]), Ok(Command::Version)));
        assert!(matches!(
            parse_str(&["dev", "--release", "--no-log"]),
            Ok(Command::Dev {
                common: Common { release: true, .. },
                log: false
            })
        ));
        assert!(matches!(
            parse_str(&["log", "-f", "com.example.a"]),
            Ok(Command::Log { follow: true, id: Some(ref id), .. }) if id == "com.example.a"
        ));
        let Ok(Command::Pack { merge, out, .. }) = parse_str(&[
            "pack",
            "--merge",
            "a.decklyPlugin",
            "b.decklyPlugin",
            "-o",
            "c",
        ]) else {
            panic!("pack --merge");
        };
        assert_eq!(merge.len(), 2);
        assert_eq!(out, Some(PathBuf::from("c")));
        assert!(parse_str(&["pack", "--merge", "a"]).is_err());
        assert!(parse_str(&["build", "--no-log"]).is_err());
        assert!(parse_str(&["frobnicate"]).is_err());
        assert!(
            matches!(parse_str(&["help", "dev"]), Ok(Command::Help(t)) if t.contains("--no-log"))
        );
    }

    #[test]
    fn create_takes_every_answer_as_a_flag() {
        let Ok(Command::Create(a)) = parse_str(&[
            "new",
            "My Plugin",
            "--author",
            "Ada",
            "--id",
            "com.ada.my-plugin",
            "--example",
            "minimal",
            "--license",
            "apache-2.0",
            "--yes",
            "--no-git",
            "--no-ci",
            "--no-build",
            "--no-link",
            "--sdk-path",
            "../sdk",
        ]) else {
            panic!("create");
        };
        assert_eq!(a.name.as_deref(), Some("My Plugin"));
        assert_eq!(a.example, Some(Example::Minimal));
        assert_eq!(a.license, Some(License::Apache));
        assert!(a.yes && !a.git && !a.ci && !a.build && !a.link);
        assert_eq!(a.sdk, Sdk::Path("../sdk".into()));
        assert!(parse_str(&["create", "--example", "huge"]).is_err());
        assert!(parse_str(&["create", "--sdk-branch", "dev"]).is_err());
    }
}
