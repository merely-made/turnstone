// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn fixed_boston_dto_goldens_preserve_exact_serialization_and_digests() {
    // These are fixed Windows DTOs, not freshly recomputed astronomy.
    // Their bytes preserve the original P0 serialization contract.
    let fixtures: [(&[u8], &str); 2] = [
        (
            include_bytes!("../../scenarios/fixtures/sky_receipt_v1_boston_2024-04-08.json"),
            "caff8371d348ba141397a8185e291c533c6ab12d4fe85f9ce3be797707ce411d",
        ),
        (
            include_bytes!("../../scenarios/fixtures/sky_receipt_v1_boston_2024-04-09.json"),
            "74883b5db9fa959cccdeb69744da4a99baec5bfd55bade77426cfe6b0d9c450f",
        ),
    ];
    for (bytes, expected_digest) in fixtures {
        let receipt = SkyReceiptV1::from_json(bytes).expect("the fixed DTO validates");
        assert_eq!(receipt.to_pretty_json().unwrap().as_slice(), bytes);
        assert_eq!(receipt.blake3_hex_digest().unwrap(), expected_digest);
        assert_eq!(blake3::hash(bytes).to_hex().as_str(), expected_digest);
    }
}

#[test]
fn pretty_json_round_trips_and_hashes_the_exact_bytes() {
    let receipt = sample_receipt();
    let first = receipt.to_pretty_json().unwrap();
    let second = receipt.to_pretty_json().unwrap();
    assert_eq!(first, second);
    assert_eq!(SkyReceiptV1::from_json(&first).unwrap(), receipt);
    assert_eq!(
        receipt.blake3_hex_digest().unwrap(),
        blake3::hash(&first).to_hex().to_string()
    );
    assert!(
        String::from_utf8(first)
            .unwrap()
            .contains("\"angular_accuracy\": null")
    );
}

#[test]
fn json_reader_rejects_another_receipt_version() {
    let mut receipt = sample_receipt();
    receipt.version = 2;
    let bytes = receipt.to_pretty_json().unwrap();
    assert!(matches!(
        SkyReceiptV1::from_json(&bytes),
        Err(SkyReceiptJsonError::UnsupportedVersion(2))
    ));
}

#[test]
fn json_reader_rejects_tampered_engine_and_day_boundaries() {
    let mut receipt = sample_receipt();
    receipt.engine.source_revision = "different source".to_owned();
    assert!(invalid_message(receipt).contains("requires turquet 0.13.0"));

    let mut receipt = sample_receipt();
    receipt.day.start_utc = "2000-01-01T00:00:00+00:00".to_owned();
    assert!(invalid_message(receipt).contains("UTC start must use canonical form"));

    let mut receipt = sample_receipt();
    receipt.day.start_tt.day2 += 1.0e-9;
    assert!(invalid_message(receipt).contains("two-part TT start"));

    let mut receipt = sample_receipt();
    receipt.day.end_utc = receipt.day.start_utc.clone();
    receipt.day.end_tt = receipt.day.start_tt;
    assert!(invalid_message(receipt).contains("UTC bounds must increase"));
}

#[test]
fn json_reader_keeps_a_sane_unavailable_zone_identity() {
    let mut receipt = sample_receipt();
    let unavailable_zone = "America/Retired_Test_Zone";
    assert!(jiff::tz::TimeZone::get(unavailable_zone).is_err());
    receipt.day.time_zone = unavailable_zone.to_owned();

    let bytes = receipt.to_pretty_json().unwrap();
    assert_eq!(SkyReceiptV1::from_json(&bytes).unwrap(), receipt);
}

#[test]
fn json_reader_rejects_unknown_root_and_nested_fields() {
    let bytes = sample_receipt().to_pretty_json().unwrap();
    let mut root: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    root.as_object_mut()
        .unwrap()
        .insert("unknown_root".to_owned(), true.into());
    assert_json_error(serde_json::to_vec(&root).unwrap());

    let mut nested: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    nested["facts"][0]["detail"]
        .as_object_mut()
        .unwrap()
        .insert("unknown_detail".to_owned(), true.into());
    assert_json_error(serde_json::to_vec(&nested).unwrap());
}

#[test]
fn json_reader_rejects_tampered_inputs_and_provenance() {
    let mut receipt = sample_receipt();
    receipt.search.sample_step_nanoseconds = 3_600_000_000_001;
    assert!(invalid_message(receipt).contains("search controls"));

    let mut receipt = sample_receipt();
    receipt.observer.latitude_radians = PI;
    assert!(invalid_message(receipt).contains("observer latitude"));

    let mut receipt = sample_receipt();
    receipt.bodies.push(receipt.bodies[0].clone());
    assert!(invalid_message(receipt).contains("selected more than once"));

    let mut receipt = sample_receipt();
    receipt.ephemeris.model.name.clear();
    assert!(invalid_message(receipt).contains("model name"));

    let mut receipt = sample_receipt();
    receipt.earth_orientation.snapshot.clear();
    assert!(invalid_message(receipt).contains("snapshot must not be empty"));

    let mut receipt = sample_receipt();
    receipt.day.time_zone = "../UTC".to_owned();
    assert!(invalid_message(receipt).contains("sane IANA-style name"));
}

#[test]
fn json_reader_rejects_tampered_fact_projection() {
    let mut receipt = sample_receipt();
    receipt.facts[0].body = SkyBodyV1::Moon;
    assert!(invalid_message(receipt).contains("unselected body"));

    let mut receipt = sample_receipt();
    receipt.facts[0].span_tt.start = receipt.day.end_tt;
    receipt.facts[0].span_tt.end = receipt.day.end_tt;
    assert!(invalid_message(receipt).contains("half-open civil day"));

    let mut receipt = sample_receipt();
    receipt.facts[0].detail = SkyFactDetailV1::LunarPhase {
        angular_separation_radians: 0.0,
    };
    assert!(invalid_message(receipt).contains("detail projection"));

    let mut receipt = sample_receipt();
    receipt.facts[0].calculation_model = Some(test_model());
    assert!(invalid_message(receipt).contains("deduplicated receipt-header models"));

    let mut receipt = sample_receipt();
    receipt.facts.push(receipt.facts[0].clone());
    assert!(invalid_message(receipt).contains("more than one position fact"));

    let mut receipt = sample_receipt();
    receipt.facts.clear();
    assert!(invalid_message(receipt).contains("exactly one position fact"));
}

#[test]
fn json_reader_rejects_same_midpoint_body_reordering() {
    let mut receipt = sample_receipt();
    let mut moon_policy = receipt.bodies[0].clone();
    moon_policy.body = SkyBodyV1::Moon;
    receipt.bodies.push(moon_policy);

    let mut moon_position = receipt.facts[0].clone();
    moon_position.body = SkyBodyV1::Moon;
    receipt.facts.insert(0, moon_position);

    assert!(invalid_message(receipt).contains("midpoint/kind/body sort order"));
}

fn invalid_message(receipt: SkyReceiptV1) -> String {
    let bytes = receipt.to_pretty_json().unwrap();
    match SkyReceiptV1::from_json(&bytes) {
        Err(SkyReceiptJsonError::Invalid(message)) => message,
        other => panic!("expected an invalid V1 receipt, got {other:?}"),
    }
}

fn assert_json_error(bytes: Vec<u8>) {
    assert!(matches!(
        SkyReceiptV1::from_json(&bytes),
        Err(SkyReceiptJsonError::Json(_))
    ));
}

fn test_model() -> SkyModelV1 {
    SkyModelV1 {
        name: "test model".to_owned(),
        revision: "1".to_owned(),
    }
}

fn sample_receipt() -> SkyReceiptV1 {
    let day = resolve_named_sky_day("2000-01-01".parse().unwrap(), "UTC").unwrap();
    let start_tt = TtJulianDateV1::from_tt(day.start_tt());
    let end_tt = TtJulianDateV1::from_tt(day.end_tt());
    let model = test_model();
    SkyReceiptV1 {
        version: SKY_RECEIPT_VERSION,
        engine: SkyEngineV1::turquet(),
        day: SkyDayV1 {
            date: day.date().to_string(),
            time_zone: day.time_zone_name().to_owned(),
            start_utc: day.start_utc().to_string(),
            end_utc: day.end_utc().to_string(),
            start_tt,
            end_tt,
        },
        observer: SkyObserverV1 {
            longitude_east_radians: 0.0,
            latitude_radians: 0.0,
            height_meters_above_wgs84_ellipsoid: 0.0,
        },
        anchor_tt: start_tt,
        search: SkySearchV1 {
            sample_step_nanoseconds: 3_600_000_000_000,
            tolerance_nanoseconds: 1_000_000_000,
        },
        bodies: vec![SkyBodyPolicyV1 {
            body: SkyBodyV1::Sun,
            rise_set: SkyRiseSetPolicyV1 {
                refraction: SkyRefractionV1 {
                    model: model.clone(),
                    apparent_lift_radians: 0.0,
                },
                limb: SkyLimbV1::Center {
                    model: model.clone(),
                },
                horizon_dip: SkyHorizonDipV1::Level {
                    model: model.clone(),
                },
            },
        }],
        ephemeris: SkyEphemerisV1 {
            model: model.clone(),
            snapshot: None,
            angular_accuracy: None,
        },
        observer_transform: SkyObserverTransformV1 {
            model: model.clone(),
            angular_accuracy: SkyAccuracyV1 {
                maximum_error_radians: 0.001,
                evidence: SkyAccuracyEvidenceV1::ExternalComparison,
                authority: "test authority".to_owned(),
                scope: "test scope".to_owned(),
            },
        },
        earth_orientation: SkySourceIdentityV1 {
            authority: "test EOP".to_owned(),
            snapshot: "test EOP snapshot".to_owned(),
        },
        facts: vec![SkyFactV1 {
            body: SkyBodyV1::Sun,
            kind: SkyFactKindV1::Position,
            span_tt: SkyFactSpanV1 {
                start: start_tt,
                end: start_tt,
            },
            calculation_model: None,
            detail: SkyFactDetailV1::Position {
                right_ascension_radians: 1.0,
                declination_radians: 0.5,
                azimuth_radians: 2.0,
                altitude_radians: 0.25,
                distance_meters: 149_597_870_700.0,
            },
        }],
    }
}
