// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Held field/coupling reads and authored-region discovery, without evaluation.
//! A geometry hit, a selector match and a measured influence are different facts.
use crate::{access_inspection::AccessSummary, forme_workspace::FormeWorkspace};
use mere::kernel::graph::{
    Coupling, CouplingId, CouplingResponse, CoverageNote, Field, FieldDefinition, FieldExtent,
    FieldId, Graph,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldTarget {
    Numen(FieldId),
    Forme(Uuid),
}

impl FieldTarget {
    /// Inspection order only; it confers no execution or placement precedence.
    pub fn sort_key(self) -> (u8, Uuid) {
        match self {
            Self::Numen(id) => (0, id.as_uuid()),
            Self::Forme(id) => (1, id),
        }
    }
}

/// Qualification of the current Canvas rebuild contract, not a force result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForceEligibility {
    Recognized,
    RetiredField,
    MissingField,
    OpenResponse,
    DefinitionMismatch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CouplingInspection {
    pub coupling: Coupling,
    pub owner: Option<Field>,
    /// Current held selector matches; extent is not used as a membership filter
    /// by the Canvas force bridge. No action or field AST is evaluated here.
    pub selector_members: Vec<AccessSummary>,
    pub rebuild_eligibility: ForceEligibility,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldInspection {
    pub field: Field,
    pub couplings: Vec<CouplingInspection>,
    /// Graph-wide known limits: today's coverage contract has no FieldId scope.
    pub coverage: CoverageNote,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormeInspection {
    pub id: Uuid,
    pub graph_id: Uuid,
    pub label: Option<String>,
    pub bounds: [f32; 4],
    pub locked: bool,
    pub visible: bool,
    pub members: Vec<Uuid>,
    pub arrangement: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegionCandidate {
    pub target: FieldTarget,
    pub name: Option<String>,
    pub bounds: [f32; 4],
}

pub fn field_roster(graph: &Graph) -> Vec<Field> {
    let mut fields: Vec<_> = graph.fields().cloned().collect();
    fields.sort_by_key(|field| field.id.as_uuid());
    fields
}

fn coupling_read(graph: &Graph, coupling: &Coupling) -> CouplingInspection {
    let owner = graph.field(coupling.field).cloned();
    let rebuild_eligibility = match &owner {
        None => ForceEligibility::MissingField,
        Some(field) if !field.is_active() => ForceEligibility::RetiredField,
        Some(_) if !coupling.response.is_force() => ForceEligibility::OpenResponse,
        Some(field) => {
            let vector = matches!(
                coupling.response,
                CouplingResponse::AlignVelocity | CouplingResponse::FlowAdvect
            );
            if vector == matches!(field.definition, FieldDefinition::Vector(_)) {
                ForceEligibility::Recognized
            } else {
                ForceEligibility::DefinitionMismatch
            }
        },
    };
    let mut selector_members: Vec<_> = graph
        .nodes_matching(&coupling.selector)
        .filter_map(|key| graph.get_node(key))
        .map(|node| AccessSummary {
            member: node.id,
            title: node.title.clone(),
            address: node.url().into(),
        })
        .collect();
    selector_members.sort_by_key(|access| access.member);
    CouplingInspection {
        coupling: coupling.clone(),
        owner,
        selector_members,
        rebuild_eligibility,
    }
}

pub fn inspect_coupling(graph: &Graph, id: CouplingId) -> Result<CouplingInspection, String> {
    let coupling = graph
        .couplings()
        .find(|coupling| coupling.id == id)
        .ok_or_else(|| format!("Coupling {} is no longer held here", id.as_uuid()))?;
    Ok(coupling_read(graph, coupling))
}

pub fn inspect_field(graph: &Graph, id: FieldId) -> Result<FieldInspection, String> {
    let field = graph
        .field(id)
        .ok_or_else(|| format!("Field {} is no longer held here", id.as_uuid()))?
        .clone();
    let mut couplings: Vec<_> = graph
        .couplings_for_field(id)
        .map(|coupling| coupling_read(graph, coupling))
        .collect();
    couplings.sort_by_key(|inspected| inspected.coupling.id.as_uuid());
    Ok(FieldInspection {
        field,
        couplings,
        coverage: graph.coverage_note(),
    })
}

pub fn inspect_forme(
    workspace: &FormeWorkspace,
    expected: Uuid,
) -> Result<FormeInspection, String> {
    if workspace.document.id.as_uuid() != expected {
        return Err(format!("Forme {expected} is no longer presented here"));
    }
    Ok(FormeInspection {
        id: expected,
        graph_id: workspace.document.graph_id,
        label: workspace.document.label.clone(),
        bounds: workspace.bounds,
        locked: workspace.locked,
        visible: workspace.visible,
        members: workspace.members(),
        arrangement: serde_json::to_value(&workspace.document.arrangement)
            .map_err(|error| format!("Cannot disclose this forme's arrangement: {error}"))?,
    })
}

/// Discover active, visible rectangular boundaries at one world point using
/// the existing inclusive Region bounds test. This is boundary discovery only,
/// not the pending spatial membership policy. Other extents remain in the roster.
/// A Forme retains its arrangement owner; no Numen field is manufactured for it.
pub fn region_candidates(
    graph: &Graph,
    point: [f32; 2],
    visible: impl Fn(FieldId) -> bool,
    forme: Option<&FormeInspection>,
) -> Result<Vec<RegionCandidate>, String> {
    if !point.iter().all(|n| n.is_finite()) {
        return Err("Region discovery needs a finite world point".into());
    }
    let contains = |bounds: [f32; 4]| {
        bounds.iter().all(|n| n.is_finite())
            && point[0] >= bounds[0]
            && point[0] <= bounds[2]
            && point[1] >= bounds[1]
            && point[1] <= bounds[3]
    };
    let mut candidates: Vec<_> = graph
        .fields()
        .filter_map(|field| {
            if !field.is_active() || !visible(field.id) {
                return None;
            }
            let FieldExtent::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } = field.extent
            else {
                return None;
            };
            let bounds = [min_x, min_y, max_x, max_y];
            contains(bounds).then(|| RegionCandidate {
                target: FieldTarget::Numen(field.id),
                name: field.name.clone(),
                bounds,
            })
        })
        .collect();
    if let Some(forme) = forme {
        if forme.visible && !forme.members.is_empty() && contains(forme.bounds) {
            candidates.push(RegionCandidate {
                target: FieldTarget::Forme(forme.id),
                name: forme.label.clone(),
                bounds: forme.bounds,
            });
        }
    }
    candidates.sort_by_key(|candidate| candidate.target.sort_key());
    Ok(candidates)
}

#[cfg(test)]
#[path = "field_inspection/tests.rs"]
mod tests;
