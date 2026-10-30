// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The capability a resident holds over its OWN nested world: a scope, not a
/// power, because a world is a place with an unbounded interior and prefix
/// containment is exactly what is wanted there.
pub fn world_cap() -> Cap {
    Cap::Scope(servitor::ScopePath::parse(SCENARIO_SCOPE).expect("a valid scope"))
}
