// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page's end of the physics Worker (Graphshell host plan, T7): a
//! [`PhysicsTransport`] over `postMessage`, and the flag that picks the
//! backend.
//!
//! `?physics=worker` (or `data-physics="worker"` on the element) puts the
//! canvas's simulation in `physics/worker.js`, a module Worker the mount
//! script names in `data-physics-worker`; `?physics=inline` keeps it in the
//! page. The read-only viewer's builds start in Worker mode
//! (`physics-worker-default`, named by `build-viewer.py`'s feature lists, after
//! the headed receipts passed in Worker mode); the web crate's own default
//! features keep the engine and start inline. A page that asks for the Worker
//! and cannot start it keeps the inline backend (where the build has one) and
//! says which it has in `data-physics-backend` on the element, so a receipt can
//! check.
//!
//! A build without `physics-engine` (the viewer's engine-free configuration)
//! has no inline backend: it always asks for the Worker, `?physics=inline` is
//! ignored with a console warning and `data-physics-inline-ignored` on the
//! element (the Worker runs), and a Worker that cannot start, or that fails
//! after it started, leaves the page with no physics. The page then says so:
//! `data-physics-backend="unavailable"` and `data-physics-unavailable` (the
//! reason) on the element, which `mount.js` turns into a visible
//! `role="status"` notice; the graph stays where it was first drawn. Nothing
//! is silently blank.
//!
//! Messages are the postcard bytes `seiche::wire` writes, one per
//! `postMessage`, each in a buffer that is transferred. The Worker is
//! started when the canvas is built (the canvas queues what it sends, in
//! order, until the Worker says `Ready`), which is on first paint's
//! heels and not on the first drag.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use js_sys::{Array, Uint8Array};
use cartography::canvas::{PhysicsTransport, UnavailableTransport};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Element, MessageEvent, Worker, WorkerOptions, WorkerType};

/// Whether this build starts in Worker mode when the page does not say.
const DEFAULT_WORKER: bool = cfg!(feature = "physics-worker-default") || !INLINE_AVAILABLE;

/// Whether this build links an engine, so a page can run its physics in process.
const INLINE_AVAILABLE: bool = cfg!(feature = "physics-engine");

/// The element the page named its worker on (the mount script sets
/// `data-physics-worker` to the script's URL on the tree element).
fn host() -> Option<Element> {
    web_sys::window()?
        .document()?
        .query_selector("[data-physics-worker]")
        .ok()
        .flatten()
}

fn requested(host: Option<&Element>) -> bool {
    let from_query = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| web_sys::UrlSearchParams::new_with_str(&search).ok())
        .and_then(|params| params.get("physics"));
    match from_query
        .or_else(|| host.and_then(|host| host.get_attribute("data-physics")))
        .as_deref()
    {
        Some("worker") => true,
        Some("inline") if !INLINE_AVAILABLE => {
            // This build has no engine to run in the page: say so, where a
            // person (the console) and a receipt (the element) can see it,
            // and use the Worker, the only physics there is.
            web_sys::console::warn_1(
                &"?physics=inline is not available: this build links no physics engine, so its physics runs in the Worker".into(),
            );
            if let Some(host) = host {
                let _ = host.set_attribute("data-physics-inline-ignored", "no engine in this build");
            }
            true
        },
        Some("inline") => false,
        _ => DEFAULT_WORKER,
    }
}

/// The transport for a canvas built now, when the page wants the Worker and it
/// starts; `None` is the inline backend. Records the answer in
/// `data-physics-backend`. A build with no engine never answers `None`: where
/// the Worker cannot start it answers a transport that fails closed and marks
/// the page's physics unavailable.
pub(crate) fn transport() -> Option<Box<dyn PhysicsTransport>> {
    let host = host();
    let say = |backend: &str| {
        if let Some(host) = &host {
            let _ = host.set_attribute("data-physics-backend", backend);
        }
    };
    if !requested(host.as_ref()) {
        say("inline");
        return None;
    }
    let url = host
        .as_ref()
        .and_then(|host| host.get_attribute("data-physics-worker"));
    let refused = |reason: String| {
        web_sys::console::error_1(&format!("physics worker did not start: {reason}").into());
        if INLINE_AVAILABLE {
            say("inline");
            None
        } else {
            mark_unavailable(host.as_ref(), &reason);
            Some(Box::new(UnavailableTransport::default()) as Box<dyn PhysicsTransport>)
        }
    };
    match url.as_deref().map(|url| Link::start(url, host.clone())) {
        Some(Ok(link)) => {
            say("worker");
            Some(Box::new(link))
        },
        Some(Err(error)) => refused(format!("{error:?}")),
        None => refused("no data-physics-worker on the element".to_owned()),
    }
}

/// The page has no physics: the element says so for `mount.js`'s notice and for
/// a receipt's probe.
fn mark_unavailable(host: Option<&Element>, reason: &str) {
    if let Some(host) = host {
        let _ = host.set_attribute("data-physics-unavailable", reason);
        let _ = host.set_attribute("data-physics-backend", "unavailable");
    }
}

/// A started Worker, its inbox, and the listeners that fill it.
struct Link {
    worker: Worker,
    inbox: Rc<RefCell<VecDeque<Vec<u8>>>>,
    /// Set when the Worker failed to load or died: nothing more is sent, and
    /// the backend is told, once, to fail closed.
    failed: Rc<RefCell<Option<UnavailableTransport>>>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(JsValue)>,
}

impl Link {
    fn start(url: &str, host: Option<Element>) -> Result<Self, JsValue> {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options(url, &options)?;
        let inbox = Rc::new(RefCell::new(VecDeque::new()));
        let failed: Rc<RefCell<Option<UnavailableTransport>>> = Rc::new(RefCell::new(None));
        let fail = {
            let failed = Rc::clone(&failed);
            let host = host.clone();
            move |reason: String| {
                web_sys::console::error_1(&format!("physics worker failed: {reason}").into());
                failed.borrow_mut().get_or_insert_with(UnavailableTransport::default);
                mark_unavailable(host.as_ref(), &reason);
            }
        };
        let sink = Rc::clone(&inbox);
        let fail_on_message = fail.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let data = event.data();
            if let Ok(bytes) = data.clone().dyn_into::<Uint8Array>() {
                sink.borrow_mut().push_back(bytes.to_vec());
            } else if let Some(reason) = data.as_string() {
                // The Worker script's own report that its wasm did not load.
                fail_on_message(reason);
            }
        });
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        let on_error = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
            let message = js_sys::Reflect::get(&event, &"message".into())
                .ok()
                .and_then(|message| message.as_string())
                .filter(|message| !message.is_empty())
                .unwrap_or_else(|| "the physics worker script did not load".to_owned());
            fail(message);
        });
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        Ok(Self {
            worker,
            inbox,
            failed,
            _on_message: on_message,
            _on_error: on_error,
        })
    }
}

impl PhysicsTransport for Link {
    fn send(&mut self, message: Vec<u8>) {
        if self.failed.borrow().is_some() {
            return;
        }
        let bytes = Uint8Array::from(message.as_slice());
        let transfer = Array::of1(&bytes.buffer());
        if let Err(error) = self.worker.post_message_with_transfer(&bytes, &transfer) {
            web_sys::console::error_2(&"physics worker post failed".into(), &error);
        }
    }

    fn receive(&mut self) -> Option<Vec<u8>> {
        if let Some(refusal) = self.failed.borrow_mut().as_mut() {
            return refusal.receive();
        }
        self.inbox.borrow_mut().pop_front()
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}
