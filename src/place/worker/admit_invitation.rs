// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Admit one invitation, or leave nothing behind.
///
/// This is the gate the place-port plan's five checks describe. The ordering is
/// the point: every artifact is verified by its own domain first, and the
/// sealed group session is written only once all of them have answered. A
/// refused invitation must not leave a Gemot store, a sealed secret, or a
/// binding a later open could mistake for an admitted place.
///
/// Structural envelope validation is not admission and does not appear here
/// beyond its first line. Holding or forwarding an envelope grants nothing.
pub fn admit_invitation(
    directory: &Path,
    invite: &PlaceInviteV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
) -> Result<AdmittedPlace, String> {
    let store_existed = place_store_dir(directory).exists();
    match admit_inner(directory, invite, identity, settings) {
        Ok(admitted) => Ok(admitted),
        Err(error) => {
            // Refusal removes only what this attempt created. A half-imported
            // Gemot store would let a later open present an unadmitted place as
            // a retained one.
            //
            // The sealed secrets are deliberately not touched. They hold this
            // profile's own group identity, whose pre-key was published before
            // any invitation arrived; deleting it on a refused invitation would
            // let any stranger destroy the key material a pending welcome is
            // already addressed to.
            if !store_existed {
                let _ = std::fs::remove_dir_all(place_store_dir(directory));
            }
            Err(error)
        },
    }
}
