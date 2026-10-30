// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn content_engines_with_client_limits(
    limits: crate::client_limits::BrowserClientLimits,
) -> SessionRegistry<Scene> {
    content_engines_with_remote(
        RemoteFetcher::new(limits.resources).with_smolweb_request_limits(
            limits.fetch.max_request_header_bytes,
            limits.fetch.max_response_header_line_bytes,
        ),
    )
}
