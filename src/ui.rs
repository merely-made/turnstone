// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The chrome layer's first tenant: the summonable omnibar (rung 3).
//!
//! One line, three intents (the omnibar design, 2026-07-10): **find** matches
//! existing graph nodes first (the graph is the history made spatial;
//! committing a match selects, never refetches), **go** engages for
//! address-shaped input or on Enter with no match, and **do** (`>` prefix) is
//! the actions lane (a hint row this slice; the filterable action list next).
//!
//! Rendering rides the family's proven DOM path: a small `ScriptedDom` +
//! stylesheet laid out by Livery/Buckram, emitted as a paint list, composited
//! by the shell as the second surface of the layered-present seam (canvas
//! below, chrome above; the chrome texture clears transparent and
//! alpha-blends over). The palette is tiny, so the document rebuilds
//! wholesale per state change rather than diffing.

use std::{collections::HashMap, hash::Hash};

use genet_livery::{
    Device, InteractionStates, LiveryLayout, LiveryPaintList, StylePlane, StyleSet, TextSystem,
    emit_paint_list_with_text_system_scrolled_with_images, layout_with_text_system, resolve_styles,
};
use genet_scripted_dom::{NodeId as DomNodeId, ScriptedDom};
use layout_dom_api::{
    AttributeView, LayoutDom, LayoutDomMut, LocalName, Namespace, NodeKind, QualName,
};
use mere::canvas::Canvas;
use paint_list_api::{DeviceIntSize, PaintCmd, PaintList};
use paint_list_render::{CompositeLayer, composite_paint_layers};
use uuid::Uuid;

// Keep only the current zoom's baseline on each UI thread. Palette queries
// reuse it instead of paying another text/layout pass for every keystroke.
thread_local! {
    static CHROME_ROW_METRICS: std::cell::RefCell<Option<(f32, (f32, f32))>> =
        const { std::cell::RefCell::new(None) };
}

/// The omnibar's state, owned by [`crate::app::App`].
#[derive(Clone, Default, PartialEq)]
pub struct OmnibarState {
    pub open: bool,
    /// What this line captures (default: the address/action lanes).
    pub mode: OmnibarMode,
    pub text: String,
    /// The caret's byte offset into `text` (always on a char boundary).
    pub cursor: usize,
    /// In-flight IME composition, shown at the caret but not part of `text`.
    /// Ephemeral by the gesture law: only the IME's commit becomes an Action
    /// ([`crate::action::Action::OmnibarInsert`]); the shell sets this
    /// directly.
    pub preedit: Option<String>,
    /// Index into `suggestions` of the highlighted row.
    pub selected: usize,
    pub suggestions: Vec<Suggestion>,
    /// The `>` lane's palette state, read through Cambium's menu session
    /// (SE48): its query, the highlighted command, and whether it was
    /// expanded to every command ("All commands…"). It starts over when the
    /// omnibar closes.
    pub menu: cambium::MenuSession,
}

/// The chrome stylesheet: the palette card, its input line, and the
/// suggestion rows. Accent values quote the node palette (selection amber)
/// so a highlighted row reads as the thing it commits to.
pub(crate) const CHROME_SHEET: &str = "\
    .omni { position: absolute; background-color: rgb(24, 30, 44); \
            border: 1px solid rgb(70, 82, 110); border-radius: 8px; \
            padding: 8px; } \
    .omni-input { color: rgb(238, 242, 250); font-size: 16px; \
                  padding: 6px 8px; background-color: rgb(15, 19, 30); \
                  border-radius: 6px; white-space: nowrap; overflow: hidden; } \
    .omni-input .field-preedit { color: rgb(180, 200, 255); \
                    text-decoration: underline; } \
    .omni-row { color: rgb(216, 222, 234); font-size: 14px; \
                padding: 5px 8px; white-space: nowrap; overflow: hidden; } \
    .omni-row-sel { color: rgb(28, 22, 10); font-size: 14px; \
                    padding: 5px 8px; white-space: nowrap; overflow: hidden; \
                    background-color: rgb(232, 150, 40); border-radius: 6px; } \
    .omni-row-review { white-space: normal; overflow-wrap: anywhere; } \
    .omni-row-command { display: flex; align-items: center; } \
    .omni-row-label { flex-grow: 1; flex-shrink: 1; white-space: nowrap; overflow: hidden; } \
    .omni-keep { flex-shrink: 0; margin-left: 8px; padding: 1px 6px; font-size: 11px; \
                 border: 1px solid rgb(70, 82, 110); border-radius: 4px; } \
    .omni-row-muted { color: rgb(140, 148, 165); font-size: 14px; \
                      padding: 5px 8px; white-space: nowrap; } \
    .whereami { position: absolute; color: rgb(170, 178, 195); \
                background-color: rgb(20, 25, 38); font-size: 12px; \
                box-sizing: border-box; padding: 2px 12px; border-radius: 0; overflow: hidden; \
                border: 0; white-space: nowrap; } \
    .browser-strip { position: absolute; display: flex; align-items: center; \
                      gap: 5px; box-sizing: border-box; color: rgb(216, 222, 234); \
                      background-color: rgb(20, 25, 38); border: 1px solid rgb(52, 62, 86); \
                      border-radius: 0; padding: 3px 12px; overflow: visible; } \
    .browser-loading { border-bottom: 2px solid rgb(206, 84, 47); } \
    .browser-button, .browser-button-disabled { flex-shrink: 0; font-size: 12px; padding: 4px 8px; \
                      border-radius: 6px; white-space: nowrap; } \
    .browser-button { color: rgb(238, 242, 250); background-color: rgb(36, 44, 62); \
                      border: 1px solid rgb(70, 82, 110); } \
    .browser-button-disabled { color: rgb(104, 112, 130); background-color: rgb(25, 30, 43); \
                      border: 1px solid rgb(45, 53, 71); } \
    .browser-status { flex: 1 1 0; min-width: 0; color: rgb(170, 178, 195); font-size: 12px; padding: 4px 7px; \
                      white-space: nowrap; overflow: hidden; } \
    .document-find { position: absolute; display: flex; align-items: center; gap: 5px; \
                     box-sizing: border-box; color: rgb(216, 222, 234); \
                     background-color: rgb(20, 25, 38); border: 1px solid rgb(70, 82, 110); \
                     border-radius: 9px; padding: 5px 7px; } \
    .document-find .read-query { color: rgb(238, 242, 250); font-size: 13px; padding: 4px 7px; \
                           width: 220px; background-color: rgb(15, 19, 30); border-radius: 6px; \
                           white-space: nowrap; overflow: hidden; } \
    .document-find .read-result { color: rgb(170, 178, 195); font-size: 12px; padding: 4px 7px; \
                             width: 170px; white-space: nowrap; overflow: hidden; } \
    .decision-card { position: absolute; display: flex; flex-direction: column; gap: 7px; \
                     box-sizing: border-box; color: rgb(216, 222, 234); \
                     background-color: rgb(20, 25, 38); border: 1px solid rgb(70, 82, 110); \
                     border-radius: 9px; padding: 10px 12px; } \
    .decision-prompt { color: rgb(238, 242, 250); font-size: 14px; \
                       white-space: normal; overflow-wrap: anywhere; } \
    .decision-status { color: rgb(170, 178, 195); font-size: 12px; } \
    .decision-error { color: rgb(244, 142, 132); font-size: 12px; } \
    .decision-fields, .decision-actions { display: flex; align-items: center; gap: 7px; } \
    .decision-input { color: rgb(238, 242, 250); font-size: 13px; padding: 5px 7px; \
                      width: 278px; background-color: rgb(15, 19, 30); \
                      border: 1px solid rgb(52, 62, 86); border-radius: 6px; \
                      white-space: nowrap; overflow: hidden; } \
    .decision-input-active { border-color: rgb(232, 150, 40); } \
    .link-preview { position: absolute; color: rgb(238, 242, 250); \
                    background-color: rgb(20, 25, 38); border: 1px solid rgb(206, 84, 47); \
                    border-radius: 7px; padding: 5px 9px; font-size: 12px; \
                    white-space: nowrap; overflow: hidden; } \
    .pane { position: absolute; background-color: rgb(22, 27, 40); } \
    .pane-label { position: absolute; color: rgb(190, 198, 214); \
                  font-size: 14px; padding: 10px 14px; white-space: nowrap; }";

/// One pane's retained scroll: how far it is scrolled, and the clock that
/// auto-hides its bars.
///
/// A pane is its own small document — its root box IS its viewport — so this
/// is *document* scroll rather than nested element scroll, and the engine's
/// own clamping decides the range. The offset is retained here because panes
/// rebuild their layout state every frame; anything the engine retained
/// would be discarded on the next paint.
///
/// The fade clock is `cambium_winit`'s, the same one the winit host uses for
/// its own overlay bars, so a pane's bars hold and fade on exactly the timing
/// the rest of the shell does rather than on a second set of constants.
pub struct PaneScroll {
    offset: (f32, f32),
    fade: cambium_winit::ScrollbarFade<()>,
}

struct LiverySnapshot<Id> {
    styles: StylePlane<Id>,
    fragments: LiveryLayout<Id>,
    text: TextSystem,
    content_extent: (f32, f32),
}

type NodeRect = (f32, f32, f32, f32);

/// One pane's Livery/Buckram layout state, retained across frames.
///
/// The DOM remains host-owned, so a mutation refreshes the public Livery
/// style/fragment planes in place here. An unchanged frame retains shaping and
/// geometry and pays paint only. Structural changes, viewport changes, and
/// stylesheet changes count as full rebuilds.
pub(crate) type InlineLayoutHandle = std::rc::Rc<std::cell::RefCell<RetainedLayout>>;
pub struct RetainedLayout {
    inline_leaves: HashMap<u64, Vec<paint_list_api::PaintCmd>>,
    native_sources: HashMap<u64, Vec<PaintCmd>>,
    inline_scroll_root: Option<DomNodeId>,
    inline_bar: Option<DomNodeId>,
    root: Option<DomNodeId>,
    mutation_cursor: u64,
    inline: Option<(std::rc::Weak<std::cell::RefCell<RetainedLayout>>, DomNodeId)>,
    snapshot: Option<LiverySnapshot<DomNodeId>>,
    /// The surface's size in physical pixels: the layout is laid out at
    /// `size / scale`, and every coordinate crossing this type's boundary is
    /// physical. See [`RetainedLayout::set_scale`].
    size: (u32, u32),
    scale: f32,
    /// The scale `snapshot` was laid out at.
    laid_at: f32,
    sheet: String,
    rebuilds: u32,
    /// Scroll offsets inside the layout, in layout (logical) pixels.
    viewport_scroll: (f32, f32),
    element_scroll: HashMap<DomNodeId, (f32, f32)>,
}

/// A borrowed DOM view re-rooted at one window root. This remains a host
/// concern: Livery sees the neutral `LayoutDom`, while Turnstone decides which
/// member of its forest is the document for this window.
struct SubtreeView<'a, D: LayoutDom> {
    dom: &'a D,
    root: D::NodeId,
}

/// One window-root's Livery/Buckram layout, retained across frames.
///
/// [`scene_from_dom`] re-rooted at `root`: only that subtree lays out (at its
/// own viewport) and paints — the forest-dom per-window path. The chrome's N
/// window-roots each render through this with the others untouched.
///
/// **Retained since 2026-08-23.** This path was one-shot before that, on the
/// reasoning that "the chrome card is a handful of rows against a pane's
/// hundreds, so it is the cheap remaining site rather than an oversight." The
/// command palette open-lag receipt
/// (`design_docs/2026-08-22_command_palette_open_lag.md`) measured that
/// belief: with the palette open the card is not a handful of rows, and the
/// one-shot cascade cost 36 ms of a 44 ms frame. The estimate was reasonable
/// and wrong, which is the argument for measuring a site rather than sizing it
/// by eye.
///
/// The subtree twin of [`RetainedLayout`], with one deliberate difference: it
/// does **not** drain the DOM's mutation queue. A chrome forest is many roots
/// over ONE `ScriptedDom`, so a per-root drain would let whichever root ran
/// first swallow the batch and leave every other root believing nothing had
/// changed — a stale window, not merely a slow one. The owner drains once and
/// calls [`RetainedSubtreeLayout::invalidate`] on every root instead.
///
/// Resize and stylesheet changes are detected here, because those are
/// per-root facts the owner would otherwise have to track for each window
/// separately.
pub struct RetainedSubtreeLayout {
    snapshot: Option<LiverySnapshot<DomNodeId>>,
    /// Physical size and scale, as [`RetainedLayout`] keeps them: the layout
    /// runs at `size / scale` and every coordinate at the boundary is physical.
    size: (u32, u32),
    scale: f32,
    laid_at: f32,
    sheet: String,
    dirty: bool,
    rebuilds: u32,
    /// DOM revision represented by the painted fragments, even after a drain.
    mutation_cursor: u64,
}

/// The host stylesheet for cambium's classes (rung 5 slice D). cambium ships no
/// sheet — it names classes a host themes, and its own reference sheet
/// (`component_catalog.css`) sets no `position` on any of them.
///
/// **Theming only.** cambium's inline styles carry ALL the geometry: the grid
/// root is a block, the body is the `position: relative` containing block, and
/// the rows/cells are absolutely placed inside it by `Placement`. A host rule
/// that sets `position` overrides that structure — it still paints, but the
/// fragment tree Livery hit-tests goes with it, and every pane click
/// misses. Colour, type, padding, and fills only.
pub const CAMBIUM_SHEET: &str = "\
    .grid { background-color: rgb(22, 27, 40); color: rgb(210, 216, 230); \
            font-size: 13px; } \
    .grid-header { background-color: rgb(28, 34, 50); } \
    .grid-header-cell { color: rgb(150, 160, 180); font-size: 12px; \
                        padding: 6px 10px; white-space: nowrap; overflow: hidden; } \
    .grid-row-odd { background-color: rgb(25, 30, 44); } \
    .grid-row-selected { background-color: rgb(232, 150, 40); } \
    .grid-cell { color: rgb(200, 208, 224); padding: 5px 10px; \
                 white-space: nowrap; overflow: hidden; } \
    .grid-row-selected .grid-cell { color: rgb(28, 22, 10); } \
    .pane { background-color: rgb(22, 27, 40); } \n    .tablist { display: flex; height: 26px; background-color: rgb(18, 22, 33); } \
    .tab { color: rgb(150, 160, 180); font-size: 12px; padding: 4px 9px; \
           white-space: nowrap; } \
    .wb-tabs-vertical .tablist { flex-direction: column; width: 100%; height: 100%; } \
    .wb-tabs-vertical .tab { flex-shrink: 0; overflow: hidden; width: 100%; min-width: 0; box-sizing: border-box; } \
    .tab.selected { color: rgb(232, 150, 40); \
                    background-color: rgb(22, 27, 40); } \
    .pane-empty { color: rgb(120, 130, 150); font-size: 12px; padding: 12px; } \
    .list-section-title { color: rgb(150, 160, 180); font-size: 12px;                           padding: 10px 14px 4px 14px; white-space: nowrap; }     .list-row { color: rgb(200, 208, 224); font-size: 13px;                 padding: 5px 14px; white-space: nowrap; overflow: hidden; }     .list-row.muted { color: rgb(120, 130, 150); }     .list-row.action { color: rgb(232, 150, 40); }     .detail-section-title { color: rgb(150, 160, 180); font-size: 12px; \
                            padding: 10px 14px 4px 14px; white-space: nowrap; } \
    .detail-row { display: flex; padding: 3px 14px; } \
    .detail-key { color: rgb(150, 160, 180); font-size: 12px; width: 130px; \
                  white-space: nowrap; overflow: hidden; } \
    .detail-value { color: rgb(200, 208, 224); font-size: 12px; \
                    white-space: nowrap; overflow: hidden; } \
    .wb-body { color: rgb(120, 130, 150); font-size: 12px; padding: 12px; \
               background-color: rgb(25, 30, 44); white-space: nowrap; overflow: hidden; } \
    .graph-canvas-swatch { background-color: rgb(18, 22, 33); \
                           border: 1px solid rgb(52, 62, 86); border-radius: 7px; } \
    .graph-canvas-swatch-node { background-color: transparent; border: 0; padding: 0; }     .graph-canvas-swatch-label { color: rgb(196, 204, 220); font-size: 10px; } \
    .graph-canvas-swatch-expand { color: rgb(150, 160, 180); font-size: 10px; \
                                  background-color: rgb(28, 34, 50); border: 0; \
                                  border-radius: 4px; padding: 2px 5px; } \
    .radio { color: rgb(220, 226, 238); font-size: 14px; } \
    .toggle { color: rgb(220, 226, 238); font-size: 14px; } \
    .radio.selected { color: rgb(232, 150, 40); }";

/// The divider band's clear colour: the chrome border tone (rgb(52, 62, 86)),
/// srgb-to-linear'd for the wgpu clear. An empty scene over this clear IS the
/// divider's paint.
pub const SEAM_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.0343,
    g: 0.0483,
    b: 0.0930,
    a: 1.0,
};

/// The height `.tablist` occupies in [`CAMBIUM_SHEET`], above a tabbed pane's
/// body. The host owns the strip's geometry (cambium's strip sets none), so the
/// host must also state its height: the Roster subtracts it from the grid's
/// viewport and adds it to a row's y. `tablist_height_matches_the_sheet` holds
/// this to what the sheet actually lays out.
pub const TABLIST_HEIGHT: f32 = cambium::ShellMetrics::DOCUMENT_ROW_HEIGHT;

#[cfg(test)]
#[path = "ui/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ui/scroll_tests.rs"]
mod scroll_tests;

#[cfg(test)]
#[path = "ui/retained_layout_tests.rs"]
mod retained_layout_tests;

#[cfg(test)]
#[path = "ui/contributed_projection_tests.rs"]
mod contributed_projection_tests;

#[cfg(test)]
#[path = "ui/projection_work_tests.rs"]
mod projection_work_tests;

#[path = "ui/chrome_row_width.rs"]
mod chrome_row_width;
pub(crate) use chrome_row_width::chrome_row_width;
#[path = "ui/chrome_card_row_metrics.rs"]
mod chrome_card_row_metrics;
use chrome_card_row_metrics::chrome_card_row_metrics;
#[path = "ui/ordinary_chrome_row_metrics.rs"]
mod ordinary_chrome_row_metrics;
pub(crate) use ordinary_chrome_row_metrics::ordinary_chrome_row_metrics;
#[path = "ui/chrome_review_row_extra_height.rs"]
mod chrome_review_row_extra_height;
pub(crate) use chrome_review_row_extra_height::chrome_review_row_extra_height;
#[path = "ui/visible_row_limit.rs"]
mod visible_row_limit;
pub use visible_row_limit::visible_row_limit;
#[path = "ui/visible_row_limit_with_extra_height.rs"]
mod visible_row_limit_with_extra_height;
pub(crate) use visible_row_limit_with_extra_height::visible_row_limit_with_extra_height;
#[path = "ui/impl_std_fmt_debug_for_micron.rs"]
mod impl_std_fmt_debug_for_micron;
#[path = "ui/impl_std_fmt_debug_for_omnibar.rs"]
mod impl_std_fmt_debug_for_omnibar;
#[path = "ui/omnibar_state_selection.rs"]
mod omnibar_state_selection;
#[path = "ui/place_prompt_hint.rs"]
mod place_prompt_hint;
#[path = "ui/recompute_suggestions.rs"]
mod recompute_suggestions;
pub use recompute_suggestions::recompute_suggestions;
#[path = "ui/recompute_suggestions_with_limit.rs"]
mod recompute_suggestions_with_limit;
pub use recompute_suggestions_with_limit::recompute_suggestions_with_limit;
#[path = "ui/normalize_address.rs"]
mod normalize_address;
pub use normalize_address::normalize_address;
#[path = "ui/themed_sheet.rs"]
mod themed_sheet;
use themed_sheet::themed_sheet;
#[path = "ui/chrome_sheet.rs"]
mod chrome_sheet;
pub(crate) use chrome_sheet::chrome_sheet;
#[path = "ui/cambium_sheet.rs"]
mod cambium_sheet;
pub(crate) use cambium_sheet::cambium_sheet;
#[path = "ui/ime_cursor_area.rs"]
mod ime_cursor_area;
pub use ime_cursor_area::ime_cursor_area;
#[path = "ui/ime_cursor_area_at.rs"]
mod ime_cursor_area_at;
pub use ime_cursor_area_at::ime_cursor_area_at;
#[path = "ui/chrome_has_content.rs"]
mod chrome_has_content;
pub use chrome_has_content::chrome_has_content;
#[path = "ui/pane_scene.rs"]
mod pane_scene;
pub use pane_scene::pane_scene;
#[path = "ui/finish_scene.rs"]
mod finish_scene;
use finish_scene::finish_scene;
#[path = "ui/impl_default_for_pane_scroll.rs"]
mod impl_default_for_pane_scroll;
#[path = "ui/pane_scroll_new.rs"]
mod pane_scroll_new;
#[path = "ui/walk_dom.rs"]
mod walk_dom;
use walk_dom::walk_dom;
#[path = "ui/content_extent.rs"]
mod content_extent;
use content_extent::content_extent;
#[path = "ui/build_livery_snapshot.rs"]
mod build_livery_snapshot;
use build_livery_snapshot::build_livery_snapshot;
#[path = "ui/paint_snapshot.rs"]
mod paint_snapshot;
use paint_snapshot::paint_snapshot;
#[path = "ui/node_transform_translation.rs"]
mod node_transform_translation;
use node_transform_translation::node_transform_translation;
#[path = "ui/accumulated_translation.rs"]
mod accumulated_translation;
use accumulated_translation::accumulated_translation;
#[path = "ui/ancestor_scroll.rs"]
mod ancestor_scroll;
use ancestor_scroll::ancestor_scroll;
#[path = "ui/snapshot_node_rect.rs"]
mod snapshot_node_rect;
use snapshot_node_rect::snapshot_node_rect;
#[path = "ui/intersect_rect.rs"]
mod intersect_rect;
use intersect_rect::intersect_rect;
#[path = "ui/snapshot_visible_rect.rs"]
mod snapshot_visible_rect;
use snapshot_visible_rect::snapshot_visible_rect;
#[path = "ui/snapshot_hit_test.rs"]
mod snapshot_hit_test;
use snapshot_hit_test::snapshot_hit_test;
#[path = "ui/clamp_scroll.rs"]
mod clamp_scroll;
use clamp_scroll::clamp_scroll;
#[path = "ui/append_scrollbars.rs"]
mod append_scrollbars;
use append_scrollbars::append_scrollbars;
#[path = "ui/clean_scale.rs"]
mod clean_scale;
pub(crate) use clean_scale::clean_scale;
#[path = "ui/logical_size.rs"]
mod logical_size;
pub(crate) use logical_size::logical_size;
#[path = "ui/composite.rs"]
mod composite;
#[path = "ui/impl_default_for_retained_layout.rs"]
mod impl_default_for_retained_layout;
#[path = "ui/retained_layout_hit_test_scrolled.rs"]
mod retained_layout_hit_test_scrolled;
#[path = "ui/retained_layout_new.rs"]
mod retained_layout_new;
#[path = "ui/retained_layout_visible_rect.rs"]
mod retained_layout_visible_rect;
use composite::composite;
#[path = "ui/composite_resources.rs"]
mod composite_resources;
use composite_resources::composite_resources;
#[path = "ui/scene_from_dom_scrolled.rs"]
mod scene_from_dom_scrolled;
pub use scene_from_dom_scrolled::scene_from_dom_scrolled;
#[path = "ui/hit_test_scrolled.rs"]
mod hit_test_scrolled;
pub use hit_test_scrolled::hit_test_scrolled;
#[path = "ui/scene_from_dom.rs"]
mod scene_from_dom;
pub use scene_from_dom::scene_from_dom;
#[path = "ui/livery_paint_list.rs"]
mod livery_paint_list;
use livery_paint_list::livery_paint_list;
#[path = "ui/node_rect.rs"]
mod node_rect;
pub(crate) use node_rect::node_rect;
#[path = "ui/hit_test.rs"]
mod hit_test;
pub(crate) use hit_test::hit_test;
#[path = "ui/impl_d_layout_dom_layout.rs"]
mod impl_d_layout_dom_layout;
#[path = "ui/impl_default_for_retained_subtree_layout.rs"]
mod impl_default_for_retained_subtree_layout;
#[path = "ui/retained_subtree_layout_selector_point.rs"]
mod retained_subtree_layout_selector_point;
#[path = "ui/subtree_node_rect.rs"]
mod subtree_node_rect;
#[path = "ui/subtree_view_a_d_new.rs"]
mod subtree_view_a_d_new;
pub(crate) use subtree_node_rect::subtree_node_rect;
#[path = "ui/custom_leaf_key.rs"]
mod custom_leaf_key;
use custom_leaf_key::custom_leaf_key;
#[path = "ui/scene_from_dom_with_leaves.rs"]
mod scene_from_dom_with_leaves;
pub use scene_from_dom_with_leaves::scene_from_dom_with_leaves;
#[path = "ui/qual.rs"]
mod qual;
use qual::qual;
#[path = "ui/cambium_shell_sheet.rs"]
mod cambium_shell_sheet;
pub(crate) use cambium_shell_sheet::cambium_shell_sheet;
#[path = "ui/max_node_matches.rs"]
mod max_node_matches;
use max_node_matches::MAX_NODE_MATCHES;
#[path = "ui/card_w.rs"]
mod card_w;
pub(crate) use card_w::CARD_W;
#[path = "ui/card_top.rs"]
mod card_top;
pub(crate) use card_top::CARD_TOP;
#[path = "ui/row_text_budget.rs"]
mod row_text_budget;
pub(crate) use row_text_budget::ROW_TEXT_BUDGET;
#[path = "ui/card_bottom_margin.rs"]
mod card_bottom_margin;
pub(crate) use card_bottom_margin::CARD_BOTTOM_MARGIN;
#[path = "ui/min_visible_rows.rs"]
mod min_visible_rows;
use min_visible_rows::MIN_VISIBLE_ROWS;
#[path = "ui/suggestion.rs"]
mod suggestion;
pub use suggestion::Suggestion;
#[path = "ui/smolweb_input_prompt.rs"]
mod smolweb_input_prompt;
pub use smolweb_input_prompt::SmolwebInputPrompt;
#[path = "ui/smolweb_submission_protocol.rs"]
mod smolweb_submission_protocol;
pub use smolweb_submission_protocol::SmolwebSubmissionProtocol;
#[path = "ui/smolweb_submission_stage.rs"]
mod smolweb_submission_stage;
pub use smolweb_submission_stage::SmolwebSubmissionStage;
#[path = "ui/micron_form_stage.rs"]
mod micron_form_stage;
pub use micron_form_stage::MicronFormStage;
#[path = "ui/micron_form_prompt.rs"]
mod micron_form_prompt;
pub use micron_form_prompt::MicronFormPrompt;
#[path = "ui/smolweb_submission_prompt.rs"]
mod smolweb_submission_prompt;
pub use smolweb_submission_prompt::SmolwebSubmissionPrompt;
#[path = "ui/smolweb_submission_result.rs"]
mod smolweb_submission_result;
pub use smolweb_submission_result::SmolwebSubmissionResult;
#[path = "ui/gemini_identity_prompt.rs"]
mod gemini_identity_prompt;
pub use gemini_identity_prompt::GeminiIdentityPrompt;
#[path = "ui/gemini_trust_prompt.rs"]
mod gemini_trust_prompt;
pub use gemini_trust_prompt::GeminiTrustPrompt;
#[path = "ui/place_prompt.rs"]
mod place_prompt;
pub use place_prompt::PlacePrompt;
#[path = "ui/omnibar_mode.rs"]
mod omnibar_mode;
pub use omnibar_mode::OmnibarMode;
