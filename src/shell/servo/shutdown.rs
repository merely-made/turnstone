// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Release the process root after all producers and imported caches are gone.
/// Reinitialization is refused for the remainder of this process.
pub(crate) fn shutdown() -> Result<(), SurfaceError> {
    if let Some(owner) = PROCESS_OWNER.get() {
        require_owner(*owner)?;
    }
    ROOT.with(|slot| {
        if slot
            .borrow()
            .as_ref()
            .is_some_and(|root| root.views.get() != 0)
        {
            return Err(SurfaceError::Busy {
                operation: "Servo shutdown with live views".into(),
            });
        }
        let root = slot.borrow_mut().take();
        drop(root);
        Ok(())
    })
}
