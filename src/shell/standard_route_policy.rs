// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Turnstone's ordinary HTML route uses the clean-room Livery lane. Mere's
/// shared policy keeps the legacy incumbent default for other hosts, so the
/// browser makes its product choice at its own composition boundary.
pub(super) fn standard_route_policy() -> inker::EngineRoutePolicy {
    let mut policy = mere::routing::route_policy();
    for rule in &mut policy.rules {
        if rule.engine_id == inker::routing::ENGINE_GENET_WEB {
            rule.engine_id = inker::routing::ENGINE_GENET_LIVERY.to_string();
        }
    }
    policy
}
