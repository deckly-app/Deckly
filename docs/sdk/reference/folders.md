# Plugin folders

Deckly keeps plugins and their logs in its configuration folder:

| System | Configuration folder |
| --- | --- |
| Windows | `%APPDATA%\Deckly` |
| macOS | `~/Library/Application Support/Deckly` |
| Linux | `$XDG_CONFIG_HOME/deckly`, else `~/.config/deckly` |

Inside it:

| Path | Holds |
| --- | --- |
| `plugins/<plugin id>/` | An installed Deckly plugin. |
| `plugins/<plugin id>.sdPlugin/` | An installed Stream Deck plugin. |
| `logs/plugins/<plugin id>.log` | A plugin's log: its standard error and Deckly's notes about it. |
| `dev-plugins.json` | Plugin folders linked by [`deckly-cli link`](cli.md), run as development plugins. |
| `dev-run/<plugin id>/` | The copy a development plugin runs from, made afresh at each restart. |
| `instance.port`, `instance.token` | While Deckly runs: the local port it listens on, and the token the plugin tool sends with a restart request. |

The Plugins tab opens the plugins folder with its folder button, and a
plugin's log with **Open Log** in its right-click menu. `deckly-cli` finds
these folders by the same rule, so it works the same on every system.

## A separate Deckly for development

Set `DECKLY_CONFIG_DIR` to run Deckly against another folder, with its own
profiles and plugins, so test plugins stay out of your everyday setup. For
a Deckly built from source:

```sh
DECKLY_CONFIG_DIR=/tmp/deckly-dev cargo dev
```

Give `deckly-cli` the same variable (or `--config-dir`), so it links into
and restarts plugins in that Deckly.

Close your everyday Deckly first: two copies of Deckly cannot drive the same
device.
