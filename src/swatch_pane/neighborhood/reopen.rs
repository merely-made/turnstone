// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::path::Path;

use muniment::FjallStore;

use super::{Page, Uuid, extract};
use crate::panes::PaneId;
use crate::research_storage::StorageLimits;
use crate::saved_sources::KeptSource;
use crate::trail_memory::{KeptDocumentRead, SourceCustody};

pub(crate) struct NeighborhoodRead {
    pub pane: PaneId,
    pub request: Uuid,
    pub read: KeptDocumentRead,
}

pub(crate) struct NeighborhoodReply {
    pub pane: PaneId,
    pub request: Uuid,
    pub generation: Uuid,
    pub source: KeptSource,
    pub(super) result: Result<Page, String>,
}

impl NeighborhoodRead {
    pub(crate) fn run(
        self,
        directory: &Path,
        generation: Uuid,
        custody: Option<&SourceCustody>,
        store: Option<&FjallStore>,
        limits: &Result<StorageLimits, String>,
    ) -> NeighborhoodReply {
        let result = self
            .read
            .read(directory, generation, custody, store, limits)
            .and_then(|(document, custody, epoch)| {
                let page = extract(&document, &self.read.source.capture_hash);
                if !custody.remains_current(epoch) {
                    return Err("Profile custody changed during the source read.".into());
                }
                Ok(page)
            });
        self.reply(result)
    }

    pub(crate) fn unavailable(&self) -> NeighborhoodReply {
        self.reply(Err("The kept source store is unavailable.".into()))
    }

    fn reply(&self, result: Result<Page, String>) -> NeighborhoodReply {
        NeighborhoodReply {
            pane: self.pane,
            request: self.request,
            generation: self.read.generation,
            source: self.read.source.clone(),
            result,
        }
    }
}

pub(super) enum Edition {
    Live {
        token: Uuid,
        page: Page,
    },
    Kept {
        source: KeptSource,
        request: Uuid,
        result: Option<Result<Page, String>>,
    },
}

impl Edition {
    pub(super) fn page(&self) -> Option<&Page> {
        match self {
            Self::Live { page, .. } => Some(page),
            Self::Kept { result, .. } => result.as_ref()?.as_ref().ok(),
        }
    }
}
