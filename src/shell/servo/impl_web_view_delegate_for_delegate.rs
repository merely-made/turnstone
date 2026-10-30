// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl WebViewDelegate for Delegate {
    fn notify_cursor_changed(&self, _: WebView, cursor: servo::Cursor) {
        use servo::Cursor as C;
        let cursor = match cursor {
            C::Pointer => CursorShape::Pointer,
            C::Text | C::VerticalText => CursorShape::Text,
            C::Grab => CursorShape::Grab,
            C::Grabbing => CursorShape::Grabbing,
            C::Crosshair => CursorShape::Crosshair,
            C::Move | C::AllScroll => CursorShape::Move,
            C::NResize | C::SResize | C::NsResize => CursorShape::ResizeNs,
            C::EResize | C::WResize | C::EwResize => CursorShape::ResizeEw,
            C::NeResize | C::SwResize | C::NeswResize => CursorShape::ResizeNesw,
            C::NwResize | C::SeResize | C::NwseResize => CursorShape::ResizeNwse,
            C::NotAllowed | C::NoDrop => CursorShape::NotAllowed,
            C::None => CursorShape::Hidden,
            _ => CursorShape::Default,
        };
        self.cursors.borrow_mut().push_back(cursor);
        (self.wake)();
    }
    fn notify_accessibility_tree_update(&self, _: WebView, update: servo::accesskit::TreeUpdate) {
        match surface_update_from_servo(&update) {
            Ok(update) => self.push_accessibility_update(update),
            Err(error) => {
                // A dropped delta would leave the host applying later deltas
                // to the wrong tree. Retire the forest; resync re-admits it.
                tracing::error!(target: "turnstone::servo::a11y",
                    %error, "Servo accessibility update did not convert");
                self.deactivate_accessibility();
            },
        }
    }
    fn request_permission(&self, _: WebView, request: PermissionRequest) {
        // This host slice exposes no permission UI or decision persistence.
        // Resolve explicitly rather than inherit a caller-selected fallback.
        request.deny();
    }
    fn notify_new_frame_ready(&self, _: WebView) {
        self.ready.set(true);
        (self.wake)();
    }
    fn notify_animating_changed(&self, _: WebView, animating: bool) {
        self.animating.set(animating);
        (self.wake)();
    }
    fn notify_url_changed(&self, _: WebView, url: Url) {
        *self.requested_url.borrow_mut() = url.to_string();
        self.push(WebSurfaceEvent::AddressChanged {
            url: url.to_string(),
        });
    }
    fn notify_page_title_changed(&self, _: WebView, title: Option<String>) {
        self.push(WebSurfaceEvent::TitleChanged {
            title: title.unwrap_or_default(),
        });
    }
    fn notify_load_status_changed(&self, view: WebView, status: LoadStatus) {
        let url = self.requested_url.borrow().clone();
        let event = match status {
            LoadStatus::Started => NavigationEvent::Started { url },
            LoadStatus::HeadParsed => NavigationEvent::Committed { url },
            LoadStatus::Complete => NavigationEvent::Finished {
                url,
                title: view.page_title(),
            },
        };
        self.push(WebSurfaceEvent::Navigation(event));
    }
    fn notify_crashed(&self, _: WebView, reason: String, _: Option<String>) {
        self.push(WebSurfaceEvent::Navigation(NavigationEvent::Failed {
            url: self.requested_url.borrow().clone(),
            reason,
        }));
    }
}
