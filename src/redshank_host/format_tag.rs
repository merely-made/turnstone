// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The short format tag the dock's face shows when there is no artwork.
pub(super) fn format_tag(item: &LibraryItem) -> String {
    let address = match item.source() {
        MediaSource::Local { path } => path.clone(),
        MediaSource::Enclosure { url } => url.clone(),
        MediaSource::Cached { origin_url, .. } => origin_url.clone(),
        MediaSource::HostBlob { id } => id.clone(),
    };
    address
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .filter(|extension| extension.len() <= 4 && extension.chars().all(char::is_alphanumeric))
        .unwrap_or_else(|| "audio".to_owned())
}
