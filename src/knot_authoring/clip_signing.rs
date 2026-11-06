// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Signing live clips as editions with the publisher's chosen tier key
//! (release log D3 and R7).
//!
//! The publisher picks a tier; the default is [`KeyTier::Unassociated`], a
//! key born outside the persona's tree so the clip's log links to no persona.
//! A tier whose key cannot be had is an error, never a quiet fall back to a
//! linked key. Every edition is checked through `verify_edition_chain` before
//! it is recorded, so a clip that does not verify is never offered.

use std::sync::{Arc, Mutex};

use gemot_site::clip::clip_address;
use gemot_site::editions::{self, EditionLimits, EditionOperation, EditionWriter, SigningKey};
use identity::KeyTier;
use zeroize::Zeroizing;

use super::clip_edition;

/// Where the signing keys come from. Production holds them in custody, so the
/// seed is a transient copy that the caller clears.
pub trait ClipKeySource: Send + Sync {
    /// The Ed25519 seed of the publisher's key at `tier`.
    fn clip_seed(&self, tier: KeyTier) -> Result<Zeroizing<[u8; 32]>, String>;
}

/// A clip signed as the first edition of a fresh log.
#[derive(Clone, Debug)]
pub struct SignedClip {
    pub tier: KeyTier,
    /// The publisher's log key, the address's host.
    pub publisher: [u8; 32],
    /// The `knot://` address that opens the clip by its edition.
    pub address: String,
    pub operation: EditionOperation,
    pub payload: Vec<u8>,
}

/// Signs this engine's clips and keeps what it issued.
pub struct ClipSigning {
    keys: Arc<dyn ClipKeySource>,
    tier: KeyTier,
    issued: Mutex<Vec<SignedClip>>,
    last_error: Mutex<Option<String>>,
}

impl ClipSigning {
    /// Sign with `keys` at the default, unassociated tier.
    pub fn new(keys: Arc<dyn ClipKeySource>) -> Arc<Self> {
        Self::with_tier(keys, KeyTier::Unassociated)
    }

    /// Sign at the tier the publisher chose.
    pub fn with_tier(keys: Arc<dyn ClipKeySource>, tier: KeyTier) -> Arc<Self> {
        Arc::new(Self {
            keys,
            tier,
            issued: Mutex::new(Vec::new()),
            last_error: Mutex::new(None),
        })
    }

    /// Sign at the tier `TURNSTONE_CLIP_KEY_TIER` names (`associated` or
    /// `unassociated`); unset or anything else, the unassociated default.
    pub fn from_env(keys: Arc<dyn ClipKeySource>) -> Arc<Self> {
        let tier = match std::env::var("TURNSTONE_CLIP_KEY_TIER").as_deref() {
            Ok("associated") => KeyTier::Associated,
            _ => KeyTier::Unassociated,
        };
        Self::with_tier(keys, tier)
    }

    pub fn tier(&self) -> KeyTier {
        self.tier
    }

    /// Editions signed so far, oldest first.
    pub fn issued(&self) -> Vec<SignedClip> {
        self.issued.lock().map(|v| v.clone()).unwrap_or_default()
    }

    /// Why the last clip went unsigned, if it did.
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|e| e.clone())
    }

    /// Build, sign and verify one clip's edition.
    pub fn sign(
        &self,
        source_url: &str,
        title: Option<&str>,
        selector: Option<&str>,
        knot_body: &str,
        created_ms: u64,
    ) -> Result<SignedClip, String> {
        let seed = self.keys.clip_seed(self.tier)?;
        let key = SigningKey::from_bytes(&seed);
        let publisher = *key.verifying_key().as_bytes();
        let (operation, payload) = clip_edition(
            source_url,
            title,
            selector,
            knot_body,
            &mut EditionWriter::new(key),
            created_ms,
        )?;
        let chain = editions::verify_edition_chain(
            &publisher,
            std::slice::from_ref(&operation),
            EditionLimits::default(),
        )
        .map_err(|e| e.to_string())?;
        let address = clip_address(publisher, chain.head().id).to_string();
        Ok(SignedClip {
            tier: self.tier,
            publisher,
            address,
            operation,
            payload,
        })
    }

    /// Keep a clip once its Knot insert was accepted, or the reason it is unsigned.
    pub(super) fn record(&self, signed: Result<SignedClip, String>) {
        match signed {
            Ok(clip) => {
                if let Ok(mut issued) = self.issued.lock() {
                    issued.push(clip);
                }
                if let Ok(mut error) = self.last_error.lock() {
                    *error = None;
                }
            },
            Err(reason) => {
                if let Ok(mut error) = self.last_error.lock() {
                    *error = Some(reason);
                }
            },
        }
    }
}
