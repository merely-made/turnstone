// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// How long a VISITED document may go without an answer from its holder before
/// the surface is told the holder is out of reach.
///
/// A holder that closes says so, and the carrier read fails at once. A holder
/// whose process dies says nothing: the carrier read stays blocked until QUIC
/// abandons the connection on its own, which is the transport's idle timeout
/// (tens of seconds) and not a bound this product chose. This is that bound,
/// measured from the hub's last completed round trip.
pub(crate) const VISIT_LIVENESS_BUDGET: Duration = Duration::from_secs(6);
