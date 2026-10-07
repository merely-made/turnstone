// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Headed browser presenter for the Graphshell reference host.
//!
//! The whole crate is browser-only: it presents onto an
//! `HtmlCanvasElement` through WebGPU and stores through IndexedDB, neither
//! of which exists off wasm. The crate-level `cfg` below makes it compile to
//! nothing on a native host, so `cargo check --workspace` — the gate that
//! covers every other member — is not permanently red for a target this
//! crate was never meant to build for. It stays a workspace member rather
//! than an `exclude`d one so it keeps sharing the workspace lock and the
//! root `[patch]` table; excluding it would mean maintaining a second copy
//! of both.
//!
//! Check it for the target it is for:
//! `cargo check -p graphshell-web --target wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

// The viewer cone (mer3ly site canvas plan, Rulings 109-110, 118): the tree page
// and what it reads are always compiled; the H5 reference host, the product
// modes and the remote board are additive features, all on by default.
#[cfg(feature = "main-page")]
mod web_events;
mod web_gpu;
mod web_graphs;
#[cfg(feature = "main-page")]
mod web_practice;
#[cfg(feature = "main-page")]
mod web_product;
#[cfg(feature = "main-page")]
mod web_projection;
#[cfg(feature = "main-page")]
mod web_remote;
#[cfg(feature = "remote")]
mod web_rtc_link;
mod web_scenario;
mod web_speed;
mod web_timing;
mod web_tree;
#[cfg(feature = "main-page")]
mod web_view;

#[cfg(feature = "product")]
use graphshell::browser_storage::{StoragePersistence, decide};
use wasm_bindgen::prelude::*;
#[cfg(feature = "product")]
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, Window};

/// Ask the browser whether this origin's storage is kept, requesting it when
/// it is not.
///
/// Every failure path lands on `Unknown` with its reason rather than on
/// `Refused`. An insecure context and a browser that declined are different
/// facts, and only one of them changes if the person installs the resident
/// host.
#[cfg(feature = "product")]
async fn resolve_storage_persistence() -> StoragePersistence {
    let Ok(window) = window() else {
        return StoragePersistence::Unknown("browser window is unavailable".to_string());
    };
    let manager = window.navigator().storage();
    let persisted = match manager.persisted() {
        Ok(promise) => match JsFuture::from(promise).await {
            Ok(value) => value
                .as_bool()
                .ok_or_else(|| "persisted() did not answer with a boolean".to_string()),
            Err(error) => Err(format!("persisted() failed: {error:?}")),
        },
        Err(error) => Err(format!("storage persistence is unavailable: {error:?}")),
    };
    // `decide` takes the request as a closure so it is never made when the
    // answer is already yes; awaiting inside one needs the future built first.
    let requested = if matches!(persisted, Ok(false)) {
        match manager.persist() {
            Ok(promise) => match JsFuture::from(promise).await {
                Ok(value) => value
                    .as_bool()
                    .ok_or_else(|| "persist() did not answer with a boolean".to_string()),
                Err(error) => Err(format!("persist() failed: {error:?}")),
            },
            Err(error) => Err(format!("persist() is unavailable: {error:?}")),
        }
    } else {
        Ok(false)
    };
    decide(persisted, move || requested)
}

fn window() -> Result<Window, String> {
    web_sys::window().ok_or_else(|| "browser window is unavailable".to_string())
}

fn document() -> Result<Document, String> {
    window()?
        .document()
        .ok_or_else(|| "browser document is unavailable".to_string())
}

#[wasm_bindgen(start)]
pub fn start() {
    // First, before any other Rust code: pre.4 CubeCL's static constructors,
    // once (burn migration plan §13.33; rulings 532 and 536).
    cambium_genet_web_host::run_static_constructors_once();
    console_error_panic_hook::set_once();
}

// The H5 reference host and its `mount` export, verbatim in their own file.
#[cfg(feature = "main-page")]
include!("web_main.rs");
