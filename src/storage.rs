//! JSON persistence in the app's data directory. Must not depend on `slint`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::model::{Library, Settings};

const LIBRARY_FILE: &str = "projects.json";
const SETTINGS_FILE: &str = "settings.json";
const PDF_DIR: &str = "pdfs";

pub struct Storage {
    dir: PathBuf,
}

impl Storage {
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(dir.join(PDF_DIR))?;
        Ok(Self { dir })
    }

    pub fn load_library(&self) -> Library {
        load_or_default(&self.dir.join(LIBRARY_FILE))
    }

    pub fn save_library(&self, library: &Library) -> io::Result<()> {
        write_json(&self.dir.join(LIBRARY_FILE), library)
    }

    pub fn load_settings(&self) -> Settings {
        load_or_default(&self.dir.join(SETTINGS_FILE))
    }

    pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        write_json(&self.dir.join(SETTINGS_FILE), settings)
    }

    pub fn pdf_path(&self, file: &str) -> PathBuf {
        self.dir.join(PDF_DIR).join(file)
    }

    /// File name for a newly imported PDF. Unique per import so that a replaced PDF never
    /// collides with the one it replaces.
    pub fn new_pdf_file(&self, project_id: u64, now_nanos: u128) -> String {
        format!("{project_id}-{now_nanos:x}.pdf")
    }

    pub fn remove_pdf(&self, file: &str) {
        let path = self.pdf_path(file);
        if let Err(e) = fs::remove_file(&path)
            && e.kind() != io::ErrorKind::NotFound
        {
            eprintln!("nøste: cannot remove {}: {e}", path.display());
        }
    }
}

/// Missing files yield the default. Unreadable files are kept as `*.broken` so no data is
/// silently overwritten.
fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return T::default(),
        Err(e) => {
            eprintln!("nøste: cannot read {}: {e}", path.display());
            return T::default();
        }
    };
    match serde_json::from_slice(&data) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("nøste: cannot parse {}: {e}", path.display());
            let _ = fs::rename(path, path.with_extension("json.broken"));
            T::default()
        }
    }
}

/// Writes to a temporary file and renames it, so a crash never leaves a half-written file.
fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Language;

    #[test]
    fn missing_files_load_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path().join("data")).unwrap();
        assert_eq!(storage.load_library(), Library::default());
        assert_eq!(storage.load_settings(), Settings::default());
    }

    #[test]
    fn library_and_settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path().to_path_buf()).unwrap();
        let mut library = Library::default();
        let id = library.create("Mütze", Some(40), 1);
        library.increment(id, 2);
        storage.save_library(&library).unwrap();
        assert_eq!(storage.load_library(), library);

        let settings = Settings {
            language: Language::De,
            ..Settings::default()
        };
        storage.save_settings(&settings).unwrap();
        assert_eq!(storage.load_settings(), settings);
    }

    #[test]
    fn broken_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path().to_path_buf()).unwrap();
        fs::write(dir.path().join(LIBRARY_FILE), b"{ not json").unwrap();
        assert_eq!(storage.load_library(), Library::default());
        assert!(dir.path().join("projects.json.broken").exists());
    }

    #[test]
    fn older_settings_without_new_fields_load() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path().to_path_buf()).unwrap();
        fs::write(dir.path().join(SETTINGS_FILE), br#"{"version":1}"#).unwrap();
        assert!(storage.load_settings().keep_screen_on);
    }
}
