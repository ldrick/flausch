# AGENTS.md – nøste

nøste is an Android crochet app (projects made of parts with their own row counters, PDF patterns) written in Rust
(edition 2024) with a [Slint](https://slint.dev) 1.18 UI. The same code runs on the desktop via winit, so most
development and checks happen with `cargo run`. APKs are built with `cargo-apk`. Only Android is a release target;
there is no iOS.

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
make check-android       # clippy for the Android code (cargo clippy --lib --target $TARGET)
make deny                # cargo deny check: advisories, licenses, sources (deny.toml; needs cargo-deny)
make ci                  # check + check-android + test + deny
make apk                 # debug APK -> target/debug/apk/noste.apk (TARGET defaults to aarch64-linux-android)
make apk-release         # release APK (lto + strip)
make install             # cargo apk run: build, install, launch (then streams logcat until stopped)
make emulator            # start the AVD "noste"; then build with TARGET=x86_64-linux-android
```

Before you finish a change, run `make check` (clippy uses `-D warnings`) and `make test`. If you touched
anything Android-specific (`src/platform/android.rs`, the Java helper, `build.rs`, Android metadata), also run
`make check-android` and `make apk TARGET=x86_64-linux-android`. After changing dependencies, run `make deny`.

Lints are configured in `Cargo.toml` (`[lints]`): `clippy::pedantic` is on, numeric cast lints are off (Slint uses
`i32`/`f32`). `unsafe` is denied except in `platform/android.rs` (JNI), where every block needs a `// SAFETY:`
comment. Prefer fixing a lint over `#[allow]`; if you must allow one, keep it local and say why.
CI (`.github/workflows/ci.yml`) runs fmt, clippy, tests, cargo-deny, Android clippy and a debug APK build.
The toolchain (stable, rustfmt, clippy, Android targets) is set in `rust-toolchain.toml`.

## Architecture

```
src/
  lib.rs              Controller: wires Store callbacks, navigation, debounced saving, PDF sessions, android_main
  main.rs             thin desktop binary -> noste::run(); no logic here
  model.rs            UI-independent data model (Library, Project, Pdf, Settings) – no slint
  storage.rs          JSON persistence (projects.json, settings.json, pdfs/, covers/) with atomic writes, ZIP backups – no slint
  pdf.rs              render workers: viewer pages (newest request only) and PNG covers of page 1
  platform/mod.rs     picks android.rs or desktop.rs via cfg
  platform/android.rs JNI: loads the dex built from android/java, picker, PdfRenderer, keep-screen-on, bar colors
  platform/desktop.rs dirs + rfd file dialog + hayro rendering; most other functions are no-ops
android/java/.../NosteHelper.java   Java helper (compiled by build.rs with javac + d8, embedded as classes.dex)
android/res/          launcher icon (mint logo on light mint, also the monochrome themed icon)
ui/
  app.slint           root window App: page switch, dialogs, notice, Back key, safe-area insets
  store.slint         Store global = the Rust↔UI contract (state properties, enums, structs, callbacks)
  theme.slint         Theme global: colors (light/dark), sizes, radii, fonts
  components.slint    shared widgets (Card, PillButton, IconButton, TopBar, Dialog, Field, Segmented, Switch…)
  counter.slint       row counter blocks (RowCount, CounterButtons) for the part screen and the PDF view
  pages/*.slint       list, project (parts + pattern), part (row counter), pdf, settings
  icons/              SVG icons; logo.svg is the app logo
tools/logo.py         generates ui/icons/logo.svg and the launcher icon from one geometry – never edit those by hand
lang/de/LC_MESSAGES/noste.po   German translations (bundled at build time)
```

- Data flow: the UI only reads `Store` properties and calls `Store` callbacks. `src/lib.rs` handles the
  callbacks, changes the `model`, saves through `storage`, and pushes new state back into `Store`. Keep UI logic
  in Slint minimal.
- Domain: a `Project` has a name, any number of `Part`s (e.g. head, legs) and an optional PDF. Each part has its
  own row counter and an optional row goal. `active_part` is the part opened last; the counter in the PDF view
  counts it (switchable there, and expandable to a large counter: `Store.counter-expanded`, Back collapses it),
  and the project list shows it. Adding a part makes it active but stays on the project screen. The PDF view has a
  movable reading line (`Pdf::lines`: fraction of the page height per page, `show_line` per PDF; its color is
  `Settings::line_color` → `Theme.line-color()`). List progress sums all parts with a goal
  (`Project::progress`); search matches project and part names (`Project::matches`). Navigation:
  list → project → part → pdf (pdf also from project, and from the list's continue button, where Back returns to
  the list).
- `model.rs` and `storage.rs` must not depend on `slint`, so they stay unit-testable (`tempfile` in tests).
- On-disk files are versioned (`LIBRARY_VERSION`, `SETTINGS_VERSION`). `Library::migrate` (called on load)
  upgrades old data, e.g. version 1 rows per project become one part; add a test for every migration.
  Imported PDFs are copied to `pdfs/`; `covers/<pdf>.png` holds a thumbnail of page 1 for the project list,
  created after import and on start if missing. `Storage::remove_pdf` removes both.
- Backups (Settings → Data) are ZIP files with `projects.json`, `settings.json` and `pdfs/`, but no covers.
  `Storage::read_backup` refuses newer versions and runs `migrate`; `Storage::restore` replaces everything. The
  Android side goes through the Storage Access Framework (`platform::pick_document` / `save_document`, no storage
  permission), via temporary `export.zip` / `import.zip` files in the data directory.
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
- Shadows are hard drop shadows: no `drop-shadow-blur`, only `drop-shadow-offset-y: Theme.shadow-offset` (or
  `Theme.shadow-offset-small` for small elements) with `drop-shadow-color: Theme.shadow`.
- Logo: soft mint line art of a ball of yarn with cat ears, a crochet hook behind it and the yarn end curled up
  as the tail. In Slint use `Icon { source: logo.svg; color: Theme.accent; }` (only lines, so it fits any
  background). If the mint changes, update `MINT` in `tools/logo.py`, run
  it, and update `ic_launcher_background` (light mint, `Theme.accent-soft`) in `android/res/values/colors.xml`.
- Design window: Pixel 9a size, 412×924 logical px (1080×2424 at density 2.625). The AVD "noste" uses the
  `pixel_9a` profile.
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

  Without gettext installed, merge with Python's `polib` in a venv (`po.merge(pot)`, fill the empty or fuzzy
  entries, drop `po.obsolete_entries()`, save). Plural strings use `@tr("{n} part" | "{n} parts" % count)`.

- Every new string needs a German translation.

## Android gotchas

- **Don't call `AndroidApp::set_window_flags`** from UI callbacks. It deadlocks (ANR) because Slint holds the
  read lock during input dispatch. Keep-screen-on goes through `NosteHelper.setKeepScreenOn` (`runOnUiThread`).
- **Back is delivered twice** with targetSdk 36 (OnBackInvokedCallback and the native KeyEvent). The
  `Controller::back()` method drops a second Back within 300 ms. Dialogs are closed in Rust. An unhandled Back on
  the project list finishes the activity.
- The status bar icon color follows `Theme.dark` (`Store.dark-changed` → `platform::set_dark_theme` →
  `NosteHelper.setLightSystemBars`).
- Pages are inset by `safe-area-insets`. Dialogs and the scrim cover the whole window. `App` copies the
  on-screen keyboard height into the `Keyboard` global, and `Dialog` moves up so the keyboard never covers it.
- Slint shows Android's cursor handles whenever the last touch was a tap, also when a dialog focuses a field from
  code. `Field.focus-input()` therefore calls `Store.hide-text-handles()` → `NosteHelper.hideTextHandles`, which
  sends the unchanged text through Slint's `SlintAndroidJavaHelper.updateText` (reflection). Tapping or selecting
  text still shows the handles.
- Rust `stderr` shows up in logcat under the tag `RustStdoutStderr`. ANR traces:
  `adb shell dumpsys dropbox --print data_app_anr`.
- After changing the Java helper, check the call signatures in `platform/android.rs` (jni 0.22: names and
  signatures go through `jni_str!` / `jni_sig!`, e.g. `jni_sig!("(Z)V")`; `onDocumentDone` is registered by hand).

## Testing on the emulator

```sh
make emulator                                   # separate terminal; AVD "noste"
make apk TARGET=x86_64-linux-android
adb install -r target/debug/apk/noste.apk
adb shell am start -n io.github.ldrick.noste/android.app.NativeActivity
adb exec-out screencap -p > /tmp/screen.png     # 1080×2424 physical pixels
adb shell input tap X Y                         # physical pixels (logical px × 2.625 at density 420)
```

The AVD uses Quick Boot (start in about 4 s instead of a cold boot) and the host GPU (`hw.gpu.mode=host`). Don't
start it with `-no-snapshot-save`/`-no-snapshot`. Shut it down with SIGTERM/SIGINT (stopping the process, Ctrl+C) so
it saves the snapshot; a SIGKILL leaves the snapshot empty and the next start is a cold boot.

On the desktop under Wayland, `env -u WAYLAND_DISPLAY` forces X11 so you can take screenshots with
`import -window`. Synthetic clicks don't work there, so test interactions on the emulator.

## Style

- Formatting follows `.editorconfig`: UTF-8, LF, a final newline, 4 spaces (2 for TOML and YAML). Rust uses
  `rustfmt`.
- Only add comments where something needs explaining.
- On Linux, `fontique` loads fontconfig at runtime (`fontconfig-dlopen`). Don't add dependencies that need
  system `-dev` packages or pkg-config. Our `fontique` must stay on the version Slint uses (Dependabot ignores it);
  bump it together with Slint.
- Don't commit unless asked. Local plans and notes go in `.scratch/` (gitignored).
