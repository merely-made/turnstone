// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Native bridge for Cambium's retained, application-owned title row.
//! Native caption controls stay with the OS. The shared title-bar component
//! owns its slots and drag declarations; this shell supplies the window verbs.

use std::sync::Arc;

use cambium_genet_winit_host::{ClickCadence, HostWindow, WinitWindow};
use cambium_rootstock::{AppRegion, TitlebarInsets};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::window::{Window, WindowAttributes, WindowId};

use super::Shell;

pub(super) fn window_attributes() -> WindowAttributes {
    let attributes = Window::default_attributes();
    // Extend the app's title row behind the standard traffic lights. The
    // caption-area observation below supplies their actual occupied space.
    #[cfg(target_os = "macos")]
    let attributes = {
        use winit::platform::macos::WindowAttributesExtMacOS;
        attributes
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true)
    };
    attributes
}

fn physical_insets(insets: TitlebarInsets, scale: f32) -> TitlebarInsets {
    TitlebarInsets {
        left: insets.left * scale,
        right: insets.right * scale,
        height: insets.height * scale,
    }
}

impl Shell {
    pub(super) fn publish_titlebar_insets(&mut self, slot: usize, window: &Arc<Window>) {
        let insets = physical_insets(
            WinitWindow(window.clone()).titlebar_insets(),
            window.scale_factor() as f32,
        );
        let mut chrome = self.app.shell_chrome_config().clone();
        if chrome.titlebar_insets.get(&slot) == Some(&insets) {
            return;
        }
        chrome.titlebar_insets.insert(slot, insets);
        self.app.set_shell_chrome_config(chrome);
        window.request_redraw();
    }

    /// Run before content delivery. The drag decision is read from the same
    /// retained chrome cascade and hit target that paint uses, including any
    /// authored no-drag descendants.
    pub(super) fn titlebar_event(&mut self, id: WindowId, event: &WindowEvent) -> bool {
        let owner = self
            .window
            .as_ref()
            .filter(|window| window.id() == id)
            .map(|window| (0, window.clone(), self.cursor))
            .or_else(|| {
                self.lens_windows
                    .get(&id)
                    .map(|lens| (lens.ordinal + 1, lens.window.clone(), lens.cursor))
            });
        let Some((slot, window, cursor)) = owner else {
            return false;
        };
        if matches!(
            event,
            WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::Moved(_)
        ) {
            self.publish_titlebar_insets(slot, &window);
        }
        if matches!(
            event,
            WindowEvent::Focused(false) | WindowEvent::CloseRequested
        ) {
            self.titlebar_clicks.remove(&slot);
        }
        if slot != 0 && matches!(event, WindowEvent::CloseRequested) {
            let mut chrome = self.app.shell_chrome_config().clone();
            chrome.titlebar_insets.remove(&slot);
            self.app.set_shell_chrome_config(chrome);
        }
        let WindowEvent::MouseInput {
            state: ElementState::Pressed,
            button,
            ..
        } = event
        else {
            return false;
        };
        if !matches!(button, MouseButton::Left | MouseButton::Right)
            || self.contributed_pointer.is_some()
            || self.pointer_capture.is_some()
            || self.outer_capture.is_some()
            || super::window_input::captured_mouse(&self.content_sessions.pool).is_some()
            || self
                .lens_windows
                .values()
                .any(|lens| lens.pointer_graph.is_some())
        {
            return false;
        }
        let mut sizes = vec![(0, self.width as f32, self.height as f32)];
        sizes.extend(
            self.lens_windows
                .values()
                .map(|lens| (lens.ordinal + 1, lens.width as f32, lens.height as f32)),
        );
        self.chrome.sync(&self.app, &sizes);
        let size = window.inner_size();
        if self
            .chrome
            .app_region_at(slot, cursor.0, cursor.1, size.width, size.height)
            != AppRegion::Drag
        {
            self.titlebar_clicks.remove(&slot);
            return false;
        }
        // A modal card or palette retains the application's existing outside
        // press/dismissal policy instead of starting a window gesture.
        if self.app.omnibar.open || self.app.user_agent_decision.is_open() || self.chrome.has_menu() {
            return false;
        }
        if *button == MouseButton::Right {
            window.show_window_menu(winit::dpi::PhysicalPosition::new(cursor.0, cursor.1));
        } else {
            let doubled = self
                .titlebar_clicks
                .entry(slot)
                .or_insert_with(ClickCadence::new)
                .press(
                    (
                        cursor.0 / window.scale_factor() as f32,
                        cursor.1 / window.scale_factor() as f32,
                    ),
                    cambium_rootstock::Instant::now(),
                );
            if doubled {
                window.set_maximized(!window.is_maximized());
            } else if let Err(error) = window.drag_window() {
                tracing::debug!(%error, "native title-bar drag unavailable");
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_caption_reservation_uses_the_shells_physical_coordinates() {
        let insets = physical_insets(
            TitlebarInsets {
                left: 78.0,
                right: 0.0,
                height: 28.0,
            },
            2.0,
        );
        assert_eq!(insets.left, 156.0);
        assert_eq!(insets.height, 56.0);
        assert_eq!(insets.titlebar_area(1024.0), (156.0, 0.0, 868.0, 56.0));
    }
}
