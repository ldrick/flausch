#!/usr/bin/env python3
"""Prepares a release and prints release notes (see "Workflow" in AGENTS.md).

    tools/release.py prepare [VERSION] [--keep REF]
        bump the version, update CHANGELOG.md, the F-Droid recipe and draft the F-Droid changelogs. VERSION
        defaults to the next one from the Conventional Commits since the last tag (1.0.0 if nothing is tagged).
        REF is the previous release branch: F-Droid changelogs edited there are kept, and commits added to
        main since then are appended to them.
    tools/release.py notes VERSION
        print the CHANGELOG.md section of VERSION

`prepare` prints the new version, or nothing if there is nothing to release. The German F-Droid changelog starts
with TODO until someone translates it; `cargo test` fails until then.
"""

import datetime
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CARGO_TOML = ROOT / "Cargo.toml"
CARGO_LOCK = ROOT / "Cargo.lock"
CHANGELOG = ROOT / "CHANGELOG.md"
RECIPE = ROOT / "fdroid" / "io.github.ldrick.flausch.yml"
FASTLANE = ROOT / "fastlane" / "metadata" / "android"
REPO = "https://github.com/ldrick/flausch"
FIRST_VERSION = "1.0.0"
TODO = "TODO: translate"

CHANGELOG_HEADER = """# Changelog

All notable changes to Flausch. The format follows [Keep a Changelog](https://keepachangelog.com), versions
follow [Semantic Versioning](https://semver.org). This file is written by `tools/release.py`.
"""

SECTIONS = [("feat", "Features"), ("fix", "Bug fixes")]
COMMIT = re.compile(r"^(?P<type>\w+)(?:\([^)]*\))?(?P<breaking>!)?: (?P<text>.+)$")


def git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=ROOT, check=True, capture_output=True, text=True).stdout


def show(ref: str, path: pathlib.Path) -> str | None:
    result = subprocess.run(["git", "show", f"{ref}:{path.relative_to(ROOT).as_posix()}"], cwd=ROOT,
                            capture_output=True, text=True)
    return result.stdout if result.returncode == 0 else None


def parse_version(version: str) -> tuple[int, int, int]:
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match:
        sys.exit(f"not a version: {version}")
    return tuple(int(part) for part in match.groups())


def version_code(version: str) -> int:
    major, minor, patch = parse_version(version)
    return 1 << 24 | major << 16 | minor << 8 | patch


def cargo_version(text: str | None = None) -> str:
    text = CARGO_TOML.read_text() if text is None else text
    match = re.search(r'^\[package\]\nname = "flausch"\nversion = "([^"]+)"', text, re.M)
    if not match:
        sys.exit("cannot find the version in Cargo.toml")
    return match.group(1)


def last_tag() -> str | None:
    tags = git("tag", "--list", "v*", "--sort=-v:refname").split()
    return tags[0] if tags else None


def commits_since(tag: str | None, end: str = "HEAD") -> list[dict]:
    log = git("log", "--format=%s%x1f%b%x1e", f"{tag}..{end}" if tag else end)
    commits = []
    for entry in filter(None, (e.strip() for e in log.split("\x1e"))):
        subject, _, body = entry.partition("\x1f")
        match = COMMIT.match(subject)
        if not match or match["text"].startswith("release v"):
            continue
        commits.append(
            {
                "type": match["type"],
                "text": match["text"],
                "breaking": bool(match["breaking"]) or "BREAKING CHANGE" in body,
            }
        )
    return commits


def next_version(current: str, commits: list[dict]) -> str | None:
    major, minor, patch = parse_version(current)
    if any(c["breaking"] for c in commits):
        return f"{major + 1}.0.0" if major > 0 else f"0.{minor + 1}.0"
    if any(c["type"] == "feat" for c in commits):
        return f"{major}.{minor + 1}.0"
    if any(c["type"] == "fix" for c in commits):
        return f"{major}.{minor}.{patch + 1}"
    return None


def entry_text(commit: dict) -> str:
    text = commit["text"]
    text = re.sub(r"\(#(\d+)\)$", rf"([#\1]({REPO}/pull/\1))", text)
    return text[0].upper() + text[1:]


def plain_text(commit: dict) -> str:
    text = re.sub(r"\s*\(#\d+\)$", "", commit["text"])
    return text[0].upper() + text[1:]


def replace(path: pathlib.Path, pattern: str, value: str, count: int = 1) -> None:
    text, n = re.subn(pattern, lambda m: m.group(1) + value + (m.group(2) if m.lastindex >= 2 else ""),
                      path.read_text(), count=count, flags=re.M)
    if n == 0:
        sys.exit(f"{path.name}: no match for {pattern}")
    path.write_text(text)


def draft(commits: list[dict]) -> str:
    return "\n".join(f"• {plain_text(c)}" for c in commits if c["type"] in ("feat", "fix"))


def changelog_path(locale: str, version: str) -> pathlib.Path:
    return FASTLANE / locale / "changelogs" / f"{version_code(version)}.txt"


def update_files(version: str, commits: list[dict], first: bool) -> None:
    code = str(version_code(version))
    old_code = str(version_code(cargo_version()))
    replace(CARGO_TOML, r'(^\[package\]\nname = "flausch"\nversion = ")[^"]+(")', version)
    replace(CARGO_TOML, r"(^version_code = )\d+()", code)
    replace(CARGO_LOCK, r'(^name = "flausch"\nversion = ")[^"]+(")', version)
    replace(RECIPE, r"(^  - versionName: ).+()", version)
    replace(RECIPE, r"(^    versionCode: )\d+()", code)
    replace(RECIPE, r"(^    commit: v).+()", version)
    replace(RECIPE, r"(^CurrentVersion: ).+()", version)
    replace(RECIPE, r"(^CurrentVersionCode: )\d+()", code)

    today = datetime.date.today().isoformat()
    section = [f"## [{version}]({REPO}/releases/tag/v{version}) - {today}"]
    for kind, title in SECTIONS:
        entries = [entry_text(c) for c in commits if c["type"] == kind]
        if entries:
            section += ["", f"### {title}", "", *(f"- {e}" for e in entries)]
    if len(section) == 1:
        section += ["", "Maintenance release."]
    old = CHANGELOG.read_text().removeprefix(CHANGELOG_HEADER) if CHANGELOG.exists() else ""
    CHANGELOG.write_text(CHANGELOG_HEADER + "\n" + "\n".join(section) + "\n" + ("\n" + old.lstrip() if old.strip() else ""))

    text = draft(commits) or "Small improvements."
    for locale, text in (("en-US", text), ("de-DE", f"{TODO}\n{text}")):
        path = FASTLANE / locale / "changelogs" / f"{code}.txt"
        unreleased = path.with_name(f"{old_code}.txt")
        if first and unreleased.exists() and unreleased != path:
            # Nothing was released yet: the texts written for the current version are the first ones.
            unreleased.rename(path)
        elif not path.exists():
            path.write_text(text + "\n")


def keep_edits(keep: str, tag: str | None, version: str) -> None:
    """Keeps the F-Droid changelogs that someone edited on the previous release branch `keep`."""
    base = git("merge-base", keep, "HEAD").strip()
    kept_version = cargo_version(show(keep, CARGO_TOML) or "")
    base_version = cargo_version(show(base, CARGO_TOML) or "")
    new = draft(commits_since(base))
    for locale in ("en-US", "de-DE"):
        kept = show(keep, changelog_path(locale, kept_version))
        if kept is None:
            continue
        # What `prepare` wrote on the release branch, so anything else was edited by hand.
        written = show(base, changelog_path(locale, kept_version))
        if written is None and tag is None:
            written = show(base, changelog_path(locale, base_version))
        if written is None:
            written = draft(commits_since(tag, base)) or "Small improvements."
            written = (f"{TODO}\n" if locale == "de-DE" else "") + written + "\n"
        if kept == written:
            continue
        text = kept.rstrip("\n")
        if new:
            text += "\n" + new
            if locale == "de-DE" and not text.startswith("TODO"):
                text = f"{TODO}\n{text}"
        changelog_path(locale, version).write_text(text + "\n")


def prepare(requested: str | None, keep: str | None) -> None:
    tag = last_tag()
    commits = commits_since(tag)
    current = tag.removeprefix("v") if tag else cargo_version()
    version = requested or (next_version(current, commits) if tag else FIRST_VERSION)
    if version is None:
        return
    if tag and parse_version(version) <= parse_version(current):
        sys.exit(f"{version} is not newer than {current}")
    update_files(version, commits, first=tag is None)
    if keep:
        keep_edits(keep, tag, version)
    print(version)


def notes(version: str) -> None:
    text = CHANGELOG.read_text()
    match = re.search(rf"^## \[{re.escape(version)}\].*?\n(.*?)(?=^## |\Z)", text, re.M | re.S)
    if not match:
        sys.exit(f"no section for {version} in CHANGELOG.md")
    print(match.group(1).strip())


def main() -> None:
    args = sys.argv[1:]
    keep = None
    if "--keep" in args:
        i = args.index("--keep")
        if i + 1 == len(args):
            sys.exit(__doc__)
        keep = args[i + 1]
        del args[i : i + 2]
    match args:
        case ["prepare"]:
            prepare(None, keep)
        case ["prepare", version]:
            prepare(version, keep)
        case ["notes", version] if keep is None:
            notes(version)
        case _:
            sys.exit(__doc__)


if __name__ == "__main__":
    main()
