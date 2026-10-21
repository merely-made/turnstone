// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Native web handoff after an exact owner acceptance, never from a reading.
use chirograph::IntentInvocation;
use graphshell::handlers::{OpenAddressV1, SYSTEM_DEFAULT_HANDLER, intent_id};

pub(super) trait Browser {
    async fn open(&mut self, address: &str) -> Result<(), String>;
}

pub(super) struct SystemBrowser;

/// Native dispatch deliberately supports only ordinary web URLs. Neither
/// graph content nor owner offers supply an executable or command arguments.
pub(super) fn address(request: &IntentInvocation) -> Result<Option<String>, String> {
    if request.intent != intent_id(SYSTEM_DEFAULT_HANDLER) {
        return Ok(None);
    }
    let payload: OpenAddressV1 = serde_json::from_slice(&request.payload)
        .map_err(|error| format!("Invalid browser request: {error}"))?;
    let url = url::Url::parse(&payload.address)
        .map_err(|error| format!("Invalid web address: {error}"))?;
    if payload.handler != SYSTEM_DEFAULT_HANDLER
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
    {
        return Err("Browser handoff requires an HTTP or HTTPS address.".into());
    }
    Ok(Some(payload.address))
}

impl Browser for SystemBrowser {
    async fn open(&mut self, address: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        let mut command = tokio::process::Command::new("/usr/bin/open");
        #[cfg(target_os = "linux")]
        let mut command = tokio::process::Command::new("xdg-open");
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut command = tokio::process::Command::new("rundll32.exe");
            command.arg("url.dll,FileProtocolHandler");
            command
        };
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        return Err("This native host does not support browser handoff.".into());

        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        {
            command.arg(address).kill_on_drop(true);
            let status = tokio::time::timeout(std::time::Duration::from_secs(3), command.status())
                .await
                .map_err(|_| "The browser handoff timed out; its outcome is unknown.".to_string())?
                .map_err(|error| format!("Browser handoff failed: {error}"))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("The system browser handoff exited with {status}."))
            }
        }
    }
}
