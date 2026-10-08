// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page's host dataset: the one input both the viewer and the practice
//! proof read (mer3ly site canvas plan, S1; Rulings 124 and 125).
//!
//! A page supplies a `scenomise.host-dataset/v1` envelope as the root's
//! `data-dataset` attribute. `loader.js` fills that attribute before mounting
//! from `?dataset=<url>` or, failing that, the root's `data-dataset-src`; a
//! fetch it could not make arrives as `data-dataset-error` instead. Either
//! way the text is parsed strictly here, and a refusal is the page's to show.

use graphshell::projection_compile::HostDatasetV1;
use web_sys::Element;

/// The envelope's text, on the mounted root.
pub(crate) const DATASET_ATTRIBUTE: &str = "data-dataset";
/// Why `loader.js` could not fetch the named dataset.
pub(crate) const DATASET_ERROR_ATTRIBUTE: &str = "data-dataset-error";

/// The host dataset `root` was given, if any, parsed and checked.
pub(crate) fn supplied(root: &Element) -> Option<Result<HostDatasetV1, String>> {
    if let Some(error) = root.get_attribute(DATASET_ERROR_ATTRIBUTE) {
        return Some(Err(format!("Dataset refused: {error}")));
    }
    let text = root.get_attribute(DATASET_ATTRIBUTE)?;
    Some(
        graphshell::projection_compile::parse_host_dataset(&text)
            .map_err(|error| format!("Dataset refused: {error}")),
    )
}
