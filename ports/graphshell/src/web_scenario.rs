// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Publish Mesquite's retained-host result and original capture pixels.
//! All scenario stepping belongs to the shared lane; this module owns only
//! browser encoding and page-level receipt elements outside the component.

use wasm_bindgen::{Clamped, JsCast};
use web_sys::{CanvasRenderingContext2d, Element, HtmlCanvasElement, ImageData};
use super::document;

/// Write `data-scenario` on the body and, when given, the result line.
pub(super) fn mark(
    document: &web_sys::Document,
    state: &str,
    result: Option<&str>,
) -> Result<(), String> {
    let body = document.body().ok_or("document has no body")?;
    body.set_attribute("data-scenario", state)
        .map_err(|_| "could not mark the scenario state")?;
    if let Some(result) = result {
        page_element(document, "scenario-result")?.set_text_content(Some(result));
    }
    Ok(())
}

/// A page-level element (the lane's own output lives outside the component).
pub(super) fn page_element(document: &web_sys::Document, id: &str) -> Result<Element, String> {
    document
        .get_element_by_id(id)
        .ok_or_else(|| format!("missing #{id}"))
}

/// Finish a capture whose readback has landed: encode the pixels as a PNG
/// through a 2D canvas (the browser's encoder, so no image dependency) and
/// hang the data URL off the DOM where a receipt can collect it.
pub(super) fn publish_capture(
    name: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<(), String> {
    let document = document()?;
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|_| "could not create the capture canvas")?
        .dyn_into()
        .map_err(|_| "capture canvas is not a canvas")?;
    canvas.set_width(width);
    canvas.set_height(height);
    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .ok_or("no 2d context for the capture")?
        .dyn_into()
        .map_err(|_| "capture context is not 2d")?;
    let image = ImageData::new_with_u8_clamped_array_and_sh(Clamped(rgba), width, height)
        .map_err(|_| "could not build the capture image")?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|_| "could not paint the capture")?;
    let url = canvas
        .to_data_url()
        .map_err(|_| "could not encode the capture")?;
    let img = document
        .create_element("img")
        .map_err(|_| "could not create the capture image")?;
    img.set_attribute("data-capture", name)
        .and_then(|_| img.set_attribute("width", &width.to_string()))
        .and_then(|_| img.set_attribute("height", &height.to_string()))
        .and_then(|_| img.set_attribute("alt", &format!("capture {name}")))
        .and_then(|_| img.set_attribute("src", &url))
        .map_err(|_| "could not describe the capture image")?;
    page_element(&document, "scenario-captures")?
        .append_child(&img)
        .map_err(|_| "could not publish the capture")?;
    Ok(())
}
