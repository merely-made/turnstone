// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The canvas command bindings used after focus-specific key routing.
pub(super) fn canvas_shortcut_action(key: &WinitKey, ctrl: bool, alt: bool) -> Option<Action> {
    match key {
        WinitKey::Named(WinitNamedKey::Space) => Some(Action::ReseedLayout),
        // Delete forgets the focused node (recoverable from the Trail's
        // Removed section).
        WinitKey::Named(WinitNamedKey::Delete) => Some(Action::DeleteFocusedNode),
        // The browser nav chords (the r3-owed row).
        WinitKey::Named(WinitNamedKey::ArrowLeft) if alt => Some(Action::NavBack),
        WinitKey::Named(WinitNamedKey::ArrowRight) if alt => Some(Action::NavForward),
        WinitKey::Character(s) if ctrl => match s.as_str() {
            // The summon chords: Ctrl+L address flavor, Ctrl+K command
            // flavor (pre-seeded `>`).
            "l" => Some(Action::OmnibarOpen { command: false }),
            "k" => Some(Action::OmnibarOpen { command: true }),
            "r" => Some(Action::Reload),
            _ => None,
        },
        WinitKey::Character(s) => match s.as_str() {
            // Plain-key summons beside the Ctrl chords: `/` (the
            // quick-switcher convention) and `>` straight into the
            // actions lane. Chord-free, so synthesized-input drivers
            // can't lose the modifier race either.
            "/" => Some(Action::OmnibarOpen { command: false }),
            ">" => Some(Action::OmnibarOpen { command: true }),
            "i" => Some(Action::ToggleIsometric),
            "q" => Some(Action::OrbitBy(-0.15)),
            "e" => Some(Action::OrbitBy(0.15)),
            "[" => Some(Action::TiltBy(-0.05)),
            "]" => Some(Action::TiltBy(0.05)),
            "h" => Some(Action::ToggleHeightByDegree),
            _ => None,
        },
        _ => None,
    }
}
