// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A currently authorized collection offered by the local search picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceCollectionChoice {
    pub name: String,
    pub version: PlaceCollectionVersion,
}
