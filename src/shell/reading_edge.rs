// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Turnstone persists Tabard's reading types; the reader lanes own theirs
//! (engine split D16). These convert at the lanes' boundary, field by field.

pub(super) fn lanes_anchor(anchor: tabard::ReadingAnchor) -> mere_document_lanes::ReadingAnchor {
    let tabard::ReadingAnchor {
        source,
        revision,
        text,
        offset,
        exact,
        prefix,
        suffix,
        line_offset,
        fallback_y,
    } = anchor;
    mere_document_lanes::ReadingAnchor {
        source,
        revision,
        text,
        offset,
        exact,
        prefix,
        suffix,
        line_offset,
        fallback_y,
    }
}

pub(super) fn tabard_anchor(anchor: mere_document_lanes::ReadingAnchor) -> tabard::ReadingAnchor {
    let mere_document_lanes::ReadingAnchor {
        source,
        revision,
        text,
        offset,
        exact,
        prefix,
        suffix,
        line_offset,
        fallback_y,
    } = anchor;
    tabard::ReadingAnchor {
        source,
        revision,
        text,
        offset,
        exact,
        prefix,
        suffix,
        line_offset,
        fallback_y,
    }
}

pub(super) fn lanes_preferences(
    preferences: tabard::ReadingPreferences,
) -> mere_document_lanes::ReadingPreferences {
    let tabard::ReadingPreferences {
        text_size,
        column_em,
    } = preferences;
    mere_document_lanes::ReadingPreferences {
        text_size,
        column_em,
    }
}

pub(super) fn tabard_preferences(
    preferences: mere_document_lanes::ReadingPreferences,
) -> tabard::ReadingPreferences {
    let mere_document_lanes::ReadingPreferences {
        text_size,
        column_em,
    } = preferences;
    tabard::ReadingPreferences {
        text_size,
        column_em,
    }
}
