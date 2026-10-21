// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The resident host's side: pending receipts become one signed turn each.
//!
//! Ingest can happen anywhere — a CLI, a test, a future importer — but
//! **authoring belongs to the resident host alone**, because it holds the
//! signing identity and the log. So ingest deposits an events file in an
//! inbox and this picks it up. The file is the hand-off: readable JSON, so
//! an owner can see what is about to be authored on their behalf, and it
//! names the source directory because staging the capture bytes into the
//! replicating store is the host's job, not ingest's.
//!
//! Everything here is ordinary library code with tests rather than logic
//! reachable only by running the host, which is the same reason
//! `device_sync`'s settings and pairing work sits beside it.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::personal_sync::PersonalGraphEvent;

/// Where the resident host looks for receipts waiting to be authored.
pub fn inbox_dir(data_root: &Path) -> PathBuf {
    data_root.join("receipts").join("inbox")
}

/// Where applied files are moved. Kept rather than deleted: it is the local
/// record of what this device authored, and it is what a person reads when
/// asking why a receipt did or did not arrive.
fn applied_dir(inbox: &Path) -> PathBuf {
    inbox.join("applied")
}

/// What a deposited receipt hands the host.
///
/// Carries the source directory as well as the events, because the bytes are
/// the host's job: ingest can compute a capture's hash anywhere, but only the
/// host can put it in the store that replicates. A receipt whose availability
/// facts named blobs the host never held would advertise captures no peer
/// could ever fetch.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InboxEntry {
    /// The receipt directory the artifacts can be read from.
    pub source: PathBuf,
    /// The events to author, in the order ingest produced them.
    pub events: Vec<PersonalGraphEvent>,
}

/// One receipt waiting to be authored.
#[derive(Clone, Debug)]
pub struct PendingReceipt {
    /// The file it came from.
    pub path: PathBuf,
    /// Where its artifact bytes live.
    pub source: PathBuf,
    /// The events to author, in the order ingest produced them.
    pub events: Vec<PersonalGraphEvent>,
}

/// One immutable hand-off owned by the resident intake loop.
///
/// The claim moves the file out of the producer's namespace before reading it.
/// Dropping a claim leaves it recoverable on the next scan. Only one resident
/// intake loop may claim an inbox; producers may deposit concurrently.
#[derive(Debug)]
pub struct ClaimedReceipt {
    receipt: PendingReceipt,
}

impl ClaimedReceipt {
    pub fn receipt(&self) -> &PendingReceipt {
        &self.receipt
    }

    /// Archive exactly this claim after successful authoring. The unique claim
    /// directory keeps separate versions of the same receipt. A failed rename
    /// leaves the claim available for retry; no copy/remove window is needed.
    pub fn mark_applied(self) -> io::Result<()> {
        let claim = self.receipt.path.parent().expect("claim directory");
        let inbox = claim.parent().and_then(Path::parent).expect("claim inbox");
        let applied = applied_dir(inbox);
        std::fs::create_dir_all(&applied)?;
        std::fs::rename(claim, applied.join(claim.file_name().expect("claim name")))
    }
}

/// The captures an entry's events refer to, as `(file name, blake3)`.
///
/// Read back out of the artifacts facet rather than passed alongside it, so
/// there is one statement of what a receipt's captures are and the host
/// stages exactly what the graph will claim.
pub fn captures_in(events: &[PersonalGraphEvent]) -> Vec<(String, [u8; 32])> {
    let mut found = Vec::new();
    for event in events {
        let PersonalGraphEvent::SetFacet { facet, value, .. } = event else {
            continue;
        };
        if facet != crate::receipts::FACET_ARTIFACTS {
            continue;
        }
        let Some(items) = value.as_array() else {
            continue;
        };
        for item in items {
            let (Some(name), Some(hex)) = (
                item.get("name").and_then(|v| v.as_str()),
                item.get("blake3").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            if let Some(hash) = parse_hex32(hex) {
                found.push((name.to_string(), hash));
            }
        }
    }
    found
}

fn parse_hex32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        out[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

/// Deposit a receipt's events for the resident host to author.
///
/// Named by node id, so re-ingesting the same receipt overwrites its pending
/// file rather than queueing a second copy of the same facts.
/// If the platform refuses atomic replacement, returns an error and leaves
/// the old deposit intact.
pub fn write_to_inbox(
    inbox: &Path,
    node: uuid::Uuid,
    source: &Path,
    events: &[PersonalGraphEvent],
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(inbox)?;
    let path = inbox.join(format!("{node}.json"));
    let entry = InboxEntry {
        source: source.to_path_buf(),
        events: events.to_vec(),
    };
    let json = serde_json::to_string_pretty(&entry).map_err(std::io::Error::other)?;
    // Readers must never observe a partially serialized hand-off. Unlike a
    // settings document, intake cannot recover a temporarily displaced target
    // from a .previous file, so installation uses a single atomic rename.
    pandect::atomic_file::write_bytes_atomic(&path, json.as_bytes())?;
    Ok(path)
}

/// Every receipt waiting in `inbox`, oldest name first.
///
/// A file that does not parse is **skipped, not fatal**: one malformed
/// hand-off must not wedge the intake loop for every later receipt. The
/// file stays put so it can be looked at. This is an inspection snapshot, not
/// an acknowledgement token. The resident uses [`claim_pending`] instead.
pub fn pending(inbox: &Path) -> std::io::Result<Vec<PendingReceipt>> {
    if !inbox.exists() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for path in entry_paths(inbox)? {
        if let Some(receipt) = read_entry(path)? {
            found.push(receipt);
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// Recover unfinished claims, then claim new receipts before reading them.
///
/// New deposits for a node with an unfinished claim wait until the next scan,
/// preventing a failed old turn from being retried after a newer version. A
/// crash after authoring but before archiving can replay a turn: this is
/// recoverable at-least-once delivery, not an exactly-once log protocol.
/// Malformed claims move to `rejected/` with a warning and retain their bytes
/// for inspection, allowing a corrected deposit for the same node to proceed.
pub fn claim_pending(inbox: &Path) -> io::Result<Vec<ClaimedReceipt>> {
    if !inbox.exists() {
        return Ok(Vec::new());
    }
    let processing = inbox.join("processing");
    std::fs::create_dir_all(&processing)?;
    let mut claims: Vec<_> = std::fs::read_dir(&processing)?
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry.file_type().map(|kind| (entry.path(), kind))),
            Err(error) => Some(Err(error)),
        })
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|(path, kind)| kind.is_dir().then_some(path))
        .collect();
    claims.sort();
    let mut found = Vec::new();
    let mut blocked = HashSet::new();
    for claim in claims {
        let paths = entry_paths(&claim)?;
        if paths.is_empty() {
            // Interrupted between reserving the directory and moving a file.
            // Never remove a nonempty directory or an unrecognized artifact.
            let _ = std::fs::remove_dir(&claim);
        }
        for path in paths {
            if let Some(claim) = read_claim(path, inbox)? {
                blocked.insert(
                    claim
                        .receipt
                        .path
                        .file_name()
                        .expect("entry file name")
                        .to_os_string(),
                );
                found.push(claim);
            }
        }
    }
    for path in entry_paths(inbox)? {
        let name = path.file_name().expect("entry file name");
        if blocked.contains(name) {
            continue;
        }
        let claim = loop {
            let name = uuid::Uuid::new_v4().to_string();
            // Also avoid an archived claim's name; a successful archive is
            // never replaced, even if a generated id happens to repeat.
            if applied_dir(inbox).join(&name).exists() {
                continue;
            }
            let claim = processing.join(name);
            match std::fs::create_dir(&claim) {
                Ok(()) => break claim,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let claimed = claim.join(name);
        if let Err(error) = std::fs::rename(&path, &claimed) {
            let _ = std::fs::remove_dir(&claim);
            if error.kind() == io::ErrorKind::NotFound {
                continue;
            }
            return Err(error);
        }
        if let Some(claim) = read_claim(claimed, inbox)? {
            found.push(claim);
        }
    }
    Ok(found)
}

fn entry_paths(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("json")
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn read_entry(path: PathBuf) -> io::Result<Option<PendingReceipt>> {
    let bytes = std::fs::read(&path)?;
    Ok(serde_json::from_slice::<InboxEntry>(&bytes)
        .ok()
        .map(|entry| PendingReceipt {
            path,
            source: entry.source,
            events: entry.events,
        }))
}

fn read_claim(path: PathBuf, inbox: &Path) -> io::Result<Option<ClaimedReceipt>> {
    if let Some(receipt) = read_entry(path.clone())? {
        return Ok(Some(ClaimedReceipt { receipt }));
    }
    // Keep malformed bytes for inspection without blocking a corrected deposit
    // for this node. Restoring them to the producer's path could overwrite a
    // newer deposit, so rejected claims retain their unique identity.
    let claim = path.parent().expect("claim directory");
    let rejected = inbox.join("rejected");
    std::fs::create_dir_all(&rejected)?;
    let rejected = rejected.join(claim.file_name().expect("claim name"));
    std::fs::rename(claim, &rejected)?;
    tracing::warn!(path = %rejected.display(), "malformed receipt retained for inspection");
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn events() -> Vec<PersonalGraphEvent> {
        vec![PersonalGraphEvent::AddNode {
            id: Uuid::from_u128(1),
            address: "receipt:woodshed/thinkpad/x/2026".into(),
            title: "woodshed · frame on thinkpad · ok".into(),
        }]
    }

    #[test]
    fn an_absent_inbox_is_empty_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        assert!(pending(&inbox).unwrap().is_empty());
    }

    #[test]
    fn a_deposited_receipt_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let node = Uuid::from_u128(1);
        write_to_inbox(&inbox, node, dir.path(), &events()).unwrap();

        let found = pending(&inbox).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].events.len(), 1);
        assert!(found[0].path.ends_with(format!("{node}.json")));
    }

    #[test]
    fn re_depositing_the_same_receipt_does_not_queue_it_twice() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let node = Uuid::from_u128(1);
        write_to_inbox(&inbox, node, dir.path(), &events()).unwrap();
        write_to_inbox(&inbox, node, dir.path(), &events()).unwrap();
        assert_eq!(pending(&inbox).unwrap().len(), 1);
    }

    #[test]
    fn applying_clears_the_pending_file_and_keeps_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let path = write_to_inbox(&inbox, Uuid::from_u128(1), dir.path(), &events()).unwrap();

        let claim = claim_pending(&inbox).unwrap().pop().unwrap();
        let archived = applied_dir(&inbox)
            .join(claim.receipt().path.parent().unwrap().file_name().unwrap())
            .join(path.file_name().unwrap());
        claim.mark_applied().unwrap();

        assert!(pending(&inbox).unwrap().is_empty(), "no longer pending");
        assert!(!path.exists());
        assert!(
            archived.exists(),
            "the local record of what this device authored survives",
        );
    }

    /// One bad hand-off must not wedge every later receipt.
    #[test]
    fn a_malformed_file_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        write_to_inbox(&inbox, Uuid::from_u128(1), dir.path(), &events()).unwrap();
        std::fs::write(inbox.join("garbage.json"), "{not json").unwrap();

        let found = pending(&inbox).unwrap();
        assert_eq!(found.len(), 1, "the good one still comes through");
        assert!(
            inbox.join("garbage.json").exists(),
            "and the bad one stays put to be looked at",
        );
    }

    /// The `applied/` directory lives inside the inbox, so the scan must not
    /// mistake it for a receipt.
    #[test]
    fn the_applied_directory_is_not_scanned_as_a_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        write_to_inbox(&inbox, Uuid::from_u128(1), dir.path(), &events()).unwrap();
        claim_pending(&inbox)
            .unwrap()
            .pop()
            .unwrap()
            .mark_applied()
            .unwrap();
        write_to_inbox(&inbox, Uuid::from_u128(2), dir.path(), &events()).unwrap();

        assert_eq!(
            pending(&inbox).unwrap().len(),
            1,
            "only the new one is pending",
        );
    }

    #[test]
    fn acknowledging_a_claim_preserves_a_new_deposit_for_the_same_node() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let node = Uuid::from_u128(1);
        let old_source = dir.path().join("old-source");
        let new_source = dir.path().join("new-source");
        let old_events = events();
        let new_events = vec![PersonalGraphEvent::AddNode {
            id: node,
            address: "receipt:woodshed/thinkpad/x/2026".into(),
            title: "a corrected receipt".into(),
        }];
        write_to_inbox(&inbox, node, &old_source, &old_events).unwrap();
        let claim = claim_pending(&inbox).unwrap().pop().unwrap();

        // The real resident awaits staging and authoring in this interval.
        // A producer may replace both the source and the events for this node.
        let new_path = write_to_inbox(&inbox, node, &new_source, &new_events).unwrap();
        let authored = claim.receipt();
        assert_eq!(authored.source, old_source);
        assert_eq!(
            serde_json::to_value(&authored.events).unwrap(),
            serde_json::to_value(&old_events).unwrap()
        );
        let old_bytes = std::fs::read(&authored.path).unwrap();
        let archive = applied_dir(&inbox)
            .join(authored.path.parent().unwrap().file_name().unwrap())
            .join(authored.path.file_name().unwrap());
        claim.mark_applied().unwrap();

        assert_eq!(std::fs::read(archive).unwrap(), old_bytes);
        assert!(new_path.exists(), "the new deposit was not acknowledged");
        let new_claim = claim_pending(&inbox).unwrap().pop().unwrap();
        assert_eq!(new_claim.receipt().source, new_source);
        assert_eq!(
            serde_json::to_value(&new_claim.receipt().events).unwrap(),
            serde_json::to_value(&new_events).unwrap()
        );
        new_claim.mark_applied().unwrap();
        assert_eq!(
            std::fs::read_dir(applied_dir(&inbox)).unwrap().count(),
            2,
            "both authored versions retain their own record"
        );
    }

    #[test]
    fn abandoned_claims_recover_before_a_newer_deposit_for_the_same_node() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let node = Uuid::from_u128(1);
        write_to_inbox(&inbox, node, &dir.path().join("original"), &events()).unwrap();
        let claim = claim_pending(&inbox).unwrap().pop().unwrap();
        let claimed_path = claim.receipt().path.clone();
        // A restart after authoring has exactly this same file state. Retrying
        // may append a duplicate operation; it must not lose the hand-off.
        drop(claim);
        let newer = write_to_inbox(&inbox, node, &dir.path().join("newer"), &events()).unwrap();

        let mut recovered = claim_pending(&inbox).unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].receipt().path, claimed_path);
        assert!(newer.exists(), "newer work waits for the unfinished claim");
        recovered.pop().unwrap().mark_applied().unwrap();
        assert_eq!(claim_pending(&inbox).unwrap().len(), 1);
    }

    #[test]
    fn a_failed_archive_preserves_the_claim_for_retry() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        write_to_inbox(&inbox, Uuid::from_u128(1), dir.path(), &events()).unwrap();
        let claim = claim_pending(&inbox).unwrap().pop().unwrap();
        let path = claim.receipt().path.clone();
        std::fs::write(applied_dir(&inbox), b"blocks the directory").unwrap();

        assert!(claim.mark_applied().is_err());
        assert!(path.exists());
        std::fs::remove_file(applied_dir(&inbox)).unwrap();
        claim_pending(&inbox)
            .unwrap()
            .pop()
            .unwrap()
            .mark_applied()
            .unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn interrupted_reservations_and_uninstalled_stages_do_not_hide_new_work() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        write_to_inbox(&inbox, Uuid::from_u128(1), dir.path(), &events()).unwrap();
        let empty_claim = inbox.join("processing").join("interrupted");
        std::fs::create_dir_all(&empty_claim).unwrap();
        std::fs::write(inbox.join("receipt.json.123.tmp"), b"{partial").unwrap();
        let claims = claim_pending(&inbox).unwrap();
        assert_eq!(claims.len(), 1);
        assert!(!empty_claim.exists());
        assert!(
            inbox.join("receipt.json.123.tmp").exists(),
            "unrecognized input is not deleted"
        );
    }

    #[test]
    fn malformed_claims_are_retained_without_blocking_a_corrected_deposit() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let node = Uuid::from_u128(1);
        let path = write_to_inbox(&inbox, node, dir.path(), &events()).unwrap();
        std::fs::write(&path, [255, 254]).unwrap();
        assert!(claim_pending(&inbox).unwrap().is_empty());
        let rejected = std::fs::read_dir(inbox.join("rejected"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::read(rejected.join(path.file_name().unwrap())).unwrap(),
            [255, 254]
        );

        write_to_inbox(&inbox, node, dir.path(), &events()).unwrap();
        let claim = claim_pending(&inbox).unwrap().pop().unwrap();
        claim.mark_applied().unwrap();
        assert!(
            rejected.exists(),
            "invalid input remains available to inspect"
        );
    }
}
