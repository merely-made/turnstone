// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Exercise the browser page's real command declarations on the CPU.

#[path = "../src/web_command_catalog.rs"]
mod web_command_catalog;

use cambium::{EffectClass, catalogue::ids};

#[test]
fn authoring_page_retains_shared_metadata_and_classifies_its_own_io() {
    let commands = web_command_catalog::page_commands();
    let create = commands.get(ids::NODE_NEW).unwrap();
    assert_eq!(create.effect, EffectClass::DomainTruth);
    assert!(!create.destructive);
    assert_eq!(
        create.category, "canvas",
        "the host retains its surface category"
    );
    assert_eq!(
        commands.get(ids::VIEW_FIT).unwrap().effect,
        EffectClass::Curation
    );
    assert_eq!(
        commands.get("save-scene").unwrap().effect,
        EffectClass::ExternalEffect
    );
    assert_eq!(
        commands.get("invoke-action").unwrap().effect,
        EffectClass::ExternalEffect
    );
}
