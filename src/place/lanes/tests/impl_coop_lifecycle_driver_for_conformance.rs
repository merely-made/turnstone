// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl coop::LifecycleDriver for ConformanceDriver {
    fn invite_wrong_target(&mut self) -> Result<(), String> {
        // Unauthorized grant, refused by name before anything is
        // authored: a reader invitation carrying a lifetime. Reuses the
        // same offered pre-key the real invite uses below, since the
        // refusal here is about the requested grant, not the offer.
        self.founder.0.command(PlaceWorkerCommand::Invite {
            session: self.founder_session,
            generation: 1,
            directory: self.host.clone(),
            prekey: self.member_prekey.clone(),
            access: crate::place::PlaceInviteAccess::Reader,
            lifetime_ms: Some(1),
        });
        match self.await_invited() {
            // Admitted: the driver reports success, and `conform` is the
            // one that flags an admission it expected refused.
            Ok(_) => Ok(()),
            Err(reason) => Err(reason),
        }
    }

    fn invite(&mut self) -> Result<(), String> {
        self.founder.0.command(PlaceWorkerCommand::Invite {
            session: self.founder_session,
            generation: 1,
            directory: self.host.clone(),
            prekey: self.member_prekey.clone(),
            access: crate::place::PlaceInviteAccess::Writer,
            lifetime_ms: Some(Self::GRANT_LIFETIME_MS),
        });
        let invite = self.await_invited()?;
        let grant = crate::place::projection_host::decode_grant(
            invite
                .projection_grant
                .as_ref()
                .ok_or_else(|| "a writer invitation carries its grant".to_string())?
                .verified_bytes("projection grant")
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        self.grant_expires_at_ms = grant.certificate.expires_at_ms;
        self.invite = Some(invite);
        Ok(())
    }

    fn join(&mut self) -> Result<(), String> {
        let invite = self
            .invite
            .clone()
            .ok_or_else(|| "invite must run before join".to_string())?;
        self.release_member();
        let identity = Arc::new(RootIdentity::Local(Self::member_identity()));
        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity, self.clock.clone());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Join {
            session,
            generation: 1,
            directory: self.guest.clone(),
            invite,
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        let outcome = loop {
            if Instant::now() > deadline {
                break Err("join never answered".to_string());
            }
            match updates.recv_timeout(Duration::from_secs(5)) {
                Ok(Update::PlaceJoined { result, .. }) => {
                    break result.map(|(_, snapshot)| snapshot);
                },
                Ok(_) => continue,
                Err(_) => continue,
            }
        };
        match outcome {
            Ok(snapshot) => {
                self.now_ms = crate::denizen::now_ms();
                self.snapshot = Some(snapshot);
                self.left = None;
                self.member = Some((worker, updates, session));
                Ok(())
            },
            Err(error) => {
                release(&worker);
                Err(error)
            },
        }
    }

    fn report(&mut self) -> coop::LifecycleReport {
        if let Some(retained) = self.left {
            return crate::place::contract::lifecycle_report(
                &crate::place::PlaceState::Left {
                    binding: self.binding.clone(),
                    retained,
                },
                self.now_ms,
            );
        }
        match &self.snapshot {
            Some(snapshot) => {
                crate::place::contract::lifecycle_report_from_snapshot(snapshot, self.now_ms)
            },
            None => crate::place::contract::lifecycle_report(
                &crate::place::PlaceState::Personal,
                self.now_ms,
            ),
        }
    }

    /// Detach: release the live worker and forget the retained view,
    /// leaving the binding, the rendezvous hints and the retained stores
    /// under `guest` all untouched -- `mark_place_left` records the
    /// retained summary beside `place.json` rather than deleting
    /// anything, the same split `finish_leave_place` and
    /// `Effect::LeavePlace`'s shell handler now draw. The retained
    /// summary is read from the snapshot in hand, exactly as
    /// `finish_leave_place` reads it from `PlaceState::Offline`.
    fn leave(&mut self) -> Result<(), String> {
        let Some(snapshot) = self.snapshot.take() else {
            return Err("not joined".into());
        };
        self.release_member();
        let retained = crate::place::PlaceLeftSummary {
            graph_nodes: snapshot.graph.nodes,
            chat_messages: snapshot.chat.messages,
            members: snapshot.moot.members,
        };
        crate::session::mark_place_left(&self.guest, &retained)
            .map_err(|error| error.to_string())?;
        self.left = Some(retained);
        self.now_ms = crate::denizen::now_ms();
        Ok(())
    }

    /// Clear the left mark -- `rejoin_place`'s first step -- then
    /// reconnect the same way a still-live place does: `leave` kept the
    /// binding, the rendezvous hints and every retained store in place,
    /// so there is nothing left to rebuild.
    fn rejoin(&mut self) -> Result<(), String> {
        crate::session::clear_place_left(&self.guest).map_err(|error| error.to_string())?;
        coop::LifecycleDriver::reconnect(self)
    }

    fn restart(&mut self) -> Result<(), String> {
        self.release_member();
        Ok(())
    }

    fn reconnect(&mut self) -> Result<(), String> {
        self.release_member();
        // `release` acks once the actor stops taking commands, not once
        // its lane tasks have dropped their store clones (see
        // `bypass_after_restart`'s own note on this), so the reopen just
        // below can briefly race a redb handle still closing. Retried
        // the same way `reopen_by_hand` retries a raw `open_cached_place`.
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let identity = Arc::new(RootIdentity::Local(Self::member_identity()));
            let (worker, updates) =
                spawn_place_worker(Arc::new(|| {}), identity, self.clock.clone());
            let session = SessionId::new();
            worker.command(PlaceWorkerCommand::Reconnect {
                contact: None,
                session,
                generation: 1,
                directory: self.guest.clone(),
                binding: self.binding.clone(),
            });
            let answer_deadline = Instant::now() + Duration::from_secs(60);
            let outcome = loop {
                if Instant::now() > answer_deadline {
                    break Err("reconnect never answered".to_string());
                }
                match updates.recv_timeout(Duration::from_secs(5)) {
                    Ok(Update::PlaceOpened { result, .. }) => break result,
                    Ok(_) => continue,
                    Err(_) => continue,
                }
            };
            match outcome {
                Ok(snapshot) => {
                    self.now_ms = crate::denizen::now_ms();
                    self.snapshot = Some(snapshot);
                    self.left = None;
                    self.member = Some((worker, updates, session));
                    return Ok(());
                },
                Err(error) => {
                    release(&worker);
                    let contended =
                        error.contains("already open") || error.contains("Cannot acquire lock");
                    if contended && Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(25));
                        continue;
                    }
                    return Err(error);
                },
            }
        }
    }

    fn expire(&mut self) -> Result<(), String> {
        let expires_at = self
            .grant_expires_at_ms
            .ok_or_else(|| "no grant lifetime recorded".to_string())?;
        let now = crate::denizen::now_ms();
        if now < expires_at {
            std::thread::sleep(Duration::from_millis(expires_at - now + 250));
        }
        let (worker, updates, session) = self
            .member
            .as_ref()
            .ok_or_else(|| "member not live".to_string())?;
        let snapshot = converge_until(
            worker,
            updates,
            *session,
            "K3 conformance: the grant expires",
            |s| matches!(s.standing, crate::place::PlaceStanding::GrantExpired { .. }),
        );
        self.now_ms = crate::denizen::now_ms();
        self.snapshot = Some(snapshot);
        Ok(())
    }

    fn revoke(&mut self) -> Result<(), String> {
        let member_root = Self::member_identity().master_public_key().to_bytes();
        let request = self.take_request();
        self.founder.0.command(PlaceWorkerCommand::Author {
            session: self.founder_session,
            generation: 1,
            request,
            command: PlaceCommand::RevokeMember {
                member: member_root,
            },
        });
        authored(&self.founder.1, request)?;
        let (worker, updates, session) = self
            .member
            .as_ref()
            .ok_or_else(|| "member not live".to_string())?;
        let snapshot = converge_until(
            worker,
            updates,
            *session,
            "K3 conformance: the revocation reaches the member",
            |s| {
                matches!(
                    s.standing,
                    crate::place::PlaceStanding::MembershipRevoked
                        | crate::place::PlaceStanding::GrantRevoked
                )
            },
        );
        self.now_ms = crate::denizen::now_ms();
        self.snapshot = Some(snapshot);
        Ok(())
    }

    /// Deliver an operation this member already holds: release its
    /// worker, reopen its stores by hand -- K4's own bypass -- and
    /// re-`accept` the retained membership removal `revoke` just
    /// converged on. `now_ms` and the retained snapshot are left
    /// exactly as `revoke` left them, so a report taken before and
    /// after compares equal precisely when nothing changed.
    fn replay_duplicate(&mut self) -> Result<(), String> {
        self.release_member();
        let identity = Self::member_identity();
        let open = reopen_by_hand(&self.guest, &self.binding, &identity);
        let membership = pollster::block_on(open.moot.snapshot())
            .map_err(|error| error.to_string())?
            .membership;
        let head = *membership
            .auth_heads
            .first()
            .ok_or_else(|| "no retained membership operation to replay".to_string())?;
        let operation = pollster::block_on(open.moot.membership_store().get(&head.into()))
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "the retained membership operation is missing".to_string())?;
        let accepted = pollster::block_on(open.moot.membership_store().accept(&operation))
            .map_err(|error| error.to_string())?;
        drop(open);
        if accepted {
            return Err("a replayed membership operation was accepted as new".into());
        }
        // No live worker to restore: `conform` walks nothing after
        // `replay_duplicate`, `report` reads `self.snapshot`/`self.left`
        // rather than a live session, and this member is revoked, so a
        // trailing `Reconnect` here would only be refused -- correctly,
        // since a revoked member cannot reconnect.
        Ok(())
    }
}
