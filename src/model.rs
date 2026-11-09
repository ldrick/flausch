//! UI-independent data model. Must not depend on `slint`.

use serde::{Deserialize, Serialize};

pub const LIBRARY_VERSION: u32 = 1;
pub const SETTINGS_VERSION: u32 = 1;

pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 4.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pdf {
    /// File name inside the PDF directory of the data dir.
    pub file: String,
    /// Original file name, shown to the user.
    pub title: String,
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    /// Scroll position as a fraction (0.0–1.0) of the scrollable range.
    #[serde(default)]
    pub scroll_x: f32,
    #[serde(default)]
    pub scroll_y: f32,
}

fn default_zoom() -> f32 {
    MIN_ZOOM
}

impl Pdf {
    pub fn new(file: String, title: String) -> Self {
        Self {
            file,
            title,
            page: 0,
            zoom: MIN_ZOOM,
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }

    pub fn set_view(&mut self, page: u32, zoom: f32, scroll_x: f32, scroll_y: f32) {
        self.page = page;
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.scroll_x = scroll_x.clamp(0.0, 1.0);
        self.scroll_y = scroll_y.clamp(0.0, 1.0);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub row: u32,
    /// Target number of rows, if any.
    #[serde(default)]
    pub row_goal: Option<u32>,
    #[serde(default)]
    pub pdf: Option<Pdf>,
    /// Unix timestamps in seconds.
    pub created: u64,
    pub updated: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub next_id: u64,
    pub projects: Vec<Project>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            version: LIBRARY_VERSION,
            next_id: 1,
            projects: Vec::new(),
        }
    }
}

impl Library {
    pub fn create(&mut self, name: &str, row_goal: Option<u32>, now: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.projects.push(Project {
            id,
            name: clean_name(name),
            row: 0,
            row_goal: row_goal.filter(|g| *g > 0),
            pdf: None,
            created: now,
            updated: now,
        });
        id
    }

    pub fn get(&self, id: u64) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// Mutable access without bumping `updated`, e.g. for the PDF scroll position.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.id == id)
    }

    /// Mutable access that also bumps `updated`.
    pub fn touch(&mut self, id: u64, now: u64) -> Option<&mut Project> {
        let project = self.projects.iter_mut().find(|p| p.id == id)?;
        project.updated = now;
        Some(project)
    }

    pub fn edit(&mut self, id: u64, name: &str, row_goal: Option<u32>, now: u64) -> bool {
        self.touch(id, now)
            .map(|p| {
                p.name = clean_name(name);
                p.row_goal = row_goal.filter(|g| *g > 0);
            })
            .is_some()
    }

    pub fn remove(&mut self, id: u64) -> Option<Project> {
        let index = self.projects.iter().position(|p| p.id == id)?;
        Some(self.projects.remove(index))
    }

    pub fn increment(&mut self, id: u64, now: u64) -> Option<u32> {
        self.touch(id, now).map(|p| {
            p.row = p.row.saturating_add(1);
            p.row
        })
    }

    pub fn decrement(&mut self, id: u64, now: u64) -> Option<u32> {
        self.touch(id, now).map(|p| {
            p.row = p.row.saturating_sub(1);
            p.row
        })
    }

    pub fn reset(&mut self, id: u64, now: u64) -> bool {
        self.touch(id, now).map(|p| p.row = 0).is_some()
    }

    /// Projects ordered by most recently used first.
    pub fn recent(&self) -> Vec<&Project> {
        let mut projects: Vec<_> = self.projects.iter().collect();
        projects.sort_by(|a, b| b.updated.cmp(&a.updated).then(b.id.cmp(&a.id)));
        projects
    }
}

fn clean_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    System,
    De,
    En,
}

impl Language {
    /// Bundled translation to select; "en" is the untranslated source language.
    pub fn translation(self, system_locale: Option<&str>) -> &'static str {
        match self {
            Language::De => "de",
            Language::En => "en",
            Language::System => match system_locale {
                Some(locale) if locale.to_ascii_lowercase().starts_with("de") => "de",
                _ => "en",
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default = "default_true")]
    pub keep_screen_on: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            language: Language::default(),
            appearance: Appearance::default(),
            keep_screen_on: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_assigns_unique_ids_and_cleans_name() {
        let mut lib = Library::default();
        let a = lib.create("  Mütze   blau ", None, 10);
        let b = lib.create("Schal", Some(0), 11);
        assert_ne!(a, b);
        assert_eq!(lib.get(a).unwrap().name, "Mütze blau");
        assert_eq!(lib.get(b).unwrap().row_goal, None);
    }

    #[test]
    fn ids_are_not_reused_after_remove() {
        let mut lib = Library::default();
        let a = lib.create("A", None, 0);
        lib.remove(a);
        assert_ne!(lib.create("B", None, 0), a);
    }

    #[test]
    fn counter_saturates_at_zero_and_updates_timestamp() {
        let mut lib = Library::default();
        let id = lib.create("A", None, 1);
        assert_eq!(lib.decrement(id, 2), Some(0));
        assert_eq!(lib.increment(id, 3), Some(1));
        assert_eq!(lib.increment(id, 4), Some(2));
        assert_eq!(lib.get(id).unwrap().updated, 4);
        assert!(lib.reset(id, 5));
        assert_eq!(lib.get(id).unwrap().row, 0);
        assert_eq!(lib.increment(999, 6), None);
    }

    #[test]
    fn recent_orders_by_last_use() {
        let mut lib = Library::default();
        let a = lib.create("A", None, 1);
        let b = lib.create("B", None, 2);
        assert_eq!(lib.recent()[0].id, b);
        lib.increment(a, 3);
        assert_eq!(lib.recent()[0].id, a);
    }

    #[test]
    fn pdf_view_is_clamped() {
        let mut pdf = Pdf::new("1.pdf".into(), "x.pdf".into());
        pdf.set_view(3, 10.0, -1.0, 2.0);
        assert_eq!(
            (pdf.page, pdf.zoom, pdf.scroll_x, pdf.scroll_y),
            (3, MAX_ZOOM, 0.0, 1.0)
        );
    }

    #[test]
    fn system_language_falls_back_to_english() {
        assert_eq!(Language::System.translation(Some("de-AT")), "de");
        assert_eq!(Language::System.translation(Some("fr_FR")), "en");
        assert_eq!(Language::System.translation(None), "en");
        assert_eq!(Language::De.translation(Some("en-US")), "de");
    }
}
