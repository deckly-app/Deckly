# Form fields

Forms are built with [`Form`] in `Action::form` (a key's settings) and
`Plugin::form` (the global settings). The guide is
[Settings forms](../guide/forms.md).

## Kinds

| Builder | Manifest `kind` | Value in settings | Shown as |
| --- | --- | --- | --- |
| `form.text(key, label)` | `text` | string | A text box; several lines with `.multiline()`. |
| `form.secret(key, label)` | `secret` | string | A hidden text box. The value goes to the system's secret store. |
| `form.number(key, label)` | `number` | number | A number box. |
| `form.slider(key, label, a..=b)` | `slider` | number | A slider. Needs both bounds. |
| `form.checkbox(key, label)` | `checkbox` | `true` / `false` | A check box, the label next to it. |
| `form.choice(key, label)` | `choice` | string | A drop-down list. |
| `form.file(key, label)` | `file` | string | A path with a file picker. |
| `form.folder(key, label)` | `folder` | string | A path with a folder picker. |
| `form.color(key, label)` | `color` | `"#rrggbb"` | A colour picker. |
| `form.hotkey(key, label)` | `hotkey` | object, see [Hotkey](#hotkey) | A key recorder. |
| `form.button(key, label)` | `button` | none | A button; calls the `button` handler with `key`. |
| `form.section(label)` | `section` | none | A heading over the fields after it. |
| `form.note(text)` | `note` | none | A line of text. |

## Properties

Each builder returns a `FieldBuilder`; chain these on it. A property that
does not apply to the field's kind is ignored.

| Method | Manifest field | Applies to | Does |
| --- | --- | --- | --- |
| `.help(text)` | `help` | all | A line under the field. |
| `.placeholder(text)` | `placeholder` | text, secret | Shown while the field is empty. |
| `.default(value)` | `default` | fields with a value | Shown while the settings hold no value. Must pass the field's own checks. |
| `.required()` | `required` | fields with a value | An empty value is refused. |
| `.range(a..=b)` | `min`, `max`, `integer` | number, slider | Bounds. An integer range also sets `integer`. |
| `.min(a)`, `.max(b)` | `min`, `max` | number, slider | One bound. |
| `.step(s)` | `step` | number, slider | How far one step moves. |
| `.integer()` | `integer` | number, slider | Whole numbers only. |
| `.length(a..=b)` | `min_length`, `max_length` | text, secret | Length in characters. |
| `.multiline()` | `multiline` | text | Several lines. |
| `.option(value, label)` | `options` | choice | Adds a fixed option. |
| `.dynamic()` | `dynamic` | choice | Options come from the plugin's `choices` handler each time the form opens. |
| `.extensions(&["wav", "ogg"])` | `extensions` | file | The file types offered, without the dot. |
| `.visible_when(key, value)` | `visible_when` | all | Shown only while field `key` holds `value`. |

## Checks and their messages

Deckly checks input as the user types and shows the message under the
field. Input that fails stays in the field and is not saved.

| Check | Message |
| --- | --- |
| `required` | This field is required. |
| `length` | Use at least *n* characters. / Use at most *n* characters. |
| `integer` | Enter a whole number. |
| `min`, `max` | Enter a number from *a* to *b*. |
| A fixed choice | Choose one of the options. |
| `color` | Choose a colour. |
| `hotkey` | Record a key combination. |

A dynamic choice accepts any string. Hidden fields (see `visible_when`)
keep their values and are not checked.

## Conditions

`visible_when(key, value)` compares the other field's value (or its default,
while the settings hold none) with `value`. A field whose condition names a
hidden field is hidden too. A condition on a property that no field shows
uses the stored setting.

## Hotkey

A hotkey field stores an object:

```json
{ "key": "K", "ctrl": true, "shift": false, "alt": false, "meta": false }
```

A settings type for it:

```rust
use deckly_sdk::prelude::*;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Hotkey {
    key: String,
    ctrl: bool,
    shift: bool,
    alt: bool,
    meta: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct ShortcutSettings {
    shortcut: Hotkey,
}
# fn main() {}
```

## Limits

| What | Limit |
| --- | --- |
| Fields in one form, sections and notes included | 64; the SDK leaves out the rest with a warning |
| A field key | 64 bytes: letters, digits, `_`, `-`, `.` |
| A label, placeholder or option label | 100 characters |
| A help line or note | 500 characters |
| Options of one choice | 256, values of up to 256 bytes |
| File extensions | 16, of up to 16 bytes |
| A default or condition value | 4 KiB as JSON |
| A text or secret value | 8192 characters |
| A whole form | 64 KiB as JSON |

The SDK cuts text to these limits. `Plugin::check()` and `--describe`
refuse a form that still breaks a rule, such as two fields with the same
key or a default its field would refuse.

[`Form`]: https://deckly-app.github.io/Deckly/api/deckly_sdk/form/struct.Form.html
