// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The "Use the model" toggle, its status line and the rule choice.

use super::GoalSurface;
use super::view::Child;
use super::view_model::rule_label;
use cambium::{button, el};
use eidetic_search::goal_model::ModelAssessRule;

const RULES: [(ModelAssessRule, &str); 3] = [
    (ModelAssessRule::Or, "goal-rule-or"),
    (ModelAssessRule::ModelReplaces, "goal-rule-model"),
    (ModelAssessRule::RecordOnly, "goal-rule-record"),
];

pub fn controls(app: &GoalSurface) -> Child {
    let panel = &app.model;
    let off = panel.disabled_reason().is_some();
    let mut toggle = button("Use the model", |s: &mut GoalSurface, _| s.model.toggle())
        .attr("id", "goal-model-toggle")
        .attr("aria-pressed", (panel.enabled && !off).to_string());
    if off {
        toggle = toggle.attr("disabled", "").attr("aria-disabled", "true").attr("aria-describedby", "goal-model-status");
    }
    let mut row: Vec<Child> = vec![Box::new(toggle)];
    if panel.enabled && !off {
        for (rule, id) in RULES {
            row.push(Box::new(
                button(rule_label(rule), move |s: &mut GoalSurface, _| s.model.rule = rule)
                    .attr("id", id)
                    .attr("aria-pressed", (panel.rule == rule).to_string()),
            ));
        }
    }
    let status = el("p", panel.status()).attr("id", "goal-model-status").attr("class", "goal-meta").attr("role", "status");
    Box::new(el("div", (el("div", row).attr("class", "goal-starts").attr("aria-label", "Model"), status)).attr("class", "goal-model"))
}
