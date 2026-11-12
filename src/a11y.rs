//! TalkBack support. Slint's Android backend has no accessibility bridge, so this module walks the
//! element tree (through the element API of `i-slint-backend-testing`) while an accessibility service is on and
//! sends a flat list of nodes to `FlauschHelper`, which exposes them as virtual views. Actions on
//! those views come back through `perform`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::ControlFlow;
use std::time::Duration;

use i_slint_backend_testing::{AccessibleLiveness, AccessibleRole, ElementHandle, ElementRoot};
use slint::ComponentHandle;
use slint::platform::{PointerEventButton, WindowEvent};

use crate::{A11yFocus, App, DialogKind, Screen, Store, platform};

/// `FlauschHelper.NO_NODE`: nothing has accessibility focus.
const NO_NODE: i32 = i32::MIN;
const POLL_INTERVAL: Duration = Duration::from_millis(300);
/// `accessible-id` of the dialog card; while it is shown, only nodes inside it are exposed.
const DIALOG_ID: &str = "dialog";

// Node kinds, mirrored in FlauschHelper.
const KIND_TEXT: i32 = 0;
const KIND_BUTTON: i32 = 1;
const KIND_SWITCH: i32 = 2;
const KIND_CHECKBOX: i32 = 3;
const KIND_EDIT: i32 = 4;
const KIND_SLIDER: i32 = 5;
const KIND_IMAGE: i32 = 6;
const KIND_RADIO: i32 = 7;
/// A button that can be moved up and down (project cards); `value` holds the action labels.
const KIND_ITEM: i32 = 8;

// Node flags, mirrored in FlauschHelper.
const FLAG_ENABLED: i32 = 1;
const FLAG_CHECKABLE: i32 = 2;
const FLAG_CHECKED: i32 = 4;

// android.view.accessibility.AccessibilityNodeInfo action ids.
const ACTION_CLICK: i32 = 0x10;
const ACTION_SCROLL_FORWARD: i32 = 0x1000;
const ACTION_SCROLL_BACKWARD: i32 = 0x2000;
const ACTION_SET_TEXT: i32 = 0x0020_0000;
// Custom actions, mirrored in FlauschHelper.
const ACTION_MOVE_UP: i32 = 0x0ff0_0001;
const ACTION_MOVE_DOWN: i32 = 0x0ff0_0002;

/// One virtual view. `rect` is x, y, width, height in physical window pixels.
#[derive(Clone, PartialEq, Debug)]
pub struct Node {
    pub id: i32,
    pub rect: [i32; 4],
    pub kind: i32,
    pub flags: i32,
    pub label: String,
    pub value: String,
}

struct State {
    app: slint::Weak<App>,
    _timer: slint::Timer,
    elements: HashMap<i32, (ElementHandle, i32)>,
    nodes: Vec<Node>,
    scope: Option<(Screen, DialogKind)>,
    live: String,
    focused: i32,
}

thread_local! {
    // The allow works around a clippy false positive on aarch64-linux-android (it is already `const`).
    #[allow(clippy::missing_const_for_thread_local)]
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Installs the bridge and starts polling the UI while an accessibility service is on.
pub fn start(app: &App) {
    platform::a11y_install();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, POLL_INTERVAL, update);
    STATE.with_borrow_mut(|s| {
        *s = Some(State {
            app: app.as_weak(),
            _timer: timer,
            elements: HashMap::new(),
            nodes: Vec::new(),
            scope: None,
            live: String::new(),
            focused: NO_NODE,
        });
    });
}

fn update() {
    STATE.with_borrow_mut(|state| {
        let Some(state) = state else { return };
        let Some(app) = state.app.upgrade() else {
            return;
        };
        if !platform::a11y_enabled() {
            if !state.nodes.is_empty() {
                state.nodes.clear();
                state.elements.clear();
                state.scope = None;
                platform::a11y_update(&[], false);
                state.focused = NO_NODE;
                show_focus(&app, state);
            }
            return;
        }

        let (nodes, elements, live) = snapshot(&app);
        let store = app.global::<Store>();
        let scope = Some((store.get_screen(), store.get_dialog()));
        let reset = scope != state.scope;
        if reset || nodes != state.nodes {
            platform::a11y_update(&nodes, reset);
        }
        if !live.is_empty() && live != state.live {
            platform::a11y_announce(&live);
        }
        state.live = live;
        state.scope = scope;
        state.nodes = nodes;
        state.elements = elements;
        show_focus(&app, state);
    });
}

/// Called when TalkBack moves its focus to the node `id` (or `NO_NODE`).
pub fn set_focus(id: i32) {
    STATE.with_borrow_mut(|state| {
        let Some(state) = state else { return };
        state.focused = id;
        if let Some(app) = state.app.upgrade() {
            show_focus(&app, state);
        }
    });
}

/// Draws the focus frame around the focused node, because Android can't draw it over Slint.
fn show_focus(app: &App, state: &State) {
    let scale = app.window().scale_factor();
    let focus = state
        .nodes
        .iter()
        .find(|n| n.id == state.focused)
        .map_or_else(A11yFocus::default, |n| A11yFocus {
            visible: true,
            x: n.rect[0] as f32 / scale,
            y: n.rect[1] as f32 / scale,
            width: n.rect[2] as f32 / scale,
            height: n.rect[3] as f32 / scale,
        });
    let store = app.global::<Store>();
    if store.get_a11y_focus() != focus {
        store.set_a11y_focus(focus);
    }
}

/// Runs an accessibility action from TalkBack on the element behind the node `id`.
pub fn perform(id: i32, action: i32, text: String) {
    let found = STATE.with_borrow(|s| {
        s.as_ref()
            .and_then(|s| Some((s.app.upgrade()?, s.elements.get(&id)?.clone())))
    });
    let Some((app, (element, kind))) = found else {
        return;
    };
    match action {
        ACTION_CLICK if kind == KIND_EDIT => tap(&app, &element),
        ACTION_CLICK => element.invoke_accessible_default_action(),
        ACTION_SCROLL_FORWARD | ACTION_MOVE_UP => element.invoke_accessible_increment_action(),
        ACTION_SCROLL_BACKWARD | ACTION_MOVE_DOWN => element.invoke_accessible_decrement_action(),
        ACTION_SET_TEXT => element.set_accessible_value(text),
        _ => return,
    }
    slint::Timer::single_shot(Duration::from_millis(50), update);
}

/// Taps the center of `element`, e.g. to focus a text field and open the keyboard.
fn tap(app: &App, element: &ElementHandle) {
    let position = element.absolute_position();
    let size = element.size();
    let position = slint::LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );
    let window = app.window();
    let button = PointerEventButton::Left;
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed { position, button });
    window.dispatch_event(WindowEvent::PointerReleased { position, button });
    window.dispatch_event(WindowEvent::PointerExited);
}

struct Candidate {
    element: ElementHandle,
    kind: i32,
    rect: [i32; 4],
    flags: i32,
    label: String,
    value: String,
}

fn snapshot(app: &App) -> (Vec<Node>, HashMap<i32, (ElementHandle, i32)>, String) {
    let scale = app.window().scale_factor();
    let store = app.global::<Store>();
    let moves = format!(
        "{}\n{}",
        store.get_move_up_label(),
        store.get_move_down_label()
    );
    let size = app.window().size();
    let window = [0, 0, size.width as i32, size.height as i32];
    let mut candidates = Vec::new();
    let mut dialog = None;
    let mut live = String::new();

    app.root_element().visit_descendants(|element| {
        if element.computed_opacity() < 0.05 {
            return ControlFlow::<()>::Continue(());
        }
        let position = element.absolute_position();
        let extent = element.size();
        let rect = intersect(
            [
                (position.x * scale).round() as i32,
                (position.y * scale).round() as i32,
                (extent.width * scale).round() as i32,
                (extent.height * scale).round() as i32,
            ],
            window,
        );
        if element.accessible_id().as_deref() == Some(DIALOG_ID) {
            dialog = Some(rect);
        }
        let label = element.accessible_label().unwrap_or_default().to_string();
        let mut value = element.accessible_value().unwrap_or_default().to_string();
        if label.is_empty() && value.is_empty() {
            return ControlFlow::Continue(());
        }
        if matches!(
            element.accessible_live_region(),
            Some(AccessibleLiveness::Polite | AccessibleLiveness::Assertive)
        ) {
            live.clone_from(&label);
        }
        let Some(kind) = element.accessible_role().and_then(kind) else {
            return ControlFlow::Continue(());
        };
        if rect[2] <= 0 || rect[3] <= 0 {
            return ControlFlow::Continue(());
        }
        if kind == KIND_ITEM {
            value.clone_from(&moves);
        }
        let checked = element.accessible_checked();
        let mut flags = 0;
        if element.accessible_enabled().unwrap_or(true) {
            flags |= FLAG_ENABLED;
        }
        if element.accessible_checkable().unwrap_or(checked.is_some()) {
            flags |= FLAG_CHECKABLE;
        }
        if checked == Some(true) {
            flags |= FLAG_CHECKED;
        }
        candidates.push(Candidate {
            element,
            kind,
            rect,
            flags,
            label,
            value,
        });
        ControlFlow::Continue(())
    });

    if let Some(dialog) = dialog {
        candidates.retain(|c| contains(dialog, c.rect));
    }
    let (nodes, elements) = nodes(candidates);
    (nodes, elements, live)
}

fn nodes(candidates: Vec<Candidate>) -> (Vec<Node>, HashMap<i32, (ElementHandle, i32)>) {
    // Texts and images inside a button become part of its description instead of own nodes.
    let mut extras: Vec<Vec<String>> = vec![Vec::new(); candidates.len()];
    let mut keep = vec![true; candidates.len()];
    for (i, c) in candidates.iter().enumerate() {
        if c.kind != KIND_TEXT && c.kind != KIND_IMAGE {
            continue;
        }
        let owner = candidates
            .iter()
            .enumerate()
            .filter(|(_, o)| is_control(o.kind) && contains(o.rect, c.rect))
            .min_by_key(|(_, o)| i64::from(o.rect[2]) * i64::from(o.rect[3]))
            .map(|(j, _)| j);
        if let Some(j) = owner {
            keep[i] = false;
            let owner = &candidates[j];
            if owner.kind != KIND_EDIT
                && c.label != owner.label
                && !owner.label.contains(&c.label)
                && !extras[j].contains(&c.label)
            {
                extras[j].push(c.label.clone());
            }
        }
    }

    let mut nodes = Vec::new();
    let mut elements = HashMap::new();
    let mut seen: HashMap<(i32, String), u32> = HashMap::new();
    for ((c, extra), keep) in candidates.into_iter().zip(extras).zip(keep) {
        if !keep {
            continue;
        }
        let ordinal = seen.entry((c.kind, c.label.clone())).or_default();
        let mut id = node_id(c.kind, &c.label, *ordinal);
        *ordinal += 1;
        while elements.contains_key(&id) {
            id = id % 0x3fff_ffff + 1;
        }
        let mut label = c.label;
        for text in extra {
            label.push_str(", ");
            label.push_str(&text);
        }
        nodes.push(Node {
            id,
            rect: c.rect,
            kind: c.kind,
            flags: c.flags,
            label,
            value: c.value,
        });
        elements.insert(id, (c.element, c.kind));
    }
    (nodes, elements)
}

fn kind(role: AccessibleRole) -> Option<i32> {
    Some(match role {
        AccessibleRole::Text => KIND_TEXT,
        AccessibleRole::ListItem => KIND_ITEM,
        AccessibleRole::Button | AccessibleRole::Tab | AccessibleRole::Combobox => KIND_BUTTON,
        AccessibleRole::Switch => KIND_SWITCH,
        AccessibleRole::Checkbox => KIND_CHECKBOX,
        AccessibleRole::RadioButton => KIND_RADIO,
        AccessibleRole::TextInput => KIND_EDIT,
        AccessibleRole::Slider | AccessibleRole::Spinbox => KIND_SLIDER,
        AccessibleRole::Image => KIND_IMAGE,
        _ => return None,
    })
}

fn is_control(kind: i32) -> bool {
    !matches!(kind, KIND_TEXT | KIND_IMAGE)
}

/// A stable id, so TalkBack keeps its focus while the rest of the screen changes.
fn node_id(kind: i32, label: &str, ordinal: u32) -> i32 {
    let mut hasher = DefaultHasher::new();
    (kind, label, ordinal).hash(&mut hasher);
    (hasher.finish() % 0x3fff_ffff) as i32 + 1
}

fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    [x, y, (right - x).max(0), (bottom - y).max(0)]
}

/// Whether the center of `inner` lies within `outer`.
fn contains(outer: [i32; 4], inner: [i32; 4]) -> bool {
    let cx = inner[0] + inner[2] / 2;
    let cy = inner[1] + inner[3] / 2;
    cx >= outer[0] && cx < outer[0] + outer[2] && cy >= outer[1] && cy < outer[1] + outer[3]
}
