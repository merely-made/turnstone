// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! View-local responsive path controls over the ordinary Graphshell canvas.
//!
//! The selected graph field and compatible-link group are references to current
//! graph identities. The analytic presets below are explicitly temporary view
//! values. Neither paths nor response settings are source truth.

use super::*;
use cambium::button;
use mere::canvas::responsive_link_adapter::ResponsivePathConfig;
use mere::kernel::graph::{FieldDefinition, FieldExtent, FieldId, ScalarField, VectorField};
use scenograph::link::LinkKey;

use mere::canvas::responsive_links::{LinkPathResponse, LinkPathResponseReport, LinkPathState};

const MIN_FIELD_STRENGTH: f32 = 1.0;
const MAX_FIELD_STRENGTH: f32 = 10_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LocalPreset {
    Attractor,
    Flow,
    Vortex,
}

impl LocalPreset {
    fn label(self) -> &'static str {
        match self {
            Self::Attractor => "Temporary attractor",
            Self::Flow => "Temporary flow",
            Self::Vortex => "Temporary vortex",
        }
    }

    fn field(self, center: (f32, f32)) -> VectorField {
        match self {
            Self::Attractor => {
                VectorField::gradient_of(ScalarField::gaussian_at(center.0, center.1, 260.0))
            },
            Self::Flow => VectorField::const_vec(0.0, -1.0),
            Self::Vortex => VectorField::Perp(Box::new(VectorField::gradient_of(
                ScalarField::gaussian_at(center.0, center.1, 260.0),
            ))),
        }
    }

    fn strength(self) -> f32 {
        match self {
            Self::Attractor | Self::Vortex => MAX_FIELD_STRENGTH,
            Self::Flow => 20.0,
        }
    }
}

fn configure_response(response: &mut LinkPathResponse) {
    response.config = ResponsivePathConfig {
        control_points: 17,
        iterations: 24,
        field_gain: 0.08,
        compatibility_gain: 0.08,
        smoothing: 0.08,
        max_step: 4.0,
        ..ResponsivePathConfig::default()
    };
}

fn graph_vector_fields(page: &TreePage) -> Result<Vec<(FieldId, String, String, bool)>, String> {
    let graph = page
        .product
        .as_ref()
        .ok_or_else(|| "Responsive paths need the local source graph".to_owned())?
        .source_graph()?;
    Ok(graph
        .fields()
        .map(|field| {
            let (kind, eligible) = match &field.definition {
                FieldDefinition::Vector(_) if !field.is_active() => ("retired", false),
                FieldDefinition::Vector(_) if !matches!(field.extent, FieldExtent::Global) => {
                    ("unsupported extent", false)
                },
                FieldDefinition::Vector(_) => ("active global vector", true),
                FieldDefinition::Scalar(_) => ("scalar field", false),
            };
            (
                field.id,
                field
                    .name
                    .clone()
                    .unwrap_or_else(|| field.id.as_uuid().to_string()),
                kind.to_owned(),
                eligible,
            )
        })
        .collect())
}

fn current(page: &TreePage) -> LinkPathResponse {
    page.shared.canvas.borrow().link_path_response().clone()
}

fn write(page: &mut TreePage, response: LinkPathResponse) {
    page.shared
        .canvas
        .borrow_mut()
        .set_link_path_response(response);
    page.shared.dirty.set(true);
}

/// Apply one local field preset centered on the current view center.
pub(super) fn set_local_preset(page: &mut TreePage, preset: LocalPreset) {
    let size = page.shared.size.get();
    let center = page
        .shared
        .canvas
        .borrow()
        .world_point_at((size.0 as f32 * 0.5, size.1 as f32 * 0.5));
    let mut response = current(page);
    response.selected_fields.clear();
    response.view_field = Some(preset.field(center).into());
    response.field_strength = preset.strength();
    configure_response(&mut response);
    response.field_response = true;
    write(page, response);
}

pub(super) fn choose_graph_field(page: &mut TreePage, id: FieldId) {
    let mut response = current(page);
    response.selected_fields.clear();
    response.selected_fields.push(id);
    response.view_field = None;
    response.field_strength = 20.0;
    configure_response(&mut response);
    response.field_response = true;
    write(page, response);
}

pub(super) fn toggle_field_response(page: &mut TreePage) {
    let mut response = current(page);
    response.field_response = !response.field_response;
    write(page, response);
}

pub(super) fn toggle_compatible_attraction(page: &mut TreePage) {
    let mut response = current(page);
    response.compatible_attraction = !response.compatible_attraction;
    write(page, response);
}

pub(super) fn adjust_field_strength(page: &mut TreePage, increase: bool) {
    let mut response = current(page);
    response.field_strength = if increase {
        (response.field_strength * 2.0).min(MAX_FIELD_STRENGTH)
    } else {
        (response.field_strength * 0.5).max(MIN_FIELD_STRENGTH)
    };
    write(page, response);
}

/// Add or remove the one currently selected canonical LinkKey in the explicit
/// compatibility group. No predicate or relationship inference is performed.
pub(super) fn toggle_selected_compatible_link(page: &mut TreePage) -> Result<(), String> {
    let selected = page.shared.canvas.borrow().selected_link_keys();
    if selected.len() != 1 {
        return Err("Select exactly one canonical link appearance first".into());
    }
    let key = selected[0].clone();
    let mut response = current(page);
    if let Some(index) = response
        .compatible_links
        .iter()
        .position(|item| item == &key)
    {
        response.compatible_links.remove(index);
    } else {
        response.compatible_links.push(key);
    }
    write(page, response);
    Ok(())
}

pub(super) fn clear_compatible_group(page: &mut TreePage) {
    let mut response = current(page);
    response.compatible_links.clear();
    write(page, response);
}

fn configured_field(response: &LinkPathResponse) -> String {
    if let Some(field) = &response.view_field {
        let label = match field.as_ref() {
            VectorField::ConstVec { .. } => "Temporary flow",
            VectorField::Gradient(scalar)
                if matches!(scalar.as_ref(), ScalarField::Gaussian { .. }) =>
            {
                "Temporary attractor"
            },
            VectorField::Perp(inner) if matches!(inner.as_ref(), VectorField::Gradient(scalar) if matches!(scalar.as_ref(), ScalarField::Gaussian { .. })) => {
                "Temporary vortex"
            },
            _ => "Temporary custom vector field",
        };
        return label.into();
    }
    if response.selected_fields.is_empty() {
        "None".into()
    } else {
        response
            .selected_fields
            .iter()
            .map(|id| id.as_uuid().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn status(report: &LinkPathResponseReport) -> String {
    match report.state {
        LinkPathState::Disabled => "Responses off".into(),
        LinkPathState::Applied if report.responsive_paths > 0 => {
            if report.presentation_fallbacks > 0 {
                format!(
                    "Curving {} appearances · {} kept at rest",
                    report.responsive_paths, report.presentation_fallbacks
                )
            } else {
                format!("Curving {} appearances", report.responsive_paths)
            }
        },
        LinkPathState::Applied if report.presentation_fallbacks > 0 => {
            "Responses on · paths kept at rest because they cannot attach clear of node faces"
                .into()
        },
        LinkPathState::Applied => "Responses on · no visible bending".into(),
        LinkPathState::Refused => format!(
            "Cannot apply: {}",
            report
                .failure
                .as_ref()
                .map(failure_message)
                .unwrap_or("the current graph or field is unsupported")
        ),
    }
}

fn failure_message(failure: &mere::canvas::responsive_links::LinkPathFailure) -> &'static str {
    use mere::canvas::responsive_links::LinkPathFailure;
    match failure {
        LinkPathFailure::MissingField(_) => "the selected graph field is unavailable",
        LinkPathFailure::RetiredField(_) => "the selected graph field is retired",
        LinkPathFailure::ScalarField(_) => "the selected graph field is scalar, not vector",
        LinkPathFailure::UnsupportedExtent(_) => {
            "the selected graph field has an unsupported extent"
        },
        LinkPathFailure::StaleCompatibleLink(_) => "a grouped link is no longer in this graph",
        LinkPathFailure::InvalidFieldStrength => "field strength is outside the allowed range",
        LinkPathFailure::FieldDependencyTooLarge => "the field expression exceeds the path limits",
        LinkPathFailure::FieldSampleKindMismatch(_) => "a sampled field has the wrong value type",
        LinkPathFailure::Adapter(_) => "the bounded path evaluator refused these inputs",
        LinkPathFailure::HiddenEndpoint => "a path endpoint is hidden",
    }
}

pub(super) fn section(page: &TreePage) -> Child {
    if page.product.is_none() {
        return Box::new(el("div", ()));
    }
    let response = current(page);
    let report = page.shared.canvas.borrow().link_path_response_report();
    let paths = &report.paths;
    let mut children: Vec<Child> = vec![
        Box::new(el("h3", "Responsive link paths")),
        Box::new(el(
            "p",
            "Temporary paths bend alongside ordinary graph behavior; they do not move or save nodes. Settings clear when this view closes.",
        )),
        Box::new(el(
            "p",
            "Surface paths are gold; Resource paths are blue. Arrows show direction, cross ticks show opposition, and selection adds a halo.",
        )),
        Box::new(
            button(
                if response.field_response {
                    "Disable field response"
                } else {
                    "Enable field response"
                },
                |page: &mut TreePage, _| toggle_field_response(page),
            )
            .attr("aria-pressed", response.field_response.to_string())
            .attr("data-link-path-control", "field-response"),
        ),
        Box::new(
            button(
                if response.compatible_attraction {
                    "Disable compatible-link attraction"
                } else {
                    "Enable compatible-link attraction"
                },
                |page: &mut TreePage, _| toggle_compatible_attraction(page),
            )
            .attr("aria-pressed", response.compatible_attraction.to_string())
            .attr("data-link-path-control", "compatible-attraction"),
        ),
        Box::new(el("h4", "Temporary local field")),
    ];
    children.push(Box::new(
        button("Weaker field", |page: &mut TreePage, _| {
            adjust_field_strength(page, false)
        })
        .attr("data-link-path-control", "weaker-field"),
    ));
    children.push(Box::new(
        button("Stronger field", |page: &mut TreePage, _| {
            adjust_field_strength(page, true)
        })
        .attr("data-link-path-control", "stronger-field"),
    ));
    for preset in [
        LocalPreset::Attractor,
        LocalPreset::Flow,
        LocalPreset::Vortex,
    ] {
        children.push(Box::new(
            button(preset.label(), move |page: &mut TreePage, _| {
                set_local_preset(page, preset)
            })
            .attr(
                "data-link-path-preset",
                format!("{preset:?}").to_lowercase(),
            ),
        ));
    }

    children.push(Box::new(el("h4", "Active graph vector fields")));
    match graph_vector_fields(page) {
        Err(error) => children.push(Box::new(el("p", error).attr("role", "status"))),
        Ok(fields) if fields.is_empty() => children.push(Box::new(el(
            "p",
            "No vector fields are held in this graph. Choose a labeled temporary local field above.",
        ))),
        Ok(fields) => {
            for (id, label, reason, eligible) in fields {
                let active = response.selected_fields.contains(&id) && response.view_field.is_none();
                let control = button(
                    format!("Use graph field · {label}"),
                    move |page: &mut TreePage, _| choose_graph_field(page, id),
                )
                .attr("aria-pressed", active.to_string())
                .attr("data-link-path-field", id.as_uuid().to_string());
                children.push(Box::new(if eligible { control } else { control.attr("disabled", "") }));
                children.push(Box::new(el("p", format!("{} · {}", id.as_uuid(), reason))));
            }
        },
    }

    children.push(Box::new(el("h4", "This view’s compatible links")));
    let selected = page.shared.canvas.borrow().selected_link_keys();
    let can_toggle = selected.len() == 1;
    let label = if can_toggle {
        let is_member = response.compatible_links.contains(&selected[0]);
        if is_member {
            "Remove selected link from this view's group"
        } else {
            "Add selected link to this view's group"
        }
    } else {
        "Select a link, then add it to this view’s group"
    };
    let action = button(label, |page: &mut TreePage, _| {
        let _ = toggle_selected_compatible_link(page);
    });
    children.push(Box::new(if can_toggle {
        action
    } else {
        action.attr("disabled", "")
    }));
    children.push(Box::new(button(
        "Clear compatible-link group",
        |page: &mut TreePage, _| clear_compatible_group(page),
    )));
    if response.compatible_links.is_empty() {
        children.push(Box::new(el(
            "p",
            "No links are grouped yet. Add each link you want to attract together.",
        )));
    } else {
        children.push(Box::new(el(
            "p",
            format!(
                "{} link(s) explicitly grouped for this view.",
                response.compatible_links.len()
            ),
        )));
    }
    children.push(Box::new(
        el("p", status(&report))
            .attr("role", "status")
            .attr("data-link-path-status", ""),
    ));
    children.push(Box::new(el(
        "p",
        format!(
            "Field: {} · strength: {} · field response {} · compatible attraction {} · group: {} link(s) · maximum visible bend: {:.1} units.",
            configured_field(&response),
            response.field_strength,
            response.field_response,
            response.compatible_attraction,
            response.compatible_links.len(),
            max_bend(&paths),
        ),
    )));
    Box::new(el("section", children).attr("aria-label", "Responsive link path controls"))
}

mod observation;

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    observation::fields(page, snapshot)
}

fn max_bend(paths: &[mere::canvas::responsive_links::EvaluatedLinkPath]) -> f32 {
    paths
        .iter()
        .map(|path| path_bend(&path.points))
        .fold(0.0_f32, f32::max)
}

fn path_bend(points: &[(f32, f32)]) -> f32 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return 0.0;
    };
    let dx = last.0 - first.0;
    let dy = last.1 - first.1;
    let length = dx.hypot(dy).max(f32::EPSILON);
    points
        .iter()
        .map(|point| ((dx * (first.1 - point.1) - (first.0 - point.0) * dy).abs()) / length)
        .fold(0.0_f32, f32::max)
}
