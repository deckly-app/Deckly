# Logs and troubleshooting

## The log

Everything the plugin writes to standard error goes to its log file, next
to Deckly's own notes about it (started, stopped, protocol errors). Open it
from the Plugins tab: right-click the plugin, **Open Log**. The files are in
the [logs folder](../reference/folders.md).

```rust
# fn main() {
# let city = "Oslo";
eprintln!("fetching the weather for {city}");
# }
```

Standard output is not for logging: it carries the protocol. The SDK
writes its own warnings to standard error too, starting with `deckly-sdk:`.

To see every message between Deckly and its plugins, start Deckly with
`DECKLY_PLUGIN_DEBUG=1`. The log then names the kind of each message (not
its contents, since settings can hold secrets).

## Restarts

When a plugin exits, crashes or breaks a protocol rule, Deckly restarts it
after 1 s, then 2 s, 4 s and so on. After 5 crashes within 10 minutes it
stops trying and the Plugins tab shows the plugin as failed. **Restart** in
the plugin's menu starts it again.

A panic in a handler ends the plugin, so it restarts and every key's state
(the fields of your actions) is lost. A panic in `choices` is the exception:
the form shows it as an error. Return errors instead of panicking, and show
them with `key.show_alert()` and a line in the log.

Run `deckly-cli log -f` in the project to follow the log in a terminal;
`deckly-cli dev` prints it as it grows.

## Check before Deckly

`deckly-cli validate` builds the plugin, checks its folder as Deckly will,
and starts it once as Deckly does. Most of the problems below show up there
first, with a plain explanation: a `println!`, slow work before the plugin
answers, a plugin that does not stop when its input closes, a missing icon,
an action id outside the plugin id.

## Common problems

**The plugin restarts again and again, and the log says "text was written
to standard output".**
Something printed to standard output: a `println!` or `print!`, or a
library that prints. Use `eprintln!` instead; `dbg!` is fine, it writes to
standard error.

**The plugin is not in the Plugins tab.**
Deckly reads `deckly-plugin.json` when it starts, when a plugin is
installed, and when `deckly-cli` asks it to. Run `deckly-cli dev` (or
`deckly-cli link`) in the project. If `dev` says Deckly is not running
although it is, the two use different settings folders: give `deckly-cli`
the same `DECKLY_CONFIG_DIR` (or `--config-dir`) as Deckly.

**The plugin shows "the linked folder … is missing".**
A development plugin's folder is gone, for example after `cargo clean`.
Run `deckly-cli dev` again, or `deckly-cli unlink`.

**`deckly-cli dev` says Deckly cannot restart plugins on request.**
That Deckly is older than the tool. Update Deckly, or press **Restart** in
the plugin's menu after each build.

**"has no program for x86_64-…"**
The plugin file holds no program for this system. Pack it on this system
too and join the files with `deckly-cli pack --merge`. See
[Several systems](packaging.md#several-systems).

**"needs a newer Deckly"**
The plugin was built with an SDK that speaks a newer protocol than this
Deckly. Update Deckly.

**"did not answer Deckly's hello in time"**
`main` did something slow (or waited) before calling `run()`. Call `run()`
first and do the slow work later, on a thread.

**The key shows only the action's icon.**
Without a connected device, keys get no events, so faces are not drawn. With
a device, check that the action's `face` returns `Some`, or that
`key_appear` draws something.

**A setting goes back to its default.**
The stored value does not fit the field's type; the log has a line
`settings of key …: <field> did not fit; using the default`. Check the
field types against the form (an `integer` range for whole numbers). The
other fields keep their values.

**Running the program by hand prints "standard input closed before hello".**
That is expected: a plugin is started by Deckly. Run it with `--describe`
to see its manifest, and test it with the [harness](testing.md).

**Keys are slow to react.**
A handler is blocking the plugin's one event thread. Move slow work to a
thread of its own; see [Timers and threads](timers.md#slow-work-on-another-thread).
