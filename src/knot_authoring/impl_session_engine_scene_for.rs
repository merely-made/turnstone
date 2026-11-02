// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SessionEngine<Scene> for KnotAuthoringEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn spawn(
        &self,
        request: &SessionSpawnRequest,
    ) -> Result<Box<dyn DocumentSession<Scene>>, SessionError> {
        if !is_knot_address(&request.address) {
            return Err(SessionError::Unsupported(format!(
                "{} is not a Knot document",
                request.address
            )));
        }
        // A place-held address decides whose endpoint answers it. This
        // profile's own root is the local vault, exactly as before; anyone
        // else's is the hub the shell dialed for that holder, and no hub
        // means the visit was never opened rather than an empty document.
        let (hub, visiting) = match parse_place_held_address(&request.address) {
            Some((root, _)) => {
                let visits = self.visits.lock().map_err(|_| {
                    SessionError::SpawnFailed("the Knot visit registry is poisoned".into())
                })?;
                if visits.is_local(&root) {
                    let hub = self.hub.clone().ok_or_else(|| {
                        SessionError::SpawnFailed(Self::no_local_vault(&request.address))
                    })?;
                    (hub, false)
                } else {
                    let hub = visits.holder(&root).ok_or_else(|| {
                        SessionError::SpawnFailed(format!(
                            "no visit is open to the mere holding {}",
                            request.address
                        ))
                    })?;
                    (hub, true)
                }
            },
            // A plain `file://` or `knot://vault/` address is this profile's
            // own document, and without a vault there is no such thing.
            None => {
                let hub = self.hub.clone().ok_or_else(|| {
                    SessionError::SpawnFailed(Self::no_local_vault(&request.address))
                })?;
                (hub, false)
            },
        };
        let opened = hub
            .open(&request.address)
            .map_err(SessionError::SpawnFailed)?;
        Ok(Box::new(KnotDocumentSession::new(
            hub,
            opened,
            request.viewport,
            self.auto_resolve,
            self.auto_run,
            visiting,
        )))
    }
}
