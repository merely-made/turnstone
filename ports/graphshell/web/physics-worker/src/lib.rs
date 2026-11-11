// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The viewer's physics Worker: seiche's engine as its own wasm.
//!
//! `physics/worker.js` loads this module in a module Worker, after the page's
//! first paint, and drives it: [`PhysicsWorker::receive`] for each message the
//! page posts (the postcard bytes of a `seiche::wire::ToWorker`, transferred),
//! [`PhysicsWorker::interval`] on a 60 Hz timer while
//! [`PhysicsWorker::is_busy`], posting what comes back. The protocol, the
//! commands and the channel cache are `seiche::PhysicsWorker`'s; this crate
//! is the `wasm-bindgen` skin and the Worker's clock (`performance.now`).

use std::time::Duration;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// The Worker global's monotonic clock, in milliseconds.
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

fn clock() -> Duration {
    Duration::from_secs_f64(performance_now().max(0.0) / 1000.0)
}

/// One simulation, driven by messages.
#[wasm_bindgen]
pub struct PhysicsWorker(seiche::PhysicsWorker);

impl Default for PhysicsWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl PhysicsWorker {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        Self(seiche::PhysicsWorker::new(clock))
    }

    /// Apply one message from the page. Returns the answer to send back, if
    /// the message wanted one (`Init` is answered with `Ready`).
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Option<Vec<u8>>, JsError> {
        let replies = self
            .0
            .receive(bytes)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(replies.into_iter().next())
    }

    /// Run one interval if there is work and answer with the update it made:
    /// the postcard bytes of a `seiche::wire::FromWorker::Update`.
    pub fn interval(&mut self) -> Option<Vec<u8>> {
        self.0.interval()
    }

    /// Whether there are ticks to run: the script keeps its timer going while
    /// true and parks when not.
    pub fn is_busy(&self) -> bool {
        self.0.is_busy()
    }
}
