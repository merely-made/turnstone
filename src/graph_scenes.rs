// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! What a native graph scene shows, for observation. The scene itself is saved
//! and restored by `scene_state`; attention remains transient.

use mere::canvas::Canvas;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Facts {
    pub arrangement: String,
    pub saved_seed: bool,
    pub regions: usize,
    pub foreground_pins: usize,
    pub retained: usize,
    pub grammar: bool,
    pub territory: bool,
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
        territory: canvas.territory_field(),
    }
}
