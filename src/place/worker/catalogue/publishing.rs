// SPDX-License-Identifier: MPL-2.0
use super::*;
pub(crate) fn advertise(
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
    let mut directory = load(open, draft.visibility)?;
    let mut seed = b"turnstone/public-site-collection/v1/".to_vec();
    seed.extend(moot);
    seed.extend(provider);
    seed.extend(draft.address.as_bytes());
    let id = match draft.visibility {
        CatalogueVisibility::Public => *blake3::hash(&seed).as_bytes(),
        CatalogueVisibility::Members => {
            let mut salt = b"turnstone.place.protected-catalogue-id.v1/".to_vec();
            salt.extend(moot);
            salt.extend(blake3::hash(draft.address.as_bytes()).as_bytes());
            let key = identity.derive_keypair(&salt).map_err(|e| e.to_string())?;
            *blake3::hash(&key.public_key().to_bytes()).as_bytes()
        },
    };
    let collection = CollectionRef {
        moot_id: moot,
        collection_id: CollectionId(id),
    };
    let (label,schema,body)=match draft.visibility {
        CatalogueVisibility::Public => (draft.title.clone(), "gemot/public-site-description/v1",serde_json::to_vec(&serde_json::json!({"version":1,"address":draft.address,"title":draft.title,"abstract":draft.abstract_text,"scroll_subject":draft.scroll_subject})).map_err(|e|e.to_string())?),
        CatalogueVisibility::Members => ("Protected site".into(),"gemot/protected-site-description/v1",protected::seal_description(moot,id,&open.group.keys.current(),&description)?),
    };
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
    let bytes = body;
    let blob = pollster::block_on(open.moot.object_store().sync_store().put_blob(&bytes))
        .map_err(|e| e.to_string())?;
    let shared = pollster::block_on(open.moot.share_authorized_for_identity(
        identity,
        blob.digest.as_32().map_err(|e| e.to_string())?,
        schema.into(),
        label.clone(),
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
            label,
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
            version: if draft.visibility == CatalogueVisibility::Public {
                1
            } else {
                2
            },
            audience: if draft.visibility == CatalogueVisibility::Members {
                Some(CatalogueAudience { members_of: moot })
            } else {
                None
            },
            collection: version,
            provider,
            sequence,
            issued_at_ms: now,
            expires_at_ms: expires,
            description: Some(description),
        },
    )?;
    retain(open, &mut directory, &hint, now)?;
    save(open, draft.visibility, &directory)?;
    publish_history(open)?;
    if let Some(lanes) = &open.lanes {
        lanes.publish_catalogue(wire_bytes(open, &hint)?)?;
    }
    Ok(())
}
pub(crate) fn withdraw(
    open: &OpenPlace,
    identity: &dyn IdentityProvider,
    collection: [u8; 32],
    now: u64,
) -> Result<(), String> {
    let provider = identity.master_public_key().to_bytes();
    let mut candidates = Vec::new();
    for visibility in MODES {
        let directory = load(open, visibility)?;
        if let Some(hint) = directory
            .outgoing(provider, now)
            .into_iter()
            .find(|h| h.advertisement.collection.collection.collection_id.0 == collection)
        {
            candidates.push((visibility, directory, hint));
        }
    }
    if candidates.len() != 1 {
        return Err("no unambiguous owned advertisement".into());
    }
    let (visibility, mut directory, old) = candidates.pop().unwrap();
    let mut ad = old.advertisement;
    ad.sequence = directory.next_sequence(provider, collection)?;
    ad.description = None;
    ad.issued_at_ms = now;
    ad.expires_at_ms = now.checked_add(lifetime()?).ok_or("clock overflow")?;
    let hint = SignedCatalogueAdvertisement::sign(identity, ad)?;
    retain(open, &mut directory, &hint, now)?;
    save(open, visibility, &directory)?;
    if let Some(lanes) = &open.lanes {
        lanes.publish_catalogue(wire_bytes(open, &hint)?)?;
    }
    Ok(())
}
