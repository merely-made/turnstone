// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The host's scoped Forme editing session. Draft history is private; saved
//! layout history and the canonical workspace commit together in one slot.
//! No document content, resource metadata or graph source is snapshotted.
use crate::forme_workspace::{FormeWorkspace, slot};
use edit_history::History;
use mere::{forme::Arrangement, kernel::graph::Graph};
use muniment::Backend;
use platen::projection_geometry::TreeGeometry;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const HISTORY_LIMIT: usize = 32;
const RECORD_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct LayoutState {
    arrangement: Arrangement,
    geometry: Option<TreeGeometry>,
    bounds: [f32; 4],
}
impl LayoutState {
    fn capture(workspace: &FormeWorkspace) -> Self {
        Self {
            arrangement: workspace.document.arrangement.clone(),
            geometry: workspace.geometry.clone(),
            bounds: workspace.bounds,
        }
    }
    fn restore(&self, workspace: &mut FormeWorkspace) {
        workspace.document.arrangement = self.arrangement.clone();
        workspace.geometry = self.geometry.clone();
        workspace.bounds = self.bounds;
    }
}

/// Flat extension of the v1 workspace record: earlier readers still see the
/// current canonical workspace. A legacy record starts with empty saved history.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedRecord {
    #[serde(flatten)]
    workspace: FormeWorkspace,
    #[serde(default = "history_version")]
    history_version: u16,
    #[serde(default)]
    layout_undo: Vec<LayoutState>,
    #[serde(default)]
    layout_redo: Vec<LayoutState>,
}
fn history_version() -> u16 {
    1
}
impl SavedRecord {
    fn new(workspace: FormeWorkspace) -> Self {
        Self {
            workspace,
            history_version: 1,
            layout_undo: Vec::new(),
            layout_redo: Vec::new(),
        }
    }
    fn validate(&self, graph_id: Uuid) -> Result<(), String> {
        self.workspace.validate(graph_id)?;
        if self.history_version != 1
            || self.layout_undo.len() > HISTORY_LIMIT
            || self.layout_redo.len() > HISTORY_LIMIT
        {
            return Err("unsupported or oversized Forme layout history".into());
        }
        for state in self.layout_undo.iter().chain(&self.layout_redo) {
            let mut workspace = self.workspace.clone();
            state.restore(&mut workspace);
            workspace.validate(graph_id)?;
        }
        Ok(())
    }
}
struct Draft {
    workspace: FormeWorkspace,
    history: History<LayoutState, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveKind {
    Apply,
    Undo,
    Redo,
    Visibility,
}
/// Prepared writes never change committed state until the store acknowledges
/// them. The session is frozen while its proposal is in flight.
#[derive(Clone)]
pub struct PreparedSave {
    next: SavedRecord,
    kind: SaveKind,
}
impl PreparedSave {
    pub async fn save<B: Backend>(&self, backend: B) -> Result<(), String> {
        self.next.validate(self.next.workspace.document.graph_id)?;
        let bytes = serde_json::to_vec(&self.next).map_err(|e| e.to_string())?;
        if bytes.len() > RECORD_LIMIT {
            return Err("Forme history exceeds 4 MiB".into());
        }
        backend
            .put(&slot(self.next.workspace.document.graph_id), &bytes)
            .await
            .map_err(|e| e.to_string())
    }
}
pub struct FormeSession {
    saved: SavedRecord,
    draft: Option<Draft>,
    busy: bool,
}
impl FormeSession {
    pub fn new(workspace: FormeWorkspace) -> Self {
        Self {
            saved: SavedRecord::new(workspace),
            draft: None,
            busy: false,
        }
    }
    pub fn view(&self) -> &FormeWorkspace {
        self.draft
            .as_ref()
            .map_or(&self.saved.workspace, |d| &d.workspace)
    }
    pub fn committed(&self) -> &FormeWorkspace {
        &self.saved.workspace
    }
    pub fn editing(&self) -> bool {
        self.draft.is_some()
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| {
            LayoutState::capture(&d.workspace) != LayoutState::capture(&self.saved.workspace)
        })
    }
    pub fn can_undo(&self) -> bool {
        !self.busy
            && self
                .draft
                .as_ref()
                .map_or(!self.saved.layout_undo.is_empty(), |d| d.history.can_undo())
    }
    pub fn can_redo(&self) -> bool {
        !self.busy
            && self
                .draft
                .as_ref()
                .map_or(!self.saved.layout_redo.is_empty(), |d| d.history.can_redo())
    }
    fn idle(&self) -> Result<(), String> {
        if self.busy {
            Err("Wait for the Forme save to finish".into())
        } else {
            Ok(())
        }
    }
    pub fn unlock(&mut self) -> Result<(), String> {
        self.idle()?;
        if self.draft.is_none() {
            let mut workspace = self.saved.workspace.clone();
            workspace.locked = false;
            self.draft = Some(Draft {
                workspace,
                history: History::new().with_cap(200).with_window(400),
            });
        }
        Ok(())
    }
    /// Record one accepted layout gesture. The host supplies a coalescing key
    /// for repeated divider samples, and no key for a completed drag/drop.
    pub fn edit(
        &mut self,
        key: Option<String>,
        now_ms: u64,
        change: impl FnOnce(&mut FormeWorkspace) -> Result<(), String>,
    ) -> Result<(), String> {
        self.idle()?;
        let draft = self
            .draft
            .as_mut()
            .ok_or("Unlock the forme to change its arrangement")?;
        let mut next = draft.workspace.clone();
        change(&mut next)?;
        if next.document.id != draft.workspace.document.id
            || next.document.graph_id != draft.workspace.document.graph_id
        {
            return Err("An arrangement edit cannot replace its Forme identity".into());
        }
        next.validate(draft.workspace.document.graph_id)?;
        let before = LayoutState::capture(&draft.workspace);
        if LayoutState::capture(&next) != before {
            draft.history.record(before, key, now_ms);
        }
        draft.workspace = next;
        Ok(())
    }
    pub fn draft_undo(&mut self, redo: bool) -> Result<bool, String> {
        self.idle()?;
        let draft = self.draft.as_mut().ok_or("No Forme draft is open")?;
        let current = LayoutState::capture(&draft.workspace);
        let state = if redo {
            draft.history.redo(current)
        } else {
            draft.history.undo(current)
        };
        if let Some(state) = state {
            state.restore(&mut draft.workspace);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn break_gesture(&mut self) {
        if let Some(d) = &mut self.draft {
            d.history.break_run();
        }
    }
    /// Resolve a handle's world-space drop against the draft's original cells,
    /// not against its changing preview. Central drops tab; edge drops split.
    pub fn drop_preview(
        &self,
        member: Uuid,
        point: (f32, f32),
        graph: &Graph,
    ) -> Result<Option<FormeWorkspace>, String> {
        self.idle()?;
        if !self.editing() {
            return Err("Unlock the forme to drag tile handles".into());
        }
        if !point.0.is_finite() || !point.1.is_finite() {
            return Err("Invalid drop position".into());
        }
        let region = self.view().region();
        let [left, top, right, bottom] = region.bounds;
        if point.0 < left || point.0 > right || point.1 < top || point.1 > bottom {
            return Ok(None);
        }
        let mut next = self.view().clone();
        next.open(member, graph)?;
        let target = region.cells.iter().find(|cell| {
            let [x, y, r, b] = region.cell_world_bounds(cell);
            cell.member != member && point.0 >= x && point.0 <= r && point.1 >= y && point.1 <= b
        });
        if let Some(cell) = target {
            let [x, y, r, b] = region.cell_world_bounds(cell);
            let (u, v) = ((point.0 - x) / (r - x), (point.1 - y) / (b - y));
            use workbench::{DropTarget, Edge, TileEvent, TilePath};
            let edges = [
                (u, Edge::Left),
                (1. - u, Edge::Right),
                (v, Edge::Top),
                (1. - v, Edge::Bottom),
            ];
            let (distance, edge) = edges
                .into_iter()
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap();
            let to = if distance < 0.25 {
                DropTarget::Edge {
                    tile: next
                        .tile_for_member(cell.member)
                        .ok_or("unknown drop cell")?,
                    edge,
                }
            } else {
                let stack = next
                    .geometry
                    .as_ref()
                    .and_then(|g| stack_path(g, cell.member))
                    .ok_or("unknown tab group")?;
                DropTarget::Stack {
                    stack: TilePath(stack),
                    index: usize::MAX,
                }
            };
            next.event(
                TileEvent::Dragged {
                    tile: next.tile_for_member(member).ok_or("unknown handle")?,
                    to,
                },
                graph,
            )?;
        } else if self.view().members().contains(&member) {
            return Ok(None);
        }
        next.validate(next.document.graph_id)?;
        Ok(Some(next))
    }
    pub fn discard(&mut self) -> Result<(), String> {
        self.idle()?;
        self.draft = None;
        // Legacy unlocked records are treated as committed, not resurrected drafts.
        self.saved.workspace.locked = true;
        Ok(())
    }
    pub fn prepare_save(&mut self, kind: SaveKind, now_ms: u64) -> Result<PreparedSave, String> {
        self.idle()?;
        let mut next = self.saved.clone();
        match kind {
            SaveKind::Apply => {
                let draft = self.draft.as_ref().ok_or("No Forme draft is open")?;
                if self.dirty() {
                    next.layout_undo.push(LayoutState::capture(&next.workspace));
                    if next.layout_undo.len() > HISTORY_LIMIT {
                        next.layout_undo.remove(0);
                    }
                    next.layout_redo.clear();
                }
                next.workspace = draft.workspace.clone();
            },
            SaveKind::Undo | SaveKind::Redo => {
                if self.editing() {
                    return Err("Apply or discard the Forme draft before saved undo".into());
                }
                let current = LayoutState::capture(&next.workspace);
                let (from, to) = if kind == SaveKind::Undo {
                    (&mut next.layout_undo, &mut next.layout_redo)
                } else {
                    (&mut next.layout_redo, &mut next.layout_undo)
                };
                let state = from.pop().ok_or("No saved Forme change to restore")?;
                to.push(current);
                state.restore(&mut next.workspace);
            },
            SaveKind::Visibility => next.workspace.visible = !next.workspace.visible,
        }
        next.workspace.locked = true;
        if next.workspace.document.created_at_ms == 0 {
            next.workspace.document.created_at_ms = now_ms;
        }
        next.workspace.document.updated_at_ms = now_ms;
        next.validate(next.workspace.document.graph_id)?;
        self.busy = true;
        Ok(PreparedSave { next, kind })
    }
    /// A failed write leaves the complete draft and its history available.
    /// Saved undo/redo also advances only after acknowledgement.
    pub fn finish_save(
        &mut self,
        proposal: PreparedSave,
        result: Result<(), String>,
    ) -> Result<(), String> {
        self.busy = false;
        result?;
        if proposal.kind == SaveKind::Visibility {
            if let Some(draft) = &mut self.draft {
                draft.workspace.visible = proposal.next.workspace.visible;
            }
        } else {
            self.draft = None;
        }
        self.saved = proposal.next;
        Ok(())
    }
    pub async fn load<B: Backend>(
        backend: B,
        graph_id: Uuid,
        graph: &Graph,
    ) -> Result<Self, String> {
        let bytes = backend
            .get(&slot(graph_id))
            .await
            .map_err(|e| e.to_string())?;
        let mut saved = match bytes {
            Some(bytes) if bytes.len() > RECORD_LIMIT => {
                return Err("Forme record exceeds 4 MiB".into());
            },
            Some(bytes) => serde_json::from_slice::<SavedRecord>(&bytes)
                .map_err(|e| format!("Invalid Forme history: {e}"))?,
            None => SavedRecord::new(FormeWorkspace::new(graph_id)),
        };
        saved.validate(graph_id)?;
        // Reconcile every reachable layout against current graph membership;
        // history may never resurrect a removed source node.
        let reconcile = |workspace: &mut FormeWorkspace| {
            let mut layout = workspace.layout();
            for member in layout.open_members() {
                if graph.get_node_by_id(member).is_none() {
                    layout.close_tile(member);
                }
            }
            workspace.keep_layout(&layout);
        };
        reconcile(&mut saved.workspace);
        for state in saved.layout_undo.iter_mut().chain(&mut saved.layout_redo) {
            let mut workspace = saved.workspace.clone();
            state.restore(&mut workspace);
            reconcile(&mut workspace);
            *state = LayoutState::capture(&workspace);
        }
        saved.workspace.locked = true;
        saved.validate(graph_id)?;
        Ok(Self {
            saved,
            draft: None,
            busy: false,
        })
    }
}
fn stack_path(geometry: &TreeGeometry, member: Uuid) -> Option<Vec<usize>> {
    match geometry {
        TreeGeometry::Stack { members, .. } => members.contains(&member).then(Vec::new),
        TreeGeometry::Split { children, .. } => {
            children.iter().enumerate().find_map(|(index, branch)| {
                let mut path = stack_path(&branch.node, member)?;
                path.insert(0, index);
                Some(path)
            })
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
    use muniment::{MemoryBackend, StoreError, WriteOp};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use workbench::SplitAxis;

    fn fixture() -> (Graph, FormeSession, [Uuid; 3]) {
        let mut graph = Graph::new();
        let members = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
        for member in members {
            add_node(
                &mut graph,
                Some(member),
                "https://example.org/same".into(),
                Default::default(),
            );
        }
        let mut workspace = FormeWorkspace::new(Uuid::from_u128(10));
        workspace.open(members[0], &graph).unwrap();
        workspace.open(members[1], &graph).unwrap();
        (graph, FormeSession::new(workspace), members)
    }
    async fn save(session: &mut FormeSession, backend: MemoryBackend, kind: SaveKind) {
        let proposal = session.prepare_save(kind, 1000).unwrap();
        let result = proposal.save(backend).await;
        session.finish_save(proposal, result).unwrap();
    }
    fn member_ids(workspace: &FormeWorkspace) -> Vec<(Uuid, Uuid)> {
        use mere::forme::ArrangementNodeKind;
        workspace
            .document
            .arrangement
            .nodes()
            .filter_map(|node| match node.kind {
                ArrangementNodeKind::MemberIntent { member } => Some((member, node.id.as_uuid())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn preview_and_geometry_edits_retain_forme_root_and_member_identities() {
        let (graph, mut session, [a, _, c]) = fixture();
        let id = session.committed().document.id;
        let root = session.committed().document.arrangement.root();
        let ids = member_ids(session.committed());
        session.unlock().unwrap();
        let first = session.view().region().cells[0].clone();
        let [x, y, r, b] = session.view().region().cell_world_bounds(&first);
        let candidate = session
            .drop_preview(c, (x + 1., (y + b) / 2.), &graph)
            .unwrap()
            .unwrap();
        assert_eq!(
            session.view().members().len(),
            2,
            "preview does not add membership"
        );
        assert!(!session.can_undo(), "preview creates no history step");
        assert_eq!(candidate.document.id, id);
        assert_eq!(candidate.document.arrangement.root(), root);
        for identity in &ids {
            assert!(member_ids(&candidate).contains(identity));
        }
        session
            .edit(None, 1, |workspace| {
                *workspace = candidate;
                Ok(())
            })
            .unwrap();
        for _ in 0..3 {
            session
                .edit(None, 2, |workspace| {
                    let mut layout = workspace.layout();
                    layout.activate(a);
                    workspace.keep_layout(&layout);
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(session.view().document.arrangement.root(), root);
        for identity in ids {
            assert!(member_ids(session.view()).contains(&identity));
        }
        assert!(r > x);
    }

    #[test]
    fn membership_nested_geometry_and_divider_runs_have_scoped_undo() {
        let (graph, mut session, [a, b, c]) = fixture();
        let original = LayoutState::capture(session.committed());
        session.unlock().unwrap();
        session
            .edit(None, 0, |workspace| workspace.open(c, &graph))
            .unwrap();
        let added = LayoutState::capture(session.view());
        session
            .edit(None, 1, |workspace| {
                let mut layout = workspace.layout();
                layout.move_to_slot_of(c, b);
                layout.split_beside_axis(b, a, SplitAxis::Column, true);
                workspace.keep_layout(&layout);
                Ok(())
            })
            .unwrap();
        let nested = LayoutState::capture(session.view());
        for (time, fraction) in [(100, 0.4), (200, 0.3), (300, 0.2)] {
            session
                .edit(Some("divider:root".into()), time, |workspace| {
                    let mut layout = workspace.layout();
                    assert!(layout.set_split_fractions(&[], &[fraction, 1. - fraction]));
                    workspace.keep_layout(&layout);
                    Ok(())
                })
                .unwrap();
        }
        let sized = LayoutState::capture(session.view());
        session.draft_undo(false).unwrap();
        assert_eq!(LayoutState::capture(session.view()), nested);
        session.draft_undo(false).unwrap();
        assert_eq!(LayoutState::capture(session.view()), added);
        session.draft_undo(false).unwrap();
        assert_eq!(LayoutState::capture(session.view()), original);
        assert!(!session.can_undo());
        for _ in 0..3 {
            assert!(session.draft_undo(true).unwrap());
        }
        assert_eq!(LayoutState::capture(session.view()), sized);
    }

    #[test]
    fn discard_keeps_source_nodes_other_formes_and_committed_state() {
        let (graph, mut session, [_, _, c]) = fixture();
        let (_, other, _) = fixture();
        let other_before = serde_json::to_value(other.committed()).unwrap();
        let original = LayoutState::capture(session.committed());
        session.unlock().unwrap();
        session
            .edit(None, 0, |workspace| {
                workspace.open(c, &graph)?;
                workspace.bounds[0] -= 50.;
                Ok(())
            })
            .unwrap();
        session.discard().unwrap();
        assert!(session.view().locked);
        assert_eq!(LayoutState::capture(session.view()), original);
        assert!(graph.get_node_by_id(c).is_some());
        assert_eq!(
            serde_json::to_value(other.committed()).unwrap(),
            other_before
        );
        session.unlock().unwrap();
        assert!(
            !session.can_undo(),
            "a new draft cannot undo into a discarded one"
        );
        assert!(!session.can_redo());
    }

    #[derive(Clone)]
    struct FailingBackend {
        inner: MemoryBackend,
        fail: Arc<AtomicBool>,
    }
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl Backend for FailingBackend {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
            self.inner.get(key).await
        }
        async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
            if self.fail.load(Ordering::Relaxed) {
                Err(StoreError::Backend("injected write failure".into()))
            } else {
                self.inner.put(key, bytes).await
            }
        }
        async fn delete(&self, key: &str) -> Result<(), StoreError> {
            self.inner.delete(key).await
        }
        async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
            self.inner.list(prefix).await
        }
        async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
            self.inner.scan(start, end).await
        }
        async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
            self.inner.apply(ops).await
        }
    }
    #[test]
    fn failed_apply_retains_draft_and_history_then_commits_one_saved_change() {
        pollster::block_on(async {
            let (graph, mut session, [_, _, c]) = fixture();
            let backend = FailingBackend {
                inner: MemoryBackend::new(),
                fail: Arc::new(AtomicBool::new(false)),
            };
            session.committed().save(backend.clone()).await.unwrap();
            let original = LayoutState::capture(session.committed());
            session.unlock().unwrap();
            session
                .edit(None, 1, |workspace| workspace.open(c, &graph))
                .unwrap();
            session
                .edit(None, 2, |workspace| {
                    workspace.bounds[0] -= 60.;
                    Ok(())
                })
                .unwrap();
            let draft = LayoutState::capture(session.view());
            backend.fail.store(true, Ordering::Relaxed);
            let proposal = session.prepare_save(SaveKind::Apply, 10).unwrap();
            assert!(session.discard().is_err(), "saving freezes the draft");
            let result = proposal.save(backend.clone()).await;
            assert!(session.finish_save(proposal, result).is_err());
            assert_eq!(LayoutState::capture(session.view()), draft);
            assert_eq!(LayoutState::capture(session.committed()), original);
            assert!(session.can_undo());
            let stored = FormeSession::load(
                backend.clone(),
                session.committed().document.graph_id,
                &graph,
            )
            .await
            .unwrap();
            assert_eq!(LayoutState::capture(stored.view()), original);
            backend.fail.store(false, Ordering::Relaxed);
            let proposal = session.prepare_save(SaveKind::Apply, 11).unwrap();
            let result = proposal.save(backend.clone()).await;
            session.finish_save(proposal, result).unwrap();
            assert!(!session.editing());
            assert!(session.view().locked);
            assert_eq!(LayoutState::capture(session.committed()), draft);
            let mut reopened = FormeSession::load(
                backend.clone(),
                session.committed().document.graph_id,
                &graph,
            )
            .await
            .unwrap();
            let proposal = reopened.prepare_save(SaveKind::Undo, 12).unwrap();
            let result = proposal.save(backend.clone()).await;
            reopened.finish_save(proposal, result).unwrap();
            assert_eq!(LayoutState::capture(reopened.view()), original);
            assert!(
                !reopened.can_undo(),
                "all draft gestures commit as one saved step"
            );
            assert!(reopened.can_redo());
        });
    }

    #[test]
    fn saved_redo_and_visibility_survive_reopen_without_undoing_source() {
        pollster::block_on(async {
            let (mut graph, mut session, [_, _, c]) = fixture();
            let backend = MemoryBackend::new();
            let graph_id = session.committed().document.graph_id;
            session.unlock().unwrap();
            session
                .edit(None, 0, |workspace| workspace.open(c, &graph))
                .unwrap();
            save(&mut session, backend.clone(), SaveKind::Apply).await;
            let tile_id = session.view().tile_for_member(c);
            session.unlock().unwrap();
            session
                .edit(None, 1, |workspace| {
                    workspace.bounds[0] -= 50.;
                    Ok(())
                })
                .unwrap();
            save(&mut session, backend.clone(), SaveKind::Visibility).await;
            assert!(session.editing());
            assert!(session.dirty());
            session.discard().unwrap();
            assert!(
                !session.view().visible,
                "boundary preference is independent of arrangement discard"
            );
            let key = graph.get_node_key_by_id(c).unwrap();
            apply_graph_delta(
                &mut graph,
                GraphDelta::SetNodeTitle {
                    key,
                    title: "source edit after arrangement save".into(),
                },
            );
            let mut reopened = FormeSession::load(backend.clone(), graph_id, &graph)
                .await
                .unwrap();
            save(&mut reopened, backend.clone(), SaveKind::Undo).await;
            assert_eq!(reopened.view().members().len(), 2);
            assert!(!reopened.view().visible);
            let mut reopened = FormeSession::load(backend.clone(), graph_id, &graph)
                .await
                .unwrap();
            save(&mut reopened, backend.clone(), SaveKind::Redo).await;
            assert_eq!(reopened.view().members().len(), 3);
            assert_eq!(reopened.view().tile_for_member(c), tile_id);
            assert_eq!(
                graph.get_node(key).unwrap().title,
                "source edit after arrangement save"
            );
        });
    }

    #[test]
    fn removed_graph_members_cannot_be_resurrected_by_saved_history() {
        pollster::block_on(async {
            let (mut graph, mut session, [_, _, c]) = fixture();
            let backend = MemoryBackend::new();
            let graph_id = session.view().document.graph_id;
            session.unlock().unwrap();
            session.edit(None, 0, |w| w.open(c, &graph)).unwrap();
            save(&mut session, backend.clone(), SaveKind::Apply).await;
            session.unlock().unwrap();
            session
                .edit(None, 1, |w| {
                    w.bounds[0] -= 50.;
                    Ok(())
                })
                .unwrap();
            save(&mut session, backend.clone(), SaveKind::Apply).await;
            let key = graph.get_node_key_by_id(c).unwrap();
            apply_graph_delta(&mut graph, GraphDelta::RemoveNode { key });
            let mut reopened = FormeSession::load(backend.clone(), graph_id, &graph)
                .await
                .unwrap();
            save(&mut reopened, backend, SaveKind::Undo).await;
            assert!(!reopened.view().members().contains(&c));
            assert!(
                !reopened
                    .view()
                    .region()
                    .cells
                    .iter()
                    .any(|cell| cell.member == c)
            );
        });
    }

    #[test]
    fn central_drop_tabs_edge_drop_splits_and_outside_drop_cancels() {
        let (graph, mut session, [_, _, c]) = fixture();
        session.unlock().unwrap();
        let region = session.view().region();
        let cell = &region.cells[0];
        let [x, y, r, b] = region.cell_world_bounds(cell);
        let centered = session
            .drop_preview(c, ((x + r) / 2., (y + b) / 2.), &graph)
            .unwrap()
            .unwrap();
        assert_eq!(centered.region().cells.len(), 2);
        assert_eq!(centered.members().len(), 3);
        let edge = session
            .drop_preview(c, ((x + r) / 2., y + 1.), &graph)
            .unwrap()
            .unwrap();
        assert_eq!(edge.region().cells.len(), 3);
        assert!(
            session
                .drop_preview(c, (region.bounds[2] + 10., y), &graph)
                .unwrap()
                .is_none()
        );
        assert!(!session.dirty());
        assert!(!session.can_undo());
        assert_eq!(session.view().members().len(), 2);
    }

    #[test]
    fn malformed_history_is_retained_and_failed_saved_undo_keeps_its_cursor() {
        pollster::block_on(async {
            let (graph, mut session, [_, _, c]) = fixture();
            let backend = MemoryBackend::new();
            let graph_id = session.view().document.graph_id;
            session.unlock().unwrap();
            session.edit(None, 0, |w| w.open(c, &graph)).unwrap();
            save(&mut session, backend.clone(), SaveKind::Apply).await;
            let current = LayoutState::capture(session.committed());
            let proposal = session.prepare_save(SaveKind::Undo, 10).unwrap();
            assert!(
                session
                    .finish_save(proposal, Err("write failed".into()))
                    .is_err()
            );
            assert_eq!(LayoutState::capture(session.committed()), current);
            assert!(session.can_undo());
            assert!(!session.can_redo());
            let mut record: serde_json::Value =
                serde_json::from_slice(&backend.get(&slot(graph_id)).await.unwrap().unwrap())
                    .unwrap();
            record["history_version"] = 99.into();
            let bytes = serde_json::to_vec(&record).unwrap();
            backend.put(&slot(graph_id), &bytes).await.unwrap();
            assert!(
                FormeSession::load(backend.clone(), graph_id, &graph)
                    .await
                    .is_err()
            );
            assert_eq!(backend.get(&slot(graph_id)).await.unwrap().unwrap(), bytes);
        });
    }
}
