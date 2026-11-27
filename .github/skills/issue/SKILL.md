---
name: issue
description: Implement a GitHub issue of Flausch end to end. Use when the user asks to implement, build or work on an issue or ticket (e.g. "setz #5 um", "implement #12"). Reads the issue, builds the feature or fix on its own branch with tests, translations and docs, checks it on the emulator and finally installs the debug app "Flausch Dev" on the user's phone so they can try it.
---

# Implementing an issue

The user wants to try the result on their phone, so the work is only done once **Flausch Dev** is installed
there. Follow `AGENTS.md` throughout; this is the order of steps.

## 1. Read the issue

```sh
gh issue view <n> --json title,body,comments,milestone,labels
```

Issues are often a single line. Fill the gaps from the principles in `AGENTS.md` (offline, one hand, little
text, no new modals) and from similar features: find the closest merged PR (`git log --oneline`, `git show
<sha> --stat`) and mirror its structure (model → `Store` contract → controller → page/`Sheet` → menu or row,
tests, translations, docs). If the issue leaves product decisions open (units, what happens on delete,
where it lives in the UI), ask them all at once in one question before writing code, with a recommended
option each. Don't ask again mid-way unless something new comes up.

## 2. Branch

```sh
git switch main && git pull --ff-only
git switch -c <type>/<n>-<short-name>   # e.g. feat/5-crochet-hooks
```

## 3. Build it

- Model changes in `model.rs` (no slint) with unit tests; new data stays optional on disk
  (`serde(default, skip_serializing_if)`), or add a migration with a test.
- UI reads `Store` and calls callbacks; logic stays in Rust.
- New icons: 24×24 stroke SVGs like the others in `ui/icons/`. Render and look at them before using them, e.g.
  with `resvg-py` in a venv in the scratchpad (ImageMagick renders them blank).
- A flow in `tests/ui.rs` through accessibility labels, plus controller tests in `src/tests.rs` where useful.
- Every new string translated in `lang/de/LC_MESSAGES/flausch.po` (see "Translations" in `AGENTS.md`;
  German uses "du"). Check that German labels fit, especially sheet titles next to the trash icon.
- Update `AGENTS.md`, and for user-visible features the feature lists in `README.md`,
  `fastlane/metadata/android/{en-US,de-DE}/full_description.txt` and `website/{,de/}index.html`.

While iterating, build with `cargo build` and run only the tests you touch (`cargo test <name>`).
Once at the end:

```sh
make check && make test
```

For Android-specific changes also `make check-android`; after dependency changes `make deny`.

## 4. Check on the emulator

Only for new or changed screens; skip it for logic-only changes (the UI tests cover those). Build for the
emulator (`make apk TARGET=x86_64-linux-android`; start it with `make emulator` in the background if it isn't
running), install, and take at most 3 screenshots of the new screens, cropped to the changed area and scaled to
50% (`magick … -crop … -resize 50%`). Tap with physical pixels from the full-size screen, never from a scaled or
side-by-side image. Fix what looks wrong (cut-off German text, dark theme), then go on; the user tries the rest
on the phone. Leave the emulator running.

## 5. Install on the phone

This is the one routine use of the phone: install the finished debug build so the user can try it.

```sh
adb devices -l                                   # the phone is the entry that is not emulator-*
make apk                                         # aarch64 -> target/debug/apk/flausch-dev.apk
adb -s <phone serial> install -r target/debug/apk/flausch-dev.apk
```

- Only install **Flausch Dev** (`io.github.ldrick.flausch.debug`) with `install -r`, which keeps its data. Never
  uninstall it, clear its data, touch the F-Droid app `io.github.ldrick.flausch`, or send input on the phone.
- If the phone isn't connected or the install fails (e.g. a signature mismatch would need an uninstall), stop
  and tell the user; don't work around it.
- Don't build the arm64 APK and the x86_64 APK at the same time: they share `target/debug/apk/flausch-dev.apk`.

## 6. Report

Tell the user, in their language, what was built, what to try on the phone and where it is in the app, and any
decisions you made for them. Then ask whether to commit, push and open the PR. Don't commit before they say so.
When they agree: a Conventional Commits title that reads well in the changelog (`feat: crochet hooks`) and
`Closes #<n>` in the body.
