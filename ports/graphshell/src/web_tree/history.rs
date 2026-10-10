// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A host history's checkpoints (mer3ly site canvas plan, S3; the sandbox's
//! checkpoint slider, in function).
//!
//! The page shows one revision of the history at a time. A slider and two
//! buttons step between checkpoints, the slider's text value names the one
//! shown, and the graph is rebuilt from it. Each node's title carries its
//! change's mark, and a "Changes" list tells every change, removed items
//! included, for a reader.
use super::*;
use cambium::{PointerPhase, button};
use graphshell::host_dataset_view::HostDatasetView;
use graphshell::host_history_view::{
    Change, ViewedChanges, change_mark, change_word, checkpoint_text, host_history_view,
};
use graphshell::projection_compile::HostDatasetV2;

/// A host history, and the checkpoint the graph shows.
pub(crate) struct HostedHistory {
    history: HostDatasetV2,
    /// The shown checkpoint, from 0.
    pub(super) index: usize,
    pub(super) changes: Option<ViewedChanges>,
    /// Why the last step was refused, if it was.
    pub(super) error: Option<String>,
}

impl HostedHistory {
    /// The history at its current (last) checkpoint, and that checkpoint's
    /// view.
    pub(super) fn open(history: HostDatasetV2) -> Result<(Self, HostDatasetView), String> {
        let index = history.revisions.len().saturating_sub(1);
        let shown = host_history_view(&history, index)?;
        let hosted = Self {
            history,
            index,
            changes: shown.changes,
            error: None,
        };
        Ok((hosted, shown.view))
    }

    pub(super) fn len(&self) -> usize {
        self.history.revisions.len()
    }

    /// The shown checkpoint's text value.
    pub(super) fn checkpoint(&self) -> String {
        checkpoint_text(&self.history, self.index)
    }

    /// The counts with their marks, as the page shows them.
    pub(super) fn summary(&self) -> Option<String> {
        let changes = self.changes.as_ref()?;
        Some(
            [
                Change::Added,
                Change::Updated,
                Change::Stable,
                Change::Removed,
            ]
            .into_iter()
            .map(|change| {
                format!(
                    "{} {} {}",
                    change_mark(change),
                    changes.count(change),
                    change_word(change)
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
        )
    }
}

impl TreePage {
    /// Show checkpoint `index`: rebuild the graph from that revision, keep
    /// the camera and the selection that still exists, and keep a chosen
    /// arrangement.
    pub(super) fn show_checkpoint(&mut self, index: usize) {
        let Some(hosted) = &mut self.history else {
            return;
        };
        let index = index.min(hosted.len().saturating_sub(1));
        if index == hosted.index {
            return;
        }
        let shown = match host_history_view(&hosted.history, index) {
            Ok(shown) => shown,
            Err(error) => {
                hosted.error = Some(format!("Checkpoint {} refused: {error}", index + 1));
                return;
            },
        };
        let view = shown.view;
        {
            let mut canvas = self.shared.canvas.borrow_mut();
            let selected = canvas.selected_members();
            canvas.set_graph(view.graph);
            canvas.set_layout_strategy(Some(web_graphs::LAYOUT.to_string()));
            canvas.apply_strategy_positions(&view.positions);
            canvas.set_selected_members(&selected);
            self.nodes = canvas.graph().node_count();
            self.picked = canvas.focused_url().map(str::to_owned);
        }
        // A transition's slots name the old graph's nodes.
        self.physics.transition = None;
        self.source = format!("host dataset, revision {}", view.revision);
        hosted.index = index;
        hosted.changes = shown.changes;
        hosted.error = None;
        self.dataset = HostedDataset::Loaded(view.relations);
        if self.physics.layout_id != web_graphs::LAYOUT {
            self.reapply_arrangement();
        }
        self.shared.dirty.set(true);
    }

    /// Step the shown checkpoint by `delta`, held to the history's ends.
    fn step_checkpoint(&mut self, delta: isize) {
        if let Some(hosted) = &self.history {
            self.show_checkpoint(hosted.index.saturating_add_signed(delta));
        }
    }
}

/// The checkpoint slider: arrows and Page keys step, Home and End go to the
/// ends, and a press or drag picks the checkpoint under the pointer.
fn slider(hosted: &HostedHistory) -> Child {
    let count = hosted.len();
    let last = count.saturating_sub(1).max(1);
    let percent = hosted.index as f32 / last as f32 * 100.0;
    let thumb = el("div", ())
        .attr("class", "history-thumb")
        .attr("style", format!("left:{percent:.2}%;"));
    let track = el("div", (el("div", ()).attr("class", "history-track"), thumb))
        .attr("class", "history-slider")
        .attr("role", "slider")
        .attr("aria-label", "Checkpoint")
        .attr("aria-valuemin", "1")
        .attr("aria-valuemax", count.to_string())
        .attr("aria-valuenow", (hosted.index + 1).to_string())
        .attr("aria-valuetext", hosted.checkpoint())
        .attr("tabindex", "0");
    let pointer = on_pointer(
        track,
        move |page: &mut TreePage, event: cambium::PointerEvent| {
            if matches!(event.phase, PointerPhase::Down | PointerPhase::Move) && event.size.0 > 0.0
            {
                let fraction = (event.local.0 / event.size.0).clamp(0.0, 1.0);
                page.show_checkpoint((fraction * last as f32).round() as usize);
            }
        },
    );
    Box::new(on_key(pointer, move |page: &mut TreePage, event| {
        let step = match &event.key {
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => Some(-1),
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => Some(1),
            Key::Named(NamedKey::PageDown) => Some(-5),
            Key::Named(NamedKey::PageUp) => Some(5),
            Key::Named(NamedKey::Home) => Some(isize::MIN / 2),
            Key::Named(NamedKey::End) => Some(isize::MAX / 2),
            _ => None,
        };
        if let Some(step) = step {
            page.step_checkpoint(step);
            event.prevent_default();
        }
    }))
}

/// The history region: the checkpoint control, its text value, the change
/// counts with their marks, and every change as a list item. Absent below
/// two checkpoints, as the sandbox hides its slider.
pub(super) fn region(page: &TreePage) -> Option<Child> {
    let hosted = page.history.as_ref()?;
    if hosted.len() < 2 {
        return None;
    }
    let mut children: Vec<Child> = vec![Box::new(
        el(
            "div",
            (
                button("Earlier checkpoint", |page: &mut TreePage, _| {
                    page.step_checkpoint(-1)
                })
                .attr("aria-disabled", (hosted.index == 0).to_string()),
                slider(hosted),
                button("Later checkpoint", |page: &mut TreePage, _| {
                    page.step_checkpoint(1)
                })
                .attr(
                    "aria-disabled",
                    (hosted.index + 1 == hosted.len()).to_string(),
                ),
            ),
        )
        .attr("class", "history-control"),
    )];
    children.push(Box::new(
        el("p", hosted.checkpoint())
            .attr("class", "history-value")
            .attr("role", "status"),
    ));
    if let Some(error) = &hosted.error {
        children.push(Box::new(
            el("p", error.clone())
                .attr("class", "tree-refusal")
                .attr("role", "alert"),
        ));
    }
    if let (Some(summary), Some(changes)) = (hosted.summary(), &hosted.changes) {
        children.push(Box::new(
            el("p", format!("Since the previous checkpoint: {summary}"))
                .attr("class", "history-summary"),
        ));
        let items: Vec<Child> = changes
            .spoken()
            .into_iter()
            .map(|text| Box::new(el("li", text).attr("role", "listitem")) as Child)
            .collect();
        children.push(Box::new(
            el("ul", items)
                .attr("role", "list")
                .attr("aria-label", "Changes")
                .attr("class", "history-changes"),
        ));
    }
    Some(Box::new(
        el("section", children)
            .attr("class", "tree-history")
            .attr("role", "region")
            .attr("aria-label", "History"),
    ))
}
