// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What Graphshell declares on every hello to Djinn (Mere's capability door
//! D24). The owner's grant decides what it receives; Djinn names nothing.

use djinn::door::declaration::{
    AppDeclaration, CustodyOperation, CustodyUse, NamePattern, SaltPattern, SaltUse,
};

/// The label Graphshell is admitted under.
pub const APP: &str = "graphshell";

/// Graphshell's routes and custody: the reservoir, identity, each mere and
/// its co-op route; its per-session endpoint keys and personal graph writer
/// keys, released; retained content sealed by custody; and the guest
/// invitations and installs Djinn signs for it, never released.
pub fn declaration() -> AppDeclaration {
    use CustodyOperation as Op;
    AppDeclaration {
        routes: vec![
            NamePattern::exact("custody"),
            NamePattern::exact("identity"),
            NamePattern::exact("reservoir"),
            NamePattern::prefix("mere/"),
            NamePattern::prefix("ambient/mere/"),
            NamePattern::prefix("coop/"),
        ],
        custody: CustodyUse {
            operations: [
                Op::Status,
                Op::Roster,
                Op::WatchLock,
                Op::Release,
                Op::Attest,
                Op::Sign,
                Op::Payload,
            ]
            .into(),
            salts: vec![
                SaltUse::persona(SaltPattern::Prefix("graphshell/endpoint-session/".into())),
                SaltUse::persona(SaltPattern::Prefix("graphshell.personal-graph.writer.v1/".into())),
            ],
            acts: vec![
                SaltUse::persona(SaltPattern::DelegationScope("mere.graphshell".into())),
                SaltUse::persona(SaltPattern::DelegationScope("mere.denizen".into())),
            ],
            ..CustodyUse::default()
        },
        ..AppDeclaration::default()
    }
}
