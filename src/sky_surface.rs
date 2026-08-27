//! Retained Cambium surface for Turnstone's daily Sky consumer.
//!
//! The pane source is immutable opening provenance. Editable fields live in
//! one retained session as an explicitly ephemeral draft; a successful
//! calculation atomically replaces that session's applied source, timeline,
//! and receipt. Durable Sky settings need a host-owned source-replacement seam
//! and are deliberately outside this first consumer surface.

mod view;

use std::time::Duration;

use cambium::{DomHandle, RetainedSurfaceSession, RunnerSurfaceSession};
use genet_host_api::{ProviderId, SourceKindId, SurfaceDescriptor, SurfaceId};
use jiff::{
    civil::{Date, Time},
    tz::TimeZone,
};
use serde::{Deserialize, Serialize};
use turquet::apparent::ApparentBody;
use turquet::events::{
    ConventionalRiseSetKind, ConventionalRiseSetPolicy, HorizonDipModel, LimbModel, LunarPhase,
    MEAN_LUNAR_RADIUS_KM, MeridianTransitKind, RefractionModel,
};
use turquet::foundation::{
    Distance, EastLongitude, JulianDate, Latitude, Length, Model, Observer, ScaleAwareEpoch,
    TimeOffset, UniversalTime1,
};
use turquet::observer::EarthOrientation;
use turquet::provider::{AnalyticalEphemeris, ConstantOffsetEarthOrientation};

use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId};
use crate::sky_receipt::SkyReceiptV1;
use crate::sky_timeline::{
    SkyBodySelection, SkyDayRequest, SkySearchControls, SkyTimeline, SkyTimelineItem,
    build_sky_timeline,
};

pub const PANE_KIND: &str = "turnstone.sky";
pub const SURFACE_ID: &str = "turnstone.sky.v1";
pub const SOURCE_SCHEMA: &str = "turnstone.sky-day.v1";
pub const SOURCE_VERSION: u32 = 1;

const MEAN_LUNAR_LIMB: Model = Model::new("mean lunar physical-radius limb", "1");

pub const SKY_SURFACE_CSS: &str = r#"
.setting-row { display: flex; align-items: center; gap: 8px; padding: 5px 14px; }
.setting-label { color: rgb(150, 160, 180); font-size: 12px; }
.setting-apply { color: rgb(238, 242, 250); background-color: rgb(36, 44, 62); border: 1px solid rgb(70, 82, 110); border-radius: 6px; padding: 5px 9px; }
.sky-controls { padding: 8px; }
.sky-fact { padding: 6px 8px; }
.sky-receipt-digest { font-family: monospace; }
.sky-alert { padding: 8px; }
"#;

/// The supported first-consumer body vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkyPaneBodyV1 {
    Sun,
    Moon,
}

impl SkyPaneBodyV1 {
    fn apparent(self) -> ApparentBody {
        match self {
            Self::Sun => ApparentBody::Sun,
            Self::Moon => ApparentBody::Moon,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sun => "Sun",
            Self::Moon => "Moon",
        }
    }
}

/// A named, inspectable bundle of refraction, limb, and horizon choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkyRiseSetPresetV1 {
    ConventionalUpperLimb,
    GeometricCenter,
}

impl SkyRiseSetPresetV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::ConventionalUpperLimb => "USNO fixed refraction, upper limb, level horizon",
            Self::GeometricCenter => "no refraction, center crossing, level horizon",
        }
    }

    fn policy(self, body: SkyPaneBodyV1) -> ConventionalRiseSetPolicy {
        match self {
            Self::GeometricCenter => ConventionalRiseSetPolicy::new(
                RefractionModel::none(),
                LimbModel::center(),
                HorizonDipModel::level(),
            ),
            Self::ConventionalUpperLimb => {
                let limb = match body {
                    SkyPaneBodyV1::Sun => LimbModel::usno_standard_solar(),
                    SkyPaneBodyV1::Moon => LimbModel::upper_physical_radius(
                        Distance::from_kilometers(MEAN_LUNAR_RADIUS_KM)
                            .expect("the published mean lunar radius is positive"),
                        MEAN_LUNAR_LIMB,
                    )
                    .expect("the published mean lunar radius is positive"),
                };
                ConventionalRiseSetPolicy::new(
                    RefractionModel::usno_standard(),
                    limb,
                    HorizonDipModel::level(),
                )
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyPaneBodySourceV1 {
    pub body: SkyPaneBodyV1,
    pub rise_set: SkyRiseSetPresetV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyPaneObserverV1 {
    pub label: String,
    pub longitude_east_degrees: f64,
    pub latitude_degrees: f64,
    pub height_meters_above_wgs84_ellipsoid: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyPaneSearchV1 {
    pub sample_step_seconds: u64,
    pub tolerance_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyPaneEarthOrientationV1 {
    pub dut1_seconds: f64,
    pub authority: String,
    pub snapshot: String,
    pub reference_date: String,
}

/// Immutable opening provenance for one retained Sky pane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyPaneSourceV1 {
    pub date: String,
    pub time_zone: String,
    pub observer: SkyPaneObserverV1,
    pub anchor_local_time: String,
    pub bodies: Vec<SkyPaneBodySourceV1>,
    pub search: SkyPaneSearchV1,
    pub earth_orientation: SkyPaneEarthOrientationV1,
}

impl SkyPaneSourceV1 {
    /// The deterministic reference source used by the first palette action and
    /// the headed receipt. The pane says that it is a Boston reference rather
    /// than presenting it as detected location.
    pub fn boston_eclipse_reference() -> Self {
        Self {
            date: "2024-04-08".into(),
            time_zone: "America/New_York".into(),
            observer: SkyPaneObserverV1 {
                label: "Boston reference observer".into(),
                longitude_east_degrees: -71.0589,
                latitude_degrees: 42.3601,
                height_meters_above_wgs84_ellipsoid: 0.0,
            },
            anchor_local_time: "12:00:00".into(),
            bodies: vec![
                SkyPaneBodySourceV1 {
                    body: SkyPaneBodyV1::Sun,
                    rise_set: SkyRiseSetPresetV1::ConventionalUpperLimb,
                },
                SkyPaneBodySourceV1 {
                    body: SkyPaneBodyV1::Moon,
                    rise_set: SkyRiseSetPresetV1::ConventionalUpperLimb,
                },
            ],
            search: SkyPaneSearchV1 {
                sample_step_seconds: 3_600,
                tolerance_seconds: 1,
            },
            earth_orientation: SkyPaneEarthOrientationV1 {
                dut1_seconds: -0.01669,
                authority: "Turnstone test fixture".into(),
                snapshot: "2024-04-08 DUT1=-0.01669 s; xp=yp=0 approximation".into(),
                reference_date: "2024-04-08".into(),
            },
        }
    }
}

pub fn boston_eclipse_reference_source() -> PaneSource {
    pane_source(SkyPaneSourceV1::boston_eclipse_reference())
}

pub fn pane_source(source: SkyPaneSourceV1) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: SOURCE_VERSION,
            payload: serde_json::to_value(source).expect("Sky source V1 is serializable"),
        },
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SkyTimelineRow {
    pub local_label: String,
    pub summary: String,
    pub interval_label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SkyPaneProjection {
    pub timeline: SkyTimeline,
    pub receipt: SkyReceiptV1,
    pub digest: String,
    pub rows: Vec<SkyTimelineRow>,
    pub empty_rise_set: Vec<String>,
}

pub(crate) fn calculate_source(source: &SkyPaneSourceV1) -> Result<SkyPaneProjection, String> {
    validate_source_identity(source)?;
    let date: Date = source
        .date
        .parse()
        .map_err(|error| format!("Date must be an ISO calendar date: {error}"))?;
    if date.to_string() != source.date {
        return Err("Date must use canonical YYYY-MM-DD form".into());
    }
    let anchor: Time = source
        .anchor_local_time
        .parse()
        .map_err(|error| format!("Anchor must be a civil time: {error}"))?;
    let observer = Observer::new(
        EastLongitude::from_degrees(source.observer.longitude_east_degrees)
            .map_err(|error| format!("Invalid east longitude: {error:?}"))?,
        Latitude::from_degrees(source.observer.latitude_degrees)
            .map_err(|error| format!("Invalid latitude: {error:?}"))?,
        Length::from_meters(source.observer.height_meters_above_wgs84_ellipsoid)
            .map_err(|error| format!("Invalid WGS84 ellipsoid height: {error:?}"))?,
    );
    let bodies = source
        .bodies
        .iter()
        .map(|selected| {
            SkyBodySelection::new(
                selected.body.apparent(),
                selected.rise_set.policy(selected.body),
            )
        })
        .collect();
    let search = SkySearchControls::new(
        Duration::from_secs(source.search.sample_step_seconds),
        Duration::from_secs(source.search.tolerance_seconds),
    )
    .map_err(|error| format!("Invalid Sky search controls: {error}"))?;
    let request = SkyDayRequest::new(
        date,
        source.time_zone.clone(),
        observer,
        anchor,
        bodies,
        search,
    )
    .map_err(|error| error.to_string())?;
    let day = request.resolve_day().map_err(|error| error.to_string())?;
    let eop = earth_orientation_provider(source, &day)?;
    let timeline = build_sky_timeline(&AnalyticalEphemeris, &eop, &request)
        .map_err(|error| error.to_string())?;
    let receipt = SkyReceiptV1::from_timeline(&timeline);
    let digest = receipt
        .blake3_hex_digest()
        .map_err(|error| format!("Could not encode the Sky receipt: {error}"))?;
    let rows = timeline
        .items()
        .iter()
        .map(|item| timeline_row(&timeline, item))
        .collect::<Result<Vec<_>, _>>()?;
    let empty_rise_set = source
        .bodies
        .iter()
        .filter(|selected| {
            !timeline.items().iter().any(|item| {
                matches!(item, SkyTimelineItem::RiseSet(event) if event.body() == selected.body.apparent())
            })
        })
        .map(|selected| {
            format!(
                "{}: No rise or set under the selected policy in this civil day.",
                selected.body.label()
            )
        })
        .collect();
    Ok(SkyPaneProjection {
        timeline,
        receipt,
        digest,
        rows,
        empty_rise_set,
    })
}

fn validate_source_identity(source: &SkyPaneSourceV1) -> Result<(), String> {
    if source.observer.label.trim().is_empty() {
        return Err("Observer label cannot be empty".into());
    }
    if source.earth_orientation.authority.trim().is_empty()
        || source.earth_orientation.snapshot.trim().is_empty()
    {
        return Err("Earth-orientation authority and snapshot cannot be empty".into());
    }
    if !source.earth_orientation.dut1_seconds.is_finite() {
        return Err("DUT1 must be finite".into());
    }
    let reference_date: Date =
        source
            .earth_orientation
            .reference_date
            .parse()
            .map_err(|error| {
                format!("Earth-orientation reference date must be an ISO date: {error}")
            })?;
    if reference_date.to_string() != source.earth_orientation.reference_date {
        return Err("Earth-orientation reference date must use canonical YYYY-MM-DD form".into());
    }
    Ok(())
}

fn earth_orientation_provider(
    source: &SkyPaneSourceV1,
    day: &crate::sky_timeline::ResolvedSkyDay,
) -> Result<ConstantOffsetEarthOrientation, String> {
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
    let dut1 = TimeOffset::from_seconds(source.earth_orientation.dut1_seconds)
        .map_err(|error| format!("Invalid DUT1 offset: {error:?}"))?;
    let ut1 = JulianDate::<UniversalTime1>::from_utc_epoch(epoch, dut1);
    Ok(ConstantOffsetEarthOrientation::new(
        day.start_tt(),
        EarthOrientation::zero_polar_motion(
            ut1,
            &source.earth_orientation.authority,
            &source.earth_orientation.snapshot,
        ),
    ))
}

fn timeline_row(timeline: &SkyTimeline, item: &SkyTimelineItem) -> Result<SkyTimelineRow, String> {
    let local = timeline
        .local_time(item)
        .map_err(|error| format!("Could not format a Sky fact in local time: {error}"))?;
    let body = item.body().name();
    let summary = match item {
        SkyTimelineItem::Position(fact) => {
            let horizon = fact.observation().value().horizon();
            format!(
                "{body} position: altitude {:.2} deg, azimuth {:.2} deg",
                horizon.latitude().radians().to_degrees(),
                horizon.longitude().radians().to_degrees()
            )
        }
        SkyTimelineItem::LunarPhase(event) => match event.phase() {
            LunarPhase::NewMoon => "Moon: New Moon".into(),
            LunarPhase::FirstQuarter => "Moon: First Quarter".into(),
            LunarPhase::FullMoon => "Moon: Full Moon".into(),
            LunarPhase::LastQuarter => "Moon: Last Quarter".into(),
        },
        SkyTimelineItem::RiseSet(event) => format!(
            "{body}: {}",
            match event.kind() {
                ConventionalRiseSetKind::Rise => "rise",
                ConventionalRiseSetKind::Set => "set",
            }
        ),
        SkyTimelineItem::MeridianTransit(event) => format!(
            "{body}: {} transit, altitude {:.2} deg",
            match event.kind() {
                MeridianTransitKind::Upper => "upper",
                MeridianTransitKind::Lower => "lower",
            },
            event.midpoint_altitude().radians().to_degrees()
        ),
    };
    let span = item.span();
    let interval_label = if item.event_interval().is_some() {
        format!(
            "bounded TT solver interval {:.3} s (JD {:.12} to {:.12})",
            (span.end().day() - span.start().day()) * 86_400.0,
            span.start().day(),
            span.end().day()
        )
    } else {
        format!("sampled instant at TT JD {:.12}", span.start().day())
    };
    Ok(SkyTimelineRow {
        local_label: local.to_string(),
        summary,
        interval_label,
    })
}

pub struct SkySurfaceProvider {
    pane_kind: PaneKindId,
    source_schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
}

impl Default for SkySurfaceProvider {
    fn default() -> Self {
        Self {
            pane_kind: PaneKindId::new(PANE_KIND),
            source_schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: SurfaceDescriptor {
                provider_id: ProviderId::from("turnstone"),
                surface_id: SurfaceId::from(SURFACE_ID),
                label: "Sky".into(),
                accepted_source: genet_host_api::SurfaceSourceShape::One(SourceKindId::from(
                    SOURCE_SCHEMA,
                )),
            },
        }
    }
}

impl SurfaceProvider for SkySurfaceProvider {
    fn pane_kind(&self) -> &PaneKindId {
        &self.pane_kind
    }

    fn source_schema(&self) -> &SourceSchemaId {
        &self.source_schema
    }

    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    fn stylesheet(&self) -> &str {
        SKY_SURFACE_CSS
    }

    fn admit(
        &self,
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        let PaneSource::Fixed(SourceRef::External { schema, payload }) = source else {
            return Err(SurfaceAdmissionError::InvalidSource {
                expected: self.source_schema.clone(),
                actual: None,
            });
        };
        if schema != &self.source_schema {
            return Err(SurfaceAdmissionError::InvalidSource {
                expected: self.source_schema.clone(),
                actual: Some(schema.clone()),
            });
        }
        if payload.version != SOURCE_VERSION {
            return Err(invalid_payload(format!(
                "version {} is not supported; expected {SOURCE_VERSION}",
                payload.version
            )));
        }
        let source: SkyPaneSourceV1 = serde_json::from_value(payload.payload.clone())
            .map_err(|error| invalid_payload(error.to_string()))?;
        let state = view::SkySurfaceState::new(source).map_err(invalid_payload)?;
        let runner = cambium::GenetAppRunner::new(dom, view::sky_surface_view, state);
        Ok(Box::new(RunnerSurfaceSession::new(
            self.descriptor.clone(),
            runner,
            |_state: &view::SkySurfaceState| genet_host_api::SurfaceAvailability::Available,
            |state: &mut view::SkySurfaceState, viewport| state.set_viewport(viewport),
            |_action: ()| Vec::new(),
        )))
    }
}

fn invalid_payload(message: String) -> SurfaceAdmissionError {
    SurfaceAdmissionError::InvalidPayload {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        message,
    }
}

#[cfg(test)]
#[path = "sky_surface/tests.rs"]
mod tests;
