// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use wasm_bindgen::JsCast;

pub(super) fn export_material(
    bytes: &[u8],
    media_type: &str,
    filename: &str,
) -> Result<String, String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("No browser document")?;
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(media_type);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("Cannot prepare download: {e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Cannot create download URL: {e:?}"))?;
    let prepare = || -> Result<(), String> {
        let anchor: web_sys::HtmlElement = document
            .create_element("a")
            .map_err(|e| format!("Cannot create download: {e:?}"))?
            .dyn_into()
            .map_err(|_| "Download element unavailable")?;
        anchor
            .set_attribute("href", &url)
            .map_err(|e| format!("Cannot set download URL: {e:?}"))?;
        anchor
            .set_attribute("download", filename)
            .map_err(|e| format!("Cannot name download: {e:?}"))?;
        document
            .body()
            .ok_or("No document body")?
            .append_child(&anchor)
            .map_err(|e| format!("Cannot attach download: {e:?}"))?;
        anchor.click();
        anchor.remove();
        Ok(())
    };
    if let Err(error) = prepare() {
        let _ = web_sys::Url::revoke_object_url(&url);
        return Err(error);
    }
    Ok(url)
}
