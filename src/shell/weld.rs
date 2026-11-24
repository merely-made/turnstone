// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's `weld.chromium` host implementation.
//!
//! The inker adapter stays CEF-free. This module owns the process runtime,
//! per-tile CEF producer, and product limits. Mere's version-pinned adapter
//! owns input, capability and ordered completion translation.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use inker::{
    Cookie, CursorShape, DocumentFindDirection, DocumentFindQuery, DragEvent, DragOperationSet,
    FocusReason, HttpAuthenticationAnswer, KeyboardEvent, MouseEvent, PermissionAnswer,
    PhysicalPosition, PointerEvent, SurfaceError, SurfaceSettings, UserAgentRequestId,
    WebFeatureStatus, WebRequestId, WebSurfaceCapabilities, WebSurfaceEvent,
};
use weld_engine::{WeldFrame, WeldProducerFactory, WeldSurface};
use welding::{CefRuntime, CefRuntimeConfig, CefSandboxMode, CefSurfaceConfig, CefSurfaceProducer};
#[cfg(windows)]
use welding::{
    HostWgpuContext, PlatformCefConfig as WindowsCefConfig,
    PlatformCefProducer as WindowsCefProducer,
};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(super) use windows::initialize_runtime;
#[cfg(windows)]
pub(crate) use windows::{WeldSandboxRoute, set_sandbox_route};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(super) use macos::runtime;
#[cfg(target_os = "macos")]
pub(super) use macos::set_appkit_drag_active;
#[cfg(target_os = "macos")]
pub(crate) use macos::{prepare_runtime, shutdown_runtime, start_runtime_pump};
use winit::dpi::PhysicalSize;

pub(super) struct TurnstoneWeldFactory {
    runtime: Arc<CefRuntime>,
    #[cfg(windows)]
    host: HostWgpuContext,
}

impl TurnstoneWeldFactory {
    pub(super) fn new(
        runtime: Arc<CefRuntime>,
        _device: wgpu::Device,
        _queue: wgpu::Queue,
    ) -> Self {
        Self {
            runtime,
            #[cfg(windows)]
            host: HostWgpuContext::new(_device, _queue),
        }
    }
}

impl WeldProducerFactory for TurnstoneWeldFactory {
    fn build(
        &self,
        request: &inker::SurfaceSpawnRequest,
    ) -> Result<Box<dyn WeldSurface>, SurfaceError> {
        let surface = weld_surface_config(request);
        #[cfg(target_os = "macos")]
        return macos::start_surface(&self.runtime, surface);
        #[cfg(windows)]
        {
            let profile_dir = PathBuf::from(&request.profile.user_data_dir);
            // Create the profile directory so a failure cannot silently select a different profile.
            std::fs::create_dir_all(&profile_dir).map_err(|error| {
                SurfaceError::SpawnFailed(format!(
                    "could not create Weld profile {}: {error}",
                    profile_dir.display()
                ))
            })?;
            let mut producer = WindowsCefProducer::new(
                self.runtime.as_ref(),
                WindowsCefConfig {
                    surface: weld_surface_config(request),
                },
                &self.host,
            )
            .map_err(weld_spawn_error)?;
            producer.set_visible(true).map_err(weld_spawn_error)?;
            Ok(Box::new(TurnstoneWeldSurface {
                inner: Some(weld_engine::WeldingSurface::new(
                    Box::new(producer),
                    &surface,
                )),
            }))
        }
    }
}

fn weld_surface_config(request: &inker::SurfaceSpawnRequest) -> CefSurfaceConfig {
    let mut surface = CefSurfaceConfig::default();
    surface.initial_url = request.url.clone();
    surface.initial_size = PhysicalSize::new(request.width.max(1), request.height.max(1));
    surface.handle_permission_requests = true;
    surface.handle_auth_challenges = true;
    let [red, green, blue, _] = SurfaceSettings::default().background_color;
    surface.background_color = Some([red, green, blue]);
    surface.user_data_dir = Some(PathBuf::from(&request.profile.user_data_dir));
    surface
}

/// Explicit `TURNSTONE_WELD_SANDBOX` override. Absent, the launch route
/// decides: `Sandboxed` under the bootstrap, `UnsandboxedTrustedContent` for
/// the direct executable. Present, it must match what the route can
/// deliver — an unsupported combination is a clear error, never a silent
/// downgrade.
#[derive(Clone, Copy, Debug)]
enum SandboxPreference {
    Sandboxed,
    Unsandboxed,
}

fn requested_sandbox_preference() -> Result<Option<SandboxPreference>, String> {
    parse_sandbox_preference(std::env::var("TURNSTONE_WELD_SANDBOX"))
}

fn parse_sandbox_preference(
    value: Result<String, std::env::VarError>,
) -> Result<Option<SandboxPreference>, String> {
    match value {
        Ok(value) => match value.as_str() {
            "sandboxed" => Ok(Some(SandboxPreference::Sandboxed)),
            "unsandboxed" => Ok(Some(SandboxPreference::Unsandboxed)),
            other => Err(format!(
                "TURNSTONE_WELD_SANDBOX={other:?} is not recognized; expected \"sandboxed\" or \"unsandboxed\""
            )),
        },
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("TURNSTONE_WELD_SANDBOX is not valid UTF-8".into())
        },
    }
}

#[cfg(test)]
mod sandbox_preference_tests {
    use super::*;

    #[test]
    fn unset_is_no_preference() {
        assert!(matches!(
            parse_sandbox_preference(Err(std::env::VarError::NotPresent)),
            Ok(None)
        ));
    }

    #[test]
    fn explicit_modes_preserve_the_requested_sandbox_preference() {
        assert!(matches!(
            parse_sandbox_preference(Ok("sandboxed".into())),
            Ok(Some(SandboxPreference::Sandboxed))
        ));
        assert!(matches!(
            parse_sandbox_preference(Ok("unsandboxed".into())),
            Ok(Some(SandboxPreference::Unsandboxed))
        ));
    }

    #[test]
    fn unrecognized_value_is_a_clear_error_not_a_silent_default() {
        let error = parse_sandbox_preference(Ok("yolo".into())).unwrap_err();
        assert!(error.contains("TURNSTONE_WELD_SANDBOX"));
        assert!(error.contains("yolo"));
    }

    #[cfg(windows)]
    #[test]
    fn non_unicode_value_is_an_explicit_error() {
        use std::os::windows::ffi::OsStringExt;
        let invalid = std::ffi::OsString::from_wide(&[0xD800]);
        assert!(invalid.to_str().is_none());
        let error =
            parse_sandbox_preference(Err(std::env::VarError::NotUnicode(invalid))).unwrap_err();
        assert_eq!(error, "TURNSTONE_WELD_SANDBOX is not valid UTF-8");
    }
}

struct TurnstoneWeldSurface {
    inner: Option<weld_engine::WeldingSurface>,
    #[cfg(target_os = "macos")]
    pending: Option<macos::PendingSurface>,
}

impl TurnstoneWeldSurface {
    fn ready(&mut self) -> Result<&mut weld_engine::WeldingSurface, SurfaceError> {
        self.inner.as_mut().ok_or_else(|| SurfaceError::Busy {
            operation: "starting the Weld browser".into(),
        })
    }

    #[cfg(target_os = "macos")]
    fn poll_startup(&mut self) -> Result<(), SurfaceError> {
        if let Some(pending) = &mut self.pending
            && let Some(surface) = pending.poll()?
        {
            self.inner = Some(surface);
            self.pending = None;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
impl Drop for TurnstoneWeldSurface {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            let _ = inner.into_inner().close();
        }
    }
}

const WELD_DEVTOOLS_UNAVAILABLE: &str = "CEF native DevTools windows are unsafe for accelerated off-screen browsers; Weld refuses them, and Turnstone has not exposed the separate CDP channel";

impl WeldSurface for TurnstoneWeldSurface {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        #[cfg(target_os = "macos")]
        if let Some(pending) = &mut self.pending {
            pending.config.surface.initial_size = PhysicalSize::new(width.max(1), height.max(1));
            return Ok(());
        }
        self.ready()?.resize(width, height)
    }

    fn acquire_frame(&mut self) -> Result<Option<WeldFrame>, SurfaceError> {
        #[cfg(target_os = "macos")]
        {
            self.poll_startup()?;
            if self.inner.is_none() {
                return Ok(None);
            }
        }
        self.ready()?.acquire_frame()
    }

    fn load_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        #[cfg(target_os = "macos")]
        if let Some(pending) = &mut self.pending {
            pending.config.surface.initial_url = url.into();
            pending.html = None;
            return Ok(());
        }
        self.ready()?.load_url(url)
    }

    fn load_html(&mut self, html: &str) -> Result<(), SurfaceError> {
        #[cfg(target_os = "macos")]
        if let Some(pending) = &mut self.pending {
            pending.html = Some(html.to_owned());
            return Ok(());
        }
        self.ready()?.load_html(html)
    }

    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.ready()?.reload()
    }

    fn stop(&mut self) -> Result<(), SurfaceError> {
        self.ready()?.stop()
    }

    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.ready()?.go_back()
    }

    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.ready()?.go_forward()
    }

    fn can_go_back(&self) -> bool {
        self.inner.as_ref().is_some_and(WeldSurface::can_go_back)
    }

    fn can_go_forward(&self) -> bool {
        self.inner.as_ref().is_some_and(WeldSurface::can_go_forward)
    }

    fn document_find(
        &mut self,
        query: &DocumentFindQuery,
        direction: DocumentFindDirection,
        _find_next: bool,
    ) -> Result<(), SurfaceError> {
        self.ready()?.document_find(query, direction, _find_next)
    }

    fn clear_document_find(&mut self) -> Result<(), SurfaceError> {
        self.ready()?.clear_document_find()
    }

    fn notify_mouse(&mut self, event: MouseEvent) -> Result<(), SurfaceError> {
        self.ready()?.notify_mouse(event)
    }

    fn notify_pointer(&mut self, event: PointerEvent) -> Result<(), SurfaceError> {
        self.ready()?.notify_pointer(event)
    }

    fn notify_drag(&mut self, event: DragEvent) -> Result<(), SurfaceError> {
        self.ready()?.notify_drag(event)
    }

    fn finish_drag_source(
        &mut self,
        position: PhysicalPosition,
        operation: DragOperationSet,
    ) -> Result<(), SurfaceError> {
        self.ready()?.finish_drag_source(position, operation)
    }

    fn notify_keyboard(&mut self, event: KeyboardEvent) -> Result<(), SurfaceError> {
        self.ready()?.notify_keyboard(event)
    }

    fn focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        #[cfg(target_os = "macos")]
        if let Some(pending) = &mut self.pending {
            pending.focus = Some(reason);
            return Ok(());
        }
        self.ready()?.focus(reason)
    }

    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        self.inner.as_mut().and_then(WeldSurface::poll_cursor_shape)
    }

    fn poll_web_event(&mut self) -> Option<WebSurfaceEvent> {
        self.inner.as_mut().and_then(WeldSurface::poll_web_event)
    }

    fn answer_permission(
        &mut self,
        id: UserAgentRequestId,
        answer: PermissionAnswer,
    ) -> Result<(), SurfaceError> {
        self.ready()?.answer_permission(id, answer)
    }

    fn answer_http_authentication(
        &mut self,
        id: UserAgentRequestId,
        answer: &HttpAuthenticationAnswer,
    ) -> Result<(), SurfaceError> {
        self.ready()?.answer_http_authentication(id, answer)
    }

    fn web_capabilities(&self) -> WebSurfaceCapabilities {
        host_capabilities(
            self.inner
                .as_ref()
                .map(WeldSurface::web_capabilities)
                .unwrap_or_default(),
        )
    }

    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        #[cfg(target_os = "macos")]
        if let Some(pending) = &mut self.pending {
            pending.settings = Some(settings.clone());
            return Ok(());
        }
        apply_weld_surface_settings(settings, |_| self.ready()?.apply_settings(settings))
    }

    fn set_cookie(&mut self, cookie: &Cookie) -> Result<(), SurfaceError> {
        self.ready()?.set_cookie(cookie)
    }

    fn delete_cookie(&mut self, cookie: &Cookie) -> Result<(), SurfaceError> {
        self.ready()?.delete_cookie(cookie)
    }

    fn request_cookies_for_url(&mut self, id: WebRequestId, url: &str) -> Result<(), SurfaceError> {
        self.ready()?.request_cookies_for_url(id, url)
    }

    fn request_script_result(
        &mut self,
        id: WebRequestId,
        script: &str,
    ) -> Result<(), SurfaceError> {
        self.ready()?.request_script_result(id, script)
    }
}

fn host_capabilities(mut capabilities: WebSurfaceCapabilities) -> WebSurfaceCapabilities {
    capabilities.devtools = WebFeatureStatus::unsupported(WELD_DEVTOOLS_UNAVAILABLE);
    capabilities.document.page_zoom = WebFeatureStatus::Partial {
        detail:
            "CEF accepts requested zoom; the threaded host cannot read back its effective level"
                .into(),
    };
    capabilities.pointer.pen = WebFeatureStatus::Partial {
        detail: "CEF accepts pen contacts, but winit does not identify pen versus touch".into(),
    };
    capabilities.pointer.contact_geometry = WebFeatureStatus::Partial {
        detail: "winit touch events do not supply contact geometry".into(),
    };
    capabilities.pointer.twist = WebFeatureStatus::Partial {
        detail: "winit touch events do not supply twist".into(),
    };
    capabilities.drag_drop.page_to_host = WebFeatureStatus::Partial {
        detail: "Turnstone offers plain text from page drags to native copy targets".into(),
    };
    capabilities.auth = WebFeatureStatus::Partial {
        detail: "answers are wired; native top-level and proxy authentication challenge delivery remain unqualified for the current CEF runtime".into(),
    };
    capabilities
        .degradation_reasons
        .push("popups, downloads and correlated snapshots await product controls".into());
    capabilities
}

fn apply_weld_surface_settings(
    settings: &SurfaceSettings,
    mut set_zoom_level: impl FnMut(f64) -> Result<(), SurfaceError>,
) -> Result<(), SurfaceError> {
    // Refuse at the host boundary before dispatching any producer operation.
    // Welding's native DevTools call is deliberately unavailable; its CDP
    // transport requires a separate control surface and opt-in configuration.
    if settings.dev_tools {
        return Err(SurfaceError::Unsupported(WELD_DEVTOOLS_UNAVAILABLE.into()));
    }
    // The contract carries a scale factor; CEF takes a logarithmic level,
    // where the scale is 1.2^level. A factor that is not a positive finite
    // number has no level, so it is refused here rather than handed to CEF
    // as a NaN.
    if !settings.zoom_factor.is_finite() || settings.zoom_factor <= 0.0 {
        return Err(SurfaceError::InputFailed(format!(
            "zoom factor {} is not a positive scale",
            settings.zoom_factor
        )));
    }
    set_zoom_level(settings.zoom_factor.ln() / 1.2_f64.ln())
}

fn weld_spawn_error(error: welding::WeldError) -> SurfaceError {
    SurfaceError::SpawnFailed(error.to_string())
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn weld_devtools_settings_are_refused_before_producer_dispatch() {
        assert!(matches!(
            host_capabilities(WebSurfaceCapabilities::default()).devtools,
            WebFeatureStatus::Unsupported { reason } if reason == WELD_DEVTOOLS_UNAVAILABLE
        ));
        let settings = SurfaceSettings {
            dev_tools: true,
            zoom_factor: 1.44,
            ..Default::default()
        };
        let result = apply_weld_surface_settings(&settings, |_| {
            panic!("unsupported DevTools settings must not dispatch to CEF")
        });
        assert!(matches!(result, Err(SurfaceError::Unsupported(reason))
            if reason == WELD_DEVTOOLS_UNAVAILABLE));
    }

    #[test]
    fn weld_zoom_settings_preserve_scale_validation_and_dispatch_failure() {
        for zoom_factor in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let settings = SurfaceSettings {
                zoom_factor,
                ..Default::default()
            };
            assert!(matches!(
                apply_weld_surface_settings(&settings, |_| {
                    panic!("invalid zoom must not reach CEF")
                }),
                Err(SurfaceError::InputFailed(_))
            ));
        }

        let settings = SurfaceSettings {
            zoom_factor: 1.44,
            ..Default::default()
        };
        let mut dispatched = Vec::new();
        apply_weld_surface_settings(&settings, |level| {
            dispatched.push(level);
            Ok(())
        })
        .unwrap();
        assert_eq!(dispatched.len(), 1);
        assert!((dispatched[0] - 2.0).abs() < 1e-12);
        assert!(matches!(
            apply_weld_surface_settings(&settings, |_| {
                Err(SurfaceError::InputFailed("CEF refused zoom".into()))
            }),
            Err(SurfaceError::InputFailed(reason)) if reason == "CEF refused zoom"
        ));
    }
}
