// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A synchronous [`IdentityProvider`] over the custody route, for an
//! application whose code derives keys through a provider (dramatis repo
//! plan, DR-C; D11, D12).
//!
//! [`CustodyIdentity`] answers `derive_keypair` with a release djinn makes
//! under its policy, one salt at a time, and holds what it was given until
//! the vault locks: a watcher on the lock broadcast drops every held key the
//! moment djinn reports Locked or goes away. Attestations and signatures for
//! salts djinn will not release (root-level acts, D11) are made inside djinn
//! through [`CustodyIdentity::attest_derived_key`],
//! [`CustodyIdentity::sign`] and [`CustodyIdentity::issue_certificate`].
//!
//! Calls run on the provider's own thread, so the provider can be used from
//! inside or outside a tokio runtime. With djinn absent or Locked,
//! [`CustodyIdentity::connect`] fails with an error whose
//! [`is_pending`](CustodyClientError::is_pending) is true and every key
//! request answers [`IdentityError::Locked`]: the identity is pending, never
//! replaced by a key of the application's own (D12).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use dramatis::roster::Roster;
use dramatis::view::VaultLockView;
use insigne::DerivedKeyAttestation;
use insigne::delegation::{
    DelegationCertificate, DelegationRevocation, SignedDelegationCertificate,
    SignedDelegationRevocation, delegation_signing_salt,
};
use personae::delegation::DelegationError;
use personae::{
    Ed25519Keypair, Ed25519PublicKey, IdentityError, IdentityProvider, ProfileId, RetainedKeys,
};

use crate::native::app_admission::{AppId, configured_app_endpoint};
use crate::native::app_client::AppClientError;
use crate::native::custody::{CustodyAnswer, CustodyCall, KeySource, ResidentStatus};
use crate::native::custody_client::{BlockingCustodyClient, CustodyClient, CustodyClientError};

type Job = Box<dyn FnOnce(Option<&mut BlockingCustodyClient>) -> bool + Send>;

/// The persona djinn speaks as, reached through the custody route.
pub struct CustodyIdentity {
    master: Ed25519PublicKey,
    status: ResidentStatus,
    jobs: Mutex<mpsc::Sender<Job>>,
    held: Arc<Mutex<HashMap<Vec<u8>, RetainedKeys>>>,
    locked: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl CustodyIdentity {
    /// Connect to djinn at the configured first-party endpoint as `app`.
    /// Pending (see [`CustodyClientError::is_pending`]) when djinn is absent
    /// or Locked.
    pub fn connect(app: &str) -> Result<Self, CustodyClientError> {
        Self::connect_at(&configured_app_endpoint(), app)
    }

    /// The same, at an explicit endpoint.
    pub fn connect_at(endpoint: &str, app: &str) -> Result<Self, CustodyClientError> {
        Self::connect_as_at(endpoint, app, None)
    }

    /// Connect as `app` speaking as persona `profile` without switching djinn
    /// to it (D13); `None` is the persona in use.
    pub fn connect_as(app: &str, profile: Option<ProfileId>) -> Result<Self, CustodyClientError> {
        Self::connect_as_at(&configured_app_endpoint(), app, profile)
    }

    /// [`Self::connect_as`] at an explicit endpoint.
    pub fn connect_as_at(
        endpoint: &str,
        app: &str,
        profile: Option<ProfileId>,
    ) -> Result<Self, CustodyClientError> {
        let app = AppId::new(app);
        let open = move |endpoint: &str, app: AppId| {
            BlockingCustodyClient::open_at(endpoint, app).map(|mut client| {
                client.speak_as(profile.clone());
                client
            })
        };
        let (jobs, received) = mpsc::channel::<Job>();
        let (ready, opened) = mpsc::channel();
        let actor_endpoint = endpoint.to_string();
        let actor_app = app.clone();
        std::thread::Builder::new()
            .name(format!("custody-{app}"))
            .spawn(move || {
                let mut client = match open(&actor_endpoint, actor_app.clone()) {
                    Ok(client) => {
                        let _ = ready.send(Ok(()));
                        Some(client)
                    },
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    },
                };
                while let Ok(job) = received.recv() {
                    if client.is_none() {
                        client = open(&actor_endpoint, actor_app.clone()).ok();
                    }
                    if !job(client.as_mut()) {
                        client = None;
                    }
                }
            })
            .map_err(|error| CustodyClientError::Absent(AppClientError::Io(error)))?;
        opened.recv().map_err(|_| closed())??;

        let status = ask_on(&jobs, |client| client.status())?;
        let Some(master) = status
            .persona_public_key
            .filter(|_| status.lock == VaultLockView::Unlocked)
        else {
            return Err(CustodyClientError::Refused(
                crate::native::custody::CustodyRefusal::Locked,
            ));
        };
        let master = Ed25519PublicKey::from_bytes(&master).map_err(CustodyClientError::Release)?;
        let this = Self {
            master,
            status,
            jobs: Mutex::new(jobs),
            held: Arc::default(),
            locked: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
        };
        this.watch(endpoint.to_string(), app);
        Ok(this)
    }

    /// The status djinn answered when this connected.
    pub fn connected_status(&self) -> &ResidentStatus {
        &self.status
    }

    /// The persona in use when this connected.
    pub fn profile(&self) -> Option<&ProfileId> {
        self.status.profile.as_ref()
    }

    /// What protects the vault, as djinn reports it.
    pub fn protection(&self) -> String {
        format!("{:?} (held by djinn)", self.status.protection)
    }

    /// Whether djinn has reported the vault Locked (or gone) since this
    /// connected: every key request is pending until it unlocks.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }

    /// The lock, the protection and the public roots, now.
    pub fn status(&self) -> Result<ResidentStatus, CustodyClientError> {
        self.ask(|client| client.status())
    }

    /// The vault's personas, for a picker. Answered while Locked too.
    pub fn roster(&self) -> Result<Roster, CustodyClientError> {
        self.ask(|client| client.roster())
    }

    /// Switch the persona djinn speaks as. Keys held for the old one are
    /// dropped; this provider keeps answering for the persona it connected
    /// as, so reconnect to speak as the new one.
    pub fn choose_profile(&self, profile: ProfileId) -> Result<(), CustodyClientError> {
        self.forget();
        self.ask(move |client| done(client.call(CustodyCall::ChooseProfile { profile })?))
    }

    /// Mint a persona in djinn's vault and switch to it.
    pub fn create_profile(
        &self,
        profile: ProfileId,
        display_name: impl Into<String>,
    ) -> Result<(), CustodyClientError> {
        let display_name = display_name.into();
        self.forget();
        self.ask(move |client| {
            done(client.call(CustodyCall::CreateProfile {
                profile,
                display_name,
            })?)
        })
    }

    /// Sign `message` inside djinn with the key `salt` derives: the signing
    /// key's public half and the signature (D11).
    pub fn sign(&self, salt: &[u8], message: &[u8]) -> Result<([u8; 32], Vec<u8>), IdentityError> {
        self.pending()?;
        let (salt, message) = (salt.to_vec(), message.to_vec());
        self.ask(move |client| client.sign(KeySource::Persona, salt, message))
            .map_err(identity_error)
    }

    /// Issue a delegation certificate as this persona, signed inside djinn:
    /// the scope's signing key never leaves it.
    pub fn issue_certificate(
        &self,
        certificate: DelegationCertificate,
    ) -> Result<SignedDelegationCertificate, DelegationError> {
        if !certificate.is_well_formed() {
            return Err(DelegationError::MalformedCertificate);
        }
        if certificate.issuer != self.master.to_bytes() {
            return Err(DelegationError::WrongIssuer);
        }
        let (signer, signature) =
            self.sign_scoped(&delegation_signing_salt(&certificate.scope), &certificate.signing_bytes())?;
        Ok(SignedDelegationCertificate::from_parts(
            certificate,
            signer,
            signature,
        ))
    }

    /// Issue a delegation revocation as this persona, signed inside djinn.
    pub fn issue_revocation(
        &self,
        revocation: DelegationRevocation,
    ) -> Result<SignedDelegationRevocation, DelegationError> {
        if !revocation.is_well_formed() {
            return Err(DelegationError::MalformedRevocation);
        }
        if revocation.issuer != self.master.to_bytes() {
            return Err(DelegationError::WrongIssuer);
        }
        let (signer, signature) =
            self.sign_scoped(&delegation_signing_salt(&revocation.scope), &revocation.signing_bytes())?;
        Ok(SignedDelegationRevocation::from_parts(
            revocation, signer, signature,
        ))
    }

    /// Drop every key held so far.
    pub fn forget(&self) {
        self.held.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    fn sign_scoped(
        &self,
        salt: &[u8],
        message: &[u8],
    ) -> Result<(DerivedKeyAttestation, Vec<u8>), DelegationError> {
        let signer = self
            .attest_derived_key(salt)
            .map_err(|_| DelegationError::Identity)?;
        let (public_key, signature) = self.sign(salt, message).map_err(|_| DelegationError::Identity)?;
        match signer.check(salt) {
            Ok(checked) if checked.derived() == &public_key => Ok((signer, signature)),
            _ => Err(DelegationError::Identity),
        }
    }

    fn pending(&self) -> Result<(), IdentityError> {
        match self.is_locked() {
            true => Err(IdentityError::Locked),
            false => Ok(()),
        }
    }

    fn ask<T: Send + 'static>(
        &self,
        call: impl FnOnce(&mut BlockingCustodyClient) -> Result<T, CustodyClientError> + Send + 'static,
    ) -> Result<T, CustodyClientError> {
        ask_on(&self.jobs.lock().unwrap_or_else(|e| e.into_inner()), call)
    }

    /// Follow the lock broadcast on a second conversation: a Locked vault,
    /// or a djinn that went away, drops every held key (D11, D12).
    fn watch(&self, endpoint: String, app: AppId) {
        let held = Arc::clone(&self.held);
        let locked = Arc::clone(&self.locked);
        let stop = Arc::clone(&self.stop);
        let spawned = std::thread::Builder::new()
            .name(format!("custody-lock-{app}"))
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let drop_all = || {
                    locked.store(true, Ordering::SeqCst);
                    held.lock().unwrap_or_else(|e| e.into_inner()).clear();
                };
                while !stop.load(Ordering::SeqCst) {
                    let mut client =
                        match runtime.block_on(CustodyClient::open_at(&endpoint, app.clone())) {
                            Ok(client) => client,
                            Err(_) => {
                                drop_all();
                                std::thread::sleep(Duration::from_secs(2));
                                continue;
                            },
                        };
                    let mut seen = VaultLockView::Unlocked;
                    loop {
                        match runtime.block_on(client.watch_lock(seen)) {
                            Ok(VaultLockView::Locked) => {
                                drop_all();
                                seen = VaultLockView::Locked;
                            },
                            Ok(VaultLockView::Unlocked) => {
                                locked.store(false, Ordering::SeqCst);
                                seen = VaultLockView::Unlocked;
                            },
                            Err(_) => {
                                drop_all();
                                break;
                            },
                        }
                        if stop.load(Ordering::SeqCst) {
                            return;
                        }
                    }
                }
            });
        if spawned.is_err() {
            tracing::warn!("custody lock watcher could not start; held keys follow calls only");
        }
    }
}

impl Drop for CustodyIdentity {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.forget();
    }
}

impl std::fmt::Debug for CustodyIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustodyIdentity")
            .field("profile", &self.status.profile)
            .field("locked", &self.is_locked())
            .finish_non_exhaustive()
    }
}

impl IdentityProvider for CustodyIdentity {
    fn master_public_key(&self) -> Ed25519PublicKey {
        self.master
    }

    /// The key djinn releases for `salt` (D11); [`IdentityError::Locked`]
    /// while pending, and an error naming the refusal for a salt djinn keeps.
    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError> {
        self.pending()?;
        if let Some(keys) = self.held.lock().unwrap_or_else(|e| e.into_inner()).get(salt) {
            return keys.derive_keypair(salt);
        }
        let asked = salt.to_vec();
        let keys = self
            .ask(move |client| client.release(KeySource::Persona, vec![asked]))
            .map_err(identity_error)?;
        if keys.master_public_key() != self.master {
            return Err(IdentityError::DerivationFailed(
                "djinn now speaks as a different persona; reconnect".into(),
            ));
        }
        let keypair = keys.derive_keypair(salt)?;
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        held.insert(salt.to_vec(), keys);
        if self.is_locked() {
            held.clear();
            return Err(IdentityError::Locked);
        }
        Ok(keypair)
    }

    /// The master's attestation for `salt`'s key, signed inside djinn.
    fn attest_derived_key(&self, salt: &[u8]) -> Result<DerivedKeyAttestation, IdentityError> {
        self.pending()?;
        if let Some(keys) = self.held.lock().unwrap_or_else(|e| e.into_inner()).get(salt) {
            return keys.attest_derived_key(salt);
        }
        let asked = salt.to_vec();
        let attestation = self
            .ask(move |client| client.attest(KeySource::Persona, asked))
            .map_err(identity_error)?;
        match attestation.check(salt) {
            Ok(checked) if checked.master() == &self.master.to_bytes() => Ok(attestation),
            _ => Err(IdentityError::DerivationFailed(
                "djinn's attestation does not name this persona".into(),
            )),
        }
    }
}

fn done(answer: CustodyAnswer) -> Result<(), CustodyClientError> {
    match answer {
        CustodyAnswer::Done => Ok(()),
        _ => Err(CustodyClientError::UnexpectedAnswer {
            asked: "profile",
            got: "another answer",
        }),
    }
}

fn closed() -> CustodyClientError {
    CustodyClientError::Transport(AppClientError::Closed)
}

/// A pending identity reads as Locked; anything else keeps its reason.
fn identity_error(error: CustodyClientError) -> IdentityError {
    match error {
        error if error.is_pending() => IdentityError::Locked,
        CustodyClientError::Release(error) => error,
        other => IdentityError::Backend(other.to_string()),
    }
}

/// Run `call` on the provider's own thread and wait for its answer.
fn ask_on<T: Send + 'static>(
    jobs: &mpsc::Sender<Job>,
    call: impl FnOnce(&mut BlockingCustodyClient) -> Result<T, CustodyClientError> + Send + 'static,
) -> Result<T, CustodyClientError> {
    let (reply, answer) = mpsc::channel();
    let job: Job = Box::new(move |client| match client {
        Some(client) => {
            let result = call(client);
            let healthy = !matches!(
                result,
                Err(CustodyClientError::Transport(_) | CustodyClientError::Absent(_))
            );
            let _ = reply.send(result);
            healthy
        },
        None => {
            let _ = reply.send(Err(CustodyClientError::Absent(AppClientError::Closed)));
            false
        },
    });
    jobs.send(job).map_err(|_| closed())?;
    answer.recv().map_err(|_| closed())?
}
