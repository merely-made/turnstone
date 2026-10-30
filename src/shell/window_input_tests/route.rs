// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn route() -> PeltRoute {
    PeltRoute {
        decision: EngineRouteDecision {
            engine_id: "fake.web".to_owned(),
            surface_contract: SurfaceContract {
                target: SurfaceTargetId::new("node-7"),
                mode: SurfaceContractMode::CompositedTexture,
            },
        },
        source: PeltRouteSource::Automatic,
        state: PeltRouteState::Surface,
    }
}
