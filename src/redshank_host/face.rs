// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn face(item: &LibraryItem) -> Face {
    match item {
        LibraryItem::FeedEpisode { facts, .. } => match facts.artwork.clone() {
            Some(artwork) => Face::Artwork(artwork),
            None => Face::Tag(format_tag(item)),
        },
        _ => Face::Tag(format_tag(item)),
    }
}
