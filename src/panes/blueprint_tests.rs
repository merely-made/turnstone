// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use mere::forme::FormeRef;

use super::*;

fn graph(n: u128) -> GraphId {
    GraphId::from_uuid(uuid::Uuid::from_u128(n))
}

fn pane(id: u64, kind: &str, source: PaneSource, context: ContextBinding) -> PaneSpec {
    PaneSpec {
        id: PaneId(id),
        kind: PaneKindId::new(kind),
        source,
        context,
        config: PaneConfig::empty(format!("turnstone.{kind}")),
    }
}

fn rect() -> RelativeRect {
    RelativeRect {
        x: 0.1,
        y: 0.1,
        width: 0.5,
        height: 0.5,
    }
}

fn space(id: &str, panes: Vec<PaneSpec>, tiled: LayoutNode) -> SpaceBlueprint {
    SpaceBlueprint {
        id: SpaceId::new(id),
        label: id.into(),
        panes,
        tiled: Some(tiled),
        floating: Vec::new(),
        chrome: ChromeBlueprint::default(),
        normalization: NormalizationPolicy::default(),
    }
}

#[path = "blueprint_tests/context_following_resolves_focus_and_pin_makes_it_cases.rs"]
mod context_following_resolves_focus_and_pin_makes_it_cases;
#[path = "blueprint_tests/floating_pane_relocates_through_dock_and_window_without_cases.rs"]
mod floating_pane_relocates_through_dock_and_window_without_cases;
