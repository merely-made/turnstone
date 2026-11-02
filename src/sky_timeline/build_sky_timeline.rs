// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Compose one daily timeline from position and event providers.
pub fn build_sky_timeline<P, E>(
    positions: &P,
    earth_orientation: &E,
    request: &SkyDayRequest,
) -> Result<SkyTimeline, SkyTimelineError<P::Error, E::Error>>
where
    P: GeocentricPositionProvider,
    E: EarthOrientationProvider,
{
    let day = request.resolve_day().map_err(SkyTimelineError::Request)?;
    let anchor_tt = request
        .resolve_anchor(&day)
        .map_err(SkyTimelineError::Request)?;
    let window = SearchWindow::new(
        day.start_tt(),
        day.end_tt(),
        request.search.step_days(),
        request.search.tolerance_days(),
    )
    .map_err(SkyTimelineError::SearchWindow)?;
    let rise_set_search =
        ConventionalRiseSetSearch::new(window).map_err(SkyTimelineError::RiseSetSearch)?;
    let transit_search =
        MeridianTransitSearch::new(window).map_err(SkyTimelineError::TransitSearch)?;

    let provider_model = positions.model();
    let provider_accuracy = positions.accuracy();
    let provider_snapshot = positions.data_snapshot().map(str::to_owned);
    let orientation_authority = earth_orientation.authority().to_owned();
    let orientation_snapshot = earth_orientation.data_snapshot().to_owned();
    let orientation =
        earth_orientation
            .at(anchor_tt)
            .map_err(|source| SkyTimelineError::EarthOrientation {
                epoch: anchor_tt,
                source,
            })?;
    if orientation.authority() != orientation_authority
        || orientation.snapshot() != orientation_snapshot
    {
        return Err(SkyTimelineError::EarthOrientationIdentityMismatch {
            expected_authority: orientation_authority,
            expected_snapshot: orientation_snapshot,
            actual_authority: orientation.authority().to_owned(),
            actual_snapshot: orientation.snapshot().to_owned(),
        });
    }
    let transform = ObserverTransform::at(anchor_tt, orientation, request.observer);

    let mut items = Vec::new();
    for selected in &request.bodies {
        let body = selected.body();
        let geocentric =
            positions
                .position(body, anchor_tt)
                .map_err(|source| SkyTimelineError::Position {
                    body,
                    epoch: anchor_tt,
                    source,
                })?;
        let observation =
            transform
                .observe(geocentric)
                .map_err(|source| SkyTimelineError::Transform {
                    body,
                    epoch: anchor_tt,
                    source,
                })?;
        items.push(SkyTimelineItem::Position(SkyPositionFact {
            body,
            epoch: anchor_tt,
            observation,
        }));

        let rise_set = conventional_rise_set_events(
            positions,
            earth_orientation,
            request.observer,
            body,
            rise_set_search,
            selected.rise_set_policy(),
        )
        .map_err(|source| SkyTimelineError::RiseSet { body, source })?;
        items.extend(
            rise_set
                .into_iter()
                .filter(|event| day.contains(event.interval().midpoint()))
                .map(SkyTimelineItem::RiseSet),
        );

        let transits = meridian_transits(
            positions,
            earth_orientation,
            request.observer,
            body,
            transit_search,
        )
        .map_err(|source| SkyTimelineError::Transit { body, source })?;
        items.extend(
            transits
                .into_iter()
                .filter(|event| day.contains(event.interval().midpoint()))
                .map(SkyTimelineItem::MeridianTransit),
        );
    }

    if request
        .bodies
        .iter()
        .any(|selected| selected.body() == ApparentBody::Moon)
    {
        let phases = ecliptic_longitude_lunar_phases(positions, window)
            .map_err(SkyTimelineError::LunarPhase)?;
        items.extend(
            phases
                .into_iter()
                .filter(|event| day.contains(event.interval().midpoint()))
                .map(SkyTimelineItem::LunarPhase),
        );
    }

    if positions.model() != provider_model
        || positions.accuracy() != provider_accuracy
        || positions.data_snapshot() != provider_snapshot.as_deref()
    {
        return Err(SkyTimelineError::FactProvenanceMismatch(
            "position-provider metadata changed during the calculation".to_owned(),
        ));
    }
    if earth_orientation.authority() != orientation_authority
        || earth_orientation.data_snapshot() != orientation_snapshot
    {
        return Err(SkyTimelineError::FactProvenanceMismatch(
            "Earth-orientation provider identity changed during the calculation".to_owned(),
        ));
    }

    let first_position = items
        .iter()
        .find_map(|item| match item {
            SkyTimelineItem::Position(fact) => Some(fact.observation()),
            _ => None,
        })
        .expect("a validated nonempty body selection always produces a position");
    let observer_transform_model = first_position.model();
    let observer_transform_accuracy = first_position.accuracy();
    validate_fact_provenance(
        &items,
        request,
        provider_model,
        provider_snapshot.as_deref(),
        observer_transform_model,
        observer_transform_accuracy,
        &orientation_authority,
        &orientation_snapshot,
    )?;

    items.sort_by(|left, right| {
        left.instant()
            .day()
            .total_cmp(&right.instant().day())
            .then_with(|| item_rank(left).cmp(&item_rank(right)))
            .then_with(|| body_rank(left.body()).cmp(&body_rank(right.body())))
    });
    let facts = items.iter().map(receipt_fact).collect();
    Ok(SkyTimeline {
        items,
        receipt: SkyDerivationReceipt {
            day,
            observer: request.observer,
            anchor_tt,
            search: request.search,
            bodies: request.bodies.clone(),
            provider_model,
            provider_accuracy,
            provider_snapshot,
            observer_transform_model,
            observer_transform_accuracy,
            earth_orientation_authority: orientation_authority,
            earth_orientation_snapshot: orientation_snapshot,
            facts,
        },
    })
}
