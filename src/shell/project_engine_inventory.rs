// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Project the host's registrations plus the explicitly declared construction
/// paths. Optional compilation, runtime prerequisites and registration are
/// different observations; none asserts a live page's capabilities.
pub(crate) fn project_engine_inventory(
    documents: &SessionRegistry<Scene>,
    surfaces: &inker::SurfaceEngineRegistry,
) -> Vec<crate::content::EngineDescriptor> {
    use crate::content::{EngineAvailability, EngineDescriptor, EngineFamily, engine_label};
    let mut engines: Vec<_> = documents
        .engine_ids()
        .map(|id| (id, EngineFamily::Document))
        .chain(surfaces.engine_ids().map(|id| (id, EngineFamily::Surface)))
        .map(|(id, family)| EngineDescriptor {
            id: id.into(),
            label: engine_label(id),
            family,
            availability: EngineAvailability::Registered,
        })
        .collect();
    engines.sort_by(|left, right| left.id.cmp(&right.id));
    engines.dedup_by(|left, right| left.id == right.id);
    for (id, availability) in [
        (
            inker::routing::ENGINE_SCRYING_WEB,
            scry_construction_availability(),
        ),
        (
            inker::routing::ENGINE_GRAFT_SERVO,
            servo_construction_availability(),
        ),
        (
            inker::routing::ENGINE_WELD_CHROMIUM,
            weld_construction_availability(),
        ),
    ] {
        if !engines.iter().any(|engine| engine.id == id) {
            engines.push(EngineDescriptor {
                id: id.into(),
                label: engine_label(id),
                family: EngineFamily::Surface,
                availability,
            });
        }
    }
    engines
}
