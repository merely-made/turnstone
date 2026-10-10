// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The projection editor's comparison grid (Scenograph editor plan, S1,
//! SE79 to SE83): the working draft and every other built-in arrangement
//! family side by side, over the whole set and, when something is selected,
//! the selection. Each cell compiles with the practice compiler and the
//! cells compose into one scene.

use scenograph::swatch::{
    Axis, AxisKind, AxisScale, Facet, FacetCell, Followed, Scope, Swatch, SwatchMode,
};
use scenomise::catalog::{FAMILIES, Family};
use scenomise::facet::compose_facet;
// The page draws the grid from these.
pub use scenomise::facet::{FACET_AXIS_ADAPTER, FACET_CELL_ADAPTER, FacetLayout};

use crate::projection_compile::{ProjectionDataset, practice_compiler};
use crate::projection_editor::{ProjectionDraft, with_kind};

/// The view a working cell reflects (SE58, SE66).
pub const EDITOR_VIEW: (&str, &str) = ("graphshell", "projection-editor");
/// The swatch id of the working draft's cell in the first row.
pub const WORKING: &str = "working";

/// One cell as the host draws and picks it.
#[derive(Clone, Debug, PartialEq)]
pub struct ComparedCell {
    pub swatch_id: String,
    /// The family this cell shows; the working draft's own kind for the
    /// working cell.
    pub family: String,
    pub working: bool,
    pub row: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Comparison {
    pub facet: Facet,
    pub scene: sceno::Scene,
    pub cells: Vec<ComparedCell>,
    /// Families that could not compile over this data, with the reason.
    pub refused: Vec<(String, String)>,
}

/// Build the comparison for `draft` over `dataset`; `selected` adds a second
/// row over that occurrence (SE83).
pub fn compare_arrangements(
    draft: &ProjectionDraft,
    dataset: &ProjectionDataset,
    selected: Option<&str>,
    layout: &FacetLayout,
) -> Result<Comparison, String> {
    let compiler = practice_compiler();
    let working_kind = draft.arrangement.kind.clone();
    let mut families = vec![working_kind.clone()];
    // An alias (`grid.default`) names the same family as its id (`grid`).
    let working_family = Family::resolve(&working_kind);
    families.extend(
        FAMILIES
            .iter()
            .filter(|family| Some(**family) != working_family)
            .map(|family| family.id().to_string()),
    );
    let selection: Option<ProjectionDataset> = selected.map(|id| {
        let mut subset = dataset.clone();
        subset
            .occurrences
            .retain(|occurrence| occurrence.occurrence_id == id);
        subset
    });
    let rows: Vec<(&str, Scope, &ProjectionDataset)> =
        std::iter::once(("All", Scope::Mere(dataset.source.resource.clone()), dataset))
            .chain(
                selection.as_ref().zip(selected).map(|(subset, id)| {
                    ("Selected", Scope::Selection(vec![id.to_string()]), subset)
                }),
            )
            .collect();

    // A family that cannot compile over the whole set is left out of the
    // grid and reported, so every column holds a drawable cell.
    let mut columns = Vec::new();
    let mut refused = Vec::new();
    let mut compiled = Vec::new();
    for family in &families {
        let mut cell_draft = draft.clone();
        cell_draft.arrangement = with_kind(&draft.arrangement, family, compiler.registry());
        let scenes: Result<Vec<_>, String> = rows
            .iter()
            .map(|(_, _, data)| {
                let definition = cell_draft.to_definition().map_err(|issues| {
                    issues
                        .iter()
                        .map(|i| i.message.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                })?;
                compiler
                    .compile(&definition, data)
                    .map(|compiled| compiled.scene)
                    .map_err(|issues| {
                        issues
                            .iter()
                            .map(|i| i.message.clone())
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
            })
            .collect();
        match scenes {
            Ok(scenes) => {
                columns.push(family.clone());
                compiled.push(scenes);
            },
            Err(reason) => refused.push((family.clone(), reason)),
        }
    }
    if columns.first() != Some(&working_kind) {
        return Err(refused
            .first()
            .map(|(_, reason)| reason.clone())
            .unwrap_or_else(|| "the working draft does not compile".into()));
    }

    let mut cells = Vec::new();
    let mut facet_cells = Vec::new();
    let mut scenes = Vec::new();
    for (column, (family, row_scenes)) in columns.iter().zip(compiled).enumerate() {
        for (row, ((_, scope, _), scene)) in rows.iter().zip(row_scenes).enumerate() {
            let working = column == 0;
            let swatch_id = match (working, row) {
                (true, 0) => WORKING.to_string(),
                _ => format!("{family}:{row}"),
            };
            let mode = if working && row == 0 {
                SwatchMode::Reflection {
                    of: Followed::View {
                        app: EDITOR_VIEW.0.into(),
                        view: EDITOR_VIEW.1.into(),
                    },
                }
            } else {
                SwatchMode::Projection {
                    definition_id: draft.id.clone(),
                    variant_id: Some(family.clone()),
                }
            };
            facet_cells.push(FacetCell {
                column,
                row,
                swatch: Swatch {
                    id: swatch_id.clone(),
                    scope: scope.clone(),
                    mode,
                },
            });
            cells.push(ComparedCell {
                swatch_id: swatch_id.clone(),
                family: family.clone(),
                working,
                row,
                column,
            });
            scenes.push((swatch_id, scene));
        }
    }
    let labels = columns
        .iter()
        .enumerate()
        .map(|(index, family)| {
            if index == 0 {
                format!("{family} (working)")
            } else {
                family.clone()
            }
        })
        .collect();
    let facet = Facet {
        id: "projection-editor.compare".into(),
        columns: Axis {
            kind: AxisKind::Arrangement,
            labels,
            scale: AxisScale::Shared,
        },
        rows: (rows.len() > 1).then(|| Axis {
            kind: AxisKind::Scope,
            labels: rows.iter().map(|(label, _, _)| label.to_string()).collect(),
            // The selection is smaller than the whole set by construction;
            // each row fits itself.
            scale: AxisScale::Independent,
        }),
        cells: facet_cells,
    };
    let scene = compose_facet(&facet, &scenes, layout).map_err(|error| format!("{error:?}"))?;
    Ok(Comparison {
        facet,
        scene,
        cells,
        refused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection_compile::default_definition;

    /// The practice fixture the web editor loads, as a draft and its data.
    fn draft_for_tests() -> (ProjectionDraft, ProjectionDataset) {
        let dataset: ProjectionDataset =
            serde_json::from_str(include_str!("../web/fixtures/woodshed-stage.json"))
                .expect("the practice fixture");
        let definition = default_definition(&dataset);
        let draft = ProjectionDraft {
            version: definition.version,
            id: definition.id.clone(),
            label: definition.label.clone(),
            source: definition.source.clone(),
            reading: definition.reading.clone(),
            encoding: definition.encoding.clone(),
            arrangement: definition.arrangement.clone(),
            interaction: definition.interaction.clone(),
            appearance: definition.appearance.clone(),
            provenance: definition.provenance.clone(),
        };
        (draft, dataset)
    }

    #[test]
    fn the_working_draft_leads_and_every_other_family_follows_once() {
        let (draft, dataset) = draft_for_tests();
        let comparison = compare_arrangements(&draft, &dataset, None, &FacetLayout::default())
            .expect("compares");
        assert_eq!(comparison.cells[0].swatch_id, WORKING);
        assert!(comparison.cells[0].working);
        assert_eq!(comparison.cells[0].family, draft.arrangement.kind);
        let mut families: Vec<&str> = comparison.cells.iter().map(|c| c.family.as_str()).collect();
        let shown = families.len();
        families.sort();
        families.dedup();
        assert_eq!(families.len(), shown, "each family once");
        assert_eq!(
            shown + comparison.refused.len(),
            11,
            "every family shown or refused"
        );
        assert_eq!(comparison.facet.validate(), Ok(()));
        assert!(comparison.facet.rows.is_none());
    }

    #[test]
    fn an_alias_working_kind_is_not_shown_twice() {
        let (mut draft, dataset) = draft_for_tests();
        draft.arrangement.kind = "grid.default".into();
        let comparison = compare_arrangements(&draft, &dataset, None, &FacetLayout::default())
            .expect("compares");
        assert_eq!(comparison.cells[0].family, "grid.default");
        assert!(!comparison.cells.iter().any(|cell| cell.family == "grid"));
    }

    #[test]
    fn a_selection_adds_a_row_over_that_occurrence() {
        let (draft, dataset) = draft_for_tests();
        let first = dataset.occurrences[0].occurrence_id.clone();
        let comparison =
            compare_arrangements(&draft, &dataset, Some(&first), &FacetLayout::default())
                .expect("compares");
        let rows = comparison.facet.rows.as_ref().expect("a scope row");
        assert_eq!(rows.labels, ["All", "Selected"]);
        let selected = comparison
            .facet
            .cells
            .iter()
            .find(|cell| cell.row == 1)
            .expect("a selected cell");
        assert_eq!(selected.swatch.scope, Scope::Selection(vec![first]));
    }

    #[test]
    fn every_compiled_cell_keeps_its_cards_inside_its_frame() {
        use sceno::{ProjectedItem, Rect, Representation, Size2, Vec2};
        let (draft, dataset) = draft_for_tests();
        let first = dataset.occurrences[0].occurrence_id.clone();
        let comparison =
            compare_arrangements(&draft, &dataset, Some(&first), &FacetLayout::default())
                .expect("compares");
        let scene = &comparison.scene;
        let world_rect = |item: &ProjectedItem| {
            let world = scene.to_world(item.space).unwrap().then(&item.transform);
            let local = item.footprint.bounds().unwrap();
            Rect::new(
                Vec2::new(
                    world.translate.x + local.origin.x * world.scale,
                    world.translate.y + local.origin.y * world.scale,
                ),
                Size2::new(local.size.w * world.scale, local.size.h * world.scale),
            )
        };
        let frames: Vec<_> = scene
            .items
            .iter()
            .filter(|item| {
                item.representation
                    == Representation::Open {
                        kind: "facet.cell".into(),
                    }
            })
            .collect();
        assert_eq!(frames.len(), comparison.cells.len());
        for (frame, cell) in frames.iter().zip(&comparison.cells) {
            let outer = world_rect(frame);
            // A cell's items sit in spaces under the cell's own space.
            let cell_space = scene
                .spaces
                .iter()
                .position(|space| {
                    space.name.as_deref() == Some(&format!("cell: {}", cell.swatch_id))
                })
                .expect("the cell's space") as u32;
            let held = scene.items.iter().filter(|item| {
                let mut space = Some(item.space);
                while let Some(id) = space {
                    if id.0 == cell_space {
                        return true;
                    }
                    space = scene.spaces[id.0 as usize].parent;
                }
                false
            });
            for item in held {
                let inner = world_rect(item);
                assert!(
                    inner.origin.x >= outer.origin.x - 0.5
                        && inner.origin.y >= outer.origin.y - 0.5
                        && inner.origin.x + inner.size.w <= outer.origin.x + outer.size.w + 0.5
                        && inner.origin.y + inner.size.h <= outer.origin.y + outer.size.h + 0.5,
                    "{}: a card escapes its frame",
                    cell.swatch_id
                );
            }
        }
    }
}
