// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use identity::IdentityProvider;
use servitor::delegation::DelegationTable;

fn user() -> crate::identity::RootIdentity {
    crate::identity::RootIdentity::from_seed([42u8; 32])
}

fn subject() -> Subject {
    Subject::new([9u8; 32])
}

/// A table holding real root delegations for the named rings, signed by
/// the test user. Nothing here fakes authority: an emission passes only if
/// a signed chain verifies.
fn authority_for(rings: &[Ring]) -> DelegationTable {
    let user = user();
    let mut table = DelegationTable::new(user.master_public_key().to_bytes());
    let caps: Vec<_> = rings
        .iter()
        .filter_map(|ring| ring.cap())
        .map(|cap| (cap, Mode::Write))
        .collect();
    for cert in crate::denizen::issue_install_certificates(&user, subject(), &caps, 1_000) {
        table.adopt(cert);
    }
    table.set_now(2_000);
    table
}

fn full_app_authority() -> DelegationTable {
    // Every grantable app ring at once: host-only must resist even this.
    // (A single "wider than any ring" grant is no longer expressible,
    // which is the point of powers: there is nothing above them to hold.)
    authority_for(&GRANTABLE_RINGS)
}

#[test]
fn every_ring_but_host_only_names_a_grantable_path() {
    for ring in [Ring::Navigate, Ring::Panes, Ring::Dispatch, Ring::Session] {
        assert!(ring.cap().is_some(), "{} must be grantable", ring.name());
    }
    assert_eq!(Ring::HostOnly.cap(), None, "the structural floor");
}

#[test]
fn a_covered_emission_passes_and_an_uncovered_one_names_its_ring() {
    let narrow = authority_for(&[Ring::Navigate]);
    assert!(emit_allowed(&narrow, subject(), &Action::OpenAddress("https://a".into())).is_ok());
    let denial = emit_allowed(&narrow, subject(), &Action::CloseSession)
        .expect_err("session is not covered");
    assert!(
        denial.contains("session"),
        "the denial names the ring: {denial}"
    );
}

#[test]
fn gate_management_resists_even_a_total_app_grant() {
    // The self-escalation guard: a component confirming its own install
    // review must be impossible under ANY authority.
    let authority = full_app_authority();
    for action in [
        Action::RevokePlaceMember { member: [7; 32] },
        Action::LeavePlace,
        Action::ReconnectPlace,
        Action::RejoinPlace,
        Action::ShowPlaceStatus,
        Action::ConfirmInstallDenizen,
        Action::OpenThemeWorkshop,
        Action::OpenAlembicWorkshop,
        Action::CancelInstallDenizen,
        Action::InstallDenizen {
            path: "x.lua".into(),
        },
        Action::RunDenizen {
            member: uuid::Uuid::from_u128(1),
        },
    ] {
        let denial =
            emit_allowed(&authority, subject(), &action).expect_err("host-only must refuse");
        assert!(denial.contains("host-only"), "{denial}");
    }
}

#[test]
fn contributed_source_authority_resists_a_total_app_grant() {
    let authority = full_app_authority();
    for action in [
        Action::ChooseKnotDocumentFile { read_only: false },
        Action::SummonContributedPane {
            kind: crate::panes::PaneKindId::new("test.contributed"),
            source: crate::panes::PaneSource::Fixed(crate::panes::SourceRef::Application),
        },
    ] {
        assert_eq!(ring_of(&action), Ring::HostOnly);
        let denial = emit_allowed(&authority, subject(), &action)
            .expect_err("source-bearing actions stay host-only");
        assert!(denial.contains("local source admission"), "{denial}");
    }
}

#[test]
fn envelopes_decode_and_misfires_are_loud() {
    assert_eq!(
        decode_envelope("open-address", r#"{"url": "https://a.test"}"#),
        Ok(Action::OpenAddress("https://a.test".to_string()))
    );
    assert_eq!(decode_envelope("fit-view", ""), Ok(Action::FitView));
    assert!(matches!(
        decode_envelope("open-address", r#"{}"#),
        Err(EnvelopeError::Malformed(_))
    ));
    assert!(matches!(
        decode_envelope("summon-the-kraken", ""),
        Err(EnvelopeError::Unknown(_))
    ));
    // A host-only name DECODES (so the denial downstream is exact);
    // authority is emit_allowed's job, not the decoder's.
    assert_eq!(
        decode_envelope("confirm-install-denizen", ""),
        Ok(Action::ConfirmInstallDenizen)
    );
}

#[test]
fn decoded_envelopes_classify_across_all_grantable_rings() {
    for (name, payload, ring) in [
        ("open-address", r#"{"url": "https://a"}"#, Ring::Navigate),
        ("new-window", "", Ring::Panes),
        ("fit-view", "", Ring::Dispatch),
        ("close-session", "", Ring::Session),
        (
            "run-denizen",
            r#"{"member": "00000000-0000-0000-0000-000000000001"}"#,
            Ring::HostOnly,
        ),
    ] {
        let action = decode_envelope(name, payload).expect(name);
        assert_eq!(ring_of(&action), ring, "{name}");
    }
}
