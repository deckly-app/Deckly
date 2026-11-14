# About these docs

These docs are Markdown files in
[`docs/sdk`](https://github.com/deckly-app/Deckly/tree/dev/docs/sdk) of
the Deckly repository. The same files make:

- this site, built with [mdBook](https://rust-lang.github.io/mdBook/);
- [`llms.txt`](https://deckly-app.github.io/Deckly/llms.txt) and
  [`llms-full.txt`](https://deckly-app.github.io/Deckly/llms-full.txt) for
  AI agents;
- the pages you read on GitHub.

## Read offline

From a clone of the Deckly repository:

```sh
cargo install mdbook
mdbook serve docs/sdk --open
```

Or build the whole site, with the API reference and the agent files, into
`target/book`:

```sh
cargo dev book
```

## Change them

Every page has an edit button at the top right. Changes go through the
usual Deckly workflow (see
[CONTRIBUTING.md](https://github.com/deckly-app/Deckly/blob/dev/CONTRIBUTING.md)).

The Rust examples on these pages are tested:

- `cargo test -p deckly-sdk` compiles every `rust` block, and runs those
  not marked `no_run`, against the SDK. Lines starting with `# ` are
  hidden on the site but are part of the test.
- The code on Getting started is a copy of `examples/hello-deckly`, which
  has its own tests. A comment `<!-- example: path:anchor -->` before a code
  block ties it to the lines between `ANCHOR: anchor` and `ANCHOR_END:
  anchor` in that file.
- `cargo test -p xtask` checks that every page is in `SUMMARY.md`, that
  links between pages work, that every page is tested, that every code
  block names its language, and that copied examples match their source.

Write short pages, code first, and keep one topic per page.
