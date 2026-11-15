# Limits

Deckly and the SDK share these bounds (`deckly_sdk::protocol::limits`). The
SDK never sends more: it cuts text and skips what does not fit, with a line
in the log. Deckly treats anything larger as a protocol error.

## Keys

| What | Limit |
| --- | --- |
| A title | 200 characters |
| One text line of a face | 32 characters |
| An image | 4 MiB |
| A raw (RGBA) image | 1024 pixels on a side |
| Timer period | at least 10 ms |

## Settings

| What | Limit |
| --- | --- |
| A key's settings | 64 KiB as JSON |
| The global settings | 64 KiB as JSON |

## Manifest

| What | Limit |
| --- | --- |
| `deckly-plugin.json` | 1 MiB |
| Plugin and action ids | 128 bytes |
| Name, author, licence, tooltip, category | 200 characters |
| Description | 2000 characters |
| A path in the plugin folder | 260 bytes |
| Actions | 256 |
| States of one action | 16 |
| Programs | 32 |

## Forms

| What | Limit |
| --- | --- |
| Fields of one form | 64 |
| A whole form | 64 KiB as JSON |
| A field key | 64 bytes |
| A label | 100 characters |
| A help line or note | 500 characters |
| Options of a choice | 256 |
| An option value | 256 bytes |
| An error from `choices` | 200 characters |
| A text or secret value | 8192 characters |

## Messages

| What | Limit |
| --- | --- |
| One JSON message | 1 MiB |
| One binary blob | 4 MiB |

## Time

| What | Limit |
| --- | --- |
| Answer to `hello` (calling `run()`) | 10 s |
| Answer to a dynamic choice | 5 s |
| Exit after Deckly closes standard input | 3 s, then the plugin is killed |
