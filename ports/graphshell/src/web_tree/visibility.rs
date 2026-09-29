// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Opt-in headed visibility receipt. `?visibility_sink=<scenario-receipt URL>`
//! posts once after a real hidden/shown transition and twelve resumed frames.
//! It observes document visibility; it never simulates a platform transition.

use super::*;
use serde_json::Value;
use web_sys::{Request, RequestInit};

const MAX_EVENTS: usize = 8;
const RESUMED_FRAMES: usize = 12;

pub(super) struct Visibility {
    sink: String,
    events: Vec<Value>,
    initial_hidden: bool,
    initial_at_ms: f64,
    hidden_at_ms: Option<f64>,
    shown_at_ms: Option<f64>,
    shown_mirror_nodes: usize,
    producer_calls: usize,
    hidden_calls: usize,
    hidden_geometry: Option<String>,
    first_resume: Option<Value>,
    resume_geometry: Option<String>,
    resumed_frames: usize,
    later_motion: bool,
    max_resumed_steps: u32,
    sent: bool,
}

pub(super) fn requested() -> Result<Option<RefCell<Visibility>>, String> {
    let search = web_sys::window()
        .ok_or("no window")?
        .location()
        .search()
        .map_err(|_| "could not read visibility query")?;
    let params = web_sys::UrlSearchParams::new_with_str(&search)
        .map_err(|_| "could not parse visibility query")?;
    Ok(params
        .get("visibility_sink")
        .filter(|sink| !sink.is_empty())
        .map(|sink| {
            RefCell::new(Visibility {
                sink,
                events: Vec::new(),
                initial_hidden: false,
                initial_at_ms: 0.0,
                hidden_at_ms: None,
                shown_at_ms: None,
                shown_mirror_nodes: 0,
                producer_calls: 0,
                hidden_calls: 0,
                hidden_geometry: None,
                first_resume: None,
                resume_geometry: None,
                resumed_frames: 0,
                later_motion: false,
                max_resumed_steps: 0,
                sent: false,
            })
        }))
}

pub(super) fn install(shared: &Rc<Shared>, document: &web_sys::Document) -> Result<(), String> {
    let Some(state) = &shared.visibility else {
        return Ok(());
    };
    {
        let mut state = state.borrow_mut();
        state.initial_hidden = document.hidden();
        state.initial_at_ms = now_ms();
        if state.initial_hidden {
            state.hidden_at_ms = Some(state.initial_at_ms);
            state.hidden_geometry = Some(geometry(&shared.canvas.borrow()));
        }
    }
    let event_shared = shared.clone();
    let event_document = document.clone();
    let listener = Closure::<dyn FnMut(Event)>::new(move |_| {
        let Some(state) = &event_shared.visibility else {
            return;
        };
        let mut state = state.borrow_mut();
        if state.sent {
            return;
        }
        let hidden = event_document.hidden();
        let at_ms = now_ms();
        let geometry = geometry(&event_shared.canvas.borrow());
        if state.events.len() < MAX_EVENTS {
            let calls = state.producer_calls;
            state.events.push(json!({
                "hidden": hidden, "at_ms": at_ms, "producer_calls": calls,
                "geometry": geometry,
            }));
        }
        if hidden && state.first_resume.is_none() {
            state.hidden_geometry = Some(geometry);
            state.hidden_at_ms = Some(at_ms);
        } else if !hidden && state.hidden_at_ms.is_some() && state.first_resume.is_none() {
            state.shown_at_ms = Some(at_ms);
            state.shown_mirror_nodes = event_document
                .query_selector("[data-cambium-mirror]")
                .ok()
                .flatten()
                .map_or(0, |mirror| mirror.child_element_count() as usize);
        }
    });
    document
        .add_event_listener_with_callback("visibilitychange", listener.as_ref().unchecked_ref())
        .map_err(|_| "could not observe headed visibility")?;
    listener.forget();
    Ok(())
}

pub(super) fn before(shared: &Shared) -> Option<String> {
    let state = shared.visibility.as_ref()?;
    let mut state = state.borrow_mut();
    if state.sent {
        return None;
    }
    state.producer_calls += 1;
    if super::super::web_timing::page_hidden() {
        state.hidden_calls += 1;
    }
    state
        .hidden_geometry
        .as_ref()
        .map(|_| geometry(&shared.canvas.borrow()))
}

pub(super) fn after(
    shared: &Shared,
    canvas: &Canvas,
    before: Option<String>,
    timestamp: Option<std::time::Duration>,
) {
    let Some(state) = &shared.visibility else {
        return;
    };
    let mut state = state.borrow_mut();
    if state.sent || state.hidden_geometry.is_none() || super::super::web_timing::page_hidden() {
        return;
    }
    // An actual shown event is required, not merely a visible producer call.
    if !state.events.iter().any(|event| event["hidden"] == false) {
        return;
    }
    let geometry = geometry(canvas);
    let report = canvas.elapsed_step_report().unwrap_or_default();
    state.max_resumed_steps = state.max_resumed_steps.max(report.steps);
    if state.first_resume.is_none() {
        state.first_resume = Some(json!({
            "at_ms": now_ms(), "steps": report.steps,
            "timestamp_ms": timestamp.map(|time| time.as_secs_f64() * 1000.0),
            "discarded_us": report.discarded_elapsed.as_micros() as u64,
            "geometry_before": before, "geometry_after": geometry,
            "matches_hidden_geometry": state.hidden_geometry.as_ref() == Some(&geometry),
            "before_matches_hidden_geometry": state.hidden_geometry == before,
        }));
        state.resume_geometry = Some(geometry.clone());
    } else {
        state.later_motion |= state.resume_geometry.as_ref() != Some(&geometry);
    }
    state.resumed_frames += 1;
    if state.resumed_frames < RESUMED_FRAMES {
        return;
    }
    state.sent = true;
    let first = state.first_resume.as_ref().unwrap();
    let hidden_duration_ms = state
        .hidden_at_ms
        .zip(state.shown_at_ms)
        .map_or(0.0, |(hidden, shown)| shown - hidden);
    // A hidden initial mount has not painted its leaf at the actual extent.
    // Its first composition may finish analytic layout preparation. Active
    // hide/show must preserve both pre-frame and post-frame world geometry.
    let geometry_preserved =
        first["matches_hidden_geometry"] == true && first["before_matches_hidden_geometry"] == true;
    let ok = state.hidden_calls == 0
        && hidden_duration_ms > 0.0
        && first["steps"] == 0
        && first["discarded_us"] == 0
        && (state.initial_hidden || geometry_preserved)
        && !first["timestamp_ms"].is_null()
        && state.max_resumed_steps <= shared.physics_config.max_steps
        && (state.later_motion || (state.initial_hidden && canvas.physics_paused()))
        && (!state.initial_hidden || state.shown_mirror_nodes > 0);
    let log = vec![
        format!("real visibility transitions: {}", state.events.len()),
        format!("producer calls while hidden: {}", state.hidden_calls),
        format!(
            "hidden interval ms: {hidden_duration_ms:.3}; initially hidden: {}",
            state.initial_hidden
        ),
        format!(
            "first resumed steps={} discarded_us={} geometry_preserved={}",
            first["steps"], first["discarded_us"], first["matches_hidden_geometry"]
        ),
        format!(
            "later motion after {} resumed frames: {}",
            state.resumed_frames, state.later_motion
        ),
    ];
    let body = json!({
        "scenario": {"result": {"result": if ok {"ok"} else {"fail"}, "log": log}},
        "visibility": {
            "initial_hidden": state.initial_hidden, "initial_at_ms": state.initial_at_ms,
            "hidden_duration_ms": hidden_duration_ms, "shown_mirror_nodes": state.shown_mirror_nodes,
            "events": state.events, "producer_calls": state.producer_calls,
            "producer_calls_while_hidden": state.hidden_calls,
            "first_resume": state.first_resume, "resumed_frames": state.resumed_frames,
            "later_motion": state.later_motion, "result": if ok {"ok"} else {"fail"},
            "max_resumed_steps": state.max_resumed_steps,
            "physics_step_cap": shared.physics_config.max_steps,
            "geometry_preservation_required": !state.initial_hidden,
        },
    });
    post(state.sink.clone(), body.to_string());
}

fn geometry(canvas: &Canvas) -> String {
    let geometry = canvas.cartography_geometry();
    let mut positions: Vec<_> = geometry.iter().collect();
    positions.sort_by_key(|(id, _)| *id);
    let hash = positions
        .iter()
        .flat_map(|(_, (x, y))| {
            x.to_bits()
                .to_le_bytes()
                .into_iter()
                .chain(y.to_bits().to_le_bytes())
        })
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    format!("{hash:016x}")
}

fn post(sink: String, body: String) {
    wasm_bindgen_futures::spawn_local(async move {
        let options = RequestInit::new();
        options.set_method("POST");
        options.set_body(&JsValue::from_str(&body));
        let result = async {
            let request = Request::new_with_str_and_init(&sink, &options)?;
            request.headers().set("Content-Type", "application/json")?;
            let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
            let response =
                wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await?;
            let response: web_sys::Response = response.dyn_into()?;
            if !response.ok() {
                return Err(JsValue::from_str("visibility receipt POST failed"));
            }
            Ok::<(), JsValue>(())
        }
        .await;
        if let Err(error) = result {
            web_sys::console::error_1(&error);
        }
    });
}
