// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl FetchedPage {
    pub fn text(content_type: Option<String>, body: impl Into<String>) -> Self {
        let body = body.into();
        let bytes = body.as_bytes().to_vec();
        Self {
            content_type,
            content_disposition: None,
            bytes,
            body,
        }
    }
}
