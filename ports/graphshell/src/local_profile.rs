// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The installed browser profile: one local, unassociated provenance scope.
//!
//! It names this browser profile's store and attributes local work to it
//! without claiming a Personae identity, custody or a paired device. Signing,
//! grants and private credentials need an admitted provider; reading, capture
//! and copying public references do not.

use serde::Deserialize;
use uuid::Uuid;

use pandect::access::AccessContext;
use crate::mere_host::SelectedPersonaRef;

pub const LOCAL_PROFILE_SCHEMA: &str = "graphshell.local-profile/v1";
/// The installed profile's one IndexedDB database; fixture routes keep theirs.
pub const LOCAL_PROFILE_DATABASE: &str = "graphshell-local-v1";
const LOCAL_SCOPE_PREFIX: &str = "graphshell://local-scope/";
const LOCAL_PROFILE_KIND: &str = "profile:graphshell-browser-local";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Browser {
    Chromium,
    Firefox,
}

impl Browser {
    fn as_str(self) -> &'static str {
        match self {
            Self::Chromium => "chromium",
            Self::Firefox => "firefox",
        }
    }
}

/// What the extension keeps in its own storage for the installed profile.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalProfileV1 {
    schema: String,
    id: Uuid,
    browser: Browser,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalProfile {
    id: Uuid,
    browser: Browser,
}

impl LocalProfile {
    pub fn new(id: Uuid, browser: Browser) -> Self {
        Self { id, browser }
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        let record: LocalProfileV1 = serde_json::from_str(json)
            .map_err(|error| format!("invalid local profile: {error}"))?;
        if record.schema != LOCAL_PROFILE_SCHEMA {
            return Err(format!(
                "unsupported local profile schema {}",
                record.schema
            ));
        }
        if record.id.is_nil() {
            return Err("local profile id is empty".into());
        }
        Ok(Self::new(record.id, record.browser))
    }

    /// The scope local work is attributed to, in the persona slot.
    pub fn scope(&self) -> String {
        format!("{LOCAL_SCOPE_PREFIX}{}", self.id)
    }

    pub fn persona(&self) -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: self.scope(),
            profile: LOCAL_PROFILE_KIND.into(),
        }
    }

    pub fn device(&self) -> String {
        format!("{}/{}", self.scope(), self.browser.as_str())
    }

    pub fn access_context(&self, at_ms: u64) -> AccessContext {
        AccessContext {
            persona: self.scope(),
            device: self.device(),
            at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "6f1f3c2e-8f4b-4f7a-9d3c-1b2a3c4d5e6f";

    #[test]
    fn parses_the_extension_record_into_a_local_scope() {
        let json =
            format!(r#"{{"schema":"{LOCAL_PROFILE_SCHEMA}","id":"{ID}","browser":"firefox"}}"#);
        let profile = LocalProfile::parse(&json).unwrap();
        assert_eq!(profile.scope(), format!("graphshell://local-scope/{ID}"));
        assert_eq!(
            profile.device(),
            format!("graphshell://local-scope/{ID}/firefox")
        );
        assert_eq!(profile.persona().profile, LOCAL_PROFILE_KIND);
        assert!(!profile.persona().persona.starts_with("personae://"));
    }

    #[test]
    fn refuses_other_schemas_nil_ids_and_extra_fields() {
        let nil = Uuid::nil();
        for json in [
            format!(r#"{{"schema":"other/v1","id":"{ID}","browser":"chromium"}}"#),
            format!(r#"{{"schema":"{LOCAL_PROFILE_SCHEMA}","id":"{nil}","browser":"chromium"}}"#),
            format!(
                r#"{{"schema":"{LOCAL_PROFILE_SCHEMA}","id":"{ID}","browser":"chromium","persona":"alice"}}"#
            ),
            format!(r#"{{"schema":"{LOCAL_PROFILE_SCHEMA}","id":"{ID}","browser":"safari"}}"#),
        ] {
            assert!(LocalProfile::parse(&json).is_err(), "{json}");
        }
    }
}
