// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The application-settings projection.
//!
//! Turnstone owns the application settings store and its application namespace.
//! The pane obtains its rows from that provider. Cambium selects controls by
//! [`SettingControl`], never by a Turnstone setting id.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, Keyed, button, el, setting_row,
    status_note,
};
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::LayoutDom;
use mere_surface_api::settings::{SettingSpec, SettingValue, SettingsProjection, SettingsProvider};
use pandect::{ApplicationSettings, ShellbarEdge};
use workbench::SettingsRef;

use crate::settings_provider::{APPLICATION_REFERENCE, ApplicationSettingsProvider};
use crate::shell_services::{ShellChromeConfig, ThemeMode};

/// The part of the application owner that the shell can observe while it is
/// running. It is intentionally a value snapshot, rather than a callback into
/// the settings pane or a route to the graph canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct ChromeSettings {
    theme_id: Option<String>,
    theme_mode: Option<String>,
    presentation: Option<tabard::ThemePresentation>,
    /// Under `System`, `presentation` is the dark reading and this the light
    /// one; the shell picks between them by the OS preference.
    light_presentation: Option<tabard::ThemePresentation>,
    typography: tabard::Typography,
    interface_font_properties: String,
    ui_zoom: f32,
    shellbar_edge: ShellbarEdge,
    shellbar_hidden: bool,
    /// Rounds one behavior cascade may run. Not chrome, but it rides the same
    /// live snapshot because it is the same kind of thing: an application
    /// setting the app must see change without a restart.
    cascade_budget: u32,
}

impl From<&ApplicationSettings> for ChromeSettings {
    fn from(settings: &ApplicationSettings) -> Self {
        let catalog = crate::appearance::ThemeCatalog {
            path: PathBuf::new(),
            registry: Default::default(),
            error: None,
        };
        Self::from_catalog(settings, &catalog)
    }
}

impl ChromeSettings {
    fn from_catalog(
        settings: &ApplicationSettings,
        catalog: &crate::appearance::ThemeCatalog,
    ) -> Self {
        let resolve = |mode: Option<tabard::theme::registry::Mode>| {
            let choice = settings.theme.as_ref()?;
            let choice = tabard::theme::choice::ThemeChoice {
                theme_mode: mode.or_else(|| choice.theme_mode.clone()),
                ..choice.clone()
            };
            catalog.resolve(&choice).ok()
        };
        let system = settings
            .theme
            .as_ref()
            .is_some_and(|choice| choice.theme_mode.is_none());
        let (resolved, light) = if system {
            use tabard::theme::registry::Mode;
            (resolve(Some(Mode::Dark)), resolve(Some(Mode::Light)))
        } else {
            (resolve(None), None)
        };
        Self {
            theme_id: settings
                .theme
                .as_ref()
                .map(|theme| theme.theme_id.clone())
                .filter(|id| !id.is_empty()),
            theme_mode: settings
                .theme
                .as_ref()
                .and_then(|theme| theme.theme_mode.as_ref())
                .map(|mode| mode.as_key()),
            typography: resolved
                .as_ref()
                .map(|r| r.theme.typography.clone())
                .unwrap_or_default(),
            interface_font_properties: settings
                .theme
                .as_ref()
                .map(|choice| catalog.interface_font_properties(&choice.fonts))
                .unwrap_or_default(),
            presentation: resolved.map(|r| r.presentation),
            light_presentation: light.map(|r| r.presentation),
            ui_zoom: settings.ui_zoom,
            shellbar_edge: settings.shellbar_edge,
            shellbar_hidden: settings.shellbar_hidden,
            cascade_budget: settings.cascade_budget,
        }
    }

    pub(crate) fn from_provider(provider: &ApplicationSettingsProvider) -> Self {
        Self::from_catalog(provider.settings(), provider.catalog())
    }

    pub fn cascade_budget(&self) -> u32 {
        self.cascade_budget
    }

    pub fn theme_id(&self) -> Option<&str> {
        self.theme_id.as_deref()
    }

    pub fn theme_mode(&self) -> Option<&str> {
        self.theme_mode.as_deref()
    }

    pub fn ui_zoom(&self) -> f32 {
        self.ui_zoom
    }

    pub fn shellbar_edge(&self) -> ShellbarEdge {
        self.shellbar_edge
    }

    pub fn shellbar_visible(&self) -> bool {
        !self.shellbar_hidden
    }

    /// Project the application-owned snapshot onto the shell's typed chrome
    /// value. This is deliberately a one-way value conversion: the provider
    /// never reaches into a renderer or a graph runtime.
    pub(crate) fn apply_to(&self, chrome: &mut ShellChromeConfig) {
        chrome.shellbar.placement =
            crate::panes::ChromePlacement::Docked(match self.shellbar_edge {
                ShellbarEdge::Left => crate::panes::ChromeEdge::Left,
                ShellbarEdge::Right => crate::panes::ChromeEdge::Right,
                ShellbarEdge::Top => crate::panes::ChromeEdge::Top,
                ShellbarEdge::Bottom => crate::panes::ChromeEdge::Bottom,
            });
        chrome.shellbar.visible = self.shellbar_visible();
        chrome.appearance.theme_id = self.theme_id.clone();
        chrome.appearance.theme_mode = ThemeMode::from_setting(self.theme_mode());
        chrome.appearance.ui_zoom = self.ui_zoom();
        chrome.appearance.theme_presentation = match &self.light_presentation {
            Some(light) if !chrome.appearance.system_dark => Some(light.clone()),
            _ => self.presentation.clone(),
        };
        chrome.appearance.theme_typography = self.typography.clone();
        chrome.appearance.interface_font_properties = self.interface_font_properties.clone();
    }
}

/// A cloneable live projection owned by the shell. The provider publishes only
/// after it has persisted a successful write, so consumers cannot observe a
/// value that failed to reach the application owner.
#[derive(Clone)]
pub struct LiveSettingsHandle(Rc<RefCell<LiveSettingsState>>);

struct LiveSettingsState {
    snapshot: ChromeSettings,
    catalog_applied: bool,
    actions: std::collections::VecDeque<crate::action::Action>,
    notice: Option<String>,
}

impl LiveSettingsHandle {
    pub fn new(settings: &ApplicationSettings) -> Self {
        Self(Rc::new(RefCell::new(LiveSettingsState {
            snapshot: ChromeSettings::from(settings),
            catalog_applied: false,
            actions: Default::default(),
            notice: None,
        })))
    }

    pub fn snapshot(&self) -> ChromeSettings {
        self.0.borrow().snapshot.clone()
    }

    fn publish(&self, settings: &ApplicationSettings) {
        let mut state = self.0.borrow_mut();
        state.snapshot = ChromeSettings::from(settings);
        state.catalog_applied = false;
    }

    pub(crate) fn report_status(&self, status: Option<String>) {
        self.0.borrow_mut().notice = status;
    }

    fn notice(&self) -> Option<String> {
        self.0.borrow().notice.clone()
    }

    pub(crate) fn request_action(&self, action: crate::action::Action) {
        self.0.borrow_mut().actions.push_back(action);
    }

    pub(crate) fn take_actions(&self) -> Vec<crate::action::Action> {
        self.0.borrow_mut().actions.drain(..).collect()
    }

    pub(crate) fn publish_provider(&self, provider: &ApplicationSettingsProvider) {
        let mut state = self.0.borrow_mut();
        let mut next = ChromeSettings::from_provider(provider);
        // Library refresh and unrelated setting writes must keep the applied
        // definition, even when an editor saved new bytes under the same id.
        if state.catalog_applied {
            next.theme_id = state.snapshot.theme_id.clone();
            next.theme_mode = state.snapshot.theme_mode.clone();
            next.presentation = state.snapshot.presentation.clone();
            next.light_presentation = state.snapshot.light_presentation.clone();
            next.typography = state.snapshot.typography.clone();
            next.interface_font_properties = state.snapshot.interface_font_properties.clone();
        }
        state.snapshot = next;
        state.catalog_applied = true;
    }

    pub(crate) fn apply_provider(&self, provider: &ApplicationSettingsProvider) {
        let mut state = self.0.borrow_mut();
        state.snapshot = ChromeSettings::from_provider(provider);
        state.catalog_applied = true;
    }
}

/// The pane's own state. In-progress edits are *not* here: each row's draft
/// lives inside its `cambium::setting_row` component, and only an applied
/// [`SettingValue`] reaches this state through the provider.
struct SettingsState {
    provider: ApplicationSettingsProvider,
    live_settings: LiveSettingsHandle,
    status: String,
    viewport_w: f32,
    viewport_h: f32,
}

type SettingsView = Box<dyn AnyView<SettingsState, (), GenetCtx, GenetElement>>;
type SettingsRunner =
    GenetAppRunner<SettingsState, fn(&SettingsState) -> SettingsView, SettingsView, ()>;

fn apply_value(state: &mut SettingsState, setting_id: &str, value: SettingValue) {
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    state.status = match state.provider.apply(&reference, setting_id, value) {
        Ok(()) => {
            if matches!(setting_id, "theme.id" | "theme.mode") {
                state.live_settings.apply_provider(&state.provider);
            } else {
                state.live_settings.publish_provider(&state.provider);
            }
            state.live_settings.report_status(None);
            format!("Saved {setting_id}")
        },
        Err(error) => format!("Could not save {setting_id}: {error:?}"),
    };
}

/// One provider spec as a Cambium row. The draft is the component's; this
/// pane only sees the applied value, which it forwards to the provider under
/// the id it passed in.
fn pane_setting_row(spec: &SettingSpec) -> SettingsView {
    let setting_id = spec.id.clone();
    let label = format!(
        "{} · {:?} · {:?} · {:?}",
        spec.label, spec.scope, spec.movement, spec.mutability
    );
    Box::new(setting_row(
        spec,
        label,
        move |state: &mut SettingsState, value: SettingValue| {
            apply_value(state, &setting_id, value);
        },
    ))
}

/// Keep each draft attached to the setting identity that owns it when the
/// provider inserts or reorders rows. A positional sequence would transfer a
/// retained row's component state to whichever spec occupies that index next.
fn setting_rows(specs: &[SettingSpec]) -> SettingsView {
    let rows = specs
        .iter()
        .map(|spec| (spec.id.clone(), pane_setting_row(spec)))
        .collect::<Vec<_>>();
    Box::new(el::<_, SettingsState, ()>("div", Keyed::new(rows)))
}

fn settings_view(state: &SettingsState) -> SettingsView {
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    let body: SettingsView = match SettingsProjection::resolve(&state.provider, &reference) {
        Ok(projection) if projection.specs.is_empty() => {
            Box::new(status_note::<SettingsState, ()>(
                "setting-empty",
                "No settings are available for this source.",
            ))
        },
        Ok(projection) => setting_rows(&projection.specs),
        Err(error) => Box::new(
            el::<_, SettingsState, ()>("div", format!("Settings are unavailable: {error:?}"))
                .attr("class", "setting-error")
                .attr("role", "alert"),
        ),
    };
    Box::new(
        el::<_, SettingsState, ()>(
            "div",
            (
                el::<_, SettingsState, ()>("div", "Application settings")
                    .attr("class", "list-section-title"),
                el::<_, SettingsState, ()>("div", APPLICATION_REFERENCE)
                    .attr("class", "list-row muted"),
                status_note::<SettingsState, ()>(
                    "list-row muted",
                    state.provider.appearance_notice(),
                ),
                button(
                    "Edit themes",
                    |state: &mut SettingsState, _: cambium::PointerClick| {
                        state
                            .live_settings
                            .request_action(crate::action::Action::OpenThemeWorkshop);
                    },
                )
                .attr("class", "setting-apply")
                .attr("data-action", "edit-themes"),
                body,
                status_note::<SettingsState, ()>(
                    "list-row muted",
                    state
                        .live_settings
                        .notice()
                        .unwrap_or_else(|| state.status.clone()),
                ),
            ),
        )
        .attr("class", "pane")
        .attr(
            "style",
            format!(
                "width: {}px; height: {}px;",
                state.viewport_w, state.viewport_h
            ),
        ),
    )
}

/// A retained application-settings projection over Turnstone's provider.
pub struct SettingsPane {
    dom: DomHandle,
    runner: SettingsRunner,
    scroll: crate::ui::PaneScroll,
    /// Kept across frames. The sheet here is derived from the live
    /// appearance, so an appearance change rebuilds and everything else
    /// reuses; see [`crate::ui::RetainedLayout`].
    layout: crate::ui::RetainedLayout,
    // Drop the runner and its widget resources before final mount custody.
    mount: Option<cambium::RetainedMount>,
}

impl SettingsPane {
    pub fn new(data_root: PathBuf) -> Self {
        Self::with_live_settings(data_root, None)
    }

    /// Construct a pane connected to the shell's value-facing settings seam.
    /// This carries settings updates outward without importing the shell or its
    /// graph runtime into the provider or pane.
    pub fn with_live_settings(
        data_root: PathBuf,
        live_settings: Option<LiveSettingsHandle>,
    ) -> Self {
        Self::new_in(None, data_root, live_settings)
    }
    pub(crate) fn new_in(
        mount: Option<cambium::RetainedMount>,
        data_root: PathBuf,
        live_settings: Option<LiveSettingsHandle>,
    ) -> Self {
        let (provider, status) = match ApplicationSettingsProvider::load(&data_root) {
            Ok(provider) => (provider, String::new()),
            Err(error) => (
                ApplicationSettingsProvider::from_failed_load(data_root, error.to_string()),
                format!("Could not load application settings: {error}"),
            ),
        };
        let live_settings =
            live_settings.unwrap_or_else(|| LiveSettingsHandle::new(provider.settings()));
        live_settings.publish_provider(&provider);
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        let state = SettingsState {
            provider,
            live_settings,
            status,
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = SettingsRunner::new_at(
            dom.clone(),
            mount
                .as_ref()
                .map_or_else(|| dom.borrow().document(), cambium::RetainedMount::node),
            settings_view as fn(&SettingsState) -> SettingsView,
            state,
        );
        Self {
            mount: mount.clone(),
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new_at(
                mount.as_ref().map(cambium::RetainedMount::node),
            ),
        }
    }

    pub(crate) fn inline_focus(&self) -> Option<genet_scripted_dom::NodeId> {
        self.runner.focus()
    }
    pub(crate) fn focus_inline(&mut self, node: genet_scripted_dom::NodeId) -> bool {
        let Some(mount) = &self.mount else {
            return false;
        };
        if !cambium_rootstock::WindowDom::new(&self.dom.borrow(), mount.node()).contains(node) {
            return false;
        }
        self.runner.set_focus(Some(node));
        true
    }
    pub(crate) fn inline_selector(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        self.layout.selector_point(&self.dom.borrow(), selector)
    }
    pub(crate) fn key_inline(&mut self, event: cambium::KeyEvent) {
        let _: Vec<()> = self.runner.dispatch_key(event);
    }
    pub(crate) fn blur_inline(&mut self) {
        self.runner.set_focus(None);
    }
    pub(crate) fn inline_accepts_text(&self) -> bool {
        self.layout
            .editable_focus(&self.dom.borrow(), self.runner.focus())
    }
    pub(crate) fn inline_cursor_area(&self) -> Option<(f32, f32, f32, f32)> {
        self.layout
            .cursor_area(&self.dom.borrow(), self.runner.focus())
    }
    pub(crate) fn inline_mount(&self) -> Option<cambium::RetainedMount> {
        self.mount.clone()
    }
    pub(crate) fn unbind_inline(&mut self) {
        self.layout.unbind_inline();
    }
    pub(crate) fn bind_inline(&mut self, layout: &crate::ui::InlineLayoutHandle) {
        if let Some(mount) = &self.mount {
            self.layout.bind_inline(layout, mount);
        }
    }
    pub fn sync(&mut self, pane_w: f32, pane_h: f32) {
        self.runner.update(|state| {
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        self.layout.set_scale(scale);
        let snapshot = self.runner.state().live_settings.snapshot();
        let mut chrome = ShellChromeConfig::default();
        snapshot.apply_to(&mut chrome);
        let sheet = crate::ui::cambium_sheet(&chrome.appearance);
        self.layout
            .scene_scrolled(&mut self.dom.borrow_mut(), &sheet, w, h, &mut self.scroll)
    }

    /// Wheel delta from the shell.
    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll.nudge(dx, dy);
    }

    /// Whether the overlay bars still need repainting as they fade.
    pub fn bars_visible(&mut self) -> bool {
        self.scroll.bars_visible()
    }

    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    /// Resolve controls from the retained fragments used by paint and clicks.
    /// A fresh generic probe layout does not share their shaped text geometry.
    pub(crate) fn selector_point(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        if !selector.matches_surface("settings") {
            return Ok(None);
        }
        let dom = self.dom.borrow();
        if !self.layout.has_current_snapshot(&dom) {
            return Err("retained Settings snapshot is unavailable or stale");
        }
        let mut point = None;
        for node in self.layout.matching_nodes(&dom, selector) {
            let Some((x, y, width, height)) = self.layout.visible_rect(&dom, node) else {
                continue;
            };
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            if point.is_some() {
                return Err("multiple visible Settings targets match the selector");
            }
            point = Some((x + width / 2.0, y + height / 2.0));
        }
        Ok(point)
    }

    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) {
        let snapshot = self.runner.state().live_settings.snapshot();
        let mut chrome = ShellChromeConfig::default();
        snapshot.apply_to(&mut chrome);
        let sheet = crate::ui::cambium_sheet(&chrome.appearance);
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            &sheet,
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        if let Some(node) = hit {
            let _: Vec<()> = self
                .runner
                .dispatch_click(node, cambium::PointerClick::at((x, y)));
        }
    }

    pub(crate) fn reload_themes(&mut self) {
        self.runner.update(|state| {
            state.provider.reload_themes();
            state.status = state.provider.appearance_notice();
        });
    }

    /// Native and scenario keys share the retained controls' keyboard route.
    pub(crate) fn key(&mut self, event: cambium::KeyEvent) {
        let _: Vec<()> = self.runner.dispatch_key(event);
    }

    pub(crate) fn focus_traverse(&mut self, forward: bool) {
        self.runner.focus_traverse(forward);
    }

    pub(crate) fn blur(&mut self) {
        self.runner.set_focus(None);
    }

    pub fn settings(&self) -> &ApplicationSettings {
        self.runner.state().provider.settings()
    }
}

#[cfg(test)]
#[path = "settings_pane/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "settings_pane/identity_tests.rs"]
mod identity_tests;
