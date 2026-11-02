// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use accesskit::{Action as A11yAction, Role};
use cambium::{GenetAppRunner, GenetCtx, GenetElement, RunnerSurfaceSession, View, el, on_click};
use layout_dom_api::{LayoutDom, NodeKind};
use mere_surface_api::{ProviderId, SourceKindId};

use super::*;

struct FakeState {
    count: u32,
    width: f32,
}

fn fake_view(
    state: &FakeState,
) -> impl View<FakeState, (), GenetCtx, Element = GenetElement> + use<> {
    on_click(
        el::<_, FakeState, ()>(
            "button",
            format!("count:{} width:{:.0}", state.count, state.width),
        ),
        |state: &mut FakeState, _| state.count += 1,
    )
}

fn descriptor(surface: &str, schema: &str) -> SurfaceDescriptor {
    SurfaceDescriptor {
        provider_id: ProviderId::from("turnstone.test"),
        surface_id: SurfaceId::from(surface),
        label: surface.to_owned(),
        accepted_source: mere_surface_api::SurfaceSourceShape::One(SourceKindId::from(schema)),
    }
}

pub(crate) struct FakeProvider {
    pane: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    pub(crate) css: String,
    pub(crate) suppress_host_effects: bool,
    unavailable: Option<SurfaceUnavailableReason>,
}

impl SurfaceProvider for FakeProvider {
    fn pane_kind(&self) -> &PaneKindId {
        &self.pane
    }

    fn source_schema(&self) -> &SourceSchemaId {
        &self.schema
    }

    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    fn stylesheet(&self) -> &str {
        &self.css
    }

    fn admit(
        &self,
        _source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        if let Some(reason) = &self.unavailable {
            return Err(SurfaceAdmissionError::Unavailable {
                reason: reason.clone(),
            });
        }
        let runner = GenetAppRunner::new(
            dom,
            fake_view,
            FakeState {
                count: 0,
                width: 0.0,
            },
        );
        let session: Box<dyn RetainedSurfaceSession> = Box::new(RunnerSurfaceSession::new(
            self.descriptor.clone(),
            runner,
            |_state: &FakeState| SurfaceAvailability::Available,
            |state: &mut FakeState, viewport| state.width = viewport.width,
            |_action: ()| Vec::new(),
        ));
        Ok(if self.suppress_host_effects {
            Box::new(NoHostEffectsSession(session))
        } else {
            session
        })
    }
}

pub(crate) fn source(schema: &str) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(schema),
        payload: crate::panes::SerializedSource {
            version: 1,
            payload: serde_json::Value::Null,
        },
    })
}

pub(crate) fn provider(pane: &str, schema: &str, surface: &str) -> FakeProvider {
    FakeProvider {
        pane: PaneKindId::new(pane),
        schema: SourceSchemaId::new(schema),
        descriptor: descriptor(surface, schema),
        css: String::new(),
        suppress_host_effects: false,
        unavailable: None,
    }
}

/// Contract fixture only: retain the real runner's mutations and focus, but
/// request no host redraw. Shipping RunnerSurfaceSession always requests redraw.
struct NoHostEffectsSession(Box<dyn RetainedSurfaceSession>);

impl RetainedSurfaceSession for NoHostEffectsSession {
    fn descriptor(&self) -> &SurfaceDescriptor {
        self.0.descriptor()
    }
    fn availability(&self) -> SurfaceAvailability {
        self.0.availability()
    }
    fn dom(&self) -> DomHandle {
        self.0.dom()
    }
    fn root(&self) -> NodeId {
        self.0.root()
    }
    fn focus(&self) -> Option<NodeId> {
        self.0.focus()
    }
    fn set_focus(&mut self, node: Option<NodeId>) -> Vec<SurfaceEffect> {
        self.0.set_focus(node);
        Vec::new()
    }
    fn focus_traverse(&mut self, forward: bool) -> Vec<SurfaceEffect> {
        self.0.focus_traverse(forward);
        Vec::new()
    }
    fn focusables(&self) -> Vec<NodeId> {
        self.0.focusables()
    }
    fn pointer_capture(&self) -> Option<NodeId> {
        self.0.pointer_capture()
    }
    fn pointer_target(&self, hit: NodeId) -> Option<NodeId> {
        self.0.pointer_target(hit)
    }
    fn hover_target(&self, hit: NodeId) -> Option<NodeId> {
        self.0.hover_target(hit)
    }
    fn wheel_target(&self, hit: NodeId) -> Option<NodeId> {
        self.0.wheel_target(hit)
    }
    fn sync_viewport(&mut self, viewport: SurfaceViewport) -> Vec<SurfaceEffect> {
        self.0.sync_viewport(viewport);
        Vec::new()
    }
    fn dispatch(&mut self, event: ResolvedSurfaceEvent) -> Vec<SurfaceEffect> {
        self.0.dispatch(event);
        Vec::new()
    }
}

fn unavailable_provider(pane: &str, schema: &str, surface: &str) -> FakeProvider {
    FakeProvider {
        unavailable: Some(SurfaceUnavailableReason::Locked),
        ..provider(pane, schema, surface)
    }
}

pub(crate) fn root_text(dom: &DomHandle, root: NodeId) -> String {
    let dom = dom.borrow();
    let text = dom
        .dom_children(root)
        .find(|node| dom.kind(*node) == NodeKind::Text)
        .expect("root text");
    dom.text(text).expect("text payload").to_owned()
}

#[path = "tests/source_schema_is_only_external_schema_cases.rs"]
mod source_schema_is_only_external_schema_cases;
