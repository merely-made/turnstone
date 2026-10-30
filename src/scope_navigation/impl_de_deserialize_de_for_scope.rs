// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<'de> Deserialize<'de> for ScopeBookmark {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Current {
            reader: Option<PaneId>,
            map: Option<PaneId>,
            reading: Option<tabard::ReadingAnchor>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Saved {
            Current(Current),
            Legacy(tabard::ReadingAnchor),
        }
        Ok(match Saved::deserialize(deserializer)? {
            Saved::Current(c) => Self {
                reader: c.reader,
                map: c.map,
                reading: c.reading,
            },
            Saved::Legacy(reading) => Self {
                reading: Some(reading),
                ..Default::default()
            },
        })
    }
}
