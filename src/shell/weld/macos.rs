// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::cell::{Cell, RefCell};
use welding::macos_cef::{MacosCefConfig, MacosCefProducer, PreparedMacosCefProfile};

thread_local! {
    static RUNTIME: RefCell<Option<Result<Arc<CefRuntime>, String>>> = const { RefCell::new(None) };
    static APPKIT_DRAG_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

fn frameworks() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let frameworks = exe
        .ancestors()
        .nth(2)
        .unwrap_or(Path::new(""))
        .join("Frameworks");
    if !frameworks
        .join("Chromium Embedded Framework.framework")
        .is_dir()
        || !frameworks.join("turnstone Helper (Renderer).app").is_dir()
    {
        return Err("Launch the Turnstone app bundle containing CEF and its helpers".into());
    }
    Ok(frameworks)
}

pub(crate) fn prepare_runtime() -> bool {
    let Ok(frameworks) = frameworks() else {
        return false;
    };
    let result = (|| {
        let preference = requested_sandbox_preference()?;
        let sandbox = match preference {
            Some(SandboxPreference::Unsandboxed) => CefSandboxMode::UnsandboxedTrustedContent,
            _ => CefSandboxMode::Sandboxed,
        };
        let cache = crate::session::default_turnstone_root()
            .join("weld")
            .join("cef-cache");
        std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        let mut config = CefRuntimeConfig::new(frameworks, sandbox);
        config.cache_path = Some(cache.canonicalize().map_err(|error| error.to_string())?);
        config.user_agent = std::env::var("TURNSTONE_WELD_USER_AGENT")
            .ok()
            .filter(|v| !v.is_empty());
        config.user_agent_product = std::env::var("TURNSTONE_WELD_USER_AGENT_PRODUCT")
            .ok()
            .filter(|v| !v.is_empty());
        if config.user_agent.is_some() && config.user_agent_product.is_some() {
            return Err(
                "Set only one of TURNSTONE_WELD_USER_AGENT and TURNSTONE_WELD_USER_AGENT_PRODUCT"
                    .into(),
            );
        }
        if std::env::var_os("TURNSTONE_SCENARIO").is_some() {
            // Isolated headed fixtures must not access the user's login keychain.
            config
                .command_line_switches
                .push(("use-mock-keychain".into(), None));
        }
        CefRuntime::initialize(config)
            .map(Arc::new)
            .map_err(|error| error.to_string())
    })();
    if let Err(error) = &result {
        tracing::warn!(%error, "could not initialize macOS Weld");
    }
    let ready = result.is_ok();
    RUNTIME.with(|runtime| *runtime.borrow_mut() = Some(result));
    ready
}

pub(in crate::shell) fn runtime() -> Result<Arc<CefRuntime>, String> {
    RUNTIME.with(|runtime| {
        runtime
            .borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Err("Launch the Turnstone app bundle to initialize Weld".into()))
    })
}

pub(crate) fn start_runtime_pump() -> objc2::rc::Retained<objc2_foundation::NSTimer> {
    use objc2_foundation::NSTimer;

    // CEF can dispatch AppKit input while pumping. Let it run from AppKit's
    // default loop while Winit has a handler, between individual callbacks.
    let callback = block2::RcBlock::new(|_: std::ptr::NonNull<NSTimer>| {
        if !APPKIT_DRAG_ACTIVE.get() && let Ok(runtime) = runtime() {
            runtime.do_message_loop_work();
        }
    });
    // SAFETY: The block captures no state. The timer is created and invalidated
    // on the main thread, and only the main thread owns the CEF runtime.
    unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.01, true, &callback) }
}

pub(in crate::shell) fn set_appkit_drag_active(active: bool) {
    // AppKit's drag tracking owns the event queue until its terminal callback.
    APPKIT_DRAG_ACTIVE.set(active);
}

pub(crate) fn shutdown_runtime() {
    RUNTIME.with(|runtime| {
        let runtime = runtime.borrow_mut().take();
        if let Some(Ok(runtime)) = runtime {
            // CEF processes the close messages posted by dropping the surfaces.
            runtime.do_message_loop_work();
        }
    });
}

pub(super) struct PendingSurface {
    pub(super) config: MacosCefConfig,
    profile: PreparedMacosCefProfile,
    pub(super) settings: Option<SurfaceSettings>,
    pub(super) html: Option<String>,
    pub(super) focus: Option<FocusReason>,
}

pub(super) fn start_surface(
    runtime: &CefRuntime,
    surface: CefSurfaceConfig,
) -> Result<Box<dyn WeldSurface>, SurfaceError> {
    let config = MacosCefConfig { surface };
    let profile = MacosCefProducer::prepare_profile(runtime, &config).map_err(weld_spawn_error)?;
    Ok(Box::new(TurnstoneWeldSurface {
        inner: None,
        pending: Some(PendingSurface {
            config,
            profile,
            settings: None,
            html: None,
            focus: None,
        }),
    }))
}

impl PendingSurface {
    pub(super) fn poll(&mut self) -> Result<Option<weld_engine::WeldingSurface>, SurfaceError> {
        let Some(mut producer) =
            MacosCefProducer::try_new_with_prepared_profile(&self.config, &mut self.profile)
                .map_err(weld_spawn_error)?
        else {
            return Ok(None);
        };
        producer.set_visible(true).map_err(weld_spawn_error)?;
        let mut surface =
            weld_engine::WeldingSurface::new(Box::new(producer), &self.config.surface);
        let setup = (|| {
            if let Some(html) = self.html.take() {
                surface.load_html(&html)?;
            }
            if let Some(settings) = self.settings.take() {
                apply_weld_surface_settings(&settings, |_| surface.apply_settings(&settings))?;
            }
            if let Some(focus) = self.focus.take() {
                surface.focus(focus)?;
            }
            Ok(())
        })();
        match setup {
            Ok(()) => Ok(Some(surface)),
            Err(error) => {
                let _ = surface.into_inner().close();
                Err(error)
            },
        }
    }
}
