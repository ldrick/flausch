use std::env;
use std::path::PathBuf;

fn main() {
    let android = env::var("TARGET").unwrap().contains("android");
    // The TalkBack bridge (src/a11y.rs) and the UI tests walk the element tree, which needs debug info.
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("lang")
        .with_debug_info(true);
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");
    println!("cargo:rerun-if-changed=lang");
    embed_changelogs();

    if android {
        build_java_helper();
    }
}

/// Writes the F-Droid changelogs (`fastlane/metadata/android/<locale>/changelogs/<version code>.txt`) to
/// `$OUT_DIR/changes.rs` as `CHANGES: &[(version code, English, German)]`, newest first, for "What's new".
fn embed_changelogs() {
    use std::fmt::Write;

    let mut changes: std::collections::BTreeMap<u32, [String; 2]> =
        std::collections::BTreeMap::new();
    for (i, locale) in ["en-US", "de-DE"].into_iter().enumerate() {
        let dir = format!("fastlane/metadata/android/{locale}/changelogs");
        println!("cargo:rerun-if-changed={dir}");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let Some(code) = path.file_stem().and_then(|s| s.to_str()?.parse().ok()) else {
                continue;
            };
            let text = std::fs::read_to_string(&path).unwrap();
            text.trim()
                .clone_into(&mut changes.entry(code).or_default()[i]);
        }
    }
    let mut out = String::from(
        "#[allow(clippy::unreadable_literal, reason = \"generated\")]\npub(crate) const CHANGES: &[(u32, &str, &str)] = &[\n",
    );
    for (code, [en, de]) in changes.iter().rev() {
        let de = if de.is_empty() { en } else { de };
        writeln!(out, "    ({code}, {en:?}, {de:?}),").unwrap();
    }
    out += "];\n";
    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    std::fs::write(out_dir.join("changes.rs"), out).unwrap();
}

/// Compiles `android/java` to `$OUT_DIR/classes.dex`, which `src/platform/android.rs` loads at
/// runtime. Needs `ANDROID_HOME` and `JAVA_HOME` (the Makefile sets both).
fn build_java_helper() {
    use android_build::{Dexer, JavaBuild};

    let src = "android/java/io/github/ldrick/flausch/FlauschHelper.java";
    println!("cargo:rerun-if-changed={src}");

    let release = env::var("PROFILE").as_deref() == Ok("release");
    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    let classes = out_dir.join("flausch-java");
    let _ = std::fs::remove_dir_all(&classes);
    std::fs::create_dir_all(&classes).unwrap();

    let android_jar =
        android_build::android_jar(None).expect("No Android platform found in ANDROID_HOME");

    let output = JavaBuild::new()
        .file(src)
        .class_path(&android_jar)
        .classes_out_dir(&classes)
        .java_source_version(8)
        .java_target_version(8)
        .command()
        .expect("Cannot build the javac command")
        .args(["-encoding", "UTF-8", "-nowarn", "-Xlint:-options"])
        .output()
        .expect("Cannot run javac");
    assert!(
        output.status.success(),
        "javac failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Dexer::new()
        .android_jar(&android_jar)
        .class_path(&classes)
        .collect_classes(&classes)
        .unwrap()
        .release(release)
        .android_min_api(29)
        .out_dir(&out_dir)
        .command()
        .expect("Cannot build the d8 command")
        .output()
        .expect("Cannot run d8");
    assert!(
        output.status.success(),
        "d8 failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
