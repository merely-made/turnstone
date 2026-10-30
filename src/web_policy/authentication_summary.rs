// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn authentication_summary(
    challenge: &HttpAuthenticationChallenge,
    disposition: &str,
) -> AuthenticationSummary {
    AuthenticationSummary {
        host: challenge.protection_space.host.clone(),
        port: challenge.protection_space.port,
        realm: challenge.protection_space.realm.clone(),
        scheme: challenge.protection_space.scheme.clone(),
        is_proxy: challenge.protection_space.is_proxy,
        disposition: disposition.into(),
    }
}
