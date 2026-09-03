// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained client for this device's resident Graphshell host.
//!
//! Turnstone is a first-party application, so it enters through the
//! first-party door — `graphshell::native::app_client` — not the browser one.
//! The service owns the worker thread and its runtime; the pane only projects
//! the latest snapshot and asks for a refresh. Sessions are opened per
//! refresh rather than held: the resident host reads its surface once at
//! session start, so a fresh session is also how new receipts become visible.

use std::sync::{Arc, Mutex, mpsc};

use graphshell::native::app_admission::AppId;
use graphshell::native::app_client::AppBrokerClient;
use tokio::sync::mpsc as tokio_mpsc;

/// One receipt card, reduced to what the pane shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ReceiptCardView {
    pub title: String,
    /// Label/value rows straight off the portable card.
    pub values: Vec<(String, String)>,
    pub badges: Vec<String>,
    /// Byte sizes of the captures the card names, in card order. Each one was
    /// actually read through the resident store, so a size here is a receipt
    /// that the bytes are reachable, not an advertisement.
    pub capture_bytes: Vec<usize>,
}

impl ReceiptCardView {
    /// Whether this card is a scenario receipt rather than a graph or sync
    /// card. Read off the badge the receipts module stamps, because the badge
    /// is what the endpoint actually sends -- the adapter name it was composed
    /// under does not cross the wire.
    pub fn is_receipt(&self) -> bool {
        self.badges.iter().any(|badge| badge == "Receipt")
    }
}

/// Pane-visible outcome of the last refresh.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceReceiptsSnapshot {
    pub status: String,
    pub cards: Vec<ReceiptCardView>,
}

impl Default for DeviceReceiptsSnapshot {
    fn default() -> Self {
        Self {
            status: "Not yet connected. Refresh to read this device's cards.".into(),
            cards: Vec::new(),
        }
    }
}

enum Command {
    Refresh,
}

/// Shell-owned handle for reading the resident host's cards.
pub struct DeviceReceiptsService {
    commands: tokio_mpsc::UnboundedSender<Command>,
    snapshot: Arc<Mutex<DeviceReceiptsSnapshot>>,
}

impl DeviceReceiptsService {
    /// Start the worker and read once, so an opened pane is current without
    /// a click when the resident host is up.
    pub fn start() -> Result<Self, String> {
        let (commands, receiver) = tokio_mpsc::unbounded_channel();
        let snapshot = Arc::new(Mutex::new(DeviceReceiptsSnapshot::default()));
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        let worker_snapshot = Arc::clone(&snapshot);
        std::thread::Builder::new()
            .name("turnstone-device-receipts".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_send.send(Err(format!("create receipts runtime: {error}")));
                        return;
                    }
                };
                let _ = ready_send.send(Ok(()));
                runtime.block_on(run(receiver, worker_snapshot));
            })
            .map_err(|error| format!("start device receipts reader: {error}"))?;
        ready_receive
            .recv()
            .map_err(|_| "device receipts reader stopped during startup".to_string())??;
        let service = Self { commands, snapshot };
        service.refresh();
        Ok(service)
    }

    pub fn snapshot(&self) -> DeviceReceiptsSnapshot {
        self.snapshot
            .lock()
            .expect("device receipts snapshot poisoned")
            .clone()
    }

    pub fn refresh(&self) {
        let _ = self.commands.send(Command::Refresh);
    }
}

async fn run(
    mut commands: tokio_mpsc::UnboundedReceiver<Command>,
    snapshot: Arc<Mutex<DeviceReceiptsSnapshot>>,
) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Refresh => {
                let outcome = read_cards().await;
                let mut current = snapshot.lock().expect("device receipts snapshot poisoned");
                match outcome {
                    Ok(cards) => {
                        current.status = if cards.is_empty() {
                            "Connected. The resident host is showing no cards.".into()
                        } else {
                            format!("Connected. {} card(s) on this device.", cards.len())
                        };
                        current.cards = cards;
                    }
                    Err(error) => {
                        // The cards from the last good read stay visible; the
                        // status says the read is stale rather than blanking
                        // the pane on a transient failure.
                        current.status = format!("Resident host not reachable: {error}");
                    }
                }
            }
        }
    }
}

async fn read_cards() -> Result<Vec<ReceiptCardView>, String> {
    let mut client = AppBrokerClient::open(AppId::new("turnstone"))
        .await
        .map_err(|error| error.to_string())?;
    let opened = client
        .open_session()
        .await
        .map_err(|error| error.to_string())?;
    let Some(projection) = opened.descriptor.projections.first() else {
        let _ = client.close().await;
        return Ok(Vec::new());
    };
    let snapshot = client
        .snapshot(projection.request.clone())
        .await
        .map_err(|error| error.to_string())?;
    let mut cards = Vec::new();
    for offer in snapshot.presentation.offers.values().flatten() {
        if offer.codec != chirograph::PresentationCodec::PortableCardV1 {
            continue;
        }
        let bytes = client
            .resource(snapshot.session.clone(), offer.resource)
            .await
            .map_err(|error| error.to_string())?;
        let card: chirograph::PortableCardV1 =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let mut capture_bytes = Vec::new();
        for capture in &card.media {
            let bytes = client
                .resource(snapshot.session.clone(), *capture)
                .await
                .map_err(|error| error.to_string())?;
            capture_bytes.push(bytes.len());
        }
        cards.push(ReceiptCardView {
            title: card.title,
            values: card
                .values
                .into_iter()
                .map(|value| (value.label, value.value))
                .collect(),
            badges: card.badges,
            capture_bytes,
        });
    }
    let _ = client.close().await;
    // Receipts first. The resident host offers its cards in graph order, so on
    // a device with any history the sync card and the blob-availability cards
    // bury the receipts -- in a pane named for them. Stable within each group,
    // so the host's own ordering still decides ties.
    cards.sort_by_key(|card| !card.is_receipt());
    Ok(cards)
}
