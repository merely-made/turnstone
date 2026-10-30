// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The explicit decision for a changed Gemini server certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeminiTrustPrompt {
    pub node: Uuid,
    /// The graph member address that owns the eventual response.
    pub requested_url: String,
    /// The final redirect target whose TLS request was refused.
    pub fetch_url: String,
    pub target: String,
    pub pinned: String,
    pub seen: String,
}
