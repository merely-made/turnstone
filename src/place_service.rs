// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! One deferred place worker; custody configuration alone is never demand.
use crate::{
    action::Update,
    place::worker::{PlaceWorkerCommand as C, PlaceWorkerSettings},
};
use armillary::demand::{Admission, DemandActor, DemandUpdates};
pub(crate) type PlacePort = DemandActor<C, Update>;
pub(crate) type PlaceUpdates = DemandUpdates<C, Update>;
pub(crate) fn configure(
    wake: armillary::Wake,
    settings: PlaceWorkerSettings,
) -> (PlacePort, PlaceUpdates) {
    #[cfg(feature = "peers")]
    let factory = {
        let w = wake.clone();
        Some(
            Box::new(move || crate::place::worker::spawn_pending_place_worker(w, settings))
                as Box<dyn FnOnce() -> _ + Send>,
        )
    };
    #[cfg(not(feature = "peers"))]
    let factory = {
        let _ = settings;
        None
    };
    DemandActor::configured(factory, admit, refuse, wake)
}
fn admit(command: C, running: bool) -> Admission<C> {
    if running {
        return Admission::Demand(command);
    }
    match command {
        C::AdoptIdentity(_) => Admission::Deferred("identity", command),
        C::Release(ack) => {
            let _ = ack.send(());
            Admission::Complete
        },
        C::RetireIdentity(ack) => {
            let _ = ack.send(());
            Admission::Withdraw
        },
        #[cfg(test)]
        C::Abandon(ack) | C::Freeze(_, ack) => {
            let _ = ack.send(());
            Admission::Complete
        },
        _ => Admission::Demand(command),
    }
}
fn refuse(command: C) -> Option<Update> {
    Some(match command {
        C::Open {
            session,
            generation,
            ..
        }
        | C::Reconnect {
            session,
            generation,
            ..
        }
        | C::Resync {
            session,
            generation,
        } => Update::PlaceOpened {
            session,
            generation,
            result: Err("Places require a build with the peers feature".into()),
        },
        C::Found {
            session,
            generation,
            ..
        } => Update::PlaceFounded {
            session,
            generation,
            result: Err("Places require a build with the peers feature".into()),
        },
        C::Join {
            session,
            generation,
            ..
        } => Update::PlaceJoined {
            session,
            generation,
            result: Err("Places require a build with the peers feature".into()),
        },
        C::OfferPrekey {
            session,
            generation,
            ..
        } => Update::PlacePrekeyOffered {
            session,
            generation,
            result: Err("Places require a build with the peers feature".into()),
        },
        C::Invite {
            session,
            generation,
            ..
        } => Update::PlaceInvited {
            session,
            generation,
            result: Err("Places require a build with the peers feature".into()),
        },
        C::SetCollection {
            session,
            generation,
            ..
        } => Update::PlaceCollectionSet {
            session,
            generation,
            result: Err("Places require a build with the peers feature".into()),
        },
        C::Author {
            session,
            generation,
            request,
            ..
        } => Update::PlaceCommandDone {
            session,
            generation,
            request,
            result: Err("Places require a build with the peers feature".into()),
        },
        C::VisitDocument {
            session,
            generation,
            request,
            holder_root,
            path,
            ..
        } => Update::PlaceDocumentVisit {
            session,
            generation,
            request,
            holder_root,
            path,
            result: Err("Places require a build with the peers feature".into()),
        },
        _ => return None,
    })
}

#[cfg(test)]
#[path = "place_service_tests.rs"]
mod tests;
