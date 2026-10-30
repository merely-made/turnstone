// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl HolderDial {
    /// Open the stream and wrap it in the carrier. Blocking, on the lane
    /// runtime, from whatever thread calls it.
    pub(crate) fn open(self) -> Result<(PlaceProjectionCarrier, PeerID), String> {
        let limits = HandshakeLimits::default();
        let transport = Arc::clone(&self.transport);
        let hello = self.hello;
        let ticket = self.ticket;
        let (stream, peer) = self.handle.block_on(async move {
            // Idempotent for a ticket already in the address book, and the
            // only way to learn the peer id a bare ticket names.
            let peer = transport
                .add_peer_ticket(&ticket)
                .await
                .map_err(|error| format!("import holder ticket: {error}"))?;
            let stream = dial_projection_session(&*transport, peer, &hello, &limits)
                .await
                .map_err(|error| format!("dial the holder: {error}"))?
                .map_err(|reason| format!("the holder refused this session: {reason:?}"))?;
            Ok::<_, String>((stream, peer))
        })?;
        let carrier = NetworkCarrier::over(stream, CarrierRuntime::borrowed(self.handle));
        Ok((carrier, peer))
    }
}
