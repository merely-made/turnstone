// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::research_storage::{SourceReadError, StorageLimits};
use crate::saved_sources::{Admission, Content, Target};

pub(super) struct Owner<'a> {
    pub directory: &'a Path,
    pub generation: uuid::Uuid,
    pub custody: Option<&'a SourceCustody>,
    pub legacy: Option<&'a FjallStore>,
    pub sealed: Option<&'a FjallStore>,
    pub legacy_error: Option<&'a SourceReadError>,
    pub sealed_error: Option<&'a SourceReadError>,
    pub limits: &'a Result<StorageLimits, String>,
}

impl Owner<'_> {
    pub(super) fn resolve(
        &self,
        generation: uuid::Uuid,
        directory: &Path,
        admission: &Admission,
        target: &Target,
    ) -> Result<Content, SourceReadError> {
        let epoch = self.custody.and_then(SourceCustody::snapshot);
        let owner = self
            .directory
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str());
        let result = if generation != self.generation
            || directory != self.directory
            || owner != Some(target.key.session.as_uuid().to_string().as_str())
            || admission.key != target.key
        {
            Err("Saved source reading belongs to another session opening or attachment".into())
        } else if epoch.is_none() {
            Err("Unlock the profile to read private saved sources".into())
        } else if let Some(error) = self.sealed_error {
            Err(error.clone())
        } else {
            match self.legacy {
                Some(store) => self
                    .limits
                    .as_ref()
                    .map_err(|error| SourceReadError::from(error.clone()))
                    .and_then(|limits| {
                        let sealer = self
                            .custody
                            .map(|binding| binding.sealer.as_ref() as &dyn eidetic::PayloadSealer);
                        saved_sources::resolve_with_budget(
                            store,
                            self.sealed,
                            sealer,
                            admission,
                            target,
                            limits.budget(),
                        )
                    }),
                None => Err(self
                    .legacy_error
                    .cloned()
                    .unwrap_or_else(|| "The session evidence store is unavailable".into())),
            }
        };
        if epoch.is_some()
            && !self
                .custody
                .is_some_and(|binding| binding.remains_current(epoch))
        {
            return Err("Profile custody changed while reading a private saved source".into());
        }
        result
    }
}
