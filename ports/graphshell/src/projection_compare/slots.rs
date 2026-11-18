// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use sceno::{Rect, Transform2, Vec2};
use scenograph::presentation::{Body, Extent, NodeMark, Paint, Silhouette};
use scenomise::arrangement::arrangement_capabilities;
use scenomise::projection::DisclosedRelationship;

pub fn compare_slots(
    draft: &ProjectionDraft,
    dataset: &ProjectionDataset,
    relationships: &[DisclosedRelationship],
    selected: Option<&str>,
    layout: &FacetLayout,
    axis: AxisKind,
    viewport: Rect,
) -> Result<Comparison, String> {
    use SwatchChoice::*;
    let caps = arrangement_capabilities(&draft.arrangement, practice_compiler().registry())
        .ok_or_else(|| "The arrangement is unknown".to_string())?;
    let candidates = match axis {
        AxisKind::NodeMark => vec![
            ("default", "Source marks", NodeMark(None)),
            ("tile", "Tile", NodeMark(Some(tile()))),
            (
                "glyph",
                "Measured glyph",
                NodeMark(Some(scenograph::presentation::NodeMark::new(
                    sceno::Representation::Glyph,
                    [Extent::Measured],
                ))),
            ),
            (
                "card",
                "Card",
                NodeMark(Some(scenograph::presentation::NodeMark::card())),
            ),
        ],
        AxisKind::RelationForm => vec![
            ("default", "Source relations", RelationForm(None)),
            (
                "stroke",
                "Stroke",
                RelationForm(Some(sceno::RelationForm::Stroke)),
            ),
            (
                "matrix",
                "Matrix cell",
                RelationForm(Some(sceno::RelationForm::MatrixCell)),
            ),
            (
                "containment",
                "Containment",
                RelationForm(Some(sceno::RelationForm::Containment)),
            ),
            (
                "order",
                "Order",
                RelationForm(Some(sceno::RelationForm::Order)),
            ),
            (
                "adjacency",
                "Adjacency",
                RelationForm(Some(sceno::RelationForm::Adjacency)),
            ),
        ],
        AxisKind::FieldForm => vec![
            ("default", "Default fields", FieldForm(None)),
            (
                "contour",
                "Contour",
                FieldForm(Some(scenograph::FieldForm::Contour)),
            ),
            (
                "backdrop",
                "Backdrop",
                FieldForm(Some(scenograph::FieldForm::Backdrop)),
            ),
            (
                "member_tint",
                "Member tint",
                FieldForm(Some(scenograph::FieldForm::MemberTint)),
            ),
            (
                "guide",
                "Guide",
                FieldForm(Some(scenograph::FieldForm::Guide)),
            ),
        ],
        AxisKind::Frame => vec![
            ("default", "Arrangement frame", Frame(None)),
            (
                "unbounded",
                "Unbounded",
                Frame(Some(sceno::Frame::Unbounded { origin: Vec2::ZERO })),
            ),
            (
                "viewport",
                "Viewport",
                Frame(Some(sceno::Frame::Bounded { bounds: viewport })),
            ),
            (
                "nested",
                "Nested",
                Frame(Some(sceno::Frame::Nested {
                    bounds: viewport,
                    transform: Transform2::IDENTITY,
                })),
            ),
            (
                "flow",
                "Flow",
                Frame(Some(sceno::Frame::Flow {
                    origin: viewport.origin,
                    width: viewport.size.w,
                })),
            ),
        ],
        _ => return Err("Choose a mark, relation, field or frame axis".into()),
    };
    let working = Candidate {
        key: WORKING.into(),
        label: draft.arrangement.kind.clone(),
        draft: draft.clone(),
        choice: None,
        refusal: None,
    };
    let candidates = std::iter::once(working).chain(
        candidates
            .into_iter()
            .map(|(key, label, choice)| {
                let refusal = match &choice {
                    NodeMark(Some(mark)) if !mark.accepts(caps.extent) => Some(format!(
                        "{} assigns {} extent and this mark cannot fill it",
                        draft.arrangement.kind,
                        caps.extent.id()
                    )),
                    RelationForm(Some(sceno::RelationForm::Adjacency)) => {
                        Some("Adjacency requires Crystal".into())
                    },
                    RelationForm(Some(form))
                        if form.is_placed() && !caps.placed_forms.contains(form) =>
                    {
                        Some(format!(
                            "{} does not accept this placed relation form",
                            draft.arrangement.kind
                        ))
                    },
                    RelationForm(Some(_)) if relationships.is_empty() => {
                        Some("This source discloses no relationships".into())
                    },
                    Frame(Some(frame)) if !caps.frames.contains(&frame.kind()) => Some(format!(
                        "{} does not accept a {} frame",
                        draft.arrangement.kind,
                        frame.kind().id()
                    )),
                    _ => None,
                };
                let mut candidate = draft.clone();
                choice.apply(&mut candidate);
                Candidate {
                    key: key.into(),
                    label: label.into(),
                    draft: candidate,
                    choice: Some(choice),
                    refusal,
                }
            })
            .filter(|candidate| candidate.draft != *draft),
    );
    compare_candidates(
        draft,
        dataset,
        selected,
        layout,
        axis,
        candidates,
        |definition, compiled, data| {
            let mut compiled = compiled;
            if !relationships.is_empty() {
                let visible: std::collections::HashSet<_> = data
                    .occurrences
                    .iter()
                    .map(|o| o.occurrence_id.as_str())
                    .collect();
                let relations: Vec<_> = relationships
                    .iter()
                    .filter(|r| {
                        visible.contains(r.from_occurrence.as_str())
                            && visible.contains(r.to_occurrence.as_str())
                    })
                    .cloned()
                    .collect();
                scenomise::host_dataset::route_relationships(&mut compiled, data, &relations)
                    .map_err(|issues| {
                        issues
                            .into_iter()
                            .map(|issue| issue.message)
                            .collect::<Vec<_>>()
                            .join(". ")
                    })?;
            }
            // A selected row can omit every group. Check field availability on the whole scope.
            if axis == AxisKind::FieldForm
                && definition.marks.field.is_some()
                && data.occurrences.len() == dataset.occurrences.len()
                && compiled.scene.regions.is_empty()
            {
                return Err("This arrangement supplies no field regions".into());
            }
            if let Some(slot) = definition.dynamics.as_ref() {
                #[cfg(feature = "physics-engine")]
                {
                    // Validate this combination through the same preview adapter
                    // that will be installed when the person picks the swatch.
                    cartography::canvas::projection_dynamics::ProjectionDynamics::new(
                        definition, &compiled, slot,
                    )
                    .map_err(|error| error.to_string())?;
                }
                #[cfg(not(feature = "physics-engine"))]
                {
                    let _ = slot;
                    return Err("Choose a host with a dynamics preview".into());
                }
            }
            Ok(compiled.scene)
        },
    )
}

fn tile() -> NodeMark {
    NodeMark {
        body: Body::Silhouette(Silhouette::Square),
        paint: Paint::Bare,
        ..NodeMark::new(sceno::Representation::Glyph, [Extent::Assigned])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenomise::authoring::{EditorAction, ProjectionEditor};
    use scenomise::host_dataset::parse_host_dataset;

    #[test]
    fn slot_comparison_offers_only_executable_cells_and_reports_refusals() {
        let host = parse_host_dataset(include_str!(
            "../../web/fixtures/host-dataset-relations.json"
        ))
        .unwrap();
        let mut definition = crate::projection_compile::default_definition(&host.dataset);
        definition.arrangement.kind = "partition".into();
        definition.encoding.x = scenograph::Channel::Field("label".into());
        definition.encoding.y = scenograph::Channel::Field("occurrence_id".into());
        let draft = crate::local_projection::draft_from_definition(&definition);
        let viewport = Rect::new(Vec2::ZERO, sceno::Size2::new(640.0, 360.0));
        let selected = host.dataset.occurrences[0].occurrence_id.as_str();
        for (axis, offered, refused) in [
            (AxisKind::NodeMark, "card:0", Some("Measured glyph")),
            (AxisKind::RelationForm, "matrix:0", Some("Order")),
            (AxisKind::FieldForm, "contour:0", None),
            (AxisKind::Frame, "viewport:0", Some("Unbounded")),
        ] {
            let comparison = compare_slots(
                &draft,
                &host.dataset,
                &host.relationships,
                Some(selected),
                &FacetLayout::default(),
                axis,
                viewport,
            )
            .unwrap();
            assert_eq!(comparison.facet.columns.kind, axis);
            assert_eq!(
                comparison.facet.rows.as_ref().unwrap().kind,
                AxisKind::Scope
            );
            assert!(comparison.facet.validate().is_ok());
            if let Some(name) = refused {
                assert!(
                    comparison
                        .refused
                        .iter()
                        .any(|(label, reason)| label == name && !reason.is_empty())
                );
                assert!(comparison.cells.iter().all(|cell| cell.label != name));
            }
            let cell = comparison
                .cells
                .iter()
                .find(|cell| cell.swatch_id == offered)
                .unwrap();
            let mut editor = ProjectionEditor::new(draft.clone());
            editor.reduce(EditorAction::ApplySwatch(cell.choice.clone().unwrap()), 0);
            let result = host
                .compile(
                    practice_compiler(),
                    &editor.draft().to_definition().unwrap(),
                )
                .unwrap();
            match axis {
                AxisKind::NodeMark => assert!(
                    result
                        .projection
                        .scene
                        .items
                        .iter()
                        .all(|item| item.representation == sceno::Representation::Card)
                ),
                AxisKind::RelationForm => assert!(
                    result
                        .projection
                        .scene
                        .relations
                        .iter()
                        .all(|relation| relation.cell_bounds().is_some())
                ),
                AxisKind::FieldForm => assert_eq!(
                    editor.draft().marks.field,
                    Some(scenograph::FieldForm::Contour)
                ),
                AxisKind::Frame => assert_eq!(
                    result.projection.scene.frame.unwrap().spec,
                    sceno::Frame::Bounded { bounds: viewport }
                ),
                _ => unreachable!(),
            }
            assert!(editor.undo());
            assert_eq!(editor.draft(), &draft);
        }
    }
}
