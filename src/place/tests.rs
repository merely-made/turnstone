// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn binding() -> PlaceBindingV1 {
    PlaceBindingV1::new(
        PlaceId([1; 32]),
        SharedContainerId([2; 32]),
        ChatSpaceId([3; 32]),
        "hall",
    )
    .unwrap()
}

/// A realistic p2panda endpoint ticket: ~200 base32 characters.
fn long_ticket() -> String {
    "abcdefghijklmnopqrstuvwxyz234567"
        .chars()
        .cycle()
        .take(200)
        .collect()
}

#[test]
fn elide_middle_keeps_the_ends_and_leaves_short_values_alone() {
    assert_eq!(elide_middle(""), "");
    assert_eq!(elide_middle("short"), "short");
    // Exactly the length eliding would produce: nothing is gained, so the
    // value is left whole.
    let same = "a".repeat(ELIDE_HEAD + ELIDE_TAIL + 1);
    assert_eq!(elide_middle(&same), same);

    let ticket = long_ticket();
    let elided = elide_middle(&ticket);
    assert_eq!(elided.chars().count(), ELIDE_HEAD + ELIDE_TAIL + 1);
    assert!(elided.starts_with(&ticket[..ELIDE_HEAD]));
    assert!(elided.ends_with(&ticket[ticket.len() - ELIDE_TAIL..]));
    assert!(elided.contains('…'));

    // Multi-byte input is cut on character boundaries, not bytes.
    let wide = "é".repeat(40);
    assert_eq!(
        elide_middle(&wide).chars().count(),
        ELIDE_HEAD + ELIDE_TAIL + 1
    );
}

/// Every status row is one `nowrap; overflow: hidden` line in a 560px
/// card, so a row that does not fit is cut off mid-string rather than
/// wrapped. The ~200-character rendezvous ticket is what broke this, and
/// six-digit lane counters against the longest lane labels are the other
/// edge: both are exercised here so the guard covers every row, lane rows
/// included.
#[test]
fn every_status_row_fits_one_palette_row() {
    let state = PlaceState::Offline {
        binding: binding(),
        generation: 1,
        snapshot: OfflinePlaceSnapshot {
            sync: Some(PlaceSyncSnapshot {
                lanes: vec![
                    PlaceLaneSnapshot {
                        name: commons::encrypted::COMMONS_ENCRYPTED_GRAPH_LANE,
                        syncing: true,
                        sync_rounds: 123456,
                        ops_received: 987654,
                        last_activity_ms: Some(42),
                    },
                    PlaceLaneSnapshot {
                        name: "gemot/constitution/v1",
                        syncing: false,
                        sync_rounds: 123456,
                        ops_received: 987654,
                        last_activity_ms: None,
                    },
                    PlaceLaneSnapshot {
                        name: stickleback::GROUP_KEY_LANE,
                        syncing: true,
                        sync_rounds: 123456,
                        ops_received: 987654,
                        last_activity_ms: None,
                    },
                ],
                projection: None,
                local_rendezvous: vec![long_ticket()],
                dialed_rendezvous: 0,
            }),
            permissions: Some(PlacePermissionSnapshot {
                message_write: true,
                graph_write: false,
            }),
            group: GroupCache {
                pending_frames: 123456,
                refused_frames: 987654,
                parked_records: 123456,
                readmitted_records: 987654,
                evicted_records: 123456,
                ..Default::default()
            },
            ..Default::default()
        },
    };

    let rows = state.status_lines();
    assert!(
        rows.iter().any(|row| row.starts_with("Local rendezvous:")),
        "the rendezvous row is the one under test",
    );
    for row in [
        "Group keys: syncing; 123456 rounds; 987654 accepted ops",
        "Group key frames: 123456 pending; 987654 refused",
        "Awaiting keys: 123456 parked; 987654 re-admitted; 123456 evicted",
    ] {
        assert!(
            rows.iter().any(|line| line == row),
            "{row} missing: {rows:?}"
        );
    }
    assert!(
        rows.iter()
            .any(|row| row == "Projection: not serving (no Knot vault)"),
        "a place with no vault says so rather than omitting the row: {rows:?}"
    );
    for row in rows.iter() {
        let width = crate::ui::chrome_row_width(row);
        assert!(
            width <= crate::ui::ROW_TEXT_BUDGET,
            "status row must fit one palette row: {width}px > {}px: {row}",
            crate::ui::ROW_TEXT_BUDGET,
        );
    }
}

/// Each withdrawal names itself in one row that fits; nothing withdrawn
/// adds no row.
#[test]
fn a_withdrawn_standing_names_itself_in_one_row() {
    let rows = |standing| {
        PlaceState::Offline {
            binding: binding(),
            generation: 1,
            snapshot: OfflinePlaceSnapshot {
                standing,
                ..Default::default()
            },
        }
        .status_lines()
    };
    let member = rows(PlaceStanding::Member);
    for (standing, line) in [
        (PlaceStanding::MembershipRevoked, "Membership: revoked"),
        (PlaceStanding::GrantRevoked, "Grant: revoked"),
        (
            PlaceStanding::GrantExpired {
                at_ms: 1_789_000_000_000,
            },
            "Grant: expired at 2026-09-10T00:26:40Z",
        ),
    ] {
        let withdrawn = rows(standing);
        assert_eq!(withdrawn.len(), member.len() + 1);
        assert!(withdrawn.iter().any(|row| row == line), "{withdrawn:?}");
        assert!(crate::ui::chrome_row_width(line) <= crate::ui::ROW_TEXT_BUDGET);
    }
}

/// The serving wording, singular and plural, and both fit one row.
#[test]
fn the_projection_row_names_what_is_served_and_what_was_refused() {
    let serving = |live, refused| {
        let state = PlaceState::Offline {
            binding: binding(),
            generation: 1,
            snapshot: OfflinePlaceSnapshot {
                sync: Some(PlaceSyncSnapshot {
                    projection: Some(PlaceProjectionSnapshot {
                        live_sessions: live,
                        refused,
                        route: "knot".into(),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };
        let rows = state.status_lines();
        let row = rows
            .iter()
            .find(|row| row.starts_with("Projection:"))
            .expect("the projection row is always present when lanes are")
            .clone();
        assert!(
            crate::ui::chrome_row_width(&row) <= crate::ui::ROW_TEXT_BUDGET,
            "the projection row must fit one palette row: {row}"
        );
        row
    };
    assert_eq!(
        serving(1, 1),
        "Projection: serving knot to 1 live session; 1 refused"
    );
    assert_eq!(
        serving(2, 0),
        "Projection: serving knot to 2 live sessions; 0 refused"
    );
}

#[test]
fn binding_json_uses_distinct_readable_hex_ids() {
    let json = serde_json::to_string_pretty(&binding()).unwrap();
    assert!(json.contains(&"01".repeat(32)));
    assert!(json.contains(&"02".repeat(32)));
    assert!(json.contains(&"03".repeat(32)));
    assert!(!json.contains("[\n    1,"));
    let restored: PlaceBindingV1 = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, binding());
}

#[test]
fn binding_validation_rejects_version_and_channel_ambiguity() {
    let mut candidate = binding();
    candidate.version = 2;
    assert_eq!(
        candidate.validate(),
        Err(PlaceBindingError::UnsupportedVersion(2))
    );

    candidate.version = PLACE_BINDING_VERSION;
    candidate.default_channel = " \t".into();
    assert_eq!(
        candidate.validate(),
        Err(PlaceBindingError::EmptyDefaultChannel)
    );

    candidate.default_channel = "hall\nannouncements".into();
    assert_eq!(
        candidate.validate(),
        Err(PlaceBindingError::DefaultChannelHasControl)
    );
}

#[test]
fn serialized_id_requires_exact_hex() {
    let error = serde_json::from_str::<PlaceId>("\"abcd\"").unwrap_err();
    assert!(error.to_string().contains("expected 64"));
    let error = serde_json::from_str::<PlaceId>(&format!("\"{}g\"", "0".repeat(63))).unwrap_err();
    assert!(error.to_string().contains("non-lowercase-hexadecimal"));
    let error = serde_json::from_str::<PlaceId>(&format!("\"{}A\"", "0".repeat(63))).unwrap_err();
    assert!(error.to_string().contains("non-lowercase-hexadecimal"));
}
