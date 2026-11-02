// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The Trail pane (rung 5 slice D): its rows rendered as a panel of absolutely
/// positioned rows at the fixed geometry the click router shares, so a pointer
/// hits exactly the row it sees. Navigable rows read in a link color; muted
/// hints and section titles are dimmed. Built on the same `ScriptedDom` +
/// Livery/Buckram path the chrome runs.
// Both list panes (Roster's grid, Trail's sectioned list) render as cambium
// views now (rung 5 slice D toolkit adoption; see `crate::cambium_pane`,
// `crate::trail_pane`), so the hand-DOM list scenes and their fixed-height row
// geometry are retired. Placeholder panes still use `pane_scene` below.

pub(super) fn finish_scene(dom: &ScriptedDom, w: u32, h: u32) -> netrender::Scene {
    scene_from_dom(dom, CHROME_SHEET, w, h)
}
