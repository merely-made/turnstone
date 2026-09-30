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
    fn contributed_readings(&self) -> Vec<(PaneId, Rect, PaneSpec)> {
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
mod tests {
    use super::*;
    use crate::contributed_surface::tests::{provider, root_text, source};
    use crate::contributed_surface::{ContributedSurfaceSessions, SurfaceProviderRegistry};
    use crate::panes::{ContextBinding, PaneConfig, PaneKindId, PaneSource, SourceRef};
    use accesskit::{Action, Role};
    use layout_dom_api::{LayoutDomMut, QualName};

    fn spec(id: u64) -> PaneSpec {
        PaneSpec {
            id: PaneId(id),
            kind: PaneKindId::new("fake"),
            source: source("fake.v1"),
            context: ContextBinding::Own,
            config: PaneConfig::empty("test.empty"),
        }
    }

    fn registry() -> SurfaceProviderRegistry {
        let mut registry = SurfaceProviderRegistry::new();
        registry
            .register_provider(provider("fake", "fake.v1", "fake.surface"))
            .unwrap();
        registry
    }

    fn attr(pane: &ContributedSurfacePane, node: NodeId, name: &str, value: &str) {
        pane.session().dom().borrow_mut().set_attribute(
            node,
            QualName::new(None, "".into(), name.into()),
            value,
        );
    }

    fn named(pane: &mut ContributedSurfacePane, far: bool) -> NodeId {
        pane.scene(320, 180, 1.0);
        let button = pane.session().root();
        let dom = pane.session().dom();
        let mut dom = dom.borrow_mut();
        if !far {
            let child = dom.dom_children(button).next().unwrap();
            dom.set_text(child, "Restore");
        }
        let label = dom.create_element(QualName::new(None, "".into(), "span".into()));
        dom.set_attribute(
            label,
            QualName::new(None, "".into(), "id".into()),
            "reference",
        );
        let text = dom.create_text(if far { "Far" } else { "Reset all controls" });
        dom.append_child(label, text);
        let document = dom.document();
        dom.append_child(document, label);
        drop(dom);
        attr(pane, button, "class", "reset-control");
        attr(pane, button, "aria-label", "Raw alias");
        attr(pane, button, "aria-labelledby", "reference");
        attr(
            pane,
            button,
            "style",
            if far {
                "position:absolute;left:10px;top:600px;width:120px;height:32px;padding:0;border:0"
            } else {
                "position:absolute;left:10px;top:35px;width:120px;height:32px;padding:0;border:0"
            },
        );
        pane.scene(320, 180, 1.0);
        button
    }

    fn reading<'a>(
        spec: &'a PaneSpec,
        pane: &'a ContributedSurfacePane,
    ) -> [(PaneId, Rect, &'a PaneSpec, &'a ContributedSurfacePane); 1] {
        [(spec.id, Rect::new(100.0, 40.0, 320.0, 180.0), spec, pane)]
    }

    #[test]
    fn scoped_request_counter_refuses_exhaustion_without_reusing_identity() {
        assert_eq!(next_request(0), Some(1));
        assert_eq!(next_request(2), Some(3));
        assert_eq!(next_request(u64::MAX - 1), Some(u64::MAX));
        assert_eq!(next_request(u64::MAX), None);
    }

    #[test]
    fn referenced_name_selection_and_platform_action_route_share_the_live_button() {
        let registry = registry();
        let spec = spec(1);
        let mut sessions = ContributedSurfaceSessions::default();
        let pane = sessions.resolve(&spec, &registry).unwrap();
        let button = named(pane, false);
        let selector = Selector::role("button")
            .containing("Reset all controls")
            .on_surface("contributed");
        let target = select(&selector, reading(&spec, pane)).unwrap();
        assert_eq!(target.identity.node, button);
        for wrong in ["Raw alias", "Restore"] {
            assert!(
                select(
                    &Selector::role("button").containing(wrong),
                    reading(&spec, pane)
                )
                .is_err()
            );
        }
        let (tree, actions) = pane.accessibility_tree().unwrap();
        let (platform, control) = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button)
            .unwrap();
        assert_eq!(control.label(), Some("Reset all controls"));
        assert_eq!(actions[platform], target.identity.node);
        let point = target.point().unwrap();
        pane.click(point.0 - target.placement.x, point.1 - target.placement.y);
        assert!(root_text(&pane.session().dom(), button).contains("count:1"));
        pane.scene(320, 180, 1.0);
        assert!(pane.accessibility_action(Action::Click, button).is_some());
        assert!(root_text(&pane.session().dom(), button).contains("count:2"));
    }

    #[test]
    fn first_layout_miss_and_explicit_dom_filters_keep_their_meanings() {
        let registry = registry();
        let spec = spec(1);
        let mut sessions = ContributedSurfaceSessions::default();
        let pane = sessions.resolve(&spec, &registry).unwrap();
        assert!(select(&Selector::role("button"), reading(&spec, pane)).is_err());
        let button = named(pane, false);
        attr(pane, button, "data-key", "stable");
        assert_eq!(
            select(
                &Selector::class("reset-control")
                    .containing("Restore")
                    .with_attr("data-key", "stable"),
                reading(&spec, pane)
            )
            .unwrap()
            .identity
            .node,
            button
        );
        assert!(
            select(
                &Selector::class("reset-control").containing("Reset all controls"),
                reading(&spec, pane)
            )
            .is_err()
        );
        assert!(
            select(
                &Selector::class("reset-control").with_attr("data-key", "absent"),
                reading(&spec, pane)
            )
            .is_err()
        );
    }

    #[test]
    fn below_fold_click_waits_for_new_paint_then_lands_once() {
        let registry = registry();
        let spec = spec(1);
        let mut sessions = ContributedSurfaceSessions::default();
        let pane = sessions.resolve(&spec, &registry).unwrap();
        let button = named(pane, true);
        let selector = Selector::role("button").containing("Far");
        let target = select(&selector, reading(&spec, pane)).unwrap();
        assert!(target.visible.is_none());
        assert!(pane.reveal(button));
        assert!(
            revalidate(&target.identity, reading(&spec, pane)).is_err(),
            "scroll cannot deliver before paint"
        );
        assert!(root_text(&pane.session().dom(), button).contains("count:0"));
        pane.scene(320, 180, 1.0);
        let target = revalidate(&target.identity, reading(&spec, pane)).unwrap();
        let point = target.point().unwrap();
        pane.click(point.0 - target.placement.x, point.1 - target.placement.y);
        assert!(root_text(&pane.session().dom(), button).contains("count:1"));
        assert!(pane.scroll().offset().1 > 0.0);
    }

    #[test]
    fn hidden_renamed_disabled_and_removed_pending_targets_never_activate() {
        for change in ["hidden", "rename", "disabled", "remove"] {
            let registry = registry();
            let spec = spec(1);
            let mut sessions = ContributedSurfaceSessions::default();
            let pane = sessions.resolve(&spec, &registry).unwrap();
            let button = named(pane, true);
            let held = select(
                &Selector::role("button").containing("Far"),
                reading(&spec, pane),
            )
            .unwrap()
            .identity;
            pane.reveal(button);
            match change {
                "hidden" => attr(pane, button, "aria-hidden", "true"),
                "rename" => attr(pane, button, "aria-labelledby", "missing-reference"),
                "disabled" => attr(pane, button, "disabled", ""),
                "remove" => {
                    pane.session().dom().borrow_mut().remove(button);
                    assert!(revalidate(&held, reading(&spec, pane)).is_err());
                    continue;
                },
                _ => unreachable!(),
            }
            pane.scene(320, 180, 1.0);
            assert!(revalidate(&held, reading(&spec, pane)).is_err(), "{change}");
            if change == "disabled" {
                assert!(pane.accessibility_action(Action::Click, button).is_none());
                assert!(root_text(&pane.session().dom(), button).contains("count:0"));
            }
        }
    }

    #[test]
    fn viewport_reveal_cannot_activate_a_target_outside_an_ancestor_clip() {
        let registry = registry();
        let spec = spec(1);
        let mut sessions = ContributedSurfaceSessions::default();
        let pane = sessions.resolve(&spec, &registry).unwrap();
        let button = named(pane, false);
        let dom = pane.session().dom();
        let mut dom = dom.borrow_mut();
        let clip = dom.create_element(QualName::new(None, "".into(), "div".into()));
        dom.set_attribute(
            clip,
            QualName::new(None, "".into(), "style".into()),
            "position:absolute;left:0px;top:0px;width:150px;height:20px;overflow:hidden",
        );
        let document = dom.document();
        dom.append_child(document, clip);
        dom.append_child(clip, button);
        drop(dom);
        pane.scene(320, 180, 1.0);
        let held = select(
            &Selector::role("button").containing("Reset all controls"),
            reading(&spec, pane),
        )
        .unwrap()
        .identity;
        pane.reveal(button);
        pane.scene(320, 180, 1.0);
        assert!(revalidate(&held, reading(&spec, pane)).is_err());
        assert_eq!(root_text(&pane.session().dom(), button), "Restore");
    }

    #[test]
    fn replacement_and_removal_recreation_reject_reused_local_dom_ids() {
        for remove in [false, true] {
            let registry = registry();
            let mut spec = spec(1);
            let mut sessions = ContributedSurfaceSessions::default();
            let pane = sessions.resolve(&spec, &registry).unwrap();
            named(pane, true);
            let held = select(
                &Selector::role("button").containing("Far"),
                reading(&spec, pane),
            )
            .unwrap()
            .identity;
            if remove {
                sessions.remove(spec.id);
            } else {
                let PaneSource::Fixed(SourceRef::External { payload, .. }) = &mut spec.source
                else {
                    unreachable!()
                };
                payload.payload = serde_json::json!({"replacement":true});
                assert!(
                    revalidate(&held, reading(&spec, sessions.get(spec.id).unwrap())).is_err(),
                    "repin is rejected before the renderer replaces its session"
                );
            }
            let pane = sessions.resolve(&spec, &registry).unwrap();
            named(pane, true);
            pane.reveal(pane.session().root());
            pane.scene(320, 180, 1.0);
            let replacement = select(&held.selector, reading(&spec, pane)).unwrap();
            assert_eq!(
                replacement.identity.node.local_index(),
                held.node.local_index(),
                "fresh provider DOMs deliberately reuse local allocation indices"
            );
            assert_ne!(
                replacement.identity.node, held.node,
                "arena identity remains intact"
            );
            assert_ne!(replacement.identity.generation, held.generation);
            assert!(revalidate(&held, reading(&spec, pane)).is_err());
            // Isolate the admission guard even if a carrier were to resolve an
            // old local index to this arena's live node. It still has no right
            // to activate a newly admitted session under the old generation.
            let mut translated_old_target = held.clone();
            translated_old_target.node = replacement.identity.node;
            assert!(revalidate(&translated_old_target, reading(&spec, pane)).is_err());
            assert!(root_text(&pane.session().dom(), pane.session().root()).contains("count:0"));
        }
    }

    #[test]
    fn two_contributed_panes_with_equal_names_and_local_ids_are_ambiguous() {
        let registry = registry();
        let left = spec(1);
        let right = spec(2);
        let mut sessions = ContributedSurfaceSessions::default();
        named(sessions.resolve(&left, &registry).unwrap(), false);
        named(sessions.resolve(&right, &registry).unwrap(), false);
        let left_pane = sessions.get(left.id).unwrap();
        let right_pane = sessions.get(right.id).unwrap();
        assert_eq!(
            left_pane.session().root().local_index(),
            right_pane.session().root().local_index()
        );
        assert_ne!(left_pane.session().root(), right_pane.session().root());
        let readings = [
            (left.id, Rect::new(0.0, 0.0, 320.0, 180.0), &left, left_pane),
            (
                right.id,
                Rect::new(320.0, 0.0, 320.0, 180.0),
                &right,
                right_pane,
            ),
        ];
        assert!(
            select(
                &Selector::role("button").containing("Reset all controls"),
                readings
            )
            .err()
            .unwrap()
            .contains("multiple")
        );
    }
}
