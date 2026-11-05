// SPDX-License-Identifier: MPL-2.0
//! Explicit public metadata publication; cached hints never grant navigation.
use super::*;
use crate::place::catalogue::{CatalogueSiteDraft, DiscoveredSite};
use gemot::moot::workflow::discovery::*;
use gemot::moot::{CollectionChange, ContributionRef};
const MAX_ENTRIES: usize = 128;
const MAX_LIFETIME_MS: u64 = 60 * 60 * 1000;
fn load(open: &OpenPlace) -> Result<CatalogueDirectory, String> {
    let path = open.directory.join("catalogue-hints.cbor");
    if !path.exists() && path.with_extension("cbor.previous").exists() {
        return Err("catalogue journal needs recovery from its previous copy".into());
    }
    if path
        .metadata()
        .map(|m| m.len() > 3 * 1024 * 1024)
        .unwrap_or(false)
    {
        return Err("catalogue journal exceeds byte ceiling".into());
    }
    use std::io::Read;
    let read = std::fs::File::open(path).and_then(|file| {
        let mut bytes = Vec::new();
        file.take(3 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 3 * 1024 * 1024 {
            return Err(std::io::Error::other("catalogue journal byte ceiling"));
        }
        Ok(bytes)
    });
    match read {
        Ok(bytes) => CatalogueDirectory::decode(&bytes, MAX_ENTRIES),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CatalogueDirectory::default()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::InMemoryProvider;
    #[test]
    fn catalogue_owner_roundtrip_reopen_withdraw_and_expiry_use_real_place_records() {
        let dir = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([67; 32]);
        let mut settings = PlaceWorkerSettings::default();
        settings.authority_clock = AuthorityClock::Fixed(100);
        let binding = found_place(dir.path(), &identity, "public garden", &settings).unwrap();
        let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
        let draft = CatalogueSiteDraft {
            address: "scroll://garden.test/".into(),
            title: "Garden".into(),
            abstract_text: "Painting and music".into(),
            scroll_subject: Some(7),
        };
        advertise(&open, &identity, &draft, 100).unwrap();
        let rows = refresh(&open, 101).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            open_address(&open, rows[0].digest, 102).unwrap(),
            draft.address
        );
        assert!(refresh(&open, rows[0].expires_at_ms).unwrap().is_empty());
        drop(open);
        let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
        assert_eq!(refresh(&open, 103).unwrap(), rows);
        withdraw(&open, &identity, rows[0].collection, 104).unwrap();
        assert!(refresh(&open, 105).unwrap().is_empty());
        assert!(open_address(&open, rows[0].digest, 105).is_err());
    }
    #[test]
    fn catalogue_refuses_standing_policy_without_amending_it() {
        let dir = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([69; 32]);
        let settings = PlaceWorkerSettings {
            founding_admission: mien::Policy::OpenWithFloor(Default::default()),
            ..Default::default()
        };
        let binding = found_place(dir.path(), &identity, "garden", &settings).unwrap();
        let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
        let before = pollster::block_on(open.moot.snapshot()).unwrap().governance;
        let draft = CatalogueSiteDraft {
            address: "https://garden.test/".into(),
            title: "Garden".into(),
            abstract_text: String::new(),
            scroll_subject: Some(7),
        };
        assert!(
            advertise(&open, &identity, &draft, settings.authority_clock.now_ms())
                .unwrap_err()
                .contains("BelowThreshold")
        );
        let after = pollster::block_on(open.moot.snapshot()).unwrap().governance;
        assert_eq!(before.revision, after.revision);
        assert_eq!(before.rules, after.rules);
    }
    #[test]
    fn malformed_catalogue_journal_refuses_before_changing_constitution() {
        let dir = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([68; 32]);
        let settings = PlaceWorkerSettings::default();
        let binding = found_place(dir.path(), &identity, "garden", &settings).unwrap();
        let (open, _) = open_cached_place(dir.path(), &binding, &identity, &settings).unwrap();
        let before = pollster::block_on(open.moot.snapshot())
            .unwrap()
            .governance
            .revision;
        std::fs::write(dir.path().join("catalogue-hints.cbor"), b"broken").unwrap();
        let draft = CatalogueSiteDraft {
            address: "scroll://garden.test/".into(),
            title: "Garden".into(),
            abstract_text: String::new(),
            scroll_subject: Some(4),
        };
        assert!(advertise(&open, &identity, &draft, 100).is_err());
        assert!(
            place_snapshot(&open, binding.moot.0, &settings)
                .unwrap()
                .catalogue_error
                .is_some()
        );
        assert_eq!(
            pollster::block_on(open.moot.snapshot())
                .unwrap()
                .governance
                .revision,
            before
        );
    }
}
fn save(open: &OpenPlace, directory: &CatalogueDirectory) -> Result<(), String> {
    muniment::atomic_file::write_bytes_with_backup(
        &open.directory.join("catalogue-hints.cbor"),
        &directory.encode()?,
    )
    .map_err(|e| e.to_string())
}
pub(super) fn refresh(open: &OpenPlace, now: u64) -> Result<Vec<DiscoveredSite>, String> {
    let mut directory = load(open)?;
    let mut changed = false;
    if let Some(lanes) = &open.lanes {
        for bytes in lanes.catalogue.drain() {
            match pollster::block_on(directory.receive(
                &open.moot,
                open.subject,
                &bytes,
                now,
                MAX_LIFETIME_MS,
                MAX_ENTRIES,
            )) {
                Ok(update) => changed |= update,
                Err(e) => tracing::debug!(%e,"catalogue hint refused"),
            }
        }
        // Repeat only previously authorized, still-unexpired publication intent.
        for hint in directory.outgoing(open.subject, now) {
            if pollster::block_on(open.moot.validate_catalogue_advertisement(
                open.subject,
                &hint,
                now,
                MAX_LIFETIME_MS,
            ))
            .is_ok()
            {
                lanes.publish_catalogue(hint.encode()?)?;
            }
        }
    }
    if changed {
        save(open, &directory)?;
    }
    let rows: Vec<DiscoveredSite> =
        pollster::block_on(directory.visible(&open.moot, open.subject, now, MAX_LIFETIME_MS))
            .into_iter()
            .map(|hint| {
                let digest = CatalogueDirectory::digest(&hint).expect("checked hint");
                let ad = hint.advertisement;
                let d = ad.description.expect("visible description");
                DiscoveredSite {
                    digest,
                    provider: ad.provider,
                    collection: ad.collection.collection.collection_id.0,
                    sequence: ad.sequence,
                    address: d.address,
                    title: d.title,
                    abstract_text: d.abstract_text,
                    scroll_subject: d.scroll_subject,
                    expires_at_ms: ad.expires_at_ms,
                }
            })
            .collect();
    if let Some(lanes) = &open.lanes {
        let active = !rows.is_empty() || !directory.outgoing(open.subject, now).is_empty();
        lanes
            .catalogue_clock
            .store(active, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(rows)
}
pub(super) fn open_address(open: &OpenPlace, digest: [u8; 32], now: u64) -> Result<String, String> {
    refresh(open, now)?
        .into_iter()
        .find(|h| h.digest == digest)
        .map(|h| h.address)
        .ok_or_else(|| "site advertisement expired, changed or is no longer permitted".into())
}
fn lifetime() -> Result<u64, String> {
    let n = std::env::var("TURNSTONE_CATALOGUE_LIFETIME_MS")
        .map(|s| {
            s.parse::<u64>()
                .map_err(|_| "invalid catalogue lifetime".to_string())
        })
        .unwrap_or(Ok(10 * 60 * 1000))?;
    if n == 0 || n > MAX_LIFETIME_MS {
        return Err("catalogue lifetime must be between 1 ms and one hour".into());
    }
    Ok(n)
}
fn publish_history(open: &OpenPlace) -> Result<(), String> {
    if let Some(lanes) = &open.lanes {
        lanes.publish_catalogue_history(&open.moot)?;
    }
    Ok(())
}
pub(super) fn advertise(
    open: &OpenPlace,
    identity: &dyn IdentityProvider,
    draft: &CatalogueSiteDraft,
    now: u64,
) -> Result<(), String> {
    let description = CatalogueDescription {
        address: draft.address.clone(),
        title: draft.title.clone(),
        abstract_text: draft.abstract_text.clone(),
        scroll_subject: draft.scroll_subject,
        language: None,
    };
    if !description.validate() {
        return Err("use a public web or small-web address and a valid subject/abstract".into());
    }
    let expires = now
        .checked_add(lifetime()?)
        .ok_or("catalogue clock overflow")?;
    let moot = open.binding.moot.0;
    let provider = identity.master_public_key().to_bytes();
    let mut seed = b"turnstone/public-site-collection/v1/".to_vec();
    seed.extend(moot);
    seed.extend(provider);
    seed.extend(draft.address.as_bytes());
    let collection = CollectionRef {
        moot_id: moot,
        collection_id: CollectionId(*blake3::hash(&seed).as_bytes()),
    };
    let mut directory = load(open)?;
    directory.ensure_capacity(provider, collection.collection_id.0, MAX_ENTRIES)?;
    let sequence = directory.next_sequence(provider, collection.collection_id.0)?;
    let snapshot = pollster::block_on(open.moot.snapshot()).map_err(|e| e.to_string())?;
    // Check signed admission before granting publication scopes or writing records.
    let group = pollster::block_on(open.moot.membership()).map_err(|e| e.to_string())?;
    let inputs = gemot::moot::MootAuthorizationProvider::inputs(
        &group,
        &gemot::moot::MootAuthorizationRequest {
            subject: provider,
            capability_path: cap_path(&gemot::moot::records::fauna_cap()),
            at_ms: now,
        },
    );
    if let mien::GateDecision::Deny(reason) = mien::authorize(
        &snapshot.governance.rules.admission,
        inputs.capability_covers,
        &inputs.facts,
        now,
    ) {
        return Err(format!(
            "this place's signed admission policy refuses site publication: {reason:?}"
        ));
    }
    let founding = identity
        .derive_keypair(&founding_salt(moot))
        .map_err(|e| e.to_string())?;
    // Only the actual constitutional founder can grant this explicit publishing
    // action. Grants are narrow: provider writes; current Read members may read.
    let mut rules = snapshot.governance.rules.clone();
    let mut grants = vec![
        (provider, gemot::moot::records::fauna_cap()),
        (provider, gemot::moot::collection_cap(collection)),
        (provider, catalogue_cap(collection.collection_id.0)),
    ];
    for member in &snapshot.membership.members {
        if member.access >= MootAccessLevel::Read {
            grants.push((
                member.member,
                gemot::moot::workflow::namespace::collection_read_cap(collection),
            ));
        }
    }
    for (subject, cap) in grants {
        let path = cap_path(&cap);
        let mut bytes = path.as_bytes().to_vec();
        bytes.extend(subject);
        bytes.extend(moot);
        rules.grant(CapabilityGrant {
            id: *blake3::hash(&bytes).as_bytes(),
            subject,
            path_prefix: path,
            not_before_ms: now,
            expires_at_ms: None,
            delegation_depth: 0,
        });
    }
    pollster::block_on(open.moot.amend(founding.to_seed(), rules, now))
        .map_err(|e| format!("catalogue publication requires this place's founder: {e}"))?;
    let bytes=serde_json::to_vec(&serde_json::json!({"version":1,"address":draft.address,"title":draft.title,"abstract":draft.abstract_text,"scroll_subject":draft.scroll_subject})).map_err(|e|e.to_string())?;
    let blob = pollster::block_on(open.moot.object_store().sync_store().put_blob(&bytes))
        .map_err(|e| e.to_string())?;
    let shared = pollster::block_on(open.moot.share_authorized_for_identity(
        identity,
        blob.digest.as_32().map_err(|e| e.to_string())?,
        "gemot/public-site-description/v1".into(),
        draft.title.clone(),
        now,
    ))
    .map_err(|e| e.to_string())?;
    if pollster::block_on(
        open.moot
            .authorized_collection(collection.collection_id, now),
    )
    .map_err(|e| e.to_string())?
    .is_none()
    {
        pollster::block_on(open.moot.declare_collection_for_identity(
            identity,
            collection.collection_id,
            draft.title.clone(),
            None,
            now,
        ))
        .map_err(|e| e.to_string())?;
    }
    let previous = pollster::block_on(
        open.moot
            .authorized_collection(collection.collection_id, now),
    )
    .map_err(|e| e.to_string())?
    .ok_or("missing catalogue collection")?;
    for contribution in previous.selected {
        let view = pollster::block_on(
            open.moot
                .authorized_collection(collection.collection_id, now),
        )
        .map_err(|e| e.to_string())?
        .ok_or("missing collection")?;
        pollster::block_on(open.moot.set_collection_membership_for_identity(
            identity,
            collection,
            view.heads,
            CollectionChange::SetMembership {
                contribution,
                included: false,
            },
            now,
        ))
        .map_err(|e| e.to_string())?;
    }
    let view = pollster::block_on(
        open.moot
            .authorized_collection(collection.collection_id, now),
    )
    .map_err(|e| e.to_string())?
    .ok_or("missing collection")?;
    pollster::block_on(open.moot.set_collection_membership_for_identity(
        identity,
        collection,
        view.heads,
        CollectionChange::SetMembership {
            contribution: ContributionRef {
                moot_id: moot,
                share: shared.operation,
            },
            included: true,
        },
        now,
    ))
    .map_err(|e| e.to_string())?;
    let version = pollster::block_on(
        open.moot
            .authorized_collection(collection.collection_id, now),
    )
    .map_err(|e| e.to_string())?
    .ok_or("missing collection")?
    .version;

    let hint = SignedCatalogueAdvertisement::sign(
        identity,
        CatalogueAdvertisement {
            version: 1,
            collection: version,
            provider,
            sequence,
            issued_at_ms: now,
            expires_at_ms: expires,
            description: Some(description),
        },
    )?;
    pollster::block_on(directory.receive(
        &open.moot,
        provider,
        &hint.encode()?,
        now,
        MAX_LIFETIME_MS,
        MAX_ENTRIES,
    ))?;
    save(open, &directory)?;
    publish_history(open)?;
    if let Some(lanes) = &open.lanes {
        lanes.publish_catalogue(hint.encode()?)?;
    }
    Ok(())
}
pub(super) fn withdraw(
    open: &OpenPlace,
    identity: &dyn IdentityProvider,
    collection: [u8; 32],
    now: u64,
) -> Result<(), String> {
    let mut directory = load(open)?;
    let provider = identity.master_public_key().to_bytes();
    let old = directory
        .outgoing(provider, now)
        .into_iter()
        .find(|h| h.advertisement.collection.collection.collection_id.0 == collection)
        .ok_or("no active advertisement owned by this profile")?;
    let mut ad = old.advertisement;
    ad.sequence = directory.next_sequence(provider, collection)?;
    ad.description = None;
    ad.issued_at_ms = now;
    ad.expires_at_ms = now.checked_add(lifetime()?).ok_or("clock overflow")?;
    let hint = SignedCatalogueAdvertisement::sign(identity, ad)?;
    pollster::block_on(directory.receive(
        &open.moot,
        provider,
        &hint.encode()?,
        now,
        MAX_LIFETIME_MS,
        MAX_ENTRIES,
    ))?;
    save(open, &directory)?;
    if let Some(lanes) = &open.lanes {
        lanes.publish_catalogue(hint.encode()?)?;
    }
    Ok(())
}

pub(super) fn snapshot(
    open: &OpenPlace,
    command: &PlaceCommand,
    settings: &PlaceWorkerSettings,
) -> Result<OfflinePlaceSnapshot, String> {
    let mut snapshot = place_snapshot(open, open.binding.moot.0, settings)?;
    if let PlaceCommand::OpenCatalogue { digest } = command {
        snapshot.catalogue_navigation = Some(open_address(
            open,
            *digest,
            settings.authority_clock.now_ms(),
        )?);
    }
    Ok(snapshot)
}

pub(super) fn admit_member(
    open: &OpenPlace,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    member: [u8; 32],
    access: crate::place::PlaceInviteAccess,
    now: u64,
    expires: Option<u64>,
) -> Result<AdmittedOps, String> {
    let advertised = match load(open) {
        Ok(d) => d.outgoing(identity.master_public_key().to_bytes(), now),
        Err(e) => {
            tracing::warn!(%e,"catalogue grants unavailable during invitation");
            Vec::new()
        },
    };
    let authored = super::admit_member(
        &open.moot,
        binding,
        identity,
        member,
        access,
        now,
        expires,
        open.offers_projection(),
    )?;
    if advertised.is_empty() {
        return Ok(authored);
    }
    let mut rules = pollster::block_on(open.moot.snapshot())
        .map_err(|e| e.to_string())?
        .governance
        .rules;
    for hint in advertised
        .into_iter()
        .filter(|h| h.advertisement.description.is_some())
    {
        let cap = gemot::moot::workflow::namespace::collection_read_cap(
            hint.advertisement.collection.collection,
        );
        let path = cap_path(&cap);
        let mut bytes = path.as_bytes().to_vec();
        bytes.extend(member);
        bytes.extend(binding.moot.0);
        rules.grant(CapabilityGrant {
            id: *blake3::hash(&bytes).as_bytes(),
            subject: member,
            path_prefix: path,
            not_before_ms: now,
            expires_at_ms: expires,
            delegation_depth: 0,
        });
    }
    let key = identity
        .derive_keypair(&founding_salt(binding.moot.0))
        .map_err(|e| e.to_string())?;
    pollster::block_on(open.moot.amend(key.to_seed(), rules, now)).map_err(|e| e.to_string())?;
    publish_history(open)?;
    Ok(authored)
}
