// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shared proof setup: actual signed owners, caller-selected public unsealed
//! storage and Notochord's verified sans-I/O admission. The default is memory;
//! durable mount tests select redb. No live carrier or private custody is proved.
#![allow(dead_code)]

use eidetic::manifest::save_manifest;
use eidetic::{
    BlobManifest, BlobSource, ManifestId, PrivacyClass, ProvenanceRecord, SchemaRef, Timestamp,
    TrustEnvelope,
};
use gemot::moot::constitution::{CapabilityGrant, ConstitutionRules};
use gemot::moot::{
    AvailabilityPolicy, CollectionChange, CollectionId, CollectionRef, ContributionRef,
    ErasurePolicy, KeepBound, Moot, MootAccessLevel, MootId, MootMember, MootMembershipAction,
    MootRetentionSettings, PolicyRevision,
};
use graphshell::admission::{
    CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_PROTOCOL, PROJECTION_SERVICE, admit_session,
    open_session,
};
use graphshell::moot_resources::{Error, Mount, Owners, Scope, Update};
use insigne::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use moot::resource_owner::ForkDestination;
use muniment::{Backend, MemoryBackend};
use notochord::{
    CarrierKind, LocalNetworkPolicy, NetworkId, ProfileRef, ProofBinding, RetainedAuthority,
    RevocationLedger, ServiceAccess, ServiceRule, SessionFacts, TrafficClass, TrustedRoot,
};
use personae::delegation::Issue;
use personae::{IdentityProvider, InMemoryProvider};
use proofs::Digest;
use servitor::{Cap, cap_path};

pub const MOOT: [u8; 32] = [71; 32];
pub const COLLECTION: [u8; 32] = [72; 32];
pub const DESTINATION: [u8; 32] = [73; 32];
pub const BOB_FORK_GRANT: [u8; 32] = [43; 32];
pub const BODY: &[u8] = b"# Our notebook\n\nAlice owns this local notebook contribution.\nBob can open it and fork the collection references.\n";
const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT: [u8; 32] = [7; 32];

pub struct Fixture<B = MemoryBackend> {
    pub alice: InMemoryProvider,
    pub bob: InMemoryProvider,
    pub moot: Moot<B>,
    pub blobs: B,
    pub rules: ConstitutionRules,
}

pub fn retention() -> MootRetentionSettings {
    MootRetentionSettings {
        revision: PolicyRevision(Digest::blake3(b"moot-host-proof")),
        availability: AvailabilityPolicy {
            promised_floor: KeepBound::Forever,
        },
        erasure: ErasurePolicy {
            history_ceiling: KeepBound::UntilCheckpoint,
        },
    }
}

impl Fixture {
    pub async fn new() -> Self {
        let founder = InMemoryProvider::from_seed([11; 32])
            .master_public_key()
            .to_bytes();
        Self::seed(
            Moot::in_memory(MootId(MOOT), founder, retention()),
            MemoryBackend::new(),
        )
        .await
    }
}

impl<B: Backend + Clone> Fixture<B> {
    /// Seed the same signed owner fixture into a caller-owned backend.
    pub async fn seed(moot: Moot<B>, blobs: B) -> Self {
        let alice = InMemoryProvider::from_seed([11; 32]);
        let bob = InMemoryProvider::from_seed([12; 32]);
        let a = alice.master_public_key().to_bytes();
        let b = bob.master_public_key().to_bytes();
        let mut rules = ConstitutionRules::founder_only(a);
        rules.admission = mien::Policy::MembersOnly {
            rate_limit: 100,
            rate_window_ms: 1_000,
        };
        for (id, subject, scope) in [
            ([41; 32], a, "moot"),
            (BOB_FORK_GRANT, b, "moot/collection"),
        ] {
            rules.grant(CapabilityGrant {
                id,
                subject,
                path_prefix: cap_path(&Cap::scope(scope).unwrap()),
                not_before_ms: 1,
                expires_at_ms: None,
                delegation_depth: 0,
            });
        }
        moot.found(
            alice.master_keypair().to_seed(),
            None,
            None,
            rules.clone(),
            1,
        )
        .await
        .unwrap();
        moot.update_membership_for_identity(
            &alice,
            MootMembershipAction::Create {
                initial_members: vec![
                    MootMember {
                        member: a,
                        access: MootAccessLevel::Manage,
                    },
                    MootMember {
                        member: b,
                        access: MootAccessLevel::Write,
                    },
                ],
            },
        )
        .await
        .unwrap();
        let id = ManifestId::of_blob(BODY);
        blobs.put("blob:notebook", BODY).await.unwrap();
        save_manifest(
            &mut blobs.clone(),
            &BlobManifest {
                id,
                schema: SchemaRef::from_id(ManifestId::of_blob(b"text/plain")),
                content_hash: id.0,
                byte_size: BODY.len() as u64,
                created_at: Timestamp::ZERO,
                last_accessed: None,
                sources: vec![BlobSource::Local {
                    key: "blob:notebook".into(),
                }],
                privacy: PrivacyClass::PublicPortable,
                provenance: ProvenanceRecord::self_imported("Moot host proof notebook"),
                trust: TrustEnvelope::self_asserted(),
                schema_metadata: serde_json::json!({}),
                manifest_version: BlobManifest::CURRENT_VERSION,
            },
        )
        .await
        .unwrap();
        let share = moot
            .share_authorized_for_identity(
                &alice,
                *id.0.as_bytes(),
                "text/plain".into(),
                "Our notebook".into(),
                10,
            )
            .await
            .unwrap();
        moot.declare_collection_for_identity(
            &alice,
            CollectionId(COLLECTION),
            "Friends' notebooks".into(),
            None,
            11,
        )
        .await
        .unwrap();
        let heads = moot
            .authorized_collection(CollectionId(COLLECTION), 12)
            .await
            .unwrap()
            .unwrap()
            .heads;
        moot.set_collection_membership_for_identity(
            &alice,
            CollectionRef {
                moot_id: MOOT,
                collection_id: CollectionId(COLLECTION),
            },
            heads,
            CollectionChange::SetMembership {
                contribution: ContributionRef {
                    moot_id: MOOT,
                    share: share.operation,
                },
                included: true,
            },
            12,
        )
        .await
        .unwrap();
        Self {
            alice,
            bob,
            moot,
            blobs,
            rules,
        }
    }

    pub fn grant(&self, reader: &InMemoryProvider, expires: u64) -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            &self.alice,
            DelegationCertificate::new(
                DelegationParent::Root(ROOT),
                self.alice.master_public_key().to_bytes(),
                reader.master_public_key().to_bytes(),
                CapabilityScope {
                    domain: GRAPHSHELL_DOMAIN.into(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: PROJECTION_SERVICE.into(),
                    actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
                },
                5,
                10,
                Some(expires),
                1,
                [1; 32],
            ),
        )
        .unwrap()
    }

    pub fn authority(&self, reader: &InMemoryProvider, expires: u64) -> RetainedAuthority {
        self.authority_at(reader, expires, 20)
    }

    pub fn authority_at(
        &self,
        reader: &InMemoryProvider,
        expires: u64,
        at_ms: u64,
    ) -> RetainedAuthority {
        let grant = self.grant(reader, expires);
        let profile = ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        };
        let mut policy = LocalNetworkPolicy::closed(NETWORK);
        policy.accepted_profiles = vec![profile.clone()];
        policy.trusted_roots = vec![TrustedRoot {
            authority: ROOT,
            issuer: self.alice.master_public_key().to_bytes(),
        }];
        policy.services.insert(
            PROJECTION_SERVICE.into(),
            ServiceRule::new(
                ServiceAccess::MemberOnly,
                GRAPHSHELL_DOMAIN,
                [CONNECT_ACTION],
                false,
                None,
            ),
        );
        let hello = open_session(
            reader,
            NETWORK,
            profile,
            TrafficClass::Interactive,
            [5; 32],
            &ProofBinding::initiator(PROJECTION_PROTOCOL, None, None),
            vec![grant.clone()],
        )
        .unwrap();
        let bytes = hello.encode(&policy.limits.clamped()).unwrap();
        let (_, principal) = admit_session(
            &policy,
            &RevocationLedger::default(),
            &bytes,
            &SessionFacts::new(PROJECTION_PROTOCOL, CarrierKind::P2panda),
            at_ms,
            0,
        );
        RetainedAuthority::new(principal.expect("actual Graphshell admission"), vec![grant])
    }

    pub async fn mount(&self, id: &str, reader: &InMemoryProvider) -> Mount {
        let mut store = self.blobs.clone();
        Mount::open(
            id,
            self.authority(reader, 1_000),
            Scope {
                collection: COLLECTION,
                fork_destination: Some(ForkDestination {
                    id: DESTINATION,
                    name: "Our retained references".into(),
                }),
            },
            &mut Owners {
                moot: &self.moot,
                identity: reader,
                store: &mut store,
                revocations: &RevocationLedger::default(),
                at_ms: 20,
            },
        )
        .await
        .unwrap()
    }

    pub async fn poll(
        &self,
        mount: &mut Mount,
        reader: &InMemoryProvider,
        ledger: &RevocationLedger,
        at_ms: u64,
    ) -> Result<Option<Update>, Error> {
        let mut store = self.blobs.clone();
        mount
            .poll(&mut Owners {
                moot: &self.moot,
                identity: reader,
                store: &mut store,
                revocations: ledger,
                at_ms,
            })
            .await
    }
}
