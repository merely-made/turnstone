// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
pub(super) enum Practice {}
pub(super) fn open(_: &Element) -> Result<Option<Practice>, String> {
    Ok(None)
}
pub(super) fn needs_frame(_: &TreePage) -> bool {
    false
}
pub(super) fn advance(_: &mut TreePage, _: f64) -> bool {
    false
}
pub(super) fn fields(_: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    snapshot
}
pub(super) fn view(_: &TreePage) -> Option<Child> {
    None
}
