// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use cambium::{
    AnyView, GenetCtx, GenetElement, PointerClick, SurfaceViewport, TextInput, clickable, el, lens,
    text, text_field_typed,
};
use jiff::civil::Date;

use super::{
    SkyPaneBodySourceV1, SkyPaneBodyV1, SkyPaneProjection, SkyPaneSourceV1, SkyRiseSetPresetV1,
    calculate_source,
};

const SKY_CALCULATION_STACK_BYTES: usize = 4 * 1024 * 1024;

fn calculate_for_surface(source: &SkyPaneSourceV1) -> Result<SkyPaneProjection, String> {
    let source = source.clone();
    std::thread::Builder::new()
        .name("turnstone-sky-calculation".into())
        .stack_size(SKY_CALCULATION_STACK_BYTES)
        .spawn(move || calculate_source(&source))
        .map_err(|error| format!("could not start Sky calculation: {error}"))?
        .join()
        .map_err(|_| "Sky calculation worker panicked".to_owned())?
}

pub(super) struct SkySurfaceState {
    applied: SkyPaneSourceV1,
    projection: SkyPaneProjection,
    observer_label: TextInput,
    date: TextInput,
    time_zone: TextInput,
    longitude: TextInput,
    latitude: TextInput,
    height: TextInput,
    status: String,
    status_is_error: bool,
    viewport_w: f32,
    viewport_h: f32,
}

impl SkySurfaceState {
    pub(super) fn new(source: SkyPaneSourceV1) -> Result<Self, String> {
        let projection = calculate_for_surface(&source)?;
        let mut state = Self {
            applied: source,
            projection,
            observer_label: TextInput::default(),
            date: TextInput::default(),
            time_zone: TextInput::default(),
            longitude: TextInput::default(),
            latitude: TextInput::default(),
            height: TextInput::default(),
            status: String::new(),
            status_is_error: false,
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        state.sync_draft();
        state.status = state.calculated_status();
        Ok(state)
    }

    pub(super) fn set_viewport(&mut self, viewport: SurfaceViewport) {
        self.viewport_w = viewport.width;
        self.viewport_h = viewport.height;
    }

    fn sync_draft(&mut self) {
        self.observer_label = TextInput::new(self.applied.observer.label.clone());
        self.date = TextInput::new(self.applied.date.clone());
        self.time_zone = TextInput::new(self.applied.time_zone.clone());
        self.longitude = TextInput::new(self.applied.observer.longitude_east_degrees.to_string());
        self.latitude = TextInput::new(self.applied.observer.latitude_degrees.to_string());
        self.height = TextInput::new(
            self.applied
                .observer
                .height_meters_above_wgs84_ellipsoid
                .to_string(),
        );
    }

    fn candidate_from_draft(&self) -> Result<SkyPaneSourceV1, String> {
        let mut candidate = self.applied.clone();
        candidate.observer.label = self.observer_label.text().trim().to_owned();
        candidate.date = self.date.text().trim().to_owned();
        candidate.time_zone = self.time_zone.text().trim().to_owned();
        candidate.observer.longitude_east_degrees =
            parse_number("East longitude", self.longitude.text())?;
        candidate.observer.latitude_degrees = parse_number("Latitude", self.latitude.text())?;
        candidate.observer.height_meters_above_wgs84_ellipsoid =
            parse_number("WGS84 ellipsoid height", self.height.text())?;
        Ok(candidate)
    }

    fn apply(&mut self, candidate: SkyPaneSourceV1) {
        match calculate_for_surface(&candidate) {
            Ok(projection) => {
                self.applied = candidate;
                self.projection = projection;
                self.sync_draft();
                self.status = self.calculated_status();
                self.status_is_error = false;
            }
            Err(error) => {
                self.status = format!("Sky calculation was not applied: {error}");
                self.status_is_error = true;
            }
        }
    }

    fn calculated_status(&self) -> String {
        let receipt = format!("Calculated receipt {}", self.projection.digest);
        if self.applied.date == self.applied.earth_orientation.reference_date {
            receipt
        } else {
            format!(
                "{receipt}. Fixed Earth-orientation input remains referenced to {}.",
                self.applied.earth_orientation.reference_date
            )
        }
    }
}

fn parse_number(field: &str, input: &str) -> Result<f64, String> {
    input
        .trim()
        .parse::<f64>()
        .map_err(|error| format!("{field} must be a number: {error}"))
}

type SkyView = Box<dyn AnyView<SkySurfaceState, (), GenetCtx, GenetElement>>;

fn field(
    getter: fn(&mut SkySurfaceState) -> &mut TextInput,
    label: &'static str,
    field_name: &'static str,
) -> SkyView {
    let input = Box::new(lens(
        move |input: &mut TextInput| text_field_typed(input),
        getter,
    )) as SkyView;
    Box::new(
        el::<_, SkySurfaceState, ()>(
            "label",
            (
                el::<_, SkySurfaceState, ()>("span", label).attr("class", "setting-label"),
                input,
            ),
        )
        .attr("class", "setting-row")
        .attr("data-sky-field", field_name),
    )
}

fn calculate(state: &mut SkySurfaceState, _: PointerClick) {
    let Some(candidate) = draft_candidate(state) else {
        return;
    };
    state.apply(candidate);
}

fn draft_candidate(state: &mut SkySurfaceState) -> Option<SkyPaneSourceV1> {
    match state.candidate_from_draft() {
        Ok(candidate) => Some(candidate),
        Err(error) => {
            state.status = format!("Sky calculation was not applied: {error}");
            state.status_is_error = true;
            None
        }
    }
}

fn previous_day(state: &mut SkySurfaceState, _: PointerClick) {
    let Some(mut candidate) = draft_candidate(state) else {
        return;
    };
    let result = candidate
        .date
        .parse::<Date>()
        .map_err(|error| error.to_string())
        .and_then(|date| date.yesterday().map_err(|error| error.to_string()));
    match result {
        Ok(date) => {
            candidate.date = date.to_string();
            state.apply(candidate);
        }
        Err(error) => {
            state.status = format!("Sky calculation was not applied: {error}");
            state.status_is_error = true;
        }
    }
}

fn next_day(state: &mut SkySurfaceState, _: PointerClick) {
    let Some(mut candidate) = draft_candidate(state) else {
        return;
    };
    let result = candidate
        .date
        .parse::<Date>()
        .map_err(|error| error.to_string())
        .and_then(|date| date.tomorrow().map_err(|error| error.to_string()));
    match result {
        Ok(date) => {
            candidate.date = date.to_string();
            state.apply(candidate);
        }
        Err(error) => {
            state.status = format!("Sky calculation was not applied: {error}");
            state.status_is_error = true;
        }
    }
}

fn toggle_sun(state: &mut SkySurfaceState, click: PointerClick) {
    toggle_body(state, SkyPaneBodyV1::Sun, click);
}

fn toggle_moon(state: &mut SkySurfaceState, click: PointerClick) {
    toggle_body(state, SkyPaneBodyV1::Moon, click);
}

fn toggle_body(state: &mut SkySurfaceState, body: SkyPaneBodyV1, _: PointerClick) {
    let Some(mut candidate) = draft_candidate(state) else {
        return;
    };
    if let Some(index) = candidate.bodies.iter().position(|item| item.body == body) {
        if candidate.bodies.len() == 1 {
            state.status = "Sky calculation was not applied: select at least one body".into();
            state.status_is_error = true;
            return;
        }
        candidate.bodies.remove(index);
    } else {
        candidate.bodies.push(SkyPaneBodySourceV1 {
            body,
            rise_set: SkyRiseSetPresetV1::ConventionalUpperLimb,
        });
        candidate.bodies.sort_by_key(|item| match item.body {
            SkyPaneBodyV1::Sun => 0,
            SkyPaneBodyV1::Moon => 1,
        });
    }
    state.apply(candidate);
}

fn toggle_sun_policy(state: &mut SkySurfaceState, click: PointerClick) {
    toggle_policy(state, SkyPaneBodyV1::Sun, click);
}

fn toggle_moon_policy(state: &mut SkySurfaceState, click: PointerClick) {
    toggle_policy(state, SkyPaneBodyV1::Moon, click);
}

fn toggle_policy(state: &mut SkySurfaceState, body: SkyPaneBodyV1, _: PointerClick) {
    let Some(mut candidate) = draft_candidate(state) else {
        return;
    };
    let Some(selected) = candidate.bodies.iter_mut().find(|item| item.body == body) else {
        state.status = format!(
            "Sky calculation was not applied: select {} before changing its policy",
            body.label()
        );
        state.status_is_error = true;
        return;
    };
    selected.rise_set = match selected.rise_set {
        SkyRiseSetPresetV1::ConventionalUpperLimb => SkyRiseSetPresetV1::GeometricCenter,
        SkyRiseSetPresetV1::GeometricCenter => SkyRiseSetPresetV1::ConventionalUpperLimb,
    };
    state.apply(candidate);
}

fn button(label: impl Into<String>, action: fn(&mut SkySurfaceState, PointerClick)) -> SkyView {
    Box::new(clickable(
        el::<_, SkySurfaceState, ()>("button", text(label.into())).attr("class", "setting-apply"),
        action,
    ))
}

fn body_controls(state: &SkySurfaceState, body: SkyPaneBodyV1) -> SkyView {
    let selected = state.applied.bodies.iter().find(|item| item.body == body);
    let selection_label = if selected.is_some() {
        format!("Remove {}", body.label())
    } else {
        format!("Add {}", body.label())
    };
    let toggle = match body {
        SkyPaneBodyV1::Sun => toggle_sun,
        SkyPaneBodyV1::Moon => toggle_moon,
    };
    let policy = selected
        .map(|item| item.rise_set.label())
        .unwrap_or("not selected");
    let policy_button = selected.map(|item| {
        let next = match item.rise_set {
            SkyRiseSetPresetV1::ConventionalUpperLimb => "geometric center",
            SkyRiseSetPresetV1::GeometricCenter => "conventional upper limb",
        };
        button(
            format!("Use {next} for {}", body.label()),
            match body {
                SkyPaneBodyV1::Sun => toggle_sun_policy,
                SkyPaneBodyV1::Moon => toggle_moon_policy,
            },
        )
    });
    let mut children = vec![
        Box::new(
            el::<_, SkySurfaceState, ()>("div", format!("{} rise/set: {policy}", body.label()))
                .attr("class", "list-row"),
        ) as SkyView,
        button(selection_label, toggle),
    ];
    if let Some(policy_button) = policy_button {
        children.push(policy_button);
    }
    Box::new(el::<_, SkySurfaceState, ()>("div", children).attr("class", "setting-row"))
}

fn timeline_rows(state: &SkySurfaceState) -> Vec<SkyView> {
    state
        .projection
        .rows
        .iter()
        .map(|row| {
            Box::new(
                el::<_, SkySurfaceState, ()>(
                    "div",
                    (
                        el::<_, SkySurfaceState, ()>("div", row.local_label.clone())
                            .attr("class", "list-row muted"),
                        el::<_, SkySurfaceState, ()>("div", row.summary.clone()),
                        el::<_, SkySurfaceState, ()>("div", row.interval_label.clone())
                            .attr("class", "list-row muted"),
                    ),
                )
                .attr("class", "list-row sky-fact")
                .attr("role", "listitem"),
            ) as SkyView
        })
        .collect()
}

pub(super) fn sky_surface_view(state: &SkySurfaceState) -> SkyView {
    let controls = vec![
        field(
            |state| &mut state.observer_label,
            "Observer label",
            "observer-label",
        ),
        field(|state| &mut state.date, "Civil date", "date"),
        field(|state| &mut state.time_zone, "IANA time zone", "time-zone"),
        field(
            |state| &mut state.longitude,
            "East longitude (degrees)",
            "longitude",
        ),
        field(
            |state| &mut state.latitude,
            "Latitude (degrees)",
            "latitude",
        ),
        field(
            |state| &mut state.height,
            "Height above WGS84 ellipsoid (meters)",
            "height",
        ),
        body_controls(state, SkyPaneBodyV1::Sun),
        body_controls(state, SkyPaneBodyV1::Moon),
    ];
    let policies = state
        .applied
        .bodies
        .iter()
        .map(|selected| format!("{}: {}", selected.body.label(), selected.rise_set.label()))
        .collect::<Vec<_>>()
        .join("; ");
    let empty_rows: Vec<SkyView> = state
        .projection
        .empty_rise_set
        .iter()
        .map(|message| {
            Box::new(
                el::<_, SkySurfaceState, ()>("div", message.clone())
                    .attr("class", "list-row muted")
                    .attr("role", "status"),
            ) as SkyView
        })
        .collect();
    let children: Vec<SkyView> = vec![
        Box::new(el::<_, SkySurfaceState, ()>("div", "Sky").attr("class", "list-section-title")),
        Box::new(
            el::<_, SkySurfaceState, ()>(
                "div",
                "Reference source; edits are retained only for this pane's current lifetime.",
            )
            .attr("class", "list-row muted"),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>(
                "div",
                format!(
                    "Applied: {} in {} at {}",
                    state.applied.date, state.applied.time_zone, state.applied.observer.label
                ),
            )
            .attr("class", "list-row"),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>(
                "div",
                format!(
                    "Observer: {:.6} deg east, {:.6} deg north, {:.1} m above WGS84",
                    state.applied.observer.longitude_east_degrees,
                    state.applied.observer.latitude_degrees,
                    state.applied.observer.height_meters_above_wgs84_ellipsoid
                ),
            )
            .attr("class", "list-row"),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>(
                "div",
                format!(
                    "Fixed Earth orientation input for {}: {} · {} · DUT1 {:.3} s",
                    state.applied.earth_orientation.reference_date,
                    state.applied.earth_orientation.authority,
                    state.applied.earth_orientation.snapshot,
                    state.applied.earth_orientation.dut1_seconds
                ),
            )
            .attr("class", "list-row"),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>("div", format!("Rise/set policies: {policies}"))
                .attr("class", "list-row"),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>(
                "div",
                vec![
                    button("Previous day", previous_day),
                    button("Next day", next_day),
                ],
            )
            .attr("class", "setting-row"),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>("div", state.status.clone())
                .attr(
                    "class",
                    if state.status_is_error {
                        "sky-alert"
                    } else {
                        "list-row muted"
                    },
                )
                .attr(
                    "role",
                    if state.status_is_error {
                        "alert"
                    } else {
                        "status"
                    },
                ),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>("div", format!("Receipt {}", state.projection.digest))
                .attr("class", "list-row muted sky-receipt-digest"),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>(
                "div",
                format!(
                    "Turquet {} · {} · {} facts",
                    state.projection.receipt.engine.version,
                    state.projection.receipt.engine.source_revision,
                    state.projection.receipt.facts.len()
                ),
            )
            .attr("class", "list-row muted"),
        ),
        Box::new(
            el::<_, SkySurfaceState, ()>("div", "Daily timeline")
                .attr("class", "list-section-title"),
        ),
        Box::new(el::<_, SkySurfaceState, ()>("div", empty_rows)),
        Box::new(el::<_, SkySurfaceState, ()>("div", timeline_rows(state)).attr("role", "list")),
        Box::new(
            el::<_, SkySurfaceState, ()>("div", "Observer and policy settings")
                .attr("class", "list-section-title"),
        ),
        Box::new(el::<_, SkySurfaceState, ()>("div", controls).attr("class", "sky-controls")),
        button("Calculate", calculate),
    ];
    Box::new(
        el::<_, SkySurfaceState, ()>("div", children)
            .attr("class", "pane")
            .attr(
                "style",
                format!(
                    "width: {}px; height: {}px;",
                    state.viewport_w, state.viewport_h
                ),
            ),
    )
}
