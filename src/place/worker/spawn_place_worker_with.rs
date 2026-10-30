// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn spawn_place_worker_with(
    wake: Wake,
    identity: Option<Arc<RootIdentity>>,
    settings: PlaceWorkerSettings,
) -> (ActorHandle<PlaceWorkerCommand>, Receiver<Update>) {
    spawn_named(
        "turnstone-place",
        wake,
        move |commands, out: Emitter<Update>| {
            let mut live: Option<OpenPlace> = None;
            let mut live_scope: Option<(SessionId, u64)> = None;
            let mut lifecycle_generation = 0u64;
            let mut adopted = identity;
            let mut waiting: std::collections::VecDeque<PlaceWorkerCommand> =
                std::collections::VecDeque::new();
            loop {
                let command = match adopted.is_some().then(|| waiting.pop_front()).flatten() {
                    Some(command) => command,
                    None => match commands.recv() {
                        Ok(command) => command,
                        Err(_) => break,
                    },
                };
                let command = match command {
                    PlaceWorkerCommand::RetireIdentity(ack) => {
                        live = None;
                        live_scope = None;
                        waiting.clear();
                        adopted = None;
                        let _ = ack.send(());
                        continue;
                    },
                    command => command,
                };
                let identity = match (command, &adopted) {
                    (PlaceWorkerCommand::AdoptIdentity(root), _) => {
                        adopted.get_or_insert(root);
                        continue;
                    },
                    (command, Some(root)) => {
                        let root = Arc::clone(root);
                        (command, root)
                    },
                    // Nothing is open while pending, so a release has nothing
                    // to drop but what that session had waiting.
                    (PlaceWorkerCommand::Release(ack), None) => {
                        waiting.clear();
                        let _ = ack.send(());
                        continue;
                    },
                    #[cfg(test)]
                    (PlaceWorkerCommand::Abandon(ack), None) => {
                        let _ = ack.send(());
                        continue;
                    },
                    #[cfg(test)]
                    (PlaceWorkerCommand::Freeze(_, ack), None) => {
                        let _ = ack.send(());
                        continue;
                    },
                    (command, None) => {
                        waiting.push_back(command);
                        continue;
                    },
                };
                let (command, identity) = identity;
                match command {
                    PlaceWorkerCommand::Open {
                        session,
                        generation,
                        directory,
                        binding,
                    } => {
                        lifecycle_generation = lifecycle_generation.max(generation);
                        live = None;
                        live_scope = None;
                        match open_cached_place(&directory, &binding, identity.as_ref(), &settings)
                        {
                            Ok((opened, snapshot)) => {
                                live_scope = Some((session, generation));
                                live = Some(opened);
                                out.emit(Update::PlaceOpened {
                                    session,
                                    generation,
                                    result: Ok(snapshot),
                                });
                            },
                            Err(error) => out.emit(Update::PlaceOpened {
                                session,
                                generation,
                                result: Err(error),
                            }),
                        }
                    },
                    PlaceWorkerCommand::Join {
                        session,
                        generation,
                        directory,
                        invite,
                    } => {
                        lifecycle_generation = lifecycle_generation.max(generation);
                        // Admission first, then the ordinary cached open. The
                        // second step is not a formality: it proves the place
                        // admission just established actually reopens through
                        // the same path every later boot will use.
                        live = None;
                        live_scope = None;
                        let joined =
                            admit_invitation(&directory, &invite, identity.as_ref(), &settings)
                                .and_then(|admitted| {
                                    crate::place::rendezvous::save_admitted_rendezvous(
                                        &directory, &invite,
                                    )?;
                                    open_cached_place(
                                        &directory,
                                        &admitted.binding,
                                        identity.as_ref(),
                                        &settings,
                                    )
                                    .map(|(opened, snapshot)| (admitted.binding, opened, snapshot))
                                })
                                .and_then(|(binding, mut opened, snapshot)| {
                                    // Dial whatever the envelope offered. A ticketless
                                    // invitation still admits: the place is real and
                                    // offline, which Degraded-vs-Offline surfaces.
                                    let tickets: Vec<String> =
                                        invite.dialable().map(|entry| entry.hint.clone()).collect();
                                    if !tickets.is_empty() {
                                        opened.lanes = Some(crate::place::lanes::join_live(
                                            &opened,
                                            &binding,
                                            identity.as_ref(),
                                            &tickets,
                                            settings.projection_setup(&binding, identity.as_ref()),
                                            // The watcher reports arrivals under THIS
                                            // open's generation, so a nudge from a
                                            // departed place is dropped by the same
                                            // guard every other answer passes.
                                            Some((out.clone(), session, generation)),
                                        )?);
                                    }
                                    Ok((binding, opened, snapshot))
                                });
                        match joined {
                            Ok((binding, opened, snapshot)) => {
                                live_scope = Some((session, generation));
                                live = Some(opened);
                                out.emit(Update::PlaceJoined {
                                    session,
                                    generation,
                                    result: Ok((binding, snapshot)),
                                });
                            },
                            Err(error) => out.emit(Update::PlaceJoined {
                                session,
                                generation,
                                result: Err(error),
                            }),
                        }
                    },
                    PlaceWorkerCommand::Found {
                        session,
                        generation,
                        directory,
                        name,
                    } => {
                        lifecycle_generation = lifecycle_generation.max(generation);
                        live = None;
                        live_scope = None;
                        let founded = found_place(&directory, identity.as_ref(), &name, &settings)
                            .and_then(|binding| {
                                crate::session::save_place_binding(&directory, &binding)
                                    .map_err(|error| format!("persist place binding: {error}"))?;
                                // An EMPTY descriptor, saved on purpose: reconnect
                                // must reach the same listen-only bind as this open.
                                crate::place::rendezvous::save_founder_rendezvous(
                                    &directory, &binding,
                                )?;
                                open_cached_place(
                                    &directory,
                                    &binding,
                                    identity.as_ref(),
                                    &settings,
                                )
                                .map(|(opened, snapshot)| (binding, opened, snapshot))
                            })
                            .and_then(|(binding, mut opened, _stale)| {
                                opened.lanes = Some(crate::place::lanes::join_live(
                                    &opened,
                                    &binding,
                                    identity.as_ref(),
                                    &[],
                                    settings.projection_setup(&binding, identity.as_ref()),
                                    Some((out.clone(), session, generation)),
                                )?);
                                // Re-fold AFTER binding, so the answer already
                                // carries this bind's own rendezvous.
                                place_snapshot(&opened, binding.moot.0, &settings)
                                    .map(|snapshot| (binding, opened, snapshot))
                            });
                        match founded {
                            Ok((binding, opened, snapshot)) => {
                                live_scope = Some((session, generation));
                                live = Some(opened);
                                out.emit(Update::PlaceFounded {
                                    session,
                                    generation,
                                    result: Ok((binding, snapshot)),
                                });
                            },
                            Err(error) => out.emit(Update::PlaceFounded {
                                session,
                                generation,
                                result: Err(error),
                            }),
                        }
                    },
                    PlaceWorkerCommand::OfferPrekey {
                        session,
                        generation,
                        directory,
                        moot,
                    } => {
                        let result = prepare_group_identity(&directory, identity.as_ref(), moot);
                        out.emit(Update::PlacePrekeyOffered {
                            session,
                            generation,
                            result,
                        });
                    },
                    PlaceWorkerCommand::Invite {
                        session,
                        generation,
                        directory,
                        prekey,
                        access,
                        lifetime_ms,
                    } => {
                        let result = match &mut live {
                            Some(_) if live_scope != Some((session, generation)) => {
                                Err("invitation belongs to a departed place generation".to_string())
                            },
                            None => Err("open a place before inviting anyone".to_string()),
                            Some(_)
                                if lifetime_ms.is_some()
                                    && access == crate::place::PlaceInviteAccess::Reader =>
                            {
                                Err("a reader invitation carries no grant to bound".to_string())
                            },
                            Some(open) => {
                                let binding = open.binding.clone();
                                let now_ms = settings.authority_clock.now_ms();
                                let expires_at_ms =
                                    lifetime_ms.map(|lifetime| now_ms.saturating_add(lifetime));
                                // The rendezvous an invitation carries is THIS
                                // bind's ticket, minted when the endpoint came
                                // up. A founder must be live before it invites.
                                let rendezvous: Vec<_> = open
                                    .lanes
                                    .as_ref()
                                    .map(|lanes| lanes.rendezvous().0)
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|hint| crate::place::invite::RendezvousV1 {
                                        carrier: crate::place::invite::P2PANDA_ENDPOINT_TICKET
                                            .into(),
                                        hint,
                                    })
                                    .collect();
                                GroupPrekeyBundle::from_bytes(&prekey)
                                    .map_err(|error| format!("decode offered pre-key: {error}"))
                                    .and_then(|bundle| {
                                        bundle.personae_root().map_err(|error| {
                                            format!("verify offered pre-key: {error}")
                                        })
                                    })
                                    .and_then(|joiner_root| {
                                        admit_member(
                                            &open.moot,
                                            &binding,
                                            identity.as_ref(),
                                            joiner_root,
                                            access,
                                            now_ms,
                                            expires_at_ms,
                                            open.offers_projection(),
                                        )
                                    })
                                    .and_then(|authored| {
                                        let projection = authored.projection;
                                        // Retention alone leaves the admission
                                        // waiting on the next reconciliation
                                        // round; a connected peer hears it only
                                        // here. A founder with no lanes open
                                        // just retains it, as before.
                                        if let Some(lanes) = &open.lanes {
                                            if let Some(operation) = authored.membership {
                                                lanes.publish_membership(operation)?;
                                            }
                                            if let Some(operation) = authored.delegation {
                                                lanes.publish_delegation(operation)?;
                                            }
                                        }
                                        let (invite, added) = author_invitation_with(
                                            &open.moot,
                                            &mut open.group,
                                            &directory,
                                            &binding,
                                            identity.as_ref(),
                                            &prekey,
                                            now_ms.saturating_add(INVITE_LIFETIME_MS),
                                            rendezvous,
                                            projection,
                                        )?;
                                        if let Some(lanes) = &open.lanes {
                                            lanes.publish_group_key(added)?;
                                        }
                                        // An add hands the invitee the keys in
                                        // hand and installs no epoch here, so
                                        // this releases nothing today. It is
                                        // the rule that every refresh is
                                        // followed by a re-admission.
                                        open.readmit_parked()?;
                                        Ok(invite)
                                    })
                                    .map(Box::new)
                            },
                        };
                        out.emit(Update::PlaceInvited {
                            session,
                            generation,
                            result,
                        });
                    },
                    PlaceWorkerCommand::Reconnect {
                        contact,
                        session,
                        generation,
                        directory,
                        binding,
                    } => {
                        if generation < lifecycle_generation {
                            out.emit(Update::PlaceOpened {
                                session,
                                generation,
                                result: Err("reconnect generation is stale".into()),
                            });
                            continue;
                        }
                        if generation == lifecycle_generation {
                            let result = if live_scope == Some((session, generation)) {
                                live.as_ref().map_or_else(
                                    || Err("reconnect has no open place".into()),
                                    |open| place_snapshot(open, open.binding.moot.0, &settings),
                                )
                            } else {
                                Err("reconnect generation was already released".into())
                            };
                            out.emit(Update::PlaceOpened {
                                session,
                                generation,
                                result,
                            });
                            continue;
                        }
                        lifecycle_generation = generation;
                        // This is the one close followed by a reopen of the
                        // same stores, so it is the one that pays for waiting.
                        if let Some(lanes) = live.as_mut().and_then(|open| open.lanes.as_mut()) {
                            lanes.leave_and_wait();
                        }
                        live = None;
                        live_scope = None;
                        let reconnected =
                            reopen_cached_place(&directory, &binding, identity.as_ref(), &settings)
                                .and_then(|(mut opened, _cached)| {
                                    let local_root = identity.master_public_key().to_bytes();
                                    let membership = pollster::block_on(opened.moot.snapshot())
                                        .map_err(|error| {
                                            format!("materialize retained Moot: {error}")
                                        })?;
                                    if !membership
                                        .membership
                                        .members
                                        .iter()
                                        .any(|member| member.member == local_root)
                                    {
                                        return Err(
                                    "this identity is no longer a member in retained place state"
                                        .into(),
                                );
                                    }
                                    let tickets = match &contact {
                                        Some(card) => {
                                            crate::place::rendezvous::card_tickets(&binding, card)?
                                        },
                                        None => crate::place::rendezvous::load_rendezvous(
                                            &directory,
                                            &binding,
                                            settings.authority_clock.now_ms(),
                                        )?,
                                    };
                                    opened.lanes = Some(crate::place::lanes::join_live(
                                        &opened,
                                        &binding,
                                        identity.as_ref(),
                                        &tickets,
                                        settings.projection_setup(&binding, identity.as_ref()),
                                        Some((out.clone(), session, generation)),
                                    )?);
                                    // A card carries contact metadata, never admission. Persist
                                    // only after retained membership and transport parsing pass.
                                    if let Some(card) = &contact {
                                        crate::place::rendezvous::save_card_rendezvous(
                                            &directory,
                                            &binding,
                                            card,
                                            settings.authority_clock.now_ms(),
                                        )?;
                                    }
                                    place_snapshot(&opened, binding.moot.0, &settings)
                                        .map(|snapshot| (opened, snapshot))
                                });
                        match reconnected {
                            Ok((opened, snapshot)) => {
                                live_scope = Some((session, generation));
                                live = Some(opened);
                                out.emit(Update::PlaceOpened {
                                    session,
                                    generation,
                                    result: Ok(snapshot),
                                });
                            },
                            Err(error) => out.emit(Update::PlaceOpened {
                                session,
                                generation,
                                result: Err(error),
                            }),
                        }
                    },
                    PlaceWorkerCommand::VisitDocument {
                        session,
                        generation,
                        directory,
                        holder_root,
                        path,
                        request,
                    } => {
                        let result = match &live {
                            _ if live_scope != Some((session, generation)) => {
                                Err("a visit belongs to a departed place generation".to_string())
                            },
                            Some(open) => visit_dial(
                                open,
                                &directory,
                                identity.as_ref(),
                                holder_root,
                                &settings,
                            ),
                            None => Err(
                                "a place-held document can only be visited from an open place"
                                    .to_string(),
                            ),
                        };
                        out.emit(Update::PlaceDocumentVisit {
                            session,
                            generation,
                            request,
                            holder_root,
                            path,
                            result: result.map(Box::new),
                        });
                    },
                    PlaceWorkerCommand::Resync {
                        session,
                        generation,
                    } => {
                        // Only meaningful with an open place; a resync of
                        // nothing answers with the error rather than silence.
                        let result = match &mut live {
                            Some(_) if live_scope != Some((session, generation)) => {
                                Err("resync belongs to a departed place generation".to_string())
                            },
                            // A nudge may carry group-key frames: apply them
                            // and re-admit what they unlock before re-folding,
                            // so chat reads the new epochs.
                            Some(open) => {
                                open.follow_group_keys(identity.as_ref()).and_then(|()| {
                                    place_snapshot(open, open.binding.moot.0, &settings)
                                })
                            },
                            None => Err("no open place to resync".to_string()),
                        };
                        out.emit(Update::PlaceOpened {
                            session,
                            generation,
                            result,
                        });
                    },
                    PlaceWorkerCommand::SetCollection {
                        session,
                        generation,
                        selection,
                    } => {
                        let result = match &mut live {
                            Some(_) if live_scope != Some((session, generation)) => Err(
                                "collection selection belongs to a departed place generation"
                                    .to_string(),
                            ),
                            Some(open) => {
                                let previous =
                                    std::mem::replace(&mut open.collection_selection, selection);
                                let result = place_snapshot(open, open.binding.moot.0, &settings)
                                    .and_then(|snapshot| {
                                        crate::session::save_place_collection(
                                            &open.directory,
                                            open.collection_selection.as_ref(),
                                        )?;
                                        Ok(snapshot)
                                    });
                                if result.is_err() {
                                    open.collection_selection = previous;
                                }
                                result
                            },
                            None => Err("no open place to select a collection in".to_string()),
                        };
                        out.emit(Update::PlaceCollectionSet {
                            session,
                            generation,
                            result,
                        });
                    },
                    PlaceWorkerCommand::Author {
                        session,
                        generation,
                        request,
                        command,
                    } => {
                        let result = match &mut live {
                            Some(open) => {
                                let binding = open.binding.clone();
                                author_into_place(
                                    open,
                                    &binding,
                                    identity.as_ref(),
                                    &command,
                                    &settings,
                                )
                                .and_then(|()| place_snapshot(open, binding.moot.0, &settings))
                            },
                            None => Err("no open place to author into".to_string()),
                        };
                        out.emit(Update::PlaceCommandDone {
                            session,
                            generation,
                            request,
                            result,
                        });
                    },
                    PlaceWorkerCommand::Release(ack) => {
                        live = None;
                        live_scope = None;
                        let _ = ack.send(());
                    },
                    PlaceWorkerCommand::AdoptIdentity(_) => {},
                    PlaceWorkerCommand::RetireIdentity(_) => {
                        unreachable!("place identity retirement is handled before command routing")
                    },
                    #[cfg(test)]
                    PlaceWorkerCommand::Abandon(ack) => {
                        if let Some(opened) = live.as_mut()
                            && let Some(lanes) = opened.lanes.take()
                        {
                            lanes.abandon();
                        }
                        let _ = ack.send(());
                    },
                    #[cfg(test)]
                    PlaceWorkerCommand::Freeze(hold, ack) => {
                        if let Some(opened) = live.as_ref()
                            && let Some(lanes) = opened.lanes.as_ref()
                        {
                            lanes.freeze(hold);
                        }
                        let _ = ack.send(());
                    },
                }
            }
            drop(live);
        },
    )
}
