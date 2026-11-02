// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::Shell;
pub(super) enum WorkshopWindow {}
impl Shell {
    pub(super) fn service_alembic_workshop(
        &mut self,
        _: &winit::event_loop::ActiveEventLoop,
    ) -> cambium_rootstock::IdlePolicy {
        if self.alembic_workshop_requested {
            self.alembic_workshop_requested = false;
            self.app
                .note(crate::observe::AppEvent::AffordanceUnavailable {
                    what: "alembic-workshop",
                    target: "Workshop requires a build with the workshop feature".into(),
                });
        }
        cambium_rootstock::IdlePolicy::Wait
    }
    pub(super) fn alembic_workshop_event(
        &mut self,
        _: winit::window::WindowId,
        _: &winit::event::WindowEvent,
    ) -> bool {
        false
    }
    pub(super) fn request_close_with_alembic_workshop(&mut self) -> bool {
        false
    }
    pub(super) fn refresh_alembic_workshop_sources(&mut self) {}
}
