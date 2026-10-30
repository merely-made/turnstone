// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A retained host bridge for Redshank's existing listening controls.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use cambium::{
    DomHandle, GenetAppRunner, ResolvedSurfaceEvent, RetainedSurfaceSession, SurfaceEffect,
    SurfaceViewport, el, lens,
};
use genet_scripted_dom::NodeId;
use mere_surface_api::{SurfaceAvailability, SurfaceDescriptor, SurfaceUnavailableReason};
use redshank_model::{CaptureAnchor, ItemId};
use redshank_surfaces::{
    CompactCommand, CompactPlayerState, FullView, ItemRow, ListenPane, NoteRowActions, NoteSummary,
    RedshankSurfaceState, TextCapture, TransportState,
};
use uuid::Uuid;

use crate::panes::SessionId;

/// The owner and selection context under which a control was rendered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NoteScope {
    pub session: SessionId,
    pub generation: Uuid,
    pub custody_epoch: Option<u64>,
    pub item: ItemId,
    pub selected: Option<ItemId>,
    pub selected_capture_generation: Option<Uuid>,
    pub control_generation: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NoteProjection {
    pub compact: CompactPlayerState,
    pub scope: Option<NoteScope>,
    pub capture: Option<(Uuid, CaptureAnchor)>,
    pub notes: Vec<NoteSummary>,
}

impl NoteProjection {
    /// Control generations invalidate queued commands, but do not own drafts.
    pub(crate) fn same_capture_owner(&self, other: &Self) -> bool {
        self.capture == other.capture
            && match (&self.scope, &other.scope) {
                (Some(a), Some(b)) => {
                    a.session == b.session
                        && a.generation == b.generation
                        && a.custody_epoch == b.custody_epoch
                        && a.item == b.item
                        && a.selected == b.selected
                        && a.selected_capture_generation == b.selected_capture_generation
                },
                (None, None) => true,
                _ => false,
            }
    }
}

pub(crate) struct NoteCommand {
    pub scope: Option<NoteScope>,
    pub capture_generation: Option<Uuid>,
    pub command: CompactCommand,
}

#[derive(Clone)]
pub(crate) struct NoteDock {
    projection: Rc<RefCell<NoteProjection>>,
    commands: Rc<RefCell<VecDeque<NoteCommand>>>,
}

impl NoteDock {
    pub fn new(compact: CompactPlayerState) -> Self {
        Self {
            projection: Rc::new(RefCell::new(NoteProjection {
                compact,
                scope: None,
                capture: None,
                notes: Vec::new(),
            })),
            commands: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    pub fn snapshot(&self) -> NoteProjection {
        self.projection.borrow().clone()
    }
    pub fn project(&self, projection: NoteProjection) {
        *self.projection.borrow_mut() = projection;
    }
    pub fn drain(&self) -> Vec<NoteCommand> {
        self.commands.borrow_mut().drain(..).collect()
    }
}

type Logic = fn(&RedshankSurfaceState) -> FullView;
type Runner = GenetAppRunner<RedshankSurfaceState, Logic, FullView, ()>;

fn view(state: &RedshankSurfaceState) -> FullView {
    let dock: FullView = Box::new(lens(
        |compact: &mut CompactPlayerState| redshank_surfaces::compact_surface(compact),
        |state: &mut RedshankSurfaceState| &mut state.compact,
    ));
    Box::new(
        el(
            "div",
            vec![redshank_surfaces::tabs::listen::panel(state), dock],
        )
        .attr(
            "class",
            format!("rs-app rs-hosted-dock {}", state.scope_class()),
        ),
    )
}

fn project_state(
    state: &mut RedshankSurfaceState,
    old: Option<&NoteProjection>,
    next: &NoteProjection,
) {
    let changed_capture = old.is_none_or(|old| !old.same_capture_owner(next));
    if changed_capture {
        state.text_capture = next.capture.as_ref().map(|(_, anchor)| TextCapture {
            anchor: anchor.clone(),
            draft: String::new(),
            end_offset_ms: None,
        });
        state.set_text_draft("");
        state.editing_note = None;
        state.open_menu = None;
        state.listen_pane = ListenPane::Notes;
    }
    if old.is_some_and(|old| old.scope != next.scope) {
        state.open_menu = None;
        state.editing_note = None;
    }
    state.compact = next.compact.clone();
    state.notes = next.notes.clone();
    state.items = next
        .compact
        .now_playing
        .as_ref()
        .map(|now| ItemRow {
            id: now.item_id.clone(),
            title: now.title.clone(),
            feed_url: None,
            feed_title: now.feed_title.clone(),
            face: now.face.clone(),
            source: now.source,
            duration_ms: now.duration_ms,
            position_ms: now.position_ms,
            completed: false,
            published: None,
            cached_bytes: None,
            note_count: next.notes.len(),
            unavailable: None,
            pinned: false,
            representation: None,
        })
        .into_iter()
        .collect();
}

struct NoteSession {
    runner: Runner,
    dock: NoteDock,
    projection: NoteProjection,
    viewport: Option<SurfaceViewport>,
    descriptor: SurfaceDescriptor,
}

impl NoteSession {
    fn refresh(&mut self) -> Vec<SurfaceEffect> {
        let next = self.dock.snapshot();
        if next == self.projection {
            return Vec::new();
        }
        let old = self.projection.clone();
        self.runner
            .update(|state| project_state(state, Some(&old), &next));
        self.projection = next;
        vec![SurfaceEffect::Redraw]
    }

    fn take_commands(&mut self) {
        let mut outbound = Vec::new();
        self.runner.update(|state| {
            let commands: Vec<_> = state.drain_commands().collect();
            for command in commands {
                if !state.apply_presentation(&command) {
                    outbound.push(command);
                }
            }
        });
        let mut queue = self.dock.commands.borrow_mut();
        for command in outbound {
            queue.push_back(NoteCommand {
                scope: self.projection.scope.clone(),
                capture_generation: self.projection.capture.as_ref().map(|(id, _)| *id),
                command,
            });
        }
    }
}

impl RetainedSurfaceSession for NoteSession {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn availability(&self) -> SurfaceAvailability {
        match &self.runner.state().compact.transport {
            TransportState::Unavailable(message) => {
                SurfaceAvailability::Unavailable(SurfaceUnavailableReason::Other(message.clone()))
            },
            _ => SurfaceAvailability::Available,
        }
    }
    fn dom(&self) -> DomHandle {
        self.runner.dom()
    }
    fn root(&self) -> NodeId {
        self.runner.root()
    }
    fn focus(&self) -> Option<NodeId> {
        self.runner.focus()
    }
    fn set_focus(&mut self, node: Option<NodeId>) -> Vec<SurfaceEffect> {
        self.runner.set_focus(node);
        vec![SurfaceEffect::Redraw]
    }
    fn focus_traverse(&mut self, forward: bool) -> Vec<SurfaceEffect> {
        self.runner.focus_traverse(forward);
        vec![SurfaceEffect::Redraw]
    }
    fn focusables(&self) -> Vec<NodeId> {
        self.runner.focusables()
    }
    fn pointer_capture(&self) -> Option<NodeId> {
        self.runner.pointer_capture()
    }
    fn pointer_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.pointer_target(hit)
    }
    fn hover_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.hover_target(hit)
    }
    fn wheel_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.wheel_target(hit)
    }
    fn sync_viewport(&mut self, viewport: SurfaceViewport) -> Vec<SurfaceEffect> {
        let resized = self.viewport != Some(viewport);
        self.viewport = Some(viewport);
        let mut effects = self.refresh();
        if resized && effects.is_empty() {
            effects.push(SurfaceEffect::Redraw);
        }
        effects
    }
    fn dispatch(&mut self, event: ResolvedSurfaceEvent) -> Vec<SurfaceEffect> {
        match event {
            ResolvedSurfaceEvent::Click { target, event } => {
                self.runner.dispatch_click(target, event)
            },
            ResolvedSurfaceEvent::Key(event) => self.runner.dispatch_key(event),
            ResolvedSurfaceEvent::PointerDown { target, event } => {
                self.runner.dispatch_pointer_down(target, event)
            },
            ResolvedSurfaceEvent::PointerMove(event) => self.runner.dispatch_pointer_move(event),
            ResolvedSurfaceEvent::PointerUp(event) => self.runner.dispatch_pointer_up(event),
            ResolvedSurfaceEvent::Hover { target, event } => {
                self.runner.dispatch_hover(target, event)
            },
            ResolvedSurfaceEvent::Wheel { target, event } => {
                self.runner.dispatch_wheel(target, event)
            },
        };
        self.take_commands();
        let mut effects = vec![SurfaceEffect::Redraw];
        effects.extend(self.refresh());
        effects
    }
}

pub(crate) fn session(dom: DomHandle, dock: NoteDock) -> Box<dyn RetainedSurfaceSession> {
    let projection = dock.snapshot();
    let mut state = RedshankSurfaceState::default();
    state.note_row_actions = NoteRowActions::PrivacyOnly;
    state.listen_pane = ListenPane::Notes;
    project_state(&mut state, None, &projection);
    let runner = Runner::new(dom, view as Logic, state);
    Box::new(NoteSession {
        runner,
        dock,
        projection,
        viewport: None,
        descriptor: redshank_surfaces::surface_api::compact_descriptor(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_refresh_preserves_the_draft_until_capture_invalidation() {
        let item = ItemId("episode".into());
        let scope = NoteScope {
            session: SessionId::new(),
            generation: Uuid::new_v4(),
            custody_epoch: Some(7),
            item: item.clone(),
            selected: Some(item.clone()),
            selected_capture_generation: None,
            control_generation: Uuid::new_v4(),
        };
        let anchor = CaptureAnchor {
            item_id: item,
            offset_ms: 42,
            end_offset_ms: None,
            pressed_offset_ms: None,
            representation: Default::default(),
            fingerprint: None,
        };
        let initial = NoteProjection {
            compact: CompactPlayerState::default(),
            scope: Some(scope),
            capture: Some((Uuid::new_v4(), anchor.clone())),
            notes: Vec::new(),
        };
        let mut state = RedshankSurfaceState::default();
        project_state(&mut state, None, &initial);
        state.set_text_draft("draft in progress");
        let mut refreshed = initial.clone();
        refreshed.compact.volume_percent = 35;
        project_state(&mut state, Some(&initial), &refreshed);
        assert_eq!(state.text_editor.text(), "draft in progress");
        assert_eq!(state.text_capture.as_ref().unwrap().anchor, anchor);
        let mut restarted = refreshed.clone();
        restarted.capture.as_mut().unwrap().0 = Uuid::new_v4();
        project_state(&mut state, Some(&refreshed), &restarted);
        assert_eq!(state.text_editor.text(), "");
        assert_eq!(state.text_capture.as_ref().unwrap().anchor, anchor);
        let mut invalid = restarted.clone();
        invalid.scope = None;
        invalid.capture = None;
        project_state(&mut state, Some(&restarted), &invalid);
        assert!(state.text_capture.is_none());
    }
}
