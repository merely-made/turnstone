// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn adapter_with_surfaces<R>(
        &self,
        f: impl FnOnce(&[taproot::ProbeSurface<'_>]) -> R,
    ) -> R {
        let plan = self.surface_plan();
        let mut guards: Vec<(
            &'static str,
            [f32; 4],
            std::cell::Ref<'_, genet_scripted_dom::ScriptedDom>,
        )> = Vec::new();
        let mut contributed_guards: Vec<(
            &'static str,
            [f32; 4],
            std::cell::Ref<'_, genet_scripted_dom::ScriptedDom>,
            &str,
        )> = Vec::new();
        let knot_sheet = format!(
            "{} {}",
            crate::ui::CAMBIUM_SHEET,
            crate::knot_authoring::KNOT_SHEET
        );
        let mut chrome = crate::shell_services::ShellChromeConfig::default();
        self.live_settings.snapshot().apply_to(&mut chrome);
        let settings_sheet = crate::ui::cambium_sheet(&chrome.appearance);
        for surface in &plan {
            let rect = [
                surface.rect.x,
                surface.rect.y,
                surface.rect.w,
                surface.rect.h,
            ];
            match surface.kind {
                crate::surface::SurfaceKind::Content(node) => {
                    if let Some(session) = self.content_sessions.session(&node)
                        && let Some(knot) = session
                            .as_any_ref()
                            .downcast_ref::<crate::knot_authoring::KnotDocumentSession>(
                        )
                    {
                        guards.push(("knot", rect, knot.dom_ref()));
                    }
                },
                crate::surface::SurfaceKind::Pane(id)
                    if self.renderers.inline_mount(id).is_some() => {},
                crate::surface::SurfaceKind::Pane(id) => match self.pane_content(id) {
                    Some(PaneContent::Roster) => {
                        if let Some(g) = self.renderers.roster.get(&id) {
                            guards.push(("roster", rect, g.dom_ref()));
                        }
                    },
                    Some(PaneContent::Trail) => {
                        if let Some(pane) = self.renderers.trail.get(&id) {
                            guards.push(("trail", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Inspector) => {
                        if let Some(pane) = self.renderers.inspector.get(&id) {
                            guards.push(("inspector", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Workbench) => {
                        if let Some(pane) = self.renderers.workbench.get(&id) {
                            guards.push(("workbench", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Gloss(_)) => {
                        if let Some(pane) = self.renderers.gloss.get(&id) {
                            guards.push(("gloss", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::TRANSCRIPT =>
                    {
                        if let Some(pane) = self.renderers.transcript.get(&id) {
                            guards.push(("transcript", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::SETTINGS =>
                    {
                        if let Some(pane) = self.renderers.settings.get(&id) {
                            guards.push(("settings", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::ARRANGE =>
                    {
                        if let Some(pane) = self.renderers.arrange.get(&id) {
                            guards.push(("arrange", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::PUBLISHING =>
                    {
                        if let Some(pane) = self.renderers.publish.get(&id) {
                            guards.push(("publishing", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::SHARED_KNOT =>
                    {
                        if let Some(pane) = self.renderers.shared_knot.get(&id) {
                            guards.push(("shared-knot", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
                    {
                        if let Some(pane) = self.renderers.device_receipts.get(&id) {
                            guards.push(("device-receipts", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION =>
                    {
                        if let Some(pane) = self.renderers.frozen_projection.get(&id) {
                            guards.push(("frozen-projection", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(_)) => {
                        if let Some(pane) = self.renderers.contributed.get(id) {
                            contributed_guards.push((
                                "contributed",
                                rect,
                                pane.dom_ref(),
                                pane.stylesheet(),
                            ));
                        }
                    },
                    Some(PaneContent::Overmap(_)) => {
                        if let Some(pane) = self.renderers.overmap.get(&id) {
                            guards.push(("overmap", rect, pane.dom_ref()));
                        }
                    },
                    _ => {},
                },
                _ => {},
            }
        }
        let mut surfaces: Vec<taproot::ProbeSurface> = guards
            .iter()
            .map(|(name, rect, r)| taproot::ProbeSurface {
                name,
                dom: r,
                rect: *rect,
                sheet: if *name == "knot" {
                    &knot_sheet
                } else if *name == "settings" {
                    &settings_sheet
                } else {
                    crate::ui::CAMBIUM_SHEET
                },
            })
            .collect();
        surfaces.extend(contributed_guards.iter().map(|(name, rect, dom, sheet)| {
            taproot::ProbeSurface {
                name,
                dom,
                rect: *rect,
                sheet,
            }
        }));
        // Shared forest targets use selector_target's actual scoped fragments.
        f(&surfaces)
    }

    pub(super) fn adapter_selector_target(
        &self,
        selector: &taproot::Selector,
    ) -> taproot::SelectorTarget {
        match selector.surface.as_deref() {
            Some("chrome") => self.chrome.selector_point(selector).map_or(
                taproot::SelectorTarget::Miss,
                |point| {
                    taproot::SelectorTarget::Hit(taproot::Hit {
                        surface: "chrome",
                        point,
                    })
                },
            ),
            None => self.retained_pane_selector_target(selector),
            Some("frame") => {
                let mut frames = self.outer_frames.borrow_mut();
                let Some(frame) = frames.get_mut(&0) else {
                    return taproot::SelectorTarget::Miss;
                };
                match frame.selector_point(selector) {
                    Ok(Some(point)) => taproot::SelectorTarget::Hit(taproot::Hit {
                        surface: "frame",
                        point,
                    }),
                    _ => taproot::SelectorTarget::Miss,
                }
            },
            Some("workbench") => self.retained_pane_selector_target(selector),
            Some("settings") => {
                let plan = self.surface_plan();
                let mut selected = None;
                for surface in &plan {
                    let crate::surface::SurfaceKind::Pane(id) = surface.kind else {
                        continue;
                    };
                    if !matches!(self.pane_content(id), Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::SETTINGS)
                    {
                        continue;
                    }
                    let Some(pane) = self.renderers.settings.get(&id) else {
                        continue;
                    };
                    let point = match pane.selector_point(selector) {
                        Ok(Some((x, y))) => (surface.rect.x + x, surface.rect.y + y),
                        Ok(None) => continue,
                        Err(_) => return taproot::SelectorTarget::Miss,
                    };
                    // Preserve ordinary physical routing and attribution:
                    // covered targets and duplicate panes cannot claim a hit.
                    if crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
                        .is_none_or(|hit| hit.id != surface.id)
                        || selected.is_some()
                    {
                        return taproot::SelectorTarget::Miss;
                    }
                    selected = Some(taproot::Hit {
                        surface: "settings",
                        point,
                    });
                }
                selected.map_or(taproot::SelectorTarget::Miss, taproot::SelectorTarget::Hit)
            },
            Some("inspector") => {
                let plan = self.surface_plan();
                let mut selected = None;
                for surface in &plan {
                    let crate::surface::SurfaceKind::Pane(id) = surface.kind else {
                        continue;
                    };
                    if self.pane_content(id) != Some(PaneContent::Inspector) {
                        continue;
                    }
                    let Some(pane) = self.renderers.inspector.get(&id) else {
                        continue;
                    };
                    let point = match pane.selector_point(selector) {
                        Ok(Some((x, y))) => (surface.rect.x + x, surface.rect.y + y),
                        Ok(None) => continue,
                        Err(_) => return taproot::SelectorTarget::Miss,
                    };
                    // Retained pane geometry still goes through ordinary
                    // physical routing. An overlapping toolbar or another
                    // surface must not receive an attributed Inspector click.
                    if crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
                        .is_none_or(|hit| hit.id != surface.id)
                    {
                        return taproot::SelectorTarget::Miss;
                    }
                    if selected.is_some() {
                        return taproot::SelectorTarget::Miss;
                    }
                    selected = Some(taproot::Hit {
                        surface: "inspector",
                        point,
                    });
                }
                selected.map_or(taproot::SelectorTarget::Miss, taproot::SelectorTarget::Hit)
            },
            Some("contributed") => match self.contributed_selector(selector) {
                Ok(target) => target
                    .point()
                    .map_or(taproot::SelectorTarget::Miss, |point| {
                        taproot::SelectorTarget::Hit(taproot::Hit {
                            surface: "contributed",
                            point,
                        })
                    }),
                Err(_) => taproot::SelectorTarget::Miss,
            },
            // Other ordinary pane scopes use their retained pane owner.
            Some(_) => self.retained_pane_selector_target(selector),
        }
    }

    pub(super) fn adapter_click_target(&mut self, selector: &taproot::Selector) -> Option<bool> {
        match selector.surface.as_deref() {
            None => None,
            Some("contributed") => Some(self.click_contributed(selector)),
            Some(_) => None,
        }
    }

    pub(super) fn adapter_text_target(
        &self,
        text: &str,
    ) -> Result<Option<taproot::TextTarget>, String> {
        let plan = self.surface_plan();
        let mut matches = plan.iter().filter_map(|surface| {
            let crate::surface::SurfaceKind::Content(node) = surface.kind else {
                return None;
            };
            self.content_sessions
                .session(&node)
                .and_then(|session| session.text_target(text))
                .map(|target| taproot::TextTarget {
                    anchor: (
                        surface.rect.x + target.anchor[0],
                        surface.rect.y + target.anchor[1],
                    ),
                    focus: (
                        surface.rect.x + target.focus[0],
                        surface.rect.y + target.focus[1],
                    ),
                })
        });
        let first = matches.next();
        if matches.next().is_some() {
            return Err("more than one live content surface matched".into());
        }
        Ok(first)
    }
}
