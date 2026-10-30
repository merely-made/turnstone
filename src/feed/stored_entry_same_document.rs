// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl StoredEntry {
    pub(super) fn same_document(&self, entry: &FeedEntry) -> bool {
        self.url == entry.url
            && self.guid == entry.guid
            && self.title == entry.title
            && self.date == entry.date
            && self.summary == entry.summary
            && self.enclosures == entry.enclosures
            && self.duration == entry.duration
            && self.artwork == entry.artwork
            && self.chapters == entry.chapters
            && self.transcripts == entry.transcripts
    }

    pub(super) fn replace_document(&mut self, entry: &FeedEntry) {
        self.url = entry.url.clone();
        self.guid = entry.guid.clone();
        self.title = entry.title.clone();
        self.date = entry.date.clone();
        self.summary = entry.summary.clone();
        self.enclosures = entry.enclosures.clone();
        self.duration = entry.duration.clone();
        self.artwork = entry.artwork.clone();
        self.chapters = entry.chapters.clone();
        self.transcripts = entry.transcripts.clone();
        self.unread = true;
    }
}
