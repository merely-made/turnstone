// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Capsule-scoped Gemini client identities.
//!
//! The durable record contains only an approval mapping: active Personae root
//! + capsule origin. Certificate and private-key bytes are reproduced from a
//! domain-separated child key when needed, so private material never lands in
//! Turnstone's files. The same persona and capsule reproduce the same X.509
//! fingerprint; another capsule receives a different key.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use identity::IdentityProvider;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use serde::{Deserialize, Serialize};

const FORMAT_VERSION: u16 = 1;
const FILE_NAME: &str = "gemini_identities.json";
const SALT_DOMAIN: &[u8] = b"personae/gemini-client-identity/v1/";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct Binding {
    persona: [u8; 32],
    origin: String,
}

/// Non-secret approvals for capsules allowed to receive an identity from a
/// particular Personae root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiIdentityBindings {
    #[serde(default = "format_version")]
    version: u16,
    #[serde(default)]
    bindings: BTreeSet<Binding>,
}

impl Default for GeminiIdentityBindings {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            bindings: BTreeSet::new(),
        }
    }
}

fn format_version() -> u16 {
    FORMAT_VERSION
}

impl GeminiIdentityBindings {
    pub fn load(data_root: &Path) -> Self {
        let path = bindings_path(data_root);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(bindings) if bindings.version == FORMAT_VERSION => bindings,
            Ok(_) => {
                tracing::warn!(path = ?path, "unsupported Gemini identity binding version");
                Self::default()
            }
            Err(error) => {
                tracing::warn!(%error, path = ?path, "failed to read Gemini identity bindings");
                Self::default()
            }
        }
    }

    pub fn save(&self, data_root: &Path) -> io::Result<()> {
        std::fs::create_dir_all(data_root)?;
        let target = bindings_path(data_root);
        let temporary = target.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        if let Err(error) = (|| -> io::Result<()> {
            std::fs::write(&temporary, bytes)?;
            if target.exists() {
                std::fs::remove_file(&target)?;
            }
            std::fs::rename(&temporary, &target)
        })() {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(())
    }

    pub fn bind(
        &mut self,
        provider: &dyn IdentityProvider,
        capsule_url: &str,
    ) -> Result<String, String> {
        let origin = capsule_origin(capsule_url)?;
        self.bindings.insert(Binding {
            persona: provider.master_public_key().to_bytes(),
            origin: origin.clone(),
        });
        Ok(origin)
    }

    pub fn identity_for(
        &self,
        provider: &dyn IdentityProvider,
        capsule_url: &str,
    ) -> Result<Option<fetch::GeminiClientIdentity>, String> {
        let origin = capsule_origin(capsule_url)?;
        let binding = Binding {
            persona: provider.master_public_key().to_bytes(),
            origin: origin.clone(),
        };
        if !self.bindings.contains(&binding) {
            return Ok(None);
        }
        mint_identity(provider, &origin).map(Some)
    }

    #[cfg(test)]
    fn contains(&self, provider: &dyn IdentityProvider, capsule_url: &str) -> bool {
        self.identity_for(provider, capsule_url)
            .is_ok_and(|identity| identity.is_some())
    }
}

pub fn capsule_origin(raw: &str) -> Result<String, String> {
    let url = url::Url::parse(raw).map_err(|error| error.to_string())?;
    if !matches!(url.scheme(), "gemini" | "titan") {
        return Err(
            "client identities are only valid for gemini:// or titan:// capsules".to_string(),
        );
    }
    let host = url
        .host_str()
        .ok_or_else(|| "Gemini capsule has no host".to_string())?
        .to_ascii_lowercase();
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    };
    Ok(match url.port().unwrap_or(1965) {
        1965 => format!("gemini://{host}"),
        port => format!("gemini://{host}:{port}"),
    })
}

fn mint_identity(
    provider: &dyn IdentityProvider,
    origin: &str,
) -> Result<fetch::GeminiClientIdentity, String> {
    let salt = [SALT_DOMAIN, origin.as_bytes()].concat();
    let derived = provider
        .derive_keypair(&salt)
        .map_err(|error| format!("derive Gemini identity: {error}"))?;
    let mut seed = derived.to_seed();
    let mut pkcs8 = ed25519_pkcs8_der(&seed);
    seed.fill(0);
    let key_pair = KeyPair::try_from(pkcs8.as_slice())
        .map_err(|error| format!("import Gemini identity key: {error}"))?;
    pkcs8.fill(0);

    let mut params = CertificateParams::new(Vec::<String>::new())
        .map_err(|error| format!("Gemini certificate parameters: {error}"))?;
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(
        DnType::CommonName,
        format!("Turnstone identity for {origin}"),
    );
    params.distinguished_name = distinguished_name;
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(2099, 12, 31);
    params.serial_number = Some(rcgen::SerialNumber::from(1u64));
    let certificate = params
        .self_signed(&key_pair)
        .map_err(|error| format!("mint Gemini client certificate: {error}"))?;
    fetch::GeminiClientIdentity::new(origin, certificate.der().to_vec(), key_pair.serialize_der())
}

fn ed25519_pkcs8_der(seed: &[u8; 32]) -> Vec<u8> {
    const PREFIX: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    [PREFIX.as_slice(), seed].concat()
}

fn bindings_path(data_root: &Path) -> PathBuf {
    data_root.join(FILE_NAME)
}

impl crate::app::App {
    /// Build the one page-fetch effect. A remembered capsule approval is
    /// projected into transient certificate bytes here, before the command
    /// crosses the fetch port.
    pub(crate) fn fetch_page_effect(
        &mut self,
        node: uuid::Uuid,
        url: String,
        owner_url: String,
    ) -> crate::action::Effect {
        let request = fetch::next_fetch_request_id();
        let supersedes = self.content.begin_fetch(node, request);
        let identity = match self
            .gemini_identities
            .identity_for(self.identity.as_ref(), &url)
        {
            Ok(identity) => identity,
            Err(error) => {
                tracing::warn!(%error, "failed to project Gemini client identity");
                None
            }
        };
        crate::action::Effect::FetchPage {
            request,
            supersedes,
            node,
            url,
            owner_url,
            identity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::InMemoryProvider;

    #[test]
    fn identity_is_stable_per_persona_and_unlinkable_across_capsules() {
        let provider = InMemoryProvider::from_seed([0x41; 32]);
        let a1 = mint_identity(&provider, "gemini://a.example").unwrap();
        let a2 = mint_identity(&provider, "gemini://a.example").unwrap();
        let b = mint_identity(&provider, "gemini://b.example").unwrap();
        assert_eq!(a1.certificate_der(), a2.certificate_der());
        assert_ne!(a1.certificate_der(), b.certificate_der());
        assert_eq!(a1.origin(), "gemini://a.example");
        assert_eq!(
            capsule_origin("gemini://[::1]:1966/private").unwrap(),
            "gemini://[::1]:1966"
        );
        assert_eq!(
            capsule_origin("titan://a.example/upload").unwrap(),
            "gemini://a.example"
        );
    }

    #[test]
    fn approval_is_persona_and_capsule_scoped_and_persists_without_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let a = InMemoryProvider::from_seed([0x51; 32]);
        let b = InMemoryProvider::from_seed([0x52; 32]);
        let mut bindings = GeminiIdentityBindings::default();
        bindings
            .bind(&a, "gemini://capsule.example/account")
            .unwrap();
        assert!(bindings.contains(&a, "gemini://capsule.example/private"));
        assert!(!bindings.contains(&a, "gemini://other.example/"));
        assert!(!bindings.contains(&b, "gemini://capsule.example/"));
        bindings.save(dir.path()).unwrap();

        let bytes = std::fs::read(bindings_path(dir.path())).unwrap();
        assert!(
            !bytes
                .windows(16)
                .any(|window| window == [0x51; 16].as_slice())
        );
        let reopened = GeminiIdentityBindings::load(dir.path());
        assert!(reopened.contains(&a, "gemini://capsule.example/again"));
    }
}
