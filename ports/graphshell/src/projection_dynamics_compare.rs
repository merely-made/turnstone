// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Arrangement columns and a user-supplied dynamics axis. Scope is an input
//! to this matrix, not silently substituted for its second axis (SE54).

use mere::canvas::projection_dynamics::ProjectionDynamics;
use scenograph::swatch::{
    Axis, AxisKind, AxisScale, Facet, FacetCell, Followed, Scope, Swatch, SwatchMode,
};
use scenograph::{DynamicsSlot, ProjectionDraft};
use scenomise::catalog::{FAMILIES, Family};
use scenomise::facet::{FacetLayout, compose_facet};

use crate::projection_compare::{ComparedCell, Comparison, EDITOR_VIEW, WORKING};
use crate::projection_compile::{ProjectionDataset, practice_compiler};
use crate::projection_editor::with_kind;

fn stop_label(report: &mere::canvas::dynamics_recipe::SettleReport) -> String {
    format!(
        "{} · {} {}",
        match report.end {
            mere::canvas::dynamics_recipe::SettleEnd::Rested => "At rest",
            mere::canvas::dynamics_recipe::SettleEnd::LawFinished => "Law finished",
            mere::canvas::dynamics_recipe::SettleEnd::StepLimit => "Step limit reached",
        },
        report.steps,
        if report.steps == 1 { "step" } else { "steps" }
    )
}

/// Retain the existing Scope axis when comparing arrangements with motion.
/// Only the whole-set working cell continues after its bounded frame.
pub fn compare_arrangements_dynamics(
    draft: &ProjectionDraft,
    dataset: &ProjectionDataset,
    selected: Option<&str>,
    bound: u32,
    layout: &FacetLayout,
) -> Result<DynamicsComparison, String> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    let mut frames = Vec::new();
    let mut focused = None;
    let comparison = crate::projection_compare::compare_arrangements_with_scene(
        draft,
        dataset,
        selected,
        layout,
        |definition, compiled| {
            let row = counts
                .entry(definition.arrangement.kind.clone())
                .or_default();
            let index = *row;
            *row += 1;
            let (scene, stop) = if let Some(slot) = &definition.dynamics {
                let mut preview = ProjectionDynamics::new(definition, &compiled, slot)
                    .map_err(|e| e.to_string())?;
                let report = preview.snapshot(bound).map_err(|e| e.to_string())?;
                let scene = preview.scene().clone();
                if definition.arrangement.kind == draft.arrangement.kind && index == 0 {
                    focused = Some(preview);
                }
                (scene, Some(stop_label(&report)))
            } else {
                (compiled.scene, None)
            };
            frames.push((
                definition.arrangement.kind.clone(),
                index,
                scene.clone(),
                stop,
            ));
            Ok(scene)
        },
    )?;
    let scenes = comparison
        .cells
        .iter()
        .map(|cell| {
            let scene = frames
                .iter()
                .find(|(family, row, _, _)| family == &cell.family && row == &cell.row)
                .expect("a composed cell has its realized frame")
                .2
                .clone();
            (cell.swatch_id.clone(), scene)
        })
        .collect();
    let stops = comparison
        .cells
        .iter()
        .filter_map(|cell| {
            let stop = &frames
                .iter()
                .find(|(family, row, _, _)| family == &cell.family && row == &cell.row)?
                .3;
            Some((cell.swatch_id.clone(), stop.clone()?))
        })
        .collect();
    Ok(DynamicsComparison {
        comparison,
        stops,
        scenes,
        focused,
        layout: *layout,
    })
}

pub struct DynamicsComparison {
    pub comparison: Comparison,
    /// Snapshot termination, kept separate from the axis label.
    pub stops: Vec<(String, String)>,
    scenes: Vec<(String, sceno::Scene)>,
    focused: Option<ProjectionDynamics>,
    layout: FacetLayout,
}

impl DynamicsComparison {
    pub fn is_running(&self) -> bool {
        self.focused
            .as_ref()
            .is_some_and(ProjectionDynamics::is_running)
    }
    /// Advance only the working cell. Every other cell keeps its bounded
    /// frame. Recomposition renumbers instances while preserving sources.
    pub fn tick(&mut self) -> Result<bool, String> {
        let Some(focused) = &mut self.focused else {
            return Ok(false);
        };
        let moving = focused.tick().map_err(|e| e.to_string())?;
        if let Some((_, scene)) = self.scenes.iter_mut().find(|(id, _)| id == WORKING) {
            let frame = scene.bounds;
            *scene = focused.scene().clone();
            // Keep the initial snapshot's frame and the shared scale. Motion
            // in one cell must not recenter or rescale every other cell.
            scene.bounds = frame;
        }
        self.comparison.scene = compose_facet(&self.comparison.facet, &self.scenes, &self.layout)
            .map_err(|e| format!("{e:?}"))?;
        Ok(moving)
    }
}

/// The supplied dynamics choices follow the working recipe as rows. The
/// caller chooses both the axis contents and the explicit bound. A selected
/// occurrence narrows the whole matrix, leaving the existing Scope matrix
/// available through `compare_arrangements`.
pub fn compare_dynamics(
    draft: &ProjectionDraft,
    dataset: &ProjectionDataset,
    selected: Option<&str>,
    choices: &[(String, DynamicsSlot)],
    bound: u32,
    layout: &FacetLayout,
) -> Result<DynamicsComparison, String> {
    let compiler = practice_compiler();
    let mut data = dataset.clone();
    let scope = if let Some(id) = selected {
        data.occurrences
            .retain(|occurrence| occurrence.occurrence_id == id);
        if data.occurrences.is_empty() {
            return Err("comparison.scope: selected occurrence is absent".into());
        }
        Scope::Selection(vec![id.into()])
    } else {
        Scope::Mere(data.source.resource.clone())
    };
    let working_family = Family::resolve(&draft.arrangement.kind);
    let families: Vec<_> = std::iter::once(draft.arrangement.kind.clone())
        .chain(
            FAMILIES
                .iter()
                .filter(|f| Some(**f) != working_family)
                .map(|f| f.id().to_string()),
        )
        .collect();
    let rows: Vec<_> = std::iter::once(("Working".to_string(), draft.dynamics.clone()))
        .chain(
            choices
                .iter()
                .map(|(label, slot)| (label.clone(), Some(slot.clone()))),
        )
        .collect();
    let mut facet_cells = Vec::new();
    let mut cells = Vec::new();
    let mut scenes = Vec::new();
    let mut refused = Vec::new();
    let mut stops = Vec::new();
    let mut focused = None;
    for (column, family) in families.iter().enumerate() {
        for (row, (label, slot)) in rows.iter().enumerate() {
            let working = column == 0 && row == 0;
            let id = if working {
                WORKING.into()
            } else {
                format!("dynamics:{column}:{row}")
            };
            let mut cell_draft = draft.clone();
            cell_draft.arrangement = with_kind(&draft.arrangement, family, compiler.registry());
            cell_draft.dynamics = slot.clone();
            let result = (|| {
                let definition = cell_draft.to_definition().map_err(|e| format!("{e:?}"))?;
                let compiled = compiler
                    .compile(&definition, &data)
                    .map_err(|e| format!("{e:?}"))?;
                if let Some(slot) = slot {
                    let mut preview = ProjectionDynamics::new(&definition, &compiled, slot)
                        .map_err(|e| e.to_string())?;
                    let report = preview.snapshot(bound).map_err(|e| e.to_string())?;
                    stops.push((id.clone(), stop_label(&report)));
                    let scene = preview.scene().clone();
                    if working {
                        focused = Some(preview);
                    }
                    Ok::<_, String>(scene)
                } else {
                    Ok(compiled.scene)
                }
            })();
            match result {
                Ok(scene) => {
                    scenes.push((id.clone(), scene));
                    facet_cells.push(FacetCell {
                        column,
                        row,
                        swatch: Swatch {
                            id: id.clone(),
                            scope: scope.clone(),
                            mode: if working {
                                SwatchMode::Reflection {
                                    of: Followed::View {
                                        app: EDITOR_VIEW.0.into(),
                                        view: EDITOR_VIEW.1.into(),
                                    },
                                }
                            } else {
                                SwatchMode::Projection {
                                    definition_id: draft.id.clone(),
                                    variant_id: Some(id.clone()),
                                }
                            },
                        },
                    });
                    cells.push(ComparedCell {
                        swatch_id: id,
                        family: family.clone(),
                        working,
                        row,
                        column,
                        dynamics: slot.clone(),
                    });
                },
                Err(reason) if working => return Err(reason),
                Err(reason) => refused.push((format!("{family} / {label}"), reason)),
            }
        }
    }
    let facet = Facet {
        id: "projection-editor.dynamics".into(),
        columns: Axis {
            kind: AxisKind::Arrangement,
            labels: families,
            scale: AxisScale::Shared,
        },
        rows: Some(Axis {
            kind: AxisKind::Dynamics,
            labels: rows.into_iter().map(|(label, _)| label).collect(),
            scale: AxisScale::Shared,
        }),
        cells: facet_cells,
    };
    let scene = compose_facet(&facet, &scenes, layout).map_err(|e| format!("{e:?}"))?;
    Ok(DynamicsComparison {
        comparison: Comparison {
            facet,
            scene,
            cells,
            refused,
        },
        stops,
        scenes,
        focused,
        layout: layout.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection_compile::default_definition;
    use crate::projection_editor::{EditorAction, ProjectionEditor};
    use mere::canvas::dynamics_recipe::preset_slot;

    fn fixture() -> (ProjectionDraft, ProjectionDataset) {
        let data: ProjectionDataset =
            serde_json::from_str(include_str!("../web/fixtures/woodshed-stage.json")).unwrap();
        let definition = default_definition(&data);
        (
            ProjectionDraft {
                version: definition.version,
                id: definition.id,
                label: definition.label,
                source: definition.source,
                reading: definition.reading,
                encoding: definition.encoding,
                arrangement: definition.arrangement,
                dynamics: definition.dynamics,
                interaction: definition.interaction,
                appearance: definition.appearance,
                provenance: definition.provenance,
            },
            data,
        )
    }

    #[test]
    fn the_matrix_keeps_occurrences_and_only_the_working_cell_moves() {
        let (mut draft, data) = fixture();
        draft.dynamics = Some(preset_slot("orbit.gravity").unwrap());
        let choices = vec![("Still".into(), preset_slot("still.default").unwrap())];
        let mut matrix =
            compare_dynamics(&draft, &data, None, &choices, 8, &FacetLayout::default()).unwrap();
        let repeated =
            compare_dynamics(&draft, &data, None, &choices, 8, &FacetLayout::default()).unwrap();
        assert_eq!(matrix.comparison.scene, repeated.comparison.scene);
        assert_eq!(matrix.comparison.facet.columns.kind, AxisKind::Arrangement);
        assert_eq!(
            matrix.comparison.facet.rows.as_ref().unwrap().kind,
            AxisKind::Dynamics
        );
        let before = matrix.scenes.clone();
        matrix.tick().unwrap();
        for ((id, old), (_, new)) in before.iter().zip(&matrix.scenes) {
            if id == WORKING {
                assert_ne!(old.items, new.items);
            } else {
                assert_eq!(old, new);
            }
        }
        let focused = matrix.focused.as_ref().unwrap();
        assert_eq!(focused.scene().items.len(), data.occurrences.len());
        assert_eq!(
            focused.scene().sources,
            practice_compiler()
                .compile(&draft.to_definition().unwrap(), &data)
                .unwrap()
                .scene
                .sources
        );
        // A different cap is the positive control for the exact trajectory.
        let capped =
            compare_dynamics(&draft, &data, None, &choices, 7, &FacetLayout::default()).unwrap();
        assert_ne!(before[0].1.items, capped.scenes[0].1.items);
    }

    #[test]
    fn a_cell_pick_changes_both_recipe_fields_in_one_undo_step() {
        let (draft, _) = fixture();
        let mut editor = ProjectionEditor::new(draft.clone());
        let arrangement = with_kind(&draft.arrangement, "spiral", practice_compiler().registry());
        editor.reduce(
            EditorAction::ApplyComparison {
                arrangement,
                dynamics: Some(preset_slot("orbit.gravity").unwrap()),
            },
            0,
        );
        assert_ne!(editor.draft(), &draft);
        assert!(editor.undo());
        assert_eq!(editor.draft(), &draft);
        assert!(!editor.can_undo());
    }

    #[test]
    fn arrangement_comparison_preserves_scope_and_moves_only_its_working_cell() {
        let (mut draft, data) = fixture();
        draft.dynamics = Some(preset_slot("orbit.gravity").unwrap());
        let selected = data.occurrences[0].occurrence_id.as_str();
        let mut matrix = compare_arrangements_dynamics(
            &draft,
            &data,
            Some(selected),
            8,
            &FacetLayout::default(),
        )
        .unwrap();
        assert_eq!(
            matrix.comparison.facet.rows.as_ref().unwrap().kind,
            AxisKind::Scope
        );
        assert!(matrix.stops.iter().all(
            |(_, stop)| stop == "Step limit reached · 8 steps" || stop.starts_with("At rest ·")
        ));
        assert_eq!(matrix.stops.len(), matrix.comparison.cells.len());
        let before = matrix.scenes.clone();
        let spaces = matrix.comparison.scene.spaces.clone();
        matrix.tick().unwrap();
        assert_eq!(matrix.comparison.scene.spaces, spaces);
        for ((id, old), (_, new)) in before.iter().zip(&matrix.scenes) {
            if id == WORKING {
                assert_ne!(old.items, new.items);
            } else {
                assert_eq!(old, new);
            }
        }
    }

    #[test]
    fn a_named_but_undisclosed_channel_is_refused() {
        let (mut draft, data) = fixture();
        let mut json: serde_json::Value =
            serde_json::from_str(&preset_slot("spring.rapier").unwrap().spec).unwrap();
        json["channels"] = serde_json::json!({"mass":"mass.pagerank"});
        draft.dynamics = Some(DynamicsSlot::from_json(1, &json.to_string()).unwrap());
        let error = compare_dynamics(&draft, &data, None, &[], 8, &FacetLayout::default())
            .err()
            .unwrap();
        assert!(error.contains("channels") && error.contains("no disclosed"));
    }

    #[test]
    fn direct_positions_hold_while_ranked_cells_can_move() {
        let (draft, mut data) = fixture();
        // Separate coordinates so F47's contact exception cannot mask drift.
        for (i, occurrence) in data.occurrences.iter_mut().enumerate() {
            for field in ["order", "tempo_bpm"] {
                occurrence.values.insert(
                    field.into(),
                    crate::projection_compile::ProjectionValue::Number(i as f64 * 1000.0),
                );
            }
        }
        let compiler = practice_compiler();
        for family in ["geographic", "embedded", "timeline"] {
            let mut variant = draft.clone();
            variant.arrangement = with_kind(&draft.arrangement, family, compiler.registry());
            let definition = variant.to_definition().unwrap();
            let compiled = compiler.compile(&definition, &data).unwrap();
            let mut accepted = 0;
            for law in mere::canvas::PhysicsLaw::ALL {
                let slot = preset_slot(law.id()).unwrap();
                let mut preview = match ProjectionDynamics::new(&definition, &compiled, &slot) {
                    Ok(preview) => preview,
                    Err(error) => {
                        assert_eq!(family, "timeline");
                        assert!(
                            error
                                .to_string()
                                .contains("encoded-axis constraint adapter")
                        );
                        continue;
                    },
                };
                let seed = preview.snapshot(0).unwrap();
                let moved = preview.snapshot(60).unwrap();
                for ((a_key, a), (b_key, b)) in seed.positions.iter().zip(&moved.positions) {
                    assert_eq!(a_key, b_key);
                    assert_eq!(a.x, b.x, "{family} / {}", law.id());
                    if family != "timeline" {
                        assert_eq!(a.y, b.y, "{family} / {}", law.id());
                    }
                }
                accepted += 1;
            }
            assert!(accepted > 0);
        }
        // Same Field encodings, different placement meaning: ranks are free.
        let definition = draft.to_definition().unwrap();
        let compiled = compiler.compile(&definition, &data).unwrap();
        let slot = preset_slot("orbit.gravity").unwrap();
        let mut free = ProjectionDynamics::new(&definition, &compiled, &slot).unwrap();
        let before = free.snapshot(0).unwrap();
        assert_ne!(before.positions, free.snapshot(60).unwrap().positions);
    }

    #[test]
    fn presentation_refresh_preserves_the_live_trajectory_and_duplicate_sources() {
        let (mut draft, mut data) = fixture();
        data.occurrences[1].source = data.occurrences[0].source.clone();
        let slot = preset_slot("orbit.gravity").unwrap();
        draft.dynamics = Some(slot.clone());
        let definition = draft.to_definition().unwrap();
        let compiled = practice_compiler().compile(&definition, &data).unwrap();
        let mut preview = ProjectionDynamics::new(&definition, &compiled, &slot).unwrap();
        let before = preview.snapshot(8).unwrap();
        assert_eq!(before.positions.len(), data.occurrences.len());
        assert_ne!(before.positions[0].0, before.positions[1].0);
        let mut changed = compiled.scene.clone();
        changed.items[0].layer += 1;
        assert!(preview.refresh_display(&slot, &changed));
        assert_eq!(preview.snapshot(0).unwrap().positions, before.positions);
        preview.tick().unwrap();
        let mut control = ProjectionDynamics::new(&definition, &compiled, &slot).unwrap();
        assert_eq!(
            preview.snapshot(0).unwrap().positions,
            control.snapshot(9).unwrap().positions
        );
        changed.items[0].footprint = sceno::Footprint::Circle { radius: 19.0 };
        assert!(!preview.refresh_display(&slot, &changed));
    }
}
