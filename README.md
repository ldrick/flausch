<img src="ui/icons/logo.svg" width="72" alt="">

# Flausch

An Android app for crocheting, written in Rust. *Flausch* is German for fluff (from *flauschig*, fluffy).

The UI uses [Slint](https://slint.dev) (`ui/*.slint`), which runs on Android via its `android-activity` backend and
on the desktop via winit – so most development happens with a plain `cargo run`. APKs are built with
[cargo-apk](https://github.com/rust-mobile/cargo-apk). `minSdk` 29 (Android 10).

## Features (MVP)

Projects made of parts (e.g. head, body, legs), each with its own row counter and optional row goal; project
list with pattern covers, total progress, search and a continue button; PDF patterns (imported and rendered on the
device) with a row counter for any part that can be enlarged and a movable reading line (color selectable); backup and restore of all data as a ZIP file; keep screen on, Android back key, German/English,
light/dark mode, TalkBack support.

## Layout

- `src/lib.rs` – controller wiring the UI to the model, `android_main` entry point (built as `cdylib`)
- `src/model.rs`, `src/storage.rs` – data model and JSON persistence (no Slint)
- `src/pdf.rs`, `src/platform/` – PDF render worker, Android (JNI) and desktop platform code
- `src/main.rs` – desktop entry point
- `ui/` – Slint UI (`app.slint` root, `store.slint` Rust↔UI contract, `theme.slint`, `pages/`)
- `lang/` – translations (gettext `.po`, bundled at build time)
- `android/java/` – small Java helper (file picker and save dialog, PdfRenderer, TalkBack bridge), compiled by
  `build.rs`
- `android/res/` – Android resources (launcher icon)
- `tools/logo.py` – generates the logo (`ui/icons/logo.svg`), the launcher icon and the store icon
- `fastlane/metadata/android/` – store texts, icon and screenshots (en-US, de-DE)
- `fdroid/` – F-Droid build recipe and release steps

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
make test                # unit, controller and UI tests (headless)
make check               # rustfmt + clippy
make ci                  # everything CI checks: also Android clippy, cargo-deny (cargo install cargo-deny)
make apk                 # target/debug/apk/flausch.apk (arm64)
make apk-release         # release APK (needs a keystore, see below)
make install             # build, install and start on a connected device
make emulator            # start the AVD "flausch"; then: make install TARGET=x86_64-linux-android
```

On Linux the desktop build loads fontconfig at runtime (`fontique/fontconfig-dlopen`), so no `-dev` packages are
needed.

## Release

`make apk-release` signs with the keystore from `CARGO_APK_RELEASE_KEYSTORE` and
`CARGO_APK_RELEASE_KEYSTORE_PASSWORD`. Releases go to F-Droid, which builds from source; see
[`fdroid/README.md`](fdroid/README.md).

## Privacy

Flausch requests no permissions, has no internet access and collects no data. Projects and PDFs stay on the device.

## License

Flausch is free software under the GNU General Public License v3.0 or later, see [LICENSE](LICENSE).

Slint is used under its Royalty-free License 2.0 (mobile apps allowed; requires an attribution, e.g. the
"Made with Slint" badge in the app or on its web page) – alternatively GPLv3.
