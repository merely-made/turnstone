// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The capabilities an install confers: the participant's own world, the read
/// face, and one per REVIEWED ring. No blanket grant — an unnamed ring is an
/// ungranted ring.
pub fn install_caps(rings: &[crate::ring::Ring], watched: Option<&Cap>) -> Vec<(Cap, Mode)> {
    let mut caps = vec![(world_cap(), Mode::Write), (read_cap(), Mode::Write)];
    // The watched region is granted as a READ scope, which is what makes the
    // containment law (watch inside read inside grant) hold by construction
    // rather than by hope: the install that promised the wake also granted
    // the reading it implies, and `WatchTable::register` checks exactly that.
    if let Some(cap) = watched {
        caps.push((cap.clone(), Mode::Read));
    }
    caps.extend(
        rings
            .iter()
            .filter_map(|ring| ring.cap())
            .map(|cap| (cap, Mode::Write)),
    );
    caps
}
