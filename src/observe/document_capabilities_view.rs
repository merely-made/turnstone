// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentCapabilitiesView {
    pub find_in_page: String,
    pub page_zoom: String,
    pub page_capture: String,
    pub navigation: String,
}
