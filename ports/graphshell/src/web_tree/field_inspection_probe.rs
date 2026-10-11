// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Probe instrumentation is not a browser qualification receipt.
use super::*;

pub(crate) fn fields(page: &TreePage, canvas: &Canvas, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let pane = &page.field_inspector;
    let snapshot = snapshot
        .with_field(
            "field-inspection-fixture",
            page.product
                .as_ref()
                .is_some_and(|p| p.inspection_fixture)
                .to_string(),
        )
        .with_field("field-inspector-open", pane.open.to_string())
        .with_field("field-inspector-picking", pane.picking.to_string())
        .with_field(
            "field-inspector-target",
            pane.target.map(target_id).unwrap_or_default(),
        )
        .with_field(
            "field-inspector-kind",
            match pane.target {
                Some(FieldTarget::Numen(_)) => "numen",
                Some(FieldTarget::Forme(_)) => "forme",
                None => "",
            },
        )
        .with_field(
            "field-inspector-id",
            pane.target
                .map(|target| match target {
                    FieldTarget::Numen(id) => id.as_uuid().to_string(),
                    FieldTarget::Forme(id) => id.to_string(),
                })
                .unwrap_or_default(),
        )
        .with_field(
            "field-inspector-point",
            pane.point.map(|p| json(&p)).unwrap_or_default(),
        );
    let roster = graph(page).map(model::field_roster);
    let snapshot = match roster {
        Ok(roster) => snapshot
            .with_field("field-inspector-roster-ready", "true")
            .with_field("field-inspector-roster-count", roster.len().to_string())
            .with_field(
                "field-inspector-roster-ids",
                roster
                    .iter()
                    .map(|f| f.id.as_uuid().to_string())
                    .collect::<Vec<_>>()
                    .join("|"),
            ),
        Err(_) => snapshot
            .with_field("field-inspector-roster-ready", "false")
            .with_field("field-inspector-roster-count", "0")
            .with_field("field-inspector-roster-ids", ""),
    };
    let snapshot = match pane.target {
        Some(FieldTarget::Numen(id)) => {
            match graph(page).and_then(|graph| model::inspect_field(graph, id)) {
                Ok(inspected) => snapshot
                    .with_field("field-inspector-ready", "true")
                    .with_field(
                        "field-inspector-couplings",
                        inspected.couplings.len().to_string(),
                    )
                    .with_field(
                        "field-inspector-lifecycle",
                        format!("{:?}", inspected.field.lifecycle),
                    )
                    .with_field(
                        "field-inspector-boundary-enabled",
                        canvas.field_visible(id).to_string(),
                    ),
                Err(_) => unready(snapshot),
            }
        },
        Some(FieldTarget::Forme(id)) => match forme(page, id) {
            Ok(inspected) => snapshot
                .with_field("field-inspector-ready", "true")
                .with_field("field-inspector-couplings", "0")
                .with_field("field-inspector-lifecycle", "arrangement-owner")
                .with_field(
                    "field-inspector-boundary-enabled",
                    inspected.visible.to_string(),
                ),
            Err(_) => unready(snapshot),
        },
        None => unready(snapshot),
    };
    let snapshot = match candidates(page, canvas) {
        Ok(candidates) => snapshot
            .with_field("field-region-ready", "true")
            .with_field("field-region-count", candidates.len().to_string())
            .with_field(
                "field-region-ids",
                candidates
                    .into_iter()
                    .map(|candidate| target_id(candidate.target))
                    .collect::<Vec<_>>()
                    .join("|"),
            ),
        Err(_) => snapshot
            .with_field("field-region-ready", "false")
            .with_field("field-region-count", "0")
            .with_field("field-region-ids", ""),
    };
    let snapshot = snapshot.with_field(
        "coupling-inspector-id",
        pane.coupling
            .map(|id| id.as_uuid().to_string())
            .unwrap_or_default(),
    );
    match pane.coupling.and_then(|id| {
        graph(page)
            .and_then(|graph| model::inspect_coupling(graph, id))
            .ok()
    }) {
        Some(inspected) => snapshot
            .with_field("coupling-inspector-ready", "true")
            .with_field(
                "coupling-inspector-field",
                inspected.coupling.field.as_uuid().to_string(),
            )
            .with_field(
                "coupling-inspector-strength",
                inspected.coupling.strength.to_string(),
            )
            .with_field(
                "coupling-inspector-response",
                json(&inspected.coupling.response),
            )
            .with_field(
                "coupling-inspector-eligibility",
                format!("{:?}", inspected.rebuild_eligibility),
            )
            .with_field(
                "coupling-inspector-members",
                inspected.selector_members.len().to_string(),
            ),
        None => snapshot
            .with_field("coupling-inspector-ready", "false")
            .with_field("coupling-inspector-field", "")
            .with_field("coupling-inspector-strength", "")
            .with_field("coupling-inspector-response", "")
            .with_field("coupling-inspector-eligibility", "")
            .with_field("coupling-inspector-members", "0"),
    }
}

fn unready(snapshot: ProbeSnapshot) -> ProbeSnapshot {
    snapshot
        .with_field("field-inspector-ready", "false")
        .with_field("field-inspector-couplings", "0")
        .with_field("field-inspector-lifecycle", "")
        .with_field("field-inspector-boundary-enabled", "")
}
