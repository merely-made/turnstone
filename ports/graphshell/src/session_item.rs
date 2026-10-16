// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The session item's server-side builders (reservoir plan §7 item 31): the
//! card and advertised actions a resident's mere route shows for the attached
//! session, built over a live `GraphSession`. Graphshell's own view never
//! shows it. The route's wire vocabulary (projection names, intents, schemas
//! and payloads) is `mere_overmap::wire`, which every reader of a mere route
//! speaks (§7 item 37).

use chirograph::{AdvertisedAction, CardValueV1, IntentEffect, PortableCardV1};
use mere_overmap::wire::{
    APPLY_EDITS_INTENT, APPLY_EDITS_SCHEMA, REDO_INTENT, SET_VIEW_INTENT, SET_VIEW_SCHEMA,
    STEP_SCHEMA, UNDO_INTENT, advertised,
};
use muniment::Backend;
use pandect::GraphSession;

/// What the session item offers.
pub fn actions() -> Vec<AdvertisedAction> {
    vec![
        advertised(
            APPLY_EDITS_INTENT,
            "Apply edits",
            "Apply edits in stable-id form to this session as one change, under your name.",
            APPLY_EDITS_SCHEMA,
            IntentEffect::DomainTruth,
        ),
        advertised(
            UNDO_INTENT,
            "Undo",
            "Undo your latest change still in effect; what others changed since stays.",
            STEP_SCHEMA,
            IntentEffect::DomainTruth,
        ),
        advertised(
            REDO_INTENT,
            "Redo",
            "Redo your most recent undo.",
            STEP_SCHEMA,
            IntentEffect::DomainTruth,
        ),
        advertised(
            SET_VIEW_INTENT,
            "Change a view",
            "Keep one of your views of this session: its folds, camera and layout.",
            SET_VIEW_SCHEMA,
            IntentEffect::Curation,
        ),
    ]
}

/// The session item's card.
pub fn card<B: Backend>(session: &GraphSession<B>) -> PortableCardV1 {
    let manifest = session.manifest();
    PortableCardV1 {
        title: manifest
            .display_name
            .clone()
            .unwrap_or_else(|| "Session".to_string()),
        values: vec![
            CardValueV1 {
                label: "Session".to_string(),
                value: session.id().as_uuid().to_string(),
            },
            CardValueV1 {
                label: "Changes".to_string(),
                value: session.changes().len().to_string(),
            },
        ],
        badges: if manifest.codicil_read_only {
            vec!["Read-only thaw; editing forks".into()]
        } else {
            Vec::new()
        },
        media: Vec::new(),
    }
}
