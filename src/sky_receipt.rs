// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Stable, serializable derivation manifests for the daily Sky projection.
//!
//! The live timeline keeps Turquet's typed values. This module projects those
//! values into a Turnstone-owned V1 schema so Turquet does not need a serde
//! dependency or a product-specific wire format. A V1 receipt preserves the
//! inputs, source identities, typed-result details, and exact DTO bytes needed
//! to inspect a derivation. It does not claim that a future engine can replay
//! the calculation bit-for-bit.

use std::{
    collections::HashSet,
    f64::consts::{FRAC_PI_2, PI, TAU},
    fmt,
    time::Duration,
};

use jiff::{Timestamp, civil::Date};
use serde::{Deserialize, Serialize};
use turquet::apparent::ApparentBody;
use turquet::events::{
    ConventionalRiseSetPolicy, HorizonDipModel, LimbModel, LunarPhase, RefractionModel,
};
use turquet::foundation::{
    Accuracy, AccuracyEvidence, JulianDate, Model, Observer, TerrestrialTime,
};

use crate::sky_timeline::{
    SkyBodySelection, SkyDerivationReceipt, SkyFactKind, SkyFactSpan, SkyReceiptFact,
    SkySearchControls, SkyTimeline, SkyTimelineItem, tt_from_utc,
};

#[cfg(test)]
use crate::sky_timeline::resolve_named_sky_day;

pub const SKY_RECEIPT_VERSION: u16 = 1;
pub const TURQUET_PACKAGE: &str = "turquet";
pub const TURQUET_VERSION: &str = "0.13.0";
pub const TURQUET_SOURCE_REVISION: &str = "bc3c454f755d0bfd70ab48bd9556a1cda2213d41";

/// Turnstone's stable V1 projection of one Sky derivation receipt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyReceiptV1 {
    pub version: u16,
    pub engine: SkyEngineV1,
    pub day: SkyDayV1,
    pub observer: SkyObserverV1,
    pub anchor_tt: TtJulianDateV1,
    pub search: SkySearchV1,
    pub bodies: Vec<SkyBodyPolicyV1>,
    pub ephemeris: SkyEphemerisV1,
    pub observer_transform: SkyObserverTransformV1,
    pub earth_orientation: SkySourceIdentityV1,
    pub facts: Vec<SkyFactV1>,
}

/// Exact package identity of the calculation engine used by this schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyEngineV1 {
    pub package: String,
    pub version: String,
    pub source_revision: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyDayV1 {
    pub date: String,
    pub time_zone: String,
    pub start_utc: String,
    pub end_utc: String,
    pub start_tt: TtJulianDateV1,
    pub end_tt: TtJulianDateV1,
}

/// A two-part Julian Date explicitly named as Terrestrial Time by its field.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TtJulianDateV1 {
    pub day1: f64,
    pub day2: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkySearchV1 {
    pub sample_step_nanoseconds: u64,
    pub tolerance_nanoseconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyObserverV1 {
    pub longitude_east_radians: f64,
    pub latitude_radians: f64,
    pub height_meters_above_wgs84_ellipsoid: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyModelV1 {
    pub name: String,
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyEphemerisV1 {
    pub model: SkyModelV1,
    pub snapshot: Option<String>,
    pub angular_accuracy: Option<SkyAccuracyV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyObserverTransformV1 {
    pub model: SkyModelV1,
    pub angular_accuracy: SkyAccuracyV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyAccuracyV1 {
    pub maximum_error_radians: f64,
    pub evidence: SkyAccuracyEvidenceV1,
    pub authority: String,
    pub scope: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyAccuracyEvidenceV1 {
    Conformance,
    ExternalComparison,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkySourceIdentityV1 {
    pub authority: String,
    pub snapshot: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyBodyPolicyV1 {
    pub body: SkyBodyV1,
    pub rise_set: SkyRiseSetPolicyV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyRiseSetPolicyV1 {
    pub refraction: SkyRefractionV1,
    pub limb: SkyLimbV1,
    pub horizon_dip: SkyHorizonDipV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyRefractionV1 {
    pub model: SkyModelV1,
    pub apparent_lift_radians: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SkyLimbV1 {
    Center {
        model: SkyModelV1,
    },
    UpperAngularRadius {
        model: SkyModelV1,
        angular_radius_radians: f64,
    },
    UpperPhysicalRadius {
        model: SkyModelV1,
        physical_radius_meters: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SkyHorizonDipV1 {
    Level {
        model: SkyModelV1,
    },
    Constant {
        model: SkyModelV1,
        dip_radians: f64,
    },
    Spherical {
        model: SkyModelV1,
        radius_meters: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyBodyV1 {
    Sun,
    Moon,
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
    Pluto,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyFactV1 {
    pub body: SkyBodyV1,
    pub kind: SkyFactKindV1,
    pub span_tt: SkyFactSpanV1,
    pub calculation_model: Option<SkyModelV1>,
    pub detail: SkyFactDetailV1,
}

/// Calculation-shaped values retained for inspection without pretending that
/// the manifest contains every transient needed to recompute the event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SkyFactDetailV1 {
    Position {
        right_ascension_radians: f64,
        declination_radians: f64,
        azimuth_radians: f64,
        altitude_radians: f64,
        distance_meters: f64,
    },
    LunarPhase {
        angular_separation_radians: f64,
    },
    RiseSet {
        airless_center_altitude_radians: f64,
        refraction_offset_radians: f64,
        limb_offset_radians: f64,
        horizon_dip_offset_radians: f64,
    },
    MeridianTransit {
        midpoint_altitude_radians: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SkyFactDetailKind {
    Position,
    LunarPhase,
    RiseSet,
    MeridianTransit,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyFactSpanV1 {
    pub start: TtJulianDateV1,
    pub end: TtJulianDateV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyFactKindV1 {
    Position,
    NewMoon,
    FirstQuarter,
    FullMoon,
    LastQuarter,
    Rise,
    Set,
    UpperTransit,
    LowerTransit,
}

#[derive(Debug)]
pub enum SkyReceiptJsonError {
    Json(serde_json::Error),
    UnsupportedVersion(u16),
    Invalid(String),
}

#[cfg(test)]
#[path = "sky_receipt/tests.rs"]
mod tests;

#[path = "sky_receipt/impl_fmt_display_for_sky_receipt.rs"]
mod impl_fmt_display_for_sky_receipt;
#[path = "sky_receipt/impl_std_error_error_for_sky.rs"]
mod impl_std_error_error_for_sky;
#[path = "sky_receipt/invalid.rs"]
mod invalid;
#[path = "sky_receipt/sky_accuracy_v1_from_accuracy.rs"]
mod sky_accuracy_v1_from_accuracy;
#[path = "sky_receipt/sky_body_policy_v1_from_selection.rs"]
mod sky_body_policy_v1_from_selection;
#[path = "sky_receipt/sky_body_v1_from_body.rs"]
mod sky_body_v1_from_body;
#[path = "sky_receipt/sky_day_v1_from_receipt.rs"]
mod sky_day_v1_from_receipt;
#[path = "sky_receipt/sky_engine_v1_turquet.rs"]
mod sky_engine_v1_turquet;
#[path = "sky_receipt/sky_ephemeris_v1_validate.rs"]
mod sky_ephemeris_v1_validate;
#[path = "sky_receipt/sky_fact_detail_v1_from_item.rs"]
mod sky_fact_detail_v1_from_item;
#[path = "sky_receipt/sky_fact_kind_v1_from_kind.rs"]
mod sky_fact_kind_v1_from_kind;
#[path = "sky_receipt/sky_fact_span_v1_from_span.rs"]
mod sky_fact_span_v1_from_span;
#[path = "sky_receipt/sky_fact_v1_from_item.rs"]
mod sky_fact_v1_from_item;
#[path = "sky_receipt/sky_horizon_dip_v1_from_model.rs"]
mod sky_horizon_dip_v1_from_model;
#[path = "sky_receipt/sky_limb_v1_from_model.rs"]
mod sky_limb_v1_from_model;
#[path = "sky_receipt/sky_model_v1_from_model.rs"]
mod sky_model_v1_from_model;
#[path = "sky_receipt/sky_observer_transform_v1_validate.rs"]
mod sky_observer_transform_v1_validate;
#[path = "sky_receipt/sky_observer_v1_from_observer.rs"]
mod sky_observer_v1_from_observer;
#[path = "sky_receipt/sky_receipt_v1_from_timeline.rs"]
mod sky_receipt_v1_from_timeline;
#[path = "sky_receipt/sky_refraction_v1_from_model.rs"]
mod sky_refraction_v1_from_model;
#[path = "sky_receipt/sky_rise_set_policy_v1_from_policy.rs"]
mod sky_rise_set_policy_v1_from_policy;
#[path = "sky_receipt/sky_search_v1_validate.rs"]
mod sky_search_v1_validate;
#[path = "sky_receipt/sky_source_identity_v1_validate.rs"]
mod sky_source_identity_v1_validate;
#[path = "sky_receipt/tt_julian_date_v1_from_tt.rs"]
mod tt_julian_date_v1_from_tt;
use invalid::invalid;
#[path = "sky_receipt/validate_time_zone_identifier.rs"]
mod validate_time_zone_identifier;
use validate_time_zone_identifier::validate_time_zone_identifier;
#[path = "sky_receipt/validate_nonempty.rs"]
mod validate_nonempty;
use validate_nonempty::validate_nonempty;
#[path = "sky_receipt/validate_optional_nonempty.rs"]
mod validate_optional_nonempty;
use validate_optional_nonempty::validate_optional_nonempty;
#[path = "sky_receipt/validate_finite.rs"]
mod validate_finite;
use validate_finite::validate_finite;
#[path = "sky_receipt/validate_closed_range.rs"]
mod validate_closed_range;
use validate_closed_range::validate_closed_range;
#[path = "sky_receipt/validate_half_open_range.rs"]
mod validate_half_open_range;
use validate_half_open_range::validate_half_open_range;
#[path = "sky_receipt/validate_open_closed_range.rs"]
mod validate_open_closed_range;
use validate_open_closed_range::validate_open_closed_range;
#[path = "sky_receipt/validate_positive.rs"]
mod validate_positive;
use validate_positive::validate_positive;
#[path = "sky_receipt/validate_nonnegative.rs"]
mod validate_nonnegative;
use validate_nonnegative::validate_nonnegative;
#[path = "sky_receipt/duration_nanoseconds.rs"]
mod duration_nanoseconds;
use duration_nanoseconds::duration_nanoseconds;
