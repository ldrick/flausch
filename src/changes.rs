//! The changelog in Settings → About: the F-Droid changelogs, embedded by `build.rs`.

include!(concat!(env!("OUT_DIR"), "/changes.rs"));

/// The version code of this build, as cargo-apk and F-Droid compute it.
pub fn current() -> u32 {
    let [major, minor, patch] = [
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
    ]
    .map(|part| part.parse::<u32>().unwrap_or(0));
    version_code(major, minor, patch)
}

pub fn version_code(major: u32, minor: u32, patch: u32) -> u32 {
    1 << 24 | major << 16 | minor << 8 | patch
}

pub fn version_name(code: u32) -> String {
    format!("{}.{}.{}", code >> 16 & 0xff, code >> 8 & 0xff, code & 0xff)
}

/// Version name and text of every release up to this build, newest first; `language` as in
/// `Language::translation`.
pub fn list(language: &str) -> Vec<(String, String)> {
    releases(CHANGES, current())
        .map(|&(code, en, de)| {
            let text = if language == "de" { de } else { en };
            (version_name(code), text.to_owned())
        })
        .collect()
}

type Entry = (u32, &'static str, &'static str);

fn releases(changes: &[Entry], current: u32) -> impl Iterator<Item = &Entry> {
    changes.iter().filter(move |(code, ..)| *code <= current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names_round_trip() {
        assert_eq!(version_code(1, 2, 3), 16_843_267);
        assert_eq!(version_name(16_843_267), "1.2.3");
    }

    #[test]
    fn every_release_has_both_changelogs() {
        for locale in ["en-US", "de-DE"] {
            let path = format!(
                "{}/fastlane/metadata/android/{locale}/changelogs/{}.txt",
                env!("CARGO_MANIFEST_DIR"),
                current()
            );
            let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {path}"));
            assert!(!text.trim().is_empty(), "{path} is empty");
            assert!(!text.contains("TODO"), "{path}: write the final text");
            assert!(
                text.chars().count() <= 500,
                "{path}: F-Droid allows 500 characters"
            );
        }
    }

    #[test]
    fn future_releases_are_left_out() {
        let changes = [
            (version_code(1, 2, 0), "1.2", ""),
            (version_code(1, 1, 0), "1.1", ""),
            (version_code(1, 0, 0), "1.0", ""),
        ];
        let texts = |current| {
            releases(&changes, current)
                .map(|(_, en, _)| *en)
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(version_code(1, 0, 0)), ["1.0"]);
        assert_eq!(texts(version_code(1, 1, 0)), ["1.1", "1.0"]);
        assert_eq!(list("de").len(), list("en").len());
        assert_ne!(list("en"), []);
    }
}
