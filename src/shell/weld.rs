// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's Windows `weld.chromium` host implementation.
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
use welding::{
    CefRuntime, CefRuntimeConfig, CefSandboxMode, CefSurfaceConfig, CefSurfaceProducer,
    CefWindowsSandboxContext, HostWgpuContext, PlatformCefConfig as WindowsCefConfig,
    PlatformCefProducer as WindowsCefProducer,
};
use winit::dpi::PhysicalSize;

/// Route through which this process reached Weld's CEF hosting.
///
/// Set once, early in `main`/`RunWinMain` via [`set_sandbox_route`], before
/// the event loop starts; [`initialize_runtime`] reads it lazily whenever the
/// user first selects `weld.chromium`. Kept thread-local rather than in a
/// process-wide static: `CefWindowsSandboxContext` wraps raw, non-`Sync`
/// pointers, but everything that ever touches it — the entry point, the
/// winit event loop, and `ensure_weld_engine` — runs on the one thread that
/// called `main`/`RunWinMain`, so a thread-local needs no synchronization.
pub(crate) enum WeldSandboxRoute {
    /// The ordinary `turnstone.exe`. CEF's Windows sandbox context cannot be
    /// created on this path, so `CefSandboxMode::Sandboxed` is refused here
    /// (see [`requested_sandbox_preference`]).
    Direct,
    /// Entered through CEF's `bootstrap.exe` calling the sibling
    /// `sandbox_bootstrap_win` crate's `RunWinMain` cdylib export. Borrows
    /// the bootstrap-owned sandbox context and defaults to
    /// `CefSandboxMode::Sandboxed`.
    Bootstrap(CefWindowsSandboxContext<'static>),
}

thread_local! {
    static SANDBOX_ROUTE: std::cell::RefCell<Option<WeldSandboxRoute>> =
        const { std::cell::RefCell::new(None) };
}

/// Record which entry point this process launched through. Called once, by
/// `crate::launch::run_direct` or `crate::launch::run_bootstrap`, before the
/// event loop starts.
pub(crate) fn set_sandbox_route(route: WeldSandboxRoute) {
    SANDBOX_ROUTE.with(|cell| *cell.borrow_mut() = Some(route));
}

pub(super) struct TurnstoneWeldFactory {
    runtime: Arc<CefRuntime>,
    host: HostWgpuContext,
}

impl TurnstoneWeldFactory {
    pub(super) fn new(runtime: Arc<CefRuntime>, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            runtime,
            host: HostWgpuContext::new(device, queue),
        }
    }
}

impl WeldProducerFactory for TurnstoneWeldFactory {
    fn build(
        &self,
        request: &inker::SurfaceSpawnRequest,
    ) -> Result<Box<dyn WeldSurface>, SurfaceError> {
        let surface = weld_surface_config(request);
        let profile_dir = PathBuf::from(&request.profile.user_data_dir);
        // CEF creates a profile's files but not a missing parent chain. Make
        // the resolved per-node directory real before request-context creation
        // so a failed mkdir cannot silently fall back to another profile.
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
            inner: weld_engine::WeldingSurface::new(
                Box::new(producer),
                &surface,
            ),
        }))
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

pub(super) fn initialize_runtime(
    cef_path: PathBuf,
    cache_root: &Path,
) -> Result<Arc<CefRuntime>, String> {
    std::fs::create_dir_all(cache_root).map_err(|error| {
        format!(
            "could not create Weld cache root {}: {error}",
            cache_root.display()
        )
    })?;
    // The sandbox mode itself is decided below, from the launch route (plus
    // an optional explicit override); `UnsandboxedTrustedContent` here is
    // just a placeholder `CefRuntimeConfig::new` needs a value for.
    let mut config = CefRuntimeConfig::new(cef_path, CefSandboxMode::UnsandboxedTrustedContent);
    config.cache_path = Some(cache_root.to_path_buf());
    config.user_agent = std::env::var("TURNSTONE_WELD_USER_AGENT")
        .ok()
        .filter(|value| !value.is_empty());
    config.user_agent_product = std::env::var("TURNSTONE_WELD_USER_AGENT_PRODUCT")
        .ok()
        .filter(|value| !value.is_empty());
    if config.user_agent.is_some() && config.user_agent_product.is_some() {
        return Err(
            "set only TURNSTONE_WELD_USER_AGENT or TURNSTONE_WELD_USER_AGENT_PRODUCT, not both"
                .into(),
        );
    }

    let preference = requested_sandbox_preference()?;
    SANDBOX_ROUTE.with(move |cell| {
        let route = cell.borrow();
        match (
            route.as_ref().unwrap_or(&WeldSandboxRoute::Direct),
            preference,
        ) {
            (WeldSandboxRoute::Direct, Some(SandboxPreference::Sandboxed)) => Err(
                "TURNSTONE_WELD_SANDBOX=sandboxed requires launching through the CEF Windows \
                 bootstrap route (RunWinMain); the direct turnstone.exe entry point cannot \
                 create a sandbox context"
                    .to_string(),
            ),
            (WeldSandboxRoute::Direct, _) => {
                config.sandbox = CefSandboxMode::UnsandboxedTrustedContent;
                CefRuntime::initialize(config)
                    .map(Arc::new)
                    .map_err(|error| format!("could not initialize Weld CEF runtime: {error}"))
            },
            (WeldSandboxRoute::Bootstrap(_), Some(SandboxPreference::Unsandboxed)) => Err(
                "TURNSTONE_WELD_SANDBOX=unsandboxed is not supported when launched through the \
                 CEF Windows bootstrap route; that context always initializes \
                 CefSandboxMode::Sandboxed"
                    .to_string(),
            ),
            (WeldSandboxRoute::Bootstrap(context), _) => {
                config.sandbox = CefSandboxMode::Sandboxed;
                context
                    .initialize(config)
                    .map(Arc::new)
                    .map_err(|error| format!("could not initialize Weld CEF runtime: {error}"))
            },
        }
    })
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
    match std::env::var("TURNSTONE_WELD_SANDBOX") {
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
        // SAFETY: test-only env mutation; this crate's test binary does not
        // run these tests concurrently with anything else reading this var.
        unsafe {
            std::env::remove_var("TURNSTONE_WELD_SANDBOX");
        }
        assert!(matches!(requested_sandbox_preference(), Ok(None)));
    }

    #[test]
    fn unrecognized_value_is_a_clear_error_not_a_silent_default() {
        // SAFETY: see `unset_is_no_preference`.
        unsafe {
            std::env::set_var("TURNSTONE_WELD_SANDBOX", "yolo");
        }
        let error = requested_sandbox_preference().unwrap_err();
        assert!(error.contains("TURNSTONE_WELD_SANDBOX"));
        // SAFETY: see `unset_is_no_preference`.
        unsafe {
            std::env::remove_var("TURNSTONE_WELD_SANDBOX");
        }
    }
}

struct TurnstoneWeldSurface {
    inner: weld_engine::WeldingSurface,
}

const WELD_DEVTOOLS_UNAVAILABLE: &str = "CEF 151 native DevTools windows are unsafe for accelerated off-screen browsers; Weld refuses them, and Turnstone has not exposed the separate CDP channel";

impl WeldSurface for TurnstoneWeldSurface {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.inner.resize(width, height)
    }

    fn acquire_frame(&mut self) -> Result<Option<WeldFrame>, SurfaceError> {
        self.inner.acquire_frame()
    }

    fn load_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        self.inner.load_url(url)
    }

    fn load_html(&mut self, html: &str) -> Result<(), SurfaceError> {
        self.inner.load_html(html)
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

    fn document_find(
        &mut self,
        query: &DocumentFindQuery,
        direction: DocumentFindDirection,
        _find_next: bool,
    ) -> Result<(), SurfaceError> {
        self.inner.document_find(query, direction, _find_next)
    }

    fn clear_document_find(&mut self) -> Result<(), SurfaceError> {
        self.inner.clear_document_find()
    }

    fn notify_mouse(&mut self, event: MouseEvent) -> Result<(), SurfaceError> {
        self.inner.notify_mouse(event)
    }

    fn notify_pointer(&mut self, event: PointerEvent) -> Result<(), SurfaceError> {
        self.inner.notify_pointer(event)
    }

    fn notify_drag(&mut self, event: DragEvent) -> Result<(), SurfaceError> {
        self.inner.notify_drag(event)
    }

    fn finish_drag_source(
        &mut self,
        position: PhysicalPosition,
        operation: DragOperationSet,
    ) -> Result<(), SurfaceError> {
        self.inner.finish_drag_source(position, operation)
    }

    fn notify_keyboard(&mut self, event: KeyboardEvent) -> Result<(), SurfaceError> {
        self.inner.notify_keyboard(event)
    }

    fn focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        self.inner.focus(reason)
    }

    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        self.inner.poll_cursor_shape()
    }

    fn poll_web_event(&mut self) -> Option<WebSurfaceEvent> {
        self.inner.poll_web_event()
    }

    fn answer_permission(
        &mut self,
        id: UserAgentRequestId,
        answer: PermissionAnswer,
    ) -> Result<(), SurfaceError> {
        self.inner.answer_permission(id, answer)
    }

    fn answer_http_authentication(
        &mut self,
        id: UserAgentRequestId,
        answer: &HttpAuthenticationAnswer,
    ) -> Result<(), SurfaceError> {
        self.inner.answer_http_authentication(id, answer)
    }

    fn web_capabilities(&self) -> WebSurfaceCapabilities {
        host_capabilities(self.inner.web_capabilities())
    }

    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        apply_weld_surface_settings(settings, |_| self.inner.apply_settings(settings))
    }

    fn set_cookie(&mut self, cookie: &Cookie) -> Result<(), SurfaceError> {
        self.inner.set_cookie(cookie)
    }

    fn delete_cookie(&mut self, cookie: &Cookie) -> Result<(), SurfaceError> {
        self.inner.delete_cookie(cookie)
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
        detail: "page drags are observable; Turnstone has no native winit drag loop".into(),
    };
    capabilities.auth = WebFeatureStatus::Partial {
        detail: "answers are wired; CEF 151 did not emit a top-level authentication challenge, and proxy authentication is untested".into(),
    };
    capabilities.degradation_reasons.push(
        "popups, downloads, native drag loops and correlated snapshots await product controls"
            .into(),
    );
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
