// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Native graph scene state uses the shared member-keyed facet contracts.
//! Attention remains transient; only seeds, regions and Mosaic travel here.

use mere::canvas::geometry::{SavedMosaic, SavedRegion};
use mere::canvas::{Canvas, CartographyGeometry};
use mere::kernel::graph::NodeFacetStore;
use uuid::Uuid;

pub fn write(store: &mut NodeFacetStore, container: Option<Uuid>, geometry: &CartographyGeometry) {
    pandect::write_arrangement_seed(store, geometry.seed().map(|s| s.iter().copied()));
    if let Some(container) = container {
        let regions: Vec<_> = geometry
            .regions()
            .iter()
            .map(|r| pandect::ArrangementRegion {
                contour: r.contour.clone(),
                label: r.label.clone(),
                members: r.members.clone(),
            })
            .collect();
        pandect::write_arrangement_regions(store, container, &regions);
        let mosaic = geometry.mosaic().map(|m| pandect::ArrangementMosaic {
            group_of: m.group_of.clone(),
            group_order: m.group_order.clone(),
            member_order: m.member_order.clone(),
            scale: m.scale.clone(),
        });
        pandect::write_arrangement_mosaic(store, container, mosaic.as_ref());
    }
}

pub fn restore(canvas: &mut Canvas, container: Option<Uuid>) {
    let store = canvas.facets();
    let regions = container
        .map(|c| pandect::read_arrangement_regions(store, c))
        .unwrap_or_default();
    let mosaic = container.and_then(|c| pandect::read_arrangement_mosaic(store, c));
    let geometry = CartographyGeometry::default()
        .with_seed(pandect::read_arrangement_seed(store))
        .with_regions(regions.into_iter().map(|r| SavedRegion {
            contour: r.contour,
            label: r.label,
            members: r.members,
        }))
        .with_mosaic(mosaic.map(|m| SavedMosaic {
            group_of: m.group_of,
            group_order: m.group_order,
            member_order: m.member_order,
            scale: m.scale,
        }));
    canvas.apply_cartography_scene_state(&geometry);
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Facts {
    pub arrangement: String,
    pub saved_seed: bool,
    pub regions: usize,
    pub foreground_pins: usize,
    pub retained: usize,
    pub grammar: bool,
}

pub fn facts(canvas: &Canvas) -> Facts {
    Facts {
        arrangement: canvas.layout_strategy().unwrap_or("free").into(),
        saved_seed: canvas.has_saved_seed(),
        regions: canvas.strategy_regions().len(),
        foreground_pins: canvas
            .graph()
            .nodes()
            .filter(|(_, n)| canvas.member_foreground_pinned(n.id) == Some(true))
            .count(),
        retained: canvas
            .members_with_attention(scenograph::attention::Attention::Retained)
            .len(),
        grammar: canvas.graph_grammar(),
    }
}
