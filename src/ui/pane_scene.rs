// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

// The hand-built chrome_scene retired 2026-07-18: the chrome is a retained
// cambium view now (`crate::chrome_view`, a forest of window-roots), diffed
// per state change instead of rebuilt wholesale. The sheet + caret constants
// above are its styling contract.

/// Lay out the chrome document and composite its paint list into a scene.
/// A pane placeholder (rung 5 slice C): a panel filling the pane's rect with its
/// kind label. The pane tree, geometry, and persistence are what slice C proves;
/// each pane's real content (Roster rows, Trail history, ...) is slice D. Built
/// on the same `ScriptedDom` + Livery/Buckram path the chrome runs, sized to the
/// pane rect (the shell rasterizes each surface at its own size).
pub fn pane_scene(label: &str, w: u32, h: u32) -> netrender::Scene {
    let mut dom = ScriptedDom::new();
    let root = dom.document();

    let panel = dom.create_element(qual("div"));
    dom.set_attribute(panel, qual("class"), "pane");
    dom.set_attribute(
        panel,
        qual("style"),
        &format!("transform: translate(0px, 0px); width: {w}px; height: {h}px;"),
    );
    dom.append_child(root, panel);

    let name = dom.create_element(qual("div"));
    dom.set_attribute(name, qual("class"), "pane-label");
    let text = dom.create_text(label);
    dom.append_child(name, text);
    dom.append_child(root, name);

    finish_scene(&dom, w, h)
}
