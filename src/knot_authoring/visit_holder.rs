// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Open one visiting hub over a prepared dial.
///
/// The shell's entry point to [`KnotHub::visit`]: the hub itself stays
/// private, and what the shell holds is a handle the engine can be given.
#[cfg(feature = "peers")]
pub(crate) fn visit_holder(
    dial: crate::place::lanes::HolderDial,
    wake: Wake,
) -> Result<Arc<KnotHub>, String> {
    KnotHub::visit(dial, wake)
}

#[cfg(not(feature = "peers"))]
pub(crate) fn visit_holder(
    dial: crate::place::lanes::HolderDial,
    _: Wake,
) -> Result<Arc<KnotHub>, String> {
    match dial {}
}
