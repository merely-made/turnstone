// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn weld_construction_availability() -> crate::content::EngineAvailability {
    use crate::content::EngineAvailability;
    #[cfg(all(feature = "weld", windows))]
    {
        let path = std::env::var_os("TURNSTONE_CEF_PATH").or_else(|| std::env::var_os("CEF_PATH"));
        match path.map(std::path::PathBuf::from) {
            Some(path) if path.is_dir() => EngineAvailability::OnDemand {
                detail:
                    "CEF runtime directory is configured; construction is checked when selected"
                        .into(),
            },
            Some(_) => EngineAvailability::Unavailable {
                reason: "the configured CEF runtime directory does not exist".into(),
            },
            None => EngineAvailability::Unavailable {
                reason: "set TURNSTONE_CEF_PATH or CEF_PATH to a CEF runtime directory".into(),
            },
        }
    }
    #[cfg(not(all(feature = "weld", windows)))]
    EngineAvailability::Unavailable {
        reason: "requires a Windows build with the weld feature".into(),
    }
}
