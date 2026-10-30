// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// What the omnibar is capturing: an address/action (the default three lanes)
/// or a free-text rename for a session. Rename mode captures the whole line as
/// the new name and commits it as [`crate::action::Action::RenameSession`],
/// bypassing the find/go/actions matching entirely.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum OmnibarMode {
    #[default]
    Address,
    /// Search only the currently projected captured-page cache for this place.
    CapturedPages,
    /// Search private source captures attached in the current session.
    SavedSources,
    /// Filter the active place's local status observations.
    PlaceStatus,
    RenameSession(crate::panes::SessionId),
    /// One free-text place prompt. Same shape as `RenameSession`: the whole
    /// line is the answer, and Enter commits it into the paired action.
    Place(PlacePrompt),
    SmolwebInput(SmolwebInputPrompt),
    SmolwebSubmission(SmolwebSubmissionPrompt),
    MicronForm(MicronFormPrompt),
    SmolwebSubmissionResult(SmolwebSubmissionResult),
    GeminiIdentity(GeminiIdentityPrompt),
    GeminiTrust(GeminiTrustPrompt),
}
