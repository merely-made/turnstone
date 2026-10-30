// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for DocumentCapabilityFacts {
    fn default() -> Self {
        let unavailable = |feature: &str| CapabilityStatus::Unsupported {
            reason: format!("{feature} capability was not reported"),
        };
        Self {
            find_in_page: unavailable("find in page"),
            page_zoom: unavailable("page zoom"),
            page_capture: unavailable("page capture"),
            navigation: unavailable("navigation"),
        }
    }
}
