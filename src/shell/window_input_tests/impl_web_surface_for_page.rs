// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl WebSurface for Page {
    fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        self.record(format!("navigate {url}"))
    }

    fn navigate_to_string(&mut self, _html: &str) -> Result<(), SurfaceError> {
        self.record("navigate string")
    }

    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.record("reload")
    }

    fn stop(&mut self) -> Result<(), SurfaceError> {
        self.record("stop")
    }

    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.record("back")
    }

    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.record("forward")
    }

    fn can_go_back(&self) -> bool {
        true
    }

    fn can_go_forward(&self) -> bool {
        true
    }

    fn set_cookie(&mut self, _cookie: &Cookie) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn poll_web_event(&mut self) -> Option<WebSurfaceEvent> {
        self.title
            .take()
            .map(|title| WebSurfaceEvent::TitleChanged { title })
    }
}
