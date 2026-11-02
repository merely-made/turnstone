// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl taproot::Automatable for Shell {
    fn with_surfaces<R>(&self, f: impl FnOnce(&[taproot::ProbeSurface<'_>]) -> R) -> R {
        self.adapter_with_surfaces(f)
    }

    fn selector_target(&self, selector: &taproot::Selector) -> taproot::SelectorTarget {
        self.adapter_selector_target(selector)
    }

    fn click_target(&mut self, selector: &taproot::Selector) -> Option<bool> {
        self.adapter_click_target(selector)
    }

    fn text_target(&self, text: &str) -> Result<Option<taproot::TextTarget>, String> {
        self.adapter_text_target(text)
    }

    fn snapshot(&self) -> taproot::ProbeSnapshot {
        self.adapter_snapshot()
    }

    fn drain_events(&mut self) -> Vec<String> {
        self.adapter_drain_events()
    }

    fn act(&mut self, label: &str) -> bool {
        self.adapter_act(label)
    }

    /// Turnstone's quiescence report, driving the `wait` verb. Busy while any of
    /// the work a scenario must not race is outstanding:
    ///
    /// - a workshop is requested or owns its native child window,
    /// - a capture is queued or its owned readback is pending,
    /// - a page, favicon, subresource, or submission FETCH is in flight (the
    ///   port has not answered; favicon failures remain UI-silent),
    /// - a content spawn is `Requested` (the effect is out, no session yet),
    /// - a live session is not `settled()` (script work or layout pending).
    ///
    /// Deliberately conservative in both directions. It reports `Some` always
    /// (turnstone DOES report quiescence, even when the honest answer is "idle"),
    /// and it counts a spawn as busy from the effect rather than from the
    /// session, so the gap between them cannot read as quiet.
    fn busy(&mut self) -> Option<bool> {
        self.adapter_busy()
    }

    fn press(&mut self, x: f32, y: f32) {
        self.adapter_press(x, y)
    }

    fn moved(&mut self, x: f32, y: f32) {
        self.adapter_moved(x, y)
    }

    fn release(&mut self, x: f32, y: f32) {
        self.adapter_release(x, y)
    }
}
