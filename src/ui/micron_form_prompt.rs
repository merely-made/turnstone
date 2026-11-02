// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Ephemeral form state for one exact fetched Micron page.  The source body
/// is retained only to reject a stale form before the request is constructed.
#[derive(Clone, PartialEq, Eq)]
pub struct MicronFormPrompt {
    pub source: Uuid,
    pub source_url: String,
    pub source_body: String,
    pub form: nematic::micron::forms::FormState,
    pub action: usize,
    pub resolved_target: Option<String>,
    pub stage: MicronFormStage,
    pub error: Option<String>,
}
