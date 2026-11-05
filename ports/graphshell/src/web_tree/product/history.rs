// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Capture and history controls over the durable access authority.
use super::*;
use crate::web_capture::{
    BatchListener, CaptureInbox, CaptureSummary, HistoryControlSummary, HistoryForgetInput,
    LiveCaptureBatch, announce_batch, announce_capture, apply_capture, apply_history_controls,
    apply_history_request, initial_capture_input, listen_for_batches,
};
use cambium::{button, checkbox, el, lens, text_field_typed};
use graphshell::access::AccessRecordFilter;
use graphshell::capture::HistoryCapturePolicy;

pub(super) const LABELS: [&str; 5] = [
    "History since (ms)",
    "History until (ms)",
    "History persona",
    "History device",
    "Forget address",
];
pub(super) struct HistoryControls {
    pub(super) open: bool,
    pub(super) inputs: Vec<TextInput>,
    remove_object: bool,
    store: IndexedDbBackend,
    policy: HistoryCapturePolicy,
    summary: HistoryControlSummary,
    capture: CaptureSummary,
    completed: Rc<RefCell<Option<HistoryControlSummary>>>,
    /// The persona slot and device that capture records name.
    attribution: (String, String),
    pub(super) inbox: CaptureInbox,
    _listener: Option<BatchListener>,
}

/// A live batch's graph returned with its number and committed counts.
pub(super) type BatchCompletion = (App, u64, Result<CaptureSummary, String>);
impl HistoryControls {
    /// `attribution` is the persona slot and device that capture records name.
    pub(super) async fn load(
        app: &mut App,
        mut store: IndexedDbBackend,
        attribution: (&str, &str),
    ) -> Result<Self, String> {
        let input = initial_capture_input(&crate::window()?)?;
        let policy = input
            .as_ref()
            .map(|(policy, _)| policy.clone())
            .unwrap_or_else(HistoryCapturePolicy::disabled);
        let (persona, device) = attribution;
        let capture = apply_capture(app, &mut store, input, persona, device, now_secs()).await?;
        // The batch has committed; only now may the extension drop what it pulled.
        announce_capture(capture)?;
        let summary = apply_history_controls(app, &mut store, policy.clone(), now_secs()).await;
        let inbox = CaptureInbox::default();
        let listener = listen_for_batches(inbox.clone())?;
        Ok(Self {
            open: false,
            inputs: LABELS.iter().map(|_| TextInput::new("")).collect(),
            remove_object: false,
            store,
            policy,
            summary,
            capture,
            completed: Rc::new(RefCell::new(None)),
            attribution: (persona.to_owned(), device.to_owned()),
            inbox,
            _listener: Some(listener),
        })
    }

    /// A follower's controls: it takes no capture and runs no history action.
    pub(super) fn idle(store: IndexedDbBackend, attribution: (&str, &str)) -> Self {
        Self {
            open: false,
            inputs: LABELS.iter().map(|_| TextInput::new("")).collect(),
            remove_object: false,
            store,
            policy: HistoryCapturePolicy::disabled(),
            summary: HistoryControlSummary::default(),
            capture: CaptureSummary::default(),
            completed: Rc::new(RefCell::new(None)),
            attribution: (attribution.0.to_owned(), attribution.1.to_owned()),
            inbox: CaptureInbox::default(),
            _listener: None,
        }
    }
    pub(super) fn poll(&mut self) {
        if let Some(summary) = self.completed.borrow_mut().take() {
            self.summary = summary;
        }
    }
}
impl SavedProduct {
    pub(super) fn live_capture_ready(&self) -> bool {
        self.capture_completed.borrow().is_some()
            || (self.app.is_some()
                && !self.selection_locked()
                && !self.history.inbox.borrow().is_empty())
    }

    /// Apply the next delivered batch once no other save holds the graph;
    /// edits wait for it, as for any save.
    pub(super) fn start_live_capture(&mut self) {
        if self.app.is_none() || self.selection_locked() {
            return;
        }
        let Some(next) = self.history.inbox.borrow_mut().pop_front() else {
            return;
        };
        let LiveCaptureBatch {
            batch,
            policy,
            visits,
        } = match next {
            Ok(batch) => batch,
            Err((batch, error)) => {
                if let Err(error) = announce_batch(batch, &Err(error)) {
                    web_sys::console::error_1(&error.into());
                }
                return;
            },
        };
        let mut app = self.app.take().expect("checked above");
        let mut store = self.history.store.clone();
        let (persona, device) = self.history.attribution.clone();
        let completed = self.capture_completed.clone();
        self.capture_pending = true;
        self.spawn_storage_task(async move {
            let result = async {
                let summary = apply_capture(
                    &mut app,
                    &mut store,
                    Some((policy, visits)),
                    &persona,
                    &device,
                    now_secs(),
                )
                .await?;
                app.host
                    .persist(now_secs())
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(summary)
            }
            .await;
            *completed.borrow_mut() = Some((app, batch, result));
        });
    }

    /// Show a finished batch and answer the extension; only a committed batch
    /// is acknowledged, so a failed one stays in its durable queue.
    pub(super) fn finish_live_capture(&mut self, canvas: &mut Canvas) -> bool {
        let Some((app, batch, result)) = self.capture_completed.borrow_mut().take() else {
            return false;
        };
        let owner = app.host.graph_session().id().0;
        let owner_changed = self.session.parse().ok() != Some(owner);
        graphshell::local_scene::adopt_host_graph(
            canvas,
            self.session.parse().ok(),
            owner,
            &app.host,
        );
        self.session = owner.to_string();
        self.forme_reconcile_session = Some(owner);
        self.app = Some(app);
        self.capture_pending = false;
        if owner_changed {
            self.restore_canvas_link_hides(canvas);
        }
        match &result {
            Ok(summary) => {
                self.history.capture.add(summary);
                if summary.accepted > 0 {
                    self.status =
                        format!("Captured {} visit(s) from this browser", summary.accepted);
                }
            },
            Err(error) => {
                self.status = format!("Capture not saved · {error} · the visits stay queued");
            },
        }
        if let Err(error) = announce_batch(batch, &result) {
            web_sys::console::error_1(&error.into());
        }
        true
    }

    pub(in crate::web_tree) fn history_fields(&self, snapshot: ProbeSnapshot) -> ProbeSnapshot {
        snapshot
            .with_field(
                "history-result-count",
                self.history.summary.records.len().to_string(),
            )
            .with_field(
                "history-forget-attempted",
                self.history.summary.forget_attempted.to_string(),
            )
            .with_field(
                "history-forgotten",
                self.history.summary.forgotten.to_string(),
            )
            .with_field(
                "history-error",
                self.history.summary.error.clone().unwrap_or_default(),
            )
            .with_field(
                "capture-accepted",
                self.history.capture.accepted.to_string(),
            )
            .with_field("capture-dropped", self.history.capture.dropped.to_string())
            .with_field(
                "capture-buffered",
                self.history.capture.buffered.to_string(),
            )
    }
    fn history_action(&mut self, forget: bool) -> Result<(), String> {
        if self.selection_locked() {
            return Err("A save is pending".into());
        }
        if self.following() {
            return Err("History actions run in the view that edits this graph".into());
        }
        let history = &self.history;
        let number = |index: usize| -> Result<Option<u64>, String> {
            let text = history.inputs[index].text().trim();
            if text.is_empty() {
                Ok(None)
            } else {
                text.parse()
                    .map(Some)
                    .map_err(|_| format!("{} requires a whole number", LABELS[index]))
            }
        };
        let optional = |index: usize| {
            let text = history.inputs[index].text().trim();
            (!text.is_empty()).then(|| text.to_owned())
        };
        let filter = AccessRecordFilter {
            start_ms: number(0)?,
            end_ms: number(1)?,
            persona: optional(2),
            device: optional(3),
        };
        let request = if forget {
            Some(HistoryForgetInput {
                url: optional(4).ok_or("Enter the address to forget")?,
                remove_object: history.remove_object,
            })
        } else {
            None
        };
        let mut store = history.store.clone();
        let policy = history.policy.clone();
        let summary_slot = history.completed.clone();
        let completed = self.operation_completed.clone();
        let mut app = self.app.take().ok_or("Local graph is busy")?;
        self.operation_reconciles_graph = true;
        self.saving = true;
        self.save_state = "saving";
        self.spawn_storage_task(async move {
            let summary =
                apply_history_request(&mut app, &mut store, policy, now_secs(), filter, request)
                    .await;
            let result = if let Some(error) = &summary.error {
                Err(error.clone())
            } else {
                app.host
                    .persist(now_secs())
                    .await
                    .map_err(|e| e.to_string())
            };
            *summary_slot.borrow_mut() = Some(summary);
            *completed.borrow_mut() = Some((app, None, result));
        });
        Ok(())
    }
}
pub(in crate::web_tree) fn section(page: &TreePage) -> Child {
    let Some(product) = &page.product else {
        return Box::new(el("div", ()));
    };
    let mut children: Vec<Child> = vec![Box::new(disclosure_trigger(
        "History",
        product.history.open,
        |page: &mut TreePage| {
            if let Some(product) = &mut page.product {
                product.history.open = !product.history.open;
            }
        },
    ))];
    if product.history.open {
        children.push(Box::new(status_note(
            "",
            format!("{} history records", product.history.summary.records.len()),
        )));
        for (index, label) in LABELS.into_iter().enumerate() {
            children.push(Box::new(el(
                "label",
                (
                    label,
                    lens(
                        move |input: &mut TextInput| {
                            text_field_typed(input).attr("aria-label", label)
                        },
                        move |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("history").history.inputs[index]
                        },
                    ),
                ),
            )));
        }
        children.push(Box::new(lens(
            |checked: &mut bool| {
                checkbox(*checked).attr("aria-label", "Remove captured object too")
            },
            |page: &mut TreePage| {
                &mut page
                    .product
                    .as_mut()
                    .expect("history")
                    .history
                    .remove_object
            },
        )));
        for (label, forget) in [("Query history", false), ("Forget address", true)] {
            children.push(Box::new(button(label, move |page: &mut TreePage, _| {
                if let Some(product) = &mut page.product {
                    if let Err(error) = product.history_action(forget) {
                        product.status = error;
                    }
                }
            })));
        }
        let rows: Vec<Child> = product
            .history
            .summary
            .records
            .iter()
            .map(|record| {
                Box::new(el(
                    "li",
                    format!(
                        "{} · {} · {} · {}",
                        record.address, record.at_ms, record.persona, record.device
                    ),
                )) as Child
            })
            .collect();
        children.push(Box::new(
            el("ul", rows).attr("aria-label", "History records"),
        ));
        if let Some(error) = &product.history.summary.error {
            children.push(Box::new(el("p", error.clone()).attr("role", "alert")));
        }
    }
    Box::new(el("section", children).attr("aria-label", "History"))
}
