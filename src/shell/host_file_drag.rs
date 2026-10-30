// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[derive(Default)]
pub(super) struct HostFileDrag {
    pub(super) files: Vec<std::path::PathBuf>,
    pub(super) target: Option<uuid::Uuid>,
}
