// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn scry_construction_availability() -> crate::content::EngineAvailability {
    use crate::content::EngineAvailability;
    #[cfg(all(feature = "scry", windows))]
    {
        EngineAvailability::OnDemand {
            detail: "WebView2 runtime and native capture construction are checked when selected"
                .into(),
        }
    }
    #[cfg(not(all(feature = "scry", windows)))]
    EngineAvailability::Unavailable {
        reason: "requires a Windows build with the scry feature".into(),
    }
}
