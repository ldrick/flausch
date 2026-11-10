<img src="ui/icons/logo.svg" width="72" alt="">

# nøste

An Android app for crocheting, written in Rust. *Nøste* is Norwegian for winding yarn into a ball.

The UI uses [Slint](https://slint.dev) (`ui/*.slint`), which runs on Android via its `android-activity` backend and
on the desktop via winit – so most development happens with a plain `cargo run`. APKs are built with
[cargo-apk](https://github.com/rust-mobile/cargo-apk). `minSdk` 29 (Android 10).

## Features (MVP)

Projects made of parts (e.g. head, body, legs), each with its own row counter and optional row goal; project
list with pattern covers, total progress, search and a continue button; PDF patterns (imported and rendered on the
device) with a row counter for any part that can be enlarged and a movable reading line (color selectable); backup and restore of all data as a ZIP file; keep screen on, Android back key, German/English,
light/dark mode.

## Layout

- `src/lib.rs` – controller wiring the UI to the model, `android_main` entry point (built as `cdylib`)
- `src/model.rs`, `src/storage.rs` – data model and JSON persistence (no Slint)
- `src/pdf.rs`, `src/platform/` – PDF render worker, Android (JNI) and desktop platform code
- `src/main.rs` – desktop entry point
- `ui/` – Slint UI (`app.slint` root, `store.slint` Rust↔UI contract, `theme.slint`, `pages/`)
- `lang/` – translations (gettext `.po`, bundled at build time)
- `android/java/` – small Java helper (file picker and save dialog, PdfRenderer), compiled by `build.rs`
- `android/res/` – Android resources (launcher icon)
- `tools/logo.py` – generates the logo (`ui/icons/logo.svg`) and the launcher icon

See [`AGENTS.md`](AGENTS.md) for conventions and development notes.

## Setup

- Rust via [rustup](https://rustup.rs); `rust-toolchain.toml` pulls in stable with clippy, rustfmt and the
  Android targets
- `cargo install cargo-apk`
- Android SDK in `~/Android/Sdk` with `platforms;android-36`, `build-tools;36.0.0`, `ndk;<version>`
  (and `emulator` plus a system image for an emulator)
- A JDK (`javac`) for Slint's Android Java helper, by default in `~/.local/opt/jdk-21`

The `Makefile` defaults to these paths; override them via `ANDROID_HOME`, `ANDROID_NDK_ROOT`, `JAVA_HOME`.

## Build

```sh
make run                 # desktop window
make test                # unit tests
make check               # rustfmt + clippy
make ci                  # everything CI checks: also Android clippy, cargo-deny (cargo install cargo-deny)
make apk                 # target/debug/apk/noste.apk (arm64)
make install             # build, install and start on a connected device
make emulator            # start the AVD "noste"; then: make install TARGET=x86_64-linux-android
```

On Linux the desktop build loads fontconfig at runtime (`fontique/fontconfig-dlopen`), so no `-dev` packages are
needed.

## License

Slint is used under its Royalty-free License 2.0 (mobile apps allowed; requires an attribution, e.g. the
"Made with Slint" badge in the app or on its web page) – alternatively GPLv3.
