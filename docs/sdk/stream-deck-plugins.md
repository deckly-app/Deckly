# Stream Deck plugins in Deckly

Deckly runs two kinds of plugins side by side:

| | Deckly plugins | Stream Deck plugins |
| --- | --- | --- |
| Written with | This SDK, in Rust | The Stream Deck SDK, mostly JavaScript |
| Talks to Deckly over | Standard input and output | A local WebSocket |
| Settings UI | A form Deckly draws | An HTML page (Property Inspector) |
| Folder | `<id>/` with `deckly-plugin.json` | `<id>.sdPlugin/` with `manifest.json` |
| Runs in the official app | No | Yes |

## Running an existing Stream Deck plugin

You do not need to change it. Install it from the Plugins tab: **Add File…**
for a `.streamDeckPlugin` file or a zip that holds one, **Add Folder…** for
a `.sdPlugin` folder. Deckly then:

- runs Node.js plugins on the Node.js version their manifest asks for,
  using `node` from `PATH` or downloading the official build once;
- shows HTML settings pages in a separate helper process with the system's
  web view, never inside Deckly itself;
- on Linux and macOS, runs Windows-only programs through Wine when it is
  installed, as a best effort.

Some plugins check that they were started by the official app and stay
disabled elsewhere. Deckly does not work around that; only the plugin's
author can change it. Copies installed through the official marketplace are
encrypted and cannot be used; get the plugin from its author instead.

The details are in the
[Deckly README](https://github.com/deckly-app/Deckly#plugins).

## Which to write

Write a **Deckly plugin** when you target Deckly: it is one small native
program per system, starts fast, draws keys and forms in the user's style
without HTML, and is tested with `cargo test`.

Write a **Stream Deck plugin** when it must also run in the official
Stream Deck app. Deckly runs it too, but its settings pages need the web
view helper and Node.js plugins need a Node.js runtime.

Deckly is an independent project and is not made, approved or supported by
Elgato or Corsair.
