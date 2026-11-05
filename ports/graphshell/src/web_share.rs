// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The attached browser's realization of Copy link and Open in this browser.
//! Both start inside the user's gesture and report the browser's own answer;
//! a refused clipboard or tab is a visible failure, never a false success.
use std::cell::RefCell;
use std::rc::Rc;

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

/// Call `target.method(args)`; a thrown error becomes `Err`.
fn call(target: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let function = property(target, method)
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        .ok_or_else(|| format!("{method} is unavailable here"))?;
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

/// Start writing `text` to the clipboard; must run inside a user gesture.
pub(crate) fn copy(
    text: &str,
    outcome: Outcome,
    done: String,
    failed: String,
) -> Result<(), String> {
    let window = crate::window()?;
    let clipboard = property(&window.navigator(), "clipboard")
        .ok_or("the clipboard is unavailable on this page")?;
    let promise = call(&clipboard, "writeText", &[JsValue::from_str(text)])?;
    settle(promise, outcome, done, failed);
    Ok(())
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
