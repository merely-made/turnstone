// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ConformanceDriver {
    pub(super) const FOUNDER_SEED: u8 = 0xd0;

    pub(super) const MEMBER_SEED: u8 = 0xd1;

    /// Long enough to survive join, leave, rejoin, restart and reconnect
    /// before the walk reaches `expire`, with real margin under `-j 4`
    /// contention from the rest of `place::`'s suite (a bare 15 s was
    /// observed to expire mid-walk, at `rejoin`, under full-suite load);
    /// short enough that waiting it out stays under a minute.
    pub(super) const GRANT_LIFETIME_MS: u64 = 45_000;

    pub(super) fn open(root: &Path) -> Self {
        let host = root.join("host");
        let guest = root.join("guest");
        std::fs::create_dir_all(&host).unwrap();
        std::fs::create_dir_all(&guest).unwrap();
        let clock = crate::place::worker::PlaceWorkerSettings {
            authority_clock: crate::place::worker::AuthorityClock::SystemTime,
            ..settings()
        };
        // This lifecycle walk checks projection-grant expiry as well as
        // Commons authority. Offer the service whose grant it inspects.
        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let host_settings = crate::place::worker::PlaceWorkerSettings {
            knot_root: Some(vault),
            ..clock.clone()
        };
        let identity = RootIdentity::from_seed([Self::FOUNDER_SEED; 32]);
        let founder = spawn_place_worker(Arc::new(|| {}), Arc::new(identity), host_settings);
        let founder_session = SessionId::new();
        founder.0.command(PlaceWorkerCommand::Found {
            session: founder_session,
            generation: 1,
            directory: host.clone(),
            name: "Coop contract".into(),
        });
        let binding = loop {
            match founder.1.recv_timeout(Duration::from_secs(60)) {
                Ok(Update::PlaceFounded {
                    result: Ok((binding, _)),
                    ..
                }) => break binding,
                Ok(Update::PlaceFounded {
                    result: Err(error), ..
                }) => {
                    panic!("founding refused: {error}")
                },
                Ok(_) => continue,
                Err(error) => panic!("founding never answered: {error}"),
            }
        };
        let member_identity = InMemoryProvider::from_seed([Self::MEMBER_SEED; 32]);
        let member_prekey =
            prepare_group_identity(&guest, &member_identity, binding.moot.0).unwrap();
        Self {
            founder,
            founder_session,
            binding,
            host,
            guest,
            member_prekey,
            clock,
            member: None,
            invite: None,
            grant_expires_at_ms: None,
            snapshot: None,
            left: None,
            now_ms: crate::denizen::now_ms(),
            next_request: 1,
        }
    }

    pub(super) fn take_request(&mut self) -> u64 {
        let request = self.next_request;
        self.next_request += 1;
        request
    }

    /// Wait for one `Invite` answer, without the panic `expect_invited`
    /// uses: `invite_wrong_target` needs the refusal, not a crash.
    pub(super) fn await_invited(&self) -> Result<Box<crate::place::invite::PlaceInviteV1>, String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if Instant::now() > deadline {
                return Err("invite never answered".into());
            }
            match self.founder.1.recv_timeout(Duration::from_secs(5)) {
                Ok(Update::PlaceInvited { result, .. }) => return result,
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
    }

    /// Release any live member worker, keeping its retained stores.
    pub(super) fn release_member(&mut self) {
        if let Some((worker, _, _)) = self.member.take() {
            release(&worker);
        }
    }

    pub(super) fn member_identity() -> InMemoryProvider {
        InMemoryProvider::from_seed([Self::MEMBER_SEED; 32])
    }
}
