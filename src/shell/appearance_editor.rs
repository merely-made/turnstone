// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone owns the workshop window and selection policy. Tabard supplies
//! its shared editor model and native bindings on the shell's existing device.

use super::Shell;
use crate::settings_provider::ApplicationSettingsProvider;
use cambium_genet_winit_host::{CloseRequest, HostOptions, WindowFrame, WinitHost};
use cambium_rootstock::IdlePolicy;
use std::sync::Arc;
use tabard::theme::choice::ThemeChoice;
use tabard_workshop::{
    WorkshopState,
    native_host::{WorkshopHost, native_hooks, native_init},
};
use winit::{event::WindowEvent, event_loop::ActiveEventLoop, window::WindowId};

pub(super) struct EditorWindow {
    pub host: WorkshopHost,
    completion: Option<std::rc::Rc<std::cell::Cell<Option<bool>>>>,
}

impl Shell {
    pub(super) fn service_theme_editor(&mut self, event_loop: &ActiveEventLoop) -> IdlePolicy {
        for action in self.live_settings.take_actions() {
            self.act(action);
        }
        if self.theme_editor_requested {
            self.theme_editor_requested = false;
            if let Some(editor) = &self.theme_editor {
                if let Some(window) = editor.host.native_window() {
                    window.focus_window();
                }
            } else if let Some(core) = self.host.as_ref().map(|host| host.shared_core()) {
                let result = (|| {
                    let provider = ApplicationSettingsProvider::load(&self.app.data_root)
                        .map_err(|error| error.to_string())?;
                    if let Some(error) = &provider.catalog().error {
                        return Err(error.clone());
                    }
                    let choice = provider
                        .settings()
                        .theme
                        .clone()
                        .unwrap_or_else(ThemeChoice::default);
                    let resolved = provider.catalog().resolve(&choice)?;
                    let mut state = WorkshopState::load(provider.catalog().path.clone())
                        .map_err(|error| error.to_string())?;
                    state.set_protected_export_paths(vec![pandect::application_settings_path(
                        &self.app.data_root,
                    )]);
                    state.set_protected_export_directories(vec![self.app.data_root.clone()]);
                    state.edit_definition(&resolved.theme, resolved.resolved.theme_mode)?;
                    let wake_proxy = self.proxy.clone();
                    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                        let _ = wake_proxy.send_event(());
                    });
                    let hooks = native_hooks(|artifact| {
                        let extension = if artifact.format == tabard_workshop::ExportFormat::Css {
                            "css"
                        } else {
                            "json"
                        };
                        cambium_genet_winit_host::choose_save_path(
                            "Export theme",
                            &artifact.suggested_name,
                            &[extension],
                        )
                    });
                    let (hooks, completion) = super::appearance_receipt::workshop_hooks(hooks)?;
                    let mut host = WinitHost::with_shared_render_core(
                        HostOptions {
                            title: "Turnstone — Edit themes".into(),
                            initial_logical_size: (1180.0, 800.0),
                            window_frame: WindowFrame::App,
                            ..Default::default()
                        },
                        move |_, commands, _| native_init(state, commands),
                        hooks,
                        core,
                        wake,
                    );
                    host.open(event_loop)?;
                    Ok(EditorWindow { host, completion })
                })();
                match result {
                    Ok(editor) => {
                        self.live_settings.report_status(None);
                        self.theme_editor = Some(editor);
                    },
                    Err(error) => {
                        if self.shared_scenario.is_some() {
                            self.main_receipt
                                .errors
                                .push(format!("Could not open theme workshop: {error}"));
                        }
                        self.live_settings
                            .report_status(Some(format!("Could not open theme workshop: {error}")));
                        tracing::warn!(%error, "could not open shared theme workshop");
                    },
                }
            }
        }
        let mut idle = IdlePolicy::Wait;
        if let Some(editor) = self.theme_editor.as_mut() {
            editor.host.wake_turn();
            idle = editor.host.idle_turn();
            if self.close_after_theme_editor
                && !editor.host.s.close_requested
                && editor
                    .host
                    .s
                    .runner
                    .as_ref()
                    .is_some_and(|runner| !runner.state().close_requested())
            {
                self.close_after_theme_editor = false;
            }
        }
        if self
            .theme_editor
            .as_ref()
            .is_some_and(|editor| editor.host.s.close_requested)
        {
            if let Some(editor) = self.theme_editor.take()
                && let Some(completion) = editor.completion
            {
                self.main_receipt.workshop = Some(completion.get() == Some(true));
            }
            // Saving the library does not select a theme. Refresh available
            // definitions while preserving the application's existing choice.
            for pane in self.renderers.settings.values_mut() {
                pane.reload_themes();
            }
            if let Ok(provider) = ApplicationSettingsProvider::load(&self.app.data_root) {
                self.live_settings.publish_provider(&provider);
            }
            self.poll_live_settings();
            if self.close_after_theme_editor {
                self.act(crate::action::Action::SaveSession);
                self.release_place_worker();
                event_loop.exit();
            } else if let Some(window) = &self.window {
                window.focus_window();
                window.request_redraw();
            }
            idle = IdlePolicy::Wait;
        }
        idle
    }

    pub(super) fn theme_editor_event(&mut self, window_id: WindowId, event: &WindowEvent) -> bool {
        let Some(editor) = self.theme_editor.as_mut().filter(|editor| {
            editor
                .host
                .native_window()
                .is_some_and(|window| window.id() == window_id)
        }) else {
            return false;
        };
        editor.host.handle_window_event(event.clone());
        self.request_redraw();
        true
    }

    /// An app quit owes the workshop its normal save/discard/cancel decision.
    pub(super) fn request_close_with_theme_editor(&mut self) -> bool {
        let Some(editor) = self.theme_editor.as_mut() else {
            return false;
        };
        self.close_after_theme_editor = true;
        editor.host.request_close(CloseRequest::Native);
        if let Some(window) = editor.host.native_window() {
            window.focus_window();
            window.request_redraw();
        }
        true
    }
}

pub(super) fn workshop_pending(requested: bool, open: bool) -> bool {
    requested || open
}

pub(super) fn editor_deadline(
    current: std::time::Instant,
    now: std::time::Instant,
    idle: IdlePolicy,
) -> winit::event_loop::ControlFlow {
    use winit::event_loop::ControlFlow;
    match idle {
        IdlePolicy::Wait => ControlFlow::WaitUntil(current),
        IdlePolicy::Animate(delay) => ControlFlow::WaitUntil(current.min(now + delay)),
        IdlePolicy::A11yWake => ControlFlow::Poll,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_wait_yields_during_request_and_open_workshop_then_resumes() {
        #[derive(Default)]
        struct Lifecycle {
            requested: bool,
            open: bool,
            applied: usize,
        }
        impl taproot::Automatable for Lifecycle {
            fn with_surfaces<R>(&self, f: impl FnOnce(&[taproot::ProbeSurface<'_>]) -> R) -> R {
                f(&[])
            }
            fn snapshot(&self) -> taproot::ProbeSnapshot {
                Default::default()
            }
            fn drain_events(&mut self) -> Vec<String> {
                Vec::new()
            }
            fn act(&mut self, label: &str) -> bool {
                match label {
                    "open" => self.requested = true,
                    "apply" => self.applied += 1,
                    _ => return false,
                }
                true
            }
            fn press(&mut self, _: f32, _: f32) {}
            fn moved(&mut self, _: f32, _: f32) {}
            fn release(&mut self, _: f32, _: f32) {}
            fn busy(&mut self) -> Option<bool> {
                Some(workshop_pending(self.requested, self.open))
            }
        }
        impl taproot::Driveable for Lifecycle {}
        let mut scenario = taproot::Scenario::parse("act open\nwait 20\nact apply").unwrap();
        let mut host = Lifecycle::default();
        assert_eq!(scenario.tick(&mut host), taproot::Progress::Running);
        assert!(host.requested);
        assert_eq!(scenario.tick(&mut host), taproot::Progress::Running);
        // Every tick returns to the caller, so it can service native events.
        for _ in 0..3 {
            assert_eq!(scenario.tick(&mut host), taproot::Progress::Running);
            assert_eq!(host.applied, 0);
        }
        host.open = true;
        host.requested = false;
        for _ in 0..3 {
            assert_eq!(scenario.tick(&mut host), taproot::Progress::Running);
            assert_eq!(host.applied, 0);
        }
        host.open = false;
        assert_eq!(scenario.tick(&mut host), taproot::Progress::Running);
        assert_eq!(host.applied, 1);
        assert_eq!(scenario.tick(&mut host), taproot::Progress::Done);
        let outcome = scenario.finish();
        assert!(outcome.ok);
        assert!(outcome.log.iter().any(|line| line == "waited 6 frames"));
    }

    #[test]
    fn editor_deadline_composes_with_document_polling_without_idle_starvation() {
        let now = std::time::Instant::now();
        let browser = now + std::time::Duration::from_millis(20);
        assert_eq!(
            editor_deadline(browser, now, IdlePolicy::Wait),
            winit::event_loop::ControlFlow::WaitUntil(browser)
        );
        assert_eq!(
            editor_deadline(
                browser,
                now,
                IdlePolicy::Animate(std::time::Duration::from_millis(5))
            ),
            winit::event_loop::ControlFlow::WaitUntil(now + std::time::Duration::from_millis(5))
        );
        assert_eq!(
            editor_deadline(
                browser,
                now,
                IdlePolicy::Animate(std::time::Duration::from_millis(50))
            ),
            winit::event_loop::ControlFlow::WaitUntil(browser)
        );
        assert_eq!(
            editor_deadline(browser, now, IdlePolicy::A11yWake),
            winit::event_loop::ControlFlow::Poll
        );
    }
}
