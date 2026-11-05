// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::place::{OfflinePlaceSnapshot, catalogue::CatalogueSiteDraft};

#[test]
fn catalogue_advertisement_reaches_later_invitee_then_withdraws_over_live_lanes() {
    let root = tempfile::tempdir().unwrap();
    let host = root.path().join("host");
    let guest = root.path().join("guest");
    let (founder, session, binding) = found_live(&host, 71);
    let draft = CatalogueSiteDraft {
        address: "scroll://garden.test/".into(),
        title: "Garden".into(),
        abstract_text: "Painting and music".into(),
        scroll_subject: Some(7),
    };
    founder.0.command(PlaceWorkerCommand::Author {
        session,
        generation: 99,
        request: 1,
        command: PlaceCommand::AdvertiseSite(draft.clone()),
    });
    assert!(authored(&founder.1, 1).unwrap_err().contains("departed"));
    founder.0.command(PlaceWorkerCommand::Author {
        session,
        generation: 1,
        request: 2,
        command: PlaceCommand::AdvertiseSite(draft),
    });
    let published = authored(&founder.1, 2).unwrap();
    assert_eq!(published.catalogues.len(), 1, "{published:?}");
    let site = published.catalogues[0].clone();
    // Publishing predates both invitation and peer connection. The invitation
    // must carry the new exact reader grant, and live retry must carry the hint.
    let invite = invite_writer(&founder, session, &host, &guest, 72, binding.moot.0);
    let (member, updates, member_session) = join_member(&guest, 72, invite);
    let received = until_catalogue(
        &founder.0,
        session,
        &member,
        &updates,
        member_session,
        |snapshot| {
            snapshot
                .catalogues
                .iter()
                .any(|row| row.digest == site.digest)
        },
    );
    assert_eq!(received.catalogues[0].scroll_subject, Some(7));
    member.command(PlaceWorkerCommand::Author {
        session: member_session,
        generation: 1,
        request: 3,
        command: PlaceCommand::OpenCatalogue {
            digest: site.digest,
        },
    });
    assert_eq!(
        authored(&updates, 3)
            .unwrap()
            .catalogue_navigation
            .as_deref(),
        Some("scroll://garden.test/")
    );
    founder.0.command(PlaceWorkerCommand::Author {
        session,
        generation: 1,
        request: 4,
        command: PlaceCommand::WithdrawSite {
            collection: site.collection,
        },
    });
    assert!(authored(&founder.1, 4).unwrap().catalogues.is_empty());
    until_catalogue(
        &founder.0,
        session,
        &member,
        &updates,
        member_session,
        |snapshot| snapshot.catalogues.is_empty(),
    );
    member.command(PlaceWorkerCommand::Author {
        session: member_session,
        generation: 1,
        request: 5,
        command: PlaceCommand::OpenCatalogue {
            digest: site.digest,
        },
    });
    assert!(
        authored(&updates, 5).is_err(),
        "withdrawn URL must not navigate"
    );
    release(&member);
    release(&founder.0);
}
fn until_catalogue(
    founder: &armillary::ActorHandle<PlaceWorkerCommand>,
    founder_session: SessionId,
    member: &armillary::ActorHandle<PlaceWorkerCommand>,
    updates: &std::sync::mpsc::Receiver<Update>,
    session: SessionId,
    done: impl Fn(&OfflinePlaceSnapshot) -> bool,
) -> OfflinePlaceSnapshot {
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut last = None;
    loop {
        assert!(
            Instant::now() < deadline,
            "catalogue failed to converge: {last:?}"
        );
        founder.command(PlaceWorkerCommand::Resync {
            session: founder_session,
            generation: 1,
        });
        member.command(PlaceWorkerCommand::Resync {
            session,
            generation: 1,
        });
        if let Ok(Update::PlaceOpened {
            result: Ok(snapshot),
            ..
        }) = updates.recv_timeout(Duration::from_millis(500))
        {
            if done(&snapshot) {
                return snapshot;
            }
            last = Some(snapshot);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
