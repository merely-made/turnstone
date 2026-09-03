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
use genet_host_api::{ProviderId, SourceKindId};
use layout_dom_api::{LayoutDom, NodeKind};

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
        accepted_source: genet_host_api::SurfaceSourceShape::One(SourceKindId::from(schema)),
    }
}

struct FakeProvider {
    pane: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    css: String,
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
        Ok(Box::new(RunnerSurfaceSession::new(
            self.descriptor.clone(),
            runner,
            |_state: &FakeState| SurfaceAvailability::Available,
            |state: &mut FakeState, viewport| state.width = viewport.width,
            |_action: ()| Vec::new(),
        )))
    }
}

fn source(schema: &str) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(schema),
        payload: crate::panes::SerializedSource {
            version: 1,
            payload: serde_json::Value::Null,
        },
    })
}

fn provider(pane: &str, schema: &str, surface: &str) -> FakeProvider {
    FakeProvider {
        pane: PaneKindId::new(pane),
        schema: SourceSchemaId::new(schema),
        descriptor: descriptor(surface, schema),
        css: String::new(),
        unavailable: None,
    }
}

fn unavailable_provider(pane: &str, schema: &str, surface: &str) -> FakeProvider {
    FakeProvider {
        unavailable: Some(SurfaceUnavailableReason::Locked),
        ..provider(pane, schema, surface)
    }
}

fn root_text(dom: &DomHandle, root: NodeId) -> String {
    let dom = dom.borrow();
    let text = dom
        .dom_children(root)
        .find(|node| dom.kind(*node) == NodeKind::Text)
        .expect("root text");
    dom.text(text).expect("text payload").to_owned()
}

#[test]
fn source_schema_is_only_external_schema() {
    let source = source("fake.v1");
    assert_eq!(
        source_schema(&source).map(SourceSchemaId::as_str),
        Some("fake.v1")
    );
    assert!(!source_matches(&SourceSchemaId::new("other"), &source));
}

#[test]
fn registration_asserts_the_descriptor_source_kind_matches_the_schema() {
    let mut registry = SurfaceProviderRegistry::new();
    let lying = FakeProvider {
        pane: PaneKindId::new("fake"),
        schema: SourceSchemaId::new("fake.v1"),
        descriptor: descriptor("fake.surface", "fake.other"),
        css: String::new(),
        unavailable: None,
    };
    assert!(matches!(
        registry.register_provider(lying),
        Err(crate::contributed_surface::SurfaceRegistrationError::SourceShapeMismatch { .. })
    ));
    assert!(registry.is_empty());
}
#[test]
fn registry_rejects_duplicate_and_validates_schema() {
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(provider("fake", "fake.v1", "fake.surface"))
        .expect("first provider");
    registry
        .register_provider(provider("other", "other.v1", "other.surface"))
        .expect("second provider");
    assert_eq!(registry.len(), 2);
    assert!(
        registry
            .register_provider(provider("fake", "fake.other", "fake.surface"))
            .is_err()
    );
    assert!(
        registry
            .register_provider(provider("fake", "fake.v2", "fake.second-surface"))
            .is_err(),
        "one pane kind cannot route ambiguously to two provider factories"
    );

    let wrong = registry.admit(&PaneKindId::new("fake"), &source("wrong.v1"));
    assert!(matches!(
        wrong,
        Err(SurfaceAdmissionError::InvalidSource { .. })
    ));
    let admitted = registry
        .admit(&PaneKindId::new("fake"), &source("fake.v1"))
        .expect("admitted source");
    assert_eq!(admitted.descriptor().surface_id.as_str(), "fake.surface");
}

#[test]
fn erased_sessions_dispatch_and_sync_independently_and_drop_from_map() {
    let first_dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let first_runner = GenetAppRunner::new(
        first_dom.clone(),
        fake_view,
        FakeState {
            count: 0,
            width: 0.0,
        },
    );
    let first_root = first_runner.root();
    let first = RunnerSurfaceSession::new(
        descriptor("first", "first.v1"),
        first_runner,
        |_state: &FakeState| SurfaceAvailability::Available,
        |state: &mut FakeState, viewport| state.width = viewport.width,
        |_action: ()| Vec::new(),
    );

    let second_dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let second_runner = GenetAppRunner::new(
        second_dom.clone(),
        fake_view,
        FakeState {
            count: 10,
            width: 0.0,
        },
    );
    let second_root = second_runner.root();
    let second = RunnerSurfaceSession::new(
        descriptor("second", "second.v1"),
        second_runner,
        |_state: &FakeState| SurfaceAvailability::Available,
        |state: &mut FakeState, viewport| state.width = viewport.width,
        |_action: ()| Vec::new(),
    );

    let mut sessions: Vec<Box<dyn RetainedSurfaceSession>> =
        vec![Box::new(first), Box::new(second)];
    sessions[0].dispatch(ResolvedSurfaceEvent::Click {
        target: first_root,
        event: PointerClick::at((0.0, 0.0)),
    });
    sessions[1].sync_viewport(SurfaceViewport {
        width: 240.0,
        height: 120.0,
        scale_factor: 1.0,
    });
    assert!(root_text(&first_dom, first_root).contains("count:1"));
    assert!(root_text(&second_dom, second_root).contains("width:240"));
    assert!(root_text(&second_dom, second_root).contains("count:10"));

    let mut map = HashMap::new();
    map.insert(
        "first",
        ContributedSurfacePane::new(
            PaneKindId::new("first"),
            source("first.v1"),
            sessions.remove(0),
            "",
        ),
    );
    map.insert(
        "second",
        ContributedSurfacePane::new(
            PaneKindId::new("second"),
            source("second.v1"),
            sessions.remove(0),
            "",
        ),
    );
    assert_eq!(map.len(), 2);
    map.remove("first");
    assert_eq!(map.len(), 1);
}

#[test]
fn retained_map_reuses_one_source_and_replaces_a_repinned_source() {
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(provider("fake", "fake.v1", "fake.surface"))
        .expect("provider");
    let mut sessions = ContributedSurfaceSessions::default();
    let mut spec = PaneSpec {
        id: PaneId(17),
        kind: PaneKindId::new("fake"),
        source: source("fake.v1"),
        context: crate::panes::ContextBinding::Own,
        config: crate::panes::PaneConfig::empty("test.empty"),
    };

    let first_dom = sessions
        .resolve(&spec, &registry)
        .expect("first admission")
        .session()
        .dom();
    let retained_dom = sessions
        .resolve(&spec, &registry)
        .expect("same admission")
        .session()
        .dom();
    assert!(Rc::ptr_eq(&first_dom, &retained_dom));

    let PaneSource::Fixed(SourceRef::External { payload, .. }) = &mut spec.source else {
        unreachable!()
    };
    payload.payload = serde_json::json!({ "path": "second.djot" });
    let replaced_dom = sessions
        .resolve(&spec, &registry)
        .expect("repinned admission")
        .session()
        .dom();
    assert!(!Rc::ptr_eq(&first_dom, &replaced_dom));
    assert_eq!(sessions.len(), 1);
    assert!(sessions.remove(spec.id).is_some());
    assert!(sessions.is_empty());
}

#[test]
fn typed_unavailability_becomes_a_generic_retained_surface() {
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(unavailable_provider(
            "locked",
            "locked.v1",
            "locked.surface",
        ))
        .expect("provider");
    let pane = registry
        .admit(&PaneKindId::new("locked"), &source("locked.v1"))
        .expect("generic unavailable pane");
    assert_eq!(
        pane.availability(),
        SurfaceAvailability::Unavailable(SurfaceUnavailableReason::Locked)
    );
    assert!(root_text(&pane.session().dom(), pane.session().root()).contains("Locked"));
}

#[test]
fn admitted_pane_exposes_its_retained_dom_and_complete_probe_stylesheet() {
    let mut registry = SurfaceProviderRegistry::new();
    let mut provider = provider("fake", "fake.v1", "fake.surface");
    provider.css =
        ".fake-surface { padding: 3px; } button { background-color: rgb(36, 44, 62); }".to_owned();
    registry.register_provider(provider).expect("provider");
    let mut pane = registry
        .admit(&PaneKindId::new("fake"), &source("fake.v1"))
        .expect("admitted surface");

    pane.scene(240, 120, 1.0);
    assert!(pane.stylesheet().contains(crate::ui::CAMBIUM_SHEET));
    assert!(pane.stylesheet().contains(".fake-surface"));

    let dom = pane.dom_ref();
    let surface = genet_probe::ProbeSurface {
        name: "contributed",
        dom: &dom,
        rect: [0.0, 0.0, 240.0, 120.0],
        sheet: pane.stylesheet(),
    };
    assert!(genet_probe::text_present(&[surface], "count:0 width:240"));

    let surface = genet_probe::ProbeSurface {
        name: "contributed",
        dom: &dom,
        rect: [0.0, 0.0, 240.0, 120.0],
        sheet: pane.stylesheet(),
    };
    let button = genet_probe::resolve(&[surface], &genet_probe::Selector::role("button"))
        .expect("semantic controls resolve under the exact stylesheet the pane presents")
        .point;
    drop(dom);

    assert_eq!(pane.click(button.0, button.1), SurfaceRequest::Redraw);
    assert!(root_text(&pane.session().dom(), pane.session().root()).contains("count:1"));
}

#[test]
fn retained_projection_and_actions_share_the_live_dom_session() {
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(provider("fake", "fake.v1", "fake.surface"))
        .expect("provider");
    let mut pane = registry
        .admit(&PaneKindId::new("fake"), &source("fake.v1"))
        .expect("admitted surface");
    let _ = pane.scene(320, 180, 1.0);

    let (tree, routes) = pane.accessibility_tree().expect("painted DOM projection");
    let (access_id, button) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Button)
        .expect("button semantics");
    assert!(button.supports_action(A11yAction::Click));
    assert!(button.supports_action(A11yAction::Focus));
    assert!(button.bounds().is_some());
    let dom_node = routes[access_id];

    pane.accessibility_action(A11yAction::Focus, dom_node);
    assert_eq!(pane.session().focus(), Some(dom_node));
    pane.accessibility_action(A11yAction::Click, dom_node);
    assert!(root_text(&pane.session().dom(), pane.session().root()).contains("count:1"));
}
