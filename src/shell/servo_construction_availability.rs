// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn servo_construction_availability() -> crate::content::EngineAvailability {
    use crate::content::EngineAvailability;
    #[cfg(all(
        feature = "servo",
        any(windows, target_os = "macos", target_os = "linux")
    ))]
    {
        EngineAvailability::OnDemand {
            detail: "upstream Servo shares the configured named profile across this process's views; native construction is checked when selected".into(),
        }
    }
    #[cfg(not(all(
        feature = "servo",
        any(windows, target_os = "macos", target_os = "linux")
    )))]
    EngineAvailability::Unavailable {
        reason: "requires a Windows, macOS or Linux build with the servo feature".into(),
    }
}
