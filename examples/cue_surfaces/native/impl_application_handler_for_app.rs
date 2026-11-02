// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(e) = self.init(el) {
                self.error = Some(e.to_string());
                self.cleanup();
                el.exit();
            }
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.cleanup();
                el.exit();
            },
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.tick() {
                    self.error = Some(e.to_string());
                    self.cleanup();
                    el.exit();
                }
                if self.passed {
                    el.exit();
                }
            },
            _ => {},
        }
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
        if !self.require_window && self.core.is_some() && !self.passed {
            if let Err(e) = self.tick() {
                self.error = Some(e.to_string());
                self.cleanup();
                el.exit();
            }
            if self.passed {
                el.exit();
            }
        } else if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
