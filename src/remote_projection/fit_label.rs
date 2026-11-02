// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The catalogue's label for fitting the view.
pub(super) fn fit_label() -> &'static str {
    cambium::catalogue::label(cambium::catalogue::ids::VIEW_FIT).unwrap_or("Fit to view")
}
