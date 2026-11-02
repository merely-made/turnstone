// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn pump() -> Result<(), SurfaceError> {
    if let Some(owner) = PROCESS_OWNER.get() {
        require_owner(*owner)?;
    }
    let root = ROOT.with(|slot| slot.borrow().clone());
    if let Some(root) = root {
        root.servo.spin_event_loop();
    }
    Ok(())
}
