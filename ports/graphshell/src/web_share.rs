// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The attached browser's realization of Copy link and Open in this browser.
//! Both start inside the user's gesture and report the browser's own answer;
//! a refused clipboard or tab is a visible failure, never a false success.
//! The clipboard is backstep's request/completion pair: the gesture hands
//! [`clipboard`] a [`ClipboardRequest`], the browser's promise answers with the
//! matching [`ClipboardEvent`].
use std::cell::RefCell;
use std::rc::Rc;

use backstep::{ClipboardError, ClipboardEvent, ClipboardRequest};
use wasm_bindgen::{JsCast, JsValue};

/// Where a finished copy or open reports its status line.
pub(crate) type Outcome = Rc<RefCell<Option<String>>>;

fn describe(error: JsValue) -> String {
    error
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .or_else(|| error.as_string())
        .unwrap_or_else(|| "the browser refused".into())
}

fn property(target: &JsValue, name: &str) -> Option<JsValue> {
    js_sys::Reflect::get(target, &JsValue::from_str(name))
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_null())
}

fn function(target: &JsValue, method: &str) -> Option<js_sys::Function> {
    property(target, method).and_then(|value| value.dyn_into::<js_sys::Function>().ok())
}

/// Call `target.method(args)`; a thrown error becomes `Err`.
fn call(target: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let function = function(target, method).ok_or_else(|| format!("{method} is unavailable here"))?;
    let args: js_sys::Array = args.iter().collect();
    function.apply(target, &args).map_err(describe)
}

fn settle(promise: JsValue, outcome: Outcome, done: String, failed: String) {
    let promise = js_sys::Promise::resolve(&promise);
    wasm_bindgen_futures::spawn_local(async move {
        let status = match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(_) => done,
            Err(error) => format!("{failed} · {}", describe(error)),
        };
        *outcome.borrow_mut() = Some(status);
    });
}

/// A browser refusal (no gesture, no permission) is `Refused`; any other
/// failure leaves the clipboard `Unavailable`.
fn clipboard_error(error: &JsValue) -> ClipboardError {
    match property(error, "name").and_then(|name| name.as_string()).as_deref() {
        Some("NotAllowedError" | "SecurityError") => ClipboardError::Refused,
        _ => ClipboardError::Unavailable,
    }
}

/// Carry a clipboard `request`; must start inside the user's gesture.
/// `complete` receives the matching [`ClipboardEvent`] once the browser's
/// promise settles, or at once when the page has no clipboard or the call
/// throws.
pub(crate) fn clipboard(request: ClipboardRequest, complete: impl FnOnce(ClipboardEvent) + 'static) {
    let (method, args) = match &request {
        ClipboardRequest::Read => ("readText", js_sys::Array::new()),
        ClipboardRequest::Write(text) => ("writeText", js_sys::Array::of1(&JsValue::from_str(text))),
    };
    let Some((clipboard, function)) = crate::window()
        .ok()
        .and_then(|window| property(&window.navigator(), "clipboard"))
        .and_then(|clipboard| function(&clipboard, method).map(|function| (clipboard, function)))
    else {
        complete(request.fail(ClipboardError::Unavailable));
        return;
    };
    let promise = match function.apply(&clipboard, &args) {
        Ok(promise) => js_sys::Promise::resolve(&promise),
        Err(error) => {
            complete(request.fail(clipboard_error(&error)));
            return;
        },
    };
    wasm_bindgen_futures::spawn_local(async move {
        complete(match wasm_bindgen_futures::JsFuture::from(promise).await {
            Err(error) => request.fail(clipboard_error(&error)),
            Ok(_) if matches!(request, ClipboardRequest::Write(_)) => ClipboardEvent::Written(Ok(())),
            Ok(text) => ClipboardEvent::Read(text.as_string().ok_or(ClipboardError::Empty)),
        });
    });
}

/// Open `url` in a new tab of the browser the extension runs in.
pub(crate) fn open_tab(
    url: &str,
    outcome: Outcome,
    done: String,
    failed: String,
) -> Result<(), String> {
    let global = js_sys::global();
    let tabs = property(&global, "browser")
        .or_else(|| property(&global, "chrome"))
        .and_then(|api| property(&api, "tabs"))
        .ok_or("this page is not attached to a browser extension")?;
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"url".into(), &url.into()).map_err(describe)?;
    js_sys::Reflect::set(&options, &"active".into(), &true.into()).map_err(describe)?;
    let promise = call(&tabs, "create", &[options.into()])?;
    settle(promise, outcome, done, failed);
    Ok(())
}
