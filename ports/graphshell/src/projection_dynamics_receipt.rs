// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! F195's native/wasm identity probe, using the actual practice compiler and
//! occurrence adapter. Floating-point words are recorded without rounding.

use crate::projection_compile::{ProjectionDataset, default_definition, practice_compiler};
use mere::canvas::{
    PhysicsLaw, dynamics_recipe::preset_slot, projection_dynamics::ProjectionDynamics,
};

pub fn catalog_receipt(bound: u32) -> Result<String, String> {
    let data: ProjectionDataset =
        serde_json::from_str(include_str!("../web/fixtures/woodshed-stage.json"))
            .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for law in PhysicsLaw::ALL {
        let mut definition = default_definition(&data);
        let slot = preset_slot(law.id())?;
        definition.dynamics = Some(slot.clone());
        let compiled = practice_compiler()
            .compile(&definition, &data)
            .map_err(|e| format!("{e:?}"))?;
        let mut preview =
            ProjectionDynamics::new(&definition, &compiled, &slot).map_err(|e| e.to_string())?;
        let report = preview.snapshot(bound).map_err(|e| e.to_string())?;
        let positions: Vec<_> = report
            .positions
            .iter()
            .map(|(key, p)| (key.index(), p.x.to_bits(), p.y.to_bits()))
            .collect();
        rows.push(serde_json::json!({"law":law.id(), "bound":bound, "steps":report.steps, "end":format!("{:?}",report.end), "positions":positions}));
    }
    serde_json::to_string(&rows).map_err(|e| e.to_string())
}
