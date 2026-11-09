# AGENTS.md – nøste

nøste is an Android crochet app (projects, row counter, PDF patterns) written in Rust (edition 2024) with a
[Slint](https://slint.dev) 1.18 UI. The same code runs on the desktop via winit, so most development and checks
happen with `cargo run`. APKs are built with `cargo-apk`. Only Android is a release target; there is no iOS.

This file is the single source of truth for coding agents. `CLAUDE.md` and `.github/copilot-instructions.md`
point here.

## Commands

The `Makefile` wraps cargo and exports `ANDROID_HOME`, `ANDROID_NDK_ROOT` (the newest NDK under
`$ANDROID_HOME/ndk`) and `JAVA_HOME` (default `~/.local/opt/jdk-21`). You can override any of them on the command
line. If `cargo` is not on `PATH`, prefix commands with `export PATH=$HOME/.cargo/bin:$PATH`.

```sh
make run                 # desktop window (cargo run)
make test                # cargo test
cargo test <name>        # run a single test (substring match)
make check               # cargo fmt --check && cargo clippy --all-targets -- -D warnings
make apk                 # debug APK -> target/debug/apk/noste.apk (TARGET defaults to aarch64-linux-android)
make apk-release         # release APK (lto + strip)
make install             # cargo apk run: build, install, launch (then streams logcat until stopped)
make emulator            # start the AVD "noste"; then build with TARGET=x86_64-linux-android
```

Before you finish a change, run `make check` (clippy uses `-D warnings`) and `make test`. If you touched
anything Android-specific (`src/platform/android.rs`, the Java helper, `build.rs`, Android metadata), also run
`make apk TARGET=x86_64-linux-android`.

## Architecture

```
src/
  lib.rs              Controller: wires Store callbacks, navigation, debounced saving, PDF sessions, android_main
  main.rs             thin desktop binary -> noste::run(); no logic here
  model.rs            UI-independent data model (Library, Project, Pdf, Settings) – no slint
  storage.rs          JSON persistence (projects.json, settings.json, pdfs/) with atomic writes – no slint
  pdf.rs              background render worker; only the newest pending request is rendered
  platform/mod.rs     picks android.rs or desktop.rs via cfg
  platform/android.rs JNI: loads the dex built from android/java, picker, PdfRenderer, keep-screen-on, bar colors
  platform/desktop.rs dirs + rfd file dialog + hayro rendering; most other functions are no-ops
android/java/.../NosteHelper.java   Java helper (compiled by build.rs with javac + d8, embedded as classes.dex)
android/res/          launcher icon
ui/
  app.slint           root window App: page switch, dialogs, notice, Back key, safe-area insets
  store.slint         Store global = the Rust↔UI contract (state properties, enums, structs, callbacks)
  theme.slint         Theme global: colors (light/dark), sizes, radii, fonts
  components.slint    shared widgets (Card, PillButton, IconButton, TopBar, Dialog, Field, Segmented, Switch…)
  pages/*.slint       list, project, pdf, settings
  icons/              SVG icons
lang/de/LC_MESSAGES/noste.po   German translations (bundled at build time)
```

- Data flow: the UI only reads `Store` properties and calls `Store` callbacks. `src/lib.rs` handles the
  callbacks, changes the `model`, saves through `storage`, and pushes new state back into `Store`. Keep UI logic
  in Slint minimal.
- `model.rs` and `storage.rs` must not depend on `slint`, so they stay unit-testable (`tempfile` in tests).
- On-disk files are versioned (`LIBRARY_VERSION`, `SETTINGS_VERSION`). Imported PDFs are copied to `pdfs/`.
- `NOSTE_DATA_DIR=/tmp/x cargo run` uses a scratch data directory on the desktop.
- PDF rendering: on Android, `android.graphics.pdf.PdfRenderer` through the Java helper; on the desktop, `hayro`
  (pure Rust). The worker renders at the display width and the viewer scales it.
- Keep platform-specific code behind `cfg(target_os = ...)` so `cargo run` and `cargo test` keep working on the
  desktop. Android builds always use `--lib` (cargo-apk packages the `cdylib`).
- Android config is in `Cargo.toml` under `[package.metadata.android]`: package `io.github.ldrick.noste`,
  `minSdk` 29 and `targetSdk` 36.

## UI conventions

- Design: soft, light and minimalist (cool off-white background, white cards, mint accent, pill buttons). Use
  colors and sizes from `Theme` only, never hard-coded colors and not `Palette`. `Theme.dark` follows the
  appearance setting (system, light, dark). The color scheme isn't final, so keep it centralized in
  `theme.slint`.
- Design window: phone-like, 390×800.
- `Rectangle` already has a (deprecated) `color` property, so name custom color properties differently
  (for example `tint`).
- Layouts ignore `x`/`y`. To center a child vertically in a `HorizontalLayout`, wrap it in
  `VerticalLayout { alignment: center; … }`.
- Use `content-*` instead of the deprecated `Flickable.viewport-*`.
- Callbacks set in Slint `init` fire during `App::new()`, before the Rust handlers are connected.
- `AboutSlint` follows `Palette`, not `Theme`. It sits on a `Palette.background` backdrop in Settings. The
  Royalty-free license requires this attribution; don't remove it.

## Translations

- Write UI strings in English with `@tr("…")`. Bundled languages are English (source) and German
  (`lang/de/LC_MESSAGES/noste.po`). The language setting calls `slint::select_bundled_translation`.
- `@tr` without an explicit context uses the **component name as msgctxt**. Update the `.po` file with
  `slint-tr-extractor` (`cargo install slint-tr-extractor`):

  ```sh
  find ui -name '*.slint' | xargs slint-tr-extractor -o /tmp/noste.pot
  msgmerge -U lang/de/LC_MESSAGES/noste.po /tmp/noste.pot   # then translate the new or fuzzy entries
  ```

- Every new string needs a German translation.

## Android gotchas

- **Don't call `AndroidApp::set_window_flags`** from UI callbacks. It deadlocks (ANR) because Slint holds the
  read lock during input dispatch. Keep-screen-on goes through `NosteHelper.setKeepScreenOn` (`runOnUiThread`).
- **Back is delivered twice** with targetSdk 36 (OnBackInvokedCallback and the native KeyEvent). The
  `Controller::back()` method drops a second Back within 300 ms. Dialogs are closed in Rust. An unhandled Back on
  the project list finishes the activity.
- The status bar icon color follows `Theme.dark` (`Store.dark-changed` → `platform::set_dark_theme` →
  `NosteHelper.setLightSystemBars`).
- Pages are inset by `safe-area-insets`. Dialogs and the scrim cover the whole window.
- Rust `stderr` shows up in logcat under the tag `RustStdoutStderr`. ANR traces:
  `adb shell dumpsys dropbox --print data_app_anr`.
- After changing the Java helper, check the call signatures in `platform/android.rs` (JNI strings such as
  `"(Z)V"`).

## Testing on the emulator

```sh
make emulator                                   # separate terminal; AVD "noste"
make apk TARGET=x86_64-linux-android
adb install -r target/debug/apk/noste.apk
adb shell am start -n io.github.ldrick.noste/android.app.NativeActivity
adb exec-out screencap -p > /tmp/screen.png     # 1080×2400 physical pixels
adb shell input tap X Y                         # physical pixels (logical px × 2.625 at density 420)
```

On the desktop under Wayland, `env -u WAYLAND_DISPLAY` forces X11 so you can take screenshots with
`import -window`. Synthetic clicks don't work there, so test interactions on the emulator.

## Style

- Formatting follows `.editorconfig`: UTF-8, LF, a final newline, 4 spaces (2 for TOML and YAML). Rust uses
  `rustfmt`.
- Only add comments where something needs explaining.
- On Linux, `fontique` loads fontconfig at runtime (`fontconfig-dlopen`). Don't add dependencies that need
  system `-dev` packages or pkg-config.
- Don't commit unless asked. Local plans and notes go in `.scratch/` (gitignored).
