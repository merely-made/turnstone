// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Browser capture input and durable history access, independent of a presenter.
use crate::window;
use graphshell::access::{AccessRecord, AccessRecordFilter, query_access_records};
use graphshell::app::GraphshellApp;
use graphshell::capture::{
    BROWSER_HISTORY_HANDLER_PREFIX, BrowserHistoryCapture, BrowserVisit, CaptureOutcome,
    ForgetMode, HistoryCapturePolicy,
};
use graphshell::mere_host::FIXTURE_DEVICE_TWO_ADDRESS;
use muniment::IndexedDbBackend;
use serde::Deserialize;
use wasm_bindgen::JsValue;
use web_sys::Window;

const CAPTURE_POLICY_GLOBAL: &str = "graphshellCapturePolicyJson";
const CAPTURE_VISITS_GLOBAL: &str = "graphshellInitialVisitsJson";
const HISTORY_FILTER_GLOBAL: &str = "graphshellHistoryFilterJson";
const HISTORY_FORGET_GLOBAL: &str = "graphshellHistoryForgetJson";
// The element `loader.js` reads the page's receipts from.
const GRAPHSHELL_ROOT: &str = "graphshell-view, graphshell-tree";
const CAPTURE_COMPLETE_EVENT: &str = "graphshell-capture-complete";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InitialCaptureSummary {
    pub(crate) active: bool,
    pub(crate) accepted: usize,
    pub(crate) dropped: usize,
    pub(crate) buffered: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct HistoryFilterInput {
    pub(crate) start_ms: Option<u64>,
    pub(crate) end_ms: Option<u64>,
    pub(crate) persona: Option<String>,
    pub(crate) device: Option<String>,
}

impl From<HistoryFilterInput> for AccessRecordFilter {
    fn from(value: HistoryFilterInput) -> Self {
        Self {
            start_ms: value.start_ms,
            end_ms: value.end_ms,
            persona: value.persona,
            device: value.device,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryForgetInput {
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) remove_object: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HistoryControlSummary {
    pub(crate) active: bool,
    pub(crate) records: Vec<AccessRecord>,
    pub(crate) forget_attempted: bool,
    pub(crate) forgotten: usize,
    pub(crate) error: Option<String>,
}

fn global_json(window: &Window, name: &str) -> Result<Option<String>, String> {
    let value = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str(name))
        .map_err(|_| format!("could not read browser global {name}"))?;
    Ok(value.as_string())
}

pub(crate) fn initial_capture_input(
    window: &Window,
) -> Result<Option<(HistoryCapturePolicy, Vec<BrowserVisit>)>, String> {
    let Some(policy_json) = global_json(window, CAPTURE_POLICY_GLOBAL)? else {
        return Ok(None);
    };
    let policy = serde_json::from_str(&policy_json)
        .map_err(|error| format!("invalid browser capture policy: {error}"))?;
    let visits_json =
        global_json(window, CAPTURE_VISITS_GLOBAL)?.unwrap_or_else(|| "[]".to_string());
    let visits = serde_json::from_str(&visits_json)
        .map_err(|error| format!("invalid browser visit batch: {error}"))?;
    Ok(Some((policy, visits)))
}

pub(crate) async fn apply_initial_capture(
    app: &mut GraphshellApp<IndexedDbBackend>,
    store: &mut IndexedDbBackend,
    input: Option<(HistoryCapturePolicy, Vec<BrowserVisit>)>,
    persona: &str,
    now_secs: u64,
) -> Result<InitialCaptureSummary, String> {
    let Some((policy, visits)) = input else {
        return Ok(InitialCaptureSummary::default());
    };
    let mut capture = BrowserHistoryCapture::load(store, policy)
        .await
        .map_err(|error| error.to_string())?;
    if visits.is_empty() {
        return Ok(InitialCaptureSummary {
            active: capture.policy().enabled,
            ..InitialCaptureSummary::default()
        });
    }
    let outcomes = capture
        .ingest_batch(
            &mut app.host,
            store,
            visits,
            persona,
            FIXTURE_DEVICE_TWO_ADDRESS,
            now_secs,
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(InitialCaptureSummary {
        active: capture.policy().enabled,
        accepted: outcomes
            .iter()
            .filter(|outcome| matches!(outcome, CaptureOutcome::Accepted { .. }))
            .count(),
        dropped: outcomes
            .iter()
            .filter(|outcome| matches!(outcome, CaptureOutcome::Dropped(_)))
            .count(),
        buffered: outcomes
            .iter()
            .filter(|outcome| matches!(outcome, CaptureOutcome::Buffered { .. }))
            .count(),
    })
}

/// Expose a landed batch's counters on the Graphshell root, then announce it on
/// the document so the extension acknowledges the visits it pulled. Call it only
/// after `apply_initial_capture` returned Ok: a failed batch must stay queued.
pub(crate) fn announce_capture(summary: InitialCaptureSummary) -> Result<(), String> {
    if !summary.active {
        return Ok(());
    }
    let document = crate::document()?;
    let root = document
        .query_selector(GRAPHSHELL_ROOT)
        .map_err(|_| "could not look up the Graphshell root".to_string())?
        .ok_or("the Graphshell root is not in the page")?;
    for (name, count) in [
        ("data-capture-accepted", summary.accepted),
        ("data-capture-dropped", summary.dropped),
        ("data-capture-buffered", summary.buffered),
    ] {
        root.set_attribute(name, &count.to_string())
            .map_err(|_| format!("could not expose {name}"))?;
    }
    let event = web_sys::Event::new(CAPTURE_COMPLETE_EVENT)
        .map_err(|_| "could not create the capture completion event".to_string())?;
    document
        .dispatch_event(&event)
        .map_err(|_| "could not announce the capture completion".to_string())?;
    Ok(())
}

pub(crate) fn history_control_input(
    window: &Window,
) -> Result<Option<(AccessRecordFilter, Option<HistoryForgetInput>)>, String> {
    let Some(filter_json) = global_json(window, HISTORY_FILTER_GLOBAL)? else {
        return Ok(None);
    };
    let filter: HistoryFilterInput = serde_json::from_str(&filter_json)
        .map_err(|error| format!("invalid history authority filter: {error}"))?;
    let forget = global_json(window, HISTORY_FORGET_GLOBAL)?
        .map(|json| {
            serde_json::from_str(&json)
                .map_err(|error| format!("invalid history forget request: {error}"))
        })
        .transpose()?;
    Ok(Some((filter.into(), forget)))
}

pub(crate) async fn apply_history_controls(
    app: &mut GraphshellApp<IndexedDbBackend>,
    store: &mut IndexedDbBackend,
    policy: HistoryCapturePolicy,
    now_secs: u64,
) -> HistoryControlSummary {
    let browser_window = match window() {
        Ok(window) => window,
        Err(error) => {
            return HistoryControlSummary {
                active: true,
                error: Some(error),
                ..HistoryControlSummary::default()
            };
        },
    };
    let input = match history_control_input(&browser_window) {
        Ok(input) => input,
        Err(error) => {
            return HistoryControlSummary {
                active: true,
                error: Some(error),
                ..HistoryControlSummary::default()
            };
        },
    };
    let Some((filter, forget)) = input else {
        return HistoryControlSummary::default();
    };
    apply_history_request(app, store, policy, now_secs, filter, forget).await
}

pub(crate) async fn apply_history_request(
    app: &mut GraphshellApp<IndexedDbBackend>,
    store: &mut IndexedDbBackend,
    policy: HistoryCapturePolicy,
    now_secs: u64,
    filter: AccessRecordFilter,
    forget: Option<HistoryForgetInput>,
) -> HistoryControlSummary {
    let mut summary = HistoryControlSummary {
        active: true,
        forget_attempted: forget.is_some(),
        ..HistoryControlSummary::default()
    };
    if let Some(forget) = forget {
        let mode = if forget.remove_object {
            ForgetMode::RemoveCapturedObject
        } else {
            ForgetMode::HistoryOnly
        };
        let result = async {
            let mut capture = BrowserHistoryCapture::load(store, policy).await?;
            capture
                .forget_url(&mut app.host, store, &forget.url, mode, now_secs)
                .await
        }
        .await;
        match result {
            Ok(forgotten) => summary.forgotten = forgotten,
            Err(error) => summary.error = Some(error.to_string()),
        }
    }
    if summary.error.is_none() {
        match query_access_records(store, &filter).await {
            Ok(records) => {
                summary.records = records
                    .into_iter()
                    .filter(|record| record.handler.starts_with(BROWSER_HISTORY_HANDLER_PREFIX))
                    .collect();
            },
            Err(error) => summary.error = Some(error.to_string()),
        }
    }
    summary
}
