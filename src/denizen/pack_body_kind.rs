// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PackBody {
    /// The participant kind this body resides as.
    pub fn kind(&self) -> pandect::DenizenKind {
        match self {
            PackBody::Scenario(_) => pandect::DenizenKind::Scenario,
            PackBody::Component(_) => pandect::DenizenKind::Pack,
        }
    }

    /// How the review names the runnable. Short on purpose: the whole ask
    /// has to fit one palette row without clipping, and what matters in it
    /// is the RINGS.
    pub fn noun(&self) -> &'static str {
        match self {
            PackBody::Scenario(_) => "lua",
            PackBody::Component(_) => "wasm",
        }
    }
}
