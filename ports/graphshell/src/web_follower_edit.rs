// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page's side of the follower edit queue. The extension page owns the
//! broadcast channel, the resend and the timeout; this module hands it a
//! request to send and hands the app what comes back, as DOM events carrying
//! JSON.
//!
//! - follower to page: `graphshell-edit-submit`, a [`Request`] without its view.
//! - page to follower: `graphshell-edit-reply`, a [`Reply`].
//! - page to writer: `graphshell-edit-admit`, a [`Request`].
//! - writer to page: `graphshell-edit-admitted`, a [`Reply`].
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use graphshell::follower_edit::{Reply, Request};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

const SUBMIT_EVENT: &str = "graphshell-edit-submit";
const REPLY_EVENT: &str = "graphshell-edit-reply";
const ADMIT_EVENT: &str = "graphshell-edit-admit";
const ADMITTED_EVENT: &str = "graphshell-edit-admitted";
/// More waiting than this means a misbehaving page; the rest are refused.
pub(crate) const MAX_WAITING: usize = 16;

pub(crate) type Wake = Rc<RefCell<Option<cambium_rootstock::HostWake>>>;

pub(crate) struct Listener {
    document: web_sys::Document,
    name: &'static str,
    callback: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .document
            .remove_event_listener_with_callback(self.name, self.callback.as_ref().unchecked_ref());
    }
}
/// What arrived, in order: parsed, or the id it carried and why it did not parse.
pub(crate) type Inbox<T> = Rc<RefCell<VecDeque<Result<T, (String, String)>>>>;

fn detail(event: &web_sys::Event) -> String {
    js_sys::Reflect::get(event, &JsValue::from_str("detail"))
        .ok()
        .and_then(|detail| detail.as_string())
        .unwrap_or_default()
}

fn id_of(json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|value| Some(value.get("id")?.as_str()?.to_owned()))
        .unwrap_or_default()
}

fn listen<T: 'static>(
    name: &'static str,
    wake: Wake,
    parse: fn(&str) -> Result<T, String>,
) -> Result<(Inbox<T>, Listener), String> {
    let inbox: Inbox<T> = Rc::default();
    let queue = inbox.clone();
    let listener = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let json = detail(&event);
        let mut queue = queue.borrow_mut();
        let parsed = if queue.len() >= MAX_WAITING {
            Err((id_of(&json), "too many edits waiting".to_owned()))
        } else {
            parse(&json).map_err(|error| (id_of(&json), error))
        };
        queue.push_back(parsed);
        if let Some(wake) = wake.borrow().as_ref() {
            wake.wake();
        }
    });
    let document = crate::document()?;
    document
        .add_event_listener_with_callback(name, listener.as_ref().unchecked_ref())
        .map_err(|_| format!("could not listen for {name}"))?;
    Ok((
        inbox,
        Listener {
            document,
            name,
            callback: listener,
        },
    ))
}

/// Replies to the edits this follower sent.
pub(crate) fn listen_for_replies(wake: Wake) -> Result<(Inbox<Reply>, Listener), String> {
    listen(REPLY_EVENT, wake, Reply::parse)
}

/// Edits followers sent to this writer.
pub(crate) fn listen_for_requests(wake: Wake) -> Result<(Inbox<Request>, Listener), String> {
    listen(ADMIT_EVENT, wake, Request::parse)
}

fn announce(name: &str, json: &str) -> Result<(), String> {
    let event = web_sys::Event::new(name).map_err(|_| format!("could not create {name}"))?;
    js_sys::Reflect::set(
        &event,
        &JsValue::from_str("detail"),
        &JsValue::from_str(json),
    )
    .map_err(|_| format!("could not carry {name}"))?;
    crate::document()?
        .dispatch_event(&event)
        .map_err(|_| format!("could not announce {name}"))?;
    Ok(())
}

/// Hand a request to the page to send to the writer.
pub(crate) fn submit(request: &Request) -> Result<(), String> {
    announce(
        SUBMIT_EVENT,
        &serde_json::to_string(request).map_err(|error| error.to_string())?,
    )
}

/// Hand the page the writer's answer to send back.
pub(crate) fn admitted(reply: &Reply) -> Result<(), String> {
    announce(ADMITTED_EVENT, &reply.to_json())
}

/// This view's name among the views of the profile, set by the page.
pub(crate) fn view_id() -> String {
    crate::window()
        .ok()
        .and_then(|window| {
            js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("graphshellViewId")).ok()
        })
        .and_then(|value| value.as_string())
        .unwrap_or_default()
}
