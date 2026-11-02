// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The explicit approval conversation for a Gemini client certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeminiIdentityPrompt {
    pub node: Uuid,
    /// The member address that initiated the actor request.
    pub requested_url: String,
    /// The final redirect target whose capsule origin receives the identity.
    pub identity_url: String,
    pub prompt: String,
}
