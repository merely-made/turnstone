// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn content_engines_with_client_limits(
    limits: crate::client_limits::BrowserClientLimits,
    #[cfg(feature = "scripted-webgl")] scripted: Option<&super::scripted_webgl::ScriptedWebGl>,
    #[cfg(feature = "scripted-webgl")] proxy: crate::shell::ShellEventProxy,
) -> SessionRegistry<Scene> {
    let remote = RemoteFetcher::new(limits.resources).with_smolweb_request_limits(
        limits.fetch.max_request_header_bytes,
        limits.fetch.max_response_header_line_bytes,
    );
    #[cfg(feature = "scripted-webgl")]
    if let Some(scripted) = scripted {
        let mut engines = content_engines_with_remote(remote.clone());
        scripted.register(&mut engines, remote, proxy);
        return engines;
    }
    content_engines_with_remote(remote)
}
