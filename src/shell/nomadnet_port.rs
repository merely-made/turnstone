// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use armillary::demand::{Admission, DemandActor, DemandUpdates};
pub(in super::super) fn spawn(
    wake: Wake,
) -> (
    DemandActor<FetchCommand, FetchUpdate>,
    DemandUpdates<FetchCommand, FetchUpdate>,
) {
    #[cfg(feature = "nomadnet")]
    let factory = {
        let w = wake.clone();
        Some(Box::new(move || spawn_active(w)) as Box<dyn FnOnce() -> _ + Send>)
    };
    #[cfg(not(feature = "nomadnet"))]
    let factory = None;
    DemandActor::configured(
        factory,
        |command, running| match command {
            FetchCommand::CancelPage { .. } if !running => Admission::Complete,
            _ => Admission::Demand(command),
        },
        |command| match command {
            FetchCommand::Page { request, url, .. } => Some(FetchUpdate::Page(FetchOutcome {
                request,
                url,
                result: Err(FetchFailure::Failed(
                    "NomadNet requires a build with the nomadnet feature".into(),
                )),
            })),
            _ => None,
        },
        wake,
    )
}
pub(in super::super) fn spawn_submissions(
    wake: Wake,
) -> (
    DemandActor<MicronSubmissionCommand, crate::action::Update>,
    DemandUpdates<MicronSubmissionCommand, crate::action::Update>,
) {
    #[cfg(feature = "nomadnet")]
    let factory = {
        let w = wake.clone();
        Some(Box::new(move || spawn_active_submissions(w)) as Box<dyn FnOnce() -> _ + Send>)
    };
    #[cfg(not(feature = "nomadnet"))]
    let factory = None;
    DemandActor::configured(
        factory,
        |command, running| match command {
            MicronSubmissionCommand::Cancel { .. } if !running => Admission::Complete,
            _ => Admission::Demand(command),
        },
        |command| match command {
            MicronSubmissionCommand::Submit {
                request,
                source,
                submission,
            } => Some(crate::action::Update::SmolwebSubmitted {
                request,
                source,
                target: submission.target,
                result: Err("NomadNet requires a build with the nomadnet feature".into()),
            }),
            _ => None,
        },
        wake,
    )
}
