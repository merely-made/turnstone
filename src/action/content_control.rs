// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Browser controls that a hosted web surface executes directly. Retained
/// document sessions use the same app actions but complete them through the
/// fetch/content ports instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentControl {
    Back,
    Forward,
    Reload,
    Stop,
}
