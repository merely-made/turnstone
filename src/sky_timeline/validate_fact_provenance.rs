// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_fact_provenance<P, E>(
    items: &[SkyTimelineItem],
    request: &SkyDayRequest,
    provider_model: Model,
    provider_snapshot: Option<&str>,
    observer_transform_model: Model,
    observer_transform_accuracy: Accuracy,
    earth_orientation_authority: &str,
    earth_orientation_snapshot: &str,
) -> Result<(), SkyTimelineError<P, E>> {
    for item in items {
        let mismatch = match item {
            SkyTimelineItem::Position(fact) => {
                let observation = fact.observation();
                let orientation = observation.value().earth_orientation();
                (observation.model() != observer_transform_model)
                    .then_some("position transform model differs from receipt header")
                    .or_else(|| {
                        (observation.accuracy() != observer_transform_accuracy)
                            .then_some("position transform accuracy differs from receipt header")
                    })
                    .or_else(|| {
                        (observation.value().observer() != request.observer)
                            .then_some("position observer differs from request")
                    })
                    .or_else(|| {
                        (orientation.authority() != earth_orientation_authority
                            || orientation.snapshot() != earth_orientation_snapshot)
                            .then_some(
                                "position Earth-orientation identity differs from receipt header",
                            )
                    })
            },
            SkyTimelineItem::LunarPhase(event) => (event.provider_model() != provider_model
                || event.provider_snapshot() != provider_snapshot)
                .then_some("lunar-phase provider identity differs from receipt header"),
            SkyTimelineItem::RiseSet(event) => {
                let requested_policy = request
                    .bodies
                    .iter()
                    .find(|selected| selected.body() == event.body())
                    .map(|selected| selected.rise_set_policy());
                (event.provider_model() != provider_model
                    || event.provider_snapshot() != provider_snapshot)
                    .then_some("rise/set provider identity differs from receipt header")
                    .or_else(|| {
                        (event.observer() != request.observer)
                            .then_some("rise/set observer differs from request")
                    })
                    .or_else(|| {
                        (event.transform_model() != observer_transform_model)
                            .then_some("rise/set transform model differs from receipt header")
                    })
                    .or_else(|| {
                        (event.earth_orientation_authority() != earth_orientation_authority
                            || event.earth_orientation_snapshot() != earth_orientation_snapshot)
                            .then_some(
                                "rise/set Earth-orientation identity differs from receipt header",
                            )
                    })
                    .or_else(|| {
                        (requested_policy != Some(event.policy()))
                            .then_some("rise/set policy differs from the selected body policy")
                    })
            },
            SkyTimelineItem::MeridianTransit(event) => (event.provider_model() != provider_model
                || event.provider_snapshot() != provider_snapshot)
                .then_some("transit provider identity differs from receipt header")
                .or_else(|| {
                    (event.observer() != request.observer)
                        .then_some("transit observer differs from request")
                })
                .or_else(|| {
                    (event.transform_model() != observer_transform_model)
                        .then_some("transit transform model differs from receipt header")
                })
                .or_else(|| {
                    (event.earth_orientation_authority() != earth_orientation_authority
                        || event.earth_orientation_snapshot() != earth_orientation_snapshot)
                        .then_some("transit Earth-orientation identity differs from receipt header")
                }),
        };
        if let Some(message) = mismatch {
            return Err(SkyTimelineError::FactProvenanceMismatch(format!(
                "{} {}: {message}",
                item.body().name(),
                fact_kind_label(receipt_fact(item).kind())
            )));
        }
    }

    Ok(())
}
