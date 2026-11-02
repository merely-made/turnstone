// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The requested page zoom as a display value, with the engine's effective
/// percentage beside it when the lane reports one. Retained sessions answer a
/// zoom request with what they actually present; a hosted surface has no
/// read-back, so it shows the request alone rather than a fabricated echo.
pub fn page_zoom_display(requested: Option<f32>, applied: Option<f32>) -> String {
    let requested = page_zoom_percent(requested);
    match applied {
        Some(applied) => format!(
            "{requested}% (applied {}%)",
            page_zoom_percent(Some(applied))
        ),
        None => format!("{requested}%"),
    }
}
