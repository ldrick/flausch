#!/usr/bin/env python3
"""Prepares a release and prints release notes (see "Workflow" in AGENTS.md).

    tools/release.py prepare [VERSION]   bump the version, update CHANGELOG.md, the F-Droid recipe and draft
                                         the F-Droid changelogs; VERSION defaults to the next one from the
                                         Conventional Commits since the last tag
    tools/release.py notes VERSION       print the CHANGELOG.md section of VERSION

`prepare` prints the new version. The German F-Droid changelog starts with TODO until someone translates it;
`cargo test` fails until then.
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

CHANGELOG_HEADER = """# Changelog

All notable changes to Flausch. The format follows [Keep a Changelog](https://keepachangelog.com), versions
follow [Semantic Versioning](https://semver.org). This file is written by `tools/release.py`.
"""

SECTIONS = [("feat", "Features"), ("fix", "Bug fixes")]
COMMIT = re.compile(r"^(?P<type>\w+)(?:\([^)]*\))?(?P<breaking>!)?: (?P<text>.+)$")


def git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=ROOT, check=True, capture_output=True, text=True).stdout


def parse_version(version: str) -> tuple[int, int, int]:
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match:
        sys.exit(f"not a version: {version}")
    return tuple(int(part) for part in match.groups())


def version_code(version: str) -> int:
    major, minor, patch = parse_version(version)
    return 1 << 24 | major << 16 | minor << 8 | patch


def cargo_version() -> str:
    match = re.search(r'^\[package\]\nname = "flausch"\nversion = "([^"]+)"', CARGO_TOML.read_text(), re.M)
    if not match:
        sys.exit("cannot find the version in Cargo.toml")
    return match.group(1)


def last_tag() -> str | None:
    tags = git("tag", "--list", "v*", "--sort=-v:refname").split()
    return tags[0] if tags else None


def commits_since(tag: str | None) -> list[dict]:
    log = git("log", "--format=%s%x1f%b%x1e", f"{tag}..HEAD" if tag else "HEAD")
    commits = []
    for entry in filter(None, (e.strip() for e in log.split("\x1e"))):
        subject, _, body = entry.partition("\x1f")
        match = COMMIT.match(subject)
        if not match:
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

    draft = "\n".join(f"• {plain_text(c)}" for c in commits if c["type"] in ("feat", "fix")) or "Small improvements."
    for locale, text in (("en-US", draft), ("de-DE", "TODO: translate\n" + draft)):
        path = FASTLANE / locale / "changelogs" / f"{code}.txt"
        unreleased = path.with_name(f"{old_code}.txt")
        if first and unreleased.exists() and unreleased != path:
            # Nothing was released yet: the texts written for the current version are the first ones.
            unreleased.rename(path)
        elif not path.exists():
            path.write_text(text + "\n")


def prepare(requested: str | None) -> None:
    tag = last_tag()
    commits = [c for c in commits_since(tag) if not c["text"].startswith("release v")]
    if tag is None and requested is None:
        sys.exit("nothing is tagged yet: give the version of the first release, e.g. 1.0.0")
    current = tag.removeprefix("v") if tag else cargo_version()
    version = requested or next_version(current, commits)
    if version is None:
        sys.exit(f"nothing to release since {tag}: no feat or fix commits")
    if tag and parse_version(version) <= parse_version(current):
        sys.exit(f"{version} is not newer than {current}")
    update_files(version, commits, first=tag is None)
    print(version)


def notes(version: str) -> None:
    text = CHANGELOG.read_text()
    match = re.search(rf"^## \[{re.escape(version)}\].*?\n(.*?)(?=^## |\Z)", text, re.M | re.S)
    if not match:
        sys.exit(f"no section for {version} in CHANGELOG.md")
    print(match.group(1).strip())


def main() -> None:
    match sys.argv[1:]:
        case ["prepare"]:
            prepare(None)
        case ["prepare", version]:
            prepare(version)
        case ["notes", version]:
            notes(version)
        case _:
            sys.exit(__doc__)


if __name__ == "__main__":
    main()
