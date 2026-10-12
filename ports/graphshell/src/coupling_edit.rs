// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One exact coupling's authored strength draft, independent of selection.
use mere::kernel::graph::{Coupling, CouplingId, Field, Graph};

#[derive(Clone, Debug)]
pub struct StrengthDraft {
    pub(crate) graph_id: uuid::Uuid,
    pub baseline: Coupling,
    owner: Field,
    pub text: String,
    undo: Vec<String>,
    redo: Vec<String>,
}

impl StrengthDraft {
    pub fn begin(graph: &Graph, graph_id: uuid::Uuid, id: CouplingId) -> Result<Self, String> {
        let baseline = graph
            .couplings()
            .find(|c| c.id == id)
            .ok_or("The exact coupling is no longer held")?
            .clone();
        let owner = graph
            .field(baseline.field)
            .ok_or("The coupling owner is not held")?
            .clone();
        if !owner.is_active() || !baseline.strength.is_finite() {
            return Err(
                "Strength editing requires an active owner and finite held strength".into(),
            );
        }
        Ok(Self {
            graph_id,
            text: baseline.strength.to_string(),
            baseline,
            owner,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub fn set_text(&mut self, text: String) -> Result<(), String> {
        if text.len() > 128 {
            return Err("Strength text exceeds 128 bytes".into());
        }
        if text != self.text {
            self.undo.push(std::mem::replace(&mut self.text, text));
            if self.undo.len() > 64 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        Ok(())
    }

    pub fn undo(&mut self, redo: bool) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(text) = from.pop() {
            to.push(std::mem::replace(&mut self.text, text));
        }
    }

    pub fn candidate(&self, graph: &Graph) -> Result<Coupling, String> {
        if self.text.len() > 128 {
            return Err("Strength text exceeds 128 bytes; draft retained".into());
        }
        let held = graph
            .couplings()
            .find(|c| c.id == self.baseline.id)
            .ok_or("The exact coupling was withdrawn; draft retained")?;
        if held != &self.baseline || graph.field(held.field) != Some(&self.owner) {
            return Err("The coupling or its owner changed; discard and inspect again, or keep the draft for comparison".into());
        }
        let strength: f32 = self
            .text
            .trim()
            .parse()
            .map_err(|_| "Enter a finite strength number")?;
        if !strength.is_finite() {
            return Err("Strength must be finite; draft retained".into());
        }
        let mut candidate = held.clone();
        candidate.strength = strength;
        Ok(candidate)
    }
}

pub(crate) fn channel(id: CouplingId) -> String {
    format!("graphshell.coupling/{}", id.as_uuid())
}

#[cfg(test)]
#[path = "coupling_edit_tests.rs"]
mod tests;
