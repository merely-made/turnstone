// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

fn main() {
    let bound = std::env::args()
        .nth(1)
        .expect("explicit step bound")
        .parse()
        .expect("u32 step bound");
    println!(
        "{}",
        graphshell::projection_dynamics_receipt::catalog_receipt(bound)
            .expect("bound practice dynamics")
    );
}
