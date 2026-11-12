//! UI-independent data model. Must not depend on `slint`.

use std::cmp::Ordering;
use std::collections::BTreeMap;

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
    #[serde(default)]
    pub show_line: bool,
    /// Reading line per page (0-based) as a fraction (0.0–1.0) of the page height.
    #[serde(default)]
    pub lines: BTreeMap<u32, f32>,
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
            show_line: false,
            lines: BTreeMap::new(),
        }
    }

    pub fn set_view(&mut self, page: u32, zoom: f32, scroll_x: f32, scroll_y: f32) {
        self.page = page;
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.scroll_x = scroll_x.clamp(0.0, 1.0);
        self.scroll_y = scroll_y.clamp(0.0, 1.0);
    }

    pub fn line(&self, page: u32) -> Option<f32> {
        self.lines.get(&page).copied()
    }

    pub fn set_line(&mut self, page: u32, line: f32) {
        let line = if line.is_finite() {
            line.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.lines.insert(page, line);
    }
}

/// What the part dialog sets. A `quantity` of 0 counts as 1.
#[derive(Clone, Copy, Debug, Default)]
pub struct PartSpec<'a> {
    pub name: &'a str,
    pub row_goal: Option<u32>,
    pub quantity: u32,
}

impl<'a> PartSpec<'a> {
    pub fn new(name: &'a str, row_goal: Option<u32>) -> Self {
        Self {
            name,
            row_goal,
            ..Self::default()
        }
    }
}

fn one() -> u32 {
    1
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if"
)]
fn is_one(n: &u32) -> bool {
    *n == 1
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if"
)]
fn is_zero(n: &u32) -> bool {
    *n == 0
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
    /// How many times the part is made, e.g. 4 legs.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub quantity: u32,
    /// Pieces finished so far; `row` counts the piece after them.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pieces_done: u32,
}

impl Part {
    fn new(id: u64, spec: &PartSpec) -> Self {
        let mut part = Self {
            id,
            name: String::new(),
            row: 0,
            row_goal: None,
            quantity: 1,
            pieces_done: 0,
        };
        part.edit(spec);
        part
    }

    fn edit(&mut self, spec: &PartSpec) {
        self.name = clean_name(spec.name);
        self.row_goal = spec.row_goal.filter(|g| *g > 0);
        self.quantity = spec.quantity.max(1);
        self.pieces_done = self.pieces_done.min(self.quantity - 1);
    }

    /// Whether the current piece is done (its goal reached, or any row without a goal) and
    /// another one is left.
    pub fn can_start_next_piece(&self) -> bool {
        self.pieces_done + 1 < self.quantity && self.row >= self.row_goal.unwrap_or(1)
    }

    /// Rows done and rows planned for all pieces, if the part has a goal.
    pub fn progress(&self) -> Option<(u32, u32)> {
        self.row_goal.map(|goal| {
            (
                self.pieces_done * goal + self.row.min(goal),
                self.quantity * goal,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Planned,
    #[default]
    Active,
    Paused,
    Finished,
}

impl Status {
    /// Position in the project list: active first, then planned and paused, finished last.
    pub fn group(self) -> u8 {
        match self {
            Status::Active => 0,
            Status::Planned | Status::Paused => 1,
            Status::Finished => 2,
        }
    }
}

/// Which projects the list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusFilter {
    #[default]
    All,
    Active,
    Planned,
    Paused,
    Finished,
}

impl StatusFilter {
    pub fn allows(self, status: Status) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Active => status == Status::Active,
            StatusFilter::Planned => status == Status::Planned,
            StatusFilter::Paused => status == Status::Paused,
            StatusFilter::Finished => status == Status::Finished,
        }
    }
}

/// Order of the projects within each status group of the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectSort {
    /// Most recently used first.
    #[default]
    Recent,
    /// A to Z.
    Name,
    /// Most recently created first.
    Newest,
    /// Furthest along first; projects without a row goal last.
    Progress,
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
    #[serde(default)]
    pub status: Status,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
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
            .filter_map(Part::progress)
            .fold((0, 0), |(done, total), (d, g)| (done + d, total + g))
    }

    /// Case-insensitive search in the project and part names, and in the name of the status in
    /// the current language (from the start of a word, so "e" doesn't match every status).
    pub fn matches(&self, query: &str, status_name: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.name.to_lowercase().contains(&query)
            || self
                .parts
                .iter()
                .any(|p| p.name.to_lowercase().contains(&query))
            || status_name
                .to_lowercase()
                .split_whitespace()
                .any(|word| word.starts_with(query.as_str()))
            || status_name.to_lowercase().starts_with(&query)
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
                    let mut part = Part::new(id, &PartSpec::new(&name, row_goal));
                    part.row = row.unwrap_or(0);
                    project.parts.push(part);
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
            status: Status::default(),
            notes: String::new(),
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

    pub fn set_status(&mut self, id: u64, status: Status, now: u64) -> bool {
        self.touch(id, now).map(|p| p.status = status).is_some()
    }

    /// Keeps line breaks, but drops whitespace at the start and end.
    pub fn set_notes(&mut self, id: u64, notes: &str, now: u64) -> bool {
        self.touch(id, now)
            .map(|p| notes.trim().clone_into(&mut p.notes))
            .is_some()
    }

    pub fn remove(&mut self, id: u64) -> Option<Project> {
        let index = self.projects.iter().position(|p| p.id == id)?;
        Some(self.projects.remove(index))
    }

    /// Adds a part and makes it the active one.
    pub fn add_part(&mut self, project: u64, spec: &PartSpec, now: u64) -> Option<u64> {
        self.get(project)?;
        let id = self.next_id();
        let project = self.touch(project, now)?;
        project.parts.push(Part::new(id, spec));
        project.active_part = Some(id);
        Some(id)
    }

    pub fn edit_part(&mut self, project: u64, part: u64, spec: &PartSpec, now: u64) -> bool {
        self.touch(project, now)
            .and_then(|p| p.part_mut(part))
            .map(|p| p.edit(spec))
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
        f: impl FnOnce(&mut Part),
    ) -> Option<u32> {
        self.get(project)?.part(part)?;
        let project = self.touch(project, now)?;
        project.active_part = Some(part);
        let part = project.part_mut(part)?;
        f(part);
        Some(part.row)
    }

    pub fn increment(&mut self, project: u64, part: u64, now: u64) -> Option<u32> {
        self.count(project, part, now, |p| p.row = p.row.saturating_add(1))
    }

    /// One row back; at row 0 of a later piece, back to the last row of the piece before.
    pub fn decrement(&mut self, project: u64, part: u64, now: u64) -> Option<u32> {
        self.count(project, part, now, |p| {
            if p.row == 0 && p.pieces_done > 0 {
                p.pieces_done -= 1;
                p.row = p.row_goal.unwrap_or(0);
            } else {
                p.row = p.row.saturating_sub(1);
            }
        })
    }

    /// Starts over: row 0 of the first piece.
    pub fn reset(&mut self, project: u64, part: u64, now: u64) -> Option<u32> {
        self.count(project, part, now, |p| {
            p.row = 0;
            p.pieces_done = 0;
        })
    }

    /// Counts the current piece as done and starts the next one at row 0.
    pub fn next_piece(&mut self, project: u64, part: u64, now: u64) -> Option<u32> {
        if !self.get(project)?.part(part)?.can_start_next_piece() {
            return None;
        }
        self.count(project, part, now, |p| {
            p.pieces_done += 1;
            p.row = 0;
        })
    }

    /// Projects the filter allows, grouped by status (`Status::group`) and sorted within a group;
    /// ties go to the most recently used.
    pub fn sorted(&self, filter: StatusFilter, sort: ProjectSort) -> Vec<&Project> {
        let mut projects: Vec<_> = self
            .projects
            .iter()
            .filter(|p| filter.allows(p.status))
            .collect();
        projects.sort_by(|a, b| {
            let by = match sort {
                ProjectSort::Recent => Ordering::Equal,
                ProjectSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                ProjectSort::Newest => b.created.cmp(&a.created),
                ProjectSort::Progress => progress_key(b).cmp(&progress_key(a)),
            };
            (a.status.group().cmp(&b.status.group()))
                .then(by)
                .then(b.updated.cmp(&a.updated))
                .then(b.id.cmp(&a.id))
        });
        projects
    }
}

/// Progress in per mille, or `None` (sorted last) without any row goal.
fn progress_key(project: &Project) -> Option<u64> {
    let (done, total) = project.progress();
    (total > 0).then(|| u64::from(done) * 1000 / u64::from(total))
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

/// Color of the reading line in the PDF view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineColor {
    /// Stands out from black text, white paper and most photos.
    #[default]
    Magenta,
    Red,
    Orange,
    Blue,
    Green,
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
    #[serde(default)]
    pub line_color: LineColor,
    /// Volume up and down count rows while a counter is shown (Android).
    #[serde(default)]
    pub volume_keys: bool,
    #[serde(default)]
    pub project_filter: StatusFilter,
    #[serde(default)]
    pub project_sort: ProjectSort,
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
            line_color: LineColor::default(),
            volume_keys: false,
            project_filter: StatusFilter::default(),
            project_sort: ProjectSort::default(),
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
        assert_eq!(lib.get(b).unwrap().parts, []);
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
        let head = lib
            .add_part(id, &PartSpec::new("Kopf", Some(22)), 2)
            .unwrap();
        let legs = lib
            .add_part(id, &PartSpec::new("Beine", Some(0)), 3)
            .unwrap();
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
    fn status_and_notes_are_kept_and_default_for_old_data() {
        let old = r#"{"version":2,"next_id":2,"projects":[
            {"id":1,"name":"Schal","created":0,"updated":0}]}"#;
        let mut lib: Library = serde_json::from_str(old).unwrap();
        let project = lib.get(1).unwrap();
        assert_eq!(
            (project.status, project.notes.as_str()),
            (Status::Active, "")
        );

        assert!(lib.set_status(1, Status::Finished, 3));
        assert!(lib.set_notes(1, "  Nadel 4\nWolle: Merino \n", 4));
        let json = serde_json::to_string(&lib).unwrap();
        let lib: Library = serde_json::from_str(&json).unwrap();
        let project = lib.get(1).unwrap();
        assert_eq!(project.status, Status::Finished);
        assert_eq!(project.notes, "Nadel 4\nWolle: Merino");
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
        assert_eq!(lib.get(2).unwrap().parts, []);
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
        assert_eq!(lib.sorted(StatusFilter::All, ProjectSort::Recent)[0].id, b);
        lib.rename(a, "A2", 3);
        assert_eq!(lib.sorted(StatusFilter::All, ProjectSort::Recent)[0].id, a);
    }

    #[test]
    fn recent_groups_and_filters_by_status() {
        let mut lib = Library::default();
        let done = lib.create("Done", 1);
        let planned = lib.create("Planned", 2);
        let active = lib.create("Active", 3);
        let paused = lib.create("Paused", 4);
        let newest = lib.create("Newest", 5);
        lib.set_status(done, Status::Finished, 10);
        lib.set_status(planned, Status::Planned, 6);
        lib.set_status(paused, Status::Paused, 7);
        let ids = |filter| -> Vec<u64> {
            lib.sorted(filter, ProjectSort::Recent)
                .iter()
                .map(|p| p.id)
                .collect()
        };
        assert_eq!(
            ids(StatusFilter::All),
            [newest, active, paused, planned, done]
        );
        assert_eq!(ids(StatusFilter::Active), [newest, active]);
        assert_eq!(ids(StatusFilter::Planned), [planned]);
        assert_eq!(ids(StatusFilter::Paused), [paused]);
        assert_eq!(ids(StatusFilter::Finished), [done]);
    }

    #[test]
    fn sorted_orders_within_status_groups() {
        let mut lib = Library::default();
        let bunny = lib.create("bunny", 1);
        let hat = lib.create("Hat", 2);
        let scarf = lib.create("Scarf", 3);
        let blanket = lib.create("Blanket", 4);
        lib.add_part(bunny, &PartSpec::new("Head", Some(10)), 5);
        lib.add_part(hat, &PartSpec::new("Hat", Some(10)), 6);
        lib.add_part(scarf, &PartSpec::new("Scarf", None), 7);
        let head = lib.get(bunny).unwrap().parts[0].id;
        let crown = lib.get(hat).unwrap().parts[0].id;
        for _ in 0..3 {
            lib.increment(bunny, head, 8);
        }
        lib.increment(hat, crown, 9);
        lib.set_status(blanket, Status::Finished, 10);
        let ids = |sort| -> Vec<u64> {
            lib.sorted(StatusFilter::All, sort)
                .iter()
                .map(|p| p.id)
                .collect()
        };
        assert_eq!(ids(ProjectSort::Recent), [hat, bunny, scarf, blanket]);
        assert_eq!(ids(ProjectSort::Name), [bunny, hat, scarf, blanket]);
        assert_eq!(ids(ProjectSort::Newest), [scarf, hat, bunny, blanket]);
        assert_eq!(ids(ProjectSort::Progress), [bunny, hat, scarf, blanket]);
    }

    #[test]
    fn progress_sums_parts_with_a_goal() {
        let mut lib = Library::default();
        let id = lib.create("Hase", 1);
        assert_eq!(lib.get(id).unwrap().progress(), (0, 0));
        let head = lib
            .add_part(id, &PartSpec::new("Kopf", Some(20)), 2)
            .unwrap();
        let ears = lib
            .add_part(id, &PartSpec::new("Ohren", Some(10)), 3)
            .unwrap();
        let free = lib.add_part(id, &PartSpec::new("Deko", None), 4).unwrap();
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
    fn parts_made_several_times() {
        let mut lib = Library::default();
        let id = lib.create("Hase", 1);
        let legs = PartSpec {
            quantity: 4,
            ..PartSpec::new("Bein", Some(3))
        };
        let leg = lib.add_part(id, &legs, 2).unwrap();
        let part = |lib: &Library| lib.get(id).unwrap().part(leg).unwrap().clone();
        assert_eq!(lib.get(id).unwrap().progress(), (0, 12));
        assert_eq!(
            lib.next_piece(id, leg, 3),
            None,
            "the goal isn't reached yet"
        );

        for _ in 0..4 {
            lib.increment(id, leg, 4);
        }
        assert_eq!(
            lib.get(id).unwrap().progress(),
            (3, 12),
            "extra rows don't count"
        );
        assert_eq!(lib.next_piece(id, leg, 5), Some(0));
        lib.increment(id, leg, 6);
        assert_eq!((part(&lib).pieces_done, part(&lib).row), (1, 1));
        assert_eq!(lib.get(id).unwrap().progress(), (4, 12));

        lib.decrement(id, leg, 7);
        assert_eq!(
            lib.decrement(id, leg, 8),
            Some(3),
            "back to the last row of leg 1"
        );
        assert_eq!(part(&lib).pieces_done, 0);

        for _ in 0..3 {
            lib.next_piece(id, leg, 9);
            for _ in 0..3 {
                lib.increment(id, leg, 9);
            }
        }
        assert_eq!(lib.get(id).unwrap().progress(), (12, 12));
        assert_eq!(lib.next_piece(id, leg, 10), None, "no fifth leg");

        let fewer = PartSpec {
            quantity: 2,
            ..legs
        };
        lib.edit_part(id, leg, &fewer, 11);
        assert_eq!(part(&lib).pieces_done, 1);
        lib.reset(id, leg, 12);
        assert_eq!((part(&lib).pieces_done, part(&lib).row), (0, 0));
    }

    #[test]
    fn without_a_goal_any_row_finishes_a_piece() {
        let mut lib = Library::default();
        let id = lib.create("Hase", 1);
        let spec = PartSpec {
            quantity: 2,
            ..PartSpec::new("Ohr", None)
        };
        let ear = lib.add_part(id, &spec, 2).unwrap();
        assert_eq!(lib.next_piece(id, ear, 3), None);
        lib.increment(id, ear, 4);
        assert_eq!(lib.next_piece(id, ear, 5), Some(0));
        assert_eq!(lib.decrement(id, ear, 6), Some(0));
        assert_eq!(lib.get(id).unwrap().part(ear).unwrap().pieces_done, 0);
    }

    #[test]
    fn old_parts_are_made_once() {
        // The second counter of earlier test builds is dropped.
        let part: Part = serde_json::from_str(
            r#"{"id": 1, "name": "Kopf", "row": 3, "counter": {"name": "Rapport", "count": 2}}"#,
        )
        .unwrap();
        assert_eq!((part.quantity, part.pieces_done), (1, 0));
        let json = serde_json::to_string(&part).unwrap();
        assert!(
            !json.contains("quantity") && !json.contains("counter"),
            "{json}"
        );
    }

    #[test]
    fn matches_project_and_part_names() {
        let mut lib = Library::default();
        let id = lib.create("Hase Hoppel", 1);
        lib.add_part(id, &PartSpec::new("Kopf", None), 2);
        let project = lib.get(id).unwrap();
        assert!(project.matches("", ""));
        assert!(project.matches(" hoppel ", ""));
        assert!(project.matches("KOPF", ""));
        assert!(!project.matches("Schal", ""));
    }

    #[test]
    fn matches_the_status_name_from_the_start_of_a_word() {
        let mut lib = Library::default();
        let id = lib.create("Mütze", 1);
        let project = lib.get(id).unwrap();
        assert!(project.matches("fertig", "Fertig"));
        assert!(project.matches("arbeit", "In Arbeit"));
        assert!(project.matches("in arb", "In Arbeit"));
        assert!(!project.matches("ertig", "Fertig"));
        assert!(!project.matches("fertig", "In Arbeit"));
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
    fn reading_line_is_clamped() {
        let mut pdf = Pdf::new("1.pdf".into(), "x.pdf".into());
        pdf.set_line(2, 1.5);
        assert_eq!(pdf.line(2), Some(1.0));
        pdf.set_line(2, f32::NAN);
        assert_eq!(pdf.line(2), Some(0.0));
        assert_eq!(pdf.line(0), None);
    }

    #[test]
    fn reading_line_is_kept_per_page() {
        let mut pdf = Pdf::new("1.pdf".into(), "x.pdf".into());
        pdf.set_line(0, 0.25);
        pdf.set_line(1, 0.75);
        let json = serde_json::to_string(&pdf).unwrap();
        let pdf: Pdf = serde_json::from_str(&json).unwrap();
        assert_eq!((pdf.line(0), pdf.line(1)), (Some(0.25), Some(0.75)));
    }

    #[test]
    fn system_language_falls_back_to_english() {
        assert_eq!(Language::System.translation(Some("de-AT")), "de");
        assert_eq!(Language::System.translation(Some("fr_FR")), "en");
        assert_eq!(Language::System.translation(None), "en");
        assert_eq!(Language::De.translation(Some("en-US")), "de");
    }
}
