// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Input routing: pointer, wheel, keys, and the drag gestures, shared by winit
//! and the scenario runner so one description drives two runners.
//!
//! Focus picks the lane, for keys as for the pointer: the omnibar when open,
//! the focused page when one holds focus, the canvas otherwise. Ephemeral
//! input (scroll, hover, blur) rides state directly per the gesture law;
//! durable intent becomes an `Action`.

use super::{NodeSessions, NodeSurfaces};
use winit::event::{Force, MouseButton, Touch, TouchPhase};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::CursorIcon;

use crate::panes::PaneContent;
use inker::{SessionClick, SessionScrollKey};
use mere::canvas::PointerButton;
use taproot::AutomatableExt as _;

use crate::action::{Action, CaretMove};
use crate::surface::Rect;

use super::{Shell, decode_sprite};

/// The canvas's `PointerButton` for a winit `MouseButton`, or `None` for
/// buttons the canvas does not handle.
pub(super) fn pointer_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Left),
        MouseButton::Middle => Some(PointerButton::Middle),
        MouseButton::Right => Some(PointerButton::Right),
        _ => None,
    }
}

pub(super) fn surface_mouse_button(button: MouseButton) -> Option<inker::MouseButton> {
    match button {
        MouseButton::Left => Some(inker::MouseButton::Left),
        MouseButton::Middle => Some(inker::MouseButton::Middle),
        MouseButton::Right => Some(inker::MouseButton::Right),
        MouseButton::Back => Some(inker::MouseButton::Back),
        MouseButton::Forward => Some(inker::MouseButton::Forward),
        _ => None,
    }
}

fn surface_button_mask(button: MouseButton) -> inker::PointerButtons {
    match button {
        MouseButton::Left => inker::PointerButtons::PRIMARY,
        MouseButton::Right => inker::PointerButtons::SECONDARY,
        MouseButton::Middle => inker::PointerButtons::AUXILIARY,
        MouseButton::Back => inker::PointerButtons::BACK,
        MouseButton::Forward => inker::PointerButtons::FORWARD,
        _ => inker::PointerButtons::NONE,
    }
}

pub(super) fn mouse_pointer_event(
    x: f32,
    y: f32,
    phase: inker::PointerPhase,
    button: Option<inker::MouseButton>,
    buttons: inker::PointerButtons,
    modifiers: inker::KeyboardModifiers,
) -> inker::PointerEvent {
    inker::PointerEvent {
        pointer_id: 1,
        pointer_type: inker::PointerType::Mouse,
        is_primary: true,
        phase,
        position: inker::PhysicalPosition { x, y },
        button,
        buttons,
        width: 1.0,
        height: 1.0,
        pressure: None,
        tangential_pressure: None,
        tilt_x: None,
        tilt_y: None,
        twist: None,
        altitude_angle: None,
        azimuth_angle: None,
        modifiers,
    }
}

fn surface_cursor_icon(shape: inker::CursorShape) -> Option<CursorIcon> {
    match shape {
        inker::CursorShape::Default => Some(CursorIcon::Default),
        inker::CursorShape::Text => Some(CursorIcon::Text),
        inker::CursorShape::Pointer => Some(CursorIcon::Pointer),
        inker::CursorShape::Crosshair => Some(CursorIcon::Crosshair),
        inker::CursorShape::Move => Some(CursorIcon::Move),
        inker::CursorShape::ResizeNs => Some(CursorIcon::NsResize),
        inker::CursorShape::ResizeEw => Some(CursorIcon::EwResize),
        inker::CursorShape::ResizeNesw => Some(CursorIcon::NeswResize),
        inker::CursorShape::ResizeNwse => Some(CursorIcon::NwseResize),
        inker::CursorShape::Grab => Some(CursorIcon::Grab),
        inker::CursorShape::Grabbing => Some(CursorIcon::Grabbing),
        inker::CursorShape::NotAllowed => Some(CursorIcon::NotAllowed),
        inker::CursorShape::Hidden => None,
    }
}

fn surface_cursor_label(shape: inker::CursorShape) -> &'static str {
    match shape {
        inker::CursorShape::Default => "default",
        inker::CursorShape::Text => "text",
        inker::CursorShape::Pointer => "pointer",
        inker::CursorShape::Grab => "grab",
        inker::CursorShape::Grabbing => "grabbing",
        inker::CursorShape::Crosshair => "crosshair",
        inker::CursorShape::Move => "move",
        inker::CursorShape::ResizeNs => "resize-ns",
        inker::CursorShape::ResizeEw => "resize-ew",
        inker::CursorShape::ResizeNesw => "resize-nesw",
        inker::CursorShape::ResizeNwse => "resize-nwse",
        inker::CursorShape::NotAllowed => "not-allowed",
        inker::CursorShape::Hidden => "hidden",
    }
}

#[derive(Clone, Copy)]
pub(super) struct ContributedPointer {
    pub surface: crate::surface::Surface,
    pub generation: u64,
    pub window: u64,
}
impl ContributedPointer {
    pub(super) fn is_current(
        &self,
        surface: Option<crate::surface::Surface>,
        generation: Option<u64>,
        window: u64,
    ) -> bool {
        surface == Some(self.surface)
            && generation == Some(self.generation)
            && window == self.window
    }
}

/// Platform-independent part of the primary native caller. The host supplies
/// placement synchronization and receiver delivery; these shared branches own
/// the shell capture decisions before renderer-specific pane/graph gestures.
/// Keeping the non-Content branch here lets native input fixtures exercise the
/// same fresh-hit capture assignment and terminal-event admission as Shell.
pub(super) trait PrimaryPointerHost {
    fn pointer_state(
        &mut self,
    ) -> (
        &mut crate::surface::FocusTarget,
        &mut super::window_input::Pool,
        &mut Option<crate::surface::SurfaceKind>,
    );
    fn sync_pointer_placements(&mut self);
    fn pointer_content_focus(&mut self, appearance: crate::surface::SurfaceId);
    fn pointer_content_mouse(
        &mut self,
        appearance: crate::surface::SurfaceId,
        local: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    );
    fn pointer_window_mouse(
        &mut self,
        appearance: Option<crate::surface::SurfaceId>,
        position: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    );
    fn pointer_user_agent_decision_open(&self) -> bool;
    fn pointer_has_foreign_capture(&self) -> bool;
    fn pointer_cancel_primary_content(&mut self);
    fn pointer_has_contributed_capture(&self) -> bool;
    fn pointer_release_contributed(&mut self, x: f32, y: f32);
    fn pointer_content_hovered(&mut self, node: uuid::Uuid);
    fn pointer_redraw(&mut self);
}

/// Portable admission for primary presses consumed before surface dispatch.
/// The host executes the same App action as Shell's chrome/modal handlers.
pub(super) trait PrimaryPressAdmissionHost: PrimaryPointerHost {
    fn pointer_press_action(&mut self, action: Action);
    fn pointer_omnibar_open(&self) -> bool;
    fn pointer_retire_primary_pointers(&mut self);
}

fn dispatch_primary_action_press(host: &mut impl PrimaryPressAdmissionHost, action: Action) {
    // Retire this window's admitted content before an App action can alter
    // focus or replace/drop its receiver. Preserve foreign-window custody.
    let foreign_capture = host.pointer_has_foreign_capture();
    host.pointer_retire_primary_pointers();
    if !foreign_capture {
        *host.pointer_state().2 = None;
    }
    host.pointer_press_action(action);
}

pub(super) fn dispatch_primary_chrome_press(
    host: &mut impl PrimaryPressAdmissionHost,
    intent: Option<crate::chrome_view::ChromeIntent>,
) -> bool {
    let Some(intent) = intent else {
        return false;
    };
    let action = match intent {
        crate::chrome_view::ChromeIntent::CommitRow(index) => Action::OmnibarCommitRow(index),
        crate::chrome_view::ChromeIntent::ToggleKeep(index) => Action::OmnibarToggleKeepRow(index),
        crate::chrome_view::ChromeIntent::NavBack => Action::NavBack,
        crate::chrome_view::ChromeIntent::NavForward => Action::NavForward,
        crate::chrome_view::ChromeIntent::Reload => Action::Reload,
        crate::chrome_view::ChromeIntent::Stop => Action::Stop,
        crate::chrome_view::ChromeIntent::KeepNode(member) => Action::KeepNode { member },
        crate::chrome_view::ChromeIntent::Address => Action::OmnibarOpen { command: false },
        crate::chrome_view::ChromeIntent::WorkspaceMap(map) => Action::SetWorkspaceMap(map),
        crate::chrome_view::ChromeIntent::Tool(kind) => Action::ToggleToolPane(crate::panes::PaneKindId::new(kind)),
        crate::chrome_view::ChromeIntent::ReadMember(member) => Action::ReadGraphMember(member),
        crate::chrome_view::ChromeIntent::NavigateRelationship(target) => Action::NavigateRelationship(target),
        crate::chrome_view::ChromeIntent::RevealMember(member) => Action::RevealGraphMember(member),
        crate::chrome_view::ChromeIntent::Layout(layout) => Action::SetChromeLayout(layout),
        crate::chrome_view::ChromeIntent::FitGraph => {
            dispatch_primary_action_press(host, Action::SetWorkspaceMap(true));
            dispatch_primary_action_press(host, Action::FitView);
            return true;
        },
        crate::chrome_view::ChromeIntent::Territory => Action::ToggleTerritory,
        crate::chrome_view::ChromeIntent::AllPanes => Action::OmnibarOpen { command: true },
        crate::chrome_view::ChromeIntent::MenuChanged => { host.pointer_redraw(); return true; },
        crate::chrome_view::ChromeIntent::FindPrevious => {
            Action::StepDocumentFind(crate::action::DocumentFindDirection::Previous)
        },
        crate::chrome_view::ChromeIntent::FindNext => {
            Action::StepDocumentFind(crate::action::DocumentFindDirection::Next)
        },
        crate::chrome_view::ChromeIntent::FindClose => Action::CloseDocumentFind,
        crate::chrome_view::ChromeIntent::Permission { request, choice } => {
            Action::ChoosePermission { request, choice }
        },
        crate::chrome_view::ChromeIntent::FocusAuthentication(field) => {
            Action::FocusAuthenticationField(field)
        },
        crate::chrome_view::ChromeIntent::ToggleAuthenticationMemory => {
            Action::ToggleAuthenticationMemory
        },
        crate::chrome_view::ChromeIntent::SubmitAuthentication(request) => {
            Action::SubmitAuthentication { request }
        },
        crate::chrome_view::ChromeIntent::CancelAuthentication(request) => {
            Action::CancelAuthentication { request }
        },
    };
    dispatch_primary_action_press(host, action);
    true
}

pub(super) fn reject_primary_omnibar_press(host: &mut impl PrimaryPressAdmissionHost) -> bool {
    if !host.pointer_omnibar_open() {
        return false;
    }
    dispatch_primary_action_press(host, Action::OmnibarClose);
    true
}

pub(super) fn dispatch_primary_context_press(
    host: &mut impl PrimaryPressAdmissionHost,
    button: MouseButton,
    hit: Option<crate::surface::HitResult>,
) -> bool {
    if button != MouseButton::Right
        || hit.is_some_and(|hit| matches!(hit.kind, crate::surface::SurfaceKind::Graph(_)))
    {
        return false;
    }
    dispatch_primary_action_press(host, Action::OmnibarOpen { command: true });
    true
}

impl PrimaryPressAdmissionHost for Shell {
    fn pointer_press_action(&mut self, action: Action) {
        self.act(action);
    }
    fn pointer_omnibar_open(&self) -> bool {
        self.app.omnibar.open
    }
    fn pointer_retire_primary_pointers(&mut self) {
        self.pointer_cancel_primary_content();
    }
}

/// The actual primary UA admission gate, after chrome has offered the press.
/// Retire primary content custody before clearing its shell mirror.
pub(super) fn reject_primary_modal_press(host: &mut impl PrimaryPointerHost) -> bool {
    if !host.pointer_user_agent_decision_open() {
        return false;
    }
    let foreign_capture = host.pointer_has_foreign_capture();
    host.pointer_cancel_primary_content();
    if !foreign_capture {
        *host.pointer_state().2 = None;
    }
    true
}

/// Dispatch a hit admitted by the primary caller's chrome/modal/context gates.
/// A returned hit continues into Shell's pane/graph/divider renderer handling.
pub(super) fn dispatch_primary_press(
    host: &mut impl PrimaryPointerHost,
    hit: Option<crate::surface::HitResult>,
    x: f32,
    y: f32,
    button: MouseButton,
) -> Option<crate::surface::HitResult> {
    // Pool custody wins over a fresh hit, including a position outside the
    // plan, only after current placement/window admission is revalidated.
    host.sync_pointer_placements();
    let hit = {
        let (focus, pool, _) = host.pointer_state();
        let position = inker::PhysicalPosition { x, y };
        super::window_input::captured_content_hit(pool, 0, position)
            .or(hit)
            .map(|hit| super::window_input::content_press_target(focus, pool, 0, position, hit))
    };
    *host.pointer_state().2 = hit.map(|h| h.kind);
    let hit = hit?;
    if !matches!(hit.kind, crate::surface::SurfaceKind::Content(_)) {
        return Some(hit);
    }
    host.pointer_content_focus(hit.id);
    if let Some(button) = surface_mouse_button(button) {
        host.pointer_content_mouse(hit.id, hit.local, inker::PointerPhase::Down, Some(button));
    }
    host.pointer_redraw();
    None
}

pub(super) fn dispatch_primary_content_move(
    host: &mut impl PrimaryPointerHost,
    x: f32,
    y: f32,
) -> bool {
    let Some(crate::surface::SurfaceKind::Content(node)) = *host.pointer_state().2 else {
        return false;
    };
    host.sync_pointer_placements();
    let target = {
        let (_, pool, _) = host.pointer_state();
        super::window_input::captured_mouse(pool).and_then(|id| {
            pool.placement(&id)
                .map(|placement| (id, (x - placement.rect.x, y - placement.rect.y)))
        })
    };
    if let Some((id, local)) = target {
        host.pointer_content_mouse(id, local, inker::PointerPhase::Move, None);
        host.pointer_content_hovered(node);
    }
    true
}

/// Deliver the native terminal event before returning non-content custody to
/// Shell's renderer-specific release handling.
pub(super) fn dispatch_primary_release(
    host: &mut impl PrimaryPointerHost,
    x: f32,
    y: f32,
    button: MouseButton,
) -> std::ops::ControlFlow<(), Option<crate::surface::SurfaceKind>> {
    let appearance = if surface_mouse_button(button).is_some()
        && !host.pointer_has_contributed_capture()
        && matches!(
            *host.pointer_state().2,
            Some(crate::surface::SurfaceKind::Content(_))
        ) {
        host.sync_pointer_placements();
        super::window_input::captured_mouse(host.pointer_state().1)
    } else {
        None
    };
    host.pointer_window_mouse(
        appearance,
        (x, y),
        inker::PointerPhase::Up,
        surface_mouse_button(button),
    );
    if host.pointer_has_contributed_capture() {
        if button == MouseButton::Left {
            host.pointer_release_contributed(x, y);
        }
        return std::ops::ControlFlow::Break(());
    }
    let captured = host.pointer_state().2.take();
    if let Some(crate::surface::SurfaceKind::Content(_)) = captured {
        if super::window_input::captured_mouse(host.pointer_state().1).is_some() {
            *host.pointer_state().2 = captured;
        }
        host.pointer_redraw();
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(captured)
}

/// Admit a Workbench seam before document dispatch, including the invisible
/// part of its grab band. Match appearance custody so another pane or float
/// covering the same coordinates keeps its input.
fn workbench_resize_target(
    app: &crate::app::App,
    plan: &[crate::surface::Surface],
    hit: Option<crate::surface::HitResult>,
    x: f32,
    y: f32,
) -> Option<(crate::panes::PaneId, crate::workbench_tiling::WbDivider, (f32, f32))> {
    use crate::surface::{SurfaceId, SurfaceKind};
    let hit = hit?;
    plan.iter().rev().find_map(|surface| {
        let SurfaceKind::Pane(pane) = surface.kind else { return None };
        if !matches!(app.pane_content(pane), Some(crate::panes::PaneContent::Workbench))
            || !surface.rect.contains(x, y) { return None; }
        let owned = match hit.kind {
            SurfaceKind::Pane(id) => id == pane,
            SurfaceKind::Content(node) => hit.id == SurfaceId::content_appearance(node,
                crate::surface::appearance::workbench_appearance_role(pane, node)),
            _ => false,
        };
        if !owned { return None; }
        let geom = app.workbench_for_pane(pane)?.to_arrangement().1;
        let tiling = crate::workbench_tiling::place_workbench_at(geom.as_ref(),
            Rect::new(0.0, 0.0, surface.rect.w, surface.rect.h),app.shell_chrome_config().layout.document_tabs);
        let divider = tiling.divider_at(x - surface.rect.x, y - surface.rect.y)?.clone();
        Some((pane, divider, (surface.rect.x, surface.rect.y)))
    })
}

impl PrimaryPointerHost for Shell {
    fn pointer_state(
        &mut self,
    ) -> (
        &mut crate::surface::FocusTarget,
        &mut super::window_input::Pool,
        &mut Option<crate::surface::SurfaceKind>,
    ) {
        (
            &mut self.app.focus,
            &mut self.content_sessions.pool,
            &mut self.pointer_capture,
        )
    }
    fn sync_pointer_placements(&mut self) {
        self.sync_content_placements();
    }
    fn pointer_content_focus(&mut self, appearance: crate::surface::SurfaceId) {
        self.content_input(
            appearance,
            pelt_core::PeltInput::Session(inker::SessionInput::Focus(true)),
        );
    }
    fn pointer_content_mouse(
        &mut self,
        appearance: crate::surface::SurfaceId,
        local: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        self.content_mouse(appearance, local, phase, button);
    }
    fn pointer_window_mouse(
        &mut self,
        appearance: Option<crate::surface::SurfaceId>,
        position: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        self.window_mouse_input(0, appearance, position, phase, button);
    }
    fn pointer_user_agent_decision_open(&self) -> bool {
        self.app.user_agent_decision.is_open()
    }
    fn pointer_has_foreign_capture(&self) -> bool {
        super::window_input::captured_mouse(&self.content_sessions.pool)
            .and_then(|id| self.content_sessions.pool.placement(&id))
            .is_some_and(|placement| placement.window != 0)
            || self
                .contributed_pointer
                .is_some_and(|admitted| admitted.window != 0)
    }
    fn pointer_cancel_primary_content(&mut self) {
        // Interception retires custody; only physical Up changes button masks.
        self.touch_input
            .cancel_window(&mut self.content_sessions.pool, 0);
    }
    fn pointer_has_contributed_capture(&self) -> bool {
        self.contributed_pointer.is_some()
    }
    fn pointer_release_contributed(&mut self, x: f32, y: f32) {
        self.release_contributed_pointer(0, x, y);
    }
    fn pointer_content_hovered(&mut self, node: uuid::Uuid) {
        self.hovered_surface = Some(node);
        self.apply_pending_surface_cursor();
    }
    fn pointer_redraw(&mut self) {
        self.request_redraw();
    }
}

impl Shell {
    /// One aim that resolved to nothing, and why.
    ///
    /// The open omnibar is the cause worth naming: it takes the press before
    /// any pane sees it, so a scenario reads "no row matched" and looks for a
    /// missing row that is right there behind the chrome.
    fn note_miss(&mut self, what: &'static str, target: &str, missed: &str) {
        let swallowed = self.app.omnibar.open;
        self.app.note(crate::observe::AppEvent::InteractionMissed {
            what,
            target: target.to_string(),
        });
        if swallowed {
            tracing::warn!(%target, "{what}: the open omnibar swallowed the press");
        } else {
            tracing::warn!(%target, "{what}: {missed}");
        }
    }

    pub(super) fn click_pane_row(&mut self, substr: &str) {
        // Both list panes resolve through the shared driver's `click`: a Trail
        // `list-row` or a grid `roster-cell` whose text contains `substr`, over
        // all surfaces at once (no per-pane dispatch). Short-circuit `||` means a
        // hit presses once; only a total miss is attributable.
        let hit = self.click(&taproot::Selector::class("radio")
            .with_attr("data-engine-id", substr)
            .on_surface("inspector"))
            || self.click(&taproot::Selector::class("roster-cell").containing(substr))
            || self.click(&taproot::Selector::class("list-row").containing(substr))
            // A settings option is a row for receipt purposes (the Inspector
            // pane's radio options).
            || self.click(&taproot::Selector::class("radio").containing(substr))
            || self.click(&taproot::Selector::class("setting-apply").containing(substr))
            // A composed list section's row (the gloss-composite): the same
            // verb addresses it, wherever the section was composed.
            || self.click(&taproot::Selector::class("section-row").containing(substr));
        if !hit {
            self.note_miss("click-row", substr, "no list-pane row matched");
        }
    }

    /// Click the Roster's tab labelled `label` (the scenario's `click-tab`),
    /// through the shared driver: a `.tab` element whose text is `label`. The
    /// strip's geometry is the layout's to know; the host names the target and
    /// the resolver finds it — the same substrate every genet app shares.
    pub(super) fn click_pane_tab(&mut self, label: &str) {
        if !self.click(&taproot::Selector::class("tab").containing(label)) {
            self.note_miss("click-tab", label, "no Roster tab matched");
        }
    }

    /// Click the Gloss minimap's node matching `substr` (the scenario's
    /// `click-node`), through the shared driver. The node buttons carry their url
    /// as `data-key`, so the driver selects on it — unique where the display
    /// label (two "Example Domain" pages) is not.
    pub(super) fn click_pane_node(&mut self, substr: &str) {
        let sel =
            taproot::Selector::class("graph-canvas-swatch-node").with_attr("data-key", substr);
        if !self.click(&sel) {
            self.note_miss("click-node", substr, "no pane node matched");
        }
    }

    /// Route a wheel event to the surface under `(x, y)` (rung 5 slice B). The
    /// page scrolls when the pointer is on it, the canvas pans when it is not.
    /// Ephemeral, so it drives the session's semantic method directly (the
    /// gesture law), never an Action. Shared by winit and the scenario runner.
    pub(super) fn deliver_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) {
        if self.app.user_agent_decision.is_open() {
            return;
        }
        let plan = self.surface_plan();
        if let Some(hit) = crate::surface::hit_test(&plan, self.app.focus, x, y)
            && let crate::surface::SurfaceKind::Content(node) = hit.kind
        {
            self.content_wheel(0, hit.id, node, hit.local, (dx, dy));
            return;
        }
        if let Some(crate::surface::HitResult {
            kind: crate::surface::SurfaceKind::Graph(pane),
            ..
        }) = crate::surface::hit_test(&plan, self.app.focus, x, y)
            && self.app.graph_pane_wheel(pane, dx, dy)
        {
            self.request_redraw();
            return;
        }
        // A list pane under the pointer scrolls itself. Without this branch a
        // pane taller than its tile simply hides its overflow: the content is
        // laid out and clipped, and nothing can reach it.
        if let Some(crate::surface::HitResult {
            kind: crate::surface::SurfaceKind::Pane(pane),
            local,
            ..
        }) = crate::surface::hit_test(&plan, self.app.focus, x, y)
        {
            if let Some(surface) = self.renderers.contributed.get_mut(pane) {
                if surface.wheel(dx, dy, local.0, local.1)
                    == crate::contributed_surface::SurfaceRequest::Redraw
                {
                    self.request_redraw();
                }
            } else if self.renderers.scroll_pane(pane, dx, dy) {
                self.request_redraw();
            }
        }
    }

    /// Route a pointer press to the surface under `(x, y)` and capture it until
    /// release (rung 5 slice B). A press on content focuses it and delivers the
    /// click: a link resolves to a durable navigation and goes through
    /// `Action::OpenAddress`, growing the graph; a press on the canvas begins a
    /// canvas gesture. Shared by winit and the scenario runner.
    pub(super) fn deliver_press(&mut self, x: f32, y: f32, button: MouseButton) {
        self.surface_pointer_buttons.0 |= surface_button_mask(button).0;
        self.window_mouse_input(
            0,
            None,
            (x, y),
            inker::PointerPhase::Down,
            surface_mouse_button(button),
        );
        if self.contributed_pointer.is_some() {
            return;
        }
        let chrome_intent = self
            .chrome
            .click(0, x, y, self.width.max(1), self.height.max(1))
            .into_iter()
            .next();
        tracing::debug!(
            x,
            y,
            ?button,
            ?chrome_intent,
            omnibar = self.app.omnibar.open,
            "physical pane press admission"
        );
        if dispatch_primary_chrome_press(self, chrome_intent) {
            return;
        }
        // A held user-agent callback is modal with respect to the page that
        // requested it. A click outside the retained decision card is consumed
        // rather than leaking through to content behind the prompt.
        if reject_primary_modal_press(self) {
            return;
        }
        // A press while the omnibar is open dismisses it and is swallowed, so
        // the surface beneath never also reacts to the same press.
        if reject_primary_omnibar_press(self) {
            return;
        }
        if button != MouseButton::Left {
            self.leave_frame_focus();
        }
        if button == MouseButton::Left
            && self.outer_press(0, (self.width.max(1), self.height.max(1)), x, y)
        {
            self.pointer_capture = None;
            self.request_redraw();
            return;
        }
        let plan = self.surface_plan();
        let hit = crate::surface::hit_test(&plan, self.app.focus, x, y);
        tracing::debug!(x, y, ?hit, "physical surface press selected");
        // Right-click is the context menu the palette registry names: the
        // command palette (the `>` actions lane). On the canvas the press goes
        // to the canvas, so a right-drag selects (Scenograph editor plan
        // SE26); the menu opens on the release the canvas reports as a click,
        // in `deliver_release`. Elsewhere the menu opens on the press, as
        // before.
        if dispatch_primary_context_press(self, button, hit) {
            return;
        }
        if button == MouseButton::Left
            && self.pointer_capture.is_none()
            && super::window_input::captured_mouse(&self.content_sessions.pool).is_none()
            && let Some((pane, divider, origin)) = workbench_resize_target(&self.app, &plan, hit, x, y)
        {
            self.app.active_pane = Some(pane);
            self.pointer_capture = Some(crate::surface::SurfaceKind::Pane(pane));
            self.wb_divider_drag = Some((divider, origin));
            self.request_redraw();
            return;
        }
        if let Some(hit) = dispatch_primary_press(self, hit, x, y, button) {
            match hit.kind {
                // Content is fully dispatched by the shared native caller.
                crate::surface::SurfaceKind::Content(_) => unreachable!(),
                // A press on a pane makes it the active pane (the anchor for
                // close/maximize/divider). A Trail pane also routes the click to
                // its row (slice D): a navigable row lowers Action::OpenAddress
                // through the same spine as a keypress. Other panes are still
                // placeholders (slice C), so the press is otherwise swallowed.
                crate::surface::SurfaceKind::Pane(id) => {
                    self.app.active_pane = Some(id);
                    // A5 floats share this ordinary pane surface path. Raising
                    // happens before the pane's retained DOM receives the
                    // click, so the next frame's paint and hit order agree.
                    self.raise_shared_float(id);
                    if button == MouseButton::Left {
                        if let Some(pane) = self.renderers.contributed.get_mut(id) {
                            self.app.focus = crate::surface::FocusTarget::Pane(id);
                            pane.focus_at(hit.local.0, hit.local.1);
                            self.contributed_pointer = plan
                                .iter()
                                .find(|surface| surface.id == hit.id)
                                .copied()
                                .map(|surface| ContributedPointer {
                                    surface,
                                    generation: pane.generation(),
                                    window: 0,
                                });
                            pane.mouse_press(hit.local.0, hit.local.1);
                            self.request_redraw();
                            return;
                        }
                        if self.renderers.inline_mount(id).is_some() {
                            self.app.focus = crate::surface::FocusTarget::Pane(id);
                            self.input_window = 0;
                        }
                        match self.pane_content(id) {
                            Some(PaneContent::Trail) => {
                                // The same cambium round trip as the Roster.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let actions = match (dims, self.renderers.trail.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for action in actions {
                                    match action {
                                        crate::trail_pane::TrailPaneAction::Navigate(url) => {
                                            self.act(Action::OpenAddress(url))
                                        },
                                        crate::trail_pane::TrailPaneAction::RecoverSession(id) => {
                                            // A Removed-sessions row: restore the
                                            // trashed session and switch (O3).
                                            if let Ok(id) = id.parse::<uuid::Uuid>() {
                                                self.act(Action::RecoverSession(
                                                    crate::panes::SessionId::from_uuid(id),
                                                ));
                                            }
                                        },
                                        crate::trail_pane::TrailPaneAction::Recover(id) => {
                                            // The Removed row carries the staged
                                            // node's ORIGINAL uuid; recovery
                                            // restores that identity.
                                            match id.parse::<uuid::Uuid>() {
                                                Ok(id) => self.act(Action::RecoverDeletedNode(id)),
                                                Err(_) => self.app.note(
                                                    crate::observe::AppEvent::InteractionMissed {
                                                        what: "recover",
                                                        target: id,
                                                    },
                                                ),
                                            }
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Roster) => {
                                // Route into the cambium grid: hit-test its DOM
                                // at the pane's size and dispatch, then lower
                                // whatever the view emitted through the spine —
                                // the same path a keypress takes. This is the
                                // general cambium pane-event round trip.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let actions = match (dims, self.renderers.roster.get_mut(&id)) {
                                    (Some((rw, rh)), Some(grid)) => {
                                        let actions = grid.click(hit.local.0, hit.local.1, rw, rh);
                                        // The strip emits no action — switching a
                                        // tab is a state change in the widget's
                                        // own state. Mirror it out so the rest of
                                        // the host can see which tab is showing.
                                        self.app.roster_tab = grid.selected_tab().0;
                                        actions
                                    },
                                    _ => Vec::new(),
                                };
                                for action in actions {
                                    match action {
                                        crate::cambium_pane::RosterAction::Binding(intent) => { self.act(Action::SetPaneBinding(intent)); },
                                        crate::cambium_pane::RosterAction::Navigate(url) => {
                                            self.act(Action::OpenAddress(url))
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Gloss(_)) => {
                                // Same hit-test round trip; the outcome arrives
                                // as drained intents (the swatch mutates state
                                // rather than bubbling), lowered here.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.gloss.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    match intent {
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Open(url),
                                        ) => self.act(Action::OpenAddress(url)),
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Switch(id),
                                        ) => self.act(Action::SwitchSession(id)),
                                        // A composed Removed row: recover the
                                        // node under its ORIGINAL id.
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Recover(id),
                                        ) => self.act(Action::RecoverDeletedNode(id)),
                                        crate::swatch_pane::SwatchIntent::Expand => {
                                            self.app.focus = crate::surface::FocusTarget::Graph(
                                                self.app.default_graph_pane(),
                                            );
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Inspector) => {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.inspector.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    match intent {
                                        crate::inspector_pane::InspectorIntent::ClipToKnot { offer } => {
                                            self.clip_document_to_knot(offer)
                                        },
                                        crate::inspector_pane::InspectorIntent::Binding(intent) => self.act(Action::SetPaneBinding(intent)),
                        crate::inspector_pane::InspectorIntent::NewResearchNote => self.act(Action::NewResearchNote),
                                        crate::inspector_pane::InspectorIntent::OpenResearchNote => self.act(Action::OpenResearchNote),
                                        crate::inspector_pane::InspectorIntent::ChooseResearchNote { choice } => self.act(Action::ChooseResearchNote { choice }),
                                        crate::inspector_pane::InspectorIntent::CaptureSourceDocument { target } => self.act(Action::CaptureSourceDocument { target }),
                                        crate::inspector_pane::InspectorIntent::SetViewer {
                                            member,
                                            viewer,
                                        } => self.act(Action::SetViewerOverride { member, viewer }),
                                    }
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::TRANSCRIPT =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let actions = match (dims, self.renderers.transcript.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for action in actions {
                                    match action {
                                        crate::transcript_pane::TranscriptPaneAction::Repeat(
                                            entry,
                                        ) => {
                                            // The lane already lowers this
                                            // Action; the pane only names the
                                            // entry to repeat.
                                            self.act(Action::RepeatShellEntry(entry));
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::ARRANGE =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.arrange.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    if let Some(action) = intent.into_action() {
                                        self.act(action);
                                    }
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::SETTINGS =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                if let (Some((rw, rh)), Some(pane)) =
                                    (dims, self.renderers.settings.get_mut(&id))
                                {
                                    self.app.focus = crate::surface::FocusTarget::Pane(id);
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::PUBLISHING =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                if let (Some((rw, rh)), Some(pane)) =
                                    (dims, self.renderers.publish.get_mut(&id))
                                {
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::SHARED_KNOT =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                if let (Some((rw, rh)), Some(pane)) =
                                    (dims, self.renderers.shared_knot.get_mut(&id))
                                {
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                if let (Some((rw, rh)), Some(pane)) =
                                    (dims, self.renderers.device_receipts.get_mut(&id))
                                {
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            },
                            Some(PaneContent::Overmap(_)) => {
                                // A session-node click adopts that session:
                                // navigating to a container IS the switch
                                // (overmap v0), through the ordinary spine.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.overmap.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    match intent {
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Open(url),
                                        ) => self.act(Action::OpenAddress(url)),
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Switch(id),
                                        ) => self.act(Action::SwitchSession(id)),
                                        // A composed Removed row: recover the
                                        // node under its ORIGINAL id.
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Recover(id),
                                        ) => self.act(Action::RecoverDeletedNode(id)),
                                        crate::swatch_pane::SwatchIntent::Expand => {
                                            self.app.focus = crate::surface::FocusTarget::Graph(
                                                self.app.default_graph_pane(),
                                            );
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Workbench) => {
                                // A press here begins a gesture, resolved on
                                // RELEASE (a tab click activates; a tab drag
                                // onto another cell stacks; a seam drag
                                // re-weights) — so record what was pressed and
                                // decide in deliver_release / deliver_move.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect,
                                        (
                                            s.rect.w.round().max(1.0) as u32,
                                            s.rect.h.round().max(1.0) as u32,
                                        ),
                                    )
                                });
                                if let (Some((rect, (rw, rh))), Some(pane)) =
                                    (dims, self.renderers.workbench.get_mut(&id))
                                {
                                    let (lx, ly) = hit.local;
                                    if let Some(div) = pane.tiling().divider_at(lx, ly).cloned() {
                                        self.wb_divider_drag = Some((div, (rect.x, rect.y)));
                                    } else if let Some(member) = pane.tab_at(lx, ly, rw, rh) {
                                        self.app.focus = crate::surface::FocusTarget::Pane(id);
                                        self.app.publish_member_context(id, Some(member));
                                        self.wb_tab_drag = Some((id, member));
                                    }
                                }
                            },
                            _ => {},
                        }
                    }
                    self.sync_ime_allowed();
                    self.request_redraw();
                    return;
                },
                crate::surface::SurfaceKind::Divider(index) => {
                    let area = Rect::full(self.width.max(1), self.height.max(1));
                    let tiling =
                        crate::pane::place_panes(&self.app.frisket, area, self.app.maximized);
                    self.divider_drag = tiling.dividers.into_iter().find(|d| d.index == index);
                    self.request_redraw();
                    return;
                },
                // A graph pane owns the view gesture. The local point is
                // essential for side-by-side graph panes.
                crate::surface::SurfaceKind::Graph(pane) => {
                    self.raise_shared_float(pane);
                    self.app.focus = crate::surface::FocusTarget::Graph(pane);
                    if let Some(button) = pointer_button(button)
                        && self
                            .app
                            .graph_pane_pointer_down(pane, button, hit.local.0, hit.local.1)
                    {
                        self.request_redraw();
                    }
                },
                crate::surface::SurfaceKind::Chrome
                | crate::surface::SurfaceKind::Frame
                | crate::surface::SurfaceKind::FloatFrame(_) => {},
            }
        }
    }

    /// Route a pointer release to whatever the matching press captured (rung 5
    /// slice B). The canvas gets a release only if its own press began the
    /// gesture, so a content click never ends a canvas drag. Shared by winit
    /// and the scenario runner.

    /// Route a pointer move. Today only the divider drag consumes moves: while
    /// a seam is captured, each move becomes a ratio through cambium's
    /// `Split::ratio_at` over the split's own container rect, lowered as an
    /// ordinary Action — the same spine as everything else.
    /// Route a pointer move into the pane under it (pane pointer-move
    /// routing): the swatch panes get their Enter/Leave hover transitions, so
    /// the hover emphasis the component always supported finally lights up.
    /// A move off a hovering pane delivers its Leave. Ephemeral, so it drives
    /// the panes' semantic methods directly (the gesture law), never an Action.
    pub(super) fn deliver_hover(&mut self, x: f32, y: f32) {
        let plan = self.surface_plan();
        let hit = crate::surface::hit_test(&plan, self.app.focus, x, y);
        let pane_hit = match hit.as_ref().map(|h| h.kind) {
            Some(crate::surface::SurfaceKind::Pane(id)) => Some(id),
            _ => None,
        };
        let previous_pane = self.hovered_pane;
        let mut redraw = false;
        // Leaving the previously hovered pane clears its emphasis.
        if let Some(prev) = self.hovered_pane
            && pane_hit != Some(prev)
        {
            if let Some(pane) = self.renderers.contributed.get_mut(prev) {
                redraw |= pane.hover(cambium::HoverPhase::Leave, 0.0, 0.0)
                    == crate::contributed_surface::SurfaceRequest::Redraw;
            } else {
                redraw |= match self.pane_content(prev) {
                    Some(PaneContent::Gloss(_)) => self
                        .renderers
                        .gloss
                        .get_mut(&prev)
                        .is_some_and(|p| p.hover_leave()),
                    Some(PaneContent::Overmap(_)) => self
                        .renderers
                        .overmap
                        .get_mut(&prev)
                        .is_some_and(|p| p.hover_leave()),
                    _ => false,
                };
            }
        }
        self.hovered_pane = pane_hit;
        if let (Some(hit), Some(id)) = (hit, pane_hit) {
            let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                (
                    s.rect.w.round().max(1.0) as u32,
                    s.rect.h.round().max(1.0) as u32,
                )
            });
            if let Some((rw, rh)) = dims {
                if let Some(pane) = self.renderers.contributed.get_mut(id) {
                    redraw |= pane.hover(
                        if previous_pane == Some(id) {
                            cambium::HoverPhase::Move
                        } else {
                            cambium::HoverPhase::Enter
                        },
                        hit.local.0,
                        hit.local.1,
                    ) == crate::contributed_surface::SurfaceRequest::Redraw;
                } else {
                    redraw |= match self.pane_content(id) {
                        Some(PaneContent::Gloss(_)) => self
                            .renderers
                            .gloss
                            .get_mut(&id)
                            .is_some_and(|p| p.hover(hit.local.0, hit.local.1, rw, rh)),
                        Some(PaneContent::Overmap(_)) => self
                            .renderers
                            .overmap
                            .get_mut(&id)
                            .is_some_and(|p| p.hover(hit.local.0, hit.local.1, rw, rh)),
                        _ => false,
                    };
                }
            }
        }
        if redraw {
            self.request_redraw();
        }
    }

    pub(super) fn deliver_move(&mut self, x: f32, y: f32) {
        if self.move_contributed_pointer(0, x, y) {
            return;
        }
        if let Some(crate::surface::SurfaceKind::Graph(pane)) = self.pointer_capture {
            let local = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Graph(pane))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            });
            if let Some((local_x, local_y)) = local
                && self.app.graph_pane_cursor_moved(pane, local_x, local_y)
            {
                self.request_redraw();
            }
            return;
        }
        if dispatch_primary_content_move(self, x, y) {
            return;
        }
        if self.pointer_capture.is_none() && self.outer_move(0, x, y) {
            self.request_redraw();
            return;
        }
        if let Some(crate::surface::SurfaceKind::Pane(pane)) = self.pointer_capture
            && let Some((local_x, local_y)) = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Pane(pane))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            })
            && let Some(surface) = self.renderers.contributed.get_mut(pane)
        {
            if surface.mouse_move(local_x, local_y)
                == crate::contributed_surface::SurfaceRequest::Redraw
            {
                self.request_redraw();
            }
            return;
        }
        // A workbench divider drag: the band's pair re-weights toward the
        // pointer (host math over platen's N-ary fractions), lowered as an
        // ordinary Action. The walk is pane-local; the origin converts.
        if let Some((div, origin)) = self.wb_divider_drag.clone() {
            let fractions =
                crate::workbench_tiling::drag_fractions(&div, x - origin.0, y - origin.1);
            self.act(Action::WorkbenchSetFractions {
                path: div.path,
                fractions,
            });
            return;
        }
        if let Some(drag) = self.divider_drag.clone() {
            let split = crate::pane::cambium_split(drag.axis, drag.ratio);
            let ratio = split.ratio_at(drag.area.w, drag.area.h, x - drag.area.x, y - drag.area.y);
            self.act(Action::SetSplitRatio {
                space: crate::action::SpaceRef::Primary,
                path: drag.path,
                ratio,
            });
            return;
        }

        let content_hit = crate::surface::hit_test(&self.surface_plan(), self.app.focus, x, y)
            .and_then(|hit| match hit.kind {
                crate::surface::SurfaceKind::Content(node) => {
                    Some((node, hit.id, hit.local.0, hit.local.1))
                },
                _ => None,
            });
        let Some((node, appearance, local_x, local_y)) = content_hit else {
            self.reset_surface_cursor();
            return;
        };
        self.sync_content_placements();
        let _ = self.content_sessions.pool.prepare(&appearance);
        if let Some(links) = self
            .content_sessions
            .appearance(node, appearance)
            .and_then(pelt_core::PeltContent::document)
            .map(|controller| controller.links())
        {
            let target = links.into_iter().find_map(|link| {
                let [left, top, width, height] = link.rect;
                (local_x >= left
                    && local_x <= left + width.max(0.0)
                    && local_y >= top
                    && local_y <= top + height.max(0.0))
                .then_some(link.url)
            });
            let preview = target.map(|url| super::content_link_target(&self.app, node, &url));
            let hovering_link = preview.is_some();
            let changed = self.app.set_link_preview(preview);
            self.hovered_surface = None;
            if let Some(window) = self.window.as_ref() {
                window.set_cursor_visible(true);
                window.set_cursor(if hovering_link {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                });
            }
            if changed {
                self.request_redraw();
            }
            return;
        }
        let preview_changed = self.app.set_link_preview(None);
        if !self.content_sessions.has_surface(&node) {
            self.reset_surface_cursor();
            return;
        }
        self.content_mouse(
            appearance,
            (local_x, local_y),
            inker::PointerPhase::Move,
            None,
        );
        self.hovered_surface = Some(node);
        self.apply_pending_surface_cursor();
        if preview_changed {
            self.request_redraw();
        }
    }

    pub(super) fn contributed_pointer_is_current(&self, admitted: ContributedPointer) -> bool {
        let crate::surface::SurfaceKind::Pane(pane) = admitted.surface.kind else {
            return false;
        };
        let plan = if admitted.window == 0 {
            self.surface_plan()
        } else if let Some(lens) = self
            .lens_windows
            .values()
            .find(|lens| lens.ordinal as u64 + 1 == admitted.window)
        {
            self.lens_plan(lens.ordinal, lens.width, lens.height)
        } else {
            return false;
        };
        admitted.is_current(
            plan.into_iter()
                .find(|surface| surface.id == admitted.surface.id),
            self.renderers
                .contributed
                .get(pane)
                .map(|surface| surface.generation()),
            admitted.window,
        )
    }

    pub(super) fn move_contributed_pointer(&mut self, window: u64, x: f32, y: f32) -> bool {
        let Some(admitted) = self.contributed_pointer else {
            return false;
        };
        if admitted.window != window {
            return false;
        }
        if !self.contributed_pointer_is_current(admitted) {
            self.cancel_content_pointer();
            return true;
        }
        let crate::surface::SurfaceKind::Pane(pane) = admitted.surface.kind else {
            return false;
        };
        if let Some(surface) = self.renderers.contributed.get_mut(pane) {
            if surface.mouse_move(x - admitted.surface.rect.x, y - admitted.surface.rect.y)
                == crate::contributed_surface::SurfaceRequest::Redraw
            {
                self.request_redraw();
            }
        }
        true
    }

    pub(super) fn release_contributed_pointer(&mut self, window: u64, x: f32, y: f32) {
        let Some(admitted) = self.contributed_pointer else {
            return;
        };
        if admitted.window != window {
            return;
        }
        if !self.contributed_pointer_is_current(admitted) {
            self.cancel_content_pointer();
            return;
        }
        self.contributed_pointer = None;
        self.pointer_capture = None;
        let crate::surface::SurfaceKind::Pane(pane) = admitted.surface.kind else {
            return;
        };
        if let Some(surface) = self.renderers.contributed.get_mut(pane) {
            if surface.mouse_release(x - admitted.surface.rect.x, y - admitted.surface.rect.y)
                == crate::contributed_surface::SurfaceRequest::Redraw
            {
                self.request_redraw();
            }
        }
    }

    /// Focus loss cancels actual retained custody, without manufacturing Up.
    pub(super) fn cancel_content_pointer(&mut self) {
        self.outer_capture = None;
        // Retire host custody first: pool input synchronizes placements and
        // may otherwise recursively discover the same invalid admission.
        let mut windows = vec![0];
        windows.extend(
            self.lens_windows
                .values()
                .map(|lens| lens.ordinal as u64 + 1),
        );
        for window in windows {
            self.touch_input
                .focus_lost(&mut self.content_sessions.pool, window);
        }
        self.window_buttons.clear();
        let captured = self.pointer_capture.take();
        let contributed = self.contributed_pointer.take();
        let pane = contributed
            .map(|admitted| admitted.surface.kind)
            .or(captured);
        if let Some(crate::surface::SurfaceKind::Pane(pane)) = pane
            && let Some(surface) = self.renderers.contributed.get_mut(pane)
        {
            surface.mouse_cancel(0.0, 0.0);
        }
        if let Some(id) = super::window_input::captured_mouse(&self.content_sessions.pool) {
            self.content_input(
                id,
                pelt_core::PeltInput::Session(inker::SessionInput::Cancel),
            );
        }
        self.surface_pointer_buttons = inker::PointerButtons::NONE;
        self.wb_divider_drag = None;
        self.wb_tab_drag = None;
        self.lens_divider_drag = None;
    }

    pub(super) fn content_mouse(
        &mut self,
        appearance: crate::surface::SurfaceId,
        local: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        self.sync_content_placements();
        let Some(placement) = self.content_sessions.pool.placement(&appearance).cloned() else {
            return;
        };
        let window = placement.window;
        let routed = super::window_input::content_mouse_input(
            &mut self.content_sessions.pool,
            &self.window_buttons,
            window,
            Some(appearance),
            inker::PhysicalPosition {
                x: local.0 + placement.rect.x,
                y: local.1 + placement.rect.y,
            },
            phase,
            button,
            inker::KeyboardModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: self.meta,
            },
        );
        self.apply_window_mouse_input(window, routed);
    }

    pub(super) fn window_mouse_input(
        &mut self,
        window: u64,
        appearance: Option<crate::surface::SurfaceId>,
        position: (f32, f32),
        phase: inker::PointerPhase,
        button: Option<inker::MouseButton>,
    ) -> bool {
        let routed = super::window_input::native_mouse_input(
            &mut self.content_sessions.pool,
            &mut self.window_buttons,
            window,
            appearance,
            inker::PhysicalPosition {
                x: position.0,
                y: position.1,
            },
            phase,
            button,
            inker::KeyboardModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: self.meta,
            },
        );
        self.apply_window_mouse_input(window, routed)
    }

    fn apply_window_mouse_input(
        &mut self,
        window: u64,
        routed: Option<(super::content_pool::ContentKey, pelt_core::PeltHostEffect)>,
    ) -> bool {
        if window == 0 {
            self.surface_pointer_buttons = self
                .window_buttons
                .get(&window)
                .copied()
                .unwrap_or(inker::PointerButtons::NONE);
        }
        let Some((content, effect)) = routed else {
            return false;
        };
        let node = match content {
            super::content_pool::ContentKey::Node(node)
            | super::content_pool::ContentKey::Reader { node, .. } => node,
        };
        self.apply_content_effect(node, effect);
        true
    }

    pub(super) fn apply_pending_surface_cursor(&mut self) {
        let shape = self
            .hovered_surface
            .and_then(|node| self.content_sessions.surface_mut(&node))
            .and_then(|producer| producer.poll_cursor_shape());
        if let (Some(shape), Some(window)) = (shape, self.window.as_ref()) {
            let icon = surface_cursor_icon(shape);
            window.set_cursor_visible(icon.is_some());
            if let Some(icon) = icon {
                window.set_cursor(icon);
            }
            if let Some(node) = self.hovered_surface {
                self.app
                    .note(crate::observe::AppEvent::SurfaceCursorChanged {
                        node,
                        shape: surface_cursor_label(shape),
                    });
                self.run_effects(Vec::new());
            }
        }
    }

    pub(super) fn reset_surface_cursor(&mut self) {
        let preview_changed = self.app.set_link_preview(None);
        if self.hovered_surface.take().is_none() {
            if preview_changed {
                self.request_redraw();
            }
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.set_cursor_visible(true);
            window.set_cursor(CursorIcon::Default);
        }
        if preview_changed {
            self.request_redraw();
        }
    }

    pub(super) fn deliver_release(&mut self, x: f32, y: f32, button: MouseButton) {
        self.surface_pointer_buttons.0 &= !surface_button_mask(button).0;
        let std::ops::ControlFlow::Continue(captured) =
            dispatch_primary_release(self, x, y, button)
        else {
            return;
        };
        if captured.is_none()
            && button == MouseButton::Left
            && self.outer_release(0, (self.width.max(1), self.height.max(1)), x, y)
        {
            self.request_redraw();
            return;
        }
        let graph_capture = match captured {
            Some(crate::surface::SurfaceKind::Graph(pane)) => Some(pane),
            _ => None,
        };
        if let Some(crate::surface::SurfaceKind::Pane(pane)) = captured
            && let Some((local_x, local_y)) = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Pane(pane))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            })
            && let Some(surface) = self.renderers.contributed.get_mut(pane)
        {
            if button == MouseButton::Left {
                if surface.mouse_release(local_x, local_y)
                    == crate::contributed_surface::SurfaceRequest::Redraw
                {
                    self.request_redraw();
                }
            }
            return;
        }
        if self.wb_divider_drag.take().is_some() {
            // Like the frisket seam: moves rode Redraw; persist on release.
            self.act(Action::SaveSession);
            return;
        }
        if let Some((pane, dragged)) = self.wb_tab_drag.take() {
            self.finish_wb_tab_gesture(pane, dragged, x, y);
            return;
        }
        if self.divider_drag.take().is_some() {
            // The drag's ratio moves rode Redraw only; the settled layout
            // persists once, on release.
            self.act(Action::SaveSession);
            return;
        }
        if let Some(pane) = graph_capture
            && let Some(button) = pointer_button(button)
            && let Some((local_x, local_y)) = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Graph(pane))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            })
        {
            if self.release_graph_pointer(pane, button, local_x, local_y) {
                self.request_redraw();
            }
            // A right click the canvas did not use is a request for the
            // context menu: select the node under it, so node-scoped commands
            // apply to it, then open the command lane.
            if let Some(request) = self.app.graph_pane_take_context_request(pane) {
                if let Some(member) = request.node {
                    self.app.graph_pane_select_member(pane, member);
                }
                self.act(Action::OmnibarOpen { command: true });
            }
        }
    }

    /// Native badge expansion is a view edit even when it bypasses Action.
    /// Both primary and lens input use this save boundary after release.
    pub(super) fn release_graph_pointer(
        &mut self,
        pane: crate::panes::PaneId,
        button: mere::canvas::PointerButton,
        x: f32,
        y: f32,
    ) -> bool {
        let graph = self.app.graph_for_pane(pane);
        let before = graph.and_then(|g| self.app.graph_runtimes.canvas(g))
            .map(|c| c.fold_records().to_vec());
        let mosaic_before = graph.and_then(|g| self.app.graph_runtimes.canvas(g))
            .and_then(|c| c.cartography_geometry().mosaic().cloned());
        let handled = self.app.graph_pane_pointer_up(pane, button, x, y);
        let mosaic_after = graph.and_then(|g| self.app.graph_runtimes.canvas(g))
            .and_then(|c| c.cartography_geometry().mosaic().cloned());
        if mosaic_before != mosaic_after { self.app.mark_graph_scene_changed(pane); }
        if let Some(graph) = graph {
            let after = self.app.graph_runtimes.canvas(graph).map(|c| c.fold_records().to_vec());
            if before != after { self.save_graph_folds(graph); }
        }
        handled
    }

    /// Lower one winit touch contact to Pointer Events. Each live contact keeps
    /// the content node hit at Started, matching implicit pointer capture for
    /// direct-manipulation devices even if later coordinates leave the tile.
    pub(super) fn deliver_touch(&mut self, touch: Touch) {
        self.deliver_touch_in(0, touch);
    }

    pub(super) fn deliver_touch_in(&mut self, window: u64, touch: Touch) {
        let phase = match touch.phase {
            TouchPhase::Started => inker::PointerPhase::Down,
            TouchPhase::Moved => inker::PointerPhase::Move,
            TouchPhase::Ended => inker::PointerPhase::Up,
            TouchPhase::Cancelled => inker::PointerPhase::Cancel,
        };
        let altitude_angle = match touch.force {
            Some(Force::Calibrated { altitude_angle, .. }) => {
                altitude_angle.map(|angle| angle as f32)
            },
            _ => None,
        };
        let pressure = touch.force.and_then(|force| {
            let value = force.normalized() as f32;
            value.is_finite().then(|| value.clamp(0.0, 1.0))
        });
        self.deliver_touch_contact_in(
            window,
            touch.id,
            phase,
            touch.location.x as f32,
            touch.location.y as f32,
            pressure,
            altitude_angle,
        );
    }

    /// The shared touch lowering seam. Winit and the headed scenario both use
    /// this, so the receipt exercises the same id allocation and per-contact
    /// capture that real hardware does after winit has decoded the OS event.
    pub(super) fn deliver_touch_contact(
        &mut self,
        host_id: u64,
        phase: inker::PointerPhase,
        x: f32,
        y: f32,
        pressure: Option<f32>,
        altitude_angle: Option<f32>,
    ) {
        self.deliver_touch_contact_in(0, host_id, phase, x, y, pressure, altitude_angle);
    }
    pub(super) fn deliver_touch_contact_in(
        &mut self,
        window: u64,
        host_id: u64,
        phase: inker::PointerPhase,
        x: f32,
        y: f32,
        pressure: Option<f32>,
        altitude_angle: Option<f32>,
    ) {
        if self.touch_input.reject_modal(
            &mut self.content_sessions.pool,
            window,
            self.app.user_agent_decision.is_open(),
        ) {
            return;
        }
        self.sync_content_placements();
        let plan = self.input_plan(window);
        if self.touch_input.deliver(
            &mut self.content_sessions.pool,
            &plan,
            self.app.focus,
            window,
            host_id,
            phase,
            inker::PhysicalPosition { x, y },
            pressure,
            altitude_angle,
            inker::KeyboardModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: self.meta,
            },
        ) {
            self.request_redraw();
        }
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

    #[test]
    fn workbench_resize_band_precedes_its_document_but_respects_other_appearances() {
        use crate::surface::{Surface, SurfaceId, SurfaceKind};
        let mut app = crate::app::App::test_stub();
        app.update(Action::OpenAddress("https://example.com/left".into()));
        app.update(Action::OpenInWorkbench);
        app.update(Action::OpenAddress("https://example.com/right".into()));
        let node = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::OpenInWorkbench);
        let pane = app.active_pane.unwrap();
        let plan = vec![Surface { id: SurfaceId::pane(pane), kind: SurfaceKind::Pane(pane),
            rect: Rect::new(20.0, 40.0, 800.0, 600.0) }];
        let hit = crate::surface::HitResult {
            id: SurfaceId::content_appearance(node,
                crate::surface::appearance::workbench_appearance_role(pane, node)),
            kind: SurfaceKind::Content(node), local: (3.0, 100.0),
        };
        assert!(workbench_resize_target(&app, &plan, Some(hit), 424.0, 200.0).is_some(),
            "four pixels into the document still grabs the seam");
        let covered = crate::surface::HitResult { id: SurfaceId::content_appearance(node, 99), ..hit };
        assert!(workbench_resize_target(&app, &plan, Some(covered), 424.0, 200.0).is_none(),
            "another appearance covering the seam keeps its input");
        assert!(workbench_resize_target(&app, &plan, Some(hit), 430.0, 200.0).is_none());
    }

    #[test]
    fn web_cursor_shapes_map_to_host_cursor_icons() {
        assert_eq!(
            surface_cursor_icon(inker::CursorShape::Pointer),
            Some(CursorIcon::Pointer)
        );
        assert_eq!(
            surface_cursor_icon(inker::CursorShape::ResizeNesw),
            Some(CursorIcon::NeswResize)
        );
        assert_eq!(surface_cursor_icon(inker::CursorShape::Hidden), None);
        assert_eq!(surface_cursor_label(inker::CursorShape::Pointer), "pointer");
    }
}

#[cfg(test)]
mod contributed_custody_tests {
    use super::*;
    #[test]
    fn content_pool_host_contributed_custody_checks_placement_and_provider() {
        let surface = crate::surface::Surface {
            id: crate::surface::SurfaceId(8),
            kind: crate::surface::SurfaceKind::Pane(crate::panes::PaneId(9)),
            rect: crate::surface::Rect::new(10., 20., 100., 80.),
        };
        let admitted = ContributedPointer {
            surface,
            generation: 4,
            window: 0,
        };
        assert!(admitted.is_current(Some(surface), Some(4), 0));
        assert!(
            !admitted.is_current(Some(surface), Some(4), 1),
            "another native window cannot inherit gesture"
        );
        assert!(
            !admitted.is_current(None, Some(4), 0),
            "hidden/closed placement retires gesture"
        );
        assert!(
            !admitted.is_current(Some(surface), Some(5), 0),
            "replacement provider retires gesture"
        );
        assert!(
            !admitted.is_current(
                Some(crate::surface::Surface {
                    rect: crate::surface::Rect::new(11., 20., 100., 80.),
                    ..surface
                }),
                Some(4),
                0
            ),
            "moved placement retires gesture"
        );
        assert!(
            !admitted.is_current(
                Some(crate::surface::Surface {
                    id: crate::surface::SurfaceId(10),
                    ..surface
                }),
                Some(4),
                0
            ),
            "another appearance cannot inherit gesture"
        );
    }
}
