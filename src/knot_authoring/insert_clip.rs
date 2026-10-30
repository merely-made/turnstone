// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn insert_clip(
    retained: &mut RetainedEndpointSession,
    mounted: &mut Option<ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    address: &str,
    expected_base: Option<Vec<u8>>,
    clip: PendingClip,
    status: &Arc<Mutex<KnotClipStatus>>,
    wake: &Wake,
    visiting: bool,
) {
    let result = ensure_binding(retained, mounted, bindings, address).and_then(|binding| {
        if expected_base
            .as_ref()
            .is_some_and(|base| base != &binding.editable.base_token)
        {
            return Err(
                "The Knot destination revision changed; choose or retry the current note.".into(),
            );
        }
        if !clip.artifacts.is_empty()
            && binding.clip_action.payload_schema != KNOT_CLIP_INSERT_SCHEMA_V2
        {
            return Err("This Knot destination cannot retain citation evidence.".into());
        }
        let session = mounted
            .as_ref()
            .expect("ensure_binding mounted the session");
        if binding.clip_action.payload_schema == KNOT_CLIP_INSERT_SCHEMA_V2 {
            if clip.artifacts.is_empty() {
                return Err(
                    "this clip lane supplied no source artifact for Knot evidence retention".into(),
                );
            }
            retained.invoke(
                session,
                binding.target,
                &binding.clip_action,
                &InsertKnotClipV2 {
                    base_token: binding.editable.base_token.clone(),
                    source_url: clip.source_url,
                    title: clip.title,
                    selectors: clip.selectors,
                    knot_body: clip.knot_body,
                    artifacts: clip.artifacts,
                    fidelity: clip.fidelity,
                    discovered_edges: clip.discovered_edges,
                },
            )
        } else {
            retained.invoke(
                session,
                binding.target,
                &binding.clip_action,
                &InsertKnotClipV1 {
                    base_token: binding.editable.base_token.clone(),
                    source_url: clip.source_url,
                    title: clip.title,
                    selector: clip.selector,
                    knot_body: clip.knot_body,
                },
            )
        }
    });
    let next = match result {
        Ok(IntentResult::Accepted) => retained
            .wait_for_change()
            .and_then(|_| {
                resolve_binding(
                    retained,
                    mounted.as_ref().expect("session remains mounted"),
                    address,
                )
            })
            .map(|current| {
                bindings.insert(address.to_string(), current);
                KnotClipStatus::Saved
            })
            .unwrap_or_else(KnotClipStatus::Rejected),
        Ok(IntentResult::Stale { .. }) => KnotClipStatus::Stale,
        Ok(IntentResult::Rejected { reason }) => KnotClipStatus::Rejected(reason),
        Err(error) => KnotClipStatus::Rejected(error),
    };
    if let Ok(mut current) = status.lock() {
        *current = next;
    }
    if matches!(status.lock().as_deref(), Ok(KnotClipStatus::Saved))
        && let Some(session) = mounted.as_ref()
    {
        refresh_subscribers(retained, session, bindings, subscribers, wake, visiting);
    }
    wake();
}
