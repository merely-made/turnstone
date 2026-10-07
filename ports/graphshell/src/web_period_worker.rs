// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The display period's tiebreaker (physics catalog plan, ruled 2026-10-06,
//! "Worker rAF, main thread fallback"): a dedicated worker running its own
//! rAF loop over a one-pixel `OffscreenCanvas`, doing no work, so it runs at
//! one refresh whatever the page's load (on this machine and the ThinkPad,
//! in Chrome and Firefox alike), and posting its intervals in batches for the
//! frame budget. Where the worker has no rAF, fails to start or is turned
//! off, the main thread's intervals stand in, and the budget says which.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Blob, BlobPropertyBag, MessageEvent, Url, Worker};

use crate::web_speed::{FrameBudget, PeriodSource};

/// The worker's script. `PLANTED` is replaced with `true` for the planted
/// failure, the fallback's control.
const SCRIPT: &str = "\
const planted = PLANTED;
if (planted || typeof requestAnimationFrame !== 'function' || typeof OffscreenCanvas !== 'function') {
  postMessage('unavailable');
} else {
  const g = new OffscreenCanvas(1, 1).getContext('2d');
  let last = null;
  let batch = [];
  const frame = (ts) => {
    if (last !== null) batch.push(ts - last);
    last = ts;
    g.fillRect(0, 0, 1, 1);
    if (batch.length >= 8) { postMessage(batch); batch = []; }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}
";

#[derive(Default)]
struct Feed {
    intervals: Vec<f64>,
    unavailable: bool,
}

/// The worker and what it has posted since the last frame.
pub(crate) struct PeriodWorker {
    worker: Option<Worker>,
    feed: Rc<RefCell<Feed>>,
    _on_message: Option<Closure<dyn FnMut(MessageEvent)>>,
    _on_error: Option<Closure<dyn FnMut(JsValue)>>,
}

impl PeriodWorker {
    /// Start the worker, unless the page turned it off; a worker that cannot
    /// start leaves the main thread's intervals.
    pub(crate) fn start(source: PeriodSource) -> Self {
        let feed = Rc::new(RefCell::new(Feed::default()));
        let none = |feed| Self {
            worker: None,
            feed,
            _on_message: None,
            _on_error: None,
        };
        if source == PeriodSource::Main {
            return none(feed);
        }
        let script = SCRIPT.replace(
            "PLANTED",
            if source == PeriodSource::PlantedFailure {
                "true"
            } else {
                "false"
            },
        );
        let options = BlobPropertyBag::new();
        options.set_type("text/javascript");
        let worker = js_sys::Array::of1(&JsValue::from_str(&script));
        let worker = Blob::new_with_str_sequence_and_options(&worker, &options)
            .and_then(|blob| Url::create_object_url_with_blob(&blob))
            .and_then(|url| Worker::new(&url));
        let Ok(worker) = worker else {
            feed.borrow_mut().unavailable = true;
            return none(feed);
        };
        let posted = feed.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let data = event.data();
            let mut feed = posted.borrow_mut();
            if data.is_string() {
                feed.unavailable = true;
            } else if let Some(batch) = data.dyn_ref::<js_sys::Array>() {
                feed.intervals
                    .extend(batch.iter().filter_map(|interval| interval.as_f64()));
            }
        });
        let failed = feed.clone();
        let on_error = Closure::<dyn FnMut(JsValue)>::new(move |_: JsValue| {
            failed.borrow_mut().unavailable = true;
        });
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        Self {
            worker: Some(worker),
            feed,
            _on_message: Some(on_message),
            _on_error: Some(on_error),
        }
    }

    /// Give the budget what the worker posted since the last frame.
    pub(crate) fn feed(&self, budget: &mut FrameBudget, now_ms: f64) {
        let mut feed = self.feed.borrow_mut();
        if feed.unavailable {
            budget.worker_unavailable();
        }
        if !feed.intervals.is_empty() {
            budget.worker_intervals(&feed.intervals, now_ms);
            feed.intervals.clear();
        }
    }
}

impl Drop for PeriodWorker {
    fn drop(&mut self) {
        if let Some(worker) = &self.worker {
            worker.terminate();
        }
    }
}
