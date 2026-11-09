//! UI-independent data model. Must not depend on `slint`.

use serde::{Deserialize, Serialize};

pub const LIBRARY_VERSION: u32 = 2;
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

/// A piece of a project with its own row counter, e.g. the head of a toy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub row: u32,
    /// Target number of rows, if any.
    #[serde(default)]
    pub row_goal: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub parts: Vec<Part>,
    /// The part opened last; the PDF view counts its rows.
    #[serde(default)]
    pub active_part: Option<u64>,
    #[serde(default)]
    pub pdf: Option<Pdf>,
    /// Unix timestamps in seconds.
    pub created: u64,
    pub updated: u64,
    /// Version 1 counted rows per project. Only read for the migration.
    #[serde(default, skip_serializing, rename = "row")]
    legacy_row: Option<u32>,
    #[serde(default, skip_serializing, rename = "row_goal")]
    legacy_row_goal: Option<u32>,
}

impl Project {
    pub fn part(&self, id: u64) -> Option<&Part> {
        self.parts.iter().find(|p| p.id == id)
    }

    fn part_mut(&mut self, id: u64) -> Option<&mut Part> {
        self.parts.iter_mut().find(|p| p.id == id)
    }

    pub fn active(&self) -> Option<&Part> {
        self.active_part.and_then(|id| self.part(id))
    }

    /// Rows done and rows planned over all parts with a goal; rows beyond a goal don't count.
    pub fn progress(&self) -> (u32, u32) {
        self.parts
            .iter()
            .filter_map(|p| p.row_goal.map(|goal| (p.row.min(goal), goal)))
            .fold((0, 0), |(done, total), (d, g)| (done + d, total + g))
    }

    /// Case-insensitive search in the project and part names.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.name.to_lowercase().contains(&query)
            || self
                .parts
                .iter()
                .any(|p| p.name.to_lowercase().contains(&query))
    }
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
    /// Brings data from older versions up to `LIBRARY_VERSION`.
    pub fn migrate(&mut self) {
        if self.version < 2 {
            for i in 0..self.projects.len() {
                let project = &mut self.projects[i];
                let (row, row_goal) = (project.legacy_row.take(), project.legacy_row_goal.take());
                if project.parts.is_empty() && (row.unwrap_or(0) > 0 || row_goal.is_some()) {
                    let name = project.name.clone();
                    let id = self.next_id();
                    let project = &mut self.projects[i];
                    project.parts.push(Part {
                        id,
                        name,
                        row: row.unwrap_or(0),
                        row_goal: row_goal.filter(|g| *g > 0),
                    });
                    project.active_part = Some(id);
                }
            }
        }
        self.version = LIBRARY_VERSION;
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn create(&mut self, name: &str, now: u64) -> u64 {
        let id = self.next_id();
        self.projects.push(Project {
            id,
            name: clean_name(name),
            parts: Vec::new(),
            active_part: None,
            pdf: None,
            created: now,
            updated: now,
            legacy_row: None,
            legacy_row_goal: None,
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

    pub fn rename(&mut self, id: u64, name: &str, now: u64) -> bool {
        self.touch(id, now)
            .map(|p| p.name = clean_name(name))
            .is_some()
    }

    pub fn remove(&mut self, id: u64) -> Option<Project> {
        let index = self.projects.iter().position(|p| p.id == id)?;
        Some(self.projects.remove(index))
    }

    /// Adds a part and makes it the active one.
    pub fn add_part(
        &mut self,
        project: u64,
        name: &str,
        row_goal: Option<u32>,
        now: u64,
    ) -> Option<u64> {
        self.get(project)?;
        let id = self.next_id();
        let project = self.touch(project, now)?;
        project.parts.push(Part {
            id,
            name: clean_name(name),
            row: 0,
            row_goal: row_goal.filter(|g| *g > 0),
        });
        project.active_part = Some(id);
        Some(id)
    }

    pub fn edit_part(
        &mut self,
        project: u64,
        part: u64,
        name: &str,
        row_goal: Option<u32>,
        now: u64,
    ) -> bool {
        self.touch(project, now)
            .and_then(|p| p.part_mut(part))
            .map(|p| {
                p.name = clean_name(name);
                p.row_goal = row_goal.filter(|g| *g > 0);
            })
            .is_some()
    }

    pub fn remove_part(&mut self, project: u64, part: u64, now: u64) -> Option<Part> {
        let project = self.touch(project, now)?;
        let index = project.parts.iter().position(|p| p.id == part)?;
        if project.active_part == Some(part) {
            project.active_part = None;
        }
        Some(project.parts.remove(index))
    }

    pub fn activate_part(&mut self, project: u64, part: u64) -> bool {
        match self.get_mut(project) {
            Some(p) if p.part(part).is_some() => {
                p.active_part = Some(part);
                true
            }
            _ => false,
        }
    }

    fn count(
        &mut self,
        project: u64,
        part: u64,
        now: u64,
        f: impl FnOnce(u32) -> u32,
    ) -> Option<u32> {
        self.get(project)?.part(part)?;
        let project = self.touch(project, now)?;
        project.active_part = Some(part);
        let part = project.part_mut(part)?;
        part.row = f(part.row);
        Some(part.row)
    }

    pub fn increment(&mut self, project: u64, part: u64, now: u64) -> Option<u32> {
        self.count(project, part, now, |r| r.saturating_add(1))
    }

    pub fn decrement(&mut self, project: u64, part: u64, now: u64) -> Option<u32> {
        self.count(project, part, now, |r| r.saturating_sub(1))
    }

    pub fn reset(&mut self, project: u64, part: u64, now: u64) -> Option<u32> {
        self.count(project, part, now, |_| 0)
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
        let a = lib.create("  Mütze   blau ", 10);
        let b = lib.create("Schal", 11);
        assert_ne!(a, b);
        assert_eq!(lib.get(a).unwrap().name, "Mütze blau");
        assert!(lib.get(b).unwrap().parts.is_empty());
    }

    #[test]
    fn ids_are_not_reused_after_remove() {
        let mut lib = Library::default();
        let a = lib.create("A", 0);
        lib.remove(a);
        assert_ne!(lib.create("B", 0), a);
    }

    #[test]
    fn parts_count_separately() {
        let mut lib = Library::default();
        let id = lib.create("Hase", 1);
        let head = lib.add_part(id, "Kopf", Some(22), 2).unwrap();
        let legs = lib.add_part(id, "Beine", Some(0), 3).unwrap();
        assert_eq!(lib.get(id).unwrap().active_part, Some(legs));
        assert_eq!(lib.get(id).unwrap().part(legs).unwrap().row_goal, None);

        assert_eq!(lib.decrement(id, head, 4), Some(0));
        assert_eq!(lib.increment(id, head, 5), Some(1));
        assert_eq!(lib.increment(id, head, 6), Some(2));
        assert_eq!(lib.increment(id, legs, 7), Some(1));
        let project = lib.get(id).unwrap();
        assert_eq!(project.part(head).unwrap().row, 2);
        assert_eq!((project.active_part, project.updated), (Some(legs), 7));

        assert_eq!(lib.reset(id, head, 8), Some(0));
        assert_eq!(lib.increment(id, 999, 9), None);
        assert_eq!(lib.get(id).unwrap().active_part, Some(head));
        assert!(lib.remove_part(id, legs, 10).is_some());
        assert_eq!(lib.get(id).unwrap().active_part, Some(head));
        assert!(lib.remove_part(id, head, 11).is_some());
        assert_eq!(lib.get(id).unwrap().active_part, None);
    }

    #[test]
    fn migrates_project_rows_into_a_part() {
        let v1 = r#"{"version":1,"next_id":3,"projects":[
            {"id":1,"name":"Decke","row":7,"row_goal":40,"created":0,"updated":0},
            {"id":2,"name":"Neu","row":0,"created":0,"updated":0}]}"#;
        let mut lib: Library = serde_json::from_str(v1).unwrap();
        lib.migrate();
        assert_eq!(lib.version, LIBRARY_VERSION);
        let part = lib.get(1).unwrap().active().unwrap().clone();
        assert_eq!(
            (part.name.as_str(), part.row, part.row_goal),
            ("Decke", 7, Some(40))
        );
        assert_eq!(part.id, 3);
        assert!(lib.get(2).unwrap().parts.is_empty());
        // Saved data has no legacy fields left, so loading it again doesn't add a second part.
        let mut again: Library =
            serde_json::from_str(&serde_json::to_string(&lib).unwrap()).unwrap();
        again.migrate();
        assert_eq!(again, lib);
    }

    #[test]
    fn recent_orders_by_last_use() {
        let mut lib = Library::default();
        let a = lib.create("A", 1);
        let b = lib.create("B", 2);
        assert_eq!(lib.recent()[0].id, b);
        lib.rename(a, "A2", 3);
        assert_eq!(lib.recent()[0].id, a);
    }

    #[test]
    fn progress_sums_parts_with_a_goal() {
        let mut lib = Library::default();
        let id = lib.create("Hase", 1);
        assert_eq!(lib.get(id).unwrap().progress(), (0, 0));
        let head = lib.add_part(id, "Kopf", Some(20), 2).unwrap();
        let ears = lib.add_part(id, "Ohren", Some(10), 3).unwrap();
        let free = lib.add_part(id, "Deko", None, 4).unwrap();
        for _ in 0..5 {
            lib.increment(id, head, 5);
        }
        for _ in 0..12 {
            lib.increment(id, ears, 6);
        }
        lib.increment(id, free, 7);
        assert_eq!(lib.get(id).unwrap().progress(), (15, 30));
    }

    #[test]
    fn matches_project_and_part_names() {
        let mut lib = Library::default();
        let id = lib.create("Hase Hoppel", 1);
        lib.add_part(id, "Kopf", None, 2);
        let project = lib.get(id).unwrap();
        assert!(project.matches(""));
        assert!(project.matches(" hoppel "));
        assert!(project.matches("KOPF"));
        assert!(!project.matches("Schal"));
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
