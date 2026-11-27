// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use incipit_session::IncipitSessions;
use graphshell_host::pinned_shacl::{
    PinnedValidationError, ValidationRevision, require_current_revision,
};

use super::*;
use crate::swatch_pane::validation::{EvidenceResponse, EvidenceTarget};

pub struct EvidenceRequest {
    pub generation: Uuid,
    pub pane: PaneId,
    pub target: EvidenceTarget,
    pub revision: ValidationRevision,
    pub directory: PathBuf,
}

impl EvidenceRequest {
    pub(super) fn run(
        self,
        directory: &Path,
        generation: Uuid,
        custody: Option<&SourceCustody>,
    ) -> EvidenceResponse {
        let result = self.check(directory, generation, custody);
        EvidenceResponse {
            generation: self.generation,
            pane: self.pane,
            target: self.target,
            result,
        }
    }

    fn check(
        &self,
        directory: &Path,
        generation: Uuid,
        custody: Option<&SourceCustody>,
    ) -> Result<(), String> {
        if self.generation != generation
            || self.directory != directory
            || !in_session_directory(directory, &self.target.source)
        {
            return Err("The finding belongs to another session opening or attachment.".into());
        }
        let custody = custody.ok_or("Unlock the profile to open finding evidence.")?;
        let epoch = custody.snapshot();
        if epoch.is_none() {
            return Err("Unlock the profile to open finding evidence.".into());
        }
        let journal = journal_path(directory, &self.target.source.key.annotation_manifest);
        if !journal.is_dir() {
            return Err("The source journal is unavailable.".into());
        }
        let result = pollster::block_on(check_revision(&journal, custody, &self.revision));
        if !custody.remains_current(epoch) {
            return Err("Profile custody changed before the evidence could be opened.".into());
        }
        result
    }
}

async fn check_revision(
    journal: &Path,
    custody: &SourceCustody,
    revision: &ValidationRevision,
) -> Result<(), String> {
    let disk = DirectoryBackend::open(journal).map_err(|error| error.to_string())?;
    let backend = sporran::WalletSealedBackend::with_sealer(disk, custody.sealer.clone());
    let sessions = MereSessions::new(backend);
    let manifest = sessions
        .latest_live()
        .await
        .map_err(|error| error.to_string())?
        .ok_or("The source journal is unavailable.")?;
    let session = sessions
        .open(manifest.session_id)
        .await
        .map_err(|error| error.to_string())?;
    require_current_revision(revision, &session).map_err(|error| match error {
        PinnedValidationError::Stale { current, .. } => format!(
            "The source journal is now at cursor {}. Reopen validation for this capture to open its evidence.",
            current.cursor.0,
        ),
        error => error.to_string(),
    })
}
