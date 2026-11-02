// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PrimaryPaneCaptureFixture {
    pub(super) fn open_primary_decision(
        &mut self,
    ) -> crate::user_agent_decision::UserAgentRequestKey {
        use crate::user_agent_decision::{PendingUserAgentDecision, UserAgentRequestKey};
        let request =
            UserAgentRequestKey::new(self.content.nodes[0], inker::UserAgentRequestId::new(71));
        assert!(self.content.app.user_agent_decision.receive(
            PendingUserAgentDecision::Permission {
                request,
                origin: "https://upper.test".into(),
                descriptors: vec![inker::PermissionDescriptor::Camera],
            }
        ));
        request
    }

    pub(super) fn press_primary_outside_decision_card(&mut self) {
        use super::super::super::input::{PrimaryPointerHost, reject_primary_modal_press};
        // Begin after chrome.click returned no intent. Use the actual shared
        // UA predicate/return branch; card geometry is a headed-test boundary.
        self.pointer_window_mouse(
            None,
            (-100.0, -100.0),
            PointerPhase::Down,
            Some(inker::MouseButton::Middle),
        );
        assert!(reject_primary_modal_press(self));
    }
}
