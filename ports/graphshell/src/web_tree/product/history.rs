// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Capture and history controls over the durable access authority.
use super::*;
use crate::web_capture::{
    HistoryControlSummary, HistoryForgetInput, InitialCaptureSummary, apply_history_controls,
    apply_history_request, apply_initial_capture, initial_capture_input,
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
    capture: InitialCaptureSummary,
    completed: Rc<RefCell<Option<HistoryControlSummary>>>,
}
impl HistoryControls {
    pub(super) async fn load(app: &mut App, mut store: IndexedDbBackend) -> Result<Self, String> {
        let input = initial_capture_input(&crate::window()?)?;
        let policy = input
            .as_ref()
            .map(|(policy, _)| policy.clone())
            .unwrap_or_else(HistoryCapturePolicy::disabled);
        let capture =
            apply_initial_capture(app, &mut store, input, FIXTURE_PERSONA_ADDRESS, now_secs())
                .await?;
        let summary = apply_history_controls(app, &mut store, policy.clone(), now_secs()).await;
        Ok(Self {
            open: false,
            inputs: LABELS.iter().map(|_| TextInput::new("")).collect(),
            remove_object: false,
            store,
            policy,
            summary,
            capture,
            completed: Rc::new(RefCell::new(None)),
        })
    }
    pub(super) fn poll(&mut self) {
        if let Some(summary) = self.completed.borrow_mut().take() {
            self.summary = summary;
        }
    }
}
impl SavedProduct {
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
    }
    fn history_action(&mut self, forget: bool) -> Result<(), String> {
        if self.selection_locked() {
            return Err("A save is pending".into());
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
        self.saving = true;
        self.save_state = "saving";
        wasm_bindgen_futures::spawn_local(async move {
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
            *completed.borrow_mut() = Some((app, result));
        });
        Ok(())
    }
}
pub(in crate::web_tree) fn section(page: &TreePage) -> Child {
    let Some(product) = &page.product else {
        return Box::new(el("div", ()));
    };
    let mut children: Vec<Child> = vec![Box::new(
        button("History", |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.history.open = !product.history.open;
            }
        })
        .attr("aria-expanded", product.history.open.to_string()),
    )];
    if product.history.open {
        children.push(Box::new(
            el(
                "p",
                format!("{} history records", product.history.summary.records.len()),
            )
            .attr("role", "status"),
        ));
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
