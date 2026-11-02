// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Dial the given tickets and join all ten of the place's lanes.
///
/// An EMPTY ticket list is a listen-only bind, not a refusal: a founder who
/// has invited nobody yet still needs an endpoint before it can mint the
/// rendezvous its first invitation carries.
///
/// Takes the already-opened place rather than opening its own, so the lanes
/// drain into the same stores the worker's projections fold from. The
/// runtime is created here and owned by the returned value; the worker
/// thread stays synchronous.
pub(crate) fn join_live(
    open: &OpenPlace,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    tickets: &[String],
    projection: Option<ProjectionSetup>,
    watch: Option<(
        armillary::Emitter<crate::action::Update>,
        crate::panes::SessionId,
        u64,
    )>,
) -> Result<LiveLanes, String> {
    let keypair = identity
        .derived_keypair(&transport_salt(binding.moot.0))
        .map_err(|error| format!("derive transport identity: {error}"))?;
    let overlays = [
        sync_overlay_topic(binding.moot.0),
        sync_overlay_topic(binding.root.0),
        sync_overlay_topic(binding.chat.0),
        group_key_overlay(binding.moot.0),
    ];

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|error| format!("build lane runtime: {error}"))?;

    let (transport, local_rendezvous, moot_lanes, graph, chat, group_keys) =
        runtime.block_on(async {
            // Active mDNS so two peers on one LAN re-find each other by node id
            // after either restarts on a fresh port; a ticket fixes an address,
            // not an identity.
            let transport = P2pandaTransport::builder_for(&keypair)
                .gossip()
                .mdns(MdnsDiscoveryMode::Active)
                // Beside the sync lanes, on the same endpoint: a place member
                // reaching the document's holder is the same peer relationship,
                // and giving it a second bind would give it a second address.
                .alpns(vec![graphshell_host::carrier::projection_alpn()])
                .bind()
                .await
                .map_err(|error| format!("bind place transport: {error}"))?;
            let local_rendezvous: Vec<String> = transport
                .ticket()
                .await
                .map(|ticket| vec![ticket])
                .unwrap_or_default();
            for ticket in tickets {
                let peer = transport
                    .add_peer_ticket(ticket)
                    .await
                    .map_err(|error| format!("import rendezvous ticket: {error}"))?;
                transport
                    .set_topics(peer, &overlays)
                    .await
                    .map_err(|error| format!("tag rendezvous overlays: {error}"))?;
            }
            let (endpoint, gossip) = transport
                .sync_parts()
                .ok_or_else(|| "place transport has no gossip".to_string())?;
            let moot_lanes = open
                .moot
                .join_lanes(endpoint.clone(), gossip.clone())
                .await
                .map_err(|error| format!("join Gemot lanes: {error}"))?;
            let graph = open
                .graph
                .join(endpoint.clone(), gossip.clone())
                .await
                .map_err(|error| format!("join graph lane: {error}"))?;
            let chat = open
                .chat
                .join(endpoint.clone(), gossip.clone())
                .await
                .map_err(|error| format!("join chat lane: {error}"))?;
            let group_keys = open
                .group
                .lane
                .join(endpoint, gossip)
                .await
                .map_err(|error| format!("join group-key lane: {error}"))?;
            Ok::<_, String>((
                transport,
                local_rendezvous,
                moot_lanes,
                graph,
                chat,
                group_keys,
            ))
        })?;

    let mut lanes = LiveLanes {
        watcher: None,
        local_rendezvous,
        dialed_rendezvous: tickets.len(),
        joined: Some(JoinedLanes {
            moot: moot_lanes,
            graph,
            chat,
            group_keys,
        }),
        moot: binding.moot.0,
        projection: None,
        _transport: Arc::new(transport),
        _runtime: Some(runtime),
    };

    // No vault root means no route and no serving, and the snapshot says so
    // rather than reporting an idle host nobody could have reached.
    if let Some(setup) = projection {
        let handle = lanes
            ._runtime
            .as_ref()
            .expect("lane runtime is present")
            .handle()
            .clone();
        // Grants revoked before this bind are refused from its first accept.
        let withdrawn = crate::place::worker::withdrawn_projection_grants(open, identity)?;
        lanes.projection = Some(ProjectionServing::start(
            Arc::clone(&lanes._transport),
            &handle,
            &setup,
            &withdrawn,
        )?);
    }

    // The watcher turns lane arrivals AND projection session changes into ONE
    // app-visible nudge per settled burst. It reports that something moved; it
    // never folds a projection itself, because the authority filter belongs on
    // the worker thread with the stores, not on a sampling task.
    //
    // Built after the projection host so the sample covers it: without that,
    // the founder's status stays at "0 live sessions" for as long as nothing
    // else on the lanes happens to arrive.
    if let Some((out, session, generation)) = watch {
        let counters = lanes.counter_handles();
        let handle = lanes
            ._runtime
            .as_ref()
            .expect("lane runtime is present")
            .handle()
            .clone();
        lanes.watcher = Some(handle.spawn(async move {
            let mut settled = counters.total();
            loop {
                tokio::time::sleep(WATCH_TICK).await;
                let now = counters.total();
                if now == settled {
                    continue;
                }
                // Wait for the burst to stop growing before reporting, so a
                // sync round of fifty operations is one re-fold and not fifty.
                tokio::time::sleep(WATCH_TICK).await;
                let after = counters.total();
                if after != now {
                    continue;
                }
                settled = after;
                out.emit(crate::action::Update::PlaceLanesAdvanced {
                    session,
                    generation,
                });
            }
        }));
    }
    Ok(lanes)
}
