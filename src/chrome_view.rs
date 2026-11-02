// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
    TextInput, caret_field_children, el, on_click, status_note,
};
use genet_scripted_dom::ScriptedDom;
// `drain_mutations` is the mutation queue's only reader; the chrome owns that
// drain for the whole forest (see `absorb_dom_mutations`).
use layout_dom_api::{LayoutDom, LayoutDomMut};

use crate::app::App;
use crate::panes::{ChromeEdge, ChromePlacement};
use crate::ui::{CARD_TOP, CARD_W, OmnibarState, Suggestion};

/// What a chrome interaction produces: commit the suggestion row at this
/// original index (the shell lowers `Action::OmnibarCommitRow`).
#[derive(Clone, Debug, PartialEq)]
pub enum ChromeIntent {
    CommitRow(usize),
    /// Keep or drop the command on this suggestion row.
    ToggleKeep(usize),
    Scope(crate::scope_navigation::ScopeCommand),
    FocusCard(crate::focus_card::Command),
    NavBack,
    NavForward,
    Reload,
    Stop,
    KeepNode(uuid::Uuid),
    Address,
    WorkspaceMap(bool),
    Tool(&'static str),
    ReadMember(uuid::Uuid),
    NavigateRelationship(crate::relationship_navigation::RelationshipTarget),
    RevealRelationship(crate::relationship_navigation::RelationshipTarget),
    RevealMember(uuid::Uuid),
    Layout(crate::chrome_layout::ChromeLayout),
    AllPanes,
    Territory,
    FitGraph,
    MenuChanged,
    /// What the mounted find field reported: a step, a close, or an edit.
    Find(cambium::ReadEvent),
    Permission {
        request: crate::user_agent_decision::UserAgentRequestKey,
        choice: crate::user_agent_decision::PermissionChoice,
    },
    FocusAuthentication(crate::user_agent_decision::AuthenticationField),
    ToggleAuthenticationMemory,
    SubmitAuthentication(crate::user_agent_decision::UserAgentRequestKey),
    CancelAuthentication(crate::user_agent_decision::UserAgentRequestKey),
}

/// How one suggestion row renders.
#[derive(Clone, Debug, PartialEq)]
struct RowView {
    text: String,
    /// Class: selected / plain / muted (hints are inert).
    class: &'static str,
    /// The row's index in `OmnibarState::suggestions` (what a click commits);
    /// `None` for inert hint rows.
    commit: Option<usize>,
    /// For a command row, whether the person keeps it; it then carries a
    /// Keep/Drop control beside its label.
    keep: Option<bool>,
}

#[derive(Clone, PartialEq)]
enum DecisionCard {
    Permission {
        request: crate::user_agent_decision::UserAgentRequestKey,
        prompt: String,
        queued: usize,
        submitting: bool,
        error: Option<String>,
    },
    Authentication {
        request: crate::user_agent_decision::UserAgentRequestKey,
        prompt: String,
        queued: usize,
        username: TextInput,
        password: TextInput,
        field: crate::user_agent_decision::AuthenticationField,
        remember_for_process: bool,
        submitting: bool,
        error: Option<String>,
    },
}

/// One window's chrome inputs: its size and its caption. The chrome is laid
/// out in layout pixels: `w` and `h` are the window's physical size over
/// `scale`, the layout scale its retained layout rasterizes at.
#[derive(Clone, Debug, Default, PartialEq)]
struct WindowChrome {
    w: f32,
    h: f32,
    caption: Option<String>,
    /// Only the primary window carries the omnibar; lenses show the chip.
    primary: bool,
    titlebar_insets: cambium_rootstock::TitlebarInsets,
    scale: f32,
}

/// The one chrome state every projection renders (the one-state contract).
struct ChromeState {
    forme_fields: std::collections::BTreeMap<usize, Vec<FormeFieldControl>>,
    focus_cards: Vec<crate::focus_card::CardView>,
    peek_selected: Option<crate::focus_card::Target>,
    peek_below: Option<crate::focus_card::Target>,
    peek_forme: Option<crate::focus_card::Target>,
    relation_targets: Vec<(
        crate::relationship_navigation::RelationshipTarget,
        crate::focus_card::Target,
    )>,
    menu: Option<ChromeMenu>,
    menu_selected: usize,
    documents_start: usize,
    relationships_start: usize,
    layout: crate::chrome_layout::ChromeLayout,
    map_active: bool,
    territory: bool,
    tools: Vec<(&'static str, String, bool)>,
    connections: Vec<crate::relationship_navigation::RelationshipRow>,
    documents: Vec<(uuid::Uuid, String, bool)>,
    reading_stacks: Vec<Vec<(uuid::Uuid, String, bool)>>,
    windows: Vec<WindowChrome>,
    open: bool,
    inline_address: bool,
    /// App-owned omnibar truth projected into Cambium's pure caret renderer.
    /// This runner never dispatches edit keys into it.
    omnibar_input: TextInput,
    rows: Vec<RowView>,
    omnibar_placement: ChromePlacement,
    shellbar_placement: ChromePlacement,
    shellbar_visible: bool,
    zoom: f32,
    scope_controls: cambium::scope_navigation::ScopeControls,
    scope_crumbs: Vec<crate::scope_navigation::ScopeCrumb>,
    can_back: bool,
    can_forward: bool,
    focused_node: Option<uuid::Uuid>,
    focused_kept: bool,
    fetching: bool,
    fetch_status: Option<String>,
    focused_url: Option<String>,
    link_preview: Option<String>,
    find: cambium::ReadFind,
    decision: Option<DecisionCard>,
}

#[derive(Clone, Debug, PartialEq)]
struct FormeFieldControl {
    pane: crate::panes::PaneId,
    field: cambium::ScopeField,
    target: crate::focus_card::Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChromeMenu {
    Panes,
    Connections,
    Documents,
}

type ChromeView = Box<dyn AnyView<ChromeState, ChromeIntent, GenetCtx, GenetElement>>;

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
    card_leaves: sprigging::LeafRegistry<u64>,
    card_paint: sprigging::RenderedLeaves,
}

#[cfg(test)]
#[path = "chrome_view/tests.rs"]
pub(crate) mod tests;

#[path = "chrome_view/impl_cambium_action_for_chrome_intent.rs"]
mod impl_cambium_action_for_chrome_intent;
#[path = "chrome_view/impl_std_fmt_debug_for_decision.rs"]
mod impl_std_fmt_debug_for_decision;
#[path = "chrome_view/scope_chrome_intent.rs"]
mod scope_chrome_intent;
use scope_chrome_intent::scope_chrome_intent;
#[path = "chrome_view/context_cards_visible.rs"]
mod context_cards_visible;
use context_cards_visible::context_cards_visible;
#[path = "chrome_view/field_controls_visible.rs"]
mod field_controls_visible;
use field_controls_visible::field_controls_visible;
#[path = "chrome_view/window_chrome_view.rs"]
mod window_chrome_view;
use window_chrome_view::window_chrome_view;
#[path = "chrome_view/window_title_bar.rs"]
mod window_title_bar;
use window_title_bar::window_title_bar;
#[path = "chrome_view/titlebar_height.rs"]
mod titlebar_height;
pub(crate) use titlebar_height::titlebar_height;
#[path = "chrome_view/browser_button.rs"]
mod browser_button;
use browser_button::browser_button;
#[path = "chrome_view/browser_control.rs"]
mod browser_control;
use browser_control::browser_control;
#[path = "chrome_view/selected_button.rs"]
mod selected_button;
use selected_button::selected_button;
#[path = "chrome_view/menu_button.rs"]
mod menu_button;
use menu_button::menu_button;
#[path = "chrome_view/activity_button.rs"]
mod activity_button;
use activity_button::activity_button;
#[path = "chrome_view/navigation_height.rs"]
mod navigation_height;
pub(crate) use navigation_height::navigation_height;
#[path = "chrome_view/status_height.rs"]
mod status_height;
use status_height::status_height;
#[path = "chrome_view/workspace_area.rs"]
mod workspace_area;
pub(crate) use workspace_area::workspace_area;
#[path = "chrome_view/workspace_area_for_window.rs"]
mod workspace_area_for_window;
pub(crate) use workspace_area_for_window::workspace_area_for_window;
#[path = "chrome_view/chrome_position.rs"]
mod chrome_position;
pub(crate) use chrome_position::chrome_position;
#[path = "chrome_view/chrome_logic.rs"]
mod chrome_logic;
use chrome_logic::chrome_logic;
#[path = "chrome_view/row_text.rs"]
mod row_text;
pub(crate) use row_text::row_text;
#[path = "chrome_view/is_install_review.rs"]
mod is_install_review;
pub(crate) use is_install_review::is_install_review;
#[path = "chrome_view/append_window_navigation.rs"]
mod append_window_navigation;
#[path = "chrome_view/chrome_surfaces_click.rs"]
mod chrome_surfaces_click;
#[path = "chrome_view/chrome_surfaces_selector_point.rs"]
mod chrome_surfaces_selector_point;
#[path = "chrome_view/chrome_surfaces_sync.rs"]
mod chrome_surfaces_sync;
use append_window_navigation::append_window_navigation;

struct NavigationGeometry {
    zoom: f32,
    title_height: f32,
    navigation_height: f32,
    navigation_top: f32,
    height: f32,
}
#[path = "chrome_view/navigation_menu.rs"]
mod navigation_menu;
use navigation_menu::append_navigation_menu;
