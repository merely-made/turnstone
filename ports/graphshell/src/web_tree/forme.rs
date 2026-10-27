// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Workbench reading tiles and their scoped Forme draft in the retained page.
use super::*;
use cambium::{Slot, button, frisket_with, lens};
use graphshell::forme_workspace::{
    self, FormeWorkspace, GraphshellFormeSession as FormeSession,
    GraphshellPreparedSave as PreparedSave, GraphshellSaveKind as SaveKind,
};
use muniment::{Backend, IndexedDbBackend};
use platen::workspace::{DropEffect, FormeLoadGate, FormeLoadTicket, FormeWriteFence};
use uuid::Uuid;
use workbench::{ContentSource, SplitAxis, Tile, TileEvent, TileTree};

pub(super) struct Pane {
    pub(super) session: FormeSession,
    backend: IndexedDbBackend,
    completed: Rc<RefCell<Option<Result<(), String>>>>,
    pending: Option<PreparedSave>,
    pub(super) retry: Option<SaveKind>,
    pub(super) saving: bool,
    pub(super) pending_read_commands: std::collections::VecDeque<(Uuid, cambium::ReadCommand)>,
    pub(super) status: String,
    pub(super) workbench: bool,
    moving: Option<Uuid>,
    candidate: Option<FormeWorkspace>,
    feedback: Option<(DropEffect, [f32; 4])>,
    drag: Option<(Uuid, (f32, f32), bool)>,
    cancelled_drag: bool,
    pub(super) reading_ui: std::collections::BTreeMap<Uuid, cambium::ReadFind>,
    controls: mere_view::workbench_editor::WorkbenchEditor,
}
impl Pane {
    pub(super) async fn open(product: &product::SavedProduct) -> Result<Self, String> {
        let (backend, session) = product.forme_source().ok_or("workspace store is busy")?;
        let bytes = backend
            .get(&forme_workspace::slot(session))
            .await
            .map_err(|e| e.to_string())?;
        let session = FormeSession::from_bytes(bytes.as_deref(), session, &product.graph())?;
        Ok(Self::from_session(backend, session))
    }
    fn from_session(backend: IndexedDbBackend, session: FormeSession) -> Self {
        let mut pane = Self {
            session,
            backend,
            completed: Rc::new(RefCell::new(None)),
            pending: None,
            retry: None,
            saving: false,
            pending_read_commands: Default::default(),
            status: String::new(),
            workbench: false,
            moving: None,
            candidate: None,
            feedback: None,
            drag: None,
            cancelled_drag: false,
            reading_ui: Default::default(),
            controls: Default::default(),
        };
        pane.refresh_controls();
        pane
    }
    pub(super) fn model(&self) -> &FormeWorkspace {
        self.session.view()
    }
    pub(super) fn current(&self) -> &FormeWorkspace {
        self.candidate.as_ref().unwrap_or_else(|| self.model())
    }
    pub(super) fn install(&self, canvas: &mut Canvas) -> Result<(), String> {
        self.project(canvas, self.model().document.graph_id)
    }
    fn project(&self, canvas: &mut Canvas, owner: Uuid) -> Result<(), String> {
        let region = forme_workspace::project_region(self.current(), owner, canvas.graph())?;
        canvas.set_forme_region(region)
    }
    pub(super) fn save(&mut self, kind: SaveKind) {
        if self.saving {
            return;
        }
        let proposal = match self.session.prepare_save(kind.clone(), now()) {
            Ok(proposal) => proposal,
            Err(error) => {
                self.status = error;
                self.refresh_controls();
                return;
            },
        };
        self.saving = true;
        self.status = "Saving workbench…".into();
        self.retry = Some(kind);
        self.pending = Some(proposal.clone());
        let backend = self.backend.clone();
        let key = forme_workspace::slot(self.model().document.graph_id);
        let bytes = match proposal.bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.saving = false;
                self.pending = None;
                let _ = self.session.finish_save(proposal, Err(error.clone()));
                self.status = error;
                self.refresh_controls();
                return;
            },
        };
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            *completed.borrow_mut() =
                Some(backend.put(&key, &bytes).await.map_err(|e| e.to_string()));
        });
        self.refresh_controls();
    }
    pub(super) fn previewing(&self) -> bool {
        self.candidate.is_some()
    }
    pub(super) fn drop_effect(&self) -> &'static str {
        self.feedback.map_or("none", |(effect, _)| effect.label())
    }
    pub(super) fn ready(&self) -> bool {
        self.completed.borrow().is_some() || self.controls.has_effects()
    }
    fn poll(&mut self) -> bool {
        let Some(result) = self.completed.borrow_mut().take() else {
            return false;
        };
        self.saving = false;
        if let Some(proposal) = self.pending.take() {
            self.status = match self.session.finish_save(proposal, result) {
                Ok(()) => {
                    self.retry = None;
                    "Workbench saved".into()
                },
                Err(error) => format!("Save failed · {error} · Draft retained; retry the save"),
            };
        }
        self.refresh_controls();
        true
    }
    fn refresh_controls(&mut self) {
        self.controls
            .set_snapshot(mere_view::workbench_editor::Snapshot {
                editing: self.session.editing(),
                dirty: self.session.dirty(),
                busy: self.session.busy() || self.saving,
                can_undo: self.session.can_undo(),
                can_redo: self.session.can_redo(),
                visible: self.model().visible,
                notice: self.status.clone(),
            });
    }
}

struct RetiredWrite {
    ticket: u64,
    completed: Rc<RefCell<Option<Result<(), String>>>>,
}
struct Loading {
    ticket: FormeLoadTicket,
    completed: Rc<RefCell<Option<Result<FormeSession, String>>>>,
}
#[derive(Default)]
pub(super) struct OwnerLifecycle {
    gate: FormeLoadGate,
    loading: Option<Loading>,
    writes: FormeWriteFence,
    retired: Vec<RetiredWrite>,
}
pub(super) fn graph_change_ready(page: &TreePage) -> bool {
    page.product
        .as_ref()
        .is_some_and(|p| p.forme_new_session.is_some() || p.forme_reconcile_session.is_some())
}
pub(super) fn owner_ready(page: &TreePage) -> bool {
    if graph_change_ready(page)
        || page
            .forme_owner
            .retired
            .iter()
            .any(|w| w.completed.borrow().is_some())
    {
        return true;
    }
    let Some(ticket) = page.forme_owner.gate.active() else {
        return false;
    };
    let Some((_, owner)) = page.product.as_ref().and_then(|p| p.forme_source()) else {
        return false;
    };
    if owner != ticket.owner {
        return true;
    }
    match &page.forme_owner.loading {
        None => !page.forme_owner.writes.blocks(owner),
        Some(load) => load.completed.borrow().is_some(),
    }
}
pub(super) fn owner_busy(page: &TreePage) -> bool {
    page.forme_owner.gate.active().is_some()
}
fn retire_owner(page: &mut TreePage, owner: Uuid) {
    // Dropping the pane/loading cell prevents old acknowledgements from being
    // installed here. An already admitted old-slot write may finish there.
    if let Some(pane) = page.forme.take() {
        if pane.saving {
            let ticket = page
                .forme_owner
                .writes
                .retire(pane.model().document.graph_id);
            page.forme_owner.retired.push(RetiredWrite {
                ticket,
                completed: pane.completed,
            });
        }
    }
    page.forme_error = None;
    page.forme_owner.loading = None;
    page.forme_owner.gate.request(owner);
    let _ = page.shared.canvas.borrow_mut().set_forme_region(None);
    page.shared.dirty.set(true);
}
pub(super) fn after_graph_change(page: &mut TreePage) {
    let (new_owner, same_owner) = page.product.as_mut().map_or((None, None), |p| {
        (p.forme_new_session.take(), p.forme_reconcile_session.take())
    });
    if new_owner.is_some() || same_owner.is_some() {
        // Durable anchors may survive a graph reopen. An unfinished click is
        // only valid for the exact live canvas that received its first press.
        page.instrument.scope.gesture_epoch = page
            .instrument
            .scope
            .gesture_epoch
            .checked_add(1)
            .expect("Gesture epoch exhausted");
        page.instrument.scope.activation.cancel();
    }
    if let Some(owner) = new_owner {
        retire_owner(page, owner);
        return;
    }
    if let Some(owner) = same_owner {
        if let Some(pane) = &mut page.forme {
            if pane.model().document.graph_id != owner {
                retire_owner(page, owner);
            } else {
                let mut canvas = page.shared.canvas.borrow_mut();
                let missing_drag = pane
                    .drag
                    .as_ref()
                    .is_some_and(|(member, _, _)| canvas.graph().get_node_by_id(*member).is_none());
                let missing_move = pane
                    .moving
                    .is_some_and(|member| canvas.graph().get_node_by_id(member).is_none());
                // A same-owner edit can remove a target while the moving
                // member survives. Retire its transient proposal and cue;
                // keep committed/draft layout and re-resolve the next gesture.
                pane.candidate = None;
                pane.feedback = None;
                if missing_drag || missing_move {
                    pane.drag = None;
                    pane.moving = None;
                    pane.cancelled_drag = missing_drag;
                    pane.session.break_gesture();
                }
                if let Err(error) = pane.project(&mut canvas, owner) {
                    pane.status = error;
                }
            }
        } else if page.forme_owner.gate.active().is_some() {
            // A graph changed while loading: use a fresh snapshot/request.
            retire_owner(page, owner);
        }
        page.shared.dirty.set(true);
    }
}
pub(super) fn poll_owner(page: &mut TreePage) {
    after_graph_change(page);
    page.forme_owner.retired.retain(|write| {
        if write.completed.borrow_mut().take().is_some() {
            page.forme_owner.writes.finish(write.ticket);
            false
        } else {
            true
        }
    });
    // No actual App while a save owns it means unknown, not a new owner.
    let Some((backend, owner)) = page.product.as_ref().and_then(|p| p.forme_source()) else {
        return;
    };
    let wrong_pane = page
        .forme
        .as_ref()
        .is_some_and(|p| p.model().document.graph_id != owner);
    let wrong_request = page
        .forme_owner
        .gate
        .active()
        .is_some_and(|t| t.owner != owner);
    if wrong_pane || wrong_request {
        retire_owner(page, owner);
    }
    let Some(ticket) = page.forme_owner.gate.active() else {
        return;
    };
    if page.forme_owner.writes.blocks(owner) {
        return;
    }
    if page.forme_owner.loading.is_none() {
        let graph = page.product.as_ref().expect("actual Product owner").graph();
        let completed = Rc::new(RefCell::new(None));
        let returned = completed.clone();
        let load_backend = backend.clone();
        let fence = page.forme_owner.writes.clone();
        page.forme_owner.loading = Some(Loading { ticket, completed });
        wasm_bindgen_futures::spawn_local(async move {
            *returned.borrow_mut() = Some(if fence.blocks(ticket.owner) {
                Err("Workbench owner still has an earlier pending write".into())
            } else {
                match load_backend.get(&forme_workspace::slot(ticket.owner)).await {
                    Ok(bytes) => FormeSession::from_bytes(bytes.as_deref(), ticket.owner, &graph),
                    Err(error) => Err(error.to_string()),
                }
            });
        });
    }
    let finished = page.forme_owner.loading.as_ref().and_then(|loading| {
        loading
            .completed
            .borrow_mut()
            .take()
            .map(|result| (loading.ticket, result))
    });
    let Some((ticket, result)) = finished else {
        return;
    };
    if !page.forme_owner.gate.accepts(ticket, Some(owner)) {
        return;
    }
    match result {
        Ok(session) => {
            if !page
                .forme_owner
                .gate
                .finish_loaded(ticket, Some(owner), &session)
            {
                page.forme_error = Some("Workbench load belongs to another graph session".into());
                page.forme_owner.gate.finish_error(ticket, Some(owner));
            } else {
                let pane = Pane::from_session(backend, session);
                match pane.project(&mut page.shared.canvas.borrow_mut(), owner) {
                    Ok(()) => {
                        page.forme = Some(pane);
                        page.forme_error = None;
                    },
                    Err(error) => {
                        page.forme_error = Some(error);
                    },
                }
            }
        },
        Err(error) => {
            page.forme_owner.gate.finish_error(ticket, Some(owner));
            page.forme_error = Some(format!(
                "Workbench could not be restored: {error}. Saved record retained."
            ));
        },
    }
    page.forme_owner.loading = None;
    page.shared.dirty.set(true);
}

fn now() -> u64 {
    js_sys::Date::now() as u64
}
pub(super) fn poll(page: &mut TreePage) {
    let effect = page
        .forme
        .as_mut()
        .and_then(|pane| pane.controls.take_effect());
    let dispatched_effect = effect.is_some();
    match effect {
        Some(mere_view::workbench_editor::Effect::Unlock) => {
            if page
                .forme
                .as_ref()
                .is_some_and(|pane| !pane.session.editing())
            {
                unlock_or_apply(page);
            }
        },
        Some(mere_view::workbench_editor::Effect::Apply) => {
            if page
                .forme
                .as_ref()
                .is_some_and(|pane| pane.session.editing())
            {
                unlock_or_apply(page);
            }
        },
        Some(mere_view::workbench_editor::Effect::Discard) => discard(page),
        Some(mere_view::workbench_editor::Effect::Undo) => undo(page, false),
        Some(mere_view::workbench_editor::Effect::Redo) => undo(page, true),
        Some(mere_view::workbench_editor::Effect::ToggleVisibility) => {
            if let Some(pane) = &mut page.forme {
                pane.save(SaveKind::Visibility);
            }
        },
        None => {},
    }
    let completed = page.forme.as_mut().is_some_and(Pane::poll);
    if completed {
        changed(page);
    } else if dispatched_effect {
        // Consuming a refused action should refresh the status/buttons without
        // clearing an in-progress tile preview or drag candidate.
        if let Some(pane) = &mut page.forme {
            pane.refresh_controls();
        }
        page.shared.dirty.set(true);
    }
    let command = page
        .forme
        .as_mut()
        .filter(|pane| !pane.saving)
        .and_then(|pane| pane.pending_read_commands.pop_front());
    if let Some((member, command)) = command {
        super::reading::read_command(page, member, command);
    }
}
fn changed(page: &mut TreePage) {
    if let Some(pane) = &mut page.forme {
        pane.candidate = None;
        pane.feedback = None;
        pane.moving = None;
        pane.drag = None;
        pane.refresh_controls();
        if let Err(error) = pane.install(&mut page.shared.canvas.borrow_mut()) {
            pane.status = error;
            pane.refresh_controls();
        }
    }
    page.shared.dirty.set(true);
}
pub(super) fn edit(
    page: &mut TreePage,
    key: Option<String>,
    auto_open: bool,
    change: impl FnOnce(&mut FormeWorkspace, &mere::kernel::graph::Graph) -> Result<(), String>,
) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving {
        return;
    }
    let immediate = !pane.session.editing() && auto_open;
    if immediate {
        let _ = pane.session.unlock();
    }
    let result = pane.session.edit(key, now(), |workspace| {
        change(workspace, page.shared.canvas.borrow().graph())
    });
    match result {
        Ok(()) => {
            pane.status = if pane.session.dirty() {
                "Pending Forme changes"
            } else {
                "Forme draft matches saved arrangement"
            }
            .into();
            if immediate {
                pane.save(SaveKind::Apply);
            }
        },
        Err(error) => {
            if immediate {
                let _ = pane.session.discard();
            }
            pane.status = error;
        },
    }
    changed(page);
}
pub(super) fn open_selected(page: &mut TreePage) {
    super::scope_navigation::capture_origin(page);
    let selected = page.shared.canvas.borrow().selected_members();
    if selected.is_empty() {
        if let Some(pane) = &mut page.forme {
            pane.status = "Select a node to open it in the workbench".into();
        }
        return;
    }
    edit(page, None, true, |workspace, graph| {
        for member in selected {
            workspace.open(member, graph)?;
        }
        Ok(())
    });
    if let Some(pane) = &mut page.forme {
        pane.workbench = true;
    }
    if let Some(member) = page
        .forme
        .as_ref()
        .and_then(|p| p.current().preferences.focused_reading)
    {
        super::scope_navigation::record_reading(page, member);
    }
}
/// Switch projections of the same graph and saved Forme arrangement. Only a
/// first Read with an empty arrangement admits the already-selected members.
pub(super) fn show_projection(page: &mut TreePage, map: bool) {
    if map {
        super::scope_navigation::command(
            page,
            graphshell::scope_policy::ScopeCommand::To(
                graphshell::scope_policy::ScopeLevel::Graph.into(),
            ),
        );
    } else {
        let member = page.shared.canvas.borrow().focused_member().or_else(|| {
            page.forme
                .as_ref()
                .and_then(|p| p.current().preferences.focused_reading)
        });
        if let Some(member) = member {
            read_related(page, member);
        } else {
            open_selected(page);
        }
    }
    page.shared.dirty.set(true);
}
pub(super) fn open_retained(page: &mut TreePage, member: Uuid) {
    super::scope_navigation::capture_origin(page);
    let (anchors, trail) = super::reading::capture_progress(page);
    edit(page, None, true, move |workspace, graph| {
        workspace.preferences.reading_anchors = anchors;
        workspace.preferences.reading_trail = trail;
        workspace.open(member, graph)
    });
    if let Some(pane) = &mut page.forme {
        pane.workbench = true;
    }
    super::scope_navigation::record_reading(page, member);
}

pub(super) fn read_related(page: &mut TreePage, member: Uuid) {
    super::scope_navigation::capture_origin(page);
    let (anchors, trail) = super::reading::capture_progress(page);
    edit(page, None, true, move |workspace, graph| {
        workspace.preferences.reading_anchors = anchors;
        workspace.preferences.reading_trail = trail;
        workspace.read(member, graph)
    });
    if let Some(pane) = &mut page.forme {
        pane.workbench = true;
    }
    super::scope_navigation::record_reading(page, member);
    page.shared.dirty.set(true);
}

pub(super) fn bind_inspector(
    page: &mut TreePage,
    source: graphshell::pane_source::InspectorSource,
) {
    if let Some(pane) = &mut page.forme {
        if pane.saving {
            return;
        }
        let update = pane.session.preferences_update(false, move |preferences| {
            preferences.inspector_source = source.clone();
        });
        pane.save(update);
    }
    page.shared.dirty.set(true);
}
fn arrange(page: &mut TreePage, axis: Option<SplitAxis>) {
    edit(page, None, false, |workspace, _| {
        let mut layout = workspace.layout();
        match axis {
            None => layout.stack_all(),
            Some(SplitAxis::Row) => layout.split_all(),
            Some(SplitAxis::Column) => {
                for pair in layout.open_members().windows(2) {
                    layout.split_beside_axis(pair[1], pair[0], SplitAxis::Column, true);
                }
            },
        }
        workspace.keep_layout(&layout);
        Ok(())
    });
}
fn tile_event(page: &mut TreePage, event: TileEvent) {
    if let TileEvent::Closed(tile) = &event {
        if let Some(member) = page
            .forme
            .as_ref()
            .and_then(|p| p.model().member_for_tile(*tile))
        {
            close_reading(page, member);
        }
        return;
    }
    let activated = if let TileEvent::Activated(tile) = &event {
        page.forme
            .as_ref()
            .and_then(|p| p.model().member_for_tile(*tile))
    } else {
        None
    };
    let key = match &event {
        TileEvent::DividerMoved { split, .. } => Some(format!("divider:{:?}", split.0)),
        _ => None,
    };
    edit(
        page,
        key,
        matches!(event, TileEvent::Activated(_) | TileEvent::Closed(_)),
        |workspace, graph| workspace.event(event, graph),
    );
    if let Some(member) = activated {
        page.shared.canvas.borrow_mut().select_member(member);
        selection_changed(page);
        super::scope_navigation::record_reading(page, member);
    }
}

pub(super) fn close_reading(page: &mut TreePage, member: Uuid) {
    let Some(pane) = page.forme.as_ref() else {
        return;
    };
    if !pane.current().members().contains(&member) {
        return;
    }
    if pane.saving {
        if page.instrument.scope.pending_closes.len() < 32 {
            page.instrument.scope.pending_closes.push_back(member);
        }
        page.shared.dirty.set(true);
        return;
    }
    let Some(tile) = pane.current().tile_for_member(member) else {
        return;
    };
    let stack = pane
        .current()
        .layout()
        .slot_views()
        .find(|slot| slot.members.contains(&member))
        .map(|slot| slot.members.to_vec())
        .unwrap_or_default();
    let next = cambium::scope_navigation::reading_after_close_in_stack(
        &pane.current().members(),
        &stack,
        pane.current().preferences.focused_reading.as_ref(),
        &member,
    );
    let current = pane.current().preferences.focused_reading;
    let reading = pane.workbench;
    super::scope_navigation::capture_origin(page);
    let (anchors, trail) = super::reading::capture_progress(page);
    if next.is_none() && reading {
        super::scope_navigation::command(
            page,
            graphshell::scope_policy::ScopeCommand::To(
                graphshell::scope_policy::ScopeLevel::Neighborhood.into(),
            ),
        );
    }
    edit(page, None, true, move |workspace, graph| {
        workspace.preferences.reading_anchors = anchors;
        workspace.preferences.reading_trail = trail;
        workspace.event(TileEvent::Closed(tile), graph)
    });
    if current == Some(member)
        && let Some(next) = next
    {
        page.shared.canvas.borrow_mut().select_member(next);
        selection_changed(page);
        if reading {
            super::scope_navigation::record_reading(page, next);
        }
    }
    page.shared.dirty.set(true);
}
fn translate(page: &mut TreePage, dx: f32, dy: f32, scale: f32) {
    edit(page, None, false, |workspace, _| {
        let [x, y, r, b] = workspace.bounds;
        let (cx, cy) = (x + (r - x) / 2. + dx, y + (b - y) / 2. + dy);
        let (w, h) = (((r - x) * scale).max(80.), ((b - y) * scale).max(80.));
        workspace.bounds = [cx - w / 2., cy - h / 2., cx + w / 2., cy + h / 2.];
        Ok(())
    });
}
pub(super) fn selection_changed(page: &mut TreePage) {
    let canvas = page.shared.canvas.borrow();
    page.picked = canvas.focused_url().map(str::to_owned);
    if let Some(product) = &mut page.product {
        product.select(canvas.selected_members().first().copied());
    }
}
fn begin_move(page: &mut TreePage, member: Uuid) {
    if let Some(pane) = &mut page.forme {
        if pane.saving || !pane.session.editing() {
            return;
        }
        pane.candidate = None;
        pane.feedback = None;
        pane.moving = Some(member);
        pane.session.break_gesture();
        let _ = pane.install(&mut page.shared.canvas.borrow_mut());
    }
    page.shared.dirty.set(true);
}
fn preview(page: &mut TreePage, target: Uuid, edge: workbench::Edge) {
    let Some((_, owner)) = page.product.as_ref().and_then(|p| p.forme_source()) else {
        return;
    };
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving || !pane.session.editing() {
        return;
    }
    let Some(moving) = pane.moving else {
        return;
    };
    let result = pane.session.edge_preview_candidate(
        owner,
        moving,
        target,
        edge,
        page.shared.canvas.borrow().graph(),
    );
    match result {
        Ok(candidate) => {
            let region = pane.model().region();
            pane.feedback = region
                .cells
                .iter()
                .find(|cell| cell.member == target)
                .map(|cell| (DropEffect::Split(edge), region.cell_world_bounds(cell)));
            pane.candidate = Some(candidate);
            if let Ok(region) = forme_workspace::project_region(
                pane.current(),
                owner,
                page.shared.canvas.borrow().graph(),
            ) {
                let _ = page.shared.canvas.borrow_mut().set_forme_region(region);
            }
            pane.status = "Preview ready · Apply move or cancel".into();
        },
        Err(error) => {
            pane.status = error;
        },
    }
    page.shared.dirty.set(true);
}
fn finish_preview(page: &mut TreePage, apply: bool) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving {
        return;
    }
    let candidate = pane.candidate.take();
    pane.feedback = None;
    pane.moving = None;
    pane.cancelled_drag |= !apply && pane.drag.is_some();
    pane.drag = None;
    if apply && let Some(candidate) = candidate {
        edit(page, None, false, |workspace, _| {
            *workspace = candidate;
            Ok(())
        });
    } else {
        pane.status = "Tile move cancelled".into();
        let _ = pane.install(&mut page.shared.canvas.borrow_mut());
        page.shared.dirty.set(true);
    }
}
fn unlock_or_apply(page: &mut TreePage) {
    if let Some(pane) = &mut page.forme {
        if pane.saving {
            return;
        }
        if pane.session.editing() {
            if pane.previewing() || pane.moving.is_some() || pane.drag.is_some() {
                pane.status = "Apply or cancel the tile move before locking the forme".into();
                return;
            }
            pane.save(SaveKind::Apply);
        } else {
            match pane.session.unlock() {
                Ok(()) => pane.status = "Forme draft open · drag nodes as tile handles".into(),
                Err(e) => pane.status = e,
            }
        }
    }
    changed(page);
}
fn discard(page: &mut TreePage) {
    if let Some(pane) = &mut page.forme {
        match pane.session.discard() {
            Ok(()) => pane.status = "Forme changes discarded".into(),
            Err(e) => pane.status = e,
        }
    }
    changed(page);
}
fn undo(page: &mut TreePage, redo: bool) {
    if page
        .forme
        .as_ref()
        .is_some_and(|pane| pane.previewing() || pane.drag.is_some() || pane.moving.is_some())
    {
        finish_preview(page, false);
        return;
    }
    if let Some(pane) = &mut page.forme {
        if pane.session.editing() {
            match pane.session.draft_undo(redo) {
                Ok(true) => pane.status = "Forme draft history restored".into(),
                Ok(false) => return,
                Err(e) => pane.status = e,
            }
        } else {
            pane.save(if redo { SaveKind::Redo } else { SaveKind::Undo });
        }
    }
    changed(page);
}
pub(super) fn shortcut(page: &mut TreePage, event: &cambium::KeyEvent) -> bool {
    if page.forme.is_none() || event.prop.default_prevented() {
        return false;
    }
    if matches!(event.key, Key::Named(NamedKey::Escape))
        && page
            .forme
            .as_ref()
            .is_some_and(|p| p.previewing() || p.drag.is_some())
    {
        finish_preview(page, false);
        return true;
    }
    if let Some(member) = page
        .forme
        .as_ref()
        .filter(|pane| pane.workbench)
        .and_then(|pane| pane.model().preferences.focused_reading)
    {
        let find_open = page
            .forme
            .as_ref()
            .and_then(|pane| pane.reading_ui.get(&member))
            .is_some_and(|ui| ui.open);
        if matches!(event.key, Key::Named(NamedKey::Escape)) && find_open {
            super::reading::read_command(page, member, cambium::ReadCommand::CloseFind);
            return true;
        }
        if (event.mods.ctrl || event.mods.meta)
            && !event.mods.alt
            && matches!(&event.key, Key::Character(key) if key.eq_ignore_ascii_case("f"))
        {
            super::reading::read_command(page, member, cambium::ReadCommand::Find);
            return true;
        }
    }
    if (event.mods.ctrl || event.mods.meta)
        && !event.mods.alt
        && let Key::Character(key) = &event.key
    {
        if key.eq_ignore_ascii_case("z") {
            undo(page, event.mods.shift);
            return true;
        }
        if key.eq_ignore_ascii_case("y") {
            undo(page, true);
            return true;
        }
    }
    false
}
/// An unlocked Forme explicitly turns graph nodes into tile handles. A
/// click remains selection; only motion past the threshold previews a drop.
pub(super) fn pointer(page: &mut TreePage, event: &cambium::PointerEvent) -> bool {
    let Some(pane) = &mut page.forme else {
        return false;
    };
    if event.phase == cambium::PointerPhase::Cancel {
        if pane.drag.is_none() && !pane.cancelled_drag {
            return false;
        }
        pane.drag = None;
        pane.candidate = None;
        pane.feedback = None;
        pane.moving = None;
        pane.cancelled_drag = false;
        pane.session.break_gesture();
        let _ = pane.install(&mut page.shared.canvas.borrow_mut());
        page.shared.dirty.set(true);
        return true;
    }
    if pane.cancelled_drag && event.phase != cambium::PointerPhase::Down {
        if event.phase == cambium::PointerPhase::Up {
            pane.cancelled_drag = false;
        }
        return true;
    }
    if !pane.session.editing() || pane.saving {
        return false;
    }
    match event.phase {
        cambium::PointerPhase::Down => {
            pane.cancelled_drag = false;
            let Some(member) = page
                .shared
                .canvas
                .borrow()
                .node_at_screen(event.local.0, event.local.1)
            else {
                return false;
            };
            pane.candidate = None;
            pane.feedback = None;
            pane.moving = None;
            pane.session.break_gesture();
            pane.drag = Some((member, event.local, false));
        },
        cambium::PointerPhase::Move => {
            let Some((member, start, active)) = &mut pane.drag else {
                return false;
            };
            *active |= (event.local.0 - start.0).hypot(event.local.1 - start.1) >= 4.;
            if !*active {
                return true;
            }
            let canvas = page.shared.canvas.borrow();
            let point = canvas.world_point_at(event.local);
            let preview = pane.session.drop_proposal(*member, point, canvas.graph());
            drop(canvas);
            let previous_feedback = pane.feedback;
            match preview {
                Ok(proposal) => {
                    pane.feedback = proposal.as_ref().map(|p| (p.effect, p.bounds));
                    pane.candidate = proposal.map(|p| p.workspace);
                    pane.status = if pane.candidate.is_some() {
                        "Drop to edit the Forme draft · Escape cancels"
                    } else {
                        "Outside a drop region · release to cancel"
                    }
                    .into();
                },
                Err(error) => {
                    pane.candidate = None;
                    pane.feedback = None;
                    pane.status = error;
                },
            }
            let _ = pane.install(&mut page.shared.canvas.borrow_mut());
            // Rebuild only when the displayed target/effect changes. Motion
            // within one drop zone keeps the retained cue and pointer capture.
            if previous_feedback == pane.feedback {
                event.defer_rebuild();
            }
        },
        cambium::PointerPhase::Up => {
            let Some((member, _start, active)) = pane.drag.take() else {
                return false;
            };
            if active {
                // Resolve the release point as well; a final release outside
                // the field must not accept the last in-bounds preview.
                let canvas = page.shared.canvas.borrow();
                let preview = pane.session.drop_preview(
                    member,
                    canvas.world_point_at(event.local),
                    canvas.graph(),
                );
                drop(canvas);
                pane.candidate = preview.ok().flatten();
                finish_preview(page, true);
            } else {
                let mut canvas = page.shared.canvas.borrow_mut();
                canvas.pointer_down(
                    mere::canvas::PointerButton::Left,
                    event.local.0,
                    event.local.1,
                );
                canvas.pointer_up(
                    mere::canvas::PointerButton::Left,
                    event.local.0,
                    event.local.1,
                );
                drop(canvas);
                selection_changed(page);
            }
        },
        cambium::PointerPhase::Cancel => {
            unreachable!("cancellation handled before draft admission")
        },
    }
    page.shared.dirty.set(true);
    true
}

/// A view-only cue above the graph. It neither catches pointer input nor
/// changes the draft; geometry and intent come from the resolved drop proposal.
pub(super) fn drop_feedback(page: &TreePage) -> Option<Child> {
    let pane = page.forme.as_ref()?;
    let (effect, bounds) = pane.feedback?;
    let canvas = page.shared.canvas.borrow();
    let [x, y, r, b] = effect.preview_bounds(bounds);
    let (left, top) = canvas.screen_point_of((x, y));
    let (right, bottom) = canvas.screen_point_of((r, b));
    let prompt = if pane.drag.is_some() {
        "Release to edit draft"
    } else {
        "Apply move to accept"
    };
    Some(Box::new(
        el(
            "div",
            el(
                "span",
                format!("{} · {prompt} · Escape cancels", effect.label()),
            )
            .attr("class", "forme-drop-label"),
        )
        .attr(
            "class",
            if effect == DropEffect::Stack {
                "forme-drop-cue forme-drop-stack"
            } else {
                "forme-drop-cue"
            },
        )
        .attr("role", "status")
        .attr("data-forme-drop-effect", effect.label())
        .attr(
            "style",
            format!(
                "left:{left}px;top:{top}px;width:{}px;height:{}px;",
                right - left,
                bottom - top
            ),
        ),
    ))
}

pub(super) fn toolbar(page: &TreePage) -> Child {
    let Some(pane) = &page.forme else {
        return Box::new(el("span", ()));
    };
    Box::new(
        el(
            "div",
            (
                button("Open selected in workbench", |p: &mut TreePage, _| {
                    open_selected(p)
                }),
                button(
                    if pane.workbench {
                        "Return to Mere"
                    } else {
                        "Show workbench"
                    },
                    |p: &mut TreePage, _| {
                        if let Some(f) = &mut p.forme {
                            f.workbench = !f.workbench;
                        }
                        p.shared.dirty.set(true);
                    },
                ),
            ),
        )
        .attr("class", "tree-controls"),
    )
}

fn split_controls(tree: &workbench::TileTree, path: Vec<usize>, out: &mut Vec<Child>) {
    let workbench::TileTree::Split { children, .. } = tree else {
        return;
    };
    let name = if path.is_empty() {
        "root".into()
    } else {
        cambium::encode_pane_path(&path)
    };
    for (label, factor) in [("Enlarge", 1.2), ("Shrink", 1. / 1.2)] {
        let at = path.clone();
        out.push(Box::new(button(
            format!("{label} first region in split {name}"),
            move |p: &mut TreePage, _| {
                let Some(f) = &mut p.forme else {
                    return;
                };
                if f.saving || f.model().locked {
                    return;
                }
                if let Some(mut shares) = f.model().layout().split_fractions(&at) {
                    f.session.break_gesture();
                    shares[0] *= factor;
                    tile_event(
                        p,
                        TileEvent::DividerMoved {
                            split: workbench::TilePath(at.clone()),
                            fractions: shares,
                        },
                    );
                }
            },
        )));
    }
    for (i, child) in children.iter().enumerate() {
        let mut at = path.clone();
        at.push(i);
        split_controls(&child.tree, at, out);
    }
}
pub(super) fn section(page: &TreePage) -> Child {
    let canvas = page.shared.canvas.borrow();
    let mut fields: Vec<_> = canvas.graph().fields().filter(|f| f.is_active()).collect();
    fields.sort_by_key(|f| f.id.as_uuid());
    let mut children: Vec<Child> = vec![Box::new(el("h2", "Fields"))];
    if let Some(error) = &page.forme_error {
        children.push(Box::new(el("p", error.clone()).attr("role", "alert")));
    }
    if let Some(pane) = &page.forme {
        let forme_start = children.len();
        children.push(Box::new(lens(
            |editor: &mut mere_view::workbench_editor::WorkbenchEditor| {
                mere_view::workbench_editor::view(editor)
            },
            |page: &mut TreePage| &mut page.forme.as_mut().expect("Forme pane mounted").controls,
        )));
        children.push(Box::new(el(
            "p",
            format!(
                "Workbench · {} accesses · {} regions",
                pane.model().members().len(),
                pane.model().region().cells.len()
            ),
        )));
        let pins = pane
            .current()
            .region()
            .cells
            .iter()
            .filter(|c| {
                canvas
                    .graph()
                    .get_node_key_by_id(c.member)
                    .is_some_and(|key| {
                        canvas.arrangement_role_of(key) == mere::canvas::Role::Pinned
                    })
            })
            .count();
        if pins > 0 {
            children.push(Box::new(el(
                "p",
                format!("{pins} position-pinned accesses keep their own positions."),
            )));
        }
        children.push(Box::new(button(
            "Select forme members",
            |p: &mut TreePage, _| {
                if let Some(f) = &p.forme {
                    let mut canvas = p.shared.canvas.borrow_mut();
                    canvas.clear_selection();
                    for m in f.model().members() {
                        canvas.toggle_select_member(m);
                    }
                }
                selection_changed(p);
                p.shared.dirty.set(true);
            },
        )));
        if !pane.model().locked {
            if let Some(tree) = pane.model().tile_tree(canvas.graph()) {
                split_controls(&tree, Vec::new(), &mut children);
            }
            for (label, axis) in [
                ("Arrange side by side", Some(SplitAxis::Row)),
                ("Arrange top to bottom", Some(SplitAxis::Column)),
                ("Group as tabs", None),
            ] {
                children.push(Box::new(button(label, move |p: &mut TreePage, _| {
                    arrange(p, axis)
                })));
            }
            for (label, dx, dy, scale) in [
                ("Move forme left", -40., 0., 1.),
                ("Move forme right", 40., 0., 1.),
                ("Move forme up", 0., -40., 1.),
                ("Move forme down", 0., 40., 1.),
                ("Enlarge forme", 0., 0., 1.2),
                ("Shrink forme", 0., 0., 1. / 1.2),
            ] {
                children.push(Box::new(button(label, move |p: &mut TreePage, _| {
                    translate(p, dx, dy, scale)
                })));
            }
        }
        if pane.moving.is_some() {
            children.push(Box::new(el(
                "p",
                "Choose a tile's placement preview, then apply or cancel.",
            )));
            children.push(Box::new(button(
                "Apply tile move",
                |p: &mut TreePage, _| finish_preview(p, true),
            )));
            children.push(Box::new(button(
                "Cancel tile move",
                |p: &mut TreePage, _| finish_preview(p, false),
            )));
        }
        children.push(Box::new(button(
            "Retry workbench save",
            |p: &mut TreePage, _| {
                if let Some(f) = &mut p.forme {
                    if let Some(kind) = f.retry.clone() {
                        f.save(kind);
                    }
                }
            },
        )));
        if !pane.status.is_empty() {
            children.push(Box::new(status_note("", pane.status.clone())));
        }
        let forme_controls = children.split_off(forme_start);
        children.push(Box::new(cambium::on_key(
            el("section", forme_controls).attr("aria-label", "Forme controls"),
            |p: &mut TreePage, event| {
                if shortcut(p, &event) {
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
        )));
    }
    // Only Forme chrome owns these shortcuts; another field's controls keep
    // their own edit/history routing.
    for field in fields {
        let id = field.id;
        children.push(Box::new(el("h3", field.name.clone())));
        children.push(Box::new(el("p", format!("Extent: {:?}", field.extent))));
        children.push(Box::new(button(
            "Locate field",
            move |p: &mut TreePage, _| {
                p.shared.canvas.borrow_mut().center_on_field(id);
                p.shared.dirty.set(true);
            },
        )));
        children.push(Box::new(button(
            if canvas.field_visible(id) {
                "Hide field"
            } else {
                "Show field"
            },
            move |p: &mut TreePage, _| {
                p.shared.canvas.borrow_mut().toggle_field_visible(id);
                p.shared.dirty.set(true);
            },
        )));
    }
    if children.len() == 1 {
        children.push(Box::new(el("p", "No fields in this view")));
    }
    Box::new(
        el("section", children)
            .attr("class", "tools-section")
            .attr("aria-label", "Fields"),
    )
}

/// Offer an opened page's unvisited links as suggestions, once per page and
/// only while suggestions are on (`link_suggestions.rs` in pictograph).
fn offer_link_values(page: &TreePage, member: Uuid, links: Vec<String>) {
    let Ok(mut canvas) = page.shared.canvas.try_borrow_mut() else {
        return;
    };
    if !canvas.suggestion_settings().on || canvas.has_offered_links(member) {
        return;
    }
    canvas.offer_page_links(member, links);
    page.shared.dirty.set(true);
}

struct ReadingTile {
    address: String,
    title: String,
    tags: Vec<String>,
}

fn tile_members(tree: &TileTree, out: &mut Vec<Uuid>) {
    match tree {
        TileTree::Stack(stack) => {
            for tile in &stack.tabs {
                if let ContentSource::Open { id, .. } = &tile.content
                    && let Ok(id) = id.parse()
                {
                    out.push(id);
                }
            }
        },
        TileTree::Split { children, .. } => {
            for child in children {
                tile_members(&child.tree, out);
            }
        },
    }
}

pub(super) fn workbench(page: &TreePage) -> Option<Child> {
    let pane = page.forme.as_ref().filter(|p| p.workbench)?;
    let displayed = pane.candidate.as_ref().unwrap_or_else(|| pane.model());
    let (tree, tile_metadata) = {
        let canvas = page.shared.canvas.borrow();
        let graph = canvas.graph();
        let tree = displayed.tile_tree(graph);
        let mut metadata = std::collections::BTreeMap::new();
        if let Some(tree) = &tree {
            let mut members = Vec::new();
            tile_members(tree, &mut members);
            for member in members {
                let Some((key, node)) = graph.get_node_by_id(member) else {
                    continue;
                };
                let mut tags: Vec<_> = graph
                    .node_content_tags(key)
                    .unwrap_or_default()
                    .into_iter()
                    .collect();
                tags.sort();
                metadata.insert(
                    member,
                    ReadingTile {
                        address: node.url().to_owned(),
                        title: node.title.clone(),
                        tags,
                    },
                );
            }
        }
        (tree, metadata)
    };
    Some(match tree {
        None => Box::new(el(
            "p",
            "No arranged documents. Select a document in Map, then choose Panes and Read selected.",
        )),
        Some(tree) => {
            let moving = pane.moving;
            let locked = pane.model().locked;
            let reading_shared = page.shared.clone();
            let reading_store = page.product.as_ref().and_then(|p| p.retained_store());
            let mut query_appearances = std::collections::BTreeMap::new();
            let mut appearances = std::collections::BTreeMap::new();
            // Seed the portable owner/access leases before a clip tile can
            // start its first asynchronous read during this initial render.
            super::reading::prepare(page);
            for member in super::reading::active_members(&tree) {
                if let Some(snapshot) = super::query_view::appearance(page, member) {
                    query_appearances.insert(member, snapshot);
                    continue;
                }
                let (appearance, links) = {
                    let canvas = page.shared.canvas.borrow();
                    let graph = canvas.graph();
                    let Some((_, node)) = graph.get_node_by_id(member) else {
                        continue;
                    };
                    let address = node.url();
                    let appearance = if matches!(
                        mere_document_lanes::retained_clip::request(address),
                        Ok(Some(mere_document_lanes::retained_clip::ClipRequest::Read(
                            _
                        )))
                    ) {
                        Some(super::reading::clip_appearance(
                            &reading_shared,
                            member,
                            address,
                            reading_store.clone(),
                        ))
                    } else {
                        node.body.as_deref().map(|body| {
                            super::reading::appearance_parts(
                                &reading_shared,
                                member,
                                address,
                                &node.title,
                                node.media_type.as_deref().unwrap_or("text/plain"),
                                body,
                            )
                        })
                    };
                    let links =
                        if canvas.suggestion_settings().on && !canvas.has_offered_links(member) {
                            node.body.as_deref().map(|body| {
                                fleece::page_links::html_links(
                                    address,
                                    node.media_type.as_deref(),
                                    body,
                                )
                            })
                        } else {
                            None
                        };
                    (appearance, links)
                };
                if let Some(links) = links {
                    offer_link_values(page, member, links);
                }
                if let Some(appearance) = appearance {
                    appearances.insert(member, appearance);
                }
            }
            let reading_toolbars: std::collections::BTreeMap<_, _> = pane
                .model()
                .members()
                .iter()
                .map(|member| (*member, super::reading::toolbar_snapshot(page, *member)))
                .collect();
            let tab_visibility: std::collections::BTreeMap<_, _> = displayed
                .layout()
                .slot_views()
                .flat_map(|stack| {
                    let visible = cambium::TabStripPolicy::Multiple.visible(stack.members.len());
                    stack
                        .members
                        .iter()
                        .copied()
                        .map(move |member| (member, visible))
                        .collect::<Vec<_>>()
                })
                .collect();
            // Frisket clones its fill closure for retained slots. Share these
            // immutable render snapshots instead of copying every tile's map.
            let tile_metadata = Rc::new(tile_metadata);
            let query_appearances = Rc::new(query_appearances);
            let appearances = Rc::new(appearances);
            let reading_toolbars = Rc::new(reading_toolbars);
            let tab_visibility = Rc::new(tab_visibility);
            let reading = move |tile: &Tile| -> Slot<TreePage, ()> {
                let ContentSource::Open { id, .. } = &tile.content else {
                    return Slot::Hole;
                };
                let Some(member) = id.parse::<Uuid>().ok() else {
                    return Slot::View(Box::new(el("p", "This access is no longer present")));
                };
                let Some(metadata) = tile_metadata.get(&member) else {
                    return Slot::View(Box::new(el("p", "This access is no longer present")));
                };
                let title = metadata.title.clone();
                let address = metadata.address.clone();
                let mut controls: Vec<Child> = Vec::new();
                if !metadata.tags.is_empty() {
                    controls.push(Box::new(
                        el("p", metadata.tags.join(" · ")).attr("class", "shell-reading-status"),
                    ));
                }
                controls.push(super::reading::toolbar(
                    member,
                    reading_toolbars[&member].clone(),
                    tab_visibility.get(&member).copied().unwrap_or(false),
                ));
                if let Some(snapshot) = query_appearances.get(&member) {
                    controls.push(super::query_view::render(snapshot.clone()));
                }
                if let Some(appearance) = appearances.get(&member) {
                    controls.push(super::reading::render(appearance.clone()));
                }
                if !locked {
                    controls.push(Box::new(
                        button("Move tile", move |p: &mut TreePage, _| {
                            begin_move(p, member);
                        })
                        .attr("data-forme-move", "true")
                        .attr(
                            "data-forme-control",
                            json!([address.as_str(), "move"]).to_string(),
                        )
                        .attr("aria-label", format!("Move tile: {title}")),
                    ));
                    if moving.is_some_and(|m| m != member) {
                        for (label, edge) in [
                            ("Preview left", workbench::Edge::Left),
                            ("Preview right", workbench::Edge::Right),
                            ("Preview above", workbench::Edge::Top),
                            ("Preview below", workbench::Edge::Bottom),
                        ] {
                            controls.push(Box::new(
                                button(label, move |p: &mut TreePage, _| preview(p, member, edge))
                                    .attr("data-forme-preview", format!("{edge:?}").to_lowercase())
                                    .attr(
                                        "data-forme-control",
                                        json!([
                                            address.as_str(),
                                            format!("{edge:?}").to_lowercase()
                                        ])
                                        .to_string(),
                                    )
                                    .attr("aria-label", format!("{label}: {title}")),
                            ));
                        }
                    }
                }
                Slot::View(Box::new(
                    el("article", controls)
                        .attr(
                            "class",
                            if appearances.contains_key(&member) {
                                "reading-pane retained-reading-pane"
                            } else {
                                "reading-pane"
                            },
                        )
                        .attr("aria-label", format!("Reading access {title}"))
                        .attr("data-forme-access", member.to_string())
                        .attr("data-forme-address", address),
                ))
            };
            Box::new(
                cambium::on_key(
                    el(
                        "div",
                        cambium::frisket_with_tab_policy(
                            &tree,
                            None,
                            &|_| None,
                            cambium::TabStripPolicy::Multiple,
                            tile_event,
                            reading,
                        ),
                    ),
                    |p: &mut TreePage, event| {
                        if shortcut(p, &event) {
                            event.prevent_default();
                            event.stop_propagation();
                        }
                    },
                )
                .attr("class", "forme-workbench cambium-document-rail"),
            )
        },
    })
}
pub(super) const SHEET: &str = " .forme-workbench { width:100%;height:100%;min-height:0; } \
    .forme-workbench .frisket-content { background:inherit; } .reading-pane { padding:8px;color:inherit;height:100%;box-sizing:border-box;min-height:0;overflow:auto; } \
    .retained-reading-pane { display:flex;flex-direction:column;overflow:hidden; } \
    .retained-reading-pane > button, .retained-reading-pane > p { flex-shrink:0; } \
    .pelt-reading { display:flex;flex-direction:column;flex-grow:1;flex-shrink:1;flex-basis:0px;min-height:0; } \
    .pelt-reading > p { flex-shrink:0; } \
    .reading-pane button { padding:4px 8px;min-height:28px;border:1px solid currentColor; } .reading-map-control { align-self:flex-start;margin-bottom:6px; } \
    .reading-address { overflow-wrap:anywhere; } .reading-body { white-space:pre-wrap;overflow-wrap:anywhere; } ";

/// Activate an already arranged member without changing its tile or split.
pub(super) fn activate_from_rail(page: &mut TreePage, member: Uuid) {
    super::scope_navigation::capture_origin(page);
    if !page
        .forme
        .as_ref()
        .is_some_and(|pane| pane.current().layout().has_tile(member))
    {
        return;
    }
    edit(page, None, true, |workspace, graph| {
        workspace.open(member, graph)
    });
    if let Some(pane) = &mut page.forme {
        pane.workbench = true;
    }
    page.shared.canvas.borrow_mut().select_member(member);
    selection_changed(page);
    super::scope_navigation::record_reading(page, member);
    page.shared.dirty.set(true);
}
