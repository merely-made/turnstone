// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// The real receiving-window pane wheel path, also used by headed fixtures.
    pub(in super::super) fn lens_pane_wheel(
        &mut self,
        window: u64,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    ) -> bool {
        let Some((id, ordinal, width, height)) = self
            .lens_windows
            .iter()
            .find(|(_, lens)| lens.ordinal as u64 + 1 == window)
            .map(|(id, lens)| (*id, lens.ordinal, lens.width, lens.height))
        else {
            return false;
        };
        let plan = self.lens_plan(ordinal, width, height);
        let Some(hit) = crate::surface::hit_test(&plan, self.app.focus, x, y) else {
            return false;
        };
        let SurfaceKind::Pane(pane) = hit.kind else {
            return false;
        };
        let redraw = if let Some(surface) = self.renderers.contributed.get_mut(pane) {
            surface.wheel(dx, dy, hit.local.0, hit.local.1)
                == crate::contributed_surface::SurfaceRequest::Redraw
        } else {
            self.renderers.scroll_pane(pane, dx, dy)
        };
        if redraw && let Some(lens) = self.lens_windows.get(&id) {
            lens.window.request_redraw();
        }
        redraw
    }

    /// Create any requested lens windows (rung 7). Called from the event
    /// handlers, where an `ActiveEventLoop` is in scope. Its graph panes
    /// install their own view state on their first render/input pass.
    pub(in super::super) fn drain_pending_windows(&mut self, event_loop: &ActiveEventLoop) {
        if !self.poll_render_recovery() {
            return;
        }
        while let Some(ordinal) = self.pending_windows.pop() {
            let Some(primary) = self.host.as_ref() else {
                tracing::warn!("lens requires the primary render core");
                continue;
            };
            let shared_core = primary.shared_core();
            let attributes = super::super::titlebar::window_attributes()
                .with_title("Turnstone — lens")
                .with_visible(false)
                .with_inner_size(PhysicalSize::new(800u32, 600u32));
            let Ok(window) = event_loop.create_window(attributes) else {
                tracing::warn!("lens window creation failed");
                continue;
            };
            let window = Arc::new(window);
            self.publish_titlebar_insets(ordinal + 1, &window);
            let size = window.inner_size();
            match SurfaceHost::from_shared_core(
                window.clone(),
                size.width.max(1),
                size.height.max(1),
                shared_core.clone(),
            ) {
                Ok(host) => {
                    debug_assert!(Arc::ptr_eq(&shared_core, &host.shared_core()));
                    let a11y_actions = Arc::new(std::sync::Mutex::new(Default::default()));
                    let a11y = super::super::a11y_bridge::install(
                        &window,
                        a11y_actions.clone(),
                        self.proxy.clone(),
                        &crate::a11y::lens_window_label(ordinal),
                    );
                    window.request_redraw();
                    let owner = self
                        .render_recovery
                        .as_ref()
                        .expect("primary recovery owner")
                        .clone();
                    let proxy = self.proxy.clone();
                    let recovery = super::super::WindowRecovery::new(
                        &owner,
                        Arc::new(move || {
                            let _ = proxy.send_event(());
                        }),
                    );
                    let id = window.id();
                    self.lens_windows.insert(
                        id,
                        LensWindow {
                            window,
                            host: Some(host),
                            recovery,
                            width: size.width.max(1),
                            height: size.height.max(1),
                            cursor: (0.0, 0.0),
                            pointer_graph: None,
                            ordinal,
                            focused: true,
                            a11y,
                            a11y_actions,
                            a11y_epoch: None,
                            a11y_forest: crate::foreign_a11y::ForeignA11y::new(),
                            a11y_scope: super::super::window_a11y::scope(
                                ordinal + 1,
                                self.app.session_id.as_uuid().as_u128(),
                            ),
                            a11y_routes: Default::default(),
                            a11y_tree: None,
                            a11y_placements: Vec::new(),
                        },
                    );
                    self.push_a11y_tree();
                    if let Some(lens) = self.lens_windows.get(&id) {
                        lens.window.set_visible(true);
                    }
                },
                Err(err) => tracing::warn!(%err, "lens surface boot failed"),
            }
        }
        self.app.window_count = 1 + self.lens_windows.len();
    }

    /// A lens window's surface plan: its OWN pane space (`App::lenses`) walked
    /// at its size — the same geometry the primary uses, per window. Canvas
    /// leaf = the lens camera's view; other leaves = panes; seams = dividers;
    /// a torn-out workbench's live tiles = content surfaces. No canvas-inset
    /// content in a lens (the tile IS the lens's content story); chrome
    /// composites separately in `render_lens`.
    pub(in super::super) fn lens_plan(
        &self,
        ordinal: usize,
        w: u32,
        h: u32,
    ) -> Vec<crate::surface::Surface> {
        let frame = self
            .app
            .lenses
            .get(ordinal)
            .and_then(Option::as_ref)
            .filter(|_| {
                self.app
                    .blueprint_space(crate::action::SpaceRef::Lens(ordinal))
                    .is_none()
            })
            .map(|space| self.outer_rects(ordinal + 1, space, None, (w.max(1), h.max(1))));
        lens_content_plan_with_rects(
            &self.app,
            &self.content_sessions,
            ordinal,
            w,
            h,
            frame.as_deref(),
            &self.outer_float_rects(ordinal + 1),
        )
    }

    /// A pane's `PaneContent` in a LENS window's space.
    pub(in super::super) fn lens_pane_content(
        &self,
        ordinal: usize,
        id: crate::panes::PaneId,
    ) -> Option<PaneContent> {
        lens_pane_content(&self.app, ordinal, id)
    }
}
