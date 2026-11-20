// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::path::{Path, PathBuf};

use muniment::FjallStore;
use uuid::Uuid;

use super::{SourceCustody, sealed_capture};
use crate::action::StoredSourceDocument;
use crate::content::FetchedDocument;
use crate::research_storage::{BudgetStore, StorageLimits};
use crate::saved_sources::KeptSource;
use crate::source_capture::CaptureRequest;

pub(crate) struct KeptDocumentRead {
    pub generation: Uuid,
    pub source: KeptSource,
    pub capture: CaptureRequest,
    pub stored: StoredSourceDocument,
    pub directory: PathBuf,
}

impl KeptDocumentRead {
    pub(crate) fn read<'a>(
        &self,
        directory: &Path,
        generation: Uuid,
        custody: Option<&'a SourceCustody>,
        store: Option<&FjallStore>,
        limits: &Result<StorageLimits, String>,
    ) -> Result<(FetchedDocument, &'a SourceCustody, Option<u64>), String> {
        if self.generation != generation
            || self.directory != directory
            || !in_session_directory(directory, &self.source)
            || !self.capture.keep
            || self.capture.id != self.source.request
            || self.capture.session != self.source.key.session
            || self.capture.graph != self.source.key.graph
            || self.capture.node != self.source.key.node
            || self.stored.annotation_manifest != self.source.key.annotation_manifest
            || !self
                .capture
                .source
                .as_ref()
                .is_some_and(|identity| identity.hash == self.source.capture_hash)
        {
            return Err("The kept source belongs to another session opening or attachment.".into());
        }
        let custody = custody.ok_or("Unlock the profile to read this kept source.")?;
        let epoch = custody.snapshot();
        if epoch.is_none() {
            return Err("Unlock the profile to read this kept source.".into());
        }
        let mut store = BudgetStore {
            store: store.ok_or("The kept source store is unavailable.")?,
            budget: limits.as_ref().map_err(Clone::clone)?.budget(),
        };
        let document = sealed_capture::read_kept_document(
            &mut store,
            custody.sealer.as_ref(),
            &self.capture,
            &self.stored,
        )
        .map_err(|error| error.to_string())?;
        if !custody.remains_current(epoch) {
            return Err("Profile custody changed during the source read.".into());
        }
        Ok((document, custody, epoch))
    }
}

pub(super) fn in_session_directory(directory: &Path, source: &KeptSource) -> bool {
    directory
        .parent()
        .and_then(Path::file_name)
        .and_then(|parent| parent.to_str())
        == Some(source.key.session.as_uuid().to_string().as_str())
}
