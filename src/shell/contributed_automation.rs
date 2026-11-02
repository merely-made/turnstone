// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Scoped selectors use the contributed provider's current retained semantics
//! and paint geometry. Delivery stays on Shell's ordinary pointer route.

use document_session_api::DocumentA11yAction;
use genet_scripted_dom::NodeId;
use layout_dom_api::LayoutDom;
use taproot::{Match, Selector};

use super::Shell;
use crate::contributed_surface::ContributedSurfacePane;
use crate::panes::{PaneId, PaneSpec};
use crate::surface::{Rect, SurfaceKind};

#[derive(Clone, Debug)]
pub(super) struct HeldClick {
    pane: PaneId,
    generation: u64,
    node: NodeId,
    selector: Selector,
    request: u64,
    trace_node: u64,
    painted_before: (f32, f32, f32, f32),
    visible_before: Option<(f32, f32, f32, f32)>,
}

pub(super) struct Target {
    identity: HeldClick,
    painted: (f32, f32, f32, f32),
    visible: Option<(f32, f32, f32, f32)>,
    placement: Rect,
}

impl Target {
    pub(super) fn pane(&self) -> PaneId {
        self.identity.pane
    }

    pub(super) fn point(&self) -> Option<(f32, f32)> {
        let (x, y, width, height) = self.visible?;
        Some((
            self.placement.x + x + width / 2.0,
            self.placement.y + y + height / 2.0,
        ))
    }

    fn fully_visible(&self) -> bool {
        self.visible == Some(self.painted)
    }
}

/// Every matching node participates in ambiguity checks, including disabled
/// matches. A disabled first target cannot silently select a second provider.
pub(super) fn select<'a>(
    selector: &Selector,
    readings: impl IntoIterator<Item = (PaneId, Rect, &'a PaneSpec, &'a ContributedSurfacePane)>,
) -> Result<Target, String> {
    let mut selected = None;
    for (pane, placement, spec, session) in readings {
        if !session.matches(&spec.kind, &spec.source) {
            continue;
        }
        let Some(projection) = session.document_projection() else {
            continue;
        };
        let dom = session.dom_ref();
        for node in taproot::matching_with_projection(&dom, selector, &projection) {
            if selected.is_some() {
                return Err("multiple contributed targets match the scoped selector".into());
            }
            let Some(semantic) = projection
                .nodes()
                .iter()
                .find(|semantic| semantic.id.get() == dom.opaque_id(node))
            else {
                continue;
            };
            let usable = !semantic.state.hidden
                && !semantic.state.disabled
                && (!matches!(selector.matcher, Match::Role(_))
                    || semantic.actions.contains(&DocumentA11yAction::Click));
            selected = Some((
                Target {
                    identity: HeldClick {
                        pane,
                        generation: session.generation(),
                        node,
                        selector: selector.clone(),
                        request: 0,
                        trace_node: dom.opaque_id(node),
                        painted_before: session.painted_rect(node).unwrap_or((0.0, 0.0, 0.0, 0.0)),
                        visible_before: session.visible_rect(node),
                    },
                    painted: session.painted_rect(node).unwrap_or((0.0, 0.0, 0.0, 0.0)),
                    visible: session.visible_rect(node),
                    placement,
                },
                usable,
            ));
        }
    }
    let (target, usable) = selected.ok_or("no current contributed semantic target")?;
    if !usable || target.painted.2 <= 0.0 || target.painted.3 <= 0.0 {
        return Err("contributed target is hidden, disabled or has no activation action".into());
    }
    Ok(target)
}

pub(super) fn revalidate<'a>(
    held: &HeldClick,
    readings: impl IntoIterator<Item = (PaneId, Rect, &'a PaneSpec, &'a ContributedSurfacePane)>,
) -> Result<Target, String> {
    let selected = select(&held.selector, readings)?;
    if selected.identity.pane != held.pane
        || selected.identity.generation != held.generation
        || selected.identity.node != held.node
    {
        return Err("held contributed target was removed, repinned or replaced".into());
    }
    if selected.visible.is_none() {
        return Err("held contributed target never came into view after scrolling".into());
    }
    Ok(selected)
}

/// Scalar correlation only. Exhaustion refuses delivery before any pointer or
/// scroll mutation; the counter never becomes another target authority.
fn next_request(current: u64) -> Option<u64> {
    current.checked_add(1)
}

impl Shell {
    pub(super) fn contributed_readings(&self) -> Vec<(PaneId, Rect, PaneSpec)> {
        self.surface_plan()
            .into_iter()
            .filter_map(|surface| {
                let SurfaceKind::Pane(pane) = surface.kind else {
                    return None;
                };
                let spec = self.contributed_pane_spec(pane, self.pane_content(pane).as_ref())?;
                Some((pane, surface.rect, spec))
            })
            .collect()
    }

    pub(super) fn contributed_selector(&self, selector: &Selector) -> Result<Target, String> {
        let readings = self.contributed_readings();
        select(
            selector,
            readings.iter().filter_map(|(pane, rect, spec)| {
                Some((*pane, *rect, spec, self.renderers.contributed.get(*pane)?))
            }),
        )
    }

    pub(super) fn click_contributed(&mut self, selector: &Selector) -> bool {
        let mut target = match self.contributed_selector(selector) {
            Ok(target) => target,
            Err(error) => {
                self.app.note(crate::observe::AppEvent::InteractionMissed {
                    what: "scoped-click",
                    target: format!("{selector:?}: {error}"),
                });
                return false;
            },
        };
        let Some(request) = next_request(self.contributed_click_request) else {
            self.app.note(crate::observe::AppEvent::InteractionMissed {
                what: "scoped-click",
                target: "contributed request counter exhausted".into(),
            });
            return false;
        };
        target.identity.request = request;
        if target.fully_visible() {
            return match self.deliver_contributed_target(&target) {
                Ok(()) => {
                    self.contributed_click_request = request;
                    true
                },
                Err(error) => {
                    self.app.note(crate::observe::AppEvent::InteractionMissed {
                        what: "scoped-click",
                        target: format!("{selector:?}: {error}"),
                    });
                    false
                },
            };
        }
        let Some(pane) = self.renderers.contributed.get_mut(target.identity.pane) else {
            return false;
        };
        if !pane.reveal(target.identity.node) {
            return false;
        }
        self.contributed_click_request = request;
        self.held_contributed_click = Some(target.identity);
        self.request_redraw();
        true
    }

    fn deliver_contributed_target(&mut self, target: &Target) -> Result<(), String> {
        let point = target.point().ok_or("contributed target is not visible")?;
        let plan = self.surface_plan();
        let hit = crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
            .ok_or("contributed target has no current surface hit")?;
        if hit.kind != SurfaceKind::Pane(target.identity.pane) {
            return Err("contributed target is covered by another surface".into());
        }
        let pane = self
            .renderers
            .contributed
            .get_mut(target.identity.pane)
            .ok_or("contributed target session disappeared")?;
        let hit_node = pane
            .hit_test(hit.local.0, hit.local.1)
            .ok_or("contributed target has no retained pointer hit")?;
        let dom = pane.dom_ref();
        let mut ancestor = Some(hit_node);
        let mut owns_hit = false;
        while let Some(node) = ancestor {
            if node == target.identity.node {
                owns_hit = true;
                break;
            }
            ancestor = dom.parent(node);
        }
        drop(dom);
        if !owns_hit {
            return Err("another retained control covers the contributed target".into());
        }
        self.deliver_press(point.0, point.1, winit::event::MouseButton::Left);
        self.deliver_release(point.0, point.1, winit::event::MouseButton::Left);
        Ok(())
    }

    /// Called only after render. Completing or failing a held click consumes
    /// this pump, so the following step observes a frame after actual delivery.
    pub(super) fn finish_contributed_click(&mut self) -> Result<bool, String> {
        let Some(held) = self.held_contributed_click.take() else {
            return Ok(false);
        };
        let readings = self.contributed_readings();
        let target = revalidate(
            &held,
            readings.iter().filter_map(|(pane, rect, spec)| {
                Some((*pane, *rect, spec, self.renderers.contributed.get(*pane)?))
            }),
        )
        .map_err(|error| format!("click {:?}: {error}", held.selector))?;
        self.deliver_contributed_target(&target)
            .map_err(|error| format!("click {:?}: {error}", held.selector))?;
        // One bounded completion record per held click, with no product text
        // or source identity. It qualifies the actual reveal/delivery path.
        eprintln!(
            "turnstone: contributed-click request={} pane={} generation={} node={} painted_before={:?} visible_before={:?} painted_after={:?} visible_after={:?}",
            held.request,
            held.pane.0,
            held.generation,
            held.trace_node,
            held.painted_before,
            held.visible_before,
            target.painted,
            target.visible,
        );
        Ok(true)
    }
}

#[cfg(test)]
#[path = "contributed_automation/tests.rs"]
mod tests;
