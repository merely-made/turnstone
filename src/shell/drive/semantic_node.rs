// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn assert_semantic_node(&mut self, args: &str) -> Result<(), String> {
        let (slot, selector) = if let Some(args) = args.strip_prefix("window:") {
            let (slot, selector) = args
                .split_once(char::is_whitespace)
                .ok_or("assert node window:N requires a selector")?;
            (
                slot.parse::<usize>().map_err(|_| "invalid window index")?,
                selector,
            )
        } else {
            (0, args)
        };
        let (selector, value) = selector
            .split_once(" = ")
            .map_or((selector, None), |(selector, value)| {
                (selector, Some(value))
            });
        let selector = taproot::parse_selector(selector)?;
        if matches!(selector.matcher, taproot::Match::Class(_))
            || selector.attr.is_some()
            || selector.surface.is_some()
        {
            return Err("assert node wants an authored key or role and name".into());
        }
        self.push_a11y_tree();
        let forest = if slot == 0 {
            &self.surface_a11y
        } else {
            &self
                .lens_windows
                .values()
                .find(|lens| lens.ordinal + 1 == slot)
                .ok_or("assert node window is absent")?
                .a11y_forest
        };
        let found = forest.observed_nodes().into_iter().any(|observation| {
            let node = observation.node;
            matches_node(&selector, node) && value.is_none_or(|value| node.value() == Some(value))
        });
        found
            .then_some(())
            .ok_or_else(|| format!("assert node {args}: no matching node"))
    }

    pub(super) fn content_selector_target(
        &self,
        selector: &taproot::Selector,
    ) -> taproot::SelectorTarget {
        use crate::surface::SurfaceKind;
        use taproot::{Hit, SelectorTarget};
        if selector.attr.is_some() || matches!(selector.matcher, taproot::Match::Class(_)) {
            return SelectorTarget::Miss;
        }
        let plan = self.surface_plan();
        let current = self.app.browser_command_target().map(|(node, _)| node);
        let mut hits = Vec::new();
        for observation in self.surface_a11y.observed_nodes() {
            let node = observation.node;
            let Some(bounds) = observation.bounds else {
                continue;
            };
            if observation.tree == accesskit::TreeId::ROOT
                || node.is_disabled()
                || bounds.is_empty()
                || !matches_node(selector, node)
            {
                continue;
            }
            let point = (
                ((bounds.x0 + bounds.x1) * 0.5) as f32,
                ((bounds.y0 + bounds.y1) * 0.5) as f32,
            );
            let Some(placement) = crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
            else {
                continue;
            };
            // Grafts or structural labels cannot accidentally click the page.
            if !node.supports_action(accesskit::Action::Click)
                && !node.supports_action(accesskit::Action::Focus)
            {
                continue;
            }
            if let SurfaceKind::Content(member) = placement.kind {
                hits.push((member, node.label(), point));
            }
        }
        if hits
            .iter()
            .any(|(_, label, _)| label.is_some() && *label == selector.text.as_deref())
        {
            hits.retain(|(_, label, _)| *label == selector.text.as_deref());
        }
        if hits.len() > 1
            && let Some(current) = current
        {
            hits.retain(|(member, _, _)| *member == current);
        }
        match hits.as_slice() {
            [(_, _, point)] => SelectorTarget::Hit(Hit {
                surface: "content",
                point: *point,
            }),
            _ => SelectorTarget::Miss,
        }
    }
}

fn matches_node(selector: &taproot::Selector, node: &accesskit::Node) -> bool {
    let role = match node.role() {
        accesskit::Role::SearchInput => Some("searchinput"),
        accesskit::Role::PasswordInput => Some("passwordinput"),
        role => document_session_api::a11y::accesskit_role_token(role),
    };
    selector.matches_semantics(role, node.label(), node.author_id())
}
