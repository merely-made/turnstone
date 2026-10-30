// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Withdraw only the physical window that owned this header gesture.
pub(in super::super) fn retire_outer_capture(
    capture: &mut Option<OuterCapture>,
    window: u64,
) -> bool {
    let owned = capture.as_ref().is_some_and(|capture| match capture {
        OuterCapture::Float { slot, .. }
        | OuterCapture::Tab { slot, .. }
        | OuterCapture::Divider { slot, .. } => *slot as u64 == window,
    });
    if owned {
        *capture = None;
    }
    owned
}
