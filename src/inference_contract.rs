// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use eidetic::models::recall::{PassageSelector, RecallSourceId, RetainedSourceText};
/// Storage remains behind its one trail actor; only exact source data crosses this seam.
pub(crate) trait RetainedSourceOwner: Send + Sync {
    fn snapshot(&self, owner: &str) -> Result<Vec<RetainedSourceText>, String>;
    fn resolve(&self, source: &RecallSourceId, version: &str) -> Result<String, String>;
    fn choices(&self) -> Vec<crate::research::Choice> {
        Vec::new()
    }
    fn generation(&self) -> Option<uuid::Uuid> {
        None
    }
    fn open_source(&self, _: &OpenSource, _: Option<uuid::Uuid>) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenSource {
    pub run: String,
    pub handle: u32,
    pub retained: RetainedSourceText,
    pub selector: PassageSelector,
}
