//! Input routing: pointer, wheel, keys, and the drag gestures, shared by winit
//! and the scenario runner so one description drives two runners.
//!
//! Focus picks the lane, for keys as for the pointer: the omnibar when open,
//! the focused page when one holds focus, the canvas otherwise. Ephemeral
//! input (scroll, hover, blur) rides state directly per the gesture law;
//! durable intent becomes an `Action`.

use winit::event::{Force, MouseButton, Touch, TouchPhase};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::CursorIcon;

use crate::panes::PaneContent;
use genet_probe::AutomatableExt as _;
use inker::{SessionClick, SessionScrollKey};
use mere::canvas::PointerButton;

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

fn surface_mouse_button(button: MouseButton) -> Option<inker::MouseButton> {
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

fn mouse_pointer_event(
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

impl Shell {
    pub(super) fn click_pane_row(&mut self, substr: &str) {
        // Both list panes resolve through the shared driver's `click`: a Trail
        // `list-row` or a grid `roster-cell` whose text contains `substr`, over
        // all surfaces at once (no per-pane dispatch). Short-circuit `||` means a
        // hit presses once; only a total miss is attributable.
        let hit = self.click(&genet_probe::Selector::class("roster-cell").containing(substr))
            || self.click(&genet_probe::Selector::class("list-row").containing(substr))
            // A settings option is a row for receipt purposes (the Apparatus
            // pane's radio options).
            || self.click(&genet_probe::Selector::class("radio").containing(substr))
            || self.click(&genet_probe::Selector::class("setting-apply").containing(substr))
            // A composed list section's row (the gloss-composite): the same
            // verb addresses it, wherever the section was composed.
            || self.click(&genet_probe::Selector::class("section-row").containing(substr));
        if !hit {
            self.app.note(crate::observe::AppEvent::InteractionMissed {
                what: "click-row",
                target: substr.to_string(),
            });
            tracing::warn!(%substr, "click-row: no list-pane row matched");
        }
    }

    /// Click the Roster's tab labelled `label` (the scenario's `click-tab`),
    /// through the shared driver: a `.tab` element whose text is `label`. The
    /// strip's geometry is the layout's to know; the host names the target and
    /// the resolver finds it — the same substrate every genet app shares.
    pub(super) fn click_pane_tab(&mut self, label: &str) {
        if !self.click(&genet_probe::Selector::class("tab").containing(label)) {
            self.app.note(crate::observe::AppEvent::InteractionMissed {
                what: "click-tab",
                target: label.to_string(),
            });
            tracing::warn!(%label, "click-tab: no Roster tab matched");
        }
    }

    /// Click the Gloss minimap's node matching `substr` (the scenario's
    /// `click-node`), through the shared driver. The node buttons carry their url
    /// as `data-key`, so the driver selects on it — unique where the display
    /// label (two "Example Domain" pages) is not.
    pub(super) fn click_pane_node(&mut self, substr: &str) {
        let sel =
            genet_probe::Selector::class("graph-canvas-swatch-node").with_attr("data-key", substr);
        if !self.click(&sel) {
            self.app.note(crate::observe::AppEvent::InteractionMissed {
                what: "click-node",
                target: substr.to_string(),
            });
            tracing::warn!(%substr, "click-node: no pane node matched");
        }
    }

    /// Route a wheel event to the surface under `(x, y)` (rung 5 slice B). The
    /// page scrolls when the pointer is on it, the canvas pans when it is not.
    /// Ephemeral, so it drives the session's semantic method directly (the
    /// gesture law), never an Action. Shared by winit and the scenario runner.
    pub(super) fn deliver_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) {
        let plan = self.surface_plan();
        if let Some(hit) = crate::surface::hit_test(&plan, self.app.focus, x, y)
            && let crate::surface::SurfaceKind::Content(node) = hit.kind
        {
            if let Some(session) = self.content_sessions.get_mut(&node) {
                if session.scroll_at(hit.local.0, hit.local.1, dx, dy) {
                    self.request_redraw();
                }
            } else if let Some(producer) = self.surface_producers.get_mut(&node) {
                if let Err(error) = producer.send_mouse_input(inker::MouseEvent {
                    position: inker::PhysicalPosition {
                        x: hit.local.0,
                        y: hit.local.1,
                    },
                    button: None,
                    kind: inker::MouseEventKind::ScrollPixels {
                        delta_x: dx,
                        delta_y: dy,
                    },
                }) {
                    tracing::warn!(%node, %error, "surface wheel delivery failed");
                }
                self.request_redraw();
            }
            return;
        }
        if let Some(crate::surface::HitResult {
            kind: crate::surface::SurfaceKind::Graph(pane),
            ..
        }) = crate::surface::hit_test(&plan, self.app.focus, x, y)
            && self.app.graph_pane_wheel(pane, dx, dy)
        {
            self.request_redraw();
        }
    }

    /// Route a pointer press to the surface under `(x, y)` and capture it until
    /// release (rung 5 slice B). A press on content focuses it and delivers the
    /// click: a link resolves to a durable navigation and goes through
    /// `Action::OpenAddress`, growing the graph; a press on the canvas begins a
    /// canvas gesture. Shared by winit and the scenario runner.
    pub(super) fn deliver_press(&mut self, x: f32, y: f32, button: MouseButton) {
        self.surface_pointer_buttons.0 |= surface_button_mask(button).0;
        // A press while the omnibar is open dismisses it and is swallowed, so
        // the surface beneath never also reacts to the same press.
        if self.app.omnibar.open {
            // A press on a suggestion row COMMITS it (the retained chrome's
            // row handlers); anywhere else is the click-away dismiss.
            let intents = self
                .chrome
                .click(0, x, y, self.width.max(1), self.height.max(1));
            if let Some(crate::chrome_view::ChromeIntent::CommitRow(index)) =
                intents.into_iter().next()
            {
                self.act(Action::OmnibarCommitRow(index));
            } else {
                self.act(Action::OmnibarClose);
            }
            self.pointer_capture = None;
            return;
        }
        let plan = self.surface_plan();
        let hit = crate::surface::hit_test(&plan, self.app.focus, x, y);
        // Right-click is the context menu the palette registry names: open the
        // command palette (the `>` actions lane), selecting the graph node
        // under the pointer first so node-scoped actions apply to it. Panes and
        // content keep their own right-click behavior (none yet); this handles
        // the canvas, which is where the node-scoped actions live.
        if button == MouseButton::Right {
            if let Some(hit) = hit
                && let crate::surface::SurfaceKind::Graph(pane) = hit.kind
                && let Some(member) =
                    self.app
                        .graph_pane_node_at_screen(pane, hit.local.0, hit.local.1)
            {
                self.app.graph_pane_select_member(pane, member);
            }
            self.act(Action::OmnibarOpen { command: true });
            self.pointer_capture = None;
            return;
        }
        self.pointer_capture = hit.map(|h| h.kind);
        if let Some(hit) = hit {
            match hit.kind {
                crate::surface::SurfaceKind::Content(node) => {
                    self.app.focus = crate::surface::FocusTarget::Content(node);
                    if let Some(session) = self.content_sessions.get_mut(&node) {
                        if button == MouseButton::Left
                            && let SessionClick::Navigate(url) =
                                session.pointer_down(hit.local.0, hit.local.1)
                        {
                            self.act(Action::OpenAddress(url));
                        }
                    } else if let (Some(producer), Some(surface_button)) = (
                        self.surface_producers.get_mut(&node),
                        surface_mouse_button(button),
                    ) {
                        if let Err(error) = producer.move_focus(inker::FocusReason::Mouse) {
                            tracing::warn!(%node, %error, "surface focus delivery failed");
                        }
                        if let Err(error) = producer.send_pointer_input(mouse_pointer_event(
                            hit.local.0,
                            hit.local.1,
                            inker::PointerPhase::Down,
                            Some(surface_button),
                            self.surface_pointer_buttons,
                            inker::KeyboardModifiers {
                                shift: self.shift,
                                ctrl: self.ctrl,
                                alt: self.alt,
                                meta: false,
                            },
                        )) {
                            tracing::warn!(%node, %error, "surface press delivery failed");
                        }
                    }
                    self.request_redraw();
                    return;
                }
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
                    self.app.raise_floating_pane(id);
                    if button == MouseButton::Left {
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
                                    }
                                    _ => Vec::new(),
                                };
                                for action in actions {
                                    match action {
                                        crate::trail_pane::TrailPaneAction::Navigate(url) => {
                                            self.act(Action::OpenAddress(url))
                                        }
                                        crate::trail_pane::TrailPaneAction::RecoverSession(id) => {
                                            // A Removed-sessions row: restore the
                                            // trashed session and switch (O3).
                                            if let Ok(id) = id.parse::<uuid::Uuid>() {
                                                self.act(Action::RecoverSession(
                                                    crate::panes::SessionId::from_uuid(id),
                                                ));
                                            }
                                        }
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
                                        }
                                    }
                                }
                            }
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
                                    }
                                    _ => Vec::new(),
                                };
                                for action in actions {
                                    match action {
                                        crate::cambium_pane::RosterAction::Navigate(url) => {
                                            self.act(Action::OpenAddress(url))
                                        }
                                    }
                                }
                            }
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
                                    }
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
                                        }
                                    }
                                }
                            }
                            Some(PaneContent::Inspector) => {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let clip = match (dims, self.renderers.inspector.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => pane
                                        .click(hit.local.0, hit.local.1, rw, rh)
                                        .into_iter()
                                        .any(|intent| {
                                            matches!(
                                                intent,
                                                crate::inspector_pane::InspectorIntent::ClipToKnot
                                            )
                                        }),
                                    _ => false,
                                };
                                if clip {
                                    self.clip_focused_document_to_knot();
                                }
                            }
                            Some(PaneContent::Apparatus) => {
                                // The same cambium round trip: the radio's own
                                // selection moves, and the diff lowers as the
                                // typed viewer Action for the FOCUSED node.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.apparatus.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    }
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    match intent {
                                        crate::apparatus_pane::ApparatusIntent::SetViewer(
                                            viewer,
                                        ) => {
                                            if let Some(member) =
                                                self.app.graph_runtimes.focused_member()
                                            {
                                                self.act(Action::SetViewerOverride {
                                                    member,
                                                    viewer,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
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
                                    }
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
                                        }
                                    }
                                }
                            }
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
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            }
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
                            }
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
                            }
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
                                    }
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
                                        }
                                    }
                                }
                            }
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
                                        self.app.publish_member_context(id, Some(member));
                                        self.wb_tab_drag = Some((id, member));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    self.request_redraw();
                    return;
                }
                crate::surface::SurfaceKind::Divider(index) => {
                    let area = Rect::full(self.width.max(1), self.height.max(1));
                    let tiling =
                        crate::pane::place_panes(&self.app.frisket, area, self.app.maximized);
                    self.divider_drag = tiling.dividers.into_iter().find(|d| d.index == index);
                    self.request_redraw();
                    return;
                }
                // A graph pane owns the view gesture. The local point is
                // essential for side-by-side graph panes.
                crate::surface::SurfaceKind::Graph(pane) => {
                    self.app.raise_floating_pane(pane);
                    self.app.focus = crate::surface::FocusTarget::Graph(pane);
                    if let Some(button) = pointer_button(button)
                        && self
                            .app
                            .graph_pane_pointer_down(pane, button, hit.local.0, hit.local.1)
                    {
                        self.request_redraw();
                    }
                }
                crate::surface::SurfaceKind::Chrome => {}
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
        let mut redraw = false;
        // Leaving the previously hovered pane clears its emphasis.
        if let Some(prev) = self.hovered_pane
            && pane_hit != Some(prev)
        {
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
        self.hovered_pane = pane_hit;
        if let (Some(hit), Some(id)) = (hit, pane_hit) {
            let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                (
                    s.rect.w.round().max(1.0) as u32,
                    s.rect.h.round().max(1.0) as u32,
                )
            });
            if let Some((rw, rh)) = dims {
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
        if redraw {
            self.request_redraw();
        }
    }

    pub(super) fn deliver_move(&mut self, x: f32, y: f32) {
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
        if let Some(crate::surface::SurfaceKind::Content(node)) = self.pointer_capture {
            let local = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Content(node))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            });
            if let Some((local_x, local_y)) = local {
                if self
                    .content_sessions
                    .get_mut(&node)
                    .is_some_and(|session| session.pointer_move(local_x, local_y))
                {
                    self.request_redraw();
                } else if let Some(producer) = self.surface_producers.get_mut(&node) {
                    if let Err(error) = producer.send_pointer_input(mouse_pointer_event(
                        local_x,
                        local_y,
                        inker::PointerPhase::Move,
                        None,
                        self.surface_pointer_buttons,
                        inker::KeyboardModifiers {
                            shift: self.shift,
                            ctrl: self.ctrl,
                            alt: self.alt,
                            meta: false,
                        },
                    )) {
                        tracing::warn!(%node, %error, "surface move delivery failed");
                    }
                    self.request_redraw();
                }
                self.hovered_surface = Some(node);
                self.apply_pending_surface_cursor();
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
                    Some((node, hit.local.0, hit.local.1))
                }
                _ => None,
            });
        let Some((node, local_x, local_y)) = content_hit else {
            self.reset_surface_cursor();
            return;
        };
        let Some(producer) = self.surface_producers.get_mut(&node) else {
            self.reset_surface_cursor();
            return;
        };
        if let Err(error) = producer.send_pointer_input(mouse_pointer_event(
            local_x,
            local_y,
            inker::PointerPhase::Move,
            None,
            self.surface_pointer_buttons,
            inker::KeyboardModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
                alt: self.alt,
                meta: false,
            },
        )) {
            tracing::warn!(%node, %error, "surface hover delivery failed");
        }
        self.hovered_surface = Some(node);
        self.apply_pending_surface_cursor();
    }

    pub(super) fn apply_pending_surface_cursor(&mut self) {
        let shape = self
            .hovered_surface
            .and_then(|node| self.surface_producers.get_mut(&node))
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
        if self.hovered_surface.take().is_none() {
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.set_cursor_visible(true);
            window.set_cursor(CursorIcon::Default);
        }
    }

    pub(super) fn deliver_release(&mut self, x: f32, y: f32, button: MouseButton) {
        self.surface_pointer_buttons.0 &= !surface_button_mask(button).0;
        let captured = self.pointer_capture;
        let graph_capture = match captured {
            Some(crate::surface::SurfaceKind::Graph(pane)) => Some(pane),
            _ => None,
        };
        self.pointer_capture = None;
        if let Some(crate::surface::SurfaceKind::Content(node)) = captured {
            let local = self.surface_plan().into_iter().find_map(|surface| {
                (surface.kind == crate::surface::SurfaceKind::Content(node))
                    .then_some((x - surface.rect.x, y - surface.rect.y))
            });
            let outcome = local.and_then(|(local_x, local_y)| {
                if button == MouseButton::Left
                    && let Some(session) = self.content_sessions.get_mut(&node)
                {
                    return Some(session.pointer_up(local_x, local_y));
                }
                if let (Some(producer), Some(surface_button)) = (
                    self.surface_producers.get_mut(&node),
                    surface_mouse_button(button),
                ) {
                    if let Err(error) = producer.send_pointer_input(mouse_pointer_event(
                        local_x,
                        local_y,
                        inker::PointerPhase::Up,
                        Some(surface_button),
                        self.surface_pointer_buttons,
                        inker::KeyboardModifiers {
                            shift: self.shift,
                            ctrl: self.ctrl,
                            alt: self.alt,
                            meta: false,
                        },
                    )) {
                        tracing::warn!(%node, %error, "surface release delivery failed");
                    }
                }
                None
            });
            if let Some(SessionClick::Navigate(url)) = outcome {
                self.act(Action::OpenAddress(url));
            }
            self.request_redraw();
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
            && self
                .app
                .graph_pane_pointer_up(pane, button, local_x, local_y)
        {
            self.request_redraw();
        }
    }

    /// Lower one winit touch contact to Pointer Events. Each live contact keeps
    /// the content node hit at Started, matching implicit pointer capture for
    /// direct-manipulation devices even if later coordinates leave the tile.
    pub(super) fn deliver_touch(&mut self, touch: Touch) {
        let phase = match touch.phase {
            TouchPhase::Started => inker::PointerPhase::Down,
            TouchPhase::Moved => inker::PointerPhase::Move,
            TouchPhase::Ended => inker::PointerPhase::Up,
            TouchPhase::Cancelled => inker::PointerPhase::Cancel,
        };
        let altitude_angle = match touch.force {
            Some(Force::Calibrated { altitude_angle, .. }) => {
                altitude_angle.map(|angle| angle as f32)
            }
            _ => None,
        };
        let pressure = touch.force.and_then(|force| {
            let value = force.normalized() as f32;
            value.is_finite().then(|| value.clamp(0.0, 1.0))
        });
        self.deliver_touch_contact(
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
        let position = (x, y);
        let active = if phase == inker::PointerPhase::Down {
            let target = crate::surface::hit_test(
                &self.surface_plan(),
                self.app.focus,
                position.0,
                position.1,
            )
            .and_then(|hit| match hit.kind {
                crate::surface::SurfaceKind::Content(node)
                    if self.surface_producers.contains_key(&node) =>
                {
                    Some(node)
                }
                _ => None,
            });
            let Some(node) = target else {
                return;
            };
            let pointer_id = self.allocate_surface_touch_id();
            let active = super::ActiveSurfaceTouch {
                pointer_id,
                node,
                is_primary: self.active_surface_touches.is_empty(),
            };
            self.active_surface_touches.insert(host_id, active);
            active
        } else {
            let Some(active) = self.active_surface_touches.get(&host_id).copied() else {
                return;
            };
            active
        };

        let local = self.surface_plan().into_iter().find_map(|surface| {
            (surface.kind == crate::surface::SurfaceKind::Content(active.node))
                .then_some((position.0 - surface.rect.x, position.1 - surface.rect.y))
        });
        let delivered = local.is_some_and(|(x, y)| {
            self.surface_producers
                .get_mut(&active.node)
                .is_some_and(|producer| {
                    match producer.send_pointer_input(inker::PointerEvent {
                        pointer_id: active.pointer_id,
                        pointer_type: inker::PointerType::Touch,
                        is_primary: active.is_primary,
                        phase,
                        position: inker::PhysicalPosition { x, y },
                        button: match phase {
                            inker::PointerPhase::Down | inker::PointerPhase::Up => {
                                Some(inker::MouseButton::Left)
                            }
                            inker::PointerPhase::Move | inker::PointerPhase::Cancel => None,
                        },
                        buttons: if matches!(phase, inker::PointerPhase::Down | inker::PointerPhase::Move) {
                            inker::PointerButtons::PRIMARY
                        } else {
                            inker::PointerButtons::NONE
                        },
                        width: 1.0,
                        height: 1.0,
                        pressure,
                        tangential_pressure: None,
                        tilt_x: None,
                        tilt_y: None,
                        twist: None,
                        altitude_angle,
                        azimuth_angle: None,
                        modifiers: inker::KeyboardModifiers {
                            shift: self.shift,
                            ctrl: self.ctrl,
                            alt: self.alt,
                            meta: false,
                        },
                    }) {
                        Ok(()) => true,
                        Err(error) => {
                            tracing::warn!(node = %active.node, %error, "surface touch delivery failed");
                            false
                        }
                    }
                })
        });
        if matches!(phase, inker::PointerPhase::Up | inker::PointerPhase::Cancel) {
            self.active_surface_touches.remove(&host_id);
        }
        if delivered {
            self.request_redraw();
        }
    }

    fn allocate_surface_touch_id(&mut self) -> i32 {
        loop {
            let candidate = self.next_surface_touch_id.max(2);
            self.next_surface_touch_id = candidate.checked_add(1).unwrap_or(2);
            if self
                .active_surface_touches
                .values()
                .all(|touch| touch.pointer_id != candidate)
            {
                return candidate;
            }
        }
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

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
