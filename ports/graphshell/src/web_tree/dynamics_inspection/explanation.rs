// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! User wording over the shared inspector's existing readbacks.
use super::*;

pub(super) fn summary(report: &DynamicsInspection) -> Vec<Child> {
    let mut lines = Vec::new();
    if let Some(pair) = &report.pair_inputs {
        lines.push(format!(
            "Relationships: {} held rows; {} currently contribute to topology; {} hidden in this view. Inspect an input assertion below for its source, basis and lifecycle.",
            pair.held_rows, pair.visible_rows, pair.held_rows.saturating_sub(pair.visible_rows),
        ));
    }
    if let Some(paths) = &report.path_response {
        let appearances = paths["paths"].as_array().map_or(0, Vec::len);
        let curved = paths["paths"].as_array().map_or(0, |paths| {
            paths.iter().filter(|p| p["responsive"] == true).count()
        });
        let state = paths["state"].as_str().unwrap_or("unavailable");
        let bend = paths["paths"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["max_bend"].as_f64())
            .fold(0.0_f64, f64::max);
        lines.push(format!("Drawn paths: {state}; {appearances} visible appearances, {curved} evaluated with responses. Field response is {}; explicit compatible-link attraction is {}. Maximum bend is {bend:.2} world units. These curves change the drawing; endpoint motion has its own physics.", on_off(&paths["field_response"]), on_off(&paths["compatible_attraction"])));
        if let Some(failure) = paths["failure"].as_str() {
            lines.push(format!("Path response refused: {failure}. The details disclose the cohort fallback and exclusion counts."));
        }
    }
    let held = report
        .members
        .iter()
        .filter(|m| {
            m.explicit_position_pin || m.arrangement_pin_installed || m.forme_hold || m.pointer_drag
        })
        .count();
    lines.push(format!("Endpoint physics: {} · {}. {held} inspected endpoints have a pin, hold or drag constraint. Topology rows are inputs, not measured forces.", if report.paused { "paused" } else { "running" }, report.law_label));
    for member in &report.members {
        let mut constraints = Vec::new();
        if member.explicit_position_pin {
            constraints.push("explicit position pin");
        }
        if member.arrangement_pin_installed {
            constraints.push("arrangement pin");
        }
        if member.forme_hold {
            constraints.push("Forme hold");
        }
        if member.pointer_drag {
            constraints.push("pointer drag");
        }
        if member.anchor_target_configured {
            constraints.push("arrangement anchor");
        }
        if !constraints.is_empty() {
            lines.push(format!("{}: {}.", member.title, constraints.join(", ")));
        }
    }
    let observation = &report.coupling_observation;
    let trace = &observation["trace"];
    if trace.is_null() {
        lines.push("Field effects: no tick observed for the current bindings yet. Opening this inspector does not advance physics.".into());
    } else {
        let rows = trace["rows"].as_array().map_or(0, Vec::len);
        lines.push(format!("Field effects: {rows} matching endpoint calls recorded at step {}, simulation time {}. This is the latest recorded tick before integration, with {} cohort rows omitted by the capture limit. It does not explain net motion or assign a force to a relationship.", trace["step"], trace["sample_time"], trace["cohort_omitted"]));
    }
    lines
        .into_iter()
        .map(|line| Box::new(el("p", line)) as Child)
        .collect()
}

pub(super) fn details(report: &DynamicsInspection) -> Vec<Child> {
    let mut children: Vec<Child> = Vec::new();
    for (label, value) in [
        (
            "Drawn path inputs and geometry",
            report.path_response.as_ref(),
        ),
        (
            "Recorded endpoint field effects",
            Some(&report.coupling_observation),
        ),
    ] {
        if let Some(value) = value {
            children.push(Box::new(el("h3", label)));
            children.push(Box::new(el(
                "pre",
                serde_json::to_string_pretty(value).unwrap(),
            )));
        }
    }
    children
}

fn on_off(value: &serde_json::Value) -> &'static str {
    if value == true { "on" } else { "off" }
}
