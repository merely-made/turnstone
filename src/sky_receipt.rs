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

impl SkyReceiptV1 {
    /// Project a live typed timeline into the stable V1 receipt schema.
    pub fn from_timeline(timeline: &SkyTimeline) -> Self {
        let receipt = timeline.receipt();
        assert_eq!(
            receipt.facts().len(),
            timeline.items().len(),
            "the typed timeline and derivation manifest must retain the same facts"
        );
        Self {
            version: SKY_RECEIPT_VERSION,
            engine: SkyEngineV1::turquet(),
            day: SkyDayV1::from_receipt(receipt),
            observer: SkyObserverV1::from_observer(receipt.observer()),
            anchor_tt: TtJulianDateV1::from_tt(receipt.anchor_tt()),
            search: SkySearchV1 {
                sample_step_nanoseconds: duration_nanoseconds(
                    receipt.search().sample_step(),
                    "Sky sample step",
                ),
                tolerance_nanoseconds: duration_nanoseconds(
                    receipt.search().tolerance(),
                    "Sky search tolerance",
                ),
            },
            bodies: receipt
                .bodies()
                .iter()
                .copied()
                .map(SkyBodyPolicyV1::from_selection)
                .collect(),
            ephemeris: SkyEphemerisV1 {
                model: SkyModelV1::from_model(receipt.provider_model()),
                snapshot: receipt.provider_snapshot().map(str::to_owned),
                angular_accuracy: receipt
                    .provider_accuracy()
                    .map(SkyAccuracyV1::from_accuracy),
            },
            observer_transform: SkyObserverTransformV1 {
                model: SkyModelV1::from_model(receipt.observer_transform_model()),
                angular_accuracy: SkyAccuracyV1::from_accuracy(
                    receipt.observer_transform_accuracy(),
                ),
            },
            earth_orientation: SkySourceIdentityV1 {
                authority: receipt.earth_orientation_authority().to_owned(),
                snapshot: receipt.earth_orientation_snapshot().to_owned(),
            },
            facts: receipt
                .facts()
                .iter()
                .copied()
                .zip(timeline.items())
                .map(|(fact, item)| SkyFactV1::from_item(fact, item))
                .collect(),
        }
    }

    /// Serialize using Turnstone's deterministic V1 field and vector order.
    ///
    /// This is the exact byte sequence used by [`Self::blake3_hex_digest`].
    /// It is a Turnstone schema guarantee, not an RFC 8785 claim.
    pub fn to_pretty_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    /// Deserialize and validate a V1 receipt.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SkyReceiptJsonError> {
        let receipt: Self = serde_json::from_slice(bytes).map_err(SkyReceiptJsonError::Json)?;
        receipt.validate()?;
        Ok(receipt)
    }

    /// BLAKE3 digest of the exact bytes returned by [`Self::to_pretty_json`].
    pub fn blake3_hex_digest(&self) -> Result<String, serde_json::Error> {
        let bytes = self.to_pretty_json()?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        if self.version != SKY_RECEIPT_VERSION {
            return Err(SkyReceiptJsonError::UnsupportedVersion(self.version));
        }
        self.engine.validate()?;
        let (day_start, day_end) = self.day.validate()?;
        self.observer.validate()?;
        self.search.validate()?;
        self.anchor_tt.validate("anchor TT")?;
        if self.anchor_tt.summed_day() < day_start.summed_day()
            || self.anchor_tt.summed_day() >= day_end.summed_day()
        {
            return Err(invalid("Sky anchor must belong to the half-open civil day"));
        }

        if self.bodies.is_empty() {
            return Err(invalid("a Sky receipt requires at least one selected body"));
        }
        let mut selected_bodies = HashSet::with_capacity(self.bodies.len());
        for (index, selection) in self.bodies.iter().enumerate() {
            selection.validate(index)?;
            if !selected_bodies.insert(selection.body) {
                return Err(invalid(format!(
                    "Sky body {:?} is selected more than once",
                    selection.body
                )));
            }
        }

        self.ephemeris.validate()?;
        self.observer_transform.validate()?;
        self.earth_orientation
            .validate("Earth-orientation source")?;

        let mut position_bodies = HashSet::with_capacity(selected_bodies.len());
        let mut previous_sort_key: Option<(f64, u8, u8)> = None;
        for (index, fact) in self.facts.iter().enumerate() {
            fact.span_tt.start.validate("fact TT start")?;
            fact.span_tt.end.validate("fact TT end")?;
            if fact.span_tt.start.summed_day() > fact.span_tt.end.summed_day() {
                return Err(invalid(format!(
                    "Sky fact {index} has decreasing TT bounds"
                )));
            }
            if !selected_bodies.contains(&fact.body) {
                return Err(invalid(format!(
                    "Sky fact {index} refers to an unselected body"
                )));
            }

            let midpoint = fact.span_tt.midpoint_day();
            if midpoint < day_start.summed_day() || midpoint >= day_end.summed_day() {
                return Err(invalid(format!(
                    "Sky fact {index} midpoint does not belong to the half-open civil day"
                )));
            }
            let sort_key = (midpoint, fact.kind.sort_rank(), fact.body.sort_rank());
            if previous_sort_key.is_some_and(|previous| {
                previous
                    .0
                    .total_cmp(&sort_key.0)
                    .then_with(|| previous.1.cmp(&sort_key.1))
                    .then_with(|| previous.2.cmp(&sort_key.2))
                    .is_gt()
            }) {
                return Err(invalid(format!(
                    "Sky fact {index} does not follow the production midpoint/kind/body sort order"
                )));
            }
            previous_sort_key = Some(sort_key);

            fact.validate(index)?;
            if fact.kind == SkyFactKindV1::Position {
                if fact.span_tt.start != fact.span_tt.end {
                    return Err(invalid(format!(
                        "Sky position fact {index} must have a zero-width TT span"
                    )));
                }
                if fact.span_tt.start != self.anchor_tt {
                    return Err(invalid(format!(
                        "Sky position fact {index} must be evaluated at the receipt anchor"
                    )));
                }
                if !position_bodies.insert(fact.body) {
                    return Err(invalid(format!(
                        "Sky body {:?} has more than one position fact",
                        fact.body
                    )));
                }
            }
        }
        if position_bodies != selected_bodies {
            return Err(invalid(
                "each selected Sky body must have exactly one position fact",
            ));
        }
        Ok(())
    }
}

/// Exact package identity of the calculation engine used by this schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyEngineV1 {
    pub package: String,
    pub version: String,
    pub source_revision: String,
}

impl SkyEngineV1 {
    fn turquet() -> Self {
        Self {
            package: TURQUET_PACKAGE.to_owned(),
            version: TURQUET_VERSION.to_owned(),
            source_revision: TURQUET_SOURCE_REVISION.to_owned(),
        }
    }

    fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        validate_nonempty("Sky engine package", &self.package)?;
        validate_nonempty("Sky engine version", &self.version)?;
        validate_nonempty("Sky engine source revision", &self.source_revision)?;
        if self.package != TURQUET_PACKAGE
            || self.version != TURQUET_VERSION
            || self.source_revision != TURQUET_SOURCE_REVISION
        {
            return Err(invalid(format!(
                "Sky receipt V1 requires {TURQUET_PACKAGE} {TURQUET_VERSION} at {TURQUET_SOURCE_REVISION}"
            )));
        }
        Ok(())
    }
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

impl SkyDayV1 {
    fn from_receipt(receipt: &SkyDerivationReceipt) -> Self {
        let day = receipt.day();
        Self {
            date: day.date().to_string(),
            time_zone: day.time_zone_name().to_owned(),
            start_utc: day.start_utc().to_string(),
            end_utc: day.end_utc().to_string(),
            start_tt: TtJulianDateV1::from_tt(day.start_tt()),
            end_tt: TtJulianDateV1::from_tt(day.end_tt()),
        }
    }

    fn validate(&self) -> Result<(TtJulianDateV1, TtJulianDateV1), SkyReceiptJsonError> {
        let date: Date = self
            .date
            .parse()
            .map_err(|_| invalid(format!("invalid Sky civil date '{}'", self.date)))?;
        if self.date != date.to_string() {
            return Err(invalid("Sky civil date must use Jiff's canonical form"));
        }
        // The stored UTC boundaries are the durable result of resolving this
        // label when the receipt was minted. A later tzdb may retire the label
        // or assign different civil rules, so reading never resolves it again.
        validate_time_zone_identifier(&self.time_zone)?;
        let start_utc: Timestamp = self
            .start_utc
            .parse()
            .map_err(|_| invalid("invalid Sky civil-day UTC start"))?;
        let end_utc: Timestamp = self
            .end_utc
            .parse()
            .map_err(|_| invalid("invalid Sky civil-day UTC end"))?;
        if self.start_utc != start_utc.to_string() {
            return Err(invalid("Sky civil-day UTC start must use canonical form"));
        }
        if self.end_utc != end_utc.to_string() {
            return Err(invalid("Sky civil-day UTC end must use canonical form"));
        }
        if start_utc >= end_utc {
            return Err(invalid("Sky civil-day UTC bounds must increase"));
        }

        self.start_tt.validate("civil-day TT start")?;
        self.end_tt.validate("civil-day TT end")?;
        let expected_start_tt = TtJulianDateV1::from_tt(tt_from_utc(start_utc));
        let expected_end_tt = TtJulianDateV1::from_tt(tt_from_utc(end_utc));
        if self.start_tt != expected_start_tt {
            return Err(invalid(
                "Sky civil-day two-part TT start does not match its UTC boundary",
            ));
        }
        if self.end_tt != expected_end_tt {
            return Err(invalid(
                "Sky civil-day two-part TT end does not match its UTC boundary",
            ));
        }
        Ok((expected_start_tt, expected_end_tt))
    }
}

/// A two-part Julian Date explicitly named as Terrestrial Time by its field.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TtJulianDateV1 {
    pub day1: f64,
    pub day2: f64,
}

impl TtJulianDateV1 {
    fn from_tt(value: JulianDate<TerrestrialTime>) -> Self {
        let (day1, day2) = value.parts();
        Self { day1, day2 }
    }

    fn summed_day(&self) -> f64 {
        self.day1 + self.day2
    }

    fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        if self.day1.is_finite() && self.day2.is_finite() && self.summed_day().is_finite() {
            Ok(())
        } else {
            Err(SkyReceiptJsonError::Invalid(format!(
                "{field} must contain finite Julian-date parts"
            )))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkySearchV1 {
    pub sample_step_nanoseconds: u64,
    pub tolerance_nanoseconds: u64,
}

impl SkySearchV1 {
    fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        SkySearchControls::new(
            Duration::from_nanos(self.sample_step_nanoseconds),
            Duration::from_nanos(self.tolerance_nanoseconds),
        )
        .map(|_| ())
        .map_err(|source| invalid(format!("invalid Sky search controls: {source}")))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyObserverV1 {
    pub longitude_east_radians: f64,
    pub latitude_radians: f64,
    pub height_meters_above_wgs84_ellipsoid: f64,
}

impl SkyObserverV1 {
    fn from_observer(observer: Observer) -> Self {
        Self {
            longitude_east_radians: observer.longitude().radians(),
            latitude_radians: observer.latitude().radians(),
            height_meters_above_wgs84_ellipsoid: observer.height().meters(),
        }
    }

    fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        validate_half_open_range(
            "Sky observer east longitude",
            self.longitude_east_radians,
            -PI,
            PI,
        )?;
        validate_closed_range(
            "Sky observer latitude",
            self.latitude_radians,
            -FRAC_PI_2,
            FRAC_PI_2,
        )?;
        validate_finite(
            "Sky observer ellipsoid height",
            self.height_meters_above_wgs84_ellipsoid,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyModelV1 {
    pub name: String,
    pub revision: String,
}

impl SkyModelV1 {
    fn from_model(model: Model) -> Self {
        Self {
            name: model.name().to_owned(),
            revision: model.revision().to_owned(),
        }
    }

    fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        validate_nonempty(&format!("{field} name"), &self.name)?;
        validate_nonempty(&format!("{field} revision"), &self.revision)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyEphemerisV1 {
    pub model: SkyModelV1,
    pub snapshot: Option<String>,
    pub angular_accuracy: Option<SkyAccuracyV1>,
}

impl SkyEphemerisV1 {
    fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        self.model.validate("Sky ephemeris model")?;
        validate_optional_nonempty("Sky ephemeris snapshot", self.snapshot.as_deref())?;
        if let Some(accuracy) = &self.angular_accuracy {
            accuracy.validate("Sky ephemeris accuracy")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyObserverTransformV1 {
    pub model: SkyModelV1,
    pub angular_accuracy: SkyAccuracyV1,
}

impl SkyObserverTransformV1 {
    fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        self.model.validate("Sky observer-transform model")?;
        self.angular_accuracy
            .validate("Sky observer-transform accuracy")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyAccuracyV1 {
    pub maximum_error_radians: f64,
    pub evidence: SkyAccuracyEvidenceV1,
    pub authority: String,
    pub scope: String,
}

impl SkyAccuracyV1 {
    fn from_accuracy(accuracy: Accuracy) -> Self {
        Self {
            maximum_error_radians: accuracy.max_angular_error().radians(),
            evidence: match accuracy.evidence() {
                AccuracyEvidence::Conformance => SkyAccuracyEvidenceV1::Conformance,
                AccuracyEvidence::ExternalComparison => SkyAccuracyEvidenceV1::ExternalComparison,
            },
            authority: accuracy.authority().to_owned(),
            scope: accuracy.scope().to_owned(),
        }
    }

    fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        validate_closed_range(
            &format!("{field} maximum angular error"),
            self.maximum_error_radians,
            0.0,
            PI,
        )?;
        validate_nonempty(&format!("{field} authority"), &self.authority)?;
        validate_nonempty(&format!("{field} scope"), &self.scope)
    }
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

impl SkySourceIdentityV1 {
    fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        validate_nonempty(&format!("{field} authority"), &self.authority)?;
        validate_nonempty(&format!("{field} snapshot"), &self.snapshot)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyBodyPolicyV1 {
    pub body: SkyBodyV1,
    pub rise_set: SkyRiseSetPolicyV1,
}

impl SkyBodyPolicyV1 {
    fn from_selection(selection: SkyBodySelection) -> Self {
        Self {
            body: SkyBodyV1::from_body(selection.body()),
            rise_set: SkyRiseSetPolicyV1::from_policy(selection.rise_set_policy()),
        }
    }

    fn validate(&self, index: usize) -> Result<(), SkyReceiptJsonError> {
        self.rise_set.validate(&format!("Sky body policy {index}"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyRiseSetPolicyV1 {
    pub refraction: SkyRefractionV1,
    pub limb: SkyLimbV1,
    pub horizon_dip: SkyHorizonDipV1,
}

impl SkyRiseSetPolicyV1 {
    fn from_policy(policy: ConventionalRiseSetPolicy) -> Self {
        Self {
            refraction: SkyRefractionV1::from_model(policy.refraction()),
            limb: SkyLimbV1::from_model(policy.limb()),
            horizon_dip: SkyHorizonDipV1::from_model(policy.horizon_dip()),
        }
    }

    fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        self.refraction.validate(field)?;
        self.limb.validate(field)?;
        self.horizon_dip.validate(field)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyRefractionV1 {
    pub model: SkyModelV1,
    pub apparent_lift_radians: f64,
}

impl SkyRefractionV1 {
    fn from_model(refraction: RefractionModel) -> Self {
        Self {
            model: SkyModelV1::from_model(refraction.model()),
            apparent_lift_radians: refraction.apparent_lift().radians(),
        }
    }

    fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        self.model.validate(&format!("{field} refraction model"))?;
        validate_closed_range(
            &format!("{field} apparent refraction lift"),
            self.apparent_lift_radians,
            0.0,
            FRAC_PI_2,
        )
    }
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

impl SkyLimbV1 {
    fn from_model(limb: LimbModel) -> Self {
        let model = SkyModelV1::from_model(limb.model());
        match (limb.angular_radius(), limb.physical_radius()) {
            (None, None) => Self::Center { model },
            (Some(radius), None) => Self::UpperAngularRadius {
                model,
                angular_radius_radians: radius.radians(),
            },
            (None, Some(radius)) => Self::UpperPhysicalRadius {
                model,
                physical_radius_meters: radius.meters(),
            },
            (Some(_), Some(_)) => {
                unreachable!("a validated Turquet limb has one radius representation")
            }
        }
    }

    fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        match self {
            Self::Center { model } => model.validate(&format!("{field} limb model")),
            Self::UpperAngularRadius {
                model,
                angular_radius_radians,
            } => {
                model.validate(&format!("{field} limb model"))?;
                validate_open_closed_range(
                    &format!("{field} upper-limb angular radius"),
                    *angular_radius_radians,
                    0.0,
                    FRAC_PI_2,
                )
            }
            Self::UpperPhysicalRadius {
                model,
                physical_radius_meters,
            } => {
                model.validate(&format!("{field} limb model"))?;
                validate_positive(
                    &format!("{field} upper-limb physical radius"),
                    *physical_radius_meters,
                )
            }
        }
    }
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

impl SkyHorizonDipV1 {
    fn from_model(horizon: HorizonDipModel) -> Self {
        let model = SkyModelV1::from_model(horizon.model());
        match (horizon.constant_dip(), horizon.spherical_radius()) {
            (None, None) => Self::Level { model },
            (Some(dip), None) => Self::Constant {
                model,
                dip_radians: dip.radians(),
            },
            (None, Some(radius)) => Self::Spherical {
                model,
                radius_meters: radius.meters(),
            },
            (Some(_), Some(_)) => {
                unreachable!("a validated Turquet horizon has one dip representation")
            }
        }
    }

    fn validate(&self, field: &str) -> Result<(), SkyReceiptJsonError> {
        match self {
            Self::Level { model } => model.validate(&format!("{field} horizon-dip model")),
            Self::Constant { model, dip_radians } => {
                model.validate(&format!("{field} horizon-dip model"))?;
                validate_closed_range(
                    &format!("{field} constant horizon dip"),
                    *dip_radians,
                    0.0,
                    FRAC_PI_2,
                )
            }
            Self::Spherical {
                model,
                radius_meters,
            } => {
                model.validate(&format!("{field} horizon-dip model"))?;
                validate_positive(&format!("{field} spherical-horizon radius"), *radius_meters)
            }
        }
    }
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

impl SkyBodyV1 {
    fn from_body(body: ApparentBody) -> Self {
        match body {
            ApparentBody::Sun => Self::Sun,
            ApparentBody::Moon => Self::Moon,
            ApparentBody::Mercury => Self::Mercury,
            ApparentBody::Venus => Self::Venus,
            ApparentBody::Mars => Self::Mars,
            ApparentBody::Jupiter => Self::Jupiter,
            ApparentBody::Saturn => Self::Saturn,
            ApparentBody::Uranus => Self::Uranus,
            ApparentBody::Neptune => Self::Neptune,
            ApparentBody::Pluto => Self::Pluto,
        }
    }

    fn sort_rank(self) -> u8 {
        match self {
            Self::Sun => 0,
            Self::Moon => 1,
            Self::Mercury => 2,
            Self::Venus => 3,
            Self::Mars => 4,
            Self::Jupiter => 5,
            Self::Saturn => 6,
            Self::Uranus => 7,
            Self::Neptune => 8,
            Self::Pluto => 9,
        }
    }
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

impl SkyFactV1 {
    fn from_item(fact: SkyReceiptFact, item: &SkyTimelineItem) -> Self {
        Self {
            body: SkyBodyV1::from_body(fact.body()),
            kind: SkyFactKindV1::from_kind(fact.kind()),
            span_tt: SkyFactSpanV1::from_span(fact.span()),
            calculation_model: fact.calculation_model().map(SkyModelV1::from_model),
            detail: SkyFactDetailV1::from_item(item),
        }
    }

    fn validate(&self, index: usize) -> Result<(), SkyReceiptJsonError> {
        let expected_detail = match self.kind {
            SkyFactKindV1::Position => SkyFactDetailKind::Position,
            SkyFactKindV1::NewMoon
            | SkyFactKindV1::FirstQuarter
            | SkyFactKindV1::FullMoon
            | SkyFactKindV1::LastQuarter => SkyFactDetailKind::LunarPhase,
            SkyFactKindV1::Rise | SkyFactKindV1::Set => SkyFactDetailKind::RiseSet,
            SkyFactKindV1::UpperTransit | SkyFactKindV1::LowerTransit => {
                SkyFactDetailKind::MeridianTransit
            }
        };
        if self.detail.kind() != expected_detail {
            return Err(invalid(format!(
                "Sky fact {index} kind and detail projection do not match"
            )));
        }
        if matches!(
            self.kind,
            SkyFactKindV1::NewMoon
                | SkyFactKindV1::FirstQuarter
                | SkyFactKindV1::FullMoon
                | SkyFactKindV1::LastQuarter
        ) && self.body != SkyBodyV1::Moon
        {
            return Err(invalid(format!(
                "Sky lunar-phase fact {index} must name the Moon"
            )));
        }

        let model_required = matches!(
            self.kind,
            SkyFactKindV1::Rise
                | SkyFactKindV1::Set
                | SkyFactKindV1::UpperTransit
                | SkyFactKindV1::LowerTransit
        );
        match (&self.calculation_model, model_required) {
            (Some(model), true) => {
                model.validate(&format!("Sky fact {index} calculation model"))?
            }
            (None, false) => {}
            (Some(_), false) => {
                return Err(invalid(format!(
                    "Sky fact {index} must use the deduplicated receipt-header models"
                )));
            }
            (None, true) => {
                return Err(invalid(format!(
                    "Sky fact {index} is missing its event-family calculation model"
                )));
            }
        }
        self.detail.validate(index)
    }
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

impl SkyFactDetailV1 {
    fn from_item(item: &SkyTimelineItem) -> Self {
        match item {
            SkyTimelineItem::Position(fact) => {
                let observation = fact.observation().value();
                let equatorial = observation.equatorial();
                let equatorial_direction = equatorial.direction();
                let horizon = observation.horizon();
                Self::Position {
                    right_ascension_radians: equatorial_direction.longitude().radians(),
                    declination_radians: equatorial_direction.latitude().radians(),
                    azimuth_radians: horizon.longitude().radians(),
                    altitude_radians: horizon.latitude().radians(),
                    distance_meters: equatorial.distance().meters(),
                }
            }
            SkyTimelineItem::LunarPhase(event) => Self::LunarPhase {
                angular_separation_radians: event.angular_separation().radians(),
            },
            SkyTimelineItem::RiseSet(event) => Self::RiseSet {
                airless_center_altitude_radians: event.airless_center_altitude().radians(),
                refraction_offset_radians: event.refraction_offset().radians(),
                limb_offset_radians: event.limb_offset().radians(),
                horizon_dip_offset_radians: event.horizon_dip_offset().radians(),
            },
            SkyTimelineItem::MeridianTransit(event) => Self::MeridianTransit {
                midpoint_altitude_radians: event.midpoint_altitude().radians(),
            },
        }
    }

    fn kind(&self) -> SkyFactDetailKind {
        match self {
            Self::Position { .. } => SkyFactDetailKind::Position,
            Self::LunarPhase { .. } => SkyFactDetailKind::LunarPhase,
            Self::RiseSet { .. } => SkyFactDetailKind::RiseSet,
            Self::MeridianTransit { .. } => SkyFactDetailKind::MeridianTransit,
        }
    }

    fn validate(&self, index: usize) -> Result<(), SkyReceiptJsonError> {
        match self {
            Self::Position {
                right_ascension_radians,
                declination_radians,
                azimuth_radians,
                altitude_radians,
                distance_meters,
            } => {
                validate_half_open_range(
                    &format!("Sky position fact {index} right ascension"),
                    *right_ascension_radians,
                    0.0,
                    TAU,
                )?;
                validate_closed_range(
                    &format!("Sky position fact {index} declination"),
                    *declination_radians,
                    -FRAC_PI_2,
                    FRAC_PI_2,
                )?;
                validate_half_open_range(
                    &format!("Sky position fact {index} azimuth"),
                    *azimuth_radians,
                    0.0,
                    TAU,
                )?;
                validate_closed_range(
                    &format!("Sky position fact {index} altitude"),
                    *altitude_radians,
                    -FRAC_PI_2,
                    FRAC_PI_2,
                )?;
                validate_nonnegative(
                    &format!("Sky position fact {index} distance"),
                    *distance_meters,
                )
            }
            Self::LunarPhase {
                angular_separation_radians,
            } => validate_closed_range(
                &format!("Sky lunar-phase fact {index} angular separation"),
                *angular_separation_radians,
                0.0,
                PI,
            ),
            Self::RiseSet {
                airless_center_altitude_radians,
                refraction_offset_radians,
                limb_offset_radians,
                horizon_dip_offset_radians,
            } => {
                validate_closed_range(
                    &format!("Sky rise/set fact {index} airless center altitude"),
                    *airless_center_altitude_radians,
                    -FRAC_PI_2,
                    FRAC_PI_2,
                )?;
                validate_closed_range(
                    &format!("Sky rise/set fact {index} refraction offset"),
                    *refraction_offset_radians,
                    0.0,
                    FRAC_PI_2,
                )?;
                validate_closed_range(
                    &format!("Sky rise/set fact {index} limb offset"),
                    *limb_offset_radians,
                    0.0,
                    FRAC_PI_2,
                )?;
                validate_closed_range(
                    &format!("Sky rise/set fact {index} horizon-dip offset"),
                    *horizon_dip_offset_radians,
                    0.0,
                    FRAC_PI_2,
                )
            }
            Self::MeridianTransit {
                midpoint_altitude_radians,
            } => validate_closed_range(
                &format!("Sky meridian-transit fact {index} midpoint altitude"),
                *midpoint_altitude_radians,
                -FRAC_PI_2,
                FRAC_PI_2,
            ),
        }
    }
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

impl SkyFactSpanV1 {
    fn from_span(span: SkyFactSpan) -> Self {
        Self {
            start: TtJulianDateV1::from_tt(span.start()),
            end: TtJulianDateV1::from_tt(span.end()),
        }
    }

    fn midpoint_day(&self) -> f64 {
        (self.start.summed_day() + self.end.summed_day()) / 2.0
    }
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

impl SkyFactKindV1 {
    fn from_kind(kind: SkyFactKind) -> Self {
        match kind {
            SkyFactKind::Position => Self::Position,
            SkyFactKind::LunarPhase(LunarPhase::NewMoon) => Self::NewMoon,
            SkyFactKind::LunarPhase(LunarPhase::FirstQuarter) => Self::FirstQuarter,
            SkyFactKind::LunarPhase(LunarPhase::FullMoon) => Self::FullMoon,
            SkyFactKind::LunarPhase(LunarPhase::LastQuarter) => Self::LastQuarter,
            SkyFactKind::Rise => Self::Rise,
            SkyFactKind::Set => Self::Set,
            SkyFactKind::UpperTransit => Self::UpperTransit,
            SkyFactKind::LowerTransit => Self::LowerTransit,
        }
    }

    fn sort_rank(self) -> u8 {
        match self {
            Self::NewMoon | Self::FirstQuarter | Self::FullMoon | Self::LastQuarter => 0,
            Self::Rise => 1,
            Self::Set => 2,
            Self::UpperTransit => 3,
            Self::LowerTransit => 4,
            Self::Position => 5,
        }
    }
}

#[derive(Debug)]
pub enum SkyReceiptJsonError {
    Json(serde_json::Error),
    UnsupportedVersion(u16),
    Invalid(String),
}

impl fmt::Display for SkyReceiptJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(source) => write!(formatter, "invalid Sky receipt JSON: {source}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported Sky receipt version {version}")
            }
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SkyReceiptJsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(source) => Some(source),
            Self::UnsupportedVersion(_) | Self::Invalid(_) => None,
        }
    }
}

fn invalid(message: impl Into<String>) -> SkyReceiptJsonError {
    SkyReceiptJsonError::Invalid(message.into())
}

fn validate_time_zone_identifier(value: &str) -> Result<(), SkyReceiptJsonError> {
    validate_nonempty("Sky IANA time-zone identifier", value)?;
    if value.len() > 255
        || !value.is_ascii()
        || value.starts_with('/')
        || value.ends_with('/')
        || value.split('/').any(|component| {
            component.is_empty()
                || component == "."
                || component == ".."
                || component.starts_with('-')
                || !component
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
        })
    {
        return Err(invalid(format!(
            "Sky time-zone identifier '{value}' is not a sane IANA-style name"
        )));
    }
    Ok(())
}

fn validate_nonempty(field: &str, value: &str) -> Result<(), SkyReceiptJsonError> {
    if value.trim().is_empty() {
        Err(invalid(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}

fn validate_optional_nonempty(field: &str, value: Option<&str>) -> Result<(), SkyReceiptJsonError> {
    if let Some(value) = value {
        validate_nonempty(field, value)?;
    }
    Ok(())
}

fn validate_finite(field: &str, value: f64) -> Result<(), SkyReceiptJsonError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid(format!("{field} must be finite")))
    }
}

fn validate_closed_range(
    field: &str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), SkyReceiptJsonError> {
    validate_finite(field, value)?;
    if (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{field} must belong to the closed range [{minimum}, {maximum}]"
        )))
    }
}

fn validate_half_open_range(
    field: &str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), SkyReceiptJsonError> {
    validate_finite(field, value)?;
    if (minimum..maximum).contains(&value) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{field} must belong to the half-open range [{minimum}, {maximum})"
        )))
    }
}

fn validate_open_closed_range(
    field: &str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), SkyReceiptJsonError> {
    validate_finite(field, value)?;
    if value > minimum && value <= maximum {
        Ok(())
    } else {
        Err(invalid(format!(
            "{field} must belong to the range ({minimum}, {maximum}]"
        )))
    }
}

fn validate_positive(field: &str, value: f64) -> Result<(), SkyReceiptJsonError> {
    validate_finite(field, value)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(invalid(format!("{field} must be positive")))
    }
}

fn validate_nonnegative(field: &str, value: f64) -> Result<(), SkyReceiptJsonError> {
    validate_finite(field, value)?;
    if value >= 0.0 {
        Ok(())
    } else {
        Err(invalid(format!("{field} must be nonnegative")))
    }
}

fn duration_nanoseconds(duration: std::time::Duration, field: &str) -> u64 {
    u64::try_from(duration.as_nanos())
        .unwrap_or_else(|_| panic!("{field} exceeds the V1 nanosecond range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_json_round_trips_and_hashes_the_exact_bytes() {
        let receipt = sample_receipt();
        let first = receipt.to_pretty_json().unwrap();
        let second = receipt.to_pretty_json().unwrap();
        assert_eq!(first, second);
        assert_eq!(SkyReceiptV1::from_json(&first).unwrap(), receipt);
        assert_eq!(
            receipt.blake3_hex_digest().unwrap(),
            blake3::hash(&first).to_hex().to_string()
        );
        assert!(
            String::from_utf8(first)
                .unwrap()
                .contains("\"angular_accuracy\": null")
        );
    }

    #[test]
    fn json_reader_rejects_another_receipt_version() {
        let mut receipt = sample_receipt();
        receipt.version = 2;
        let bytes = receipt.to_pretty_json().unwrap();
        assert!(matches!(
            SkyReceiptV1::from_json(&bytes),
            Err(SkyReceiptJsonError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn json_reader_rejects_tampered_engine_and_day_boundaries() {
        let mut receipt = sample_receipt();
        receipt.engine.source_revision = "different source".to_owned();
        assert!(invalid_message(receipt).contains("requires turquet 0.13.0"));

        let mut receipt = sample_receipt();
        receipt.day.start_utc = "2000-01-01T00:00:00+00:00".to_owned();
        assert!(invalid_message(receipt).contains("UTC start must use canonical form"));

        let mut receipt = sample_receipt();
        receipt.day.start_tt.day2 += 1.0e-9;
        assert!(invalid_message(receipt).contains("two-part TT start"));

        let mut receipt = sample_receipt();
        receipt.day.end_utc = receipt.day.start_utc.clone();
        receipt.day.end_tt = receipt.day.start_tt;
        assert!(invalid_message(receipt).contains("UTC bounds must increase"));
    }

    #[test]
    fn json_reader_keeps_a_sane_unavailable_zone_identity() {
        let mut receipt = sample_receipt();
        let unavailable_zone = "America/Retired_Test_Zone";
        assert!(jiff::tz::TimeZone::get(unavailable_zone).is_err());
        receipt.day.time_zone = unavailable_zone.to_owned();

        let bytes = receipt.to_pretty_json().unwrap();
        assert_eq!(SkyReceiptV1::from_json(&bytes).unwrap(), receipt);
    }

    #[test]
    fn json_reader_rejects_unknown_root_and_nested_fields() {
        let bytes = sample_receipt().to_pretty_json().unwrap();
        let mut root: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        root.as_object_mut()
            .unwrap()
            .insert("unknown_root".to_owned(), true.into());
        assert_json_error(serde_json::to_vec(&root).unwrap());

        let mut nested: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        nested["facts"][0]["detail"]
            .as_object_mut()
            .unwrap()
            .insert("unknown_detail".to_owned(), true.into());
        assert_json_error(serde_json::to_vec(&nested).unwrap());
    }

    #[test]
    fn json_reader_rejects_tampered_inputs_and_provenance() {
        let mut receipt = sample_receipt();
        receipt.search.sample_step_nanoseconds = 3_600_000_000_001;
        assert!(invalid_message(receipt).contains("search controls"));

        let mut receipt = sample_receipt();
        receipt.observer.latitude_radians = PI;
        assert!(invalid_message(receipt).contains("observer latitude"));

        let mut receipt = sample_receipt();
        receipt.bodies.push(receipt.bodies[0].clone());
        assert!(invalid_message(receipt).contains("selected more than once"));

        let mut receipt = sample_receipt();
        receipt.ephemeris.model.name.clear();
        assert!(invalid_message(receipt).contains("model name"));

        let mut receipt = sample_receipt();
        receipt.earth_orientation.snapshot.clear();
        assert!(invalid_message(receipt).contains("snapshot must not be empty"));

        let mut receipt = sample_receipt();
        receipt.day.time_zone = "../UTC".to_owned();
        assert!(invalid_message(receipt).contains("sane IANA-style name"));
    }

    #[test]
    fn json_reader_rejects_tampered_fact_projection() {
        let mut receipt = sample_receipt();
        receipt.facts[0].body = SkyBodyV1::Moon;
        assert!(invalid_message(receipt).contains("unselected body"));

        let mut receipt = sample_receipt();
        receipt.facts[0].span_tt.start = receipt.day.end_tt;
        receipt.facts[0].span_tt.end = receipt.day.end_tt;
        assert!(invalid_message(receipt).contains("half-open civil day"));

        let mut receipt = sample_receipt();
        receipt.facts[0].detail = SkyFactDetailV1::LunarPhase {
            angular_separation_radians: 0.0,
        };
        assert!(invalid_message(receipt).contains("detail projection"));

        let mut receipt = sample_receipt();
        receipt.facts[0].calculation_model = Some(test_model());
        assert!(invalid_message(receipt).contains("deduplicated receipt-header models"));

        let mut receipt = sample_receipt();
        receipt.facts.push(receipt.facts[0].clone());
        assert!(invalid_message(receipt).contains("more than one position fact"));

        let mut receipt = sample_receipt();
        receipt.facts.clear();
        assert!(invalid_message(receipt).contains("exactly one position fact"));
    }

    #[test]
    fn json_reader_rejects_same_midpoint_body_reordering() {
        let mut receipt = sample_receipt();
        let mut moon_policy = receipt.bodies[0].clone();
        moon_policy.body = SkyBodyV1::Moon;
        receipt.bodies.push(moon_policy);

        let mut moon_position = receipt.facts[0].clone();
        moon_position.body = SkyBodyV1::Moon;
        receipt.facts.insert(0, moon_position);

        assert!(invalid_message(receipt).contains("midpoint/kind/body sort order"));
    }

    fn invalid_message(receipt: SkyReceiptV1) -> String {
        let bytes = receipt.to_pretty_json().unwrap();
        match SkyReceiptV1::from_json(&bytes) {
            Err(SkyReceiptJsonError::Invalid(message)) => message,
            other => panic!("expected an invalid V1 receipt, got {other:?}"),
        }
    }

    fn assert_json_error(bytes: Vec<u8>) {
        assert!(matches!(
            SkyReceiptV1::from_json(&bytes),
            Err(SkyReceiptJsonError::Json(_))
        ));
    }

    fn test_model() -> SkyModelV1 {
        SkyModelV1 {
            name: "test model".to_owned(),
            revision: "1".to_owned(),
        }
    }

    fn sample_receipt() -> SkyReceiptV1 {
        let day = resolve_named_sky_day("2000-01-01".parse().unwrap(), "UTC").unwrap();
        let start_tt = TtJulianDateV1::from_tt(day.start_tt());
        let end_tt = TtJulianDateV1::from_tt(day.end_tt());
        let model = test_model();
        SkyReceiptV1 {
            version: SKY_RECEIPT_VERSION,
            engine: SkyEngineV1::turquet(),
            day: SkyDayV1 {
                date: day.date().to_string(),
                time_zone: day.time_zone_name().to_owned(),
                start_utc: day.start_utc().to_string(),
                end_utc: day.end_utc().to_string(),
                start_tt,
                end_tt,
            },
            observer: SkyObserverV1 {
                longitude_east_radians: 0.0,
                latitude_radians: 0.0,
                height_meters_above_wgs84_ellipsoid: 0.0,
            },
            anchor_tt: start_tt,
            search: SkySearchV1 {
                sample_step_nanoseconds: 3_600_000_000_000,
                tolerance_nanoseconds: 1_000_000_000,
            },
            bodies: vec![SkyBodyPolicyV1 {
                body: SkyBodyV1::Sun,
                rise_set: SkyRiseSetPolicyV1 {
                    refraction: SkyRefractionV1 {
                        model: model.clone(),
                        apparent_lift_radians: 0.0,
                    },
                    limb: SkyLimbV1::Center {
                        model: model.clone(),
                    },
                    horizon_dip: SkyHorizonDipV1::Level {
                        model: model.clone(),
                    },
                },
            }],
            ephemeris: SkyEphemerisV1 {
                model: model.clone(),
                snapshot: None,
                angular_accuracy: None,
            },
            observer_transform: SkyObserverTransformV1 {
                model: model.clone(),
                angular_accuracy: SkyAccuracyV1 {
                    maximum_error_radians: 0.001,
                    evidence: SkyAccuracyEvidenceV1::ExternalComparison,
                    authority: "test authority".to_owned(),
                    scope: "test scope".to_owned(),
                },
            },
            earth_orientation: SkySourceIdentityV1 {
                authority: "test EOP".to_owned(),
                snapshot: "test EOP snapshot".to_owned(),
            },
            facts: vec![SkyFactV1 {
                body: SkyBodyV1::Sun,
                kind: SkyFactKindV1::Position,
                span_tt: SkyFactSpanV1 {
                    start: start_tt,
                    end: start_tt,
                },
                calculation_model: None,
                detail: SkyFactDetailV1::Position {
                    right_ascension_radians: 1.0,
                    declination_radians: 0.5,
                    azimuth_radians: 2.0,
                    altitude_radians: 0.25,
                    distance_meters: 149_597_870_700.0,
                },
            }],
        }
    }
}
