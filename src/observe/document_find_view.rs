// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentFindView {
    pub target: Uuid,
    pub query: String,
    /// The field's caret as a grapheme index, and the text it selects.
    pub caret: usize,
    pub selection: String,
    pub count: usize,
    pub current: Option<usize>,
    pub pending: bool,
    pub status: String,
    pub current_role: Option<String>,
    pub current_label: Option<String>,
}
