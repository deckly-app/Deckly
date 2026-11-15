# %%name_text%%

%%description_text%%

A plugin for [Deckly](https://github.com/deckly-app/Deckly), written with the
[Deckly plugin SDK](https://deckly-app.github.io/Deckly/). Plugin id:
`%%id_text%%`.

## Develop

```sh
deckly-cli dev
```

Builds the plugin, links it into the Deckly running on this computer and
restarts it there on every save. The plugin's log is printed as it grows.
In Deckly, find **%%name_text%%** in the action list and drag an action onto a key.
Ctrl+C stops; `deckly-cli unlink` removes the link.

## Test

```sh
cargo test
deckly-cli validate
```

`cargo test` runs the tests in `src/main.rs` against the SDK's test harness,
without Deckly or a device. `deckly-cli validate` checks the plugin folder
and starts the plugin once, as Deckly does.

## Pack

```sh
deckly-cli pack
```

Makes one `.decklyPlugin` file in `target/deckly` for this system. Install
it in Deckly with **Add File…** in the Plugins tab, or drop it on the
window.

## Release

Push a tag such as `v0.0.1`. The workflow in `.github/workflows/release.yml`
packs the plugin on Linux, Windows and macOS, joins the files into one with
`deckly-cli pack --merge`, and attaches it to a GitHub release.
