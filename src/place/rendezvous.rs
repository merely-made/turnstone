//! Bounded contact hints for an already admitted place, never admission evidence.

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::PlaceBindingV1;
use super::invite::{P2PANDA_ENDPOINT_TICKET, PlaceInviteV1, RendezvousV1};

pub(crate) const RENDEZVOUS_FILE: &str = "place-rendezvous.json";
const MAX_BYTES: u64 = 512 * 1024;
const MAX_TICKETS: usize = 16;
const MAX_HINT_BYTES: usize = 4096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceRendezvousV1 {
    version: u16,
    moot: super::PlaceId,
    root: super::SharedContainerId,
    chat: super::ChatSpaceId,
    // Reusing contact hints is bounded by the original offer. Expiry demands
    // a renewed offer, not silent replay of its old welcome or governance.
    not_after_ms: u64,
    rendezvous: Vec<RendezvousV1>,
}

impl PlaceRendezvousV1 {
    fn tickets(&self, binding: &PlaceBindingV1, now_ms: u64) -> Result<Vec<String>, String> {
        if self.version != 1
            || self.moot != binding.moot
            || self.root != binding.root
            || self.chat != binding.chat
        {
            return Err("saved rendezvous belongs to another place or version".into());
        }
        if now_ms > self.not_after_ms {
            return Err("saved rendezvous offer expired; obtain a renewed invitation".into());
        }
        if self.rendezvous.len() > MAX_TICKETS {
            return Err("saved rendezvous exceeds the ticket limit".into());
        }
        let mut tickets = Vec::new();
        for entry in &self.rendezvous {
            if entry.carrier != P2PANDA_ENDPOINT_TICKET
                || entry.hint.is_empty()
                || entry.hint.len() > MAX_HINT_BYTES
            {
                return Err("saved rendezvous has an unsupported or invalid ticket".into());
            }
            if !tickets.contains(&entry.hint) {
                tickets.push(entry.hint.clone());
            }
        }
        // An empty list is not a failure. A founder saves a ticketless
        // descriptor on purpose, and reopening it is a listen-only bind.
        Ok(tickets)
    }
}

/// Called by the worker only after admission. No welcome, key or governance
/// artifact is represented here. A ticketless offer remains valid offline.
pub(crate) fn save_admitted_rendezvous(
    directory: &Path,
    invite: &PlaceInviteV1,
) -> Result<(), String> {
    invite.validate().map_err(|error| error.to_string())?;
    let saved = PlaceRendezvousV1 {
        version: 1,
        moot: invite.binding.moot,
        root: invite.binding.root,
        chat: invite.binding.chat,
        not_after_ms: invite.not_after_ms,
        rendezvous: invite.dialable().cloned().collect(),
    };
    let bytes = serde_json::to_vec(&saved).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("saved rendezvous exceeds the byte limit".into());
    }
    replace_descriptor(directory, &bytes)
}

/// Called by the worker after founding. The founder has dialed nobody, so
/// the descriptor it saves is deliberately empty: a place it holds itself
/// needs an endpoint, not a contact hint, and reopening it listens.
pub(crate) fn save_founder_rendezvous(
    directory: &Path,
    binding: &PlaceBindingV1,
) -> Result<(), String> {
    let saved = PlaceRendezvousV1 {
        version: 1,
        moot: binding.moot,
        root: binding.root,
        chat: binding.chat,
        // Nothing to expire: an empty offer replays no welcome and reveals
        // no address, so bounding it would only break the founder's reopen.
        not_after_ms: u64::MAX,
        rendezvous: Vec::new(),
    };
    let bytes = serde_json::to_vec(&saved).map_err(|error| error.to_string())?;
    replace_descriptor(directory, &bytes)
}

fn replace_descriptor(directory: &Path, bytes: &[u8]) -> Result<(), String> {
    let target = directory.join(RENDEZVOUS_FILE);
    let temporary = directory.join("place-rendezvous.json.tmp");
    let result =
        std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, &target));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    // Rename replaces a file without deleting the usable old descriptor first.
    // This is replacement failure safety, not a power-cut durability receipt.
    result.map_err(|error| format!("save rendezvous: {error}"))
}

pub(crate) fn load_rendezvous(
    directory: &Path,
    binding: &PlaceBindingV1,
    now_ms: u64,
) -> Result<Vec<String>, String> {
    // A detached place cannot be resurrected by leftover contact metadata.
    let admitted = crate::session::load_place_binding(directory)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "place is no longer locally joined".to_string())?;
    if admitted.moot != binding.moot
        || admitted.root != binding.root
        || admitted.chat != binding.chat
    {
        return Err("saved place binding changed".into());
    }
    let file = std::fs::File::open(directory.join(RENDEZVOUS_FILE))
        .map_err(|error| format!("open saved rendezvous: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read rendezvous: {error}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("saved rendezvous exceeds the byte limit".into());
    }
    let saved: PlaceRendezvousV1 =
        serde_json::from_slice(&bytes).map_err(|error| format!("decode rendezvous: {error}"))?;
    saved.tickets(binding, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn rendezvous_failed_replacement_keeps_previous_descriptor() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = std::env::temp_dir().join(format!(
            "turnstone-rendezvous-replace-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        replace_descriptor(&directory, b"previous").unwrap();
        let target = directory.join(RENDEZVOUS_FILE);
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        assert!(replace_descriptor(&directory, b"replacement").is_err());
        drop(locked);
        assert_eq!(std::fs::read(&target).unwrap(), b"previous");
        assert!(!directory.join("place-rendezvous.json.tmp").exists());
        replace_descriptor(&directory, b"replacement").unwrap();
        assert_eq!(std::fs::read(target).unwrap(), b"replacement");
    }

    fn binding() -> PlaceBindingV1 {
        PlaceBindingV1::new(
            super::super::PlaceId([1; 32]),
            super::super::SharedContainerId([2; 32]),
            super::super::ChatSpaceId([3; 32]),
            "place",
        )
        .unwrap()
    }

    fn saved(binding: &PlaceBindingV1) -> PlaceRendezvousV1 {
        PlaceRendezvousV1 {
            version: 1,
            moot: binding.moot,
            root: binding.root,
            chat: binding.chat,
            not_after_ms: 1000,
            rendezvous: vec![RendezvousV1 {
                carrier: P2PANDA_ENDPOINT_TICKET.into(),
                hint: "ticket".into(),
            }],
        }
    }

    #[test]
    fn rendezvous_refuses_expiry_wrong_scope_unknown_carriers_and_oversize_hints() {
        let binding = binding();
        let mut saved = saved(&binding);
        assert_eq!(saved.tickets(&binding, 999).unwrap(), ["ticket"]);
        assert!(saved.tickets(&binding, 1000).is_ok());
        assert!(saved.tickets(&binding, 1001).is_err());
        saved.root = super::super::SharedContainerId([9; 32]);
        assert!(saved.tickets(&binding, 0).is_err());
        saved.root = binding.root;
        saved.rendezvous[0].carrier = "foreign".into();
        assert!(saved.tickets(&binding, 0).is_err());
        saved.rendezvous[0].carrier = P2PANDA_ENDPOINT_TICKET.into();
        saved.rendezvous[0].hint = "x".repeat(MAX_HINT_BYTES + 1);
        assert!(saved.tickets(&binding, 0).is_err());
    }

    #[test]
    fn rendezvous_requires_binding_and_preserves_only_contact_metadata() {
        let directory =
            std::env::temp_dir().join(format!("turnstone-rendezvous-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let binding = binding();
        let saved = saved(&binding);
        let bytes = serde_json::to_vec(&saved).unwrap();
        let fields: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(fields.as_object().unwrap().len(), 6);
        for field in ["governance", "key_welcome", "key_direct", "inviter_prekey"] {
            assert!(fields.get(field).is_none());
        }
        std::fs::write(directory.join(RENDEZVOUS_FILE), bytes).unwrap();
        assert!(load_rendezvous(&directory, &binding, 0).is_err());
        crate::session::save_place_binding(&directory, &binding).unwrap();
        assert_eq!(
            load_rendezvous(&directory, &binding, 0).unwrap(),
            ["ticket"]
        );
        crate::session::remove_place_binding(&directory).unwrap();
        assert!(load_rendezvous(&directory, &binding, 0).is_err());
        assert!(!directory.join(RENDEZVOUS_FILE).exists());
    }
}
