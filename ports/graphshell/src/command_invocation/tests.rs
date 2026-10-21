// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::viewer_commands::Availability;
use cambium::EffectClass;

const SIZE: (u32, u32) = (800, 600);

fn fixture() -> Canvas {
    let persona = crate::mere_host::SelectedPersonaRef {
        persona: crate::mere_host::FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-tree".into(),
    };
    let app = crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas.resize(SIZE.0, SIZE.1);
    canvas
}

fn request(commands: &CommandSet, id: &str) -> Invocation {
    // Match the script/serialized scenario edge, rather than invoking a typed
    // operation directly. Effect facts come from this host's current offer.
    let invocation = commands.get(id).unwrap().invocation(SCHEMA, Vec::new());
    serde_json::from_slice(&serde_json::to_vec(&invocation).unwrap()).unwrap()
}

fn state(canvas: &Canvas) -> (mere::canvas::CameraView, bool, bool) {
    (
        canvas.camera(),
        canvas.physics_paused(),
        canvas.view_follows(),
    )
}

fn script_requests(commands: &CommandSet, source: &str) -> Result<Vec<Invocation>, String> {
    use script_rhai::command::{Budget, CommandLimits, collect_invocations};
    collect_invocations(
        source,
        commands,
        SCHEMA,
        Budget::Steps(10_000),
        CommandLimits::default(),
    )
}

#[test]
fn rhai_batch_serializes_into_the_same_actual_viewer_receiver() {
    let commands = viewer_commands::commands(Availability::default());
    let mut canvas = fixture();
    let before = state(&canvas);
    let requests = script_requests(
        &commands,
        r#"command("view:zoom_in", ""); command("physics:toggle", "");"#,
    )
    .unwrap();
    let encoded = serde_json::to_vec(&requests).unwrap();
    let received: Vec<Invocation> = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(received, requests);
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].id, ids::VIEW_ZOOM_IN);
    assert_eq!(received[1].id, ids::PHYSICS_TOGGLE);
    for invocation in received {
        assert_eq!(invocation.schema_id, SCHEMA);
        assert_eq!(
            invocation.effect,
            commands.get(&invocation.id).unwrap().effect
        );
        assert_eq!(
            invoke(&mut canvas, SIZE, &commands, &invocation),
            Outcome::Done
        );
    }
    assert!(canvas.camera().zoom > before.0.zoom);
    assert_ne!(canvas.physics_paused(), before.1);
}

#[test]
fn a_failed_script_exposes_no_partial_requests_to_the_viewer() {
    let commands = viewer_commands::commands(Availability::default());
    let mut canvas = fixture();
    let before = state(&canvas);
    let result = script_requests(
        &commands,
        r#"command("physics:toggle", ""); throw "later failure";"#,
    )
    .map(|requests| {
        requests
            .into_iter()
            .map(|request| invoke(&mut canvas, SIZE, &commands, &request))
            .collect::<Vec<_>>()
    });
    assert!(result.is_err());
    assert_eq!(state(&canvas), before);
    // A later successful script starts a fresh batch; the failed toggle cannot
    // leak into it and reverse the observed one-toggle result.
    let requests = script_requests(&commands, r#"command("physics:toggle", "");"#).unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        invoke(&mut canvas, SIZE, &commands, &requests[0]),
        Outcome::Done
    );
    assert_ne!(canvas.physics_paused(), before.1);
}

#[test]
fn serialized_viewer_commands_complete_only_after_real_canvas_changes() {
    let mut canvas = fixture();
    assert!(
        canvas.graph().nodes().count() > 0,
        "real Graphshell fixture"
    );
    let commands = viewer_commands::commands(Availability::default());
    let before = canvas.camera().zoom;
    assert_eq!(
        invoke(
            &mut canvas,
            SIZE,
            &commands,
            &request(&commands, ids::VIEW_ZOOM_IN)
        ),
        Outcome::Done
    );
    assert!(canvas.camera().zoom > before);
    let enlarged = canvas.camera().zoom;
    assert_eq!(
        invoke(
            &mut canvas,
            SIZE,
            &commands,
            &request(&commands, ids::VIEW_ZOOM_OUT)
        ),
        Outcome::Done
    );
    assert!(canvas.camera().zoom < enlarged);
    let paused = canvas.physics_paused();
    assert_eq!(
        invoke(
            &mut canvas,
            SIZE,
            &commands,
            &request(&commands, ids::PHYSICS_TOGGLE)
        ),
        Outcome::Done
    );
    assert_ne!(canvas.physics_paused(), paused);
}

#[test]
fn forged_contracts_are_refused_without_changing_the_canvas() {
    let commands = viewer_commands::commands(Availability::default());
    let mut canvas = fixture();
    let before = state(&canvas);
    for field in 0..3 {
        let mut invocation = request(&commands, ids::VIEW_ZOOM_IN);
        match field {
            0 => invocation.schema_id.push_str("-other"),
            1 => invocation.effect = EffectClass::DomainTruth,
            _ => invocation.arguments.push(1),
        }
        assert!(matches!(
            invoke(&mut canvas, SIZE, &commands, &invocation),
            Outcome::Refused { .. }
        ));
        assert_eq!(state(&canvas), before);
    }
}

#[test]
fn retired_disabled_and_unsupported_offers_do_not_mutate_the_canvas() {
    let commands = viewer_commands::commands(Availability::default());
    let mut canvas = fixture();
    let before = state(&canvas);
    let mut retired = request(&commands, ids::VIEW_ZOOM_IN);
    retired.id = "viewer:retired".into();
    assert_eq!(
        invoke(&mut canvas, SIZE, &commands, &retired),
        Outcome::Stale
    );
    let locked = viewer_commands::commands(Availability {
        locked: true,
        ..Default::default()
    });
    assert!(matches!(
        invoke(
            &mut canvas,
            SIZE,
            &locked,
            &request(&commands, ids::VIEW_ZOOM_IN)
        ),
        Outcome::Refused { .. }
    ));
    assert!(matches!(
        invoke(
            &mut canvas,
            SIZE,
            &commands,
            &request(&commands, ids::VIEW_FIT)
        ),
        Outcome::Refused { .. }
    ));
    assert_eq!(state(&canvas), before);
}

#[test]
fn zoom_limits_do_not_report_done_or_stop_layout_following() {
    let commands = viewer_commands::commands(Availability::default());
    let mut canvas = fixture();
    let (low, high) = canvas.zoom_range();
    for (zoom, id) in [(high, ids::VIEW_ZOOM_IN), (low, ids::VIEW_ZOOM_OUT)] {
        canvas.set_camera(mere::canvas::CameraView {
            zoom,
            offset: (12.0, 18.0),
        });
        canvas.set_view_follow(true);
        let before = state(&canvas);
        assert!(matches!(
            invoke(&mut canvas, SIZE, &commands, &request(&commands, id)),
            Outcome::Refused { .. }
        ));
        assert_eq!(state(&canvas), before);
    }
}
