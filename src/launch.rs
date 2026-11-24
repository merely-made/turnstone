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

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use winit::event_loop::EventLoop;

use crate::graph_coop::{guest, worker};

enum Launch {
    Address(Option<String>),
    Contact(PathBuf),
    Annotation(PathBuf, String),
    Invitation(guest::Visit),
}

impl Launch {
    fn parse(mut arguments: impl Iterator<Item = OsString>) -> Result<Self, &'static str> {
        let Some(first) = arguments.next() else {
            return Ok(Self::Address(None));
        };
        match first.to_str() {
            Some("--open-annotation") => {
                let file = arguments.next().filter(|p| !p.is_empty());
                let note = arguments
                    .next()
                    .and_then(|arg| arg.into_string().ok())
                    .filter(|id| !id.is_empty());
                match (file, note, arguments.next()) {
                    (Some(path), Some(note), None) => Ok(Self::Annotation(path.into(), note)),
                    _ => Err("usage: turnstone --open-annotation FILE NOTE_IRI"),
                }
            },
            Some("--contact") => {
                let path = arguments.next().filter(|path| !path.is_empty());
                match (path, arguments.next()) {
                    (Some(path), None) => Ok(Self::Contact(path.into())),
                    _ => Err("usage: turnstone --contact FILE"),
                }
            },
            Some("--invitation") => {
                let invitation = arguments.next().filter(|path| !path.is_empty());
                let flag = arguments.next();
                let contact = arguments.next().filter(|path| !path.is_empty());
                match (invitation, flag.as_deref(), contact, arguments.next()) {
                    (Some(invitation), Some(flag), Some(contact), None)
                        if flag == OsStr::new("--contact") =>
                    {
                        Ok(Self::Invitation(guest::Visit {
                            invitation: invitation.into(),
                            contact: contact.into(),
                        }))
                    },
                    _ => Err("usage: turnstone --invitation FILE --contact FILE"),
                }
            },
            Some(address) => Ok(Self::Address(Some(address.into()))),
            None => Err("the address must be valid UTF-8"),
        }
    }
}

fn launch_command_line() -> i32 {
    match Launch::parse(std::env::args_os().skip(1)) {
        Ok(Launch::Address(address)) => start_app(address, None),
        Ok(Launch::Annotation(path, note)) => {
            let target = turnstone_ontology::selected_profile()
                .map_err(str::to_owned)
                .and_then(|profile| turnstone_ontology::open_annotation(profile, &path, &note));
            match target {
                Ok(address) => start_app(Some(address.into()), None),
                Err(error) => {
                    eprintln!("turnstone: {error}");
                    1
                },
            }
        },
        Ok(Launch::Invitation(visit)) => start_app(None, Some(visit)),
        Ok(Launch::Contact(path)) => match worker::export_contact(&path) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("turnstone: could not export public co-op contact: {error}");
                1
            },
        },
        Err(error) => {
            eprintln!("turnstone: {error}");
            2
        },
    }
}

/// Start tracing and run Turnstone's event loop. Shared tail for both hosting
/// routes; only reached once CEF's process-role probe (when one runs) has
/// confirmed this is the browser process.
fn start_app(address: Option<String>, visit: Option<guest::Visit>) -> i32 {
    use tracing_subscriber::prelude::*;

    let diagnostic_run = apparatus_core::RunId::from(uuid::Uuid::new_v4().to_string());
    let reactive_boot = apparatus_core::SourceBootId::from(uuid::Uuid::new_v4().to_string());
    let (reactive_layer, reactive_receiver) =
        crate::reactive_trace::channel(diagnostic_run.clone(), reactive_boot.clone());
    let fmt_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("turnstone=info"));
    let reactive_filter = tracing_subscriber::filter::filter_fn(|metadata| {
        matches!(
            metadata.target(),
            "cambium::reactive" | "meristem::reactive"
        )
    });
    let reactive_layer = reactive_layer.with_filter(reactive_filter);
    let fmt_layer = tracing_subscriber::fmt::layer().with_filter(fmt_filter);
    tracing_subscriber::registry()
        .with(reactive_layer)
        .with(fmt_layer)
        .init();

    if let Err(error) = turnstone_ontology::selected_profile() {
        tracing::warn!(%error, "Turnstone ontology lens unavailable");
    }

    // Which graph actually shows (restored session / fresh-from-address /
    // sample) is decided and logged inside `App::boot`, after the restore
    // attempt; claiming it here would lie on a restoring launch.
    match (&address, visit.is_some()) {
        (Some(url), _) => tracing::info!(%url, "turnstone starting on an address"),
        (_, true) => tracing::info!("turnstone starting with a co-op invitation"),
        _ => tracing::info!("turnstone starting"),
    }

    #[cfg(all(feature = "weld", target_os = "macos"))]
    let pump_cef = crate::shell::weld::prepare_runtime();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut shell =
        crate::shell::Shell::new_with_diagnostic_run(proxy, address, diagnostic_run, visit);
    if shell
        .attach_reactive_trace(reactive_receiver, &reactive_boot)
        .is_err()
    {
        tracing::warn!("reactive diagnostics receiver did not attach to this run");
    }
    #[cfg(all(feature = "weld", target_os = "macos"))]
    let cef_timer = pump_cef.then(crate::shell::weld::start_runtime_pump);
    let result = event_loop.run_app(shell);
    #[cfg(all(feature = "weld", target_os = "macos"))]
    if let Some(timer) = cef_timer {
        timer.invalidate();
        crate::shell::weld::shutdown_runtime();
    }
    result.expect("event loop error");
    0
}

/// Run as the ordinary `turnstone.exe` with its native command-line arguments.
pub fn run_command_line() -> i32 {
    run_direct_with(launch_command_line)
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
    run_direct_with(|| start_app(address, None))
}

fn run_direct_with(start: impl FnOnce() -> i32) -> i32 {
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
                Ok(None) => {},
                Err(error) => {
                    eprintln!("turnstone: CEF subprocess probe failed: {error}");
                    return 1;
                },
            }
        }
    }
    start()
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
            },
        };
    match context.execute_process(&cef_path) {
        Ok(Some(code)) => return code,
        Ok(None) => {},
        Err(error) => {
            eprintln!("turnstone: sandboxed CEF subprocess probe failed: {error}");
            return 112;
        },
    }
    crate::shell::weld::set_sandbox_route(crate::shell::weld::WeldSandboxRoute::Bootstrap(context));
    launch_command_line()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Launch, &'static str> {
        Launch::parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn co_op_launch_preserves_the_selected_files() {
        let Launch::Invitation(visit) = parse(&[
            "--invitation",
            "shared graph.json",
            "--contact",
            "my contact.json",
        ])
        .unwrap() else {
            panic!("expected an invitation launch");
        };
        assert_eq!(visit.invitation, PathBuf::from("shared graph.json"));
        assert_eq!(visit.contact, PathBuf::from("my contact.json"));

        let Launch::Contact(path) = parse(&["--contact", "public contact.json"]).unwrap() else {
            panic!("expected a contact export");
        };
        assert_eq!(path, PathBuf::from("public contact.json"));
    }

    #[test]
    fn ontology_file_open_requires_a_note_identity() {
        assert!(
            matches!(parse(&["--open-annotation", "notes.json", "urn:note"]),
            Ok(Launch::Annotation(path, note)) if path == PathBuf::from("notes.json") && note == "urn:note")
        );
        assert!(parse(&["--open-annotation", "notes.json"]).is_err());
    }

    #[test]
    fn malformed_co_op_arguments_fail_before_startup() {
        assert!(parse(&["--contact"]).is_err());
        assert!(parse(&["--contact", "contact.json", "extra"]).is_err());
        assert!(parse(&["--invitation", "invite.json"]).is_err());
        assert!(parse(&["--invitation", "invite.json", "--other", "contact.json"]).is_err());
        assert!(parse(&["--invitation", "", "--contact", "contact.json"]).is_err());
        assert!(matches!(
            parse(&["mere://resident/notes"]).unwrap(),
            Launch::Address(Some(address)) if address == "mere://resident/notes"
        ));
    }
}
