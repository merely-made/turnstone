// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Scenario input samples the ordinary canvas's current painted path.
use super::*;

pub(super) fn click(
    lane: &TreeLane,
    ctx: &mut mesquite::Ctx<'_, TreeLane>,
    request: &str,
) -> Result<(), String> {
    use graphshell::mere_host::{
        LINK_REST_A1_ADDRESS, LINK_REST_A2_ADDRESS, LINK_REST_B1_ADDRESS, LINK_REST_B2_ADDRESS,
    };
    let words = request.split_whitespace().collect::<Vec<_>>();
    let [pair, stratum, sample] = words.as_slice() else {
        return Err(
            "click-link-path wants <a1-b1|a1-b2|a2-b1|a2-b2> <surface|resource> midpoint".into(),
        );
    };
    if *sample != "midpoint" {
        return Err("click-link-path only supports an actual sampled midpoint".into());
    }
    let (from_address, to_address) = match *pair {
        "a1-b1" => (LINK_REST_A1_ADDRESS, LINK_REST_B1_ADDRESS),
        "a1-b2" => (LINK_REST_A1_ADDRESS, LINK_REST_B2_ADDRESS),
        "a2-b1" => (LINK_REST_A2_ADDRESS, LINK_REST_B1_ADDRESS),
        "a2-b2" => (LINK_REST_A2_ADDRESS, LINK_REST_B2_ADDRESS),
        _ => return Err("click-link-path has an unknown source pair".into()),
    };
    let stratum = match *stratum {
        "surface" => scenograph::link::LinkStratum::Surface,
        "resource" => scenograph::link::LinkStratum::Resource,
        _ => return Err("click-link-path stratum must be surface or resource".into()),
    };
    let point = {
        let canvas = lane.shared.canvas.borrow();
        let graph = canvas.graph();
        let (_, from) = graph
            .get_node_by_url(from_address)
            .ok_or("source endpoint missing")?;
        let (_, to) = graph
            .get_node_by_url(to_address)
            .ok_or("target endpoint missing")?;
        let paths = canvas.visible_link_paths();
        let path = paths
            .iter()
            .find(|path| {
                path.stratum == stratum
                    && ((path.displayed_from == from.id && path.displayed_to == to.id)
                        || (path.displayed_from == to.id && path.displayed_to == from.id))
            })
            .ok_or("requested source path has no painted geometry")?;
        if path.points.len() < 2 {
            return Err("painted path has no selectable segment".into());
        }
        let middle = (path.points.len() - 1) / 2;
        let a = path.points[middle];
        let b = path.points[middle + 1];
        canvas.screen_point_of(((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5))
    };
    let (left, top, width, height) = leaf_rect(ctx).ok_or("canvas leaf is not painted")?;
    if !point.0.is_finite()
        || !point.1.is_finite()
        || point.0 < 0.0
        || point.1 < 0.0
        || point.0 >= width
        || point.1 >= height
    {
        return Err("sampled path midpoint is outside the painted canvas".into());
    }
    ctx.pointer
        .push(HostPointer::Press(left + point.0, top + point.1));
    ctx.pointer
        .push(HostPointer::Release(left + point.0, top + point.1));
    Ok(())
}

pub(super) fn fields(canvas: &Canvas, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let paths = canvas.visible_link_paths();
    let selected = canvas.selected_link_keys();
    let selected_source = !selected.is_empty()
        && selected.iter().all(|key| {
            let Some((a, b)) = canvas.link_appearance_pair(key) else {
                return false;
            };
            paths.iter().any(|path| {
                &path.key == key
                    && ((path.displayed_from == a && path.displayed_to == b)
                        || (path.displayed_from == b && path.displayed_to == a))
                    && !path.buckets.is_empty()
                    && path.buckets.iter().all(|bucket| {
                        !bucket.assertion_ids.is_empty() || !bucket.legacy_members.is_empty()
                    })
            })
        });
    let hidden = canvas.hidden_link_keys();
    let selected_pair = selected
        .first()
        .and_then(|key| canvas.link_appearance_pair(key));
    use graphshell::mere_host::{
        LINK_REST_A1_ADDRESS, LINK_REST_A2_ADDRESS, LINK_REST_B1_ADDRESS, LINK_REST_B2_ADDRESS,
    };
    let selected_pair_tag = [
        ("a1-b1", LINK_REST_A1_ADDRESS, LINK_REST_B1_ADDRESS),
        ("a1-b2", LINK_REST_A1_ADDRESS, LINK_REST_B2_ADDRESS),
        ("a2-b1", LINK_REST_A2_ADDRESS, LINK_REST_B1_ADDRESS),
        ("a2-b2", LINK_REST_A2_ADDRESS, LINK_REST_B2_ADDRESS),
    ]
    .into_iter()
    .find_map(|(tag, a, b)| {
        let (_, a) = canvas.graph().get_node_by_url(a)?;
        let (_, b) = canvas.graph().get_node_by_url(b)?;
        let pair = selected_pair?;
        ((pair.0 == a.id && pair.1 == b.id) || (pair.0 == b.id && pair.1 == a.id)).then_some(tag)
    })
    .unwrap_or("");
    snapshot
        .with_field("link-path-selected-source", selected_source.to_string())
        .with_field("link-path-selected-display-pair", selected_pair_tag)
        .with_field(
            "link-path-hidden-omitted",
            paths
                .iter()
                .all(|path| !hidden.contains(&path.key))
                .to_string(),
        )
}
