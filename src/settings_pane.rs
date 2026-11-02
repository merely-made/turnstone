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
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, Keyed, LifecycleEventSink, button,
    el, status_note,
};
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::LayoutDom;
use mere_surface_api::settings::{SettingSpec, SettingValue, SettingsProjection};
use pandect::{ApplicationSettings, ShellbarEdge};
use workbench::SettingsRef;

use crate::settings_provider::SettingsBinding;
use crate::settings_provider::{
    APPLICATION_REFERENCE, ApplicationSettingsProvider, SharedApplicationSettingsOwner,
};
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

mod live_settings;
pub use live_settings::LiveSettingsHandle;
mod commands;
use commands::apply_value;

mod state;
use state::{RowState, SettingsState};

type SettingsView = Box<dyn AnyView<SettingsState, (), GenetCtx, GenetElement>>;
type SettingsRunner =
    GenetAppRunner<SettingsState, fn(&SettingsState) -> SettingsView, SettingsView, ()>;

mod composition;
#[cfg(test)]
use composition::setting_rows;

mod view;
use view::settings_view;
mod arrangement;
mod editor;
mod editor_host;
mod record_host;
mod row_commands;
mod source_sync;

pub(crate) fn validate_record(
    config: &crate::panes::PaneConfig,
    view: &crate::panes::PaneViewState,
) -> Result<(), String> {
    arrangement::validate_record(config, view)
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
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        let state = SettingsState::new(provider, live_settings, status);
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
        self.flush_editor_commands();
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
        self.flush_editor_commands();
        self.runner.update(|state| {
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
            commands::refresh_settings(state);
        });
        self.include_reactive_mutations();
    }

    pub(crate) fn include_reactive_mutations(&self) {
        let state = self.runner.state();
        if state.input_queue_error.borrow().is_some() {
            crate::reactive_render::mark_input_gap();
        }
        if let Some(queue) = &state.input_queue {
            if queue.dropped_mutation_records() > 0 || queue.generation_exhaustions() > 0 {
                crate::reactive_render::mark_input_gap();
            }
            crate::reactive_render::include_mutations(&queue.latest_mutations());
        } else {
            crate::reactive_render::mark_input_gap();
        }
    }

    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        let records = self
            .runner
            .state()
            .input_queue
            .as_ref()
            .map(|queue| queue.latest_mutations())
            .unwrap_or_default();
        let _reactive_work = crate::reactive_render::WorkScope::settings(&records);
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
        self.recompute_unsaved();
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
            self.flush_editor_commands();
        }
    }

    pub(crate) fn reload_themes(&mut self) {
        self.runner.update(|state| {
            let reload = state.owner.as_ref().map(|owner| owner.reload_themes());
            commands::refresh_settings(state);
            state.status = match reload {
                Some(Err(error)) => format!("Could not reload themes: {error:?}"),
                _ => state.provider.appearance_notice(),
            };
        });
    }

    /// Native and scenario keys share the retained controls' keyboard route.
    pub(crate) fn key(&mut self, event: cambium::KeyEvent) {
        let _: Vec<()> = self.runner.dispatch_key(event);
        self.flush_editor_commands();
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

#[cfg(test)]
#[path = "settings_pane/commands_tests.rs"]
mod commands_tests;

#[cfg(test)]
#[path = "settings_pane/composition_tests.rs"]
mod composition_tests;

#[cfg(test)]
#[path = "settings_pane/p3_tests.rs"]
mod p3_tests;

#[cfg(test)]
#[path = "settings_pane/reactive_tests.rs"]
mod reactive_tests;
