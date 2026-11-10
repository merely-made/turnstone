// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Native host for guest block adapters: the app's side of the review.
//!
//! A guest block adapter is a component pack reviewed once ([`CapsuleMount`], the
//! `power:block-adapter` grant) and run in process under wasmtime by `block-adapter`'s
//! `GuestAdapter`, which reviews nothing. [`attach_reviewed`] is the check between the two.

use block_adapter::{GuestAdapter, GuestEngine};
use graphshell::capsule_applet::{BLOCK_ADAPTER, CapsuleMount};

/// Instantiate the component a reviewed mount approved. The mount must be a block adapter
/// holding the `power:block-adapter` grant, and `bytes` must be exactly the reviewed
/// component.
pub fn attach_reviewed(
    engine: &GuestEngine,
    mount: &CapsuleMount,
    bytes: &[u8],
) -> Result<GuestAdapter, String> {
    if !mount.is_block_adapter() || !mount.permits(BLOCK_ADAPTER) {
        return Err("This component has not been approved as a block adapter".into());
    }
    if !mount.verify_component(bytes) {
        return Err("The component is not the one that was reviewed".into());
    }
    GuestAdapter::attach(engine, bytes, &mount.granted())
}
