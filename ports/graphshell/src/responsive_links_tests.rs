// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn fixture() -> ResponsiveLinksFixture {
    ResponsiveLinksFixture::new().expect("deterministic source fixture")
}

fn deviations(snapshot: &ResponsiveLinksSnapshot<'_>) -> Vec<f32> {
    snapshot
        .paths
        .iter()
        .map(|appearance| appearance.max_chord_deviation)
        .collect()
}

#[test]
fn fixture_discloses_six_fixed_endpoints_three_real_source_appearances_and_stable_handles() {
    let fixture = fixture();
    let snapshot = fixture.evaluate().unwrap();

    assert_eq!(snapshot.endpoints.len(), 6);
    assert_eq!(snapshot.paths.len(), 3);
    assert_eq!(
        snapshot
            .paths
            .iter()
            .map(|appearance| appearance.label)
            .collect::<Vec<_>>(),
        [
            "Compatible path A",
            "Compatible path B",
            "Incompatible control path"
        ]
    );
    assert_eq!(
        snapshot
            .paths
            .iter()
            .map(|appearance| appearance.compatibility)
            .collect::<Vec<_>>(),
        [
            Some("Declared compatible group"),
            Some("Declared compatible group"),
            Some("Declared incompatible control")
        ]
    );
    assert_eq!(snapshot.settings.time, 0.0);
    assert_eq!(snapshot.settings.control_points, 17);
    assert_eq!(snapshot.field_strength, 10_000.0);
    assert!(snapshot.field_description.contains("Gaussian gradient"));
    assert_eq!(snapshot.field_samples.len(), 4);
    assert!(
        snapshot
            .field_samples
            .iter()
            .any(|sample| sample.vector.0.hypot(sample.vector.1) > 0.0)
    );

    for (index, appearance) in snapshot.paths.iter().enumerate() {
        assert_eq!(appearance.assertion_ids.len(), 1);
        assert_eq!(
            appearance.assertion_ids[0],
            [
                "fixture:v1:assertion:path-a",
                "fixture:v1:assertion:path-b",
                "fixture:v1:assertion:path-c"
            ][index]
        );
        assert_eq!(appearance.key.stratum, LinkStratum::Surface);
        assert_eq!(
            appearance.source,
            &LinkSource::Available {
                binding: SourceBinding {
                    authority: FIXTURE_AUTHORITY.into(),
                    domain: FIXTURE_DOMAIN.into(),
                    resource: FIXTURE_RESOURCE.into(),
                },
                revision: LinkSourceRevision::Available {
                    revision: PublicSourceRevision::new(FIXTURE_REVISION),
                },
            }
        );
        assert_eq!(appearance.buckets.len(), 1);
        assert_eq!(
            appearance.buckets[0].assertions[0].asserted_at_ms,
            Some(1_791_000_000_000 + index as u64)
        );
        assert!(appearance.endpoints_preserved);
    }
    assert_eq!(fixture.source_readings().len(), 3);
}

#[test]
fn both_responses_off_is_a_straight_fixed_endpoint_baseline() {
    let mut fixture = fixture();
    fixture.set_field_response_enabled(false);
    fixture.set_compatible_attraction_enabled(false);
    let snapshot = fixture.evaluate().unwrap();

    assert!(!snapshot.field_response_enabled);
    assert!(!snapshot.compatible_attraction_enabled);
    assert_eq!(deviations(&snapshot), [0.0, 0.0, 0.0]);
    for (appearance, endpoints) in snapshot
        .paths
        .iter()
        .zip(snapshot.endpoints.chunks_exact(2))
    {
        assert_eq!(appearance.path.points.first(), Some(&endpoints[0].position));
        assert_eq!(appearance.path.points.last(), Some(&endpoints[1].position));
    }
}

#[test]
fn field_response_kinds_and_toggle_are_independent_of_compatible_attraction() {
    let mut fixture = fixture();
    fixture.set_compatible_attraction_enabled(false);
    let mut field_deviations = Vec::new();
    for kind in [FieldKind::Attractor, FieldKind::Flow, FieldKind::Vortex] {
        fixture.set_field(kind);
        fixture.set_field_response_enabled(true);
        let snapshot = fixture.evaluate().unwrap();
        assert_eq!(snapshot.field, kind);
        assert!(!snapshot.compatible_attraction_enabled);
        assert!(
            deviations(&snapshot)
                .iter()
                .any(|deviation| *deviation > 0.05)
        );
        field_deviations.push(deviations(&snapshot));
    }
    assert_ne!(field_deviations[0], field_deviations[1]);
    assert_ne!(field_deviations[1], field_deviations[2]);

    fixture.set_field_response_enabled(false);
    fixture.set_field(FieldKind::Flow);
    let disabled = fixture.evaluate().unwrap();
    assert_eq!(deviations(&disabled), [0.0, 0.0, 0.0]);
}

#[test]
fn only_declared_compatible_paths_attract_when_field_response_is_off() {
    let mut fixture = fixture();
    fixture.set_field_response_enabled(false);
    fixture.set_compatible_attraction_enabled(true);
    let snapshot = fixture.evaluate().unwrap();
    let deviation = deviations(&snapshot);

    assert!(deviation[0] > 0.05);
    assert!(deviation[1] > 0.05);
    assert_eq!(deviation[2], 0.0, "incompatible control must stay straight");
    assert!(
        snapshot
            .paths
            .iter()
            .all(|appearance| appearance.endpoints_preserved)
    );
}

#[test]
fn configured_controls_geometry_and_spring_share_are_reported_separately() {
    let mut fixture = fixture();
    fixture.set_field(FieldKind::Vortex);
    fixture.set_field_response_enabled(true);
    fixture.set_compatible_attraction_enabled(true);
    let snapshot = fixture.evaluate().unwrap();

    assert!(snapshot.settings.field_response);
    assert!(snapshot.settings.compatible_attraction);
    assert_eq!(snapshot.field_strength, 10_000.0);
    assert!(
        snapshot
            .field_description
            .contains("Perpendicular Gaussian")
    );
    assert_eq!(snapshot.spring_share, SpringShareAvailability::Unavailable);
    assert!(
        snapshot
            .spring_share_note
            .contains("does not observe or apportion")
    );
    assert!(
        snapshot
            .paths
            .iter()
            .any(|path| path.max_chord_deviation > 0.0)
    );
}
