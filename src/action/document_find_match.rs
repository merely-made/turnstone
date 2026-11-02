// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentFindMatch {
    pub label: String,
    pub role: Option<String>,
}
