// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ChromeSurfaces {
    /// Route a click at window-local `(x, y)` into `slot`'s chrome: hit-test
    /// that window-root's own subtree layout, dispatch through the runner,
    /// return what bubbled (a row commit).
    pub fn click(&mut self, slot: usize, x: f32, y: f32, w: u32, h: u32) -> Vec<ChromeIntent> {
        let Some(&id) = self.projections.get(slot) else {
            return Vec::new();
        };
        self.absorb_dom_mutations();
        let hit = {
            let Some(root) = self.runner.window_root(id) else {
                return Vec::new();
            };
            let sheet = crate::ui::chrome_sheet(&self.appearance);
            let scale = self.window_scale(slot);
            let dom = self.dom.borrow();
            let Some(layout) = self.layouts.get_mut(slot) else {
                return Vec::new();
            };
            layout.set_scale(scale);
            layout.hit_test(&dom, root, &sheet, w, h, x, y)
        };
        let had_menu = slot == 0 && self.runner.state().menu.is_some();
        let mut intents = match hit {
            Some(node) => self
                .runner
                .dispatch_click(id, node, PointerClick::at((x, y))),
            None => Vec::new(),
        };
        if intents
            .iter()
            .any(|intent| !matches!(intent, ChromeIntent::MenuChanged))
        {
            self.runner.update(|state| state.menu = None);
            // Focus may emit a view-only hint before the actual click. The
            // pointer host consumes one intent, so keep the admitted action
            // ahead of ephemeral chrome updates.
            intents.retain(|intent| !matches!(intent, ChromeIntent::MenuChanged));
        }
        if had_menu && intents.is_empty() {
            self.dismiss_menu();
            intents.push(ChromeIntent::MenuChanged);
        }
        intents
    }

    /// Screen attachments use the same measured surface aperture as rendering
    /// and input, and the viewport stashed by that exact graph pane.
    pub(crate) fn sync_forme_fields(
        &mut self,
        app: &App,
        slot: usize,
        surfaces: &[crate::surface::Surface],
    ) {
        let chrome_scale = app.shell_chrome_config().layout_scale(slot);
        let entries = surfaces
            .iter()
            .filter_map(|surface| {
                let crate::surface::SurfaceKind::Graph(pane) = surface.kind else {
                    return None;
                };
                let target = app.forme_field_target(pane)?;
                let region = app.graph_views.forme_region(pane)?;
                let bounds = region.screen_bounds(app.graph_views.viewport(pane)?)?;
                let scale = app.pane_layout_scale(pane) / chrome_scale;
                let x = surface.rect.x / chrome_scale;
                let y = surface.rect.y / chrome_scale;
                let aperture = [
                    x,
                    y,
                    x + surface.rect.w / chrome_scale,
                    y + surface.rect.h / chrome_scale,
                ];
                let place = app.scope_place();
                let field = cambium::ScopeField {
                    label: "Reading layout".into(),
                    bounds: [
                        x + bounds[0] * scale,
                        y + bounds[1] * scale,
                        x + bounds[2] * scale,
                        y + bounds[3] * scale,
                    ],
                    aperture,
                    can_enter: place.graph != target.graph
                        || place.level != crate::scope_navigation::ScopeLevel::Forme(region.id),
                    scale: self.runner.state().zoom,
                };
                field.placement()?;
                Some(FormeFieldControl {
                    pane,
                    field,
                    target,
                })
            })
            .collect::<Vec<_>>();
        if self.runner.state().forme_fields.get(&slot) == Some(&entries) {
            return;
        }
        self.runner.update(|state| {
            state.forme_fields.insert(slot, entries);
        });
    }

    pub(crate) fn forme_field_control_count(&self, slot: usize) -> usize {
        if !field_controls_visible(self.runner.state()) {
            return 0;
        }
        self.runner
            .state()
            .forme_fields
            .get(&slot)
            .map_or(0, Vec::len)
    }

    pub(crate) fn dismiss_menu(&mut self) -> bool {
        if self.runner.state().menu.is_none() {
            return false;
        }
        self.runner.update(|state| state.menu = None);
        true
    }

    pub(crate) fn has_menu(&self) -> bool {
        self.runner.state().menu.is_some()
    }

    pub(crate) fn focus_cards_visible(&self) -> bool {
        context_cards_visible(self.runner.state())
    }

    /// Deliver a key to the open find field and return what the field's
    /// shared key map reported. Focus returns to the field first, so a click
    /// on a neighbouring control does not strand the typing.
    pub(crate) fn find_key(&mut self, event: cambium::KeyEvent) -> Vec<cambium::ReadEvent> {
        let id = self.projections[0];
        let field = {
            let dom = self.dom.borrow();
            self.runner
                .window_root(id)
                .and_then(|root| dom.all_with_class(root, "read-query").into_iter().next())
        };
        if let Some(field) = field.filter(|field| self.runner.focus(id) != Some(*field)) {
            self.runner.set_focus(id, Some(field));
        }
        self.runner
            .dispatch_key(id, event)
            .into_iter()
            .filter_map(|intent| match intent {
                ChromeIntent::Find(event) => Some(event),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn focus_card_key(&mut self, event: cambium::KeyEvent) -> Vec<ChromeIntent> {
        let id = self.projections[0];
        let inside = {
            let dom = self.dom.borrow();
            let root = self.runner.window_root(id).unwrap();
            self.runner.focus(id).is_some_and(|focus| {
                dom.all_with_class(root, "chrome-focus-cards")
                    .into_iter()
                    .any(|deck| dom.all_with_class(deck, "shell-control").contains(&focus))
            })
        };
        if !inside {
            let first = {
                let dom = self.dom.borrow();
                let root = self.runner.window_root(id).unwrap();
                dom.all_with_class(root, "cambium-focus-card")
                    .into_iter()
                    .rev()
                    .find_map(|card| dom.all_with_class(card, "shell-control").first().copied())
            };
            self.runner.set_focus(id, first);
        }
        self.runner.dispatch_key(id, event)
    }

    /// Menu keys use the same retained buttons and click dispatch as a pointer.
    pub(crate) fn menu_key(&mut self, key: &str) -> Option<Vec<ChromeIntent>> {
        self.runner.state().menu?;
        if key == "escape" {
            self.dismiss_menu();
            return Some(Vec::new());
        }
        if !matches!(key, "up" | "down" | "left" | "right" | "enter") {
            return None;
        }
        // Keyboard activation targets the retained control itself. Measuring
        // every menu row with a separate full cascade made arrow navigation
        // increasingly expensive as the shared shell gained more controls.
        let targets = {
            let dom = self.dom.borrow();
            let root = self.runner.window_root(self.projections[0])?;
            let menu = dom.all_with_class(root, "chrome-menu").into_iter().next()?;
            dom.all_with_class(menu, "shell-control")
                .into_iter()
                .filter(|node| {
                    dom.attribute(
                        *node,
                        &layout_dom_api::Namespace::from(""),
                        &layout_dom_api::LocalName::from("aria-disabled"),
                    ) != Some("true")
                })
                .collect::<Vec<_>>()
        };
        if targets.is_empty() {
            return Some(Vec::new());
        }
        let selected = self.runner.state().menu_selected.min(targets.len() - 1);
        if key == "enter" {
            let intents = self.runner.dispatch_click(
                self.projections[0],
                targets[selected],
                PointerClick::at((0.0, 0.0)),
            );
            if intents
                .iter()
                .any(|intent| !matches!(intent, ChromeIntent::MenuChanged))
            {
                self.runner.update(|state| state.menu = None);
            }
            return Some(intents);
        }
        let next = if matches!(key, "up" | "left") {
            (selected + targets.len() - 1) % targets.len()
        } else {
            (selected + 1) % targets.len()
        };
        self.runner.update(|state| state.menu_selected = next);
        Some(Vec::new())
    }
}
