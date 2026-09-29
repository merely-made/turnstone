// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read the retired pane tags from real frame/window sidecars, then immediately
//! lower them to current content. New saves emit only Gloss and Inspector.

use super::{PaneComposition, PaneContent, PaneKindId};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
enum SavedContent {
    Workbench,
    Orrery,
    Gloss(PaneComposition),
    Roster,
    #[serde(alias = "Apparatus")]
    Inspector,
    Trail,
    Steward,
    Comms,
    Alembic,
    Overmap(PaneComposition),
    Registered(PaneKindId),
    Tile(uuid::Uuid),
}

impl<'de> Deserialize<'de> for PaneContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match SavedContent::deserialize(deserializer)? {
            SavedContent::Workbench => Self::Workbench,
            SavedContent::Orrery => Self::Orrery,
            SavedContent::Gloss(config) => Self::Gloss(config),
            SavedContent::Roster => Self::Roster,
            SavedContent::Inspector => Self::Inspector,
            SavedContent::Trail => Self::Trail,
            SavedContent::Steward => Self::Gloss(PaneComposition {
                sections: vec!["downloads".into()],
            }),
            SavedContent::Comms => Self::Comms,
            SavedContent::Alembic => Self::Alembic,
            SavedContent::Overmap(config) => Self::Overmap(config),
            SavedContent::Registered(kind) => match kind.as_str() {
                "turnstone.apparatus" => Self::Inspector,
                "turnstone.steward" => Self::Gloss(PaneComposition {
                    sections: vec!["downloads".into()],
                }),
                _ => Self::Registered(kind),
            },
            SavedContent::Tile(member) => Self::Tile(member),
        })
    }
}
