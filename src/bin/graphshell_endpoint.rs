// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

fn main() {
    let mut endpoint = turnstone::remote_projection::TurnstoneEndpoint::fixture()
        .expect("Turnstone projection fixture is valid");
    graphshell_stdio::serve_basic(
        &mut endpoint,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )
    .expect("Turnstone Graphshell endpoint failed");
}
