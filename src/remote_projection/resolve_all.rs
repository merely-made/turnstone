// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn resolve_all(
    endpoint: &mut TurnstoneEndpoint,
    client: &mut ClientState,
    session: &ProjectionSession,
    profile: &CapabilityProfile,
) -> Result<Vec<ResolvedPresentation>, String> {
    let instances: Vec<_> = client
        .mounted(session)
        .ok_or_else(|| "client did not mount the Turnstone projection".to_string())?
        .scene
        .active_items_in_order()
        .into_iter()
        .map(|(instance, _)| instance)
        .collect();
    let mut resolved = Vec::new();
    for instance in instances {
        let presentation = match client
            .resolve(session, instance, profile)
            .map_err(resolution_error)?
        {
            PresentationResolution::Ready(presentation) => presentation,
            PresentationResolution::NeedsResource(request) => {
                let response = endpoint.resource(request)?;
                client
                    .apply_resource(response)
                    .map_err(|error| format!("client refused Turnstone resource: {error:?}"))?;
                match client
                    .resolve(session, instance, profile)
                    .map_err(resolution_error)?
                {
                    PresentationResolution::Ready(presentation) => presentation,
                    PresentationResolution::NeedsResource(_) => {
                        return Err("Turnstone resource remained unresolved after transfer".into());
                    },
                }
            },
        };
        resolved.push(presentation);
    }
    Ok(resolved)
}
