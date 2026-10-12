// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The supplying application's read and action bridge. No peer authority here.
use super::*;
use graphshell::moot_resources::Host;
use moot::resource_coop::{Outcome, Snapshot};
fn with_host<R>(f: impl FnOnce(&mut Host) -> Result<R, String>) -> Result<R, JsValue> {
    TREE.with(|tree| {
        let tree = tree.borrow();
        let tree = tree.as_ref().ok_or("Mount Graphshell first")?;
        let result = f(&mut tree.shared.moot_resources.borrow_mut());
        tree.window.request_redraw();
        result
    })
    .map_err(|error: String| JsValue::from_str(&error))
}
#[wasm_bindgen]
pub fn moot_resource_read(id: u32, json: &str) -> Result<(), JsValue> {
    let snapshot: Snapshot =
        serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_host(|host| host.install(id, snapshot))
}
#[wasm_bindgen]
pub fn moot_resource_close(id: u32) -> Result<(), JsValue> {
    with_host(|host| {
        host.close(id);
        Ok(())
    })
}
#[wasm_bindgen]
pub fn moot_resource_commands() -> Result<String, JsValue> {
    with_host(|host| serde_json::to_string(&host.take_commands()).map_err(|e| e.to_string()))
}
#[wasm_bindgen]
pub fn moot_resource_prepare_dispatch(id: u32, token: u32, current: &str) -> Result<(), JsValue> {
    let snapshot: Snapshot =
        serde_json::from_str(current).map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_host(|host| host.prepare_dispatch(id, token, &snapshot))
}
#[wasm_bindgen]
pub fn moot_resource_failed(id: u32, token: u32, reason: &str) -> Result<bool, JsValue> {
    with_host(|host| Ok(host.fail(id, token, reason.into())))
}
#[wasm_bindgen]
pub fn moot_resource_complete(id: u32, token: u32, json: &str) -> Result<bool, JsValue> {
    let outcome: Outcome =
        serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_host(|host| Ok(host.complete(id, token, outcome)))
}
#[wasm_bindgen]
pub fn moot_resource_refreshed(id: u32, token: u32, json: &str) -> Result<bool, JsValue> {
    let snapshot: Snapshot =
        serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_host(|host| host.complete_refresh(id, token, snapshot))
}
#[wasm_bindgen]
pub fn moot_resource_opened(id: u32, token: u32, json: &str, text: &str) -> Result<bool, JsValue> {
    let outcome: Outcome =
        serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_host(|host| Ok(host.complete_open(id, token, outcome, text.into())))
}
pub(super) fn section(page: &TreePage) -> Child {
    let host = page.shared.moot_resources.borrow();
    let mut children: Vec<Child> = vec![];
    for (&id, reading) in &host.readings {
        let mut reading_children: Vec<Child> = vec![];
        reading_children.push(Box::new(el("h2", format!("Community reading {id}"))));
        reading_children.push(Box::new(moot::resource_coop::view::view(
            &format!("community-{id}"),
            reading,
            move |page: &mut TreePage, intent| {
                page.shared.moot_resources.borrow_mut().begin(id, intent);
            },
        )));
        if let Some((reference, text)) = host.opened.get(&id) {
            reading_children.push(Box::new(el(
                "h3",
                format!("Opened content in reading {id}"),
            )));
            reading_children.push(Box::new(el(
                "p",
                format!(
                    "Exact manifest {}",
                    moot::resource_coop::view::hex(reference.manifest)
                ),
            )));
            reading_children.push(Box::new(
                el("pre", text.clone()).attr("class", "moot-opened-content"),
            ));
        }
        reading_children.push(Box::new(cambium::button(
            format!("Close community reading {id}"),
            move |page: &mut TreePage, _| {
                page.shared.moot_resources.borrow_mut().close(id);
            },
        )));
        children.push(Box::new(
            el("section", reading_children)
                .attr("role", "region")
                .attr("aria-label", format!("Community reading {id}"))
                .attr("id", format!("community-reading-{id}")),
        ));
    }
    Box::new(el("section", children).attr("class", "tools-section tree-inspection tree-community"))
}
pub(super) fn fields(page: &TreePage, mut snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let host = page.shared.moot_resources.borrow();
    snapshot = snapshot.with_field("community-readings", host.readings.len().to_string());
    for (&id, reading) in &host.readings {
        let pane = &reading.pane;
        snapshot = snapshot
            .with_field(
                format!("community-{id}-opened"),
                host.opened.contains_key(&id).to_string(),
            )
            .with_field(
                format!("community-{id}-resource"),
                pane.selected_resource()
                    .map(|r| moot::resource_coop::view::hex(r.contribution))
                    .unwrap_or_default(),
            )
            .with_field(
                format!("community-{id}-activity"),
                pane.selected_activity()
                    .map(|r| r.activity.clone())
                    .unwrap_or_default(),
            )
            .with_field(
                format!("community-{id}-pending"),
                pane.pending().is_some().to_string(),
            )
            .with_field(
                format!("community-{id}-outcome"),
                match pane.outcome() {
                    Some(Outcome::Accepted { .. }) => "accepted",
                    Some(Outcome::Refused { .. }) => "refused",
                    Some(Outcome::Unsupported { .. }) => "unsupported",
                    None => "none",
                },
            )
            .with_field(
                format!("community-{id}-resources"),
                pane.snapshot().resources.len().to_string(),
            );
    }
    snapshot
}
