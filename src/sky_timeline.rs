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

impl SkyBodySelection {
    pub fn new(body: ApparentBody, rise_set_policy: ConventionalRiseSetPolicy) -> Self {
        Self {
            body,
            rise_set_policy,
        }
    }

    pub fn body(self) -> ApparentBody {
        self.body
    }

    pub fn rise_set_policy(self) -> ConventionalRiseSetPolicy {
        self.rise_set_policy
    }
}

/// Explicit numerical controls shared by the bounded daily searches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkySearchControls {
    sample_step: Duration,
    tolerance: Duration,
}

impl SkySearchControls {
    pub fn new(sample_step: Duration, tolerance: Duration) -> Result<Self, SkySearchControlsError> {
        if sample_step.is_zero() {
            return Err(SkySearchControlsError::SampleStepZero);
        }
        if sample_step > MAX_SAMPLE_STEP {
            return Err(SkySearchControlsError::SampleStepTooLarge);
        }
        if tolerance.is_zero() {
            return Err(SkySearchControlsError::ToleranceZero);
        }
        if tolerance > sample_step {
            return Err(SkySearchControlsError::ToleranceExceedsStep);
        }
        Ok(Self {
            sample_step,
            tolerance,
        })
    }

    pub fn one_hour_one_second() -> Self {
        Self {
            sample_step: MAX_SAMPLE_STEP,
            tolerance: Duration::from_secs(1),
        }
    }

    pub fn sample_step(self) -> Duration {
        self.sample_step
    }

    pub fn tolerance(self) -> Duration {
        self.tolerance
    }

    fn step_days(self) -> f64 {
        self.sample_step.as_secs_f64() / 86_400.0
    }

    fn tolerance_days(self) -> f64 {
        self.tolerance.as_secs_f64() / 86_400.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkySearchControlsError {
    SampleStepZero,
    SampleStepTooLarge,
    ToleranceZero,
    ToleranceExceedsStep,
}

impl fmt::Display for SkySearchControlsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SampleStepZero => "Sky sample step must be positive",
            Self::SampleStepTooLarge => "Sky sample step cannot exceed one hour",
            Self::ToleranceZero => "Sky search tolerance must be positive",
            Self::ToleranceExceedsStep => "Sky search tolerance cannot exceed the sample step",
        })
    }
}

impl std::error::Error for SkySearchControlsError {}

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

impl SkyDayRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        date: Date,
        time_zone_name: impl Into<String>,
        observer: Observer,
        anchor_local_time: Time,
        bodies: Vec<SkyBodySelection>,
        search: SkySearchControls,
    ) -> Result<Self, SkyRequestError> {
        let time_zone_name = time_zone_name.into();
        let time_zone = TimeZone::get(&time_zone_name)
            .map_err(|_| SkyRequestError::UnknownTimeZone(time_zone_name.clone()))?;
        if bodies.is_empty() {
            return Err(SkyRequestError::EmptyBodySet);
        }
        for (index, selected) in bodies.iter().enumerate() {
            if bodies[..index]
                .iter()
                .any(|prior| prior.body == selected.body)
            {
                return Err(SkyRequestError::DuplicateBody(selected.body));
            }
        }
        let request = Self {
            date,
            time_zone,
            time_zone_name,
            observer,
            anchor_local_time,
            bodies,
            search,
        };
        let day = request.resolve_day()?;
        let _ = request.resolve_anchor(&day)?;
        Ok(request)
    }

    pub fn date(&self) -> Date {
        self.date
    }

    pub fn time_zone_name(&self) -> &str {
        &self.time_zone_name
    }

    pub fn observer(&self) -> Observer {
        self.observer
    }

    pub fn anchor_local_time(&self) -> Time {
        self.anchor_local_time
    }

    pub fn bodies(&self) -> &[SkyBodySelection] {
        &self.bodies
    }

    pub fn search(&self) -> SkySearchControls {
        self.search
    }

    pub fn resolve_day(&self) -> Result<ResolvedSkyDay, SkyRequestError> {
        resolve_sky_day(
            self.date,
            self.time_zone.clone(),
            self.time_zone_name.clone(),
        )
    }

    fn resolve_anchor(
        &self,
        day: &ResolvedSkyDay,
    ) -> Result<JulianDate<TerrestrialTime>, SkyRequestError> {
        let local = self.date.to_datetime(self.anchor_local_time);
        let utc = resolve_local(&self.time_zone, local, &self.time_zone_name)?;
        let anchor = tt_from_utc(utc);
        if !day.contains(anchor) {
            return Err(SkyRequestError::AnchorOutsideDay);
        }
        Ok(anchor)
    }
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

impl ResolvedSkyDay {
    pub fn date(&self) -> Date {
        self.date
    }

    pub fn time_zone_name(&self) -> &str {
        &self.time_zone_name
    }

    pub fn start_utc(&self) -> Timestamp {
        self.start_utc
    }

    pub fn end_utc(&self) -> Timestamp {
        self.end_utc
    }

    pub fn start_tt(&self) -> JulianDate<TerrestrialTime> {
        self.start_tt
    }

    pub fn end_tt(&self) -> JulianDate<TerrestrialTime> {
        self.end_tt
    }

    pub fn elapsed_seconds(&self) -> i64 {
        self.end_utc.as_second() - self.start_utc.as_second()
    }

    pub fn contains(&self, epoch: JulianDate<TerrestrialTime>) -> bool {
        epoch.day() >= self.start_tt.day() && epoch.day() < self.end_tt.day()
    }

    pub fn local_time(
        &self,
        epoch: JulianDate<TerrestrialTime>,
    ) -> Result<Zoned, SkyLocalTimeError> {
        Ok(utc_from_tt(epoch)?.to_zoned(self.time_zone.clone()))
    }
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

impl fmt::Display for SkyRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTimeZone(name) => write!(formatter, "unknown IANA time zone '{name}'"),
            Self::EmptyBodySet => formatter.write_str("a Sky day requires at least one body"),
            Self::DuplicateBody(body) => {
                write!(formatter, "Sky body '{}' was selected twice", body.name())
            }
            Self::CivilDateOverflow => {
                formatter.write_str("the following civil date is outside range")
            }
            Self::CivilDayBoundaryOutOfRange { date, time_zone } => write!(
                formatter,
                "civil date {date} cannot be resolved in {time_zone}"
            ),
            Self::AmbiguousLocalTime { local, time_zone } => write!(
                formatter,
                "local time {local} occurs twice in {time_zone}; select an unambiguous anchor"
            ),
            Self::NonexistentLocalTime { local, time_zone } => write!(
                formatter,
                "local time {local} does not exist in {time_zone}"
            ),
            Self::NonIncreasingCivilDay => {
                formatter.write_str("resolved civil-day end must follow its start")
            }
            Self::AnchorOutsideDay => {
                formatter.write_str("resolved Sky anchor does not belong to the selected day")
            }
        }
    }
}

impl std::error::Error for SkyRequestError {}

#[derive(Clone, Debug, PartialEq)]
pub struct SkyPositionFact {
    body: ApparentBody,
    epoch: JulianDate<TerrestrialTime>,
    observation: Modelled<Observation>,
}

impl SkyPositionFact {
    pub fn body(&self) -> ApparentBody {
        self.body
    }

    pub fn epoch(&self) -> JulianDate<TerrestrialTime> {
        self.epoch
    }

    pub fn observation(&self) -> &Modelled<Observation> {
        &self.observation
    }
}

/// One typed row in the daily projection.
#[derive(Clone, Debug, PartialEq)]
pub enum SkyTimelineItem {
    Position(SkyPositionFact),
    LunarPhase(LunarPhaseEvent),
    RiseSet(ConventionalRiseSetEvent),
    MeridianTransit(MeridianTransitEvent),
}

impl SkyTimelineItem {
    pub fn instant(&self) -> JulianDate<TerrestrialTime> {
        match self {
            Self::Position(fact) => fact.epoch(),
            Self::LunarPhase(event) => event.interval().midpoint(),
            Self::RiseSet(event) => event.interval().midpoint(),
            Self::MeridianTransit(event) => event.interval().midpoint(),
        }
    }

    /// The raw Turquet interval for an event row.
    ///
    /// A sampled position is an instant rather than a Turquet event, so it has
    /// no event interval. The derivation receipt represents it with a
    /// Turnstone-owned zero-width [`SkyFactSpan`] instead.
    pub fn event_interval(&self) -> Option<EventInterval> {
        match self {
            Self::Position(_) => None,
            Self::LunarPhase(event) => Some(event.interval()),
            Self::RiseSet(event) => Some(event.interval()),
            Self::MeridianTransit(event) => Some(event.interval()),
        }
    }

    pub fn span(&self) -> SkyFactSpan {
        self.event_interval()
            .map(SkyFactSpan::from)
            .unwrap_or_else(|| SkyFactSpan::at(self.instant()))
    }

    pub fn body(&self) -> ApparentBody {
        match self {
            Self::Position(fact) => fact.body(),
            Self::LunarPhase(_) => ApparentBody::Moon,
            Self::RiseSet(event) => event.body(),
            Self::MeridianTransit(event) => event.body(),
        }
    }
}

/// A receipt-owned TT span for either an event interval or a sampled instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyFactSpan {
    start: JulianDate<TerrestrialTime>,
    end: JulianDate<TerrestrialTime>,
}

impl SkyFactSpan {
    pub fn at(epoch: JulianDate<TerrestrialTime>) -> Self {
        Self {
            start: epoch,
            end: epoch,
        }
    }

    pub fn start(self) -> JulianDate<TerrestrialTime> {
        self.start
    }

    pub fn end(self) -> JulianDate<TerrestrialTime> {
        self.end
    }

    pub fn midpoint(self) -> JulianDate<TerrestrialTime> {
        JulianDate::from_julian_day((self.start.day() + self.end.day()) / 2.0)
            .expect("a span between finite epochs has a finite midpoint")
    }
}

impl From<EventInterval> for SkyFactSpan {
    fn from(interval: EventInterval) -> Self {
        Self {
            start: interval.start(),
            end: interval.end(),
        }
    }
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

impl SkyReceiptFact {
    pub fn body(self) -> ApparentBody {
        self.body
    }

    pub fn kind(self) -> SkyFactKind {
        self.kind
    }

    pub fn span(self) -> SkyFactSpan {
        self.span
    }

    /// Event-family model where Turquet exposes one separately from the
    /// ephemeris and observer-transform models retained in the receipt header.
    pub fn calculation_model(self) -> Option<Model> {
        self.calculation_model
    }
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

impl SkyDerivationReceipt {
    pub fn day(&self) -> &ResolvedSkyDay {
        &self.day
    }

    pub fn observer(&self) -> Observer {
        self.observer
    }

    pub fn anchor_tt(&self) -> JulianDate<TerrestrialTime> {
        self.anchor_tt
    }

    pub fn search(&self) -> SkySearchControls {
        self.search
    }

    pub fn bodies(&self) -> &[SkyBodySelection] {
        &self.bodies
    }

    pub fn provider_model(&self) -> Model {
        self.provider_model
    }

    pub fn provider_accuracy(&self) -> Option<Accuracy> {
        self.provider_accuracy
    }

    pub fn provider_snapshot(&self) -> Option<&str> {
        self.provider_snapshot.as_deref()
    }

    pub fn observer_transform_model(&self) -> Model {
        self.observer_transform_model
    }

    pub fn observer_transform_accuracy(&self) -> Accuracy {
        self.observer_transform_accuracy
    }

    pub fn earth_orientation_authority(&self) -> &str {
        &self.earth_orientation_authority
    }

    pub fn earth_orientation_snapshot(&self) -> &str {
        &self.earth_orientation_snapshot
    }

    pub fn facts(&self) -> &[SkyReceiptFact] {
        &self.facts
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkyTimeline {
    items: Vec<SkyTimelineItem>,
    receipt: SkyDerivationReceipt,
}

impl SkyTimeline {
    pub fn items(&self) -> &[SkyTimelineItem] {
        &self.items
    }

    pub fn receipt(&self) -> &SkyDerivationReceipt {
        &self.receipt
    }

    pub fn local_time(&self, item: &SkyTimelineItem) -> Result<Zoned, SkyLocalTimeError> {
        self.receipt.day.local_time(item.instant())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SkyLocalTimeError {
    UtcCalendarOutOfRange(JulianDate<TerrestrialTime>),
    LeapSecond(JulianDate<TerrestrialTime>),
}

impl fmt::Display for SkyLocalTimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UtcCalendarOutOfRange(epoch) => {
                write!(
                    formatter,
                    "TT JD {} is outside the civil-time range",
                    epoch.day()
                )
            }
            Self::LeapSecond(epoch) => write!(
                formatter,
                "TT JD {} maps to a UTC leap second which Jiff cannot label",
                epoch.day()
            ),
        }
    }
}

impl std::error::Error for SkyLocalTimeError {}

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

impl<P: fmt::Display, E: fmt::Display> fmt::Display for SkyTimelineError<P, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(source) => source.fmt(formatter),
            Self::SearchWindow(source) => source.fmt(formatter),
            Self::RiseSetSearch(source) => source.fmt(formatter),
            Self::TransitSearch(source) => source.fmt(formatter),
            Self::Position {
                body,
                epoch,
                source,
            } => write!(
                formatter,
                "could not position {} at TT JD {}: {source}",
                body.name(),
                epoch.day()
            ),
            Self::EarthOrientation { epoch, source } => write!(
                formatter,
                "could not resolve Earth orientation at TT JD {}: {source}",
                epoch.day()
            ),
            Self::EarthOrientationIdentityMismatch {
                expected_authority,
                expected_snapshot,
                actual_authority,
                actual_snapshot,
            } => write!(
                formatter,
                "Earth-orientation result identity {actual_authority}/{actual_snapshot} does not match provider {expected_authority}/{expected_snapshot}"
            ),
            Self::Transform {
                body,
                epoch,
                source,
            } => write!(
                formatter,
                "could not transform {} at TT JD {}: {source}",
                body.name(),
                epoch.day()
            ),
            Self::RiseSet { body, source } => {
                write!(
                    formatter,
                    "could not find {} rise/set: {source}",
                    body.name()
                )
            }
            Self::Transit { body, source } => {
                write!(
                    formatter,
                    "could not find {} transits: {source}",
                    body.name()
                )
            }
            Self::LunarPhase(source) => write!(formatter, "could not find lunar phases: {source}"),
            Self::FactProvenanceMismatch(message) => {
                write!(formatter, "Sky fact provenance mismatch: {message}")
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_fact_provenance<P, E>(
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
            }
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
            }
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

fn fact_kind_label(kind: SkyFactKind) -> &'static str {
    match kind {
        SkyFactKind::Position => "position",
        SkyFactKind::LunarPhase(LunarPhase::NewMoon) => "new moon",
        SkyFactKind::LunarPhase(LunarPhase::FirstQuarter) => "first quarter",
        SkyFactKind::LunarPhase(LunarPhase::FullMoon) => "full moon",
        SkyFactKind::LunarPhase(LunarPhase::LastQuarter) => "last quarter",
        SkyFactKind::Rise => "rise",
        SkyFactKind::Set => "set",
        SkyFactKind::UpperTransit => "upper transit",
        SkyFactKind::LowerTransit => "lower transit",
    }
}

fn resolve_local(
    time_zone: &TimeZone,
    local: CivilDateTime,
    time_zone_name: &str,
) -> Result<Timestamp, SkyRequestError> {
    let ambiguous = time_zone.to_ambiguous_timestamp(local);
    match ambiguous.offset() {
        AmbiguousOffset::Unambiguous { .. } => {
            ambiguous
                .unambiguous()
                .map_err(|_| SkyRequestError::NonexistentLocalTime {
                    local,
                    time_zone: time_zone_name.to_owned(),
                })
        }
        AmbiguousOffset::Fold { .. } => Err(SkyRequestError::AmbiguousLocalTime {
            local,
            time_zone: time_zone_name.to_owned(),
        }),
        AmbiguousOffset::Gap { .. } => Err(SkyRequestError::NonexistentLocalTime {
            local,
            time_zone: time_zone_name.to_owned(),
        }),
    }
}

fn resolve_sky_day(
    date: Date,
    time_zone: TimeZone,
    time_zone_name: String,
) -> Result<ResolvedSkyDay, SkyRequestError> {
    let following_date = date
        .tomorrow()
        .map_err(|_| SkyRequestError::CivilDateOverflow)?;
    // A calendar day still exists when an offset transition skips or repeats
    // midnight. Jiff's compatible date resolution chooses the first valid
    // instant belonging to each date. Caller-selected anchors remain strict.
    let start_utc = date
        .to_zoned(time_zone.clone())
        .map(|zoned| zoned.timestamp())
        .map_err(|_| SkyRequestError::CivilDayBoundaryOutOfRange {
            date,
            time_zone: time_zone_name.clone(),
        })?;
    let end_utc = following_date
        .to_zoned(time_zone.clone())
        .map(|zoned| zoned.timestamp())
        .map_err(|_| SkyRequestError::CivilDayBoundaryOutOfRange {
            date: following_date,
            time_zone: time_zone_name.clone(),
        })?;
    if end_utc <= start_utc {
        return Err(SkyRequestError::NonIncreasingCivilDay);
    }
    Ok(ResolvedSkyDay {
        date,
        time_zone,
        time_zone_name,
        start_utc,
        end_utc,
        start_tt: tt_from_utc(start_utc),
        end_tt: tt_from_utc(end_utc),
    })
}

#[cfg(test)]
pub(crate) fn resolve_named_sky_day(
    date: Date,
    time_zone_name: &str,
) -> Result<ResolvedSkyDay, SkyRequestError> {
    let time_zone = TimeZone::get(time_zone_name)
        .map_err(|_| SkyRequestError::UnknownTimeZone(time_zone_name.to_owned()))?;
    resolve_sky_day(date, time_zone, time_zone_name.to_owned())
}

pub(crate) fn tt_from_utc(utc: Timestamp) -> JulianDate<TerrestrialTime> {
    let utc = TimeZone::UTC.to_datetime(utc);
    JulianDate::from_epoch(ScaleAwareEpoch::from_gregorian_utc(
        i32::from(utc.year()),
        utc.month() as u8,
        utc.day() as u8,
        utc.hour() as u8,
        utc.minute() as u8,
        utc.second() as u8,
        utc.subsec_nanosecond() as u32,
    ))
}

fn utc_from_tt(epoch: JulianDate<TerrestrialTime>) -> Result<Timestamp, SkyLocalTimeError> {
    let (year, month, day, hour, minute, second, nanosecond) = epoch.to_epoch().to_gregorian_utc();
    if second == 60 {
        return Err(SkyLocalTimeError::LeapSecond(epoch));
    }
    let year = i16::try_from(year).map_err(|_| SkyLocalTimeError::UtcCalendarOutOfRange(epoch))?;
    let utc = CivilDateTime::new(
        year,
        month as i8,
        day as i8,
        hour as i8,
        minute as i8,
        second as i8,
        nanosecond as i32,
    )
    .map_err(|_| SkyLocalTimeError::UtcCalendarOutOfRange(epoch))?;
    TimeZone::UTC
        .to_timestamp(utc)
        .map_err(|_| SkyLocalTimeError::UtcCalendarOutOfRange(epoch))
}

fn item_rank(item: &SkyTimelineItem) -> u8 {
    match item {
        SkyTimelineItem::LunarPhase(_) => 0,
        SkyTimelineItem::RiseSet(event) => match event.kind() {
            ConventionalRiseSetKind::Rise => 1,
            ConventionalRiseSetKind::Set => 2,
        },
        SkyTimelineItem::MeridianTransit(event) => match event.kind() {
            MeridianTransitKind::Upper => 3,
            MeridianTransitKind::Lower => 4,
        },
        SkyTimelineItem::Position(_) => 5,
    }
}

fn body_rank(body: ApparentBody) -> u8 {
    match body {
        ApparentBody::Sun => 0,
        ApparentBody::Moon => 1,
        ApparentBody::Mercury => 2,
        ApparentBody::Venus => 3,
        ApparentBody::Mars => 4,
        ApparentBody::Jupiter => 5,
        ApparentBody::Saturn => 6,
        ApparentBody::Uranus => 7,
        ApparentBody::Neptune => 8,
        ApparentBody::Pluto => 9,
    }
}

fn receipt_fact(item: &SkyTimelineItem) -> SkyReceiptFact {
    let (kind, calculation_model) = match item {
        SkyTimelineItem::Position(_) => (SkyFactKind::Position, None),
        SkyTimelineItem::LunarPhase(event) => (SkyFactKind::LunarPhase(event.phase()), None),
        SkyTimelineItem::RiseSet(event) => match event.kind() {
            ConventionalRiseSetKind::Rise => (SkyFactKind::Rise, Some(event.circumstance_model())),
            ConventionalRiseSetKind::Set => (SkyFactKind::Set, Some(event.circumstance_model())),
        },
        SkyTimelineItem::MeridianTransit(event) => match event.kind() {
            MeridianTransitKind::Upper => (SkyFactKind::UpperTransit, Some(event.transit_model())),
            MeridianTransitKind::Lower => (SkyFactKind::LowerTransit, Some(event.transit_model())),
        },
    };
    SkyReceiptFact {
        body: item.body(),
        kind,
        span: item.span(),
        calculation_model,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::convert::Infallible;

    use super::*;
    use crate::sky_receipt::{
        SKY_RECEIPT_VERSION, SkyBodyV1, SkyFactKindV1, SkyLimbV1, SkyReceiptV1,
    };
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
            receipt.facts.iter().any(|fact| {
                fact.body == SkyBodyV1::Moon && fact.kind == SkyFactKindV1::NewMoon
            })
        );
        assert_eq!(receipt.to_pretty_json().unwrap(), bytes);
        assert_eq!(SkyReceiptV1::from_json(&bytes).unwrap(), receipt);
        assert_eq!(
            receipt.blake3_hex_digest().unwrap(),
            "caff8371d348ba141397a8185e291c533c6ab12d4fe85f9ce3be797707ce411d"
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
}
