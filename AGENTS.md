# Deckly — agent instructions

Read this file first. It applies to every human and AI contributor.

## 1. Project goal

Build an independent, cross-platform desktop application for Elgato Stream Deck hardware on Windows, Linux, and macOS. Aim for 1:1 functional and workflow parity with the official Stream Deck app (profiles, pages, folders, multi-actions, built-in actions, device settings, plugins) while creating an independently written application.

Deckly is unofficial. Never imply that it is made, approved, or supported by Elgato or Corsair. Do not put "Stream Deck" or "Elgato" in the product name, binary names, crate names, or icons. Nominative use in text ("works with Stream Deck MK.2", "runs Stream Deck SDK plugins") is fine.

The product name is written `Deckly` in user-facing text; crates, binaries, and identifiers stay lowercase (`deckly-*`).

## 2. Core requirements

- Write all first-party application, device-protocol, plugin-host, CLI, build, and test code in Rust. Do not add JavaScript, TypeScript, Python, C, or C++ application code, Electron, or a webview to the main application UI.
- Use egui for UI, eframe for the native shell, and wgpu as the renderer. Use the framework renderer; avoid custom low-level wgpu code unless a measured requirement calls for it.
- Prefer pure-Rust dependencies and keep the tree small. Use OS libraries or FFI only for device access or desktop integration (HID, input injection, tray, autostart); isolate that code in small platform modules and explain why it is needed.
- Windows, Linux, and macOS are first-class. Keep shared logic portable. Do not silently make a feature Windows-only; a platform that lacks a feature returns a clear "unsupported" error.
- Keep the MIT license. Do not change it without the project owner's approval. Keep third-party notices intact.
- Use SVG icons or one consistent Rust icon library (egui-phosphor). Never use Unicode characters or emoji as substitute UI icons.

### Plugin compatibility

Running existing Stream Deck SDK plugins unchanged is a goal. Plugins are third-party programs, not Deckly code:

- Deckly implements the **host side** of the publicly documented plugin protocol (manifest + WebSocket JSON events) in Rust.
- Plugins run as separate child processes with their own runtime: the plugin's native executable, or a Node.js runtime for Node-based plugins. Deckly never embeds a JavaScript engine in its own process and never ships first-party JavaScript.
- Property Inspectors are HTML pages written by plugin authors. Rendering them needs a web engine; this is the only allowed place for one, and it must live in a separate, optional helper process (system webview), never inside the egui app. Until that helper exists, plugin settings are edited through Deckly's own UI where possible.
- A misbehaving plugin must never crash, hang, or block Deckly. Treat every message from a plugin as untrusted input.

## 3. Workspace map

Crates form layers. A crate may depend only on crates in lower layers.

| Layer | Crate | Purpose |
| --- | --- | --- |
| L0 | `deckly-core` | Device model table, profile/page/key/action data model, serialization, validation. No I/O except profile load/save. |
| L1 | `deckly-device` | Stream Deck HID protocol: report encoding/decoding (pure, unit-tested) and the transport that talks to real devices. |
| L1 | `deckly-plugin-protocol` | Deckly plugin protocol: message types, framing, settings form descriptions and limits shared by the host and the SDK. Pure, no I/O. Depends only on `serde` and `serde_json`. Own version (`0.0.x`), not the workspace's. |
| L2 | `deckly-plugin-host` | Runs plugins: manifests, plugin installs, plugin processes and their restarts; Stream Deck SDK plugins over the local WebSocket server, Deckly plugins over pipes. May use `deckly-plugin-protocol`, and re-exports it (`proto`) for the app's settings forms. |
| L2 | `deckly-sdk` | What a Deckly plugin author uses: actions, settings, settings forms, global settings, keys, faces, the run loop, a test harness. Depends only on `deckly-plugin-protocol`, `serde` and `serde_json`: never on `deckly-core`, `deckly-device`, the host or the app. Own version (`0.0.x`). |
| L3 | `deckly` | The desktop app: egui UI, key image rendering, built-in action execution, runtime wiring. |
| helper | `deckly-web` | Separate process with the system web view for HTML plugins and Property Inspectors. Never linked into the app. |
| tool | `deckly-cli` | The plugin tool for plugin authors: `create` (a new project from templates), `build`, `link`/`unlink`/`list`, `dev`, `validate`, `pack`, `log`. Also installed as `cargo-deckly`. Depends only on `deckly-plugin-protocol` (L1), never on the app, the host or the SDK; runs `cargo` and `git` as commands. Own version (`0.0.x`). |
| example | `examples/hello-deckly`, `examples/counter-plugin`, `examples/transport-bench` | The plugin from the SDK book's Getting started page, a fuller example Deckly plugin with its tests, and the transport measurements. Not shipped in releases. |

Planned, add when the work starts (not before): headless control and inspection of Deckly, as more subcommands of `deckly-cli`.

The plugin SDK docs for plugin authors are the mdBook in `docs/sdk`, published on GitHub Pages; `cargo dev book` builds them. A change to the SDK's API updates those pages in the same branch. `docs/plugin-sdk.md` is the design record for Deckly contributors, not user docs.

## 4. Golden rules

### Never crash

- Non-test code must not panic. No `unwrap`, `expect`, unchecked indexing, or `panic!` on data that comes from a device, a file, a plugin, or the user. Return errors and show them.
- Treat device reports, profile files, plugin manifests, plugin messages, and images as hostile: bound lengths, sizes, counts, and allocations.
- A device disconnect, a corrupt profile, or a dead plugin is a normal event, not a crash. Recover and keep the rest of the app working.
- No `unsafe` outside small, documented platform modules. Each `unsafe` block states its safety requirements.
- Every crash fix comes with a regression test.

### Everything is an action

- User-visible key behavior is an action with a stable UUID (`com.deckly.*` for built-ins, the plugin's UUID for plugin actions), a name, a category, settings stored as JSON, and an execution path. The UI, the device runtime, and future CLI/plugin-host code all go through the same action model.
- Profile data stays serializable. The UI is a view over that data; do not hide state in widgets.

### Keep it fast

- Low startup time, idle CPU, memory, and key-press latency on all platforms. No busy polling; block on device reads or use events.
- Re-render and re-send a key image only when it changed.
- Measure before optimizing and do not claim numbers you did not measure.

## 5. Clean-room implementation

Deckly is a clean-room reimplementation: study features and behavior, document findings in our own words, implement from scratch in Rust.

- Allowed sources: public documentation (the Stream Deck SDK docs, USB HID specs, OS API docs), observation of the official app's UI and behavior, USB captures of the owner's own devices, and analysis of artifacts the owner is authorized to inspect.
- When investigating a shipped application or binary, follow the [reverse-engineer-anything (REA) skill](https://github.com/morluto/rea/blob/main/.agents/skills/reverse-engineer-anything/SKILL.md) and its target guides. Decompiled output and captures are evidence about behavior, never code to reuse.
- Do not copy proprietary code, firmware, UI assets, icons, artwork, sounds, profile templates, screenshots, or substantial documentation text into the repository. Do not commit third-party plugins or their assets; test against plugins installed on the developer's machine.
- Do not read or copy source code of other Stream Deck reimplementations or driver libraries into Deckly. Implement from our own notes and hardware observations.
- Do not extract credentials, signing keys, marketplace tokens, or license material, and do not bypass access controls.
- A familiar layout and visually similar interface are explicit goals; draw everything in Deckly's own code.
- Keep the behavior and evidence log in `docs/clean-room/`. For each entry record the source or method, date, device model and firmware where known, what was observed, and the test that checks it. Keep **observed**, **inferred**, and **unknown** apart.
- A clean-room process lowers copying risk; it is not legal clearance. Never describe it as such.

## 6. Implementation workflow

### Branches and human approval

- `main` is the only long-lived branch. There is no `dev` branch; do not create one.
- Start each new task on a separate branch based on the latest `origin/main` after fetching. Continue related work on that task branch; keep unrelated work in separate branches.
- Name a branch `<kind>/<short-task-name>` after the work it holds: `feature/` for new behavior, `fix/` for a bug fix, `docs/` for documentation, `refactor/` for code that keeps its behavior, `chore/` for everything else (dependencies, CI, tooling). Never put the name of an agent, tool, or model in a branch name.
- Create the branch with `git switch --no-track -c <kind>/<short-task-name> origin/main`, so that it does not track `origin/main` and a plain push cannot reach `main`.
- Within the authorized task, agents may edit, test, commit, and push the task branch without asking for confirmation at each step. Push explicitly to the task branch; never include `main` in a blanket push.
- Once the task is complete, report the changes and validation results, including what the owner should look at themselves (a UI change, behavior on a device or system the agent could not run). Then ask whether to merge the task branch into `main`. Green checks are not approval: the owner decides after trying the result.
- Merge into `main` only after the owner's explicit approval for that branch: update the local `main` from `origin/main`, merge the task branch with `git merge --no-ff`, run the checks again if `main` moved since the task branch was cut, and push `main`. Then delete the task branch locally and on `origin`.
- Without explicit human approval for the specific operation, agents must not change local or remote `main`: no direct commits, merges, cherry-picks, rebases, resets, pushes, force-pushes, deletions, or other branch updates. Fetching and reading `main`, creating a task branch from `origin/main`, and incorporating `origin/main` into a task branch are allowed because they do not update `main`.
- Approval applies only to the stated operation, target, and agreed changes; it is not blanket approval for later tasks or releases. An explicit approval already given in the conversation remains valid within that scope; do not ask for it again.
- Do not open pull requests for task branches, and do not enable auto-merge or otherwise bypass these approval requirements.
- Each task that changes what users get adds a line to the `Unreleased` section of `CHANGELOG.md` in plain words, in the same branch. A release is a tag on `main`, made only when the owner asks (see `docs/releasing.md`); its notes come from that section.

### Pushing with a token for this repository (optional)

A convenience, not a requirement. If your own `git` and `gh` login can push to `deckly-app/Deckly`, skip this. Otherwise, to let an agent push and open pull requests without your whole GitHub login, the token can live in a `.env` file in the clone:

1. Create a fine-grained token: resource owner `deckly-app`; the `Deckly` repository, or all of the organization's repositories so plugin repositories work too; Contents, Pull requests and Workflows read and write (Workflows is needed to push changes under `.github/workflows`). The organization may have to approve it.
2. Put it in `.env` at the repository root as one line: `GH_TOKEN=github_pat_...`. `.env` must stay listed in `.gitignore`; check `git check-ignore .env` before you create it. Another repository of the organization gets its own `.env` (and its own `.gitignore` entry) the same way; one token for all of `deckly-app` can go into each.
3. Once per clone, make `git` read it for GitHub. This applies to this clone and its worktrees only; other repositories keep your usual login:

   ```sh
   git config --local credential.https://github.com.helper ''
   git config --local --add credential.https://github.com.helper '!f() { test "$1" = get || exit 0; t=$(sed -n "s/^GH_TOKEN=//p" "$(git rev-parse --path-format=absolute --git-common-dir)/../.env" 2>/dev/null); test -n "$t" && printf "username=x-access-token\npassword=%s\n" "$t"; }; f'
   ```

4. Agents run `gh` with the same token: `GH_TOKEN=$(sed -n 's/^GH_TOKEN=//p' .env) gh pr create ...`.

Never commit `.env`, and never print the token in chat, logs, commit messages, or pull requests. To stop using it, delete `.env` and run `git config --local --unset-all credential.https://github.com.helper`.

### Versions and publishing

- Everything Deckly ships stays below 0.1.0 until the owner declares it
  finished: the app (`0.0.x`), and the plugin crates (`deckly-sdk`,
  `deckly-plugin-protocol`, the planned `deckly-cli`) each from `0.0.1` on.
  A release raises the patch number (`0.0.1`, `0.0.2`, ...). Only the owner
  decides when something becomes `0.1.0`; never propose or set it on your own.
- Publishing to crates.io cannot be undone (a version can only be yanked).
  Publish only after the work is complete and checked locally, and only with
  the owner's explicit approval for that version. Run
  `cargo publish --dry-run` first.

### Implementation and validation

- Read the relevant code and docs before changing files. Keep changes small; no speculative architecture.
- Every change includes tests where the logic is testable without hardware (protocol encoding, model validation, serialization round-trips, rendering sizes).
- Visual UI changes are checked by running the app and looking at it.
- Before finishing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- For platform-dependent changes, compile-check Windows, Linux, and macOS targets when the environment allows. Report compile checks separately from runtime and hardware testing; never imply a platform or device was tested when it was not.
- Do not claim support for a device model or OS until it was tested and recorded in `docs/devices.md`.
