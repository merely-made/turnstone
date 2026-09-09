// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shared process entry-point logic for Turnstone's two CEF hosting routes
//! on Windows: the ordinary `turnstone.exe` ([`run_direct`], CEF's process
//! sandbox stays off) and CEF's Windows bootstrap/client-DLL contract (the
//! sibling `sandbox_bootstrap_win` crate's `RunWinMain` export calls
//! [`run_bootstrap`], CEF's process sandbox is on). Both converge on
//! [`start_app`] once CEF's process-role probe confirms this is the browser
//! process, not a re-executed renderer/GPU/utility subprocess. Off Windows,
//! or without the `weld` feature, there is no probe and no second route:
//! [`run_direct`] goes straight to [`start_app`].
//!
//! See `design_docs/2026-08-03_turnstone_engine_adoption_plan.md`,
//! "E2-Weld Windows sandbox bootstrap", for the bundle layout and the two
//! launch commands.

use winit::event_loop::EventLoop;

/// Start tracing and run Turnstone's event loop. Shared tail for both hosting
/// routes; only reached once CEF's process-role probe (when one runs) has
/// confirmed this is the browser process.
fn start_app(address: Option<String>) -> i32 {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("turnstone=info")),
        )
        .init();

    // Which graph actually shows (restored session / fresh-from-address /
    // sample) is decided and logged inside `App::boot`, after the restore
    // attempt; claiming it here would lie on a restoring launch.
    match &address {
        Some(url) => tracing::info!(%url, "turnstone starting on an address"),
        None => tracing::info!("turnstone starting"),
    }

    let event_loop = EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut shell = crate::shell::Shell::new(proxy, address);
    event_loop.run_app(&mut shell).expect("event loop error");
    0
}

/// Run as the ordinary `turnstone.exe`.
///
/// CEF re-executes this same executable for renderer/GPU/utility
/// subprocesses; that role must be resolved before tracing, winit, or any
/// thread pool starts. This route can only run CEF's process sandbox off
/// (`CefSandboxMode::UnsandboxedTrustedContent`) — the ordinary executable
/// entry point has no way to manufacture the bootstrap's sandbox context.
/// `TURNSTONE_WELD_SANDBOX=sandboxed` on this route therefore fails with a
/// clear diagnostic (raised when the Weld engine is first selected, in
/// `shell::weld::initialize_runtime`) rather than silently downgrading to
/// unsandboxed.
pub fn run_direct(address: Option<String>) -> i32 {
    #[cfg(all(feature = "weld", windows))]
    {
        crate::shell::weld::set_sandbox_route(crate::shell::weld::WeldSandboxRoute::Direct);
        // CEF re-executes this executable for renderer/GPU/utility
        // subprocesses. It must inspect that role before tracing, winit, or
        // any thread pool.
        if let Some(cef_path) =
            std::env::var_os("TURNSTONE_CEF_PATH").or_else(|| std::env::var_os("CEF_PATH"))
        {
            match welding::CefRuntime::execute_process_from(
                std::path::Path::new(&cef_path),
                welding::CefSandboxMode::UnsandboxedTrustedContent,
            ) {
                Ok(Some(code)) => return code,
                Ok(None) => {}
                Err(error) => {
                    eprintln!("turnstone: CEF subprocess probe failed: {error}");
                    return 1;
                }
            }
        }
    }
    start_app(address)
}

/// Run inside CEF's Windows bootstrap/client-DLL contract.
///
/// Called only from the sibling `sandbox_bootstrap_win` crate's `RunWinMain`
/// cdylib export, itself called only by CEF's matching `bootstrap.exe`.
///
/// # Safety
///
/// `instance` and `sandbox_info` must be the unmodified values `RunWinMain`
/// received from the bootstrap. They must stay valid for the duration of
/// this call; because this function does not return until the event loop
/// (and the process) exits, extending the borrowed sandbox context to
/// `'static` below is sound — nothing outlives what the bootstrap keeps
/// alive for the call.
#[cfg(all(feature = "weld", windows))]
pub unsafe fn run_bootstrap(instance: *mut std::ffi::c_void, sandbox_info: *mut u8) -> i32 {
    // Sandboxed subprocesses may receive a reduced environment. The
    // supported bundle keeps libcef beside bootstrap.exe (and this crate's
    // client DLL), so the running executable's own directory is canonical;
    // fall back to the direct route's env vars only if that cannot be
    // resolved.
    let cef_path = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .or_else(|| {
            std::env::var_os("TURNSTONE_CEF_PATH")
                .or_else(|| std::env::var_os("CEF_PATH"))
                .map(std::path::PathBuf::from)
        });
    let Some(cef_path) = cef_path else {
        eprintln!("turnstone: cannot locate the bundled CEF distribution");
        return 110;
    };
    // SAFETY: see this function's safety section.
    let context: welding::CefWindowsSandboxContext<'static> =
        match unsafe { welding::CefWindowsSandboxContext::from_raw(instance, sandbox_info) } {
            Ok(context) => context,
            Err(error) => {
                eprintln!("turnstone: invalid CEF bootstrap sandbox context: {error}");
                return 111;
            }
        };
    match context.execute_process(&cef_path) {
        Ok(Some(code)) => return code,
        Ok(None) => {}
        Err(error) => {
            eprintln!("turnstone: sandboxed CEF subprocess probe failed: {error}");
            return 112;
        }
    }
    crate::shell::weld::set_sandbox_route(crate::shell::weld::WeldSandboxRoute::Bootstrap(
        context,
    ));
    start_app(std::env::args().nth(1))
}
