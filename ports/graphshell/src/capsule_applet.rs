// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A bounded Graphshell host dialect for the existing app-core envelope.
//!
//! The supplying member owns collection selection. This host checks signed
//! authorship and exact addressed bytes, but does not establish Gemot history
//! from an unsigned snapshot. Local execution review is a separate authority.
//! No guest emission grants a capability, mutates a Moot or retains a file.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use eidetic::pack::{PackManifest, PackPartRole, PackVerdict, verify_pack};
use eidetic::{ManifestId, TrustEnvelope};
use serde::{Deserialize, Serialize};

pub const VIEW: &str = "power:capsule-view";
pub const NAVIGATE: &str = "power:navigate";
pub const MAX_COMPONENT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DISCLOSURE_BYTES: usize = 1024 * 1024;
pub const MAX_ENTRIES: usize = 1024;
pub const MAX_CAPSULE_BYTES: u64 = 16 * 1024 * 1024;
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPack {
    pub manifest: PackManifest,
    pub trust: TrustEnvelope,
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::pack::{PackPart, sign_pack};
    use personae::{IdentityProvider, InMemoryProvider};

    const COMPONENT: &[u8] = b"fixture component bytes";
    const BODY: &[u8] = b"# Garden\n";

    fn pack(owner: u8, name: &str, bytes: &[u8], role: PackPartRole, asks: Vec<String>) -> Vec<u8> {
        let author = InMemoryProvider::from_seed([owner; 32]);
        let manifest = PackManifest {
            name: name.into(),
            version: "1".into(),
            author: author
                .master_public_key()
                .to_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            requested_scopes: asks,
            parts: vec![PackPart {
                name: "part".into(),
                role,
                blob: ManifestId::of_blob(bytes),
                bytes: bytes.len() as u64,
            }],
        };
        let mut trust = TrustEnvelope::self_asserted();
        trust
            .signatures
            .push(sign_pack(&manifest, author.master_keypair()));
        serde_json::to_vec(&SignedPack { manifest, trust }).unwrap()
    }

    fn fixture() -> (Vec<u8>, CapsuleDisclosure) {
        let app = pack(
            1,
            "Library",
            COMPONENT,
            PackPartRole::WasmComponent,
            vec![VIEW.into(), NAVIGATE.into()],
        );
        let pack_bytes = pack(2, "Garden", BODY, PackPartRole::Asset, vec![]);
        let capsule: SignedPack = serde_json::from_slice(&pack_bytes).unwrap();
        let revision = hash(&pack_bytes);
        let entry = CapsuleEntry {
            title: "Garden".into(),
            author: capsule.manifest.author,
            revision: revision.clone(),
            url: format!("mere://capsule/{revision}"),
            bytes: BODY.len() as u64,
        };
        (
            app,
            CapsuleDisclosure {
                moot: "moot:fixture".into(),
                collection: "collection:fixture".into(),
                revision: "disclosed-frontier".into(),
                capsules: vec![DisclosedCapsule { entry, pack_bytes }],
            },
        )
    }

    fn reviewed() -> CapsuleMount {
        let (app, disclosure) = fixture();
        CapsuleMount::review(&app, &hash(&app), COMPONENT, disclosure).unwrap()
    }

    #[test]
    fn signatures_and_exact_bytes_do_not_grant_execution() {
        let mut mount = reviewed();
        assert!(matches!(
            mount.accept("capsule-library-view", "[]"),
            Err(Refusal::Denied(_))
        ));
        assert!(mount.approve(vec!["power:write-moot".into()]).is_err());
        mount.approve(vec![VIEW.into()]).unwrap();
        assert!(!mount.permits(NAVIGATE));
        assert!(mount.approve(vec![VIEW.into(), NAVIGATE.into()]).is_err());
    }

    #[test]
    fn altered_asks_component_and_contributor_are_rejected() {
        let (app, disclosure) = fixture();
        let mut changed: SignedPack = serde_json::from_slice(&app).unwrap();
        changed
            .manifest
            .requested_scopes
            .push("power:write-moot".into());
        let bytes = serde_json::to_vec(&changed).unwrap();
        assert!(
            CapsuleMount::review(&bytes, &hash(&bytes), COMPONENT, disclosure.clone()).is_err()
        );
        assert!(
            CapsuleMount::review(&app, &hash(&app), b"substitution", disclosure.clone()).is_err()
        );
        let mut wrong = disclosure;
        wrong.capsules[0].entry.author = "foreign author".into();
        assert!(CapsuleMount::review(&app, &hash(&app), COMPONENT, wrong).is_err());
    }

    #[test]
    fn revoked_queued_proposals_and_late_bodies_cannot_apply() {
        let mut mount = reviewed();
        mount.approve(vec![VIEW.into(), NAVIGATE.into()]).unwrap();
        let url = mount.entries()[0].url.clone();
        let accepted = mount
            .accept("open-address", &serde_json::json!({"url":url}).to_string())
            .unwrap();
        mount.revoke(NAVIGATE);
        assert!(matches!(mount.lower(accepted), Err(Refusal::Denied(_))));
        assert!(mount.open(&url, BODY).is_err());
        let projection = mount
            .accept(
                "capsule-library-view",
                &serde_json::to_string(&mount.entries()).unwrap(),
            )
            .unwrap();
        mount.revoke(VIEW);
        assert!(mount.lower(projection).is_err());
        assert!(mount.projection.is_empty());
    }

    #[test]
    fn guest_authority_and_extra_payload_fields_are_refused() {
        let mut mount = reviewed();
        mount.approve(vec![VIEW.into(), NAVIGATE.into()]).unwrap();
        assert!(matches!(
            mount.accept("confirm-install-participant", ""),
            Err(Refusal::Denied(_))
        ));
        assert!(matches!(
            mount.accept("open-address", "{}"),
            Err(Refusal::Malformed(_))
        ));
        assert!(matches!(
            mount.accept("open-address", r#"{"url":"https://outside.invalid/"}"#),
            Err(Refusal::Denied(_))
        ));
        assert!(matches!(
            mount.accept("new-host-action", ""),
            Err(Refusal::Unknown(_))
        ));
        let mut entries = mount.entries();
        entries.push(entries[0].clone());
        assert!(
            mount
                .accept(
                    "capsule-library-view",
                    &serde_json::to_string(&entries).unwrap()
                )
                .is_err()
        );
        let mut value = serde_json::to_value(mount.entries()).unwrap();
        value[0]["onClick"] = serde_json::json!("arbitrary guest script");
        assert!(matches!(
            mount.accept("capsule-library-view", &value.to_string()),
            Err(Refusal::Malformed(_))
        ));
    }

    #[test]
    fn body_integrity_is_checked_and_stopped_instances_need_review() {
        let mut mount = reviewed();
        mount.approve(vec![VIEW.into(), NAVIGATE.into()]).unwrap();
        let url = mount.entries()[0].url.clone();
        assert!(mount.open(&url, b"# Forged\n").is_err());
        assert_eq!(mount.open(&url, BODY).unwrap().body.as_bytes(), BODY);
        mount.end_execution();
        assert!(mount.open(&url, BODY).is_err());
        assert!(mount.granted().is_empty());
        mount.approve(vec![VIEW.into()]).unwrap();
        assert!(mount.accept("capsule-library-view", "[]").is_ok());
    }

    #[test]
    fn replacing_with_the_same_pack_does_not_reuse_a_queued_grant() {
        let mut previous = reviewed();
        previous.approve(vec![VIEW.into()]).unwrap();
        let queued = previous.accept("capsule-library-view", "[]").unwrap();
        let mut replacement = reviewed();
        replacement.approve(vec![VIEW.into()]).unwrap();
        assert!(matches!(replacement.lower(queued), Err(Refusal::Denied(_))));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapsuleEntry {
    pub title: String,
    pub author: String,
    pub revision: String,
    pub url: String,
    pub bytes: u64,
}

/// Exact pack bytes accompany the metadata; capsule bodies are fetched on open.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosedCapsule {
    pub entry: CapsuleEntry,
    pub pack_bytes: Vec<u8>,
}

/// A host disclosure, not a signed constitutional or community record.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapsuleDisclosure {
    pub moot: String,
    pub collection: String,
    pub revision: String,
    pub capsules: Vec<DisclosedCapsule>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "tag", content = "val", rename_all = "lowercase")]
pub enum Refusal {
    Denied(String),
    Unknown(String),
    Malformed(String),
}

/// Private variants: accepted proposals can only originate at this gate.
pub struct Proposal {
    action: Action,
    pack_hash: String,
    scope: [String; 3],
    instance: u64,
    execution: u64,
}
enum Action {
    View(Vec<CapsuleEntry>),
    Open(String),
}

#[derive(Clone, Debug, Serialize)]
pub struct OpenedCapsule {
    pub entry: CapsuleEntry,
    pub body: String,
    pub content_hash: String,
}

pub struct CapsuleMount {
    pub pack: SignedPack,
    pub pack_hash: String,
    pub component_hash: String,
    pub disclosure: CapsuleDisclosure,
    grants: BTreeSet<String>,
    approved: bool,
    instance: u64,
    execution: u64,
    pub projection: Vec<CapsuleEntry>,
}

fn hash(bytes: &[u8]) -> String {
    ManifestId::of_blob(bytes).0.to_hex()
}

fn signed(bytes: &[u8], expected: &str) -> Result<SignedPack, String> {
    if bytes.len() > MAX_DISCLOSURE_BYTES || hash(bytes) != expected {
        return Err("Pack address or size mismatch".into());
    }
    let pack: SignedPack = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if verify_pack(&pack.manifest, &pack.trust) != PackVerdict::Trusted {
        return Err("Pack signature failed".into());
    }
    Ok(pack)
}

impl CapsuleMount {
    /// Preparing a review verifies bytes but authorizes no execution.
    pub fn review(
        pack_bytes: &[u8],
        pack_hash: &str,
        component: &[u8],
        disclosure: CapsuleDisclosure,
    ) -> Result<Self, String> {
        let pack = signed(pack_bytes, pack_hash)?;
        if component.len() > MAX_COMPONENT_BYTES
            || pack.manifest.parts.len() != 1
            || pack.manifest.parts[0].role != PackPartRole::WasmComponent
            || pack.manifest.parts[0].bytes != component.len() as u64
            || pack.manifest.parts[0].blob != ManifestId::of_blob(component)
        {
            return Err("Component inventory, address or size mismatch".into());
        }
        if disclosure.moot.is_empty()
            || disclosure.collection.is_empty()
            || disclosure.revision.is_empty()
            || disclosure.capsules.len() > MAX_ENTRIES
            || serde_json::to_vec(&disclosure)
                .map_err(|e| e.to_string())?
                .len()
                > MAX_DISCLOSURE_BYTES
        {
            return Err("Disclosure identity or size is invalid".into());
        }
        let mut seen = BTreeSet::new();
        for capsule in &disclosure.capsules {
            let entry = &capsule.entry;
            let selected = signed(&capsule.pack_bytes, &entry.revision)?;
            let manifest = selected.manifest;
            if !seen.insert(&entry.revision)
                || entry.url != format!("mere://capsule/{}", entry.revision)
                || manifest.author != entry.author
                || manifest.name != entry.title
                || manifest.parts.len() != 1
                || manifest.parts[0].role != PackPartRole::Asset
                || manifest.parts[0].bytes != entry.bytes
                || entry.bytes > MAX_CAPSULE_BYTES
            {
                return Err("Disclosed metadata does not match its signed capsule".into());
            }
        }
        Ok(Self {
            pack,
            pack_hash: pack_hash.to_owned(),
            component_hash: hash(component),
            disclosure,
            grants: BTreeSet::new(),
            approved: false,
            instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            execution: 0,
            projection: Vec::new(),
        })
    }

    /// Called by the local person's review, never by the guest action path.
    pub fn approve(&mut self, grants: Vec<String>) -> Result<(), String> {
        if self.approved {
            return Err("A new grant requires a new review".into());
        }
        if grants.iter().any(|g| {
            !matches!(g.as_str(), VIEW | NAVIGATE)
                || !self.pack.manifest.requested_scopes.contains(g)
        }) {
            return Err("Grant exceeds this host's dialect or the reviewed ask".into());
        }
        self.grants = grants.into_iter().collect();
        self.approved = true;
        self.execution += 1;
        Ok(())
    }

    pub fn granted(&self) -> Vec<String> {
        self.grants.iter().cloned().collect()
    }

    pub fn end_execution(&mut self) {
        self.grants.clear();
        self.approved = false;
        self.execution += 1;
        self.projection.clear();
    }

    pub fn permits(&self, power: &str) -> bool {
        self.approved && self.grants.contains(power)
    }

    pub fn revoke(&mut self, power: &str) {
        self.grants.remove(power);
        if power == VIEW {
            self.projection.clear();
        }
    }

    pub fn entries(&self) -> Vec<CapsuleEntry> {
        self.disclosure
            .capsules
            .iter()
            .map(|c| c.entry.clone())
            .collect()
    }

    fn require(&self, power: &str) -> Result<(), Refusal> {
        if self.permits(power) {
            Ok(())
        } else {
            Err(Refusal::Denied(power.into()))
        }
    }

    fn proposal(&self, action: Action) -> Proposal {
        Proposal {
            action,
            pack_hash: self.pack_hash.clone(),
            scope: [
                self.disclosure.moot.clone(),
                self.disclosure.collection.clone(),
                self.disclosure.revision.clone(),
            ],
            instance: self.instance,
            execution: self.execution,
        }
    }

    pub fn accept(&self, name: &str, payload: &str) -> Result<Proposal, Refusal> {
        if name == "confirm-install-participant" {
            return Err(Refusal::Denied("host-only".into()));
        }
        if payload.len() > MAX_DISCLOSURE_BYTES {
            return Err(Refusal::Malformed("Action exceeds size limit".into()));
        }
        match name {
            "capsule-library-view" => {
                self.require(VIEW)?;
                let entries: Vec<CapsuleEntry> =
                    serde_json::from_str(payload).map_err(|e| Refusal::Malformed(e.to_string()))?;
                let mut seen = BTreeSet::new();
                if entries.iter().any(|e| {
                    !seen.insert(&e.revision)
                        || !self.disclosure.capsules.iter().any(|c| c.entry == *e)
                }) {
                    return Err(Refusal::Denied("Outside disclosed catalogue".into()));
                }
                Ok(self.proposal(Action::View(entries)))
            },
            "open-address" => {
                self.require(NAVIGATE)?;
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Open {
                    url: String,
                }
                let open: Open =
                    serde_json::from_str(payload).map_err(|e| Refusal::Malformed(e.to_string()))?;
                if !self
                    .disclosure
                    .capsules
                    .iter()
                    .any(|c| c.entry.url == open.url)
                {
                    return Err(Refusal::Denied("Outside disclosed catalogue".into()));
                }
                Ok(self.proposal(Action::Open(open.url)))
            },
            _ => Err(Refusal::Unknown(name.into())),
        }
    }

    /// Recheck after the guest turn. A queued proposal cannot outlive revocation.
    pub fn lower(&mut self, proposal: Proposal) -> Result<Option<String>, Refusal> {
        if proposal.pack_hash != self.pack_hash
            || proposal.scope
                != [
                    self.disclosure.moot.clone(),
                    self.disclosure.collection.clone(),
                    self.disclosure.revision.clone(),
                ]
            || proposal.instance != self.instance
            || proposal.execution != self.execution
        {
            return Err(Refusal::Denied(
                "Stale proposal or different applet scope".into(),
            ));
        }
        match proposal.action {
            Action::View(entries) => {
                self.require(VIEW)?;
                // The supplying host can replace the mount while a turn runs.
                self.accept(
                    "capsule-library-view",
                    &serde_json::to_string(&entries).unwrap(),
                )?;
                self.projection = entries;
                Ok(None)
            },
            Action::Open(url) => {
                self.require(NAVIGATE)?;
                self.accept("open-address", &serde_json::json!({"url": url}).to_string())?;
                Ok(Some(url))
            },
        }
    }

    /// Body arrival is checked against the current scope and grant again.
    pub fn open(&self, url: &str, body: &[u8]) -> Result<OpenedCapsule, String> {
        self.require(NAVIGATE).map_err(|e| format!("{e:?}"))?;
        let capsule = self
            .disclosure
            .capsules
            .iter()
            .find(|c| c.entry.url == url)
            .ok_or("Address outside disclosed catalogue")?;
        let pack = signed(&capsule.pack_bytes, &capsule.entry.revision)?;
        let part = &pack.manifest.parts[0];
        if part.bytes != body.len() as u64 || part.blob != ManifestId::of_blob(body) {
            return Err("Capsule body hash or size mismatch".into());
        }
        Ok(OpenedCapsule {
            entry: capsule.entry.clone(),
            body: String::from_utf8(body.to_vec()).map_err(|e| e.to_string())?,
            content_hash: hash(body),
        })
    }
}
