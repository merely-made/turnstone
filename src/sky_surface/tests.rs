// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProviderRegistry, SurfaceRequest};
use layout_dom_api::LayoutDom;

fn admitted_reference() -> crate::contributed_surface::ContributedSurfacePane {
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(SkySurfaceProvider::default())
        .expect("Sky provider");
    registry
        .admit(
            &PaneKindId::new(PANE_KIND),
            &boston_eclipse_reference_source(),
        )
        .expect("reference Sky source")
}

fn text_present(dom: &genet_scripted_dom::ScriptedDom, needle: &str) -> bool {
    fn contains(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        needle: &str,
    ) -> bool {
        dom.text(node).is_some_and(|text| text.contains(needle))
            || dom
                .dom_children(node)
                .any(|child| contains(dom, child, needle))
    }
    contains(dom, dom.document(), needle)
}

#[test]
fn reference_source_reproduces_the_p0_receipt() {
    let projection = calculate_source(&SkyPaneSourceV1::boston_eclipse_reference()).unwrap();
    assert_eq!(projection.receipt.day.date, "2024-04-08");
    assert_eq!(projection.receipt.day.time_zone, "America/New_York");
    assert_eq!(projection.receipt.facts.len(), 11);
    assert_eq!(
        projection.digest,
        "caff8371d348ba141397a8185e291c533c6ab12d4fe85f9ce3be797707ce411d"
    );
    assert!(
        projection
            .rows
            .iter()
            .any(|row| row.interval_label.contains("bounded TT solver interval"))
    );
    assert!(
        projection
            .rows
            .iter()
            .any(|row| row.interval_label.contains("sampled instant"))
    );
}

#[test]
fn changing_date_or_policy_rebuilds_a_distinct_receipt() {
    let original = SkyPaneSourceV1::boston_eclipse_reference();
    let original_digest = calculate_source(&original).unwrap().digest;

    let mut next_day = original.clone();
    next_day.date = "2024-04-09".into();
    let next_digest = calculate_source(&next_day).unwrap().digest;
    assert_ne!(next_digest, original_digest);

    let mut geometric = original;
    geometric.bodies[0].rise_set = SkyRiseSetPresetV1::GeometricCenter;
    let geometric_digest = calculate_source(&geometric).unwrap().digest;
    assert_ne!(geometric_digest, original_digest);
    assert_ne!(geometric_digest, next_digest);
}

#[test]
fn polar_empty_crossings_are_described_without_visibility_inference() {
    let mut source = SkyPaneSourceV1::boston_eclipse_reference();
    source.date = "2024-06-21".into();
    source.time_zone = "Europe/Oslo".into();
    source.observer = SkyPaneObserverV1 {
        label: "Tromso reference observer".into(),
        longitude_east_degrees: 18.9553,
        latitude_degrees: 69.6492,
        height_meters_above_wgs84_ellipsoid: 0.0,
    };
    source.bodies = vec![SkyPaneBodySourceV1 {
        body: SkyPaneBodyV1::Sun,
        rise_set: SkyRiseSetPresetV1::GeometricCenter,
    }];

    let projection = calculate_source(&source).unwrap();
    assert_eq!(projection.empty_rise_set.len(), 1);
    assert_eq!(
        projection.empty_rise_set[0],
        "Sun: No rise or set under the selected policy in this civil day."
    );
    assert!(
        !projection.empty_rise_set[0]
            .to_ascii_lowercase()
            .contains("circumpolar")
    );
    assert!(
        !projection.empty_rise_set[0]
            .to_ascii_lowercase()
            .contains("always visible")
    );
}

#[test]
fn provider_admits_semantic_controls_and_provenance() {
    let pane = admitted_reference();
    assert_eq!(pane.descriptor().surface_id.as_str(), SURFACE_ID);
    let handle = pane.session().dom();
    let dom = handle.borrow();
    assert!(text_present(&dom, "Sky"));
    assert!(text_present(&dom, "Boston reference observer"));
    assert!(text_present(&dom, "America/New_York"));
    assert!(text_present(
        &dom,
        "2024-04-08 DUT1=-0.01669 s; xp=yp=0 approximation"
    ));
    assert!(text_present(&dom, "Receipt caff8371"));
    assert!(text_present(&dom, "bounded TT solver interval"));
    assert!(dom.all_with_class(dom.document(), "setting-apply").len() >= 7);
    assert_eq!(dom.all_with_class(dom.document(), "sky-alert").len(), 0);
    assert!(dom.first_tag(dom.document(), "input").is_some());
    assert!(dom.first_tag(dom.document(), "button").is_some());
    assert!(dom.first_tag(dom.document(), "label").is_some());
}

#[test]
fn next_day_replaces_one_retained_projection_without_rewriting_opening_provenance() {
    let opening = boston_eclipse_reference_source();
    let mut changed = admitted_reference();
    let unchanged = admitted_reference();
    changed.scene(1_200, 1_600, 1.0);

    let next_day_point = {
        let dom = changed.dom_ref();
        let surface = genet_probe::ProbeSurface {
            name: "sky",
            dom: &dom,
            rect: [0.0, 0.0, 1_200.0, 1_600.0],
            sheet: changed.stylesheet(),
        };
        genet_probe::resolve(
            &[surface],
            &genet_probe::Selector::role("button").containing("Next day"),
        )
        .expect("semantic Next day control")
        .point
    };
    let hit = changed
        .hit_test(next_day_point.0, next_day_point.1)
        .expect("Probe point must hit the retained Sky layout");
    assert_ne!(hit, changed.session().root());
    assert_eq!(
        changed.click(next_day_point.0, next_day_point.1),
        SurfaceRequest::Redraw
    );

    let mut next_source = SkyPaneSourceV1::boston_eclipse_reference();
    next_source.date = "2024-04-09".into();
    let next_digest = calculate_source(&next_source).unwrap().digest;
    assert_eq!(
        next_digest,
        "74883b5db9fa959cccdeb69744da4a99baec5bfd55bade77426cfe6b0d9c450f"
    );
    let changed_dom = changed.dom_ref();
    assert!(text_present(
        &changed_dom,
        "Applied: 2024-04-09 in America/New_York"
    ));
    assert!(text_present(
        &changed_dom,
        &format!("Receipt {next_digest}")
    ));
    assert!(text_present(
        &changed_dom,
        "Fixed Earth-orientation input remains referenced to 2024-04-08."
    ));
    drop(changed_dom);

    let unchanged_dom = unchanged.dom_ref();
    assert!(text_present(
        &unchanged_dom,
        "Applied: 2024-04-08 in America/New_York"
    ));
    drop(unchanged_dom);
    assert!(changed.matches(&PaneKindId::new(PANE_KIND), &opening));
    assert!(unchanged.matches(&PaneKindId::new(PANE_KIND), &opening));
}

#[test]
fn source_version_and_unknown_fields_fail_closed() {
    let provider = SkySurfaceProvider::default();
    let mut wrong_version = boston_eclipse_reference_source();
    let PaneSource::Fixed(SourceRef::External { payload, .. }) = &mut wrong_version else {
        unreachable!()
    };
    payload.version += 1;
    let mut registry = SurfaceProviderRegistry::new();
    registry.register_provider(provider).expect("Sky provider");
    assert!(matches!(
        registry.admit(&PaneKindId::new(PANE_KIND), &wrong_version),
        Err(SurfaceAdmissionError::InvalidPayload { .. })
    ));

    let mut unknown = boston_eclipse_reference_source();
    let PaneSource::Fixed(SourceRef::External { payload, .. }) = &mut unknown else {
        unreachable!()
    };
    payload.payload["invented"] = serde_json::json!(true);
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(SkySurfaceProvider::default())
        .unwrap();
    assert!(matches!(
        registry.admit(&PaneKindId::new(PANE_KIND), &unknown),
        Err(SurfaceAdmissionError::InvalidPayload { .. })
    ));
}

#[test]
fn duplicate_body_and_bad_zone_are_rejected_before_calculation() {
    let mut duplicate = SkyPaneSourceV1::boston_eclipse_reference();
    duplicate.bodies.push(duplicate.bodies[0].clone());
    assert!(
        calculate_source(&duplicate)
            .unwrap_err()
            .contains("selected twice")
    );

    let mut bad_zone = SkyPaneSourceV1::boston_eclipse_reference();
    bad_zone.time_zone = "Not/A_Zone".into();
    assert!(
        calculate_source(&bad_zone)
            .unwrap_err()
            .contains("unknown IANA time zone")
    );
}
