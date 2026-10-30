// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Uses the exact UA gate called by Shell before retained touch delivery.
/// App admission, NodeDocuments, TouchInput and producer receivers are real;
/// native Window creation and OS event decoding remain outside this fixture.
pub(super) fn modal_touch_open(
    f: &mut CallerCaptureFixture,
) -> crate::user_agent_decision::UserAgentRequestKey {
    let focus = f.app.focus;
    let effects = f
        .app
        .apply_update(crate::action::Update::PermissionRequested {
            node: f.nodes[0],
            id: inker::UserAgentRequestId::new(901),
            origin: "https://upper.test".into(),
            descriptors: vec![inker::PermissionDescriptor::Geolocation],
        });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, crate::action::Effect::Redraw))
    );
    assert_eq!(
        f.app.focus, focus,
        "retained UA admission is not native focus loss"
    );
    f.app
        .user_agent_decision
        .active_request()
        .expect("actual App update opened the UA gate")
}
