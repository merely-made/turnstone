// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use welding::CefWindowsSandboxContext;

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

pub(in crate::shell) fn initialize_runtime(
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
