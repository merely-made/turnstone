// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's pure daily-Sky consumer model.
//!
//! Civil-day and presentation policy live here. Celestial calculations remain
//! in Turquet and are reached only through its public provider and event APIs.

use std::fmt;
use std::time::Duration;

use jiff::{
    Timestamp, Zoned,
    civil::{Date, DateTime as CivilDateTime, Time},
    tz::{AmbiguousOffset, TimeZone},
};
use turquet::apparent::ApparentBody;
use turquet::events::{
    ConventionalRiseSetError, ConventionalRiseSetEvent, ConventionalRiseSetKind,
    ConventionalRiseSetPolicy, ConventionalRiseSetSearch, ConventionalRiseSetSearchError,
    EventError, EventInterval, LunarPhase, LunarPhaseEvent, MeridianTransitError,
    MeridianTransitEvent, MeridianTransitKind, MeridianTransitSearch, MeridianTransitSearchError,
    SearchWindow, SearchWindowError, conventional_rise_set_events, ecliptic_longitude_lunar_phases,
    meridian_transits,
};
use turquet::foundation::{
    Accuracy, JulianDate, Model, Modelled, Observer, ScaleAwareEpoch, TerrestrialTime,
};
use turquet::observer::{Observation, ObserverTransform, ObserverTransformError};
use turquet::provider::{EarthOrientationProvider, GeocentricPositionProvider};

const MAX_SAMPLE_STEP: Duration = Duration::from_secs(60 * 60);

/// One body and the conventional horizon policy selected for it.
///
/// Policies are per body because a solar fixed limb, a lunar physical limb,
/// and a center crossing are materially different choices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyBodySelection {
    body: ApparentBody,
    rise_set_policy: ConventionalRiseSetPolicy,
}

/// Explicit numerical controls shared by the bounded daily searches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkySearchControls {
    sample_step: Duration,
    tolerance: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkySearchControlsError {
    SampleStepZero,
    SampleStepTooLarge,
    ToleranceZero,
    ToleranceExceedsStep,
}

/// Caller-owned inputs for one local calendar day.
#[derive(Clone, Debug, PartialEq)]
pub struct SkyDayRequest {
    date: Date,
    time_zone: TimeZone,
    time_zone_name: String,
    observer: Observer,
    anchor_local_time: Time,
    bodies: Vec<SkyBodySelection>,
    search: SkySearchControls,
}

/// The two real instants bounding one named-zone calendar day.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSkyDay {
    date: Date,
    time_zone: TimeZone,
    time_zone_name: String,
    start_utc: Timestamp,
    end_utc: Timestamp,
    start_tt: JulianDate<TerrestrialTime>,
    end_tt: JulianDate<TerrestrialTime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkyRequestError {
    UnknownTimeZone(String),
    EmptyBodySet,
    DuplicateBody(ApparentBody),
    CivilDateOverflow,
    CivilDayBoundaryOutOfRange {
        date: Date,
        time_zone: String,
    },
    AmbiguousLocalTime {
        local: CivilDateTime,
        time_zone: String,
    },
    NonexistentLocalTime {
        local: CivilDateTime,
        time_zone: String,
    },
    NonIncreasingCivilDay,
    AnchorOutsideDay,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkyPositionFact {
    body: ApparentBody,
    epoch: JulianDate<TerrestrialTime>,
    observation: Modelled<Observation>,
}

/// One typed row in the daily projection.
#[derive(Clone, Debug, PartialEq)]
pub enum SkyTimelineItem {
    Position(SkyPositionFact),
    LunarPhase(LunarPhaseEvent),
    RiseSet(ConventionalRiseSetEvent),
    MeridianTransit(MeridianTransitEvent),
}

/// A receipt-owned TT span for either an event interval or a sampled instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyFactSpan {
    start: JulianDate<TerrestrialTime>,
    end: JulianDate<TerrestrialTime>,
}

/// Stable, calculation-shaped kinds retained in the derivation receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkyFactKind {
    Position,
    LunarPhase(LunarPhase),
    Rise,
    Set,
    UpperTransit,
    LowerTransit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyReceiptFact {
    body: ApparentBody,
    kind: SkyFactKind,
    span: SkyFactSpan,
    calculation_model: Option<Model>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkyDerivationReceipt {
    day: ResolvedSkyDay,
    observer: Observer,
    anchor_tt: JulianDate<TerrestrialTime>,
    search: SkySearchControls,
    bodies: Vec<SkyBodySelection>,
    provider_model: Model,
    provider_accuracy: Option<Accuracy>,
    provider_snapshot: Option<String>,
    observer_transform_model: Model,
    observer_transform_accuracy: Accuracy,
    earth_orientation_authority: String,
    earth_orientation_snapshot: String,
    facts: Vec<SkyReceiptFact>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkyTimeline {
    items: Vec<SkyTimelineItem>,
    receipt: SkyDerivationReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SkyLocalTimeError {
    UtcCalendarOutOfRange(JulianDate<TerrestrialTime>),
    LeapSecond(JulianDate<TerrestrialTime>),
}

#[derive(Debug)]
pub enum SkyTimelineError<P, E> {
    Request(SkyRequestError),
    SearchWindow(SearchWindowError),
    RiseSetSearch(ConventionalRiseSetSearchError),
    TransitSearch(MeridianTransitSearchError),
    Position {
        body: ApparentBody,
        epoch: JulianDate<TerrestrialTime>,
        source: P,
    },
    EarthOrientation {
        epoch: JulianDate<TerrestrialTime>,
        source: E,
    },
    EarthOrientationIdentityMismatch {
        expected_authority: String,
        expected_snapshot: String,
        actual_authority: String,
        actual_snapshot: String,
    },
    Transform {
        body: ApparentBody,
        epoch: JulianDate<TerrestrialTime>,
        source: ObserverTransformError,
    },
    RiseSet {
        body: ApparentBody,
        source: ConventionalRiseSetError<P, E>,
    },
    Transit {
        body: ApparentBody,
        source: MeridianTransitError<P, E>,
    },
    LunarPhase(EventError<P>),
    FactProvenanceMismatch(String),
}

#[cfg(test)]
#[path = "sky_timeline/tests.rs"]
mod tests;

#[path = "sky_timeline/build_sky_timeline.rs"]
mod build_sky_timeline;
#[path = "sky_timeline/impl_fmt_display_for_sky_local.rs"]
mod impl_fmt_display_for_sky_local;
#[path = "sky_timeline/impl_fmt_display_for_sky_request.rs"]
mod impl_fmt_display_for_sky_request;
#[path = "sky_timeline/impl_fmt_display_for_sky_search.rs"]
mod impl_fmt_display_for_sky_search;
#[path = "sky_timeline/impl_from_event_interval_for.rs"]
mod impl_from_event_interval_for;
#[path = "sky_timeline/impl_std_error_error_for_sky.rs"]
mod impl_std_error_error_for_sky;
#[path = "sky_timeline/impl_std_error_error_for_sky_2.rs"]
mod impl_std_error_error_for_sky_2;
#[path = "sky_timeline/impl_std_error_error_for_sky_3.rs"]
mod impl_std_error_error_for_sky_3;
#[path = "sky_timeline/resolved_sky_day_date.rs"]
mod resolved_sky_day_date;
#[path = "sky_timeline/sky_body_selection_new.rs"]
mod sky_body_selection_new;
#[path = "sky_timeline/sky_day_request_new.rs"]
mod sky_day_request_new;
#[path = "sky_timeline/sky_derivation_receipt_day.rs"]
mod sky_derivation_receipt_day;
#[path = "sky_timeline/sky_fact_span_at.rs"]
mod sky_fact_span_at;
#[path = "sky_timeline/sky_position_fact_body.rs"]
mod sky_position_fact_body;
#[path = "sky_timeline/sky_receipt_fact_body.rs"]
mod sky_receipt_fact_body;
#[path = "sky_timeline/sky_search_controls_new.rs"]
mod sky_search_controls_new;
#[path = "sky_timeline/sky_timeline_item_instant.rs"]
mod sky_timeline_item_instant;
#[path = "sky_timeline/sky_timeline_items.rs"]
mod sky_timeline_items;
pub use build_sky_timeline::build_sky_timeline;
#[path = "sky_timeline/impl_p_fmt_display_e.rs"]
mod impl_p_fmt_display_e;
#[path = "sky_timeline/validate_fact_provenance.rs"]
mod validate_fact_provenance;
use validate_fact_provenance::validate_fact_provenance;
#[path = "sky_timeline/fact_kind_label.rs"]
mod fact_kind_label;
use fact_kind_label::fact_kind_label;
#[path = "sky_timeline/resolve_local.rs"]
mod resolve_local;
use resolve_local::resolve_local;
#[path = "sky_timeline/resolve_sky_day.rs"]
mod resolve_sky_day;
use resolve_sky_day::resolve_sky_day;
#[path = "sky_timeline/resolve_named_sky_day.rs"]
#[cfg(test)]
mod resolve_named_sky_day;
#[cfg(test)]
pub(crate) use resolve_named_sky_day::resolve_named_sky_day;
#[path = "sky_timeline/tt_from_utc.rs"]
mod tt_from_utc;
pub(crate) use tt_from_utc::tt_from_utc;
#[path = "sky_timeline/utc_from_tt.rs"]
mod utc_from_tt;
use utc_from_tt::utc_from_tt;
#[path = "sky_timeline/item_rank.rs"]
mod item_rank;
use item_rank::item_rank;
#[path = "sky_timeline/body_rank.rs"]
mod body_rank;
use body_rank::body_rank;
#[path = "sky_timeline/receipt_fact.rs"]
mod receipt_fact;
use receipt_fact::receipt_fact;
