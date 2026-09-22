// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable, host-owned resident lifecycle state.
//!
//! Servitor supplies the portable binding and admission decision. Turnstone
//! owns this versioned sidecar because lifecycle transitions, skipped wakes,
//! and the mapping from a graph member to an instance are session facts.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use servitor::resident::{BodyRevision, Lifecycle, ResidentBinding, ResidentId};
use servitor::Subject;
use uuid::Uuid;

const VERSION: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct ResidentAdmissions {
    records: BTreeMap<Uuid, ResidentRecord>,
}

#[derive(Clone, Debug)]
pub struct ResidentRecord {
    pub binding: ResidentBinding,
    pub skipped: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskState {
    version: u32,
    residents: Vec<DiskResident>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskResident {
    member: Uuid,
    id: String,
    subject: String,
    revision: String,
    generation: u64,
    lifecycle: String,
    skipped: u64,
}

pub fn path(session_dir: &Path) -> PathBuf {
    session_dir.join("denizens").join("resident-admission.json")
}

pub fn body_revision(bytes: &[u8]) -> BodyRevision {
    BodyRevision(*blake3::hash(bytes).as_bytes())
}

pub fn resident_id(member: Uuid) -> ResidentId {
    ResidentId(*member.as_bytes())
}

pub fn load(session_dir: &Path) -> Result<ResidentAdmissions, String> {
    let target = path(session_dir);
    let text = std::fs::read_to_string(&target)
        .map_err(|err| {
            format!(
                "resident admission state unreadable at {}: {err}",
                target.display()
            )
        })?;
    let disk: DiskState = serde_json::from_str(&text)
        .map_err(|err| format!("resident admission state malformed at {}: {err}", target.display()))?;
    if disk.version != VERSION {
        return Err(format!("resident admission state has unsupported version {}", disk.version));
    }
    let mut records = BTreeMap::new();
    for item in disk.residents {
        let id = ResidentId(decode::<16>(&item.id, "resident id")?);
        if id != resident_id(item.member) {
            return Err("resident admission id does not match its graph member".into());
        }
        let binding = ResidentBinding {
            id,
            subject: Subject::new(decode::<32>(&item.subject, "subject")?),
            revision: BodyRevision(decode::<32>(&item.revision, "body revision")?),
            generation: item.generation,
            lifecycle: match item.lifecycle.as_str() {
                "active" => Lifecycle::Active,
                "paused" => Lifecycle::Paused,
                "revoked" => Lifecycle::Revoked,
                _ => return Err("resident admission state has unknown lifecycle".into()),
            },
        };
        if records
            .insert(
                item.member,
                ResidentRecord {
                    binding,
                    skipped: item.skipped,
                },
            )
            .is_some()
        {
            return Err("resident admission state names a member twice".into());
        }
    }
    Ok(ResidentAdmissions { records })
}

pub fn save(session_dir: &Path, state: &ResidentAdmissions) -> Result<(), String> {
    let target = path(session_dir);
    let disk = DiskState {
        version: VERSION,
        residents: state
            .records
            .iter()
            .map(|(member, record)| DiskResident {
                member: *member,
                id: hex(&record.binding.id.0),
                subject: record.binding.subject.to_hex(),
                revision: hex(&record.binding.revision.0),
                generation: record.binding.generation,
                lifecycle: match record.binding.lifecycle {
                    Lifecycle::Active => "active",
                    Lifecycle::Paused => "paused",
                    Lifecycle::Revoked => "revoked",
                }
                .into(),
                skipped: record.skipped,
            })
            .collect(),
    };
    let payload = serde_json::to_vec_pretty(&disk)
        .map_err(|err| format!("resident admission state cannot encode: {err}"))?;
    let parent = target
        .parent()
        .ok_or_else(|| "resident admission state has no parent".to_string())?;
    std::fs::create_dir_all(parent).map_err(|err| format!("cannot create resident admission directory: {err}"))?;
    let temporary = target.with_extension("json.new");
    let mut file = std::fs::File::create(&temporary)
        .map_err(|err| format!("cannot create resident admission state: {err}"))?;
    file.write_all(&payload)
        .map_err(|err| format!("cannot write resident admission state: {err}"))?;
    file.sync_all()
        .map_err(|err| format!("cannot sync resident admission state: {err}"))?;
    drop(file);
    std::fs::rename(&temporary, &target)
        .map_err(|err| format!("cannot commit resident admission state: {err}"))
}

impl ResidentAdmissions {
    pub fn get(&self, member: Uuid) -> Option<&ResidentRecord> {
        self.records.get(&member)
    }

    pub fn get_mut(&mut self, member: Uuid) -> Option<&mut ResidentRecord> {
        self.records.get_mut(&member)
    }

    pub fn insert(&mut self, member: Uuid, record: ResidentRecord) {
        self.records.insert(member, record);
    }

    pub fn active(member: Uuid, subject: Subject, revision: BodyRevision) -> ResidentRecord {
        ResidentRecord {
            binding: ResidentBinding {
                id: resident_id(member),
                subject,
                revision,
                generation: 0,
                lifecycle: Lifecycle::Active,
            },
            skipped: 0,
        }
    }

    pub fn revoked(member: Uuid, subject: Subject, revision: BodyRevision) -> ResidentRecord {
        ResidentRecord {
            binding: ResidentBinding {
                id: resident_id(member),
                subject,
                revision,
                generation: 0,
                lifecycle: Lifecycle::Revoked,
            },
            skipped: 0,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode<const N: usize>(value: &str, name: &str) -> Result<[u8; N], String> {
    if !value.is_ascii() || value.len() != N * 2 {
        return Err(format!("resident admission {name} has wrong length"));
    }
    let mut out = [0; N];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| format!("resident admission {name} is not hexadecimal"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_paused_and_revoked_records() {
        let dir = std::env::temp_dir().join(format!(
            "turnstone-resident-admission-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let member = Uuid::from_u128(7);
        let mut state = ResidentAdmissions::default();
        let mut paused = ResidentAdmissions::active(member, Subject::new([2; 32]), body_revision(b"body"));
        paused.binding.lifecycle = Lifecycle::Paused;
        paused.binding.generation = 4;
        paused.skipped = 3;
        state.insert(member, paused);
        state.insert(
            Uuid::from_u128(8),
            ResidentAdmissions::revoked(
                Uuid::from_u128(8),
                Subject::new([3; 32]),
                body_revision(b"other"),
            ),
        );
        save(&dir, &state).unwrap();
        save(&dir, &state).unwrap();
        let reopened = load(&dir).unwrap();
        assert_eq!(reopened.get(member).unwrap().binding.lifecycle, Lifecycle::Paused);
        assert_eq!(reopened.get(member).unwrap().binding.generation, 4);
        assert_eq!(reopened.get(member).unwrap().skipped, 3);
        assert_eq!(reopened.get(Uuid::from_u128(8)).unwrap().binding.lifecycle, Lifecycle::Revoked);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_state_is_not_treated_as_empty() {
        let dir = std::env::temp_dir().join(format!(
            "turnstone-resident-admission-corrupt-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("denizens")).unwrap();
        std::fs::write(path(&dir), "{ broken").unwrap();
        assert!(load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_ascii_hex_is_refused_without_panicking() {
        assert!(decode::<16>(&"é".repeat(16), "resident id").is_err());
    }

    fn serialized_record(member: Uuid) -> serde_json::Value {
        serde_json::json!({
            "member": member,
            "id": hex(&resident_id(member).0),
            "subject": hex(&[2; 32]),
            "revision": hex(&[3; 32]),
            "generation": 0,
            "lifecycle": "active",
            "skipped": 0,
        })
    }

    fn write_serialized(dir: &Path, value: serde_json::Value) {
        std::fs::create_dir_all(dir.join("denizens")).unwrap();
        std::fs::write(path(dir), serde_json::to_vec(&value).unwrap()).unwrap();
    }

    #[test]
    fn loader_rejects_unknown_version_and_fields() {
        let dir = std::env::temp_dir().join(format!(
            "turnstone-resident-admission-loader-{}",
            std::process::id()
        ));
        let member = Uuid::from_u128(9);
        let record = serialized_record(member);

        write_serialized(
            &dir,
            serde_json::json!({ "version": 2, "residents": [record] }),
        );
        assert!(load(&dir).is_err());

        let mut state = serde_json::json!({
            "version": VERSION,
            "residents": [serialized_record(member)],
        });
        state["unexpected"] = serde_json::json!(true);
        write_serialized(&dir, state);
        assert!(load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loader_rejects_mismatched_member_and_id() {
        let dir = std::env::temp_dir().join(format!(
            "turnstone-resident-admission-identity-{}",
            std::process::id()
        ));
        let member = Uuid::from_u128(10);
        let other = Uuid::from_u128(11);
        let mut record = serialized_record(member);
        record["member"] = serde_json::json!(other);
        write_serialized(
            &dir,
            serde_json::json!({ "version": VERSION, "residents": [record] }),
        );
        assert!(load(&dir).is_err());

        let mut record = serialized_record(member);
        record["id"] = serde_json::json!(hex(&resident_id(other).0));
        write_serialized(
            &dir,
            serde_json::json!({ "version": VERSION, "residents": [record] }),
        );
        assert!(load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loader_rejects_duplicate_members() {
        let dir = std::env::temp_dir().join(format!(
            "turnstone-resident-admission-duplicates-{}",
            std::process::id()
        ));
        let member = Uuid::from_u128(12);
        write_serialized(
            &dir,
            serde_json::json!({
                "version": VERSION,
                "residents": [serialized_record(member), serialized_record(member)]
            }),
        );
        assert!(load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
