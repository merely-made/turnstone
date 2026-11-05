// SPDX-License-Identifier: MPL-2.0
//! Explicit disclosure and worker-validated navigation for site catalogues.
use super::*;
use crate::place::catalogue::{CatalogueSiteDraft, CatalogueVisibility, DiscoveredSite};
use gemot::moot::workflow::discovery::*;
use gemot::moot::{CollectionChange, ContributionRef};
const MAX_ENTRIES: usize = 128;
const MAX_LIFETIME_MS: u64 = 60 * 60 * 1000;
#[path = "catalogue/cache.rs"]
mod cache;
pub(super) use cache::refresh;
use cache::{MODES, load, outgoing, retain, save, wire_bytes};
#[path = "catalogue/publishing.rs"]
mod publishing;
pub(super) use publishing::{advertise, withdraw};
#[cfg(test)]
#[path = "catalogue/tests.rs"]
mod tests;
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
    let advertised = match outgoing(open, identity.master_public_key().to_bytes(), now) {
        Ok(hints) => hints,
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
