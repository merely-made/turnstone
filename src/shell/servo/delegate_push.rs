// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Delegate {
    pub(super) fn push(&self, event: WebSurfaceEvent) {
        self.events.borrow_mut().push_back(event);
        (self.wake)();
    }

    pub(super) fn push_accessibility_update(&self, update: SurfaceAccessibilityUpdate) {
        if !self.accessibility_active.get() {
            return;
        }
        // Servo sends its wrapper graft before the document's update. Keep
        // every nested TreeId and the callback order intact for host admission.
        self.accessibility_updates.borrow_mut().push_back(update);
        (self.wake)();
    }

    pub(super) fn deactivate_accessibility(&self) {
        self.accessibility_active.set(false);
        self.accessibility_updates.borrow_mut().clear();
        self.accessibility_focus_provenance.set(0);
    }

    pub(super) fn poll_projected_accessibility_update(
        &self,
        policy: A11yFocusPolicy,
    ) -> Option<SurfaceAccessibilityUpdate> {
        // The FIFO always holds the exact raw supplier callback first. Change
        // only focus at this producer boundary, preserving supplier identities,
        // graph, bounds and advertised actions. DOM focused-node support and
        // assistive actions remain unsupported.
        let mut update = self.accessibility_updates.borrow_mut().pop_front()?;
        let supplier_focus = update.focus;
        policy.project(&mut update);
        // Bound provenance to the first 32 callbacks per activation. Metadata
        // alone is logged; labels, values and page text are never included.
        let count = self.accessibility_focus_provenance.get();
        if count < 32 {
            tracing::debug!(target: "turnstone::servo::a11y",
                tree_id = %update.tree_id.0,
                supplier_focus = supplier_focus.0,
                projected_focus = update.focus.0,
                policy = policy.name(),
                "Servo accessibility focus projection");
            self.accessibility_focus_provenance.set(count + 1);
        }
        Some(update)
    }
}
