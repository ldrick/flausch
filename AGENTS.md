# AGENTS.md – Flausch

Flausch is an Android crochet app (projects made of parts with their own row counters, PDF patterns) written in Rust
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
make test                # cargo test: unit, controller (src/tests.rs) and UI tests (tests/)
cargo test <name>        # run a single test (substring match)
make check               # cargo fmt --check && cargo clippy --all-targets -- -D warnings
make check-android       # clippy for the Android code (cargo clippy --lib --target $TARGET)
make deny                # cargo deny check: advisories, licenses, sources (deny.toml; needs cargo-deny)
make ci                  # check + check-android + test + deny
make apk                 # debug APK -> target/debug/apk/flausch.apk (TARGET defaults to aarch64-linux-android)
make apk-release         # release APK (lto + strip; needs CARGO_APK_RELEASE_KEYSTORE(_PASSWORD))
make install             # cargo apk run: build, install, launch (then streams logcat until stopped)
make emulator            # start the AVD "flausch"; then build with TARGET=x86_64-linux-android
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
  lib.rs              Controller: wires Store callbacks, navigation, debounced saving, PDF sessions, android_main;
                      pub start/stop connect an App to a data dir (used by run and the tests)
  tests.rs            controller tests on Slint's testing backend
  main.rs             thin desktop binary -> flausch::run(); no logic here
tests/ui.rs           UI tests: drive the app through accessibility labels; fixtures in tests/fixtures/
  model.rs            UI-independent data model (Library, Project, Pdf, Settings) – no slint
  storage.rs          JSON persistence (projects.json, settings.json, pdfs/, covers/) with atomic writes, ZIP backups – no slint
  changes.rs          changelog (Settings → About → Changelog): the F-Droid changelogs embedded by build.rs, version codes
  a11y.rs             Android only: TalkBack bridge (walks the Slint element tree, sends virtual views to Java)
  pdf.rs              render workers: viewer pages (newest request only) and PNG covers of page 1
  platform/mod.rs     picks android.rs or desktop.rs via cfg
  platform/android.rs JNI: loads the dex built from android/java, picker, PdfRenderer, keep-screen-on, bar colors
  platform/desktop.rs dirs + rfd file dialog + hayro rendering; most other functions are no-ops
android/java/.../FlauschHelper.java   Java helper (compiled by build.rs with javac + d8, embedded as classes.dex)
android/res/          launcher icon (pink logo on light pink, also the monochrome themed icon)
ui/
  app.slint           root window App: page switch, dialogs, notice, Back key, safe-area insets
  store.slint         Store global = the Rust↔UI contract (state properties, enums, structs, callbacks)
  theme.slint         Theme global: colors (light/dark), sizes, radii, fonts
  components.slint    shared widgets (Card, PillButton, IconButton, TopBar, Dialog, Field, Segmented, Switch…)
  counter.slint       row counter blocks (RowCount, CounterButtons) for the part screen and the PDF view
  pages/*.slint       list, project (parts + pattern), part (row counter), pdf, settings, about (About and Changelog sub-pages)
  icons/              SVG icons; logo.svg is the app logo
tools/release.py      prepares a release (version bump, CHANGELOG.md, F-Droid changelogs) and prints release notes
tools/logo.py         generates ui/icons/logo.svg, the launcher icon and the store icon (needs resvg) – never edit those by hand
fastlane/metadata/    F-Droid store texts, icon, screenshots (en-US, de-DE)
fdroid/               F-Droid recipe and release steps (fdroid/README.md)
lang/de/LC_MESSAGES/flausch.po   German translations (bundled at build time)
```

- Data flow: the UI only reads `Store` properties and calls `Store` callbacks. `src/lib.rs` handles the
  callbacks, changes the `model`, saves through `storage`, and pushes new state back into `Store`. Keep UI logic
  in Slint minimal.
- Domain: a `Project` has a name, any number of `Part`s (e.g. head, legs) and an optional PDF. Each part has its
  own row counter and an optional row goal. `active_part` is the part opened last; the counter in the PDF view
  counts it (switchable there, and expandable to a large counter: `Store.counter-expanded`, Back collapses it),
  and the project list shows it. Adding a part makes it active but stays on the project screen. The PDF view has a
  movable reading line (`Pdf::lines`: fraction of the page height per page, `show_line` per PDF; its color is
  `Settings::line_color` → `Theme.line-color()`). At zoom 1, swiping left or right turns the page
  (`SwipeGestureHandler` around the page's `Flickable`); zoomed in, a drag moves the view. List progress sums all
  parts with a goal
  (`Project::progress`); search matches project and part names and the localized status name
  (`Project::matches`, names from `Store.status-names`). Navigation:
  list → project → part → pdf (pdf also from project, and from the list's continue button, where Back returns to
  the list). Tapping a part makes it active and opens the PDF view if the project has a PDF, otherwise the part
  screen. The pencil on a part card opens the edit dialog (with delete) for that part via `Store.edited`, without
  changing the active part. Settings → About lists sub-pages (`NavRow` → `Store.open-page(Screen)`): About
  Flausch, Changelog and Donate (GitHub Sponsors and Liberapay, opened in the browser through
  `Store.open-url` → `platform::open_url`; the same links are in `.github/FUNDING.yml` and the F-Droid recipe).
  New info pages get a `Screen` value and a row there. A project also has a `Status` (planned, in progress, paused, finished; shown as a
  badge in the list) and free-text `notes`. The list has filter chips (`Settings::project_filter`) and is grouped by status
  (`Library::sorted`: in progress, then planned and paused, then finished; within a group by
  `Settings::project_sort`: last used, name, newest or progress, chosen in the sort dialog); with "All" and no search, finished
  projects sit in a collapsed section (`Store.finished`). Finished projects have no continue button. A part can be made several times (`Part::quantity`, e.g. 4 legs): `pieces_done` counts
  finished pieces, "Next piece" (`Library::next_piece`) starts the next one at row 0, Previous row at row 0 goes
  back to the last row of the piece before, Reset starts over, and progress counts `goal × quantity` rows; the UI
  shows `PartItem.title` ("Leg 2/4"). There is deliberately no second counter (removed as too complicated). With `Settings::volume_keys`, the volume keys count the active part, but
  only on the part and PDF screens (`update_volume_keys`).
- Opening or sharing a PDF into Flausch (VIEW/SEND intent filters in `Cargo.toml`): NativeActivity has no
  `onNewIntent`, so every intent starts a new activity. `android_main` first calls `platform::receive_document`;
  if the intent carries a PDF, Java copies it to `incoming.pdf` (title in `incoming.txt`), brings the running
  instance to the front (`showRunningApp`, never a second NativeActivity) and Rust returns, which finishes the
  trampoline. The main instance picks the file up in `check_incoming` (on start and on resume) and asks for a new
  project or an existing one (`Storage::adopt_incoming` / `discard_incoming`).
- `model.rs` and `storage.rs` must not depend on `slint`, so they stay unit-testable (`tempfile` in tests).
- On-disk files are versioned (`LIBRARY_VERSION`, `SETTINGS_VERSION`). `Library::migrate` (called on load)
  upgrades old data, e.g. version 1 rows per project become one part; add a test for every migration.
  Imported PDFs are copied to `pdfs/`; `covers/<pdf>.png` holds a thumbnail of page 1 for the project list,
  created after import and on start if missing. `Storage::remove_pdf` removes both.
- Backups (Settings → Data) are ZIP files with `projects.json`, `settings.json` and `pdfs/`, but no covers.
  `Storage::read_backup` refuses newer versions and runs `migrate`; `Storage::restore` replaces everything. The
  Android side goes through the Storage Access Framework (`platform::pick_document` / `save_document`, no storage
  permission), via temporary `export.zip` / `import.zip` files in the data directory.
- `FLAUSCH_DATA_DIR=/tmp/x cargo run` uses a scratch data directory on the desktop.
- PDF rendering: on Android, `android.graphics.pdf.PdfRenderer` through the Java helper; on the desktop, `hayro`
  (pure Rust). The worker renders at the display width and the viewer scales it.
- Keep platform-specific code behind `cfg(target_os = ...)` so `cargo run` and `cargo test` keep working on the
  desktop. Android builds always use `--lib` (cargo-apk packages the `cdylib`).
- Android config is in `Cargo.toml` under `[package.metadata.android]`: package `io.github.ldrick.flausch`,
  `minSdk` 29 and `targetSdk` 36. cargo-apk derives the versionCode from `version`
  (`1<<24 | major<<16 | minor<<8 | patch`); `[package.metadata.fdroid] version_code` mirrors it for F-Droid's
  update check, and a test keeps both in sync. A release needs a changelog per version code in
  `fastlane/metadata/android/*/changelogs/` (see `fdroid/README.md`).

## UI conventions

- Principles: offline and local (no account, no server, no internet access, no permissions; data stays in the app
  directory) and usable with one hand (large touch targets, little text, nothing resets by accident). Open tasks
  live in GitHub Issues, grouped by milestone.
- Design: soft, light and minimalist (warm off-white background, white cards, soft pink accent, pill buttons). Use
  colors and sizes from `Theme` only, never hard-coded colors and not `Palette`. `Theme.dark` follows the
  appearance setting (system, light, dark). The color scheme isn't final, so keep it centralized in
  `theme.slint`.
- Shadows are hard drop shadows: no `drop-shadow-blur`, only `drop-shadow-offset-y: Theme.shadow-offset` (or
  `Theme.shadow-offset-small` for small elements) with `drop-shadow-color: Theme.shadow`.
- Logo: soft pink line art of a ball of yarn with cat ears, a crochet hook behind it and the yarn end curled up
  as the tail. In Slint use `Icon { source: logo.svg; color: Theme.accent; }` (only lines, so it fits any
  background). If the pink changes, update `PINK` in `tools/logo.py`, run
  it, and update `ic_launcher_background` (light pink, `Theme.accent-soft`) in `android/res/values/colors.xml`.
- Design window: Pixel 9a size, 412×924 logical px (1080×2424 at density 2.625). The AVD "flausch" uses the
  `pixel_9a` profile.
- `Rectangle` already has a (deprecated) `color` property, so name custom color properties differently
  (for example `tint`).
- Layouts ignore `x`/`y`. To center a child vertically in a `HorizontalLayout`, wrap it in
  `VerticalLayout { alignment: center; … }`.
- Use `content-*` instead of the deprecated `Flickable.viewport-*`.
- Callbacks set in Slint `init` fire during `App::new()`, before the Rust handlers are connected.
- `AboutSlint` follows `Palette`, not `Theme`. It sits on a `Palette.background` backdrop on the About page. The
  Royalty-free license requires this attribution; don't remove it.

## Translations

- Write UI strings in English with `@tr("…")`. Bundled languages are English (source) and German
  (`lang/de/LC_MESSAGES/flausch.po`). The language setting calls `slint::select_bundled_translation`.
- `@tr` without an explicit context uses the **component name as msgctxt**. Update the `.po` file with
  `slint-tr-extractor` (`cargo install slint-tr-extractor`):

  ```sh
  find ui -name '*.slint' | xargs slint-tr-extractor -o /tmp/flausch.pot
  msgmerge -U lang/de/LC_MESSAGES/flausch.po /tmp/flausch.pot   # then translate the new or fuzzy entries
  ```

  Without gettext installed, merge with Python's `polib` in a venv (`po.merge(pot)`, fill the empty or fuzzy
  entries, drop `po.obsolete_entries()`, save). Plural strings use `@tr("{n} part" | "{n} parts" % count)`.

- Every new string needs a German translation.

## Android gotchas

- **Don't call `AndroidApp::set_window_flags`** from UI callbacks. It deadlocks (ANR) because Slint holds the
  read lock during input dispatch. Keep-screen-on goes through `FlauschHelper.setKeepScreenOn` (`runOnUiThread`).
- **Back is delivered twice** with targetSdk 36 (OnBackInvokedCallback and the native KeyEvent). The
  `Controller::back()` method drops a second Back within 300 ms. Dialogs are closed in Rust. An unhandled Back on
  the project list finishes the activity.
- Slint leaves volume keys unhandled. `FlauschHelper.setVolumeKeys` wraps the window's `Window.Callback` in a
  Proxy that consumes them while enabled and calls `onVolumeKey` on the first key down.
- The status bar icon color follows `Theme.dark` (`Store.dark-changed` → `platform::set_dark_theme` →
  `FlauschHelper.setLightSystemBars`).
- Pages are inset by `safe-area-insets`. Dialogs and the scrim cover the whole window. `App` copies the
  on-screen keyboard height into the `Keyboard` global, and `Dialog` moves up so the keyboard never covers it.
- Slint shows Android's cursor handles whenever the last touch was a tap, also when a dialog focuses a field from
  code. `Field.focus-input()` therefore calls `Store.hide-text-handles()` → `FlauschHelper.hideTextHandles`, which
  sends the unchanged text through Slint's `SlintAndroidJavaHelper.updateText` (reflection). Tapping or selecting
  text still shows the handles.
- Rust `stderr` shows up in logcat under the tag `RustStdoutStderr`. ANR traces:
  `adb shell dumpsys dropbox --print data_app_anr`.
- After changing the Java helper, check the call signatures in `platform/android.rs` (jni 0.22: names and
  signatures go through `jni_str!` / `jni_sig!`, e.g. `jni_sig!("(Z)V")`; `onDocumentDone` is registered by hand).

## Accessibility (TalkBack)

Slint's Android backend has no accessibility support, so Flausch brings its own bridge:

- `src/a11y.rs` polls the element tree every 300 ms while any accessibility service is on, using
  `i-slint-backend-testing`'s `ElementHandle` API. It is pinned to the exact Slint version (`=1.18.x`); bump both
  together. The walk needs Slint debug info, so `build.rs` sets `with_debug_info` for Android.
- `FlauschHelper` exposes the nodes as virtual views through an `AccessibilityNodeProvider` on the NativeActivity
  content view; hover events (touch exploration) map to nodes. Actions (click, scroll = increment/decrement,
  set text) go back to Rust through `onA11yAction`.
- Android can't draw the focus frame over the native surface, so Java reports focus (`onA11yFocus`) and
  `App` draws it from `Store.a11y-focus` in `Theme.a11y-focus`.
- What the UI needs: `accessible-role` and `accessible-label` on every control (Text and Image nodes are folded
  into the label of the smallest control that contains them). Dialog cards carry `accessible-id: "dialog"` so
  only the dialog is exposed while it is open. Notices use `accessible-live-region` and are announced.
- Testing: `adb shell input` bypasses touch exploration. On the emulator, real touches go through the console:
  `adb emu event mouse X Y 0 1` (down) / `… 0 0` (up); hold ~1 s to explore, two quick taps to double tap.
  `uiautomator dump` shows the nodes (it pauses TalkBack, which is why the bridge checks
  `AccessibilityManager.isEnabled()` rather than touch exploration).

## Tests

- Unit tests live next to the code (`model.rs`, `storage.rs`, `pdf.rs`).
- `src/tests.rs` tests the controller: the real `App` on `i_slint_backend_testing::init_no_event_loop()`
  (per test thread, no window) with a `tempfile` data dir. It calls `Controller` methods and feeds worker
  results (`pdf_picked`, `pdf_rendered`, `backup_picked`, …) directly, because `invoke_from_event_loop` does
  nothing without an event loop. `mock_elapsed_time` fires timers such as the debounced save.
- `tests/ui.rs` is black-box: `flausch::start`/`stop`, then `ElementHandle::find_by_accessible_label` and
  `invoke_accessible_default_action`/`set_accessible_value`, the same labels TalkBack uses. New UI features
  should get a flow there; they need an `accessible-label` anyway. Call `settle()` after changes so `changed`
  handlers run and `if` elements appear. Two Back presses within 300 ms count as one, so `go_back` waits.
- The element tree API needs Slint debug info, so `build.rs` always enables it.
- Never open a file dialog in tests (`pick_pdf`, `import_backup`, `export_backup`).

## Testing on the emulator

Develop and test on the emulator. The user's phone holds real data: use it only when the user asks (to install a
finished build or to reproduce a specific bug there). On the phone, don't change device settings, don't touch
Flausch's data, make sure Flausch is in the foreground before sending input, and ask the user to unlock it if
needed.

```sh
make emulator                                   # separate terminal; AVD "flausch"
make apk TARGET=x86_64-linux-android
adb install -r target/debug/apk/flausch.apk
adb shell am start -n io.github.ldrick.flausch/android.app.NativeActivity
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
- Local plans and notes go in `.scratch/` (gitignored).

## Workflow

`main` isn't protected yet (private repo), so these rules are a convention; follow them anyway.

- Don't commit unless asked. Never push to `main` directly: every change goes through a pull request.
- One branch per issue, named `<type>/<issue>-<short-name>` (e.g. `feat/16-project-status`). The PR body says
  `Closes #<issue>`.
- PR titles follow [Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`, `docs:`,
  `chore:`, `refactor:`, `test:`, `ci:`), written in English and user-facing for `feat`/`fix`, because they end up
  in the changelog. PRs are squash merged, so the PR title becomes the commit message.
- CI must pass before a merge. Merge only when the user says so.
- Versions are bumped only in a release PR, never in feature PRs. Run the **Release** workflow by hand (Actions →
  Release → Run workflow, or `gh workflow run release.yml [-f version=X.Y.Z]`). It runs `tools/release.py prepare`
  and opens the PR `chore: release vX.Y.Z`. The next version follows the commits since the last tag (breaking →
  major, `feat` → minor, `fix` → patch); the first release needs an explicit version. The PR updates `version` and
  `version_code` in `Cargo.toml`, `Cargo.lock`, the F-Droid recipe, `CHANGELOG.md` and drafts the F-Droid
  changelogs `fastlane/metadata/android/{en-US,de-DE}/changelogs/<versionCode>.txt`.
- In the release PR, rewrite both changelogs for users (short, max. 500 characters, one point per line starting with `•`; the app
  shows them as a list) and translate the German one;
  it starts with `TODO` and `cargo test` fails until it doesn't. These texts are also the changelog in the app
  (Settings → About → Changelog, all releases up to the running build; `src/changes.rs`, embedded by `build.rs`).
- Merging the release PR runs the `publish` job: it tags `vX.Y.Z`, creates the GitHub release with the
  `CHANGELOG.md` section and, if the `RELEASE_KEYSTORE` (base64) and `RELEASE_KEYSTORE_PASSWORD` secrets are set,
  attaches a signed APK. F-Droid picks up the tag (see `fdroid/README.md`).
