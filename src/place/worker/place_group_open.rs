// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlaceGroup {
    pub(super) fn open(
        directory: &Path,
        identity: &dyn IdentityProvider,
        moot: [u8; 32],
    ) -> Result<Self, String> {
        let session = load_group_session(directory, identity, moot)?;
        if session.group() != GroupSessionId(moot) {
            return Err("sealed group session addresses another Moot".to_string());
        }
        if session.personae_root() != identity.master_public_key().to_bytes() {
            return Err("sealed group session belongs to another Personae root".to_string());
        }
        let keys = GroupKeys::from_bytes(&keyring_state(&session)?)
            .map_err(|error| format!("decode group data epochs: {error}"))?;
        let backend = RedbBackend::open(place_store_dir(directory).join(GROUP_KEY_STORE))
            .map_err(|error| format!("open group-key lane: {error}"))?;
        let lane = GroupKeyLane::for_identity(backend, GroupSessionId(moot), identity)
            .map_err(|error| format!("bind group-key writer: {error}"))?;
        Ok(Self {
            session,
            keys,
            lane,
            pending: 0,
            refused: 0,
        })
    }

    /// A copy to change, so a failed change leaves the live session as it was.
    pub(super) fn copy(&self) -> Result<GroupSession, String> {
        self.session
            .to_bytes()
            .and_then(|bytes| GroupSession::from_bytes(&bytes))
            .map_err(|error| format!("copy group session: {error}"))
    }

    pub(super) fn refresh(&self) -> Result<(), String> {
        self.keys
            .replace_from_bytes(&keyring_state(&self.session)?)
            .map_err(|error| format!("refresh group keys: {error}"))
    }

    /// Adopt a locally changed session: persist it, refresh the handle, then
    /// author its dispatches on the lane. Persisting first is the lane's rule:
    /// a frame whose transition the author lost would fork its sequence.
    pub(super) fn commit(
        &mut self,
        directory: &Path,
        identity: &dyn IdentityProvider,
        next: GroupSession,
        dispatches: &[GroupSessionDispatch],
    ) -> Result<Vec<stickleback::Operation<GroupKeyExt>>, String> {
        save_group_session(directory, identity, &next)?;
        self.session = next;
        self.refresh()?;
        dispatches
            .iter()
            .map(|dispatch| {
                pollster::block_on(self.lane.author_dispatch(dispatch))
                    .map_err(|error| format!("author group-key frame: {error}"))
            })
            .collect()
    }

    /// Apply every ready frame the lane holds, persisting the sealed session
    /// before frames settle, then refresh the handle.
    pub(crate) fn drain(
        &mut self,
        directory: &Path,
        identity: &dyn IdentityProvider,
    ) -> Result<(), String> {
        let drain = pollster::block_on(self.lane.drain_ready(&mut self.session, |session| {
            save_group_session(directory, identity, session)
        }))
        .map_err(|error| format!("drain group-key lane: {error}"))?;
        for frame in &drain.refused {
            tracing::warn!(reason = %frame.reason, "group-key frame refused");
        }
        self.pending = drain.pending.len();
        self.refused += drain.refused.len();
        self.refresh()
    }
}
