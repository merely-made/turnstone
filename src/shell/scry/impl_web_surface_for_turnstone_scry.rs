// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl WebSurface for TurnstoneScryProducer {
    fn capabilities(&self) -> WebSurfaceCapabilities {
        host_capabilities(self.inner.capabilities())
    }

    fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        self.inner.navigate_to_url(url)
    }

    fn navigate_to_string(&mut self, html: &str) -> Result<(), SurfaceError> {
        self.inner.navigate_to_string(html)
    }

    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.inner.reload()
    }

    fn stop(&mut self) -> Result<(), SurfaceError> {
        self.inner.stop()
    }

    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.inner.go_back()
    }

    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.inner.go_forward()
    }

    fn can_go_back(&self) -> bool {
        self.inner.can_go_back()
    }

    fn can_go_forward(&self) -> bool {
        self.inner.can_go_forward()
    }

    fn set_cookie(&mut self, cookie: &Cookie) -> Result<(), SurfaceError> {
        self.inner.set_cookie(cookie)
    }

    fn request_cookies_for_url(&mut self, id: WebRequestId, url: &str) -> Result<(), SurfaceError> {
        self.inner.request_cookies_for_url(id, url)
    }

    fn request_script_result(
        &mut self,
        id: WebRequestId,
        script: &str,
    ) -> Result<(), SurfaceError> {
        self.inner.request_script_result(id, script)
    }

    fn poll_web_event(&mut self) -> Option<WebSurfaceEvent> {
        self.inner.poll_web_event()
    }
}
