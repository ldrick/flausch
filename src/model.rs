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
    /// Arranged by hand (`Project::position`).
    Custom,
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
    /// Place in the custom order (`ProjectSort::Custom`), smallest first.
    #[serde(default)]
    pub position: i64,
    /// The folder it is in; `None` is the top level of the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<u64>,
    /// Yarns from the stash, in the order they were added. Their skeins are taken out of the stash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yarns: Vec<ProjectYarn>,
    /// Hooks (ids), in the order they were added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<u64>,
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

/// A yarn used in a project: full `skeins` and part of one (`opened`, in the yarn's units: grams or
/// tenths of a skein) came out of the stash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectYarn {
    pub yarn: u64,
    pub skeins: u32,
    /// Development builds called it `grams`.
    #[serde(default, skip_serializing_if = "is_zero", alias = "grams")]
    pub opened: u32,
}

impl ProjectYarn {
    /// All of it in units, with the units per skein.
    pub fn units(&self, unit: u32) -> u32 {
        self.skeins.saturating_mul(unit).saturating_add(self.opened)
    }

    fn set_units(&mut self, units: u32, unit: u32) {
        self.skeins = units / unit;
        self.opened = units % unit;
    }
}

/// What the yarn form sets. Text is trimmed; empty fields and zeros mean unknown.
#[derive(Clone, Copy, Debug, Default)]
pub struct YarnSpec<'a> {
    pub brand: &'a str,
    pub name: &'a str,
    pub color: &'a str,
    pub dye_lot: &'a str,
    pub fiber: &'a str,
    /// Grams per skein.
    pub weight: Option<u32>,
    /// Meters per skein.
    pub length: Option<u32>,
    /// What is in the stash, in units of this spec's weight (`units_per_skein`).
    pub amount: u32,
}

/// Yarn is counted in grams if the weight of a skein is known, else in tenths of a skein. Returns how
/// many of these units make a skein.
pub fn units_per_skein(weight: Option<u32>) -> u32 {
    weight.filter(|w| *w > 0).unwrap_or(10)
}

/// Converts an amount of yarn when the units change, e.g. when the weight becomes known.
fn convert_units(units: u32, from: u32, to: u32) -> u32 {
    if from == to {
        return units;
    }
    let units = (u64::from(units) * u64::from(to) + u64::from(from) / 2) / u64::from(from);
    u32::try_from(units).unwrap_or(u32::MAX)
}

/// A yarn in the stash: one color (and dye lot) of a yarn line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Yarn {
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub brand: String,
    /// The yarn line, e.g. "Catania".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Color name or number.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dye_lot: String,
    /// Fiber content, e.g. "100% cotton".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fiber: String,
    /// Grams per skein.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    /// Meters per skein.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    /// Full skeins in the stash.
    #[serde(default)]
    pub quantity: u32,
    /// What is left of an opened skein, in units (`units_per_skein`): grams, or tenths of a skein.
    /// Development builds called it `rest` (grams).
    #[serde(default, skip_serializing_if = "is_zero", alias = "rest")]
    pub opened: u32,
    /// Unix timestamp in seconds.
    #[serde(default)]
    pub created: u64,
}

impl Yarn {
    /// Units (grams or tenths) per skein.
    pub fn unit(&self) -> u32 {
        units_per_skein(self.weight)
    }

    /// What is in the stash, in units.
    pub fn units(&self) -> u32 {
        self.quantity
            .saturating_mul(self.unit())
            .saturating_add(self.opened)
    }

    fn set_units(&mut self, units: u32) {
        let unit = self.unit();
        self.quantity = units / unit;
        self.opened = units % unit;
    }

    /// Grams in the stash, if the weight of a skein is known.
    pub fn grams(&self) -> Option<u32> {
        self.weight.map(|_| self.units())
    }

    /// Nothing left in the stash.
    pub fn used_up(&self) -> bool {
        self.units() == 0
    }

    fn edit(&mut self, spec: &YarnSpec) {
        self.brand = clean_name(spec.brand);
        self.name = clean_name(spec.name);
        self.color = clean_name(spec.color);
        self.dye_lot = clean_name(spec.dye_lot);
        self.fiber = clean_name(spec.fiber);
        self.weight = spec.weight.filter(|w| *w > 0);
        self.length = spec.length.filter(|l| *l > 0);
        self.set_units(spec.amount);
    }

    /// Brand and yarn line, e.g. "Schachenmayr Catania".
    pub fn title(&self) -> String {
        [self.brand.as_str(), self.name.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What the hook form sets. Text is trimmed.
#[derive(Clone, Copy, Debug, Default)]
pub struct HookSpec<'a> {
    /// Hundredths of a millimeter; the form needs one.
    pub size: Option<u32>,
    pub brand: &'a str,
    /// The kind of hook, e.g. "ergonomic" or "Tunisian".
    pub kind: &'a str,
}

/// A crochet hook.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hook {
    pub id: u64,
    /// Hundredths of a millimeter, e.g. 350 for 3.5 mm.
    pub size: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub brand: String,
    /// The kind of hook, e.g. "ergonomic" or "Tunisian".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// Unix timestamp in seconds.
    #[serde(default)]
    pub created: u64,
}

/// The largest hook size the form takes: 50 mm.
pub const MAX_HOOK_SIZE: u32 = 5000;

/// US size of a hook: the letter and number, and whether it is a steel hook (thread crochet),
/// whose numbers count the other way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsHookSize {
    pub name: &'static str,
    pub steel: bool,
}

/// US sizes by millimeters (hundredths), after the Craft Yarn Council. Sizes in between, e.g.
/// 2.5 or 3 mm, have no US size.
const US_HOOK_SIZES: &[(u32, &str, bool)] = &[
    (75, "14", true),
    (85, "13", true),
    (100, "12", true),
    (110, "11", true),
    (130, "10", true),
    (140, "9", true),
    (150, "8", true),
    (165, "7", true),
    (180, "6", true),
    (190, "5", true),
    (200, "4", true),
    (210, "3", true),
    (225, "B-1", false),
    (275, "C-2", false),
    (325, "D-3", false),
    (350, "E-4", false),
    (375, "F-5", false),
    (400, "G-6", false),
    (450, "7", false),
    (500, "H-8", false),
    (550, "I-9", false),
    (600, "J-10", false),
    (650, "K-10½", false),
    (800, "L-11", false),
    (900, "M/N-13", false),
    (1000, "N/P-15", false),
    (1500, "P/Q", false),
    (1575, "Q", false),
    (1900, "S", false),
];

impl Hook {
    fn edit(&mut self, spec: &HookSpec) {
        if let Some(size) = valid_hook_size(spec.size) {
            self.size = size;
        }
        self.brand = clean_name(spec.brand);
        self.kind = clean_name(spec.kind);
    }

    /// Brand and kind, e.g. "Clover Amour"; empty if neither is known.
    pub fn title(&self) -> String {
        [self.brand.as_str(), self.kind.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The US size of a hook of `size` hundredths of a millimeter, if it has one.
pub fn us_hook_size(size: u32) -> Option<UsHookSize> {
    US_HOOK_SIZES
        .iter()
        .find(|(mm, _, _)| *mm == size)
        .map(|&(_, name, steel)| UsHookSize { name, steel })
}

/// The size if the form takes it: more than 0, up to `MAX_HOOK_SIZE`.
pub fn valid_hook_size(size: Option<u32>) -> Option<u32> {
    size.filter(|s| (1..=MAX_HOOK_SIZE).contains(s))
}

/// A folder on the project list. Folders don't nest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: u64,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub next_id: u64,
    pub projects: Vec<Project>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<Folder>,
    /// The yarn stash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yarns: Vec<Yarn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Hook>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            version: LIBRARY_VERSION,
            next_id: 1,
            projects: Vec::new(),
            folders: Vec::new(),
            yarns: Vec::new(),
            hooks: Vec::new(),
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
        // Projects in a folder that is gone are back at the top level.
        for i in 0..self.projects.len() {
            if let Some(folder) = self.projects[i].folder
                && self.folder(folder).is_none()
            {
                self.projects[i].folder = None;
            }
        }
        // Links to yarns and hooks that are gone.
        let yarns: Vec<u64> = self.yarns.iter().map(|y| y.id).collect();
        let hooks: Vec<u64> = self.hooks.iter().map(|h| h.id).collect();
        for project in &mut self.projects {
            project.yarns.retain(|y| yarns.contains(&y.yarn));
            project.hooks.retain(|h| hooks.contains(h));
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
        // New projects go to the top of the custom order.
        let position = self.projects.iter().map(|p| p.position).min().unwrap_or(1) - 1;
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
            position,
            folder: None,
            yarns: Vec::new(),
            hooks: Vec::new(),
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

    pub fn folder(&self, id: u64) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    /// Folders by name.
    pub fn sorted_folders(&self) -> Vec<&Folder> {
        let mut folders: Vec<_> = self.folders.iter().collect();
        folders.sort_by_cached_key(|f| (f.name.to_lowercase(), f.id));
        folders
    }

    pub fn create_folder(&mut self, name: &str) -> u64 {
        let id = self.next_id();
        self.folders.push(Folder {
            id,
            name: clean_name(name),
        });
        id
    }

    pub fn rename_folder(&mut self, id: u64, name: &str) -> bool {
        self.folders
            .iter_mut()
            .find(|f| f.id == id)
            .map(|f| f.name = clean_name(name))
            .is_some()
    }

    /// Removes a folder; its projects go back to the top level.
    pub fn remove_folder(&mut self, id: u64) -> Option<Folder> {
        let index = self.folders.iter().position(|f| f.id == id)?;
        for project in &mut self.projects {
            if project.folder == Some(id) {
                project.folder = None;
            }
        }
        Some(self.folders.remove(index))
    }

    /// Puts a project into a folder, or back to the top level with `None`.
    pub fn move_to_folder(&mut self, project: u64, folder: Option<u64>) -> bool {
        if folder.is_some_and(|f| self.folder(f).is_none()) {
            return false;
        }
        self.get_mut(project).map(|p| p.folder = folder).is_some()
    }

    /// Number of projects in a folder.
    pub fn folder_size(&self, id: u64) -> usize {
        self.projects
            .iter()
            .filter(|p| p.folder == Some(id))
            .count()
    }

    #[cfg(test)]
    pub fn yarn(&self, id: u64) -> Option<&Yarn> {
        self.yarns.iter().find(|y| y.id == id)
    }

    /// Adds a yarn to the stash unless brand, name and color are all empty.
    pub fn add_yarn(&mut self, spec: &YarnSpec, now: u64) -> Option<u64> {
        if !has_yarn_name(spec) {
            return None;
        }
        let id = self.next_id();
        let mut yarn = Yarn {
            id,
            brand: String::new(),
            name: String::new(),
            color: String::new(),
            dye_lot: String::new(),
            fiber: String::new(),
            weight: None,
            length: None,
            quantity: 0,
            opened: 0,
            created: now,
        };
        yarn.edit(spec);
        self.yarns.push(yarn);
        Some(id)
    }

    /// Changes a yarn; like `add_yarn`, it needs a brand, name or color.
    pub fn edit_yarn(&mut self, id: u64, spec: &YarnSpec) -> bool {
        if !has_yarn_name(spec) {
            return false;
        }
        let Some(yarn) = self.yarns.iter_mut().find(|y| y.id == id) else {
            return false;
        };
        let (from, to) = (yarn.unit(), units_per_skein(spec.weight));
        yarn.edit(spec);
        // With a new weight, what projects used changes its units, too.
        for project in &mut self.projects {
            for used in project.yarns.iter_mut().filter(|u| u.yarn == id) {
                let units = convert_units(used.units(from), from, to);
                used.set_units(units, to);
            }
        }
        true
    }

    /// Removes a yarn from the stash and from the projects that use it.
    pub fn remove_yarn(&mut self, id: u64) -> Option<Yarn> {
        let index = self.yarns.iter().position(|y| y.id == id)?;
        for project in &mut self.projects {
            project.yarns.retain(|y| y.yarn != id);
        }
        Some(self.yarns.remove(index))
    }

    /// Sets how much of a stash yarn a project uses, in the yarn's units (`Yarn::unit`), taking it out of
    /// the stash or putting it back. It can't take more than the stash has; 0 takes the yarn off the
    /// project. Returns what the project uses now, or `None` if the project or the yarn doesn't exist.
    pub fn set_project_yarn(
        &mut self,
        project: u64,
        yarn: u64,
        units: u32,
        now: u64,
    ) -> Option<ProjectYarn> {
        let stash = self.yarns.iter().find(|y| y.id == yarn)?;
        let (unit, in_stash) = (stash.unit(), stash.units());
        let used = self
            .get(project)?
            .yarns
            .iter()
            .find(|y| y.yarn == yarn)
            .map_or(0, |y| y.units(unit));
        let available = in_stash.saturating_add(used);
        let units = units.min(available);
        let mut new = ProjectYarn {
            yarn,
            skeins: 0,
            opened: 0,
        };
        new.set_units(units, unit);

        self.yarns
            .iter_mut()
            .find(|y| y.id == yarn)?
            .set_units(available - units);
        let project = self.touch(project, now)?;
        let index = project.yarns.iter().position(|y| y.yarn == yarn);
        match index {
            Some(i) if units == 0 => {
                project.yarns.remove(i);
            }
            Some(i) => project.yarns[i] = new,
            None if units > 0 => project.yarns.push(new),
            None => {}
        }
        Some(new)
    }

    /// The projects that use a yarn, by name.
    pub fn projects_with_yarn(&self, yarn: u64) -> Vec<&Project> {
        let mut projects: Vec<_> = self
            .projects
            .iter()
            .filter(|p| p.yarns.iter().any(|y| y.yarn == yarn))
            .collect();
        projects.sort_by_cached_key(|p| p.name.to_lowercase());
        projects
    }

    /// The stash by brand, yarn line and color.
    pub fn sorted_yarns(&self) -> Vec<&Yarn> {
        let mut yarns: Vec<_> = self.yarns.iter().collect();
        yarns.sort_by_cached_key(|y| {
            (
                y.brand.to_lowercase(),
                y.name.to_lowercase(),
                y.color.to_lowercase(),
                y.id,
            )
        });
        yarns
    }

    #[cfg(test)]
    pub fn hook(&self, id: u64) -> Option<&Hook> {
        self.hooks.iter().find(|h| h.id == id)
    }

    /// Adds a hook unless its size is missing or out of range.
    pub fn add_hook(&mut self, spec: &HookSpec, now: u64) -> Option<u64> {
        let size = valid_hook_size(spec.size)?;
        let id = self.next_id();
        let mut hook = Hook {
            id,
            size,
            brand: String::new(),
            kind: String::new(),
            created: now,
        };
        hook.edit(spec);
        self.hooks.push(hook);
        Some(id)
    }

    /// Changes a hook; like `add_hook`, it needs a size.
    pub fn edit_hook(&mut self, id: u64, spec: &HookSpec) -> bool {
        if valid_hook_size(spec.size).is_none() {
            return false;
        }
        self.hooks
            .iter_mut()
            .find(|h| h.id == id)
            .map(|h| h.edit(spec))
            .is_some()
    }

    /// Removes a hook, also from the projects that use it.
    pub fn remove_hook(&mut self, id: u64) -> Option<Hook> {
        let index = self.hooks.iter().position(|h| h.id == id)?;
        for project in &mut self.projects {
            project.hooks.retain(|h| *h != id);
        }
        Some(self.hooks.remove(index))
    }

    /// Adds a hook to a project or takes it off. Returns false if nothing changed.
    pub fn set_project_hook(&mut self, project: u64, hook: u64, used: bool, now: u64) -> bool {
        if !self.hooks.iter().any(|h| h.id == hook) {
            return false;
        }
        let Some(p) = self.get(project) else {
            return false;
        };
        if p.hooks.contains(&hook) == used {
            return false;
        }
        let Some(project) = self.touch(project, now) else {
            return false;
        };
        if used {
            project.hooks.push(hook);
        } else {
            project.hooks.retain(|h| *h != hook);
        }
        true
    }

    /// The hooks from the smallest, then by brand and kind.
    pub fn sorted_hooks(&self) -> Vec<&Hook> {
        let mut hooks: Vec<_> = self.hooks.iter().collect();
        hooks.sort_by_cached_key(|h| (h.size, h.brand.to_lowercase(), h.kind.to_lowercase(), h.id));
        hooks
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
                ProjectSort::Custom => a.position.cmp(&b.position),
            };
            (a.status.group().cmp(&b.status.group()))
                .then(by)
                .then(b.updated.cmp(&a.updated))
                .then(b.id.cmp(&a.id))
        });
        projects
    }

    /// Moves project `id` to the place of `target` in the custom order, starting from the order of
    /// `from` (the current sort). Both must be in the same status group.
    pub fn move_project(&mut self, id: u64, target: u64, from: ProjectSort) -> bool {
        let mut order: Vec<u64> = self
            .sorted(StatusFilter::All, from)
            .iter()
            .map(|p| p.id)
            .collect();
        let (Some(a), Some(b)) = (self.get(id), self.get(target)) else {
            return false;
        };
        if id == target || a.status.group() != b.status.group() {
            return false;
        }
        let Some(index) = order.iter().position(|&p| p == id) else {
            return false;
        };
        let Some(to) = order.iter().position(|&p| p == target) else {
            return false;
        };
        order.remove(index);
        order.insert(to, id);
        for project in &mut self.projects {
            if let Some(position) = order.iter().position(|&p| p == project.id) {
                project.position = position as i64;
            }
        }
        true
    }
}

/// Progress in per mille, or `None` (sorted last) without any row goal.
fn progress_key(project: &Project) -> Option<u64> {
    let (done, total) = project.progress();
    (total > 0).then(|| u64::from(done) * 1000 / u64::from(total))
}

fn has_yarn_name(spec: &YarnSpec) -> bool {
    [spec.brand, spec.name, spec.color]
        .iter()
        .any(|s| !s.trim().is_empty())
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
    /// Yarn is counted in grams instead of skeins (yarns with a known weight only).
    #[serde(default)]
    pub yarn_in_grams: bool,
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
            yarn_in_grams: false,
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
        assert_eq!(ids(ProjectSort::Custom), [scarf, hat, bunny, blanket]);
    }

    #[test]
    fn move_project_arranges_the_custom_order() {
        let mut lib = Library::default();
        let a = lib.create("A", 1);
        let b = lib.create("B", 2);
        let c = lib.create("C", 3);
        let done = lib.create("Done", 4);
        lib.set_status(done, Status::Finished, 5);
        let ids = |lib: &Library| -> Vec<u64> {
            lib.sorted(StatusFilter::All, ProjectSort::Custom)
                .iter()
                .map(|p| p.id)
                .collect()
        };
        // Starts from the name order and moves A down past C.
        assert!(lib.move_project(a, c, ProjectSort::Name));
        assert_eq!(ids(&lib), [b, c, a, done]);
        assert!(lib.move_project(a, b, ProjectSort::Custom));
        assert_eq!(ids(&lib), [a, b, c, done]);
        // Not across status groups.
        assert!(!lib.move_project(c, done, ProjectSort::Custom));
        assert_eq!(ids(&lib), [a, b, c, done]);
        // New projects go to the top.
        let d = lib.create("D", 6);
        assert_eq!(ids(&lib), [d, a, b, c, done]);
    }

    #[test]
    fn folders_hold_projects() {
        let mut lib = Library::default();
        let hat = lib.create("Hat", 1);
        let scarf = lib.create("Scarf", 2);
        let toys = lib.create_folder("  Toys  ");
        let gifts = lib.create_folder("gifts");
        assert_eq!(lib.folder(toys).unwrap().name, "Toys");
        let names: Vec<_> = lib.sorted_folders().iter().map(|f| f.id).collect();
        assert_eq!(names, [gifts, toys]);

        assert!(lib.move_to_folder(hat, Some(toys)));
        assert!(lib.move_to_folder(scarf, Some(toys)));
        assert!(!lib.move_to_folder(scarf, Some(999)), "no such folder");
        assert_eq!(lib.folder_size(toys), 2);
        assert!(lib.move_to_folder(scarf, None));
        assert_eq!(lib.folder_size(toys), 1);

        assert!(lib.rename_folder(toys, "Amigurumi"));
        assert_eq!(lib.folder(toys).unwrap().name, "Amigurumi");
        // Removing a folder keeps its projects.
        assert!(lib.remove_folder(toys).is_some());
        assert_eq!(lib.get(hat).unwrap().folder, None);
        assert_eq!(lib.projects.len(), 2);
        assert_ne!(lib.create_folder("New"), toys, "ids are not reused");
    }

    #[test]
    fn folders_are_kept_and_optional_on_disk() {
        let mut lib = Library::default();
        let hat = lib.create("Hat", 1);
        let json = serde_json::to_string(&lib).unwrap();
        assert!(!json.contains("folder"), "{json}");

        let toys = lib.create_folder("Toys");
        lib.move_to_folder(hat, Some(toys));
        let json = serde_json::to_string(&lib).unwrap();
        let mut again: Library = serde_json::from_str(&json).unwrap();
        again.migrate();
        assert_eq!(again, lib);
    }

    #[test]
    fn the_stash_holds_yarns() {
        let mut lib = Library::default();
        let hat = lib.create("Hat", 1);
        let spec = YarnSpec {
            brand: " Schachenmayr ",
            name: "Catania",
            color: "Rosa  0222",
            dye_lot: "4711",
            fiber: "100% cotton",
            weight: Some(50),
            length: Some(125),
            // In grams, since the weight is known.
            amount: 170,
        };
        let catania = lib.add_yarn(&spec, 5).unwrap();
        assert_ne!(catania, hat, "ids are shared with projects");
        let yarn = lib.yarn(catania).unwrap();
        assert_eq!(yarn.title(), "Schachenmayr Catania");
        assert_eq!(yarn.color, "Rosa 0222");
        assert_eq!(
            (yarn.weight, yarn.length, yarn.quantity, yarn.opened),
            (Some(50), Some(125), 3, 20)
        );

        // Only a color is enough, but nothing at all is not.
        let blue = lib
            .add_yarn(
                &YarnSpec {
                    color: "Blue",
                    weight: Some(0),
                    ..YarnSpec::default()
                },
                6,
            )
            .unwrap();
        assert_eq!(lib.yarn(blue).unwrap().weight, None, "0 g means unknown");
        assert_eq!(lib.yarn(blue).unwrap().title(), "");
        assert_eq!(
            lib.add_yarn(
                &YarnSpec {
                    fiber: "Wool",
                    ..YarnSpec::default()
                },
                7
            ),
            None
        );
        assert!(!lib.edit_yarn(blue, &YarnSpec::default()));

        let sorted: Vec<_> = lib.sorted_yarns().iter().map(|y| y.id).collect();
        assert_eq!(sorted, [blue, catania], "no brand comes first");

        assert!(lib.edit_yarn(
            catania,
            &YarnSpec {
                amount: 100,
                ..spec
            }
        ));
        assert_eq!(lib.yarn(catania).unwrap().units(), 100);
        assert!(lib.remove_yarn(catania).is_some());
        assert!(lib.yarn(catania).is_none());
        assert!(!lib.edit_yarn(catania, &spec));
    }

    #[test]
    fn hooks_have_a_size() {
        let mut lib = Library::default();
        let hat = lib.create("Hat", 1);
        let spec = HookSpec {
            size: Some(350),
            brand: " Clover ",
            kind: "Amour  ergonomic",
        };
        let amour = lib.add_hook(&spec, 5).unwrap();
        assert_ne!(amour, hat, "ids are shared with projects");
        let hook = lib.hook(amour).unwrap();
        assert_eq!(
            (hook.size, hook.title().as_str()),
            (350, "Clover Amour ergonomic")
        );

        // A size is enough, but needed.
        let small = lib
            .add_hook(
                &HookSpec {
                    size: Some(200),
                    ..HookSpec::default()
                },
                6,
            )
            .unwrap();
        assert_eq!(lib.hook(small).unwrap().title(), "");
        for size in [None, Some(0), Some(MAX_HOOK_SIZE + 1)] {
            let spec = HookSpec { size, ..spec };
            assert_eq!(lib.add_hook(&spec, 7), None, "{size:?}");
            assert!(!lib.edit_hook(small, &spec), "{size:?}");
        }

        let big = lib
            .add_hook(
                &HookSpec {
                    size: Some(350),
                    brand: "Addi",
                    ..HookSpec::default()
                },
                8,
            )
            .unwrap();
        let sorted: Vec<_> = lib.sorted_hooks().iter().map(|h| h.id).collect();
        assert_eq!(sorted, [small, big, amour], "by size, then brand");

        assert!(lib.edit_hook(
            amour,
            &HookSpec {
                size: Some(400),
                ..spec
            }
        ));
        assert_eq!(lib.hook(amour).unwrap().size, 400);
        assert!(lib.remove_hook(amour).is_some());
        assert!(lib.hook(amour).is_none());
        assert!(!lib.edit_hook(amour, &spec));
    }

    #[test]
    fn hooks_are_kept_and_optional_on_disk() {
        let mut lib = Library::default();
        lib.create("Hat", 1);
        let json = serde_json::to_string(&lib).unwrap();
        assert!(!json.contains("hooks"), "{json}");

        lib.add_hook(
            &HookSpec {
                size: Some(450),
                ..HookSpec::default()
            },
            2,
        );
        let json = serde_json::to_string(&lib).unwrap();
        assert!(!json.contains("brand"), "empty fields are left out: {json}");
        let mut again: Library = serde_json::from_str(&json).unwrap();
        again.migrate();
        assert_eq!(again, lib);
    }

    #[test]
    fn us_hook_sizes() {
        let us = |size| us_hook_size(size).map(|s| (s.name, s.steel));
        assert_eq!(us(350), Some(("E-4", false)));
        assert_eq!(us(650), Some(("K-10½", false)));
        assert_eq!(us(150), Some(("8", true)));
        assert_eq!(us(300), None, "3 mm has no US size");
        assert!(
            US_HOOK_SIZES.windows(2).all(|w| w[0].0 < w[1].0),
            "sorted, no duplicates"
        );
    }

    #[test]
    fn projects_take_yarn_out_of_the_stash() {
        let mut lib = Library::default();
        let hat = lib.create("Hat", 1);
        let scarf = lib.create("Scarf", 1);
        // No weight: counted in tenths of a skein.
        let catania = lib
            .add_yarn(
                &YarnSpec {
                    name: "Catania",
                    amount: 30,
                    ..YarnSpec::default()
                },
                1,
            )
            .unwrap();
        let stash = |lib: &Library| lib.yarn(catania).unwrap().units();
        let used = |lib: &mut Library, project, units| {
            lib.set_project_yarn(project, catania, units, 5)
                .map(|u| (u.skeins, u.opened))
        };

        assert_eq!(used(&mut lib, hat, 15), Some((1, 5)), "1.5 skeins");
        assert_eq!(stash(&lib), 15);
        assert_eq!(lib.get(hat).unwrap().updated, 5);
        // No more than the stash has.
        assert_eq!(used(&mut lib, scarf, 40), Some((1, 5)));
        assert!(lib.yarn(catania).unwrap().used_up());
        assert_eq!(used(&mut lib, hat, 30), Some((1, 5)), "none left");
        let users: Vec<_> = lib
            .projects_with_yarn(catania)
            .iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(users, [hat, scarf]);

        // Giving some back, and 0 takes the yarn off the project.
        assert_eq!(used(&mut lib, hat, 5), Some((0, 5)));
        assert_eq!(stash(&lib), 10);
        assert_eq!(used(&mut lib, scarf, 0), Some((0, 0)));
        assert_eq!(stash(&lib), 25);
        assert_eq!(lib.get(scarf).unwrap().yarns, []);
        assert_eq!(used(&mut lib, 99, 1), None);
        assert_eq!(lib.set_project_yarn(hat, 99, 1, 6), None);

        // Deleting the yarn takes it off the projects.
        lib.remove_yarn(catania);
        assert_eq!(lib.get(hat).unwrap().yarns, []);
    }

    #[test]
    fn yarn_with_a_weight_is_counted_in_grams() {
        let mut lib = Library::default();
        let hat = lib.create("Hat", 1);
        let spec = YarnSpec {
            name: "Paris",
            amount: 24,
            ..YarnSpec::default()
        };
        let paris = lib.add_yarn(&spec, 1).unwrap();
        lib.set_project_yarn(hat, paris, 10, 2);
        let stash = |lib: &Library| {
            let y = lib.yarn(paris).unwrap();
            (y.quantity, y.opened, y.grams())
        };
        assert_eq!(stash(&lib), (1, 4, None), "1.4 skeins");

        // The weight becomes known: what the hat used turns into grams, too.
        assert!(lib.edit_yarn(
            paris,
            &YarnSpec {
                weight: Some(50),
                amount: 70,
                ..spec
            }
        ));
        assert_eq!(stash(&lib), (1, 20, Some(70)));
        assert_eq!(lib.get(hat).unwrap().yarns[0].units(50), 50);

        let used = lib.set_project_yarn(hat, paris, 83, 3).unwrap();
        assert_eq!((used.skeins, used.opened), (1, 33));
        assert_eq!(stash(&lib), (0, 37, Some(37)));
        assert_eq!(convert_units(83, 50, 10), 17, "1.66 skeins are 1.7");
    }

    #[test]
    fn projects_use_hooks() {
        let mut lib = Library::default();
        let hat = lib.create("Hat", 1);
        let hook = lib
            .add_hook(
                &HookSpec {
                    size: Some(350),
                    ..HookSpec::default()
                },
                1,
            )
            .unwrap();
        assert!(lib.set_project_hook(hat, hook, true, 2));
        assert!(!lib.set_project_hook(hat, hook, true, 3), "already there");
        assert_eq!(lib.get(hat).unwrap().hooks, [hook]);
        assert!(!lib.set_project_hook(hat, 99, true, 3));
        assert!(lib.set_project_hook(hat, hook, false, 3));
        assert_eq!(lib.get(hat).unwrap().hooks, Vec::<u64>::new());

        lib.set_project_hook(hat, hook, true, 4);
        lib.remove_hook(hook);
        assert_eq!(lib.get(hat).unwrap().hooks, Vec::<u64>::new());
    }

    #[test]
    fn materials_are_kept_and_links_to_missing_ones_dropped() {
        let mut lib = Library::default();
        let hat = lib.create("Hat", 1);
        let json = serde_json::to_string(&lib).unwrap();
        assert!(!json.contains("hooks"), "{json}");
        let yarn = lib
            .add_yarn(
                &YarnSpec {
                    color: "Blue",
                    amount: 20,
                    ..YarnSpec::default()
                },
                1,
            )
            .unwrap();
        let hook = lib
            .add_hook(
                &HookSpec {
                    size: Some(400),
                    ..HookSpec::default()
                },
                1,
            )
            .unwrap();
        lib.set_project_yarn(hat, yarn, 15, 2);
        lib.set_project_hook(hat, hook, true, 2);
        let json = serde_json::to_string(&lib).unwrap();
        let mut again: Library = serde_json::from_str(&json).unwrap();
        again.migrate();
        assert_eq!(again, lib);

        // E.g. edited by hand: links to a yarn or hook that is gone are dropped.
        let mut broken: Library = serde_json::from_str(&json).unwrap();
        broken.yarns.clear();
        broken.hooks.clear();
        broken.migrate();
        let project = broken.get(hat).unwrap();
        assert!(project.yarns.is_empty() && project.hooks.is_empty());
    }

    #[test]
    fn yarns_are_kept_and_optional_on_disk() {
        let mut lib = Library::default();
        lib.create("Hat", 1);
        let json = serde_json::to_string(&lib).unwrap();
        assert!(!json.contains("yarns"), "{json}");

        let spec = YarnSpec {
            name: "Catania",
            weight: Some(50),
            amount: 50,
            ..YarnSpec::default()
        };
        lib.add_yarn(&spec, 2);
        let json = serde_json::to_string(&lib).unwrap();
        assert!(
            !json.contains("dye_lot"),
            "empty fields are left out: {json}"
        );
        let mut again: Library = serde_json::from_str(&json).unwrap();
        again.migrate();
        assert_eq!(again, lib);
    }

    #[test]
    fn migrate_drops_missing_folders() {
        let json = r#"{"version":2,"next_id":3,"folders":[{"id":2,"name":"Toys"}],"projects":[
            {"id":1,"name":"Hat","folder":7,"created":0,"updated":0},
            {"id":3,"name":"Bear","folder":2,"created":0,"updated":0}]}"#;
        let mut lib: Library = serde_json::from_str(json).unwrap();
        lib.migrate();
        assert_eq!(lib.get(1).unwrap().folder, None);
        assert_eq!(lib.get(3).unwrap().folder, Some(2));
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
