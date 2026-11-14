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

The Plugins tab opens the plugins folder with its folder button, and a
plugin's log with **Open Log** in its right-click menu.

## A separate Deckly for development

Set `DECKLY_CONFIG_DIR` to run Deckly against another folder, with its own
profiles and plugins, so test plugins stay out of your everyday setup. For
a Deckly built from source:

```sh
DECKLY_CONFIG_DIR=/tmp/deckly-dev cargo dev
```

Close your everyday Deckly first: two copies of Deckly cannot drive the same
device.
