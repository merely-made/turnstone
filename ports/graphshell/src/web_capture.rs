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
use graphshell::local_profile::LocalProfile;
use muniment::IndexedDbBackend;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::Window;

const CAPTURE_POLICY_GLOBAL: &str = "graphshellCapturePolicyJson";
const CAPTURE_VISITS_GLOBAL: &str = "graphshellInitialVisitsJson";
const HISTORY_FILTER_GLOBAL: &str = "graphshellHistoryFilterJson";
const HISTORY_FORGET_GLOBAL: &str = "graphshellHistoryForgetJson";
// The installed extension profile; absent on reference and fixture pages.
const LOCAL_PROFILE_GLOBAL: &str = "graphshellLocalProfileJson";
// "follower" when another view of the installed profile holds its writer.
const VIEW_ROLE_GLOBAL: &str = "graphshellViewRole";
// The writer finished a save; a follower reads the store again.
const COMMITTED_EVENT: &str = "graphshell-committed";
// The element `loader.js` reads the page's receipts from.
const GRAPHSHELL_ROOT: &str = "graphshell-view, graphshell-tree";
const CAPTURE_COMPLETE_EVENT: &str = "graphshell-capture-complete";
// A batch the extension pulled while this view stays open, and its answer.
const CAPTURE_BATCH_EVENT: &str = "graphshell-capture-batch";
const CAPTURE_BATCH_COMPLETE_EVENT: &str = "graphshell-capture-batch-complete";
// The extension keeps one batch in flight; more waiting means a misbehaving page.
const MAX_WAITING_BATCHES: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CaptureSummary {
    pub(crate) active: bool,
    pub(crate) accepted: usize,
    pub(crate) dropped: usize,
    pub(crate) buffered: usize,
}

impl CaptureSummary {
    pub(crate) fn add(&mut self, other: &Self) {
        self.active |= other.active;
        self.accepted += other.accepted;
        self.dropped += other.dropped;
        self.buffered += other.buffered;
    }
}

/// One delivery pulled from the extension's durable queue while the view is open.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LiveCaptureBatch {
    pub(crate) batch: u64,
    pub(crate) policy: HistoryCapturePolicy,
    pub(crate) visits: Vec<BrowserVisit>,
}

/// Batches waiting for the graph to be free; a refused one keeps its number.
pub(crate) type CaptureInbox = Rc<RefCell<VecDeque<Result<LiveCaptureBatch, (u64, String)>>>>;
pub(crate) type BatchListener = Closure<dyn FnMut(web_sys::Event)>;

fn parse_batch(detail: &str) -> Result<LiveCaptureBatch, (u64, String)> {
    serde_json::from_str(detail).map_err(|error| {
        let batch = serde_json::from_str::<serde_json::Value>(detail)
            .ok()
            .and_then(|value| value.get("batch")?.as_u64())
            .unwrap_or(0);
        (batch, format!("invalid capture batch: {error}"))
    })
}

/// Queue each delivered batch for the frame loop, which applies it when no
/// other save holds the graph. Keep the returned listener for the page's life.
pub(crate) fn listen_for_batches(inbox: CaptureInbox) -> Result<BatchListener, String> {
    let listener = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let detail = js_sys::Reflect::get(&event, &JsValue::from_str("detail"))
            .ok()
            .and_then(|detail| detail.as_string())
            .unwrap_or_default();
        let mut inbox = inbox.borrow_mut();
        let parsed = match parse_batch(&detail) {
            Ok(batch) if inbox.len() >= MAX_WAITING_BATCHES => {
                Err((batch.batch, "too many capture batches waiting".into()))
            },
            parsed => parsed,
        };
        inbox.push_back(parsed);
    });
    crate::document()?
        .add_event_listener_with_callback(CAPTURE_BATCH_EVENT, listener.as_ref().unchecked_ref())
        .map_err(|_| "could not listen for capture batches".to_string())?;
    Ok(listener)
}

fn graphshell_root() -> Result<web_sys::Element, String> {
    crate::document()?
        .query_selector(GRAPHSHELL_ROOT)
        .map_err(|_| "could not look up the Graphshell root".to_string())?
        .ok_or_else(|| "the Graphshell root is not in the page".to_string())
}

fn dispatch(name: &str) -> Result<(), String> {
    let event =
        web_sys::Event::new(name).map_err(|_| format!("could not create the {name} event"))?;
    crate::document()?
        .dispatch_event(&event)
        .map_err(|_| format!("could not announce {name}"))?;
    Ok(())
}

/// Answer a live batch on the Graphshell root. Only `Ok` lets the extension
/// acknowledge it; an error leaves the visits queued for the next pull.
pub(crate) fn announce_batch(
    batch: u64,
    result: &Result<CaptureSummary, String>,
) -> Result<(), String> {
    let root = graphshell_root()?;
    let summary = result.as_ref().copied().unwrap_or_default();
    let error = result.as_ref().err().cloned().unwrap_or_default();
    for (name, value) in [
        ("data-capture-batch", batch.to_string()),
        ("data-capture-batch-error", error),
        ("data-capture-batch-accepted", summary.accepted.to_string()),
        ("data-capture-batch-dropped", summary.dropped.to_string()),
        ("data-capture-batch-buffered", summary.buffered.to_string()),
    ] {
        root.set_attribute(name, &value)
            .map_err(|_| format!("could not expose {name}"))?;
    }
    dispatch(CAPTURE_BATCH_COMPLETE_EVENT)
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

pub(crate) fn local_profile_input(window: &Window) -> Result<Option<LocalProfile>, String> {
    global_json(window, LOCAL_PROFILE_GLOBAL)?
        .map(|json| LocalProfile::parse(&json))
        .transpose()
}

/// Whether this page observes the installed profile while another view writes.
pub(crate) fn following_input(window: &Window) -> Result<bool, String> {
    Ok(global_json(window, VIEW_ROLE_GLOBAL)?.as_deref() == Some("follower"))
}

/// Raise `requested` whenever the writer reports a finished save.
pub(crate) fn listen_for_commits(
    requested: Rc<std::cell::Cell<bool>>,
) -> Result<BatchListener, String> {
    let listener =
        Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| requested.set(true));
    crate::document()?
        .add_event_listener_with_callback(COMMITTED_EVENT, listener.as_ref().unchecked_ref())
        .map_err(|_| "could not listen for the writer's saves".to_string())?;
    Ok(listener)
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

pub(crate) async fn apply_capture(
    app: &mut GraphshellApp<IndexedDbBackend>,
    store: &mut IndexedDbBackend,
    input: Option<(HistoryCapturePolicy, Vec<BrowserVisit>)>,
    persona: &str,
    device: &str,
    now_secs: u64,
) -> Result<CaptureSummary, String> {
    let Some((policy, visits)) = input else {
        return Ok(CaptureSummary::default());
    };
    let mut capture = BrowserHistoryCapture::load(store, policy)
        .await
        .map_err(|error| error.to_string())?;
    if visits.is_empty() {
        return Ok(CaptureSummary {
            active: capture.policy().enabled,
            ..CaptureSummary::default()
        });
    }
    let outcomes = capture
        .ingest_batch(&mut app.host, store, visits, persona, device, now_secs)
        .await
        .map_err(|error| error.to_string())?;
    Ok(CaptureSummary {
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
/// after `apply_capture` returned Ok: a failed batch must stay queued.
pub(crate) fn announce_capture(summary: CaptureSummary) -> Result<(), String> {
    if !summary.active {
        return Ok(());
    }
    let root = graphshell_root()?;
    for (name, count) in [
        ("data-capture-accepted", summary.accepted),
        ("data-capture-dropped", summary.dropped),
        ("data-capture-buffered", summary.buffered),
    ] {
        root.set_attribute(name, &count.to_string())
            .map_err(|_| format!("could not expose {name}"))?;
    }
    dispatch(CAPTURE_COMPLETE_EVENT)
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
