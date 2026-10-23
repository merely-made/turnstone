// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;
use cambium_rootstock::TextureProducer;

#[test]
fn access_sessions_survive_reprojection_and_hidden_tabs_refuse_input() {
    let mut host = ContentHost::default();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let source = HeldContent::text("mere://saved/a", "Held A", "A long paragraph. ".repeat(600));
    let first = host.open(a, source.clone()).unwrap();
    let other = host.open(b, source.clone()).unwrap();
    assert_ne!(
        first.borrow().session_identity(),
        other.borrow().session_identity()
    );
    host.show_only(&[a]);
    first.borrow_mut().frame(300., 120., 1.).unwrap();
    assert!(first.borrow_mut().scroll(0., 80.));
    let identity = first.borrow().session_identity();
    let again = host.open(a, source).unwrap();
    assert!(Rc::ptr_eq(&first, &again));
    assert_eq!(identity, again.borrow().session_identity());
    assert!(other.borrow_mut().frame(300., 120., 1.).is_err());
    assert!(
        !other
            .borrow_mut()
            .input(SessionInput::Text("hidden".into()))
    );
    host.show_only(&[b]);
    assert!(first.borrow_mut().frame(300., 120., 2.).is_err());
    other.borrow_mut().frame(300., 120., 2.).unwrap();
    host.close(a);
    assert_eq!(host.len(), 1);
    assert!(!first.borrow_mut().scroll(0., 10.));
}

#[test]
fn failed_source_replacement_keeps_the_actual_held_session() {
    let mut host = ContentHost::default();
    let access = Uuid::new_v4();
    let original = HeldContent::text("mere://saved/a", "A", "original");
    let slot = host.open(access, original.clone()).unwrap();
    let mut invalid = original.clone();
    invalid.media_type = "application/unknown".into();
    assert!(host.open(access, invalid).is_err());
    assert!(Rc::ptr_eq(&slot, &host.get(access).unwrap()));
    host.show_only(&[access]);
    let replacement = host
        .open(
            access,
            HeldContent::text("mere://saved/a", "A", "replacement"),
        )
        .unwrap();
    assert_ne!(
        slot.borrow().session_identity(),
        replacement.borrow().session_identity()
    );
    assert!(!slot.borrow().is_visible());
    assert_eq!(replacement.borrow().source().bytes, b"replacement");
}

#[test]
fn actual_genet_link_becomes_a_host_request_without_loading_the_destination() {
    let mut host = ContentHost::default();
    let access = Uuid::new_v4();
    let source = HeldContent { address: "https://held.example/source".into(), title: "Held HTML".into(), media_type: "text/html".into(), bytes: b"<!doctype html><html><body><h1>Held document</h1><a href='/destination'>Retained link</a></body></html>".to_vec(), engine: None, media: None };
    let slot = host.open(access, source).unwrap();
    host.show_only(&[access]);
    slot.borrow_mut().frame(640., 400., 1.).unwrap();
    let link = slot
        .borrow()
        .content
        .document()
        .unwrap()
        .links()
        .into_iter()
        .find(|l| l.url.ends_with("/destination"))
        .unwrap();
    for state in [SessionButtonState::Pressed, SessionButtonState::Released] {
        slot.borrow_mut().input(SessionInput::PointerButton {
            x: link.rect[0] + link.rect[2] / 2.,
            y: link.rect[1] + link.rect[3] / 2.,
            button: SessionPointerButton::Primary,
            state,
            modifiers: SessionModifiers::default(),
        });
    }
    let effects = slot.borrow_mut().take_effects();
    assert!(effects.iter().any(|e| {
        e.navigation
            .as_ref()
            .is_some_and(|n| n.request.address == "https://held.example/destination")
    }));
    assert_eq!(
        slot.borrow().content.address(),
        Some("https://held.example/source")
    );
    assert!(
        slot.borrow_mut()
            .content
            .document_mut()
            .unwrap()
            .take_load_commands()
            .is_empty()
    );
}

#[test]
fn readonly_tests_producer_actions_keep_owner_identity_and_reject_retired_sources() {
    let mut host = ContentHost::default();
    let access = Uuid::new_v4();
    let source = HeldContent {
        address: "https://held.example/source".into(),
        title: "Held HTML".into(),
        media_type: "text/html".into(),
        bytes: b"<html><body><a href='/target'>Target</a></body></html>".to_vec(),
        engine: None,
        media: None,
    };
    let slot = host.open(access, source).unwrap();
    host.show_only(&[access]);
    slot.borrow_mut().frame(400., 200., 1.).unwrap();
    let mut producer = ContentProducer::new(slot.clone(), 91);
    let owner = slot.borrow();
    let keys = owner.semantic_keys.clone();
    let first = producer.semantics().unwrap();
    assert_eq!(owner.semantic_keys, keys);
    drop(owner);
    slot.borrow_mut().frame(400., 200., 1.1).unwrap();
    assert_eq!(
        slot.borrow().semantic_keys,
        keys,
        "ordinary frames preserve live keys"
    );
    let target = first
        .children
        .iter()
        .find_map(|n| {
            n.actions
                .iter()
                .find(|a| a.id.starts_with("activate/"))
                .map(|a| (n.key, a.id.clone()))
        })
        .unwrap();
    let life = producer.child_lifetime(target.0).unwrap();
    let second = producer.semantics().unwrap();
    assert_eq!(producer.child_lifetime(target.0), Some(life.clone()));
    assert!(second.children.iter().any(|n| n.key == target.0));
    assert!(!producer.act(target.0, "activate/999999"));
    let recreated = ContentProducer::new(slot.clone(), 92);
    let recreated_description = recreated.semantics().unwrap();
    assert_eq!(recreated.child_lifetime(target.0), Some(life.clone()));
    assert!(
        recreated_description
            .children
            .iter()
            .any(|node| node.key == target.0)
    );
    assert_eq!(slot.borrow().semantic_keys, keys);
    // Activation resolves against the owner without a priming semantic read.
    let mut unobserved = ContentProducer::new(slot.clone(), 93);
    assert!(unobserved.act(target.0, &target.1));
    let effects = host.get(access).unwrap().borrow_mut().take_effects();
    assert!(effects.iter().any(|effect| {
        effect
            .navigation
            .as_ref()
            .is_some_and(|navigation| navigation.request.address == "https://held.example/target")
    }));
    host.close(access);
    assert_eq!(producer.child_lifetime(target.0), None);
    assert!(!producer.act(target.0, &target.1));
    assert!(producer.semantics().is_none());
}

#[test]
fn native_resource_inventory_has_no_ambient_file_or_network_fetch() {
    use pelt_core::native::ResourceFetcher;
    let mut held = pelt_core::native::HeldResources::default();
    held.0.insert(
        "https://held.example/image".into(),
        b"actual held bytes".to_vec(),
    );
    assert_eq!(
        held.fetch("https://held.example/image"),
        Some(b"actual held bytes".to_vec())
    );
    assert_eq!(held.fetch("file:///private/unheld"), None);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"private unregistered bytes").unwrap();
    let address = url::Url::from_file_path(file.path()).unwrap();
    assert_eq!(held.fetch(address.as_str()), None);
    assert_eq!(held.fetch("https://held.example/unheld"), None);
    assert_eq!(held.fetch("data:text/plain,held"), Some(b"held".to_vec()));
}

#[test]
fn selected_clip_admission_survives_owner_reopen_and_uses_verified_custody() {
    use muniment::Backend;
    pollster::block_on(async {
        let limits = mere_document_lanes::retained_clip::ClipLimits {
            max_parts: 16,
            max_bytes: 1024 * 1024,
            max_record_bytes: 128 * 1024,
        };
        let context = fleece::portable_clip::CaptureContext {
            source: eidetic::clip_profile::ClipSourceV1 {
                owner: Some("source-app".into()),
                entity: "selected".into(),
                revision: None,
            },
            capture_id: "actual-selected-package".into(),
            provenance: eidetic::ProvenanceRecord::self_imported("explicit selected content"),
            trust: eidetic::TrustEnvelope::self_asserted(),
            captured_at: eidetic::Timestamp(7),
        };
        let mut source = eidetic::MemoryBackend::new();
        let selected = serde_json::from_str(include_str!(
            "../../../../crates/fleece/tests/fixtures/portable_clip/knot.json"
        ))
        .unwrap();
        let id = fleece::portable_clip::retain_knot_export(
            &mut source,
            None,
            &selected,
            Some("private prefix; café; private suffix".as_bytes().to_vec()),
            &context,
            limits,
        )
        .await
        .unwrap();
        let package = fleece::portable_clip::export_selected_clip(
            &mut source,
            None,
            id,
            &["body".into()],
            limits,
        )
        .await
        .unwrap();
        let wire = serde_json::to_vec(&package).unwrap();
        let backend = muniment::MemoryBackend::default();
        let persona = crate::mere_host::SelectedPersonaRef {
            persona: crate::mere_host::FIXTURE_PERSONA_ADDRESS.into(),
            profile: "fixture".into(),
        };
        let mut app = crate::app::GraphshellApp::fixture(backend.clone(), persona.clone()).unwrap();
        let keys = backend.list("").await.unwrap();
        assert!(
            import_clip(&mut app, b"malformed package", limits, &context)
                .await
                .is_err()
        );
        assert_eq!(backend.list("").await.unwrap(), keys);
        let member = import_clip(&mut app, &wire, limits, &context)
            .await
            .unwrap();
        crate::local_intake::persist_intake(&mut app, member, 9)
            .await
            .unwrap();
        drop(app);
        let reopened = crate::app::GraphshellApp::open_or_fixture(backend.clone(), persona)
            .await
            .unwrap();
        let (_, node) = reopened.host.graph().get_node_by_id(member).unwrap();
        let clip = match mere_document_lanes::retained_clip::request(&node.url())
            .unwrap()
            .unwrap()
        {
            mere_document_lanes::retained_clip::ClipRequest::Read(id) => id,
            _ => panic!("read reference"),
        };
        let mut host = ContentHost::default();
        let mut store = backend;
        let slot = host
            .open_clip(member, &mut store, None, clip, limits)
            .await
            .unwrap();
        host.show_only(&[member]);
        slot.borrow_mut().frame(600., 400., 1.).unwrap();
        let view = mere_document_lanes::retained_clip::read_store(&mut store, None, clip, limits)
            .await
            .unwrap();
        assert!(
            matches!(view.body, mere_document_lanes::retained_clip::ClipBody::Text(ref text) if text == "café")
        );
        assert!(
            !slot
                .borrow()
                .source()
                .bytes
                .windows(b"private prefix".len())
                .any(|bytes| bytes == b"private prefix")
        );
    });
}

#[test]
fn portable_content_view_reserves_the_requested_paint_box() {
    use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement};
    use cambium_rootstock::{
        Host, HostHooks, HostOptions, HostState, HostTree as _, HostWake, ScriptedDom, WindowDom,
    };
    use std::sync::Arc;
    struct State(Rc<RefCell<ContentSlot>>, Rc<Cell<u32>>);
    type Child = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
    type Logic = fn(&State) -> Child;
    fn view(state: &State) -> Child {
        let clicks = state.1.clone();
        Box::new(cambium::on_click(
            cambium::el(
                "div",
                content_view::<State, ()>(73, state.0.clone(), 400, 217),
            ),
            move |_: &mut State, _| clicks.set(clicks.get() + 1),
        ))
    }
    let mut content = ContentHost::default();
    let member = Uuid::new_v4();
    let slot = content
        .open(
            member,
            HeldContent {
                address: "https://held.example/layout".into(),
                title: "Layout".into(),
                media_type: "text/html".into(),
                bytes: b"<a href='/target'>Target</a>".to_vec(),
                engine: None,
                media: None,
            },
        )
        .unwrap();
    content.show_only(&[member]);
    let producer_key = ContentProducer::allocate_key();
    let mut producers = cambium_rootstock::ProducerRegistry::new();
    producers
        .register(
            producer_key,
            ContentProducer::new(slot.clone(), producer_key),
            &[],
        )
        .unwrap();
    assert!(producers.contains(producer_key));
    let mut state = HostState::<State, Logic, Child>::new();
    let parent_clicks = Rc::new(Cell::new(0));
    state.runner = Some(GenetAppRunner::new(
        Rc::new(RefCell::new(ScriptedDom::new())),
        view as Logic,
        State(slot.clone(), parent_clicks.clone()),
    ));
    let wake = HostWake::new(state.wake_pending.clone(), Arc::new(|| {}));
    let mut host = Host::new(
        HostOptions::default(),
        None,
        HostHooks::inert(),
        state,
        wake,
    );
    host.set_surface_size(400., 300.);
    host.relayout(400., 300.);
    let bounds = {
        let runner = host.s.runner.as_ref().unwrap();
        let dom = runner.dom();
        let dom = dom.borrow();
        let dom = WindowDom::new(&dom, runner.mount());
        let projection =
            cambium_rootstock::document_projection(&dom, host.s.layout.as_ref().unwrap(), None);
        let node = projection
            .nodes()
            .iter()
            .find(|node| node.name.as_deref() == Some("Retained document Layout"))
            .unwrap();
        node.bounds.unwrap()
    };
    assert_eq!((bounds.width, bounds.height), (400., 217.));
    slot.borrow_mut().frame(400., 217., 1.).unwrap();
    let point = {
        let slot = slot.borrow();
        let document = slot.content.document().unwrap();
        let projection = document.accessibility_projection().unwrap();
        let target = projection
            .nodes()
            .iter()
            .find(|node| node.actions.contains(&inker::DocumentA11yAction::Click))
            .unwrap();
        document
            .accessibility_click_target(target.id)
            .unwrap()
            .point
    };
    host.pointer_moved(bounds.x + point.x, bounds.y + point.y);
    host.click();
    host.release();
    assert_eq!(parent_clicks.get(), 0, "content owns the tile-body click");
    assert!(slot.borrow_mut().take_effects().iter().any(|effect| {
        effect
            .navigation
            .as_ref()
            .is_some_and(|navigation| navigation.request.address == "https://held.example/target")
    }));
}

#[test]
fn filling_content_view_reserves_headers_and_scrolls_to_export_in_short_panels() {
    use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement};
    use cambium_rootstock::{
        Host, HostHooks, HostOptions, HostState, HostTree as _, HostWake, ScriptedDom, WindowDom,
    };
    use std::sync::Arc;
    struct State(Rc<RefCell<ContentSlot>>, u32);
    type Child = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
    type Logic = fn(&State) -> Child;
    fn view(state: &State) -> Child {
        Box::new(cambium::el("article", (
            cambium::el("div", "Header").attr("style", "height:33px;flex-shrink:0;"),
            cambium::el("div", (
                filling_content_view::<State, ()>(74, state.0.clone()),
                cambium::el("div", "Status").attr("style", "height:26px;flex-shrink:0;"),
            )).attr("style", "display:flex;flex-direction:column;flex-grow:1;flex-shrink:1;flex-basis:0px;min-height:0;"),
        )).attr("style", format!("width:400px;height:{}px;padding:16px;box-sizing:border-box;display:flex;flex-direction:column;min-height:0;overflow:hidden;", state.1)))
    }
    for panel_height in [217, 150] {
        let mut content = ContentHost::default();
        let member = Uuid::new_v4();
        let slot = content
            .open(
                member,
                HeldContent {
                    address: "https://held.example/short".into(),
                    title: "Short panel".into(),
                    media_type: "text/html".into(),
                    bytes: format!(
                        "{}<a href='/export'>Export selected clip</a>",
                        "<p>Retained passage</p>".repeat(12)
                    )
                    .into_bytes(),
                    engine: None,
                    media: None,
                },
            )
            .unwrap();
        content.show_only(&[member]);
        let mut state = HostState::<State, Logic, Child>::new();
        state.runner = Some(GenetAppRunner::new(
            Rc::new(RefCell::new(ScriptedDom::new())),
            view as Logic,
            State(slot.clone(), panel_height),
        ));
        let wake = HostWake::new(state.wake_pending.clone(), Arc::new(|| {}));
        let mut host = Host::new(
            HostOptions::default(),
            None,
            HostHooks::inert(),
            state,
            wake,
        );
        host.set_surface_size(400., panel_height as f32);
        host.relayout(400., panel_height as f32);
        let bounds = {
            let runner = host.s.runner.as_ref().unwrap();
            let dom = runner.dom();
            let dom = dom.borrow();
            let dom = WindowDom::new(&dom, runner.mount());
            let projection =
                cambium_rootstock::document_projection(&dom, host.s.layout.as_ref().unwrap(), None);
            projection
                .nodes()
                .iter()
                .find(|node| node.name.as_deref() == Some("Retained document Short panel"))
                .unwrap()
                .bounds
                .unwrap()
        };
        assert_eq!(bounds.width, 368.);
        assert_eq!(bounds.height, panel_height as f32 - 32. - 33. - 26.);
        assert!(bounds.y + bounds.height + 26. <= panel_height as f32 - 16.);
        slot.borrow_mut()
            .frame(bounds.width, bounds.height, 1.)
            .unwrap();
        assert!(slot.borrow_mut().scroll_key(SessionScrollKey::End));
        slot.borrow_mut()
            .frame(bounds.width, bounds.height, 2.)
            .unwrap();
        host.pointer_moved(bounds.x + 5., bounds.y + 5.);
        host.wheel(0., 639.);
        assert!(
            host.s.runner.as_ref().unwrap().default_prevented(),
            "bounded reader keeps wheel custody at its scroll limit"
        );
        content.show_only(&[]);
        host.wheel(0., 639.);
        assert!(
            !host.s.runner.as_ref().unwrap().default_prevented(),
            "a hidden or retired reader cannot consume wheel input"
        );
        content.show_only(&[member]);
        let point = {
            let slot = slot.borrow();
            let document = slot.content.document().unwrap();
            let projection = document.accessibility_projection().unwrap();
            let export = projection
                .nodes()
                .iter()
                .find(|node| node.actions.contains(&inker::DocumentA11yAction::Click))
                .unwrap();
            document
                .accessibility_click_target(export.id)
                .unwrap()
                .point
        };
        assert!(
            point.y >= 0. && point.y < bounds.height,
            "export must be inside actual short viewport"
        );
        host.pointer_moved(bounds.x + point.x, bounds.y + point.y);
        host.click();
        host.release();
        assert!(slot.borrow_mut().take_effects().iter().any(|effect| {
            effect
                .navigation
                .as_ref()
                .is_some_and(|nav| nav.request.address == "https://held.example/export")
        }));
    }
}

#[test]
fn native_factory_passes_host_font_bytes_into_document_resource_custody() {
    let font = include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec();
    let engines = pelt_core::native::sessions_with_fetcher_and_fonts(
        pelt_core::native::HeldResources::default(),
        vec![font.clone()],
    );
    let mut session = engines
        .spawn(
            inker::routing::ENGINE_GENET_LIVERY,
            &SessionSpawnRequest::new("https://held.example/font")
                .with_body(r#"<p style="font-family:IBM Plex Sans">Held text</p>"#)
                .with_content_type("text/html"),
        )
        .unwrap();
    let scene = session.frame(400, 217);
    assert!(
        scene
            .ops
            .iter()
            .any(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
    );
    assert!(
        scene
            .fonts
            .iter()
            .any(|face| face.data.as_ref() == font.as_slice())
    );
}

#[test]
fn verified_voice_controls_emit_host_play_and_stop_requests() {
    pollster::block_on(async {
        let limits = mere_document_lanes::retained_clip::ClipLimits {
            max_parts: 16,
            max_bytes: 1024 * 1024,
            max_record_bytes: 128 * 1024,
        };
        let mut store = muniment::MemoryBackend::default();
        let persona = crate::mere_host::SelectedPersonaRef {
            persona: crate::mere_host::FIXTURE_PERSONA_ADDRESS.into(),
            profile: "fixture".into(),
        };
        let mut app = crate::app::GraphshellApp::fixture(store.clone(), persona).unwrap();
        fixture::seed(&mut app, limits).await.unwrap();
        let address =
            "mere://clip/9cb36d1a9fc01a661d1d1941fad00fb10faf077829b229ea896c9e59e0262e29";
        let mere_document_lanes::retained_clip::ClipRequest::Read(clip) =
            mere_document_lanes::retained_clip::request(address)
                .unwrap()
                .unwrap()
        else {
            panic!("read clip")
        };
        let mut host = ContentHost::default();
        let member = Uuid::new_v4();
        let slot = host
            .open_clip(member, &mut store, None, clip, limits)
            .await
            .unwrap();
        host.show_only(&[member]);
        slot.borrow_mut().frame(207., 300., 1.).unwrap();
        let mut producer = ContentProducer::new(slot.clone(), 92);
        for (name, command) in [("Play voice note", "play"), ("Stop voice note", "stop")] {
            let semantics = producer.semantics().unwrap();
            let node = semantics
                .children
                .iter()
                .find(|node| node.name == name)
                .unwrap();
            let action = node
                .actions
                .iter()
                .find(|action| action.id.starts_with("activate/"))
                .unwrap();
            assert!(producer.act(node.key, &action.id));
            let effects = slot.borrow_mut().take_effects();
            assert!(
                effects.iter().any(
                    |effect| effect
                        .navigation
                        .as_ref()
                        .is_some_and(|navigation| navigation.request.address
                            == format!("{address}/{command}"))
                )
            );
        }
    });
}

#[test]
fn content_view_cancel_with_auxiliary_button_retires_actual_genet_press_before_release() {
    use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement};
    use cambium_rootstock::{
        Host, HostHooks, HostOptions, HostState, HostTree as _, HostWake, ScriptedDom, WindowDom,
    };
    use std::sync::Arc;
    struct State(Rc<RefCell<ContentSlot>>, Rc<Cell<u32>>);
    type Child = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
    type Logic = fn(&State) -> Child;
    fn view(state: &State) -> Child {
        let clicks = state.1.clone();
        Box::new(cambium::on_click(
            cambium::el(
                "div",
                content_view::<State, ()>(73, state.0.clone(), 400, 217),
            ),
            move |_: &mut State, _| clicks.set(clicks.get() + 1),
        ))
    }
    let mut content = ContentHost::default();
    let member = Uuid::new_v4();
    let slot = content
        .open(
            member,
            HeldContent {
                address: "https://held.example/layout".into(),
                title: "Layout".into(),
                media_type: "text/html".into(),
                bytes: b"<a href='/target'>Target</a>".to_vec(),
                engine: None,
                media: None,
            },
        )
        .unwrap();
    content.show_only(&[member]);
    let mut state = HostState::<State, Logic, Child>::new();
    let parent_clicks = Rc::new(Cell::new(0));
    state.runner = Some(GenetAppRunner::new(
        Rc::new(RefCell::new(ScriptedDom::new())),
        view as Logic,
        State(slot.clone(), parent_clicks.clone()),
    ));
    let wake = HostWake::new(state.wake_pending.clone(), Arc::new(|| {}));
    let mut host = Host::new(
        HostOptions::default(),
        None,
        HostHooks::inert(),
        state,
        wake,
    );
    host.set_surface_size(400., 300.);
    host.relayout(400., 300.);
    let bounds = {
        let runner = host.s.runner.as_ref().unwrap();
        let dom = runner.dom();
        let dom = dom.borrow();
        let dom = WindowDom::new(&dom, runner.mount());
        let projection =
            cambium_rootstock::document_projection(&dom, host.s.layout.as_ref().unwrap(), None);
        let node = projection
            .nodes()
            .iter()
            .find(|node| node.name.as_deref() == Some("Retained document Layout"))
            .unwrap();
        node.bounds.unwrap()
    };
    assert_eq!((bounds.width, bounds.height), (400., 217.));
    slot.borrow_mut().frame(400., 217., 1.).unwrap();
    let point = {
        let slot = slot.borrow();
        let document = slot.content.document().unwrap();
        let projection = document.accessibility_projection().unwrap();
        let target = projection
            .nodes()
            .iter()
            .find(|node| node.actions.contains(&inker::DocumentA11yAction::Click))
            .unwrap();
        document
            .accessibility_click_target(target.id)
            .unwrap()
            .point
    };
    host.pointer_moved(bounds.x + point.x, bounds.y + point.y);
    host.click();
    assert!(host.s.runner.as_ref().unwrap().pointer_capture().is_some());
    assert!(
        slot.borrow_mut()
            .take_effects()
            .iter()
            .all(|effect| effect.navigation.is_none())
    );
    {
        let runner = host.s.runner.as_mut().unwrap();
        runner.dispatch_pointer_cancel(
            cambium::PointerEvent::new(
                cambium::PointerPhase::Cancel,
                (point.x, point.y),
                (400., 217.),
            )
            .with_button(cambium::PointerButton::Auxiliary),
        );
        assert!(runner.pointer_capture().is_none());
    }
    host.release();
    // A late document-level release must also find the actual Genet press
    // retired. Runner capture retirement alone cannot establish this fence.
    slot.borrow_mut().input(SessionInput::PointerButton {
        x: point.x,
        y: point.y,
        button: SessionPointerButton::Primary,
        state: SessionButtonState::Released,
        modifiers: SessionModifiers::default(),
    });
    assert!(
        slot.borrow_mut()
            .take_effects()
            .iter()
            .all(|effect| effect.navigation.is_none()),
        "cancelled content must not emit navigation on a later release"
    );
    assert_eq!(
        parent_clicks.get(),
        0,
        "content cancellation does not activate its parent"
    );
    host.pointer_moved(bounds.x + point.x, bounds.y + point.y);
    host.click();
    host.release();
    assert_eq!(parent_clicks.get(), 0, "content owns the tile-body click");
    assert!(slot.borrow_mut().take_effects().iter().any(|effect| {
        effect
            .navigation
            .as_ref()
            .is_some_and(|navigation| navigation.request.address == "https://held.example/target")
    }));
}

#[test]
fn held_thai_text_asks_for_its_dictionary_and_lays_out_again_with_it() {
    let mut host = ContentHost::default();
    let access = Uuid::new_v4();
    let thai = "ภาษาไทยเป็นภาษาที่มีระดับเสียงของคำแน่นอน ".repeat(40);
    let slot = host
        .open(access, HeldContent::text("mere://saved/thai", "Thai", thai))
        .unwrap();
    host.show_only(&[access]);
    assert!(
        host.wanted_segmenter_dictionaries().is_empty(),
        "nothing laid out yet"
    );
    slot.borrow_mut().frame(300., 200., 1.).unwrap();
    let wanted = host.wanted_segmenter_dictionaries();
    assert!(
        wanted.southeast_asian && !wanted.chinese_japanese,
        "{wanted:?}"
    );

    let blob = mere_document_lanes::DictionaryBlob::new(
        include_bytes!("../../web/dictionaries/segmenter-southeast-asian.dict").to_vec(),
    )
    .unwrap();
    host.set_segmenter_dictionaries(vec![blob]);
    slot.borrow_mut().frame(300., 200., 1.).unwrap();
    assert!(
        host.wanted_segmenter_dictionaries().is_empty(),
        "the dictionary is held and the text laid out with it"
    );
    let later = Uuid::new_v4();
    let other = host
        .open(
            later,
            HeldContent::text("mere://saved/thai-2", "Thai", "ภาษาไทย ".repeat(40)),
        )
        .unwrap();
    host.show_only(&[later]);
    other.borrow_mut().frame(300., 200., 1.).unwrap();
    assert!(
        host.wanted_segmenter_dictionaries().is_empty(),
        "later sessions get it too"
    );
}

#[test]
fn host_font_roles_keep_code_and_missing_glyph_fallback_across_reflow() {
    let serif = include_bytes!("../../web/Newsreader-Regular.ttf").to_vec();
    let sans = include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec();
    let mono = include_bytes!("../../web/IBMPlexMono-Regular.ttf").to_vec();
    let symbols = include_bytes!("../../web/NotoSansMath-Regular.ttf").to_vec();
    let engines = pelt_core::native::sessions_with_generic_fonts(
        pelt_core::native::HeldResources::default(),
        vec![serif.clone(), sans.clone(), mono.clone(), symbols.clone()],
        vec!["Newsreader".into(), "IBM Plex Sans".into(), "Noto Sans Math".into()],
        vec![("sans-serif".into(), sans.clone()), ("monospace".into(), mono.clone())],
    );
    let mut document = inker::EngineDocument {
        address: "mere://reading/font-roles".into(),
        title: None,
        content_type: "text/plain".into(),
        lang: None,
        provenance: Default::default(),
        trust: Default::default(),
        diagnostics: Vec::new(),
        navigation: Default::default(),
        blocks: vec![
            inker::Block::Paragraph { spans: vec![inker::InlineSpan::Text("Reading → a related source".into())] },
            inker::Block::CodeBlock { language: None, text: "document -> projection".into() },
            inker::Block::Paragraph { spans: vec![inker::InlineSpan::Link {
                url: "mere://reading/next".into(), title: None,
                spans: vec![inker::InlineSpan::Text("Same-scheme link".into())], predicate: None,
            }] },
        ],
    };
    document.provenance.source_kind = Some(pelt_core::native::DOCUMENT_ENGINE_ID.into());
    let mut session = engines.spawn(pelt_core::native::DOCUMENT_ENGINE_ID,
        &SessionSpawnRequest::new(&document.address)
            .with_body(serde_json::to_string(&document).unwrap())
            .with_content_type("application/vnd.mere.document+json")
    ).unwrap();
    for width in [800, 280] {
        let scene = session.frame(width, 400);
        for op in &scene.ops {
            if let netrender::SceneOp::GlyphRun(run) = op {
                assert!(run.glyphs.iter().all(|glyph| glyph.id != 0),
                    "reading, code and both link arrows must have real glyphs");
            }
        }
        for (role, bytes) in [("prose", &serif), ("fallback", &sans), ("code", &mono), ("symbols", &symbols)] {
            assert!(scene.fonts.iter().any(|face| face.data.as_ref() == bytes.as_slice()),
                "{role} must use its actual supplied face at viewport {width}");
        }
    }
}

#[test]
fn reading_controls_keep_held_accesses_and_search_independent() {
    let mut host=ContentHost::default();
    let a=Uuid::from_u128(11); let b=Uuid::from_u128(12);
    let source=HeldContent::text("mere://saved/shared","Shared paper","One needle in each paragraph. ".repeat(80));
    let first=host.open(a,source.clone()).unwrap(); let second=host.open(b,source.clone()).unwrap();
    host.show_only(&[a,b]);
    first.borrow_mut().frame(640.,240.,1.).unwrap(); second.borrow_mut().frame(640.,240.,1.).unwrap();
    let identity=first.borrow().session_identity();
    let mut value=tabard::ReadingPreferences::default();value.step_text(true);value.step_column(false);
    assert!(first.borrow_mut().reading_preferences(value).unwrap());
    first.borrow_mut().frame(640.,240.,2.).unwrap();
    assert_eq!(first.borrow_mut().find("needle",None).unwrap().count,80);
    assert_eq!(first.borrow_mut().find("needle",Some(ReadFindDirection::Previous)).unwrap().current,Some(79));
    assert_eq!(second.borrow_mut().find("absent",None).unwrap().count,0);
    assert_eq!(first.borrow().session_identity(),identity);
    assert_eq!(first.borrow().source().bytes,source.bytes);
    assert!(Rc::ptr_eq(&first,&host.open(a,source).unwrap()));
    first.borrow_mut().clear_find();
    assert_eq!(first.borrow_mut().find("",Some(ReadFindDirection::Next)).unwrap().count,0);
}

#[test]
fn cue_controls_reach_the_same_retained_genet_receiver_and_close_cancels_work() {
    use inker::webcue::{Command, Error, Event, Input, Reply, RequestId, Surface};
    let mut host = ContentHost::default();
    let access = Uuid::new_v4();
    let slot = host
        .open(
            access,
            HeldContent {
                address: "https://held.example/cue".into(),
                title: "Cue".into(),
                media_type: "text/html".into(),
                bytes: b"<input value='held'><p>Shared receiver</p>".to_vec(),
                engine: None,
                media: None,
            },
        )
        .unwrap();
    host.show_only(&[access]);
    slot.borrow_mut().frame(320., 160., 0.).unwrap();
    let identity = slot.borrow().session_identity().unwrap();
    let mut producer = ContentProducer::new(slot.clone(), ContentProducer::allocate_key());
    assert_eq!(producer.scope().owner, u128::from(identity.instance_id));
    Surface::input(&mut producer, Input::Document(SessionInput::Focus(true))).unwrap();
    assert!(matches!(
        producer.poll_event().unwrap().event,
        Event::DocumentInput(_)
    ));
    let request = producer.request(RequestId(1), Command::Zoom(1.2)).unwrap();
    producer.advance(std::time::Duration::ZERO).unwrap();
    assert!(
        matches!(producer.poll_event().unwrap().event,Event::Completed{request:key,result:Ok(Reply::Zoom(_))} if key==request)
    );
    let pending = producer.request(RequestId(2), Command::Reload).unwrap();
    host.close(access);
    assert!(
        matches!(producer.poll_event().unwrap().event,Event::Completed{request:key,result:Err(Error::Retired)} if key==pending)
    );
    assert!(matches!(
        producer.request(RequestId(3), Command::Reload),
        Err(Error::Retired)
    ));
}

#[test]
#[ignore = "requires a real wgpu adapter; run explicitly"]
fn cue_graphshell_genet_produces_pixels_on_the_borrowed_host_device() {
    use inker::webcue::{Surface,Viewport,Error};
    use wgpu_cue::{GpuHost,TextureSource,Rendered};
    let core=genet_render_host::RenderCore::boot(netrender::NetrenderOptions{enable_vello:true,tile_cache_size:Some(256),..Default::default()}).unwrap();
    let gpu=GpuHost::new(core.device().clone(),core.queue().clone()).unwrap();
    let mut host=ContentHost::default();
    let access=Uuid::new_v4();
    let slot=host.open(access,HeldContent{address:"https://held.example/pixels".into(),title:"Pixels".into(),media_type:"text/html".into(),bytes:b"<html><body style='margin:0;background:#00ff00'><p>Genet</p></body></html>".to_vec(),engine:None,media:None}).unwrap();
    host.show_only(&[access]);
    let mut producer=ContentProducer::with_core(slot,&core).unwrap();
    producer.advance(std::time::Duration::from_millis(5)).unwrap();
    let cx=gpu.context(Viewport{width:160,height:100,scale:1.}).unwrap();
    assert!(matches!(TextureSource::render(&mut producer,&cx,true).unwrap(),Rendered::Pending));
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);
    let frame=loop {
        if let Rendered::Changed(frame)=TextureSource::render(&mut producer,&cx,true).unwrap(){break frame;}
        assert!(std::time::Instant::now()<deadline,"Graphshell shared frame timed out");
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(frame.stamp.scope,producer.scope());
    let pixels=core.read_rgba8_texture(&frame.texture,160,100).unwrap();
    assert_eq!(&pixels.rgba[(80*160+8)*4..(80*160+8)*4+4],&[0,255,0,255]);
    host.close(access);
    assert!(matches!(TextureSource::render(&mut producer,&cx,true),Err(Error::Retired)));
}
