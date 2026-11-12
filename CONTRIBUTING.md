# Contributing to Flausch

Flausch is written in Rust with a [Slint](https://slint.dev) UI (`ui/*.slint`). Slint runs on Android via its
`android-activity` backend and on the desktop via winit, so most development happens with a plain `cargo run`.
APKs are built with [cargo-apk](https://github.com/rust-mobile/cargo-apk).

The architecture, UI and translation conventions and Android notes are in [`AGENTS.md`](AGENTS.md).

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

## Pull requests

One change per branch and pull request. Fill in the template, run `make check` and `make test`, and give every new
UI string a German translation in `lang/de/LC_MESSAGES/flausch.po` (see `AGENTS.md`).
