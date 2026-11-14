# Stream Deck plugin CLI and getting-started flow

Follows [Plugin host](2026-10-28-plugin-host.md). Input to
[docs/plugin-cli.md](../plugin-cli.md), the proposal for Deckly's plugin tool.

- Date: 2026-11-14.
- Sources and method:
  - Elgato's public documentation, read as web pages: the SDK introduction
    (getting started, first changes, plugin environment, distribution), the
    guides on actions, settings and logging, and every page of the CLI
    reference (intro, `create`, `link`, `unlink`, `restart`, `stop`, `dev`,
    `validate`, `pack`, `config`, `list`), all on docs.elgato.com. SDK docs
    were at version 3.0.0 as the site showed it.
  - The public README of the CLI's GitHub repository (the repository is
    archived, read-only since 2026-08-19; the README points to the SDK
    repository).
  - The public demo animation of `create` in the SDK repository's `assets`
    folder, looked at in a browser. Only part of it was visible (the left
    part of the terminal window was cut off), so only the beginning and the
    confirmation step were seen.
  - A report from the project owner, who ran the CLI on Linux (below).
  - Not used: the CLI's or the SDK's source code, project templates, file
    contents of a generated project, or any asset. Nothing was installed or
    run by us.
- Device: none. Official app: not run.
- Checked by: nothing yet; this entry is the input to a design, no Deckly code
  was written. Tests come with the implementation.

## Observed

Install and prerequisites (docs):

- The CLI is an npm package installed globally; it needs Node.js 24 or newer
  and a Stream Deck app 7.1 or newer. The docs recommend a Node version
  manager (nvm, nvm-windows). They mention an execution-policy change for
  PowerShell on Windows. `sd` is a short alias of `streamdeck`. No page lists
  supported operating systems, and Linux is not mentioned.

The commands (docs), one line each:

- `create`: an interactive wizard that makes a new plugin folder in the
  current directory, named after the plugin in lowercase.
- `link [path]`: installs a folder into the app without copying it. The folder
  must be named `<uuid>.sdPlugin`.
- `unlink <uuid>`: removes a linked plugin from the app; the source folder is
  left alone. A flag also removes plugins that were installed normally.
- `list`: shows linked plugins and where they come from; a flag shows all.
- `restart <uuid>` (alias `r`): stops and starts the plugin; all of its
  resources including the manifest are read again. Starts it if it was not
  running.
- `stop <uuid>` (alias `s`): stops the plugin and unloads its files so they
  can be changed.
- `dev`: turns developer mode on or off. On, a debugger can attach to Node
  plugins and a Property Inspector can be debugged in an ordinary browser on a
  local port.
- `validate [path]`: checks a plugin and prints the result. Rules are
  downloaded and refreshed by the tool itself (a check about once a day,
  skippable for build pipelines).
- `pack [path]` (alias `bundle`): validates, then makes one installer file
  (`.streamDeckPlugin`). Everything under the folder goes in except `.git`,
  `.env*`, `*.log` and `*.js.map`; a `.sdignore` file in the plugin folder
  (gitignore syntax) adds exclusions. Flags: set the version in the manifest,
  output folder, overwrite, dry run, hide the file list.
- `config set/unset/list/reset`: two settings, a reduced-motion switch for the
  busy indicator and the package manager (npm, pnpm, yarn, bun) used by
  `create`.

`create` (docs, README and the animation):

- Prompts, in order: author, plugin name, plugin UUID, description; then a
  yes/no confirmation that defaults to yes. The docs list no defaults and no
  validation rules, only sample answers. The UUID rule is stated in the
  getting-started page: reverse-DNS, lowercase letters, digits, hyphens and
  periods, and it can never change after publishing.
- After the confirmation it shows progress steps: turn developer mode on,
  generate the plugin, install dependencies, build, finish. It ends by saying
  the plugin is installed in the app and ready, and asks whether to open the
  folder in VS Code (yes by default).
- The animation shows: the command typed, an ASCII-art banner, one line saying
  this is a creation wizard for a local development environment for a plugin
  with a link to the docs, the prompts, and the confirmation with "Yes"
  highlighted.
- The generated project, from the getting-started page's tree: a
  `<uuid>.sdPlugin` folder (compiled output, images, logs, a `ui` folder with
  one Property Inspector page, `manifest.json`), a `src` folder with an entry
  file and one action class, and top-level `package.json`, a bundler config and
  a `tsconfig.json`. The page names the entry file two ways (`index.ts` and
  `plugin.ts`).

Development loop (docs):

- After `create`, `npm run watch` is the whole loop: the bundler rebuilds when
  the manifest or the sources change and, when a build ends, runs the CLI's
  `restart` for the plugin's UUID. Only the plugin process restarts, not the
  app. Without watch: build, then `restart <uuid>` by hand.
- Logs: the SDK's logger writes to a file under the plugin's own folder,
  `logs/<uuid>.<n>.log` (0 is newest), ten files of at most 10 MiB, rotated on
  start. In development, logs are also copied to the terminal. Uninstalling
  the plugin deletes its logs.
- Debugging a Node plugin: attach a debugger to the plugin's Node process; the
  app picks the port unless the manifest sets one.
- If a newly installed plugin does not appear, the docs suggest restarting the
  app (it may be running with elevated rights after an install or update).

What a plugin author writes (docs):

- The manifest is written by hand in `manifest.json`: id, name, version, one
  entry per action (id, icon, tooltip, controllers, states), code path, SDK
  version, minimum app version, operating systems, runtime version.
- An action is one class with an id on a decorator, extending a base class and
  overriding event methods (appears, key down, settings received and others).
  The entry file registers each action and calls connect last. A minimal key
  press logger is about fifteen lines; a counter with settings and a title is
  twenty to twenty-five.
- Settings are JSON; values can be missing, so the author guards with
  defaults or a validator. Action settings are stored as plain text and go
  into exported profiles; the docs say not to keep secrets there and
  recommend global settings for user-given tokens.
- A settings page is an HTML file using the vendor's component library, one
  page per action or one for the plugin.

Publishing (docs):

- `pack` makes the installer file; submission to the marketplace goes through
  the vendor's Maker Console website after review of guidelines, an icon and
  gallery items. The CLI has no publish command.
- Since CLI 1.6, packaging prepares for copy protection when the SDK and
  manifest versions allow it; it applies only after upload, so it can be tried
  only through the console. Files must not change after shipping.

Linux (from the owner, 2026-11-14, not reproduced by us):

- `streamdeck create` on Linux asked for author and plugin name, then crashed
  while checking the plugin UUID. The owner reports that the check scans
  `~/AppData/Roaming/Elgato/StreamDeck/Plugins`, a folder that exists only on
  Windows (and, with another path, macOS). No stack trace or log was kept.

## Inferred

- `link` works by registering the folder's location with the app, not by
  copying: `unlink` leaves the source alone and `list` prints "source paths".
  How the registration is stored (a file, a symlink, an app call) is not
  said.
- `restart` talks to the running app, since the plugin process belongs to the
  app. The docs do not say how (a local port or a file).
- `create` checks the new UUID against the installed plugins to avoid a
  clash; this follows from the owner's crash report and is not stated in the
  docs.
- The experience people like is the chain of three things: one command to a
  ready project, one command that rebuilds and reloads on save, and one file
  to share. The docs show no other single feature that dominates.
- A hand-written manifest beside the code keeps each action's id in two
  places (the manifest entry and the class), so they can drift apart. This is
  our reading of the layout, not something the docs say.

## Unknown

- The exact prompts' wording, their defaults, and how they validate input; the
  animation was only partly visible.
- Which rules `validate` applies; the docs say only that rules are downloaded.
- Where the CLI keeps its settings and links, and how it reaches the running
  app.
- Whether the CLI works on Linux when the app does not exist there at all (the
  official app has no Linux version); the owner's report says it does not
  reach the end of `create`.
- What the generated example looks like on disk beyond the tree in the docs.

## Deckly decisions

- Deckly's own plugin tool follows the experience (one command to a working
  project, a live reload loop, one file to share), not the code, wording or
  layout. See [docs/plugin-cli.md](../plugin-cli.md).
- Where Elgato's tool fails the owner (it does not run on Linux), Deckly's
  must be tested on Linux, Windows and macOS before it is called done.
