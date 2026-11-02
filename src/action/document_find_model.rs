// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentFindModel {
    /// Authoritative count, including engine-managed matches that do not carry
    /// host-visible structural descriptions.
    pub count: usize,
    pub matches: Vec<DocumentFindMatch>,
    pub current: Option<usize>,
    pub complete: bool,
}
