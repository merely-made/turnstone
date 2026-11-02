// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The one action row allowed to grow vertically rather than clip.
pub(crate) fn is_install_review(s: &Suggestion) -> bool {
    matches!(
        s,
        Suggestion::Act {
            action: crate::action::Action::ConfirmInstallDenizen,
            ..
        }
    )
}
