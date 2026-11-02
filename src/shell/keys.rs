// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Keyboard routing. Focus picks the lane, exactly as it does for the pointer:
//! the omnibar when open, the focused page when one holds focus, the canvas
//! otherwise.
//!
//! A page's scroll keys and Escape are ephemeral (delivered inline, consumed,
//! never an Action) while its durable node/nav chords still lower through the
//! spine. The canvas view hotkeys stay suspended while a page reads, so a
//! stray `space` cannot reseed the graph behind it.

use super::NodeSurfaces;
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

use inker::SessionScrollKey;

use crate::action::{Action, CaretMove};

use super::Shell;

/// The scroll a focused content session should perform for a key, or `None`
/// when the key is not a content-scroll key. Space pages down, Shift+Space
/// pages up (the browser convention); the arrows line-scroll, and Home/End
/// jump to the ends.
/// The page-zoom chords, held under ctrl. `=` joins `+` and `_` joins `-` so
/// neither needs shift, which is what every browser binds.
#[derive(Clone, Copy)]
enum PageZoomChord {
    In,
    Out,
    Reset,
}

#[cfg(test)]
#[path = "keys/tests.rs"]
mod tests;

#[path = "keys/is_claimed_ctrl_chord.rs"]
mod is_claimed_ctrl_chord;
#[path = "keys/page_zoom_chord_for_character.rs"]
mod page_zoom_chord_for_character;
use is_claimed_ctrl_chord::is_claimed_ctrl_chord;
#[path = "keys/content_scroll_key.rs"]
mod content_scroll_key;
use content_scroll_key::content_scroll_key;
#[path = "keys/canvas_shortcut_action.rs"]
mod canvas_shortcut_action;
#[path = "keys/shell_cambium_key.rs"]
mod shell_cambium_key;
#[path = "keys/shell_on_key_in_window.rs"]
mod shell_on_key_in_window;
use canvas_shortcut_action::canvas_shortcut_action;
#[path = "keys/windows_virtual_key.rs"]
mod windows_virtual_key;
use windows_virtual_key::windows_virtual_key;
