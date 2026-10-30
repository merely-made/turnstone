// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::cell::Cell;
use std::convert::Infallible;

use super::*;
use crate::sky_receipt::{SKY_RECEIPT_VERSION, SkyBodyV1, SkyFactKindV1, SkyLimbV1, SkyReceiptV1};
use jiff::civil::{date, time};
use turquet::events::{HorizonDipModel, LimbModel, MEAN_LUNAR_RADIUS_KM, RefractionModel};
use turquet::foundation::{
    AccuracyEvidence, Angle, Direction, Distance, EastLongitude, Latitude, Length, Longitude,
    State, TimeOffset, TrueEclipticEquinoxOfDate, UniversalTime1,
};
use turquet::observer::EarthOrientation;
use turquet::provider::{AnalyticalEphemeris, ConstantOffsetEarthOrientation};

const SPY_MODEL: Model = Model::new("Turnstone Sky provider spy", "1");
const MEAN_LUNAR_LIMB: Model = Model::new("mean lunar physical-radius limb", "1");

struct SpyProvider {
    calls: Cell<usize>,
}

impl SpyProvider {
    fn new() -> Self {
        Self {
            calls: Cell::new(0),
        }
    }
}

impl GeocentricPositionProvider for SpyProvider {
    type Error = Infallible;

    fn model(&self) -> Model {
        SPY_MODEL
    }

    fn accuracy(&self) -> Option<Accuracy> {
        Some(
            Accuracy::new(
                Angle::from_degrees(0.25).unwrap(),
                AccuracyEvidence::ExternalComparison,
                "Turnstone test",
                "fixed provider spy",
            )
            .unwrap(),
        )
    }

    fn position(
        &self,
        _body: ApparentBody,
        epoch: JulianDate<TerrestrialTime>,
    ) -> Result<State<TrueEclipticEquinoxOfDate>, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(State::new(
            epoch,
            Direction::new(
                Longitude::from_degrees(45.0).unwrap(),
                Latitude::from_degrees(10.0).unwrap(),
            ),
            Distance::from_astronomical_units(1.0).unwrap(),
        ))
    }
}

fn boston() -> Observer {
    Observer::new(
        EastLongitude::from_degrees(-71.0589).unwrap(),
        Latitude::from_degrees(42.3601).unwrap(),
        Length::from_meters(0.0).unwrap(),
    )
}

fn center_policy() -> ConventionalRiseSetPolicy {
    ConventionalRiseSetPolicy::new(
        RefractionModel::none(),
        LimbModel::center(),
        HorizonDipModel::level(),
    )
}

fn request(date: Date) -> SkyDayRequest {
    SkyDayRequest::new(
        date,
        "America/New_York",
        boston(),
        time(12, 0, 0, 0),
        vec![SkyBodySelection::new(ApparentBody::Sun, center_policy())],
        SkySearchControls::one_hour_one_second(),
    )
    .unwrap()
}

fn orientation_with_dut1(
    day: &ResolvedSkyDay,
    dut1_seconds: f64,
    authority: &str,
    snapshot: &str,
) -> ConstantOffsetEarthOrientation {
    let utc = TimeZone::UTC.to_datetime(day.start_utc());
    let epoch = ScaleAwareEpoch::from_gregorian_utc(
        i32::from(utc.year()),
        utc.month() as u8,
        utc.day() as u8,
        utc.hour() as u8,
        utc.minute() as u8,
        utc.second() as u8,
        utc.subsec_nanosecond() as u32,
    );
    let ut1 = JulianDate::<UniversalTime1>::from_utc_epoch(
        epoch,
        TimeOffset::from_seconds(dut1_seconds).unwrap(),
    );
    ConstantOffsetEarthOrientation::new(
        day.start_tt(),
        EarthOrientation::zero_polar_motion(ut1, authority, snapshot),
    )
}

fn orientation(day: &ResolvedSkyDay) -> ConstantOffsetEarthOrientation {
    orientation_with_dut1(day, 0.0, "Turnstone test", "UT1=UTC; xp=yp=0")
}

fn sun_policy() -> ConventionalRiseSetPolicy {
    ConventionalRiseSetPolicy::new(
        RefractionModel::usno_standard(),
        LimbModel::usno_standard_solar(),
        HorizonDipModel::level(),
    )
}

fn moon_policy() -> ConventionalRiseSetPolicy {
    ConventionalRiseSetPolicy::new(
        RefractionModel::usno_standard(),
        LimbModel::upper_physical_radius(
            Distance::from_kilometers(MEAN_LUNAR_RADIUS_KM).unwrap(),
            MEAN_LUNAR_LIMB,
        )
        .unwrap(),
        HorizonDipModel::level(),
    )
}

fn fact_count(timeline: &SkyTimeline, kind: SkyFactKind, body: ApparentBody) -> usize {
    timeline
        .receipt()
        .facts()
        .iter()
        .filter(|fact| fact.kind() == kind && fact.body() == body)
        .count()
}

fn local_minute(timeline: &SkyTimeline, body: ApparentBody, kind: SkyFactKind) -> i32 {
    let item = timeline
        .items()
        .iter()
        .find(|item| {
            let fact = receipt_fact(item);
            fact.body() == body && fact.kind() == kind
        })
        .expect("requested fact");
    let local = timeline.local_time(item).unwrap();
    i32::from(local.hour()) * 60 + i32::from(local.minute())
}

#[test]
fn new_york_spring_transition_is_a_23_hour_half_open_day() {
    let day = request(date(2024, 3, 10)).resolve_day().unwrap();
    assert_eq!(day.elapsed_seconds(), 23 * 60 * 60);
    assert!(day.contains(day.start_tt()));
    assert!(!day.contains(day.end_tt()));
}

#[test]
fn new_york_fall_transition_is_a_25_hour_half_open_day() {
    let day = request(date(2024, 11, 3)).resolve_day().unwrap();
    assert_eq!(day.elapsed_seconds(), 25 * 60 * 60);
}

#[test]
fn request_rejects_gap_and_fold_anchors() {
    let body = SkyBodySelection::new(ApparentBody::Sun, center_policy());
    let gap = SkyDayRequest::new(
        date(2024, 3, 10),
        "America/New_York",
        boston(),
        time(2, 30, 0, 0),
        vec![body],
        SkySearchControls::one_hour_one_second(),
    );
    assert!(matches!(
        gap,
        Err(SkyRequestError::NonexistentLocalTime { .. })
    ));

    let fold = SkyDayRequest::new(
        date(2024, 11, 3),
        "America/New_York",
        boston(),
        time(1, 30, 0, 0),
        vec![body],
        SkySearchControls::one_hour_one_second(),
    );
    assert!(matches!(
        fold,
        Err(SkyRequestError::AmbiguousLocalTime { .. })
    ));
}

#[test]
fn skipped_midnight_still_resolves_the_named_calendar_day() {
    let request = SkyDayRequest::new(
        date(2015, 10, 18),
        "America/Sao_Paulo",
        boston(),
        time(12, 0, 0, 0),
        vec![SkyBodySelection::new(ApparentBody::Sun, center_policy())],
        SkySearchControls::one_hour_one_second(),
    )
    .unwrap();
    let day = request.resolve_day().unwrap();

    assert_eq!(day.start_utc().to_string(), "2015-10-18T03:00:00Z");
    assert_eq!(day.end_utc().to_string(), "2015-10-19T02:00:00Z");
    assert_eq!(day.elapsed_seconds(), 23 * 60 * 60);
}

#[test]
fn request_rejects_unknown_zone_and_duplicate_body() {
    let body = SkyBodySelection::new(ApparentBody::Sun, center_policy());
    let unknown = SkyDayRequest::new(
        date(2024, 4, 8),
        "Mars/Olympus_Mons",
        boston(),
        time(12, 0, 0, 0),
        vec![body],
        SkySearchControls::one_hour_one_second(),
    );
    assert!(matches!(unknown, Err(SkyRequestError::UnknownTimeZone(_))));

    let duplicate = SkyDayRequest::new(
        date(2024, 4, 8),
        "America/New_York",
        boston(),
        time(12, 0, 0, 0),
        vec![body, body],
        SkySearchControls::one_hour_one_second(),
    );
    assert_eq!(
        duplicate,
        Err(SkyRequestError::DuplicateBody(ApparentBody::Sun))
    );
}

#[test]
fn daily_projection_uses_public_provider_and_retains_receipt_identity() {
    let request = request(date(2024, 4, 8));
    let day = request.resolve_day().unwrap();
    let eop = orientation(&day);
    let provider = SpyProvider::new();
    let timeline = build_sky_timeline(&provider, &eop, &request).unwrap();

    assert!(
        provider.calls.get() > 1,
        "event searches must use the provider"
    );
    assert_eq!(timeline.receipt().provider_model(), SPY_MODEL);
    assert!(timeline.receipt().provider_accuracy().is_some());
    assert_eq!(
        timeline.receipt().earth_orientation_snapshot(),
        "UT1=UTC; xp=yp=0"
    );
    assert!(
        timeline
            .items()
            .iter()
            .any(|item| matches!(item, SkyTimelineItem::Position(_)))
    );
    for item in timeline.items() {
        let local = timeline.local_time(item).unwrap();
        assert_eq!(local.time_zone().iana_name(), Some("America/New_York"));
    }
}

#[test]
fn boston_eclipse_day_composes_an_analytical_sun_moon_receipt() {
    let request = SkyDayRequest::new(
        date(2024, 4, 8),
        "America/New_York",
        boston(),
        time(12, 0, 0, 0),
        vec![
            SkyBodySelection::new(ApparentBody::Sun, sun_policy()),
            SkyBodySelection::new(ApparentBody::Moon, moon_policy()),
        ],
        SkySearchControls::one_hour_one_second(),
    )
    .unwrap();
    let day = request.resolve_day().unwrap();
    assert_eq!(day.start_utc().to_string(), "2024-04-08T04:00:00Z");
    assert_eq!(day.end_utc().to_string(), "2024-04-09T04:00:00Z");
    let eop = orientation_with_dut1(
        &day,
        -0.01669,
        "Turnstone test fixture",
        "2024-04-08 DUT1=-0.01669 s; xp=yp=0 approximation",
    );
    let timeline = build_sky_timeline(&AnalyticalEphemeris, &eop, &request).unwrap();

    assert_eq!(timeline.items().len(), 11);
    for body in [ApparentBody::Sun, ApparentBody::Moon] {
        assert_eq!(fact_count(&timeline, SkyFactKind::Position, body), 1);
        assert_eq!(fact_count(&timeline, SkyFactKind::Rise, body), 1);
        assert_eq!(fact_count(&timeline, SkyFactKind::Set, body), 1);
        assert_eq!(fact_count(&timeline, SkyFactKind::UpperTransit, body), 1);
        assert_eq!(fact_count(&timeline, SkyFactKind::LowerTransit, body), 1);
    }
    assert_eq!(
        fact_count(
            &timeline,
            SkyFactKind::LunarPhase(LunarPhase::NewMoon),
            ApparentBody::Moon,
        ),
        1
    );
    assert_eq!(
        timeline.receipt().provider_model(),
        turquet::apparent::ANALYTICAL_APPARENT
    );
    assert_eq!(timeline.receipt().provider_snapshot(), None);
    assert_eq!(
        timeline
            .receipt()
            .provider_accuracy()
            .expect("analytical accuracy")
            .max_angular_error()
            .degrees(),
        0.010
    );
    assert_eq!(
        timeline.receipt().earth_orientation_authority(),
        "Turnstone test fixture"
    );
    assert_eq!(timeline.receipt().search(), request.search());
    assert_eq!(
        timeline.receipt().bodies()[0].rise_set_policy(),
        sun_policy()
    );
    assert_eq!(
        timeline.receipt().bodies()[1].rise_set_policy(),
        moon_policy()
    );

    let receipt = SkyReceiptV1::from_timeline(&timeline);
    let bytes = receipt.to_pretty_json().unwrap();
    assert_eq!(receipt.version, SKY_RECEIPT_VERSION);
    assert_eq!(receipt.day.date, "2024-04-08");
    assert_eq!(receipt.day.time_zone, "America/New_York");
    assert_eq!(receipt.facts.len(), 11);
    assert!(matches!(
        receipt.bodies[0].rise_set.limb,
        SkyLimbV1::UpperAngularRadius { .. }
    ));
    assert!(matches!(
        receipt.bodies[1].rise_set.limb,
        SkyLimbV1::UpperPhysicalRadius { .. }
    ));
    assert!(
        receipt
            .facts
            .iter()
            .any(|fact| { fact.body == SkyBodyV1::Moon && fact.kind == SkyFactKindV1::NewMoon })
    );
    assert_eq!(receipt.to_pretty_json().unwrap(), bytes);
    assert_eq!(SkyReceiptV1::from_json(&bytes).unwrap(), receipt);
    assert_eq!(
        receipt.blake3_hex_digest().unwrap(),
        blake3::hash(&bytes).to_hex().to_string()
    );
    let repeated = build_sky_timeline(&AnalyticalEphemeris, &eop, &request).unwrap();
    assert_eq!(
        SkyReceiptV1::from_timeline(&repeated)
            .to_pretty_json()
            .unwrap(),
        bytes,
        "the same provider and request produce identical receipt bytes on this platform"
    );

    let mut prior = None;
    for item in timeline.items() {
        let instant = item.instant().day();
        if let Some(prior) = prior {
            assert!(instant >= prior);
        }
        prior = Some(instant);
        assert_eq!(timeline.local_time(item).unwrap().date(), date(2024, 4, 8));
        let span = item.span();
        if matches!(item, SkyTimelineItem::Position(_)) {
            assert_eq!(span.start().parts(), span.end().parts());
        } else {
            assert!((span.end().day() - span.start().day()) * 86_400.0 <= 1.01);
        }
    }

    assert!((local_minute(&timeline, ApparentBody::Sun, SkyFactKind::Rise) - 374).abs() <= 5);
    assert!((local_minute(&timeline, ApparentBody::Sun, SkyFactKind::Set) - 1_159).abs() <= 5);
    assert!(
        (local_minute(
            &timeline,
            ApparentBody::Moon,
            SkyFactKind::LunarPhase(LunarPhase::NewMoon),
        ) - 861)
            .abs()
            <= 5
    );
}

#[test]
fn tromso_solstice_reports_no_sampled_conventional_crossing_under_policy() {
    let tromso = Observer::new(
        EastLongitude::from_degrees(18.9553).unwrap(),
        Latitude::from_degrees(69.6492).unwrap(),
        Length::from_meters(0.0).unwrap(),
    );
    let request = SkyDayRequest::new(
        date(2024, 6, 21),
        "Europe/Oslo",
        tromso,
        time(12, 0, 0, 0),
        vec![SkyBodySelection::new(ApparentBody::Sun, center_policy())],
        SkySearchControls::one_hour_one_second(),
    )
    .unwrap();
    let day = request.resolve_day().unwrap();
    assert_eq!(day.start_utc().to_string(), "2024-06-20T22:00:00Z");
    assert_eq!(day.end_utc().to_string(), "2024-06-21T22:00:00Z");
    let eop = orientation_with_dut1(
        &day,
        -0.012,
        "Turnstone test fixture",
        "2024-06-21 DUT1=-0.012 s; xp=yp=0 approximation",
    );
    let timeline = build_sky_timeline(&AnalyticalEphemeris, &eop, &request).unwrap();

    assert_eq!(timeline.items().len(), 3);
    assert_eq!(
        fact_count(&timeline, SkyFactKind::Position, ApparentBody::Sun),
        1
    );
    assert_eq!(
        fact_count(&timeline, SkyFactKind::Rise, ApparentBody::Sun),
        0
    );
    assert_eq!(
        fact_count(&timeline, SkyFactKind::Set, ApparentBody::Sun),
        0
    );
    assert_eq!(
        fact_count(&timeline, SkyFactKind::UpperTransit, ApparentBody::Sun),
        1
    );
    assert_eq!(
        fact_count(&timeline, SkyFactKind::LowerTransit, ApparentBody::Sun),
        1
    );
    assert_eq!(
        timeline.receipt().bodies()[0].rise_set_policy(),
        center_policy()
    );
    assert!(
        timeline
            .items()
            .iter()
            .all(|item| timeline.local_time(item).unwrap().date() == date(2024, 6, 21))
    );
}
