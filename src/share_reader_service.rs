// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained recipient service for one private Knot handoff at a time.
//!
//! This owns the protocol-scoped reader key and its p2panda carrier. The pane
//! only holds temporary input and projects a checked document. Active mDNS is
//! attempted first; the sender's endpoint ticket is a direct-address fallback,
//! never a relay configuration.

use std::sync::{Arc, Mutex, mpsc};

use identity::IdentityProvider;
use knot::{
    KNOT_PUBLISH_READER_KEY_CONTEXT, KnotPublishClientError, KnotPublishedDocument, ProfileRef,
    decode_share_ticket, fetch_published_document, hex32, publish_alpn,
};
use tokio::sync::mpsc as tokio_mpsc;
use transport::Transport;
use transport::p2panda_transport::{MdnsDiscoveryMode, P2pandaTransport};

use crate::identity::RootIdentity;
use crate::publish_service::{PROFILE_ID, PROFILE_REVISION};

/// Reader-visible outcome retained while the Shared Knot pane is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedKnotSnapshot {
    pub status: String,
    pub route: String,
    pub reader_key: String,
    pub document: Option<KnotPublishedDocument>,
}

impl Default for SharedKnotSnapshot {
    fn default() -> Self {
        Self {
            status: "Starting private-share reader…".into(),
            route: "mDNS direct LAN first; endpoint ticket fallback; no relay configured".into(),
            reader_key: String::new(),
            document: None,
        }
    }
}

enum ReaderCommand {
    Open(String),
}

/// Shell-owned handle for local ticket import and private one-document reads.
pub struct KnotShareReaderService {
    commands: Option<tokio_mpsc::UnboundedSender<ReaderCommand>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    snapshot: Arc<Mutex<SharedKnotSnapshot>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl KnotShareReaderService {
    /// Starts a stable reader key and an active-mDNS carrier from the profile
    /// root. It does not require a local Knot authoring host.
    pub fn start(identity: Arc<RootIdentity>) -> Result<Self, String> {
        let (commands, receiver) = tokio_mpsc::unbounded_channel();
        let (shutdown, shutdown_receive) = tokio::sync::oneshot::channel();
        let snapshot = Arc::new(Mutex::new(SharedKnotSnapshot::default()));
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        let worker_snapshot = Arc::clone(&snapshot);
        let worker = std::thread::Builder::new()
            .name("turnstone-knot-share-reader".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_send.send(Err(format!("create reader runtime: {error}")));
                        return;
                    }
                };
                if let Err(error) =
                    runtime.block_on(run(identity, receiver, shutdown_receive, worker_snapshot, ready_send))
                {
                    tracing::warn!(%error, "Knot share reader stopped");
                }
            })
            .map_err(|error| format!("start Knot share reader: {error}"))?;
        let service = Self {
            commands: Some(commands),
            shutdown: Some(shutdown),
            snapshot,
            worker: Some(worker),
        };
        match ready_receive.recv() {
            Ok(Ok(())) => Ok(service),
            Ok(Err(error)) => {
                drop(service);
                Err(error)
            }
            Err(_) => {
                drop(service);
                Err("Knot share reader stopped during startup".to_string())
            }
        }
    }

    pub fn snapshot(&self) -> SharedKnotSnapshot {
        self.snapshot
            .lock()
            .expect("share reader snapshot poisoned")
            .clone()
    }

    /// The pasted ticket stays only in the command and worker stack; it is
    /// not serialized into the shell's retained layout or session state.
    pub fn open(&self, ticket: String) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(ReaderCommand::Open(ticket));
        }
    }
}

impl Drop for KnotShareReaderService {
    fn drop(&mut self) {
        // Closing the command sender lets the async owner close its carrier;
        // joining makes that close complete before the shell drops its last
        // service state.
        self.commands.take();
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

async fn run(
    identity: Arc<RootIdentity>,
    mut commands: tokio_mpsc::UnboundedReceiver<ReaderCommand>,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
    snapshot: Arc<Mutex<SharedKnotSnapshot>>,
    ready: mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let reader = identity
        .derive_keypair(KNOT_PUBLISH_READER_KEY_CONTEXT)
        .map_err(|error| format!("derive Knot reader key: {error}"))?;
    let reader_key = reader.public_key().to_bytes();
    let carrier = P2pandaTransport::builder_from_seed(reader.to_seed())
        .alpns(vec![publish_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|error| format!("bind Knot share reader carrier: {error}"))?;
    if carrier.local_peer_id().to_bytes() != reader_key {
        let error = "Knot share reader carrier identity differs from its reader key".to_string();
        let _ = ready.send(Err(error.clone()));
        let _ = carrier.close().await;
        return Err(error);
    }
    {
        let mut current = snapshot.lock().expect("share reader snapshot poisoned");
        current.reader_key = hex32(&reader_key);
        current.status =
            "Ready. Give this reader key to the publisher before they issue a share.".into();
    }
    let _ = ready.send(Ok(()));

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break,
            command = commands.recv() => {
                let Some(command) = command else { break; };
                match command {
                    ReaderCommand::Open(encoded) => {
                        tokio::select! {
                            biased;
                            _ = &mut shutdown => break,
                            _ = open_ticket(&carrier, &reader, &snapshot, encoded) => {}
                        }
                    }
                }
            }
        }
    }
    carrier
        .close()
        .await
        .map_err(|error| format!("close Knot share reader carrier: {error}"))?;
    Ok(())
}

async fn open_ticket(
    carrier: &P2pandaTransport,
    reader: &identity::Ed25519Keypair,
    snapshot: &Arc<Mutex<SharedKnotSnapshot>>,
    encoded: String,
) {
    let ticket = match decode_share_ticket(&encoded) {
        Ok(ticket) => ticket,
        Err(error) => {
            clear_with_status(
                snapshot,
                format!("Could not import this private ticket: {error}"),
            );
            return;
        }
    };
    let recipient = ticket
        .delegations
        .last()
        .map(|certificate| certificate.certificate.subject);
    if recipient != Some(reader.public_key().to_bytes()) {
        clear_with_status(
            snapshot,
            "This private ticket belongs to a different reader key. Ask the publisher to issue one for the key shown here.".to_string(),
        );
        return;
    }
    set_status(snapshot, "Trying direct mDNS/known-LAN route…");
    let profile = ProfileRef {
        id: PROFILE_ID.into(),
        revision: PROFILE_REVISION,
    };
    match fetch_published_document(carrier, reader, profile.clone(), &ticket).await {
        Ok(read) => apply_read(
            snapshot,
            read,
            "Read through direct mDNS or a known LAN address.",
        ),
        Err(error) if error.allows_endpoint_fallback() => {
            set_status(
                snapshot,
                "No direct discovery route yet. Trying the ticket's direct endpoint…",
            );
            let peer = match carrier.add_peer_ticket(&ticket.endpoint_ticket).await {
                Ok(peer) if peer.to_bytes() == ticket.publisher => peer,
                Ok(_) => {
                    clear_with_status(
                        snapshot,
                        "Ticket endpoint does not name its published carrier.".into(),
                    );
                    return;
                }
                Err(error) => {
                    clear_with_status(
                        snapshot,
                        format!("Could not use the ticket endpoint: {error}"),
                    );
                    return;
                }
            };
            let _ = peer;
            match fetch_published_document(carrier, reader, profile, &ticket).await {
                Ok(read) => apply_read(
                    snapshot,
                    read,
                    "Read through the ticket-provided direct address.",
                ),
                Err(error) => apply_read_error(snapshot, error),
            }
        }
        Err(error) => apply_read_error(snapshot, error),
    }
}

fn apply_read(snapshot: &Arc<Mutex<SharedKnotSnapshot>>, read: knot::KnotPublishRead, route: &str) {
    let mut current = snapshot.lock().expect("share reader snapshot poisoned");
    match read {
        knot::KnotPublishRead::Document(document) => {
            current.document = Some(document);
            current.status = route.into();
        }
        knot::KnotPublishRead::NotAvailable => {
            current.document = None;
            current.status = "Not available.".into();
        }
    }
}

fn apply_read_error(snapshot: &Arc<Mutex<SharedKnotSnapshot>>, error: KnotPublishClientError) {
    let status = match error {
        KnotPublishClientError::Refused => "Not available.".to_string(),
        KnotPublishClientError::TicketCommitment => {
            "The host returned a document outside this private ticket.".to_string()
        }
        other => format!("Could not read this private ticket: {other}"),
    };
    clear_with_status(snapshot, status);
}

fn clear_with_status(snapshot: &Arc<Mutex<SharedKnotSnapshot>>, status: String) {
    let mut current = snapshot.lock().expect("share reader snapshot poisoned");
    current.document = None;
    current.status = status;
}

fn set_status(snapshot: &Arc<Mutex<SharedKnotSnapshot>>, status: impl Into<String>) {
    snapshot
        .lock()
        .expect("share reader snapshot poisoned")
        .status = status.into();
}
