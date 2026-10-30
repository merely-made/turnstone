// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RetainedSurfaceSession for AnswerSession {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn availability(&self) -> SurfaceAvailability {
        SurfaceAvailability::Available
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
        self.effects()
    }
    fn focus_traverse(&mut self, forward: bool) -> Vec<SurfaceEffect> {
        self.runner.focus_traverse(forward);
        self.effects()
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
    fn set_focus_exits(&mut self, exits: bool) -> bool {
        self.runner.set_focus_exits(exits);
        true
    }
    fn sync_viewport(&mut self, viewport: SurfaceViewport) -> Vec<SurfaceEffect> {
        let next = self.runner.state().service.snapshot();
        let state = self.runner.state();
        if state.snapshot.revision == next.revision
            && state.width == viewport.width
            && state.height == viewport.height
        {
            return Vec::new();
        }
        self.runner.update(|state| {
            if state.local_status == "Sending question…"
                && state
                    .active
                    .as_ref()
                    .is_some_and(|id| next.runs.iter().any(|run| &run.run == id))
            {
                state.local_status.clear();
            }
            state.snapshot = next;
            state.width = viewport.width;
            state.height = viewport.height;
        });
        self.effects()
    }
    fn dispatch(&mut self, event: ResolvedSurfaceEvent) -> Vec<SurfaceEffect> {
        match event {
            ResolvedSurfaceEvent::Click { target, event } => {
                self.runner.dispatch_click(target, event);
            },
            ResolvedSurfaceEvent::Key(event) => {
                self.runner.dispatch_key(event);
            },
            ResolvedSurfaceEvent::PointerDown { target, event } => {
                self.runner.dispatch_pointer_down(target, event);
            },
            ResolvedSurfaceEvent::PointerMove(event) => {
                self.runner.dispatch_pointer_move(event);
            },
            ResolvedSurfaceEvent::PointerUp(event) => {
                self.runner.dispatch_pointer_up(event);
            },
            ResolvedSurfaceEvent::Hover { target, event } => {
                self.runner.dispatch_hover(target, event);
            },
            ResolvedSurfaceEvent::Wheel { target, event } => {
                self.runner.dispatch_wheel(target, event);
            },
        }
        self.effects()
    }
}
