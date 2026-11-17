---
name: release
description: Release a new version of Flausch. Use when the user asks to release, publish or ship a version, or to finish the release PR. Writes the user-facing F-Droid changelogs (English and German) in the open release PR, waits for CI and merges after the user agrees.
---

# Releasing Flausch

The Release workflow keeps the PR `chore: release vX.Y.Z` (branch `release/next`) up to date on every push to
`main`: version bump, `CHANGELOG.md` and drafts of the F-Droid changelogs. Merging it tags the version, creates
the GitHub release and attaches the signed APK. Your job is to turn the drafts into texts for users and merge.
See "Workflow" in `AGENTS.md` for the background.

## 1. Find the release PR

```sh
gh pr list --head release/next --state open --json number,title,url
```

No PR means there is no `feat` or `fix` since the last tag: tell the user and stop. Otherwise note the version
from the title and check out the branch:

```sh
git fetch origin release/next && git switch -C release/next origin/release/next
```

## 2. Write the changelogs

Read the new section of `CHANGELOG.md` and the drafts in
`fastlane/metadata/android/{en-US,de-DE}/changelogs/<versionCode>.txt` (the only new files there). Rewrite both:

- For users, not developers: what they can do now or what works better. Merge related points, drop internal
  changes (CI, refactoring, tooling) and PR numbers.
- One point per line, each starting with `• `. The app shows them as a list.
- At most 500 characters per file (F-Droid's limit), the shorter the better.
- German: a translation of the English text, without the `TODO` line. Use the app's own words from
  `lang/de/LC_MESSAGES/flausch.po` (e.g. Projekt, Teil, Reihe, Anleitung).
- Look at earlier files in the same directories for tone and length.

Show both texts to the user and wait for their OK or changes. Don't push before that.

## 3. Push

```sh
cargo test changes        # every release has both changelogs, no TODO, max. 500 characters
git add fastlane/metadata/android && git commit -m "docs: changelog for vX.Y.Z"
git push origin release/next
```

If the push is rejected, `main` moved and the workflow rebuilt the branch. It keeps edited changelogs and appends
new points (German gets `TODO` again). Fetch, check out again and repeat step 2 for the new points.

## 4. Merge after the user agrees

```sh
gh pr checks <number> --watch
```

Report the result. Merge only when CI is green and the user says so; keep the title, it starts the publish job:

```sh
gh pr merge <number> --squash
```

Then watch the run (`gh run list --workflow release.yml --limit 1`, `gh run watch <id>`) and report the release
URL (`gh release view vX.Y.Z --json url,assets`). Without the `RELEASE_KEYSTORE` secret the release has no APK;
say so. F-Droid picks up the tag by itself.
