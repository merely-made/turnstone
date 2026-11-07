// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The notation scenarios drive the guest flow: they parse, and every block-pane control and
//! probe they name exists in `block_pane`. (They run headed in a browser, with the reviewed
//! adapter installed first; this is the check that needs neither.)

const PANE: &str = include_str!("../../src/block_pane.rs");
const SCENARIOS: [(&str, &str); 4] = [
    ("notation_readings", include_str!("../../web/scenarios/notation_readings.scn")),
    ("notation_readings_reopen", include_str!("../../web/scenarios/notation_readings_reopen.scn")),
    ("notation_tempo", include_str!("../../web/scenarios/notation_tempo.scn")),
    ("editable_bar_blocks", include_str!("../../web/scenarios/editable_bar_blocks.scn")),
];

#[test]
fn the_notation_scenarios_parse() {
    for (name, text) in SCENARIOS {
        taproot::Scenario::parse(text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
}

#[test]
fn every_control_class_and_probe_a_scenario_names_exists_in_the_pane() {
    // Classes the pane builds from a format string, by their fixed prefix.
    let dynamic = ["blocks-view-", "blocks-command-", "blocks-guest-event-", "blocks-select-", "blocks-change-"];
    for (name, text) in SCENARIOS {
        for line in text.lines() {
            for word in line.split_whitespace() {
                if let Some(class) = word.strip_prefix('.').filter(|w| w.starts_with("blocks-")) {
                    let known = PANE.contains(&format!("\"{class}\""))
                        || PANE.contains(&format!("\"{class} "))
                        || dynamic.iter().any(|p| class.starts_with(p) && PANE.contains(&format!("\"{p}")))
                        || dynamic.iter().any(|p| class.starts_with(p) && PANE.contains(&format!("{p}{{")));
                    assert!(known, "{name}: `.{class}` is not a control of the block pane");
                }
                if line.starts_with("assert snap blocks-") && word.starts_with("blocks-") {
                    assert!(PANE.contains(&format!("\"{word}\"")), "{name}: probe {word} is not published");
                }
            }
        }
    }
}

#[test]
fn the_scenarios_open_with_the_reviewed_adapter_installed() {
    for (name, text) in SCENARIOS {
        if text.contains("blocks-insert-example") || text.contains("blocks-command-") {
            assert!(
                text.contains("assert snap blocks-adapters == metre,progression"),
                "{name} must assert the guest is installed"
            );
        }
    }
}
