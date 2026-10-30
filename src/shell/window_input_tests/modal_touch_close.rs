// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn modal_touch_close(
    f: &mut CallerCaptureFixture,
    request: crate::user_agent_decision::UserAgentRequestKey,
) {
    f.app
        .apply_update(crate::action::Update::UserAgentRequestWithdrawn {
            request,
            kind: "permission",
            reason: "test-withdrawal",
        });
    assert!(!f.app.user_agent_decision.is_open());
}
