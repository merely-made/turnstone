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
//! page. Neither is the build's default (`physics-worker-default`, off until
//! the headed receipts passed in Worker mode). A page that asks for the Worker
//! and cannot start it keeps the inline backend and says which it has in
//! `data-physics-backend` on the element, so a receipt can check.
//!
//! Messages are the postcard bytes `seiche::wire` writes, one per
//! `postMessage`, each in a buffer that is transferred. The Worker is
//! started when the canvas is built (the canvas queues what it sends, in
//! order, until the Worker says `Ready`), which is on first paint's
//! heels and not on the first drag.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use js_sys::{Array, Uint8Array};
use mere::canvas::PhysicsTransport;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Element, MessageEvent, Worker, WorkerOptions, WorkerType};

/// Whether this build starts in Worker mode when the page does not say.
const DEFAULT_WORKER: bool = cfg!(feature = "physics-worker-default");

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
        Some("inline") => false,
        _ => DEFAULT_WORKER,
    }
}

/// The transport for a canvas built now, when the page wants the Worker and it
/// starts; `None` is the inline backend. Records the answer in
/// `data-physics-backend`.
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
    match url.as_deref().map(Link::start) {
        Some(Ok(link)) => {
            say("worker");
            Some(Box::new(link))
        },
        Some(Err(error)) => {
            web_sys::console::error_1(&format!("physics worker did not start: {error:?}").into());
            say("inline");
            None
        },
        None => {
            web_sys::console::error_1(&"physics worker requested, no data-physics-worker".into());
            say("inline");
            None
        },
    }
}

/// A started Worker, its inbox, and the listeners that fill it.
struct Link {
    worker: Worker,
    inbox: Rc<RefCell<VecDeque<Vec<u8>>>>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(JsValue)>,
}

impl Link {
    fn start(url: &str) -> Result<Self, JsValue> {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options(url, &options)?;
        let inbox = Rc::new(RefCell::new(VecDeque::new()));
        let sink = Rc::clone(&inbox);
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Ok(bytes) = event.data().dyn_into::<Uint8Array>() {
                sink.borrow_mut().push_back(bytes.to_vec());
            }
        });
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        let on_error = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
            web_sys::console::error_2(&"physics worker error".into(), &event);
        });
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        Ok(Self {
            worker,
            inbox,
            _on_message: on_message,
            _on_error: on_error,
        })
    }
}

impl PhysicsTransport for Link {
    fn send(&mut self, message: Vec<u8>) {
        let bytes = Uint8Array::from(message.as_slice());
        let transfer = Array::of1(&bytes.buffer());
        if let Err(error) = self.worker.post_message_with_transfer(&bytes, &transfer) {
            web_sys::console::error_2(&"physics worker post failed".into(), &error);
        }
    }

    fn receive(&mut self) -> Option<Vec<u8>> {
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
