// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ChromeSurfaces {
    pub(crate) fn selector_point(&self, selector: &taproot::Selector) -> Option<(f32, f32)> {
        let root = self.runner.window_root(*self.projections.first()?)?;
        self.layouts
            .first()?
            .selector_point(&self.dom.borrow(), root, selector)
    }

    pub fn new() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = ChromeState {
            focus_cards: Vec::new(),
            forme_fields: Default::default(),
            peek_selected: None,
            peek_below: None,
            peek_forme: None,
            relation_targets: Vec::new(),
            menu: None,
            menu_selected: 0,
            documents_start: 0,
            relationships_start: 0,
            layout: Default::default(),
            map_active: false,
            territory: false,
            tools: Vec::new(),
            connections: Vec::new(),
            documents: Vec::new(),
            reading_stacks: Vec::new(),
            windows: vec![WindowChrome {
                primary: true,
                ..WindowChrome::default()
            }],
            open: false,
            inline_address: false,
            omnibar_input: TextInput::default(),
            rows: Vec::new(),
            omnibar_placement: ChromePlacement::Overlay,
            shellbar_placement: ChromePlacement::Docked(ChromeEdge::Right),
            shellbar_visible: true,
            zoom: 1.0,
            scope_controls: Default::default(),
            scope_crumbs: Vec::new(),
            can_back: false,
            can_forward: false,
            focused_node: None,
            focused_kept: false,
            fetching: false,
            fetch_status: None,
            focused_url: None,
            link_preview: None,
            find: cambium::ReadFind::default(),
            decision: None,
        };
        let mut runner = GenetMultiRunner::new(state);
        let primary =
            runner.push_forest_projection(dom.clone(), Box::new(chrome_logic(0)) as ChromeLogic);
        Self {
            dom,
            runner,
            projections: vec![primary],
            layouts: vec![crate::ui::RetainedSubtreeLayout::new()],
            appearance: crate::shell_services::AppearanceConfig::default(),
            card_leaves: sprigging::LeafRegistry::new(),
            card_paint: sprigging::RenderedLeaves::default(),
        }
    }

    /// Make sure a projection exists for `slot` (a lens window registering).
    pub fn ensure_slot(&mut self, slot: usize) {
        while self.projections.len() <= slot {
            let next = self.projections.len();
            let id = self.runner.push_forest_projection(
                self.dom.clone(),
                Box::new(chrome_logic(next)) as ChromeLogic,
            );
            self.projections.push(id);
            self.layouts.push(crate::ui::RetainedSubtreeLayout::new());
        }
    }

    /// How many full cascades one window-root has paid for.
    ///
    /// The retention contract in one number: it reaches 1 and stays there
    /// while the chrome is merely repainted. Exposed so a receipt can assert
    /// retention rather than infer it from a duration, which would make the
    /// guard a benchmark and therefore flaky.
    pub fn layout_rebuilds(&self, slot: usize) -> u32 {
        self.layouts.get(slot).map_or(0, |layout| layout.rebuilds())
    }

    /// Drain the shared DOM once and stale every root that reads it.
    ///
    /// This is the whole reason [`crate::ui::RetainedSubtreeLayout`] does not
    /// drain for itself. The chrome is N window-roots over ONE `ScriptedDom`,
    /// so a per-root drain would let whichever root ran first consume the
    /// batch and leave the rest painting a tree that no longer exists. Draining
    /// here, and invalidating every root when the batch is non-empty, cannot
    /// produce that split.
    ///
    /// Every entry point that reads the layout calls this, not just `sync`: a
    /// click dispatch mutates the DOM between frames, and a lens window can
    /// render in a frame where the primary did not sync.
    pub(super) fn absorb_dom_mutations(&mut self) {
        let mut mutations = Vec::new();
        self.dom.borrow_mut().drain_mutations(&mut mutations);
        if mutations.is_empty() {
            return;
        }
        for layout in &mut self.layouts {
            layout.invalidate();
        }
    }
}
