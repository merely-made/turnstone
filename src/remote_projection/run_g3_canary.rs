// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn run_g3_canary() -> Result<G3Run, String> {
    let mut endpoint = TurnstoneEndpoint::fixture()?;
    let session = endpoint.session().clone();
    let request = ProjectionRequest {
        version: ProtocolVersion::V1,
        session: session.clone(),
        score: Score::new(Arrangement::Spiral(Spiral::default())),
    };
    let snapshot = endpoint
        .snapshot(request)
        .map_err(|error| error.to_string())?;
    let layout = graphshell_host::view::ProjectionLayoutView::from_scene(&snapshot.scene);
    let graph_revision_before = endpoint.app().graph_runtimes.graph().revision();
    let graph_nodes_before = endpoint.app().graph_runtimes.graph().nodes().count();
    let mut client = ClientState::default();
    client
        .apply_snapshot(snapshot)
        .map_err(|error| format!("client rejected Turnstone snapshot: {error:?}"))?;
    let profile = CapabilityProfile::new([
        PresentationCapability::PortableCard,
        PresentationCapability::NativeGlyph,
    ]);
    let presentations = resolve_all(&mut endpoint, &mut client, &session, &profile)?;
    let ack = client
        .acknowledgement(&session)
        .ok_or_else(|| "client did not acknowledge the Turnstone snapshot".to_string())?;
    let target = client
        .mounted(&session)
        .and_then(|mounted| {
            mounted
                .scene
                .active_items_in_order()
                .first()
                .map(|(id, _)| *id)
        })
        .ok_or_else(|| "Turnstone snapshot disclosed no target".to_string())?;
    let fit_result = endpoint.invoke(IntentInvocation {
        session: session.clone(),
        target,
        observed_epoch: ack.epoch,
        observed_revision: ack.revision,
        intent: FIT_INTENT.into(),
        payload: Vec::new(),
    })?;
    let open_result = endpoint.invoke(IntentInvocation {
        session: session.clone(),
        target,
        observed_epoch: ack.epoch,
        observed_revision: ack.revision,
        intent: OPEN_INTENT.into(),
        payload: b"mere://graphshell/rejected".to_vec(),
    })?;
    let graph_revision_after = endpoint.app().graph_runtimes.graph().revision();
    let graph_nodes_after = endpoint.app().graph_runtimes.graph().nodes().count();
    let audit_revision = endpoint.audit().revision();
    let audit_author = endpoint
        .audit()
        .log()
        .entries()
        .last()
        .map(|entry| entry.author.as_str().to_string())
        .unwrap_or_default();

    Ok(G3Run {
        session,
        presentations,
        layout,
        fit_result,
        open_result,
        graph_revision_before,
        graph_revision_after,
        graph_nodes_before,
        graph_nodes_after,
        audit_revision,
        audit_author,
    })
}
