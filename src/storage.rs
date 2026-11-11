//! JSON persistence in the app's data directory. Must not depend on `slint`.

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::model::{LIBRARY_VERSION, Library, Settings};

const LIBRARY_FILE: &str = "projects.json";
const SETTINGS_FILE: &str = "settings.json";
const PDF_DIR: &str = "pdfs";
const COVER_DIR: &str = "covers";
/// Temporary files of a backup export or import; removed on start.
const EXPORT_FILE: &str = "export.zip";
const IMPORT_FILE: &str = "import.zip";
const RESTORE_DIR: &str = "restore.tmp";
const INCOMING_PDF: &str = "incoming.pdf";
const INCOMING_TITLE: &str = "incoming.txt";
/// Upper bound for the JSON files in a backup, against broken or malicious files.
const MAX_JSON_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct Storage {
    dir: PathBuf,
}

/// The contents of a backup ZIP. `settings` is `None` if the backup has none.
#[derive(Debug, PartialEq)]
pub struct Backup {
    pub library: Library,
    pub settings: Option<Settings>,
}

#[derive(Debug)]
pub enum BackupError {
    /// The file is not a Flausch backup (no ZIP, or no readable `projects.json`).
    Invalid,
    /// The backup was written by a newer version of the app.
    TooNew,
    Io(io::Error),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => f.write_str("not a Flausch backup"),
            Self::TooNew => f.write_str("backup from a newer version"),
            Self::Io(e) => e.fmt(f),
        }
    }
}

impl From<io::Error> for BackupError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl Storage {
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(dir.join(PDF_DIR))?;
        fs::create_dir_all(dir.join(COVER_DIR))?;
        remove_file(&dir.join(EXPORT_FILE));
        remove_file(&dir.join(IMPORT_FILE));
        let _ = fs::remove_dir_all(dir.join(RESTORE_DIR));
        Ok(Self { dir })
    }

    pub fn load_library(&self) -> Library {
        let mut library: Library = load_or_default(&self.dir.join(LIBRARY_FILE));
        library.migrate();
        library
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
    pub fn new_pdf_file(project_id: u64, now_nanos: u128) -> String {
        format!("{project_id}-{now_nanos:x}.pdf")
    }

    /// Thumbnail of the first page of the PDF `file`, see `pdf::Covers`.
    pub fn cover_path(&self, file: &str) -> PathBuf {
        self.dir.join(COVER_DIR).join(format!("{file}.png"))
    }

    /// Removes the PDF and its cover.
    pub fn remove_pdf(&self, file: &str) {
        remove_file(&self.pdf_path(file));
        remove_file(&self.cover_path(file));
    }

    /// Where a PDF opened or shared from another app waits until the user decides where it goes,
    /// and the file with its title. Static, because Android writes them before the app starts.
    pub fn incoming_paths(dir: &Path) -> (PathBuf, PathBuf) {
        (dir.join(INCOMING_PDF), dir.join(INCOMING_TITLE))
    }

    /// The path and title of a waiting PDF, see `incoming_paths`.
    pub fn incoming(&self) -> Option<(PathBuf, String)> {
        let (pdf, title) = Self::incoming_paths(&self.dir);
        if !pdf.is_file() {
            return None;
        }
        let title = fs::read_to_string(title).unwrap_or_default();
        let title = match title.trim() {
            "" => "document.pdf".to_owned(),
            title => title.to_owned(),
        };
        Some((pdf, title))
    }

    /// Moves the waiting PDF into the PDF directory as `file`.
    pub fn adopt_incoming(&self, file: &str) -> io::Result<()> {
        let (pdf, title) = Self::incoming_paths(&self.dir);
        fs::rename(pdf, self.pdf_path(file))?;
        remove_file(&title);
        Ok(())
    }

    pub fn discard_incoming(&self) {
        let (pdf, title) = Self::incoming_paths(&self.dir);
        remove_file(&pdf);
        remove_file(&title);
    }

    /// Where a backup is written before it is copied to the place the user chose.
    pub fn export_path(&self) -> PathBuf {
        self.dir.join(EXPORT_FILE)
    }

    /// Where a chosen backup is copied to before it is read.
    pub fn import_path(&self) -> PathBuf {
        self.dir.join(IMPORT_FILE)
    }

    /// Writes a backup ZIP with the projects, the settings and every PDF of a project.
    pub fn export(&self, library: &Library, settings: &Settings, path: &Path) -> io::Result<()> {
        let mut zip = ZipWriter::new(fs::File::create(path)?);
        let json = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        // PDFs are compressed already.
        let pdf = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(true);
        zip.start_file(LIBRARY_FILE, json)?;
        serde_json::to_writer_pretty(&mut zip, library)?;
        zip.start_file(SETTINGS_FILE, json)?;
        serde_json::to_writer_pretty(&mut zip, settings)?;
        for file in library.projects.iter().filter_map(|p| p.pdf.as_ref()) {
            let mut source = fs::File::open(self.pdf_path(&file.file))?;
            zip.start_file(format!("{PDF_DIR}/{}", file.file), pdf)?;
            io::copy(&mut source, &mut zip)?;
        }
        zip.finish()?.sync_all()
    }

    /// Reads and checks a backup without changing any data. Projects whose PDF is missing in the
    /// backup keep their parts but lose the pattern.
    pub fn read_backup(path: &Path) -> Result<Backup, BackupError> {
        let mut zip = ZipArchive::new(fs::File::open(path)?).map_err(|_| BackupError::Invalid)?;
        let mut library: Library = read_json(&mut zip, LIBRARY_FILE).ok_or(BackupError::Invalid)?;
        if library.version > LIBRARY_VERSION {
            return Err(BackupError::TooNew);
        }
        library.migrate();
        for project in &mut library.projects {
            if project.pdf.as_ref().is_some_and(|pdf| {
                !is_plain_name(&pdf.file)
                    || zip
                        .index_for_name(&format!("{PDF_DIR}/{}", pdf.file))
                        .is_none()
            }) {
                project.pdf = None;
            }
        }
        let settings = read_json(&mut zip, SETTINGS_FILE);
        Ok(Backup { library, settings })
    }

    /// Replaces all projects, PDFs and settings with the backup at `path`. Settings stay as they
    /// are if the backup has none.
    pub fn restore(&self, path: &Path) -> Result<Backup, BackupError> {
        let backup = Self::read_backup(path)?;
        let staging = self.dir.join(RESTORE_DIR);
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(staging.join(PDF_DIR))?;
        if let Err(e) = extract_pdfs(path, &backup.library, &staging.join(PDF_DIR)) {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
        let old = staging.join("old");
        fs::rename(self.dir.join(PDF_DIR), &old)?;
        if let Err(e) = fs::rename(staging.join(PDF_DIR), self.dir.join(PDF_DIR)) {
            let _ = fs::rename(&old, self.dir.join(PDF_DIR));
            return Err(e.into());
        }
        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(self.dir.join(COVER_DIR));
        fs::create_dir_all(self.dir.join(COVER_DIR))?;
        self.save_library(&backup.library)?;
        if let Some(settings) = &backup.settings {
            self.save_settings(settings)?;
        }
        Ok(backup)
    }
}

fn extract_pdfs(path: &Path, library: &Library, dest: &Path) -> Result<(), BackupError> {
    let mut zip = ZipArchive::new(fs::File::open(path)?).map_err(|_| BackupError::Invalid)?;
    for pdf in library.projects.iter().filter_map(|p| p.pdf.as_ref()) {
        let mut entry = zip
            .by_name(&format!("{PDF_DIR}/{}", pdf.file))
            .map_err(|_| BackupError::Invalid)?;
        let mut out = fs::File::create(dest.join(&pdf.file))?;
        io::copy(&mut entry, &mut out)?;
        out.sync_all()?;
    }
    Ok(())
}

/// File name for a backup written at `secs` (Unix time), e.g. `flausch-2026-11-10.zip`.
pub fn backup_name(secs: u64) -> String {
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm), in UTC.
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("flausch-{year:04}-{month:02}-{day:02}.zip")
}

/// A file name without any directory part, so it stays inside the PDF directory.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

fn read_json<T: DeserializeOwned>(zip: &mut ZipArchive<fs::File>, name: &str) -> Option<T> {
    let entry = zip.by_name(name).ok()?;
    let mut data = Vec::new();
    entry.take(MAX_JSON_SIZE).read_to_end(&mut data).ok()?;
    serde_json::from_slice(&data).ok()
}

fn remove_file(path: &Path) {
    if let Err(e) = fs::remove_file(path)
        && e.kind() != io::ErrorKind::NotFound
    {
        eprintln!("flausch: cannot remove {}: {e}", path.display());
    }
}

/// Missing files yield the default. Unreadable files are kept as `*.broken` so no data is
/// silently overwritten.
fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return T::default(),
        Err(e) => {
            eprintln!("flausch: cannot read {}: {e}", path.display());
            return T::default();
        }
    };
    match serde_json::from_slice(&data) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("flausch: cannot parse {}: {e}", path.display());
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
    use crate::model::{Language, LineColor, Pdf};

    /// A storage with one project whose pattern is the PDF `file` with `content`.
    fn with_pdf(dir: &Path, file: &str, content: &[u8]) -> (Storage, Library) {
        let storage = Storage::new(dir.to_path_buf()).unwrap();
        let mut library = Library::default();
        let id = library.create("Hase", 1);
        library.add_part(id, "Kopf", Some(22), "", 1).unwrap();
        library.get_mut(id).unwrap().pdf = Some(Pdf::new(file.into(), "Hase.pdf".into()));
        fs::write(storage.pdf_path(file), content).unwrap();
        storage.save_library(&library).unwrap();
        (storage, library)
    }

    fn zip_with(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
        for (name, data) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn backup_round_trip() {
        let source = tempfile::tempdir().unwrap();
        let (storage, library) = with_pdf(source.path(), "1-a.pdf", b"%PDF-old");
        let settings = Settings {
            language: Language::De,
            ..Settings::default()
        };
        let backup = source.path().join("backup.zip");
        storage.export(&library, &settings, &backup).unwrap();

        let target = tempfile::tempdir().unwrap();
        let (other, _) = with_pdf(target.path(), "9-b.pdf", b"%PDF-other");
        fs::write(other.cover_path("9-b.pdf"), b"png").unwrap();
        let restored = other.restore(&backup).unwrap();

        assert_eq!(restored.library, library);
        assert_eq!(restored.settings, Some(settings.clone()));
        assert_eq!(other.load_library(), library);
        assert_eq!(other.load_settings(), settings);
        assert_eq!(fs::read(other.pdf_path("1-a.pdf")).unwrap(), b"%PDF-old");
        assert!(!other.pdf_path("9-b.pdf").exists());
        assert!(!other.cover_path("9-b.pdf").exists());
        assert!(!target.path().join(RESTORE_DIR).exists());
    }

    #[test]
    fn other_files_are_no_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.zip");
        fs::write(&path, b"%PDF-1.7").unwrap();
        assert!(matches!(
            Storage::read_backup(&path),
            Err(BackupError::Invalid)
        ));
        zip_with(&path, &[("other.json", b"{}")]);
        assert!(matches!(
            Storage::read_backup(&path),
            Err(BackupError::Invalid)
        ));
    }

    #[test]
    fn backups_from_newer_versions_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.zip");
        let json = format!(
            r#"{{"version":{},"next_id":1,"projects":[]}}"#,
            LIBRARY_VERSION + 1
        );
        zip_with(&path, &[(LIBRARY_FILE, json.as_bytes())]);
        assert!(matches!(
            Storage::read_backup(&path),
            Err(BackupError::TooNew)
        ));
    }

    #[test]
    fn missing_or_unsafe_pdfs_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.zip");
        let mut library = Library::default();
        for file in ["missing.pdf", "../evil.pdf", "ok.pdf"] {
            let id = library.create(file, 1);
            library.get_mut(id).unwrap().pdf = Some(Pdf::new(file.into(), file.into()));
        }
        let json = serde_json::to_vec(&library).unwrap();
        zip_with(
            &path,
            &[
                (LIBRARY_FILE, &json),
                ("pdfs/../evil.pdf", b"%PDF"),
                ("pdfs/ok.pdf", b"%PDF"),
            ],
        );
        let backup = Storage::read_backup(&path).unwrap();
        let files: Vec<_> = backup
            .library
            .projects
            .iter()
            .map(|p| p.pdf.as_ref().map(|pdf| pdf.file.as_str()))
            .collect();
        assert_eq!(files, [None, None, Some("ok.pdf")]);
        assert_eq!(backup.settings, None);
    }

    #[test]
    fn backup_names_use_the_date() {
        assert_eq!(backup_name(0), "flausch-1970-01-01.zip");
        assert_eq!(backup_name(1_794_337_200), "flausch-2026-11-10.zip");
        assert_eq!(backup_name(951_782_400), "flausch-2000-02-29.zip");
    }

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
        let id = library.create("Mütze", 1);
        let part = library.add_part(id, "Krempe", Some(12), "", 2).unwrap();
        library.increment(id, part, 3);
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
        let settings = storage.load_settings();
        assert!(settings.keep_screen_on);
        assert_eq!(settings.line_color, LineColor::Magenta);
    }
}
