// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page's host dataset: the one input both the viewer and the practice
//! proof read (mer3ly site canvas plan, S1 and S3; Rulings 124, 125, 144 and
//! 148).
//!
//! A page supplies the envelope as the root's `data-dataset` attribute. The
//! mount script fills that attribute before mounting from `?dataset=<url>`
//! or, failing that, the root's `data-dataset-src`; a fetch it could not make
//! arrives as `data-dataset-error` instead. Either way the text is parsed
//! strictly here, and a refusal is the page's to show. The viewer reads a
//! `scenomise.host-dataset/v2` history, a v1 envelope being its one
//! revision; the practice proof reads one v1 dataset.

#[cfg(feature = "main-page")]
use graphshell::projection_compile::HostDatasetV1;
use graphshell::projection_compile::HostDatasetV2;
use web_sys::Element;

/// The envelope's text, on the mounted root.
pub(crate) const DATASET_ATTRIBUTE: &str = "data-dataset";
/// Why the mount script could not fetch the named dataset.
pub(crate) const DATASET_ERROR_ATTRIBUTE: &str = "data-dataset-error";

/// The envelope text `root` was given, or why it has none to give.
fn supplied_text(root: &Element) -> Option<Result<String, String>> {
    if let Some(error) = root.get_attribute(DATASET_ERROR_ATTRIBUTE) {
        return Some(Err(format!("Dataset refused: {error}")));
    }
    root.get_attribute(DATASET_ATTRIBUTE).map(Ok)
}

/// The one host dataset `root` was given, if any, parsed and checked: the
/// practice proof's input.
#[cfg(feature = "main-page")]
pub(crate) fn supplied(root: &Element) -> Option<Result<HostDatasetV1, String>> {
    Some(supplied_text(root)?.and_then(|text| {
        graphshell::projection_compile::parse_host_dataset(&text)
            .map_err(|error| format!("Dataset refused: {error}"))
    }))
}

/// The host history `root` was given, if any, parsed and checked: a v2
/// envelope, or a v1 envelope as its one revision. The viewer's input.
pub(crate) fn supplied_history(root: &Element) -> Option<Result<HostDatasetV2, String>> {
    Some(supplied_text(root)?.and_then(|text| {
        graphshell::projection_compile::parse_host_history(&text)
            .map_err(|error| format!("Dataset refused: {error}"))
    }))
}
