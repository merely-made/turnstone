//! The chrome as a cambium view over a FOREST of window-roots — the toolkit
//! question's endpoint, executed 2026-07-18 ("chrome migrates to a cambium
//! view"), and turnstone's literal consumption of the two forest primitives:
//! cambium's `push_forest_projection` (every window's chrome is a window-root
//! subtree of ONE shared document) and retained Livery/Buckram subtree layout (each
//! window lays out and paints ITS root at its own size).
//!
//! What changed from the rung-3 hand chrome (`ui::chrome_scene`, retired with
//! this): the DOM is RETAINED and diffed per state change instead of rebuilt
//! wholesale per frame; suggestion rows carry real `on_click` handlers (a row
//! click commits — new capability, lowered as `OmnibarCommitRow` through the
//! spine); and lens windows get chrome (the caption chip) as their own forest
//! projections — one chrome document, N window-roots.
//!
//! What deliberately did NOT migrate, and why (consumer-pull teaching the
//! catalog, recorded against the surfaces-in-cambium mapping's prediction):
//!
//! - `command_palette`/`action_list` own their `query` state and filter a
//!   static item list; turnstone's omnibar keys lower through the Action spine
//!   (doctrine 2) and its suggestions are GRAPH TRUTH re-queried per edit.
//!   The rows here render `OmnibarState`, they do not own it.
//! - The omnibar is a controlled mirror, not an editor: keys lower through the
//!   Action spine and the view must not mutate a second text model. Cambium now
//!   exposes `caret_field_children`, the pure text/preedit/caret projection
//!   beneath its editable field. Turnstone consumes that rendering while
//!   retaining app truth and input authority here.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetCtx, GenetElement, GenetMultiRunner, PointerClick, ProjectionId,
    TextInput, caret_field_children, el, on_click,
};
use genet_scripted_dom::ScriptedDom;
// `drain_mutations` is the mutation queue's only reader; the chrome owns that
// drain for the whole forest (see `absorb_dom_mutations`).
use layout_dom_api::LayoutDomMut;

use crate::app::App;
use crate::panes::{ChromeEdge, ChromePlacement};
use crate::ui::{CARD_TOP, CARD_W, OmnibarState, Suggestion};

/// What a chrome interaction produces: commit the suggestion row at this
/// original index (the shell lowers `Action::OmnibarCommitRow`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeIntent {
    CommitRow(usize),
    NavBack,
    NavForward,
    Reload,
    Stop,
    KeepNode(uuid::Uuid),
    FindPrevious,
    FindNext,
    FindClose,
}

impl cambium::Action for ChromeIntent {}

/// How one suggestion row renders.
#[derive(Clone, Debug, PartialEq)]
struct RowView {
    text: String,
    /// Class: selected / plain / muted (hints are inert).
    class: &'static str,
    /// The row's index in `OmnibarState::suggestions` (what a click commits);
    /// `None` for inert hint rows.
    commit: Option<usize>,
}

/// One window's chrome inputs: its size and its caption.
#[derive(Clone, Debug, Default, PartialEq)]
struct WindowChrome {
    w: f32,
    h: f32,
    caption: Option<String>,
    /// Only the primary window carries the omnibar; lenses show the chip.
    primary: bool,
}

/// The one chrome state every projection renders (the one-state contract).
struct ChromeState {
    windows: Vec<WindowChrome>,
    open: bool,
    /// App-owned omnibar truth projected into Cambium's pure caret renderer.
    /// This runner never dispatches edit keys into it.
    omnibar_input: TextInput,
    rows: Vec<RowView>,
    omnibar_placement: ChromePlacement,
    shellbar_placement: ChromePlacement,
    shellbar_visible: bool,
    can_back: bool,
    can_forward: bool,
    focused_node: Option<uuid::Uuid>,
    focused_kept: bool,
    fetching: bool,
    fetch_status: Option<String>,
    focused_url: Option<String>,
    link_preview: Option<String>,
    find_open: bool,
    find_input: TextInput,
    find_status: String,
}

type ChromeView = Box<dyn AnyView<ChromeState, ChromeIntent, GenetCtx, GenetElement>>;

/// One window's chrome view: the caption chip, plus the omnibar card on the
/// primary while open. Positioned by transform-translate (the property the
/// canvas gnode pool proves the retained engine honors on absolutes).
fn window_chrome_view(state: &ChromeState, slot: usize) -> ChromeView {
    let Some(win) = state.windows.get(slot).cloned() else {
        return Box::new(el::<_, ChromeState, ChromeIntent>("div", ()));
    };
    let mut children: Vec<ChromeView> = Vec::new();

    if win.primary && state.shellbar_visible {
        let back = browser_button("Back", state.can_back.then_some(ChromeIntent::NavBack));
        let forward = browser_button(
            "Forward",
            state.can_forward.then_some(ChromeIntent::NavForward),
        );
        let reload = browser_button(
            if state.fetching { "Stop" } else { "Reload" },
            state.focused_node.map(|_| {
                if state.fetching {
                    ChromeIntent::Stop
                } else {
                    ChromeIntent::Reload
                }
            }),
        );
        let keep = browser_button(
            if state.focused_kept { "Kept" } else { "Keep" },
            state
                .focused_node
                .filter(|_| !state.focused_kept)
                .map(ChromeIntent::KeepNode),
        );
        let status = state
            .fetch_status
            .clone()
            .or_else(|| state.focused_url.clone())
            .unwrap_or_else(|| "No focused node".to_string());
        let mut strip_children = vec![back, forward, reload, keep];
        strip_children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", status).attr("class", "browser-status"),
        ));
        let left = ((win.w - 620.0) / 2.0).max(8.0);
        children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", strip_children)
                .attr(
                    "class",
                    if state.fetching {
                        "browser-strip browser-loading"
                    } else {
                        "browser-strip"
                    },
                )
                .attr(
                    "style",
                    format!("transform: translate({left}px, 12px); width: 620px;"),
                ),
        ));

        if let Some(target) = &state.link_preview {
            children.push(Box::new(
                el::<_, ChromeState, ChromeIntent>("div", format!("Prospective node  {target}"))
                    .attr("class", "link-preview")
                    .attr(
                        "style",
                        format!(
                            "transform: translate(12px, {}px); max-width: {}px;",
                            (win.h - 42.0).max(8.0),
                            (win.w - 24.0).max(120.0)
                        ),
                    ),
            ));
        }
    }

    if win.primary && state.find_open {
        let input = el::<_, ChromeState, ChromeIntent>(
            "div",
            caret_field_children::<ChromeState, ChromeIntent>(&state.find_input, &[]),
        )
        .attr("class", "document-find-input")
        .attr("role", "searchbox")
        .attr("aria-label", "Find in document");
        let status = el::<_, ChromeState, ChromeIntent>("div", state.find_status.clone())
            .attr("class", "document-find-status")
            .attr("role", "status");
        let find_children = vec![
            Box::new(input) as ChromeView,
            Box::new(status) as ChromeView,
            browser_button("Previous", Some(ChromeIntent::FindPrevious)),
            browser_button("Next", Some(ChromeIntent::FindNext)),
            browser_button("Close", Some(ChromeIntent::FindClose)),
        ];
        let left = ((win.w - 620.0) / 2.0).max(8.0);
        children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", find_children)
                .attr("class", "document-find")
                .attr(
                    "style",
                    format!("transform: translate({left}px, 58px); width: 620px;"),
                ),
        ));
    }

    if let Some(caption) = &win.caption
        && state.shellbar_visible
        && let Some((left, top)) = chrome_position(&state.shellbar_placement, win.w, win.h, 148.0)
    {
        children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", caption.clone())
                .attr("class", "whereami")
                .attr("style", format!("transform: translate({left}px, {top}px);")),
        ));
    }

    if win.primary
        && state.open
        && let Some((left, top)) = chrome_position(&state.omnibar_placement, win.w, win.h, CARD_W)
    {
        let input = el::<_, ChromeState, ChromeIntent>(
            "div",
            caret_field_children::<ChromeState, ChromeIntent>(&state.omnibar_input, &[]),
        )
        .attr("class", "omni-input");

        let mut card_children: Vec<ChromeView> = vec![Box::new(input)];
        for row in &state.rows {
            let base = el::<_, ChromeState, ChromeIntent>("div", row.text.clone())
                .attr("class", row.class);
            card_children.push(match row.commit {
                // A row click COMMITS that row — the handler is the point of
                // the retained view (the hand chrome had no row handlers).
                Some(index) => Box::new(on_click(
                    base,
                    move |_state: &mut ChromeState, _click: PointerClick| {
                        ChromeIntent::CommitRow(index)
                    },
                )),
                None => Box::new(base),
            });
        }
        children.push(Box::new(
            el::<_, ChromeState, ChromeIntent>("div", card_children)
                .attr("class", "omni")
                .attr(
                    "style",
                    format!("transform: translate({left}px, {top}px); width: {CARD_W}px;"),
                ),
        ));
    }

    Box::new(el::<_, ChromeState, ChromeIntent>("div", children))
}

fn browser_button(label: &'static str, intent: Option<ChromeIntent>) -> ChromeView {
    let base = el::<_, ChromeState, ChromeIntent>("div", label).attr(
        "class",
        if intent.is_some() {
            "browser-button"
        } else {
            "browser-button-disabled"
        },
    );
    if let Some(intent) = intent {
        Box::new(on_click(
            base,
            move |_state: &mut ChromeState, _click: PointerClick| intent,
        ))
    } else {
        Box::new(base)
    }
}

/// Map a configured projection placement to this pre-A4 chrome surface. A
/// `Pane` placement has no station yet, so it deliberately renders nothing
/// rather than fabricating a second pane tree.
pub(crate) fn chrome_position(
    placement: &ChromePlacement,
    width: f32,
    height: f32,
    item_width: f32,
) -> Option<(f32, f32)> {
    let centred = || ((width - item_width) / 2.0).max(8.0);
    match placement {
        ChromePlacement::Overlay | ChromePlacement::Floating => Some((centred(), CARD_TOP)),
        ChromePlacement::Docked(ChromeEdge::Top) => Some((centred(), 8.0)),
        ChromePlacement::Docked(ChromeEdge::Bottom) => Some((centred(), (height - 52.0).max(8.0))),
        ChromePlacement::Docked(ChromeEdge::Left) => Some((8.0, (height * 0.5 - 18.0).max(8.0))),
        ChromePlacement::Docked(ChromeEdge::Right) => Some((
            (width - item_width - 8.0).max(8.0),
            (height * 0.5 - 18.0).max(8.0),
        )),
        ChromePlacement::Pane(_) | ChromePlacement::Hidden => None,
    }
}
/// The per-projection logic: one closure definition (so every projection
/// shares one `Logic` type), instantiated with its window slot.
fn chrome_logic(slot: usize) -> impl FnMut(&ChromeState) -> ChromeView {
    move |state| window_chrome_view(state, slot)
}

type ChromeLogic = Box<dyn FnMut(&ChromeState) -> ChromeView>;

/// The chrome surfaces: one shared document, one forest projection per
/// window. Slot 0 is the primary; a lens's slot is `ordinal + 1`.
pub struct ChromeSurfaces {
    dom: DomHandle,
    runner: GenetMultiRunner<ChromeState, ChromeLogic, ChromeView, ChromeIntent>,
    projections: Vec<ProjectionId>,
    /// One retained layout per window-root, slot-indexed beside `projections`.
    /// Before these existed, every frame and every pointer press re-cascaded
    /// the whole chrome subtree from scratch; with the palette open that was
    /// 36 ms of a 44 ms frame.
    layouts: Vec<crate::ui::RetainedSubtreeLayout>,
    /// The host-owned presentation value applied to the retained DOM during
    /// scene layout and hit testing.
    appearance: crate::shell_services::AppearanceConfig,
}

/// A suggestion row's display text (the same rendering the hand chrome drew).
pub(crate) fn row_text(s: &Suggestion) -> String {
    match s {
        Suggestion::Node { label, host, .. } if host.is_empty() => label.clone(),
        Suggestion::Node { label, host, .. } => format!("{label}  \u{00b7}  {host}"),
        Suggestion::Go { url } => format!("\u{2192} open {url}"),
        // The clock face marks provenance: this row came from what you
        // visited, not from what is on the canvas or what you just typed.
        Suggestion::Recall { url, title, .. } => match title {
            Some(title) if !title.trim().is_empty() => {
                format!("\u{1f552} {title}  \u{00b7}  {url}")
            }
            _ => format!("\u{1f552} {url}"),
        },
        Suggestion::Act { label, .. } => format!("\u{203a} {label}"),
        Suggestion::Hint(hint) => (*hint).to_string(),
        Suggestion::Prompt(prompt) => prompt.clone(),
    }
}

/// The one action row allowed to grow vertically rather than clip.
pub(crate) fn is_install_review(s: &Suggestion) -> bool {
    matches!(
        s,
        Suggestion::Act {
            action: crate::action::Action::ConfirmInstallDenizen,
            ..
        }
    )
}

impl ChromeSurfaces {
    pub fn new() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = ChromeState {
            windows: vec![WindowChrome {
                primary: true,
                ..WindowChrome::default()
            }],
            open: false,
            omnibar_input: TextInput::default(),
            rows: Vec::new(),
            omnibar_placement: ChromePlacement::Overlay,
            shellbar_placement: ChromePlacement::Docked(ChromeEdge::Right),
            shellbar_visible: true,
            can_back: false,
            can_forward: false,
            focused_node: None,
            focused_kept: false,
            fetching: false,
            fetch_status: None,
            focused_url: None,
            link_preview: None,
            find_open: false,
            find_input: TextInput::default(),
            find_status: String::new(),
        };
        let mut runner = GenetMultiRunner::new(state);
        let primary =
            runner.push_forest_projection(dom.clone(), Box::new(chrome_logic(0)) as ChromeLogic);
        Self {
            dom,
            runner,
            projections: vec![primary],
            layouts: vec![crate::ui::RetainedSubtreeLayout::new()],
            appearance: crate::shell_services::AppearanceConfig::default(),
        }
    }

    /// Make sure a projection exists for `slot` (a lens window registering).
    pub fn ensure_slot(&mut self, slot: usize) {
        while self.projections.len() <= slot {
            let next = self.projections.len();
            let id = self.runner.push_forest_projection(
                self.dom.clone(),
                Box::new(chrome_logic(next)) as ChromeLogic,
            );
            self.projections.push(id);
            self.layouts.push(crate::ui::RetainedSubtreeLayout::new());
        }
    }

    /// How many full cascades one window-root has paid for.
    ///
    /// The retention contract in one number: it reaches 1 and stays there
    /// while the chrome is merely repainted. Exposed so a receipt can assert
    /// retention rather than infer it from a duration, which would make the
    /// guard a benchmark and therefore flaky.
    pub fn layout_rebuilds(&self, slot: usize) -> u32 {
        self.layouts.get(slot).map_or(0, |layout| layout.rebuilds())
    }

    /// Drain the shared DOM once and stale every root that reads it.
    ///
    /// This is the whole reason [`crate::ui::RetainedSubtreeLayout`] does not
    /// drain for itself. The chrome is N window-roots over ONE `ScriptedDom`,
    /// so a per-root drain would let whichever root ran first consume the
    /// batch and leave the rest painting a tree that no longer exists. Draining
    /// here, and invalidating every root when the batch is non-empty, cannot
    /// produce that split.
    ///
    /// Every entry point that reads the layout calls this, not just `sync`: a
    /// click dispatch mutates the DOM between frames, and a lens window can
    /// render in a frame where the primary did not sync.
    fn absorb_dom_mutations(&mut self) {
        let mut mutations = Vec::new();
        self.dom.borrow_mut().drain_mutations(&mut mutations);
        if mutations.is_empty() {
            return;
        }
        for layout in &mut self.layouts {
            layout.invalidate();
        }
    }

    /// Mirror app truth into the chrome state and rebuild every projection
    /// (one update, every window re-reads — the one-state contract). The
    /// caller passes each live window's size, slot-indexed.
    pub fn sync(&mut self, app: &App, sizes: &[(usize, f32, f32)]) {
        for &(slot, _, _) in sizes {
            self.ensure_slot(slot);
        }
        let caption = crate::app::focused_caption(&app.graph_runtimes);
        let omnibar: &OmnibarState = &app.omnibar;
        let mut omnibar_input = TextInput::new(omnibar.presented_text());
        omnibar_input.set_caret_byte(omnibar.presented_cursor(), false);
        if let Some(preedit) = omnibar.presented_preedit() {
            omnibar_input.set_preedit(preedit);
        }
        let chrome = app.shell_chrome_config();
        let rows: Vec<RowView> = omnibar
            .suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| RowView {
                text: row_text(s),
                class: match s {
                    Suggestion::Hint(_) | Suggestion::Prompt(_) => "omni-row-muted",
                    _ if is_install_review(s) && i == omnibar.selected => {
                        "omni-row-sel omni-row-review"
                    }
                    _ if is_install_review(s) => "omni-row omni-row-review",
                    _ if i == omnibar.selected => "omni-row-sel",
                    _ => "omni-row",
                },
                commit: match s {
                    Suggestion::Hint(_) | Suggestion::Prompt(_) => None,
                    _ => Some(i),
                },
            })
            .collect();
        let open = omnibar.open;
        let focused = app.graph_runtimes.focused_member();
        let focused_kept = focused.is_some_and(|member| app.node_is_kept(member));
        let can_back = app.focused_can_back();
        let can_forward = app.focused_can_forward();
        let fetching = focused.is_some_and(|node| app.content.fetch_in_progress(node));
        let fetch_status = focused.and_then(|node| match app.content.fetch_phase(node) {
            Some(crate::content::PageFetchPhase::Requested) => Some("Requested".to_string()),
            Some(crate::content::PageFetchPhase::Streaming { received_bytes, .. }) => {
                Some(format!("Streaming  {received_bytes} bytes"))
            }
            Some(crate::content::PageFetchPhase::Loading { progress_millis }) => {
                Some(match progress_millis {
                    Some(value) => format!("Loading  {}%", value / 10),
                    None => "Loading".to_string(),
                })
            }
            Some(crate::content::PageFetchPhase::Stopped { received_bytes }) => {
                Some(format!("Stopped  {received_bytes} bytes"))
            }
            Some(crate::content::PageFetchPhase::Settled { .. }) | None => None,
        });
        let focused_url = app.graph_runtimes.focused_url().map(str::to_string);
        let link_preview = app.link_preview.clone();
        let mut find_input = TextInput::new(app.document_find.query.clone());
        find_input.set_caret_byte(app.document_find.query.len(), false);
        let find_open = app.document_find.open;
        let find_status = app.document_find.status();
        self.runner.update(|state| {
            for &(slot, w, h) in sizes {
                while state.windows.len() <= slot {
                    state.windows.push(WindowChrome::default());
                }
                let win = &mut state.windows[slot];
                win.w = w;
                win.h = h;
                win.caption = caption.clone();
                win.primary = slot == 0;
            }
            state.open = open;
            state.omnibar_input = omnibar_input.clone();
            state.rows = rows.clone();
            state.omnibar_placement = chrome.omnibar.placement.clone();
            state.shellbar_placement = chrome.shellbar.placement.clone();
            state.shellbar_visible = chrome.projects_shellbar();
            state.can_back = can_back;
            state.can_forward = can_forward;
            state.focused_node = focused;
            state.focused_kept = focused_kept;
            state.fetching = fetching;
            state.fetch_status = fetch_status.clone();
            state.focused_url = focused_url.clone();
            state.link_preview = link_preview.clone();
            state.find_open = find_open;
            state.find_input = find_input.clone();
            state.find_status = find_status.clone();
        });
        self.appearance = chrome.appearance.clone();
        self.absorb_dom_mutations();
    }

    /// One window's chrome scene: ITS window-root laid out at its own size
    /// through a per-root Livery/Buckram subtree cascade — the true forest-dom F2
    /// path. (This replaced the `display: none` visibility flip the chrome
    /// bridged with while the retired genet-stylo sharing cache panicked on
    /// subtree roots; the fix published as 0.19.1 and the flip is gone.)
    pub fn scene(&mut self, slot: usize, w: u32, h: u32) -> netrender::Scene {
        self.absorb_dom_mutations();
        let Some(&id) = self.projections.get(slot) else {
            return netrender::Scene::new(w, h);
        };
        let Some(root) = self.runner.window_root(id) else {
            return netrender::Scene::new(w, h);
        };
        let sheet = crate::ui::chrome_sheet(&self.appearance);
        let dom = self.dom.borrow();
        let Some(layout) = self.layouts.get_mut(slot) else {
            return netrender::Scene::new(w, h);
        };
        layout.scene(&dom, root, &sheet, w, h)
    }

    /// Route a click at window-local `(x, y)` into `slot`'s chrome: hit-test
    /// that window-root's own subtree layout, dispatch through the runner,
    /// return what bubbled (a row commit).
    pub fn click(&mut self, slot: usize, x: f32, y: f32, w: u32, h: u32) -> Vec<ChromeIntent> {
        let Some(&id) = self.projections.get(slot) else {
            return Vec::new();
        };
        self.absorb_dom_mutations();
        let hit = {
            let Some(root) = self.runner.window_root(id) else {
                return Vec::new();
            };
            let sheet = crate::ui::chrome_sheet(&self.appearance);
            let dom = self.dom.borrow();
            let Some(layout) = self.layouts.get_mut(slot) else {
                return Vec::new();
            };
            layout.hit_test(&dom, root, &sheet, w, h, x, y)
        };
        match hit {
            Some(node) => self
                .runner
                .dispatch_click(id, node, PointerClick::at((x, y))),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use layout_dom_api::LayoutDom;

    use super::*;
    use crate::action::Action;

    fn open_omnibar_app() -> App {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://alpha".to_string()));
        app.update(Action::OmnibarOpen { command: true });
        app.update(Action::OmnibarChar('r'));
        app
    }

    fn long_install_review_app() -> (App, String) {
        let watch_url = format!(
            "https://watch.example/{}",
            "averyverylongunbrokenwatchtarget".repeat(6)
        );
        let mut app = App::test_stub();
        app.viewport = (1024.0, 320.0);
        app.pending_install = Some(crate::denizen::PendingInstall {
            path: std::path::PathBuf::from("watcher.lua"),
            label: "watcher".into(),
            body: crate::denizen::PackBody::Scenario("mere.snapshot()".into()),
            subject: servitor::Subject::new([7; 32]),
            rings: crate::denizen::default_rings(),
            watch_url: Some(watch_url.clone()),
            deadband: None,
        });
        app.update(Action::OmnibarOpen { command: true });
        (app, watch_url)
    }

    fn browser_history_app(settled: bool) -> App {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("gemini://capsule.test/a".into()));
        let node = app.graph_runtimes.focused_member().expect("focused node");
        let request = app.content.active_fetch(node).expect("initial fetch");
        assert!(app.content.settle_fetch(node, request));
        app.update(Action::ContentNavigationCommitted {
            member: node,
            url: "gemini://capsule.test/b".into(),
        });
        app.update(Action::ContentNavigationCommitted {
            member: node,
            url: "gemini://capsule.test/c".into(),
        });
        app.update(Action::NavBack);
        if settled {
            let request = app.content.active_fetch(node).expect("Back fetch");
            assert!(app.content.settle_fetch(node, request));
        }
        app
    }

    fn browser_button_intents(chrome: &mut ChromeSurfaces) -> Vec<ChromeIntent> {
        let rects = {
            let dom = chrome.dom.borrow();
            let root = chrome
                .runner
                .window_root(chrome.projections[0])
                .expect("the primary window-root exists");
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            dom.all_with_class(dom.document(), "browser-button")
                .into_iter()
                .map(|button| {
                    crate::ui::subtree_node_rect(&dom, root, button, &sheet, 1024, 600)
                        .expect("browser button has a rect")
                })
                .collect::<Vec<_>>()
        };
        rects
            .into_iter()
            .flat_map(|(x, y, w, h)| chrome.click(0, x + w / 2.0, y + h / 2.0, 1024, 600))
            .collect()
    }

    #[test]
    fn browser_strip_routes_per_node_back_forward_and_reload() {
        let app = browser_history_app(true);
        assert!(app.focused_can_back() && app.focused_can_forward());
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert_eq!(
            browser_button_intents(&mut chrome),
            vec![
                ChromeIntent::NavBack,
                ChromeIntent::NavForward,
                ChromeIntent::Reload,
                ChromeIntent::KeepNode(app.graph_runtimes.focused_member().expect("focused node")),
            ]
        );
    }

    #[test]
    fn active_fetch_switches_reload_to_stop_and_projects_link_preview() {
        let mut app = browser_history_app(false);
        app.set_link_preview(Some("gemini://capsule.test/prospective".into()));
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        {
            let dom = chrome.dom.borrow();
            assert_eq!(
                dom.all_with_class(dom.document(), "browser-loading").len(),
                1
            );
            assert_eq!(dom.all_with_class(dom.document(), "link-preview").len(), 1);
        }
        assert_eq!(
            browser_button_intents(&mut chrome),
            vec![
                ChromeIntent::NavBack,
                ChromeIntent::NavForward,
                ChromeIntent::Stop,
                ChromeIntent::KeepNode(app.graph_runtimes.focused_member().expect("focused node")),
            ]
        );
    }

    #[test]
    fn document_find_projects_controlled_input_status_and_controls() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("gemini://capsule.test/find".into()));
        let node = app.graph_runtimes.focused_member().expect("focused node");
        app.content.note_live(node, None);
        app.update(Action::OpenDocumentFind);
        app.document_find.query = "needle".into();
        app.document_find.model = crate::action::DocumentFindModel {
            count: 1,
            matches: vec![crate::action::DocumentFindMatch {
                label: "Needle heading".into(),
                role: Some("heading".into()),
            }],
            current: Some(0),
            complete: true,
        };

        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let dom = chrome.dom.borrow();
        assert_eq!(dom.all_with_class(dom.document(), "document-find").len(), 1);
        assert_eq!(
            dom.all_with_class(dom.document(), "document-find-input")
                .len(),
            1
        );
        assert_eq!(
            dom.all_with_class(dom.document(), "document-find-status")
                .len(),
            1
        );
        let enabled = dom.all_with_class(dom.document(), "browser-button").len();
        let disabled = dom
            .all_with_class(dom.document(), "browser-button-disabled")
            .len();
        assert_eq!(
            enabled + disabled,
            7,
            "four browser controls plus previous, next, and close"
        );
    }

    #[test]
    fn keep_button_targets_the_member_then_projects_kept_state() {
        let mut app = browser_history_app(true);
        let member = app.graph_runtimes.focused_member().expect("focused node");
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(browser_button_intents(&mut chrome).contains(&ChromeIntent::KeepNode(member)));

        app.update(Action::KeepNode { member });
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(app.node_is_kept(member));
        assert!(!browser_button_intents(&mut chrome).contains(&ChromeIntent::KeepNode(member)));
        let dom = chrome.dom.borrow();
        assert_eq!(
            dom.all_with_class(dom.document(), "browser-button-disabled")
                .len(),
            1,
            "the one-way control remains visible as disabled Kept state"
        );
    }

    #[test]
    fn hidden_shellbar_is_removed_from_the_chrome_and_a11y_projections() {
        let mut app = open_omnibar_app();
        let mut config = app.shell_chrome_config().clone();
        config.shellbar.visible = false;
        app.set_shell_chrome_config(config);
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let dom = chrome.dom.borrow();
        assert!(dom.all_with_class(dom.document(), "whereami").is_empty());
        drop(dom);
        assert!(
            !crate::a11y::a11y_lines(&app)
                .iter()
                .any(|line| line.starts_with("label: ")),
            "the hidden shellbar cannot leave an inaccessible caption behind"
        );
    }

    #[test]
    fn live_appearance_is_applied_to_the_retained_chrome_surface() {
        let mut app = open_omnibar_app();
        let mut config = app.shell_chrome_config().clone();
        config.appearance.theme_id = Some("theme:night".into());
        config.appearance.theme_mode = crate::shell_services::ThemeMode::Light;
        config.appearance.ui_zoom = 1.5;
        app.set_shell_chrome_config(config);

        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert_eq!(chrome.appearance, app.shell_chrome_config().appearance);
        assert!(
            crate::ui::chrome_sheet(&chrome.appearance).contains("24.00px"),
            "the retained chrome is laid out at the live zoom"
        );
        assert!(
            !chrome.scene(0, 1024, 600).ops.is_empty(),
            "the live appearance stylesheet drives the rendered chrome surface"
        );
    }

    /// The forest topology: primary + a lens chrome are sibling window-roots
    /// of ONE document, and each window's scene lays out only ITS subtree —
    /// the omnibar card exists under the primary root and not the lens's.
    #[test]
    fn chrome_windows_are_forest_siblings_with_scoped_layout() {
        let app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0), (1, 800.0, 500.0)]);
        let dom = chrome.dom.borrow();
        let doc = dom.document();
        let roots: Vec<_> = dom.dom_children(doc).collect();
        assert_eq!(roots.len(), 2, "two window-roots under one document");
        // The omnibar card renders under the PRIMARY root only.
        let cards = dom.all_with_class(doc, "omni");
        assert_eq!(cards.len(), 1, "one omnibar card in the whole forest");
        let chips = dom.all_with_class(doc, "whereami");
        assert_eq!(chips.len(), 2, "both windows carry the caption chip");
        drop(dom);
        // Scoped scenes: the primary's has content; the lens's renders too
        // (its chip), from its own root at its own size.
        let primary = chrome.scene(0, 1024, 600);
        let lens = chrome.scene(1, 800, 500);
        assert!(!primary.ops.is_empty(), "primary chrome paints");
        assert!(!lens.ops.is_empty(), "lens chrome paints its chip");
    }

    /// A suggestion-row click bubbles its ORIGINAL index — the new capability
    /// the retained view adds over the hand chrome.
    #[test]
    fn clicking_a_suggestion_row_bubbles_its_index() {
        let app = open_omnibar_app();
        assert!(
            !app.omnibar.suggestions.is_empty(),
            "the '>r' query filters the registry to something"
        );
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        // Resolve the first selectable row's centre off the laid-out chrome
        // (the primary root's OWN subtree layout, the per-window pass the
        // shell runs).
        let (x, y) = {
            let dom = chrome.dom.borrow();
            let root = chrome
                .runner
                .window_root(chrome.projections[0])
                .expect("the primary window-root exists");
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            let row = dom
                .all_with_class(dom.document(), "omni-row-sel")
                .into_iter()
                .next()
                .expect("a selected row is drawn");
            let (rx, ry, rw, rh) = crate::ui::subtree_node_rect(&dom, root, row, &sheet, 1024, 600)
                .expect("row has a rect");
            (rx + rw / 2.0, ry + rh / 2.0)
        };
        let intents = chrome.click(0, x, y, 1024, 600);
        assert_eq!(intents, vec![ChromeIntent::CommitRow(0)]);
    }

    #[test]
    fn install_review_wraps_intact_and_keeps_card_and_click_geometry() {
        let (app, watch_url) = long_install_review_app();
        let review = app
            .omnibar
            .suggestions
            .iter()
            .find(|suggestion| is_install_review(suggestion))
            .expect("the pending install review is visible");
        let expected = row_text(review);
        assert!(
            crate::ui::chrome_row_width(&expected) > crate::ui::ROW_TEXT_BUDGET,
            "the fixture must actually overflow an ordinary one-line row"
        );
        assert!(
            expected.contains(&watch_url),
            "the displayed review is intact"
        );
        assert!(
            crate::a11y::a11y_lines(&app)
                .iter()
                .any(|line| line == &format!("button: {expected}")),
            "accessibility carries the same complete review"
        );

        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 320.0)]);
        let (x, y) = {
            let dom = chrome.dom.borrow();
            let root = chrome
                .runner
                .window_root(chrome.projections[0])
                .expect("the primary window-root exists");
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            let row = dom
                .all_with_class(dom.document(), "omni-row-review")
                .into_iter()
                .next()
                .expect("the review has its wrapping class");
            let (rx, ry, rw, rh) = crate::ui::subtree_node_rect(&dom, root, row, &sheet, 1024, 320)
                .expect("review has a rect");
            assert!(
                rw <= CARD_W,
                "the wrapped review stays within the card: {rw}px"
            );
            assert!(
                rh > 44.0,
                "emergency breaking takes the URL beyond the two natural URL lines: {rh}px"
            );

            let card = dom
                .all_with_class(dom.document(), "omni")
                .into_iter()
                .next()
                .expect("the omnibar card exists");
            let (_, cy, _, ch) = crate::ui::subtree_node_rect(&dom, root, card, &sheet, 1024, 320)
                .expect("card has a rect");
            assert!(
                cy + ch <= 320.0 - 16.0,
                "the row budget leaves the expanded card inside the viewport"
            );
            (rx + rw / 2.0, ry + rh / 2.0)
        };
        assert_eq!(
            chrome.click(0, x, y, 1024, 320),
            vec![ChromeIntent::CommitRow(0)],
            "the expanded row remains one clickable review"
        );
    }

    /// The caret split mirrors the omnibar state: before/caret/after and the
    /// preedit ride the retained DOM (the receipt-proven IME honesty, kept).
    #[test]
    fn the_caret_split_mirrors_omnibar_state() {
        let mut app = App::test_stub();
        app.update(Action::OmnibarOpen { command: false });
        for c in "abd".chars() {
            app.update(Action::OmnibarChar(c));
        }
        app.update(Action::OmnibarCaret(crate::action::CaretMove::Left));
        app.omnibar.preedit = Some("c".to_string());
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        let dom = chrome.dom.borrow();
        let input = dom
            .all_with_class(dom.document(), "omni-input")
            .into_iter()
            .next()
            .expect("the input line is drawn");
        let texts: Vec<String> = dom
            .dom_children(input)
            .flat_map(|c| {
                let mut out = Vec::new();
                if let Some(t) = dom.text(c) {
                    out.push(t.to_string());
                }
                out.extend(
                    dom.dom_children(c)
                        .filter_map(|g| dom.text(g).map(str::to_string)),
                );
                out
            })
            .collect();
        let joined = texts.join("|");
        assert!(joined.contains("ab"), "text before the caret: {joined}");
        assert!(joined.contains('c'), "the preedit rides inline: {joined}");
        assert!(
            joined.contains('\u{258d}'),
            "the caret glyph is at the split: {joined}"
        );
        assert!(joined.contains('d'), "text after the caret: {joined}");
    }

    /// The retention contract: a chrome that nothing has changed re-paints
    /// without re-cascading. This is the unit-level guard for the command
    /// palette open-lag receipt, which measured the unretained path at 36 ms
    /// of a 44 ms frame.
    #[test]
    fn an_unchanged_chrome_paints_without_cascading_again() {
        let app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);

        assert!(!chrome.scene(0, 1024, 600).ops.is_empty());
        assert_eq!(chrome.layout_rebuilds(0), 1, "the first frame cascades");

        // Idle frames: sync still runs (the shell calls it every frame), but
        // it writes no mutations, so nothing is invalidated.
        for _ in 0..5 {
            chrome.sync(&app, &[(0, 1024.0, 600.0)]);
            assert!(!chrome.scene(0, 1024, 600).ops.is_empty());
        }
        assert_eq!(
            chrome.layout_rebuilds(0),
            1,
            "five idle frames must not re-cascade"
        );

        // A resize is a per-root fact the layout detects for itself.
        assert!(!chrome.scene(0, 800, 600).ops.is_empty());
        assert_eq!(chrome.layout_rebuilds(0), 2, "a resize re-cascades");
    }

    /// A state change reaches the layout. The retention must not be so eager
    /// that a changed chrome keeps painting the old cascade.
    #[test]
    fn a_changed_chrome_cascades_again() {
        let mut app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        chrome.scene(0, 1024, 600);
        assert_eq!(chrome.layout_rebuilds(0), 1);

        app.update(Action::OmnibarChar('e'));
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        chrome.scene(0, 1024, 600);
        assert_eq!(
            chrome.layout_rebuilds(0),
            2,
            "an edited omnibar re-cascades"
        );
    }

    /// The forest hazard, stated as a test: N window-roots share ONE DOM and
    /// therefore ONE mutation queue. A per-root drain would let whichever root
    /// rendered first swallow the batch, leaving every other window painting a
    /// tree that no longer exists — stale, not merely slow. The drain lives in
    /// `absorb_dom_mutations` for exactly this reason, and this asserts that a
    /// single change reaches BOTH roots.
    #[test]
    fn one_mutation_invalidates_every_window_root() {
        let mut app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        let sizes = [(0, 1024.0, 600.0), (1, 800.0, 500.0)];

        chrome.sync(&app, &sizes);
        chrome.scene(0, 1024, 600);
        chrome.scene(1, 800, 500);
        assert_eq!(chrome.layout_rebuilds(0), 1);
        assert_eq!(chrome.layout_rebuilds(1), 1);

        // The primary renders FIRST, which is precisely the ordering that
        // would hide a per-root drain bug.
        app.update(Action::OmnibarChar('e'));
        chrome.sync(&app, &sizes);
        chrome.scene(0, 1024, 600);
        chrome.scene(1, 800, 500);
        assert_eq!(chrome.layout_rebuilds(0), 2, "the primary re-cascaded");
        assert_eq!(
            chrome.layout_rebuilds(1),
            2,
            "the lens re-cascaded too, rather than losing the batch to the primary"
        );
    }

    /// A pointer press hit-tests the layout the frame painted, and pays no
    /// cascade to do it. Before retention this path rebuilt the whole subtree
    /// on every press, including presses that miss chrome entirely.
    #[test]
    fn a_press_hit_tests_without_cascading_again() {
        let app = open_omnibar_app();
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        chrome.scene(0, 1024, 600);
        assert_eq!(chrome.layout_rebuilds(0), 1);

        // A press well away from any chrome control: it still hit-tests, and
        // it must not provoke a cascade to answer "nothing here".
        let intents = chrome.click(0, 5.0, 590.0, 1024, 600);
        assert!(
            intents.is_empty(),
            "a press on empty chrome commits nothing"
        );
        assert_eq!(
            chrome.layout_rebuilds(0),
            1,
            "hit testing shares the frame's layout instead of building its own"
        );
    }
}
