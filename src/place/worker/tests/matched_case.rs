// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A joiner directory with a prepared identity, plus an invitation whose
/// welcome is genuinely addressed to it.
pub(super) fn matched_case(
    root: &Path,
    name: &str,
    binding: &PlaceBindingV1,
    founder: &InMemoryProvider,
    joiner: &RootIdentity,
) -> (PathBuf, PlaceInviteV1) {
    let directory = root.join(name);
    let published = prepare_group_identity(&directory, joiner, binding.moot.0).unwrap();
    let invite = real_invitation(
        &root.join(format!("{name}-founder")),
        binding,
        founder,
        published,
        joiner.master_public_key().to_bytes(),
    );
    (directory, invite)
}
