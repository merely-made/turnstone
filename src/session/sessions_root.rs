// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The sessions directory under the data root: one subdirectory per session,
/// named by its uuid (ManifestStore's own layout).
pub fn sessions_root(data_root: &Path) -> PathBuf {
    data_root.join("sessions")
}
