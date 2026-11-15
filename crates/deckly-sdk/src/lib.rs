//! Write plugins for Deckly in Rust.
//!
//! A plugin is a program that Deckly starts and talks to over its standard
//! input and output. Each action is a type implementing [`Action`]; Deckly
//! makes one value of it per key that shows the action, so per-key state is
//! a plain field.
//!
//! ```no_run
//! use deckly_sdk::prelude::*;
//!
//! #[derive(Clone, Default, Serialize, Deserialize)]
//! struct CounterSettings {
//!     step: u32,
//! }
//!
//! #[derive(Default)]
//! struct Counter {
//!     count: u32,
//! }
//!
//! impl Action for Counter {
//!     const ID: &'static str = "count";
//!     const NAME: &'static str = "Counter";
//!     type Settings = CounterSettings;
//!
//!     fn key_down(&mut self, key: &Key<Self>) {
//!         self.count += key.settings().step.max(1);
//!     }
//!
//!     fn face(&self, _key: &Key<Self>) -> Option<Face> {
//!         Some(Face::new().value(self.count.to_string()).label("Count"))
//!     }
//! }
//!
//! deckly_sdk::plugin!("com.example.counter", "Counter", [Counter]);
//! ```
//!
//! Standard output belongs to the protocol: anything printed there with
//! `println!` breaks the connection, and Deckly restarts the plugin. Write
//! logs to standard error (`eprintln!`); Deckly keeps them in the plugin's
//! log file. The plugin exits when Deckly closes its standard input.
//!
//! Inside are threads and blocking I/O; there is no async runtime. A
//! [`Key`] is cheap to clone and can be moved to any thread.

pub mod form;
mod globals;
mod key;
mod run;
mod settings;
mod shared;
pub mod testing;

pub use deckly_plugin_protocol as protocol;
pub use deckly_plugin_protocol::{Choice, Color, Face, ManifestState as State};
pub use form::Form;
pub use globals::Globals;
pub use key::{Image, Key};
pub use shared::{Running, Shared};

use deckly_plugin_protocol::{Manifest, ManifestAction, PROTOCOL, Validate};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt;
use std::io::{Read, Write};
use std::sync::Arc;

/// What a plugin author usually needs.
pub mod prelude {
    pub use crate::{
        Action, Choice, Color, Face, Form, Globals, Image, Key, Plugin, Running, Shared, State,
    };
    pub use serde::{Deserialize, Serialize};
}

/// Lists the options of a dynamic choice in the plugin-wide form.
pub type ChoicesHandler =
    Arc<dyn Fn(&str, &Globals) -> std::result::Result<Vec<Choice>, String> + Send + Sync>;
/// Answers a button of the plugin-wide form.
pub type ButtonHandler = Arc<dyn Fn(&str, &Globals) + Send + Sync>;
/// Hears that the global settings changed.
pub type GlobalsHandler = Arc<dyn Fn(&Globals) + Send + Sync>;
/// Runs once when the plugin starts.
pub type StartHandler = Box<dyn FnOnce(&Running) + Send>;

/// The SDK version, written into `ready` and the manifest.
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// Deckly sent something the SDK cannot use, or the plugin was started
    /// by something that is not Deckly.
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Protocol(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<deckly_plugin_protocol::Error> for Error {
    fn from(e: deckly_plugin_protocol::Error) -> Self {
        match e {
            deckly_plugin_protocol::Error::Io(e) => Error::Io(e),
            other => Error::Protocol(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// One kind of key a plugin offers. Deckly creates one value per key that
/// shows the action (with [`Default`]) and drops it when the key goes away.
///
/// Every handler has a default that does nothing.
pub trait Action: Default + Send + 'static {
    /// The action's id. A short name without dots (`"wave"`) stands for
    /// `<plugin id>.wave`; a full id (`"com.example.hello.wave"`) must start
    /// with the plugin id. Profiles store the full id.
    const ID: &'static str;
    /// The name in Deckly's action list.
    const NAME: &'static str;
    const TOOLTIP: &'static str = "";
    /// An image file inside the plugin folder, for the action list.
    const ICON: &'static str = "";

    /// The key's settings, stored by Deckly in the profile. Keys start with
    /// `Default`. Saved settings are read over `Default`: a field they lack,
    /// or one that no longer fits its type, keeps its default value and the
    /// other fields keep theirs.
    type Settings: Serialize + DeserializeOwned + Default + Clone + Send + 'static;

    /// The states a key can be in, with the title and image Deckly shows
    /// before the plugin draws anything. None means one plain state.
    fn states() -> Vec<State> {
        Vec::new()
    }

    /// The key became visible: on a device, or in the editor.
    fn key_appear(&mut self, _key: &Key<Self>) {}
    /// The key is no longer visible. The value is dropped right after.
    fn key_disappear(&mut self, _key: &Key<Self>) {}
    fn key_down(&mut self, _key: &Key<Self>) {}
    fn key_up(&mut self, _key: &Key<Self>) {}
    /// The user changed the key's settings in Deckly.
    fn settings_changed(&mut self, _key: &Key<Self>) {}
    /// The key's timer fired; see [`Key::every`].
    fn tick(&mut self, _key: &Key<Self>) {}

    /// Describes the settings form Deckly shows for a key with this action.
    /// Each field's key names a field of `Settings`.
    fn form(_form: &mut Form) {}

    /// The options of a choice field marked [`dynamic`](form::FieldBuilder::dynamic),
    /// asked each time a key's form opens. Runs on a thread of its own, so
    /// a slow answer does not hold up key presses; Deckly stops waiting
    /// after a few seconds and offers a retry. An error is shown to the user.
    fn choices(
        field: &str,
        _settings: &Self::Settings,
        _globals: &Globals,
    ) -> std::result::Result<Vec<Choice>, String> {
        Err(format!("no options for {field}"))
    }

    /// The user pressed a button in the key's form. The key may be in the
    /// editor only, without a device showing it.
    fn button(&mut self, _key: &Key<Self>, _button: &str) {}

    /// The plugin's global settings changed; see [`Key::globals`].
    fn global_settings_changed(&mut self, _key: &Key<Self>) {}

    /// What the key shows, drawn by Deckly. The SDK asks after the key
    /// appears, after every event and tick, and after any change to a
    /// [`Shared`] value, and sends the face only when it differs from the
    /// one shown. `None`, the default, leaves the key
    /// to [`Key::set_face`] and the other `set_` calls.
    fn face(&self, _key: &Key<Self>) -> Option<Face> {
        None
    }
}

/// The full id of an action: a short id (no dots) is put after the plugin
/// id.
pub(crate) fn full_action_id(plugin: &str, action: &str) -> String {
    if action.contains('.') {
        action.to_owned()
    } else {
        format!("{plugin}.{action}")
    }
}

/// Builds the plugin and its `main` in one line:
///
/// ```no_run
/// # use deckly_sdk::prelude::*;
/// # #[derive(Default)] struct Wave;
/// # impl Action for Wave {
/// #     const ID: &'static str = "wave";
/// #     const NAME: &'static str = "Wave";
/// #     type Settings = serde_json::Value;
/// # }
/// deckly_sdk::plugin!("com.example.hello", "Hello", [Wave]);
/// ```
///
/// It defines `fn plugin() -> Plugin`, for the tests, and `fn main()`. The
/// version, authors, licence and description come from `Cargo.toml`. For
/// anything more (icons, a plugin-wide form) build the [`Plugin`] yourself.
#[macro_export]
macro_rules! plugin {
    ($id:expr, $name:expr, [$($action:ty),* $(,)?] $(,)?) => {
        /// The plugin, as `main` runs it and the tests start it.
        fn plugin() -> $crate::Plugin {
            $crate::Plugin::new($id, $name)
                .version(env!("CARGO_PKG_VERSION"))
                .author(&env!("CARGO_PKG_AUTHORS").replace(':', ", "))
                .license(env!("CARGO_PKG_LICENSE"))
                .description(env!("CARGO_PKG_DESCRIPTION"))
                $(.action::<$action>())*
        }

        fn main() {
            plugin().run_or_exit()
        }
    };
}

/// A plugin: its description and its actions.
pub struct Plugin {
    manifest: Manifest,
    makers: Vec<run::Maker>,
    /// Mistakes found while adding actions, reported by [`Plugin::check`].
    problems: Vec<String>,
    pub(crate) on_choices: Option<ChoicesHandler>,
    pub(crate) on_button: Option<ButtonHandler>,
    pub(crate) on_globals: Option<GlobalsHandler>,
    pub(crate) on_start: Option<StartHandler>,
}

impl Plugin {
    /// `id` is a reverse-DNS name (`com.example.counter`); `name` is shown
    /// to the user.
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            manifest: Manifest {
                id: id.to_owned(),
                name: name.to_owned(),
                version: "0.0.1".to_owned(),
                protocol: PROTOCOL,
                sdk: SDK_VERSION.to_owned(),
                ..Manifest::default()
            },
            makers: Vec::new(),
            problems: Vec::new(),
            on_choices: None,
            on_button: None,
            on_globals: None,
            on_start: None,
        }
    }

    pub fn version(mut self, version: &str) -> Self {
        version.clone_into(&mut self.manifest.version);
        self
    }

    pub fn author(mut self, author: &str) -> Self {
        author.clone_into(&mut self.manifest.author);
        self
    }

    pub fn license(mut self, license: &str) -> Self {
        license.clone_into(&mut self.manifest.license);
        self
    }

    pub fn description(mut self, text: &str) -> Self {
        text.clone_into(&mut self.manifest.description);
        self
    }

    /// An image file inside the plugin folder.
    pub fn icon(mut self, path: &str) -> Self {
        path.clone_into(&mut self.manifest.icon);
        self
    }

    /// The group in Deckly's action list; the plugin name by default.
    pub fn category(mut self, category: &str) -> Self {
        category.clone_into(&mut self.manifest.category);
        self
    }

    pub fn action<A: Action>(mut self) -> Self {
        let id = full_action_id(&self.manifest.id, A::ID);
        if !id.starts_with(&format!("{}.", self.manifest.id)) {
            self.problems.push(format!(
                "action {id} does not start with the plugin id {}; use a short id such as \"{}\"",
                self.manifest.id,
                id.rsplit('.').next().unwrap_or_default()
            ));
        }
        let mut form = Form::default();
        A::form(&mut form);
        let form = form.into_description();
        if let Some(form) = &form {
            self.problems.extend(unknown_fields::<A>(&id, form));
        }
        self.manifest.actions.push(ManifestAction {
            id: id.clone(),
            name: A::NAME.to_owned(),
            icon: A::ICON.to_owned(),
            tooltip: A::TOOLTIP.to_owned(),
            states: A::states(),
            form,
        });
        self.makers.push(run::maker::<A>(id));
        self
    }

    /// The plugin-wide settings form, for the global settings shared by
    /// every key ([`Globals`]). Deckly shows it where the plugin is managed.
    pub fn form(mut self, describe: impl FnOnce(&mut Form)) -> Self {
        let mut form = Form::default();
        describe(&mut form);
        self.manifest.form = form.into_description();
        self
    }

    /// Lists the options of a dynamic choice in the plugin-wide form. Runs
    /// on a thread of its own, like [`Action::choices`].
    pub fn on_choices(
        mut self,
        handler: impl Fn(&str, &Globals) -> std::result::Result<Vec<Choice>, String>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.on_choices = Some(Arc::new(handler));
        self
    }

    /// Answers a button of the plugin-wide form.
    pub fn on_button(mut self, handler: impl Fn(&str, &Globals) + Send + Sync + 'static) -> Self {
        self.on_button = Some(Arc::new(handler));
        self
    }

    /// Hears that the global settings changed, before the keys do.
    pub fn on_global_settings(
        mut self,
        handler: impl Fn(&Globals) + Send + Sync + 'static,
    ) -> Self {
        self.on_globals = Some(Arc::new(handler));
        self
    }

    /// Runs once when the plugin starts, before any key appears, on the
    /// thread that handles key events. The place to start a thread that
    /// listens to another program and changes a [`Shared`] value:
    ///
    /// ```no_run
    /// # use deckly_sdk::prelude::*;
    /// #[derive(Default)]
    /// struct Status {
    ///     online: bool,
    /// }
    ///
    /// let plugin = Plugin::new("com.example.status", "Status").on_start(|plugin| {
    ///     let status = plugin.shared::<Status>();
    ///     std::thread::spawn(move || status.update(|s| s.online = true));
    /// });
    /// ```
    pub fn on_start(mut self, handler: impl FnOnce(&Running) + Send + 'static) -> Self {
        self.on_start = Some(Box::new(handler));
        self
    }

    /// Checks the manifest as Deckly will: a form over a limit or with a
    /// default its field refuses is an error here rather than in Deckly.
    /// Also checks what Deckly cannot see: that every action id starts with
    /// the plugin id, and that every form field names a field of the
    /// action's settings (as `Default` serializes them), so a typo in a key
    /// is found by a test.
    pub fn check(&self) -> Result<()> {
        if let Some(problem) = self.problems.first() {
            return Err(Error::Protocol(problem.clone()));
        }
        self.manifest()
            .validate()
            .map_err(|e| Error::Protocol(format!("the manifest is not valid: {e}")))
    }

    /// The manifest (`deckly-plugin.json`) for this plugin, naming the
    /// running program as the one for this target.
    pub fn manifest(&self) -> Manifest {
        let mut m = self.manifest.clone();
        let program = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name()?.to_str().map(str::to_owned));
        if let Some(program) = program {
            m.programs
                .insert(deckly_plugin_protocol::current_target(), program);
        }
        m
    }

    /// Runs the plugin on standard input and output until Deckly closes
    /// standard input. With `--describe` it prints the manifest instead.
    pub fn run(self) -> Result<()> {
        if std::env::args().skip(1).any(|a| a == "--describe") {
            self.check()?;
            let text = serde_json::to_string_pretty(&self.manifest())
                .map_err(|e| Error::Protocol(e.to_string()))?;
            println!("{text}");
            return Ok(());
        }
        run::run(self, std::io::stdin(), std::io::stdout(), true)
    }

    /// Runs the plugin like [`Plugin::run`]; on an error it prints the error
    /// to standard error and exits with status 1. The `main` that
    /// [`plugin!`] writes calls it.
    pub fn run_or_exit(self) {
        let name = self.manifest.name.clone();
        if let Err(e) = self.run() {
            eprintln!("{name}: {e}");
            std::process::exit(1);
        }
    }

    /// Runs the plugin on any pair of streams; [`testing::Harness`] uses it.
    pub fn run_with<R, W>(self, input: R, output: W) -> Result<()>
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        run::run(self, input, output, false)
    }
}

/// The form fields of action `id` whose keys are not fields of its
/// settings. Settings that do not serialize to an object are not checked.
fn unknown_fields<A: Action>(id: &str, form: &protocol::Form) -> Vec<String> {
    let Ok(serde_json::Value::Object(fields)) = serde_json::to_value(A::Settings::default()) else {
        return Vec::new();
    };
    form.fields
        .iter()
        .filter(|f| f.kind.has_value() && !fields.contains_key(&f.key))
        .map(|f| {
            format!(
                "the form of action {id} has a field {:?} that its settings do not have",
                f.key
            )
        })
        .collect()
}

/// The pages of the SDK book (`docs/sdk`), so `cargo test` compiles and runs
/// their Rust examples against this crate. `cargo test -p xtask` checks that
/// this list stays complete.
#[cfg(doctest)]
mod book {
    macro_rules! pages {
        ($($name:ident = $path:literal),* $(,)?) => {$(
            #[doc = include_str!(concat!("../../../docs/sdk/", $path))]
            struct $name;
        )*};
    }

    pages! {
        Introduction = "README.md",
        GettingStarted = "getting-started.md",
        Actions = "guide/actions.md",
        Drawing = "guide/drawing.md",
        Timers = "guide/timers.md",
        SharedState = "guide/shared-state.md",
        Settings = "guide/settings.md",
        Forms = "guide/forms.md",
        GlobalSettings = "guide/global-settings.md",
        Testing = "guide/testing.md",
        Packaging = "guide/packaging.md",
        Troubleshooting = "guide/troubleshooting.md",
        Manifest = "reference/manifest.md",
        FormFields = "reference/form-fields.md",
        Limits = "reference/limits.md",
        Folders = "reference/folders.md",
        Cli = "reference/cli.md",
        Protocol = "reference/protocol.md",
        Api = "reference/api.md",
        StreamDeckPlugins = "stream-deck-plugins.md",
        About = "about.md",
    }
}
