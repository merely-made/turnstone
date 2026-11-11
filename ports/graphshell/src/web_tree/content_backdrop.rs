// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::{Child, TreePage, el};
use cambium::{button, focusable_if, on_click};

#[derive(Default)]
pub(super) struct Controls {
    url: Option<String>,
    status: Option<String>,
}

impl Drop for Controls {
    fn drop(&mut self) {
        if let Some(url) = self.url.take() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    }
}

pub(super) fn append_controls(page: &TreePage, buttons: &mut Vec<Child>) {
    let canvas = page.shared.canvas.borrow();
    let enabled = canvas.content_backdrop_enabled();
    let awaiting_layout = enabled && !canvas.physics_paused() && canvas.layout_strategy().is_none();
    drop(canvas);
    buttons.push(Box::new(
        button("Content backdrop", |page: &mut TreePage, _| {
            if page.resident_mere_shown() {
                return;
            }
            let mut canvas = page.shared.canvas.borrow_mut();
            let enabled = !canvas.content_backdrop_enabled();
            canvas.set_content_backdrop(enabled);
            page.content_backdrop.status = None;
            page.shared.dirty.set(true);
        })
        .attr("class", "content-backdrop-toggle")
        .attr("aria-pressed", if enabled { "true" } else { "false" }),
    ));
    let mut export = focusable_if(
        on_click(
            el("button", "Export backdrop SVG"),
            |page: &mut TreePage, _| {
                if page.resident_mere_shown() {
                    return;
                }
                let canvas = page.shared.canvas.borrow();
                if !canvas.content_backdrop_enabled() {
                    return;
                }
                let svg = canvas.content_backdrop_svg();
                let result = match svg {
                    Ok(Some(svg)) => super::download::export_material(
                        svg.as_bytes(),
                        "image/svg+xml",
                        "content-backdrop.svg",
                    ),
                    Ok(None) => {
                        Err("The current layout has no rendered backdrop to export.".into())
                    },
                    Err(error) => Err(error.to_string()),
                };
                page.content_backdrop.status = Some(match result {
                    Ok(url) => {
                        if let Some(old) = page.content_backdrop.url.replace(url) {
                            let _ = web_sys::Url::revoke_object_url(&old);
                        }
                        "Backdrop SVG handed to the browser.".into()
                    },
                    Err(error) => error,
                });
            },
        ),
        enabled,
    );
    if !enabled {
        export = export.attr("disabled", "").attr("aria-disabled", "true");
    }
    buttons.push(Box::new(export));
    if let Some(status) = page.content_backdrop.status.as_deref().or(awaiting_layout
        .then_some("Pause physics or select a held arrangement to show the backdrop."))
    {
        buttons.push(Box::new(
            el("span", status.to_owned()).attr("role", "status"),
        ));
    }
}
