# Copilot instructions for nøste

Read [`AGENTS.md`](../AGENTS.md) in the repository root. It is the single source of truth for commands,
architecture, UI and translation conventions, and Android gotchas.

Key points:

- Rust (edition 2024) with a Slint 1.18 UI. It runs on the desktop (`make run`) and on Android (`make apk`,
  always `--lib`).
- Run `make check` (fmt + clippy `-D warnings`) and `make test` before you finish.
- `model.rs` and `storage.rs` stay free of `slint`. The UI talks to Rust only through the `Store` global in
  `ui/store.slint`.
- Use `Theme` colors, not `Palette`. Every `@tr` string needs a German translation in
  `lang/de/LC_MESSAGES/noste.po`.
- Keep platform-specific code behind `cfg(target_os = ...)`. Never call `AndroidApp::set_window_flags`
  from UI callbacks.
