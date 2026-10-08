// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Storing the session's changes without holding the host across an await
//! (Scenograph editor plan, SE22).
//!
//! The host lives in an `Rc<RefCell<…>>` that every handler and frame borrows,
//! and IndexedDB writes await. So a store runs in three steps: under the
//! borrow, take the pending batch; without it, write the batch through a clone
//! of the store; under it again, mark the batch stored. One write is in flight
//! at a time, and changes made meanwhile wait for the next batch.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen_futures::spawn_local;
use web_sys::Event;

use super::{BrowserHost, root};

/// Where the session's changes stand, as the page's `data-session-store` token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SessionStore {
    Stored,
    Pending,
    Writing,
    Failed,
}

impl SessionStore {
    /// The page's token. `unstored` is a settled state with changes nobody
    /// asked to store, such as a scene save (Scenograph editor plan, E2b
    /// findings), so the token never claims what the store does not hold.
    pub(super) fn token(self, unstored: bool) -> &'static str {
        match self {
            Self::Stored if unstored => "unstored",
            Self::Stored => "stored",
            Self::Pending => "pending",
            Self::Writing => "writing",
            Self::Failed => "failed",
        }
    }

    /// Whether the frame pump should keep running until this settles.
    pub(super) fn busy(self) -> bool {
        matches!(self, Self::Pending | Self::Writing)
    }
}

/// Start a write if changes are pending and none is in flight. Called by the
/// frame pump once it has let go of the host.
pub(super) fn store_pending(state: &Rc<RefCell<BrowserHost>>) {
    let (staged, store) = {
        let mut host = state.borrow_mut();
        if host.session_store != SessionStore::Pending {
            return;
        }
        let now_secs = (js_sys::Date::now() / 1_000.0) as u64;
        match host.app.host.prepare_store(now_secs) {
            Ok(staged) => {
                host.session_store = SessionStore::Writing;
                (staged, host.app.host.store())
            },
            Err(error) => {
                host.session_store = SessionStore::Failed;
                host.session_store_error = error.to_string();
                return;
            },
        }
    };
    let state = state.clone();
    spawn_local(async move {
        let written = staged.write(&store).await;
        {
            let mut host = state.borrow_mut();
            match written {
                Ok(()) => {
                    host.app.host.staged(staged);
                    // A change made during the write is still pending.
                    host.session_store = if host.app.host.has_unstored() {
                        SessionStore::Pending
                    } else {
                        SessionStore::Stored
                    };
                },
                Err(error) => {
                    host.session_store = SessionStore::Failed;
                    host.session_store_error = error.to_string();
                },
            }
            host.chrome_dirty = true;
        }
        wake();
    });
}

/// Ask the frame pump for a frame, without touching the host.
fn wake() {
    if let Ok(root) = root()
        && let Ok(event) = Event::new("graphshell-wake")
    {
        let _ = root.dispatch_event(&event);
    }
}
