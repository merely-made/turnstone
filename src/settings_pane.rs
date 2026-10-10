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
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, button, el, setting_row,
};
use genet_scripted_dom::ScriptedDom;
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
            presentation: settings
                .theme
                .as_ref()
                .and_then(|choice| catalog.resolve(choice).ok())
                .map(|resolved| resolved.presentation),
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
        chrome.appearance.theme_presentation = self.presentation.clone();
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

fn settings_view(state: &SettingsState) -> SettingsView {
    let reference = SettingsRef(APPLICATION_REFERENCE.into());
    let body: SettingsView = match SettingsProjection::resolve(&state.provider, &reference) {
        Ok(projection) if projection.specs.is_empty() => Box::new(
            el::<_, SettingsState, ()>("div", "No settings are available for this source.")
                .attr("class", "setting-empty")
                .attr("role", "status"),
        ),
        Ok(projection) => {
            let rows = projection
                .specs
                .iter()
                .map(pane_setting_row)
                .collect::<Vec<_>>();
            Box::new(el::<_, SettingsState, ()>("div", rows))
        },
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
                el::<_, SettingsState, ()>("div", state.provider.appearance_notice())
                    .attr("class", "list-row muted")
                    .attr("role", "status"),
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
                el::<_, SettingsState, ()>(
                    "div",
                    state
                        .live_settings
                        .notice()
                        .unwrap_or_else(|| state.status.clone()),
                )
                .attr("class", "list-row muted")
                .attr("role", "status"),
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
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = SettingsState {
            provider,
            live_settings,
            status,
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = SettingsRunner::new(
            dom.clone(),
            settings_view as fn(&SettingsState) -> SettingsView,
            state,
        );
        Self {
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new(),
        }
    }

    pub fn sync(&mut self, pane_w: f32, pane_h: f32) {
        self.runner.update(|state| {
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    pub fn scene(&mut self, w: u32, h: u32) -> netrender::Scene {
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
        let mut point = None;
        for node in taproot::matching(&dom, selector) {
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
mod tests {
    use super::*;
    use layout_dom_api::LayoutDom;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "turnstone-settings-pane-{label}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn pane_renders_controls_from_setting_control_not_setting_ids() {
        let library = tempfile::tempdir().unwrap();
        let mut pane = SettingsPane::new(root("render"));
        pane.runner.update(|state| {
            state.provider = ApplicationSettingsProvider::from_settings(
                root("render"),
                ApplicationSettings::default(),
            )
            .with_theme_library(library.path().join("themes.json"));
        });
        let dom = pane.dom_ref();
        assert_eq!(dom.all_with_class(dom.document(), "setting-row").len(), 6);
        assert_eq!(dom.all_with_class(dom.document(), "setting-label").len(), 6);
        assert_eq!(dom.all_with_class(dom.document(), "setting-apply").len(), 7);
        // UI zoom and cascade budget are number controls.
        assert_eq!(dom.all_with_class(dom.document(), "slider-track").len(), 2);
        assert_eq!(dom.all_with_class(dom.document(), "toggle").len(), 1);
        // Four shared built-ins plus the Turnstone default, five mode choices,
        // and four shellbar edges are controlled choices in the actual pane.
        assert_eq!(dom.all_with_class(dom.document(), "radio").len(), 14);
    }

    #[test]
    fn actual_edit_themes_button_requests_host_work_without_changing_selection() {
        let root = tempfile::tempdir().unwrap();
        let live = LiveSettingsHandle::new(&ApplicationSettings::default());
        let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        let before = live.snapshot();
        let node = taproot::matching(
            &*pane.dom_ref(),
            &taproot::Selector::role("button").with_attr("data-action", "edit-themes"),
        )[0];
        let _: Vec<()> = pane
            .runner
            .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
        assert!(matches!(
            live.take_actions().as_slice(),
            [crate::action::Action::OpenThemeWorkshop]
        ));
        assert!(
            live.take_actions().is_empty(),
            "the host consumes each request once"
        );
        assert_eq!(
            live.snapshot(),
            before,
            "opening a workshop does not select a theme"
        );
        assert!(!pandect::application_settings_exist(root.path()));
    }

    #[test]
    fn provider_owner_is_available_to_host_after_projection_construction() {
        let pane = SettingsPane::new(root("owner"));
        assert_eq!(pane.settings().ui_zoom, 1.1);
        assert_eq!(pane.settings().theme, None);
    }

    #[test]
    fn successful_apply_publishes_a_value_snapshot_for_the_shell() {
        let settings = ApplicationSettings::default();
        let live = LiveSettingsHandle::new(&settings);
        let mut pane = SettingsPane::with_live_settings(root("live"), Some(live.clone()));
        pane.runner.update(|state| {
            apply_value(
                state,
                "chrome.shellbar.visible",
                SettingValue::Boolean(false),
            );
        });
        assert!(!live.snapshot().shellbar_visible());
        assert_eq!(live.snapshot().shellbar_edge(), ShellbarEdge::Left);
        let mut app = crate::app::App::test_stub();
        assert!(app.apply_chrome_settings_snapshot(&live.snapshot()));
        assert!(
            !app.shell_chrome_config().shellbar.visible,
            "the shell can observe the same persisted snapshot without a pane callback"
        );
    }

    #[test]
    fn saved_same_identity_waits_for_explicit_apply_and_reopens_latest_definition() {
        use tabard::{
            Theme,
            library::ThemeLibraryStore,
            theme::{
                choice::ThemeChoice,
                registry::{Mode, ThemeRegistry},
            },
        };
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("shared/themes.json");
        let mut theme = Theme::new(
            "theme:turnstone-held",
            "Held appearance",
            ThemeRegistry::default().list()[0].seeds,
        );
        theme.mode_sheets.insert(
            "dark".into(),
            vec![":root{--tabard-color-bg:#123456;}".into()],
        );
        ThemeLibraryStore::load(&library)
            .unwrap()
            .save(&[theme.clone()])
            .unwrap();
        let choice = ThemeChoice::new(theme.id.clone(), Some(Mode::Dark));
        let live = LiveSettingsHandle::new(&ApplicationSettings::default());
        let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        pane.runner.update(|state| {
            state.provider = ApplicationSettingsProvider::from_settings(
                root.path(),
                ApplicationSettings::default(),
            )
            .with_theme_library(&library);
            state.provider.apply_theme_choice(choice.clone()).unwrap();
            state.live_settings.apply_provider(&state.provider);
        });
        let applied = live.snapshot();
        let persisted = std::fs::read(pandect::application_settings_path(root.path())).unwrap();
        theme.mode_sheets.insert(
            "dark".into(),
            vec![":root{--tabard-color-bg:#654321;}".into()],
        );
        ThemeLibraryStore::load(&library)
            .unwrap()
            .save(&[theme])
            .unwrap();
        pane.reload_themes();
        assert_eq!(
            live.snapshot(),
            applied,
            "saving and refreshing a same-id definition must not implicitly apply"
        );
        assert_eq!(
            std::fs::read(pandect::application_settings_path(root.path())).unwrap(),
            persisted
        );
        // A second Settings pane shares the applied presentation rather than
        // resolving the edited library into the live shell on construction.
        let _second = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        assert_eq!(live.snapshot(), applied);
        pane.runner
            .update(|state| apply_value(state, "ui.zoom", SettingValue::Number(1.5)));
        assert_eq!(live.snapshot().presentation, applied.presentation);
        assert_eq!(live.snapshot().ui_zoom(), 1.5);
        // A new process resolves the saved definition; an explicit same-mode
        // Apply in the existing process reaches that same fresh presentation.
        let reopened = ApplicationSettingsProvider::load(root.path())
            .unwrap()
            .with_theme_library(&library);
        let restarted = LiveSettingsHandle::new(reopened.settings());
        restarted.publish_provider(&reopened);
        assert_ne!(restarted.snapshot().presentation, applied.presentation);
        pane.runner
            .update(|state| apply_value(state, "theme.mode", SettingValue::Text("dark".into())));
        assert_eq!(live.snapshot(), restarted.snapshot());
        assert_eq!(pane.settings().theme, Some(choice));
    }

    #[test]
    fn stale_settings_panes_cannot_apply_saved_definition_or_overwrite_selected_theme() {
        use tabard::{
            Theme,
            library::ThemeLibraryStore,
            theme::{
                choice::ThemeChoice,
                registry::{Mode, ThemeRegistry},
            },
        };
        for reverse in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let library = root.path().join("shared/themes.json");
            let mut theme = Theme::new(
                "theme:retained-choice",
                "Retained choice",
                ThemeRegistry::default().list()[0].seeds,
            );
            theme.mode_sheets.insert(
                "dark".into(),
                vec![":root{--tabard-color-bg:#123456;}".into()],
            );
            ThemeLibraryStore::load(&library)
                .unwrap()
                .save(&[theme.clone()])
                .unwrap();
            let live = LiveSettingsHandle::new(&ApplicationSettings::default());
            let mut first =
                SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
            let mut stale =
                SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
            for pane in [&mut first, &mut stale] {
                pane.runner.update(|state| {
                    state.provider = ApplicationSettingsProvider::from_settings(
                        root.path(),
                        ApplicationSettings::default(),
                    )
                    .with_theme_library(&library);
                });
            }
            let choice = ThemeChoice::new(theme.id.clone(), Some(Mode::Dark));
            first.runner.update(|state| {
                state.provider.apply_theme_choice(choice.clone()).unwrap();
                state.live_settings.apply_provider(&state.provider);
            });
            let applied = live.snapshot();
            let saved_choice =
                std::fs::read(pandect::application_settings_path(root.path())).unwrap();
            theme.mode_sheets.insert(
                "dark".into(),
                vec![":root{--tabard-color-bg:#654321;}".into()],
            );
            ThemeLibraryStore::load(&library)
                .unwrap()
                .save(&[theme])
                .unwrap();
            if reverse {
                stale.reload_themes();
                first.reload_themes();
            } else {
                first.reload_themes();
                stale.reload_themes();
            }
            let owner = ApplicationSettingsProvider::load(root.path())
                .unwrap()
                .with_theme_library(&library);
            live.publish_provider(&owner);
            assert_eq!(
                live.snapshot(),
                applied,
                "both retained-pane refresh orders must keep the applied definition"
            );
            assert_eq!(
                std::fs::read(pandect::application_settings_path(root.path())).unwrap(),
                saved_choice
            );
            stale
                .runner
                .update(|state| apply_value(state, "ui.zoom", SettingValue::Number(1.5)));
            assert_eq!(live.snapshot().theme_id, applied.theme_id);
            assert_eq!(live.snapshot().theme_mode, applied.theme_mode);
            assert_eq!(live.snapshot().presentation, applied.presentation);
            assert_eq!(live.snapshot().ui_zoom(), 1.5);
            assert_eq!(
                ApplicationSettingsProvider::load(root.path())
                    .unwrap()
                    .settings()
                    .theme,
                Some(choice)
            );
            stale.runner.update(|state| {
                apply_value(state, "theme.mode", SettingValue::Text("dark".into()))
            });
            assert_ne!(
                live.snapshot().presentation,
                applied.presentation,
                "only explicit Apply adopts the saved definition"
            );
        }
    }

    #[test]
    fn snapshot_projects_every_live_application_axis_to_chrome() {
        let settings = ApplicationSettings {
            theme: Some(tabard::theme::choice::ThemeChoice::new(
                "theme:night",
                Some(tabard::theme::registry::Mode::Light),
            )),
            ui_zoom: 1.75,
            shellbar_edge: ShellbarEdge::Bottom,
            shellbar_hidden: true,
            ..ApplicationSettings::default()
        };
        let mut chrome = ShellChromeConfig::default();
        ChromeSettings::from(&settings).apply_to(&mut chrome);

        assert_eq!(
            chrome.shellbar.placement,
            crate::panes::ChromePlacement::Docked(crate::panes::ChromeEdge::Bottom)
        );
        assert!(!chrome.shellbar.visible);
        assert_eq!(chrome.appearance.theme_id.as_deref(), Some("theme:night"));
        assert_eq!(chrome.appearance.theme_mode, ThemeMode::Light);
        assert_eq!(chrome.appearance.zoom(), 1.75);
    }
    #[test]
    fn painted_settings_targets_drive_physical_edit_and_apply_at_wide_and_narrow_sizes() {
        // Main acceptance requests logical 1180x800 and 640x780 windows.
        // At the native 2x scale the right-hand Settings pane has these sizes.
        for (width, height) in [(1180, 1600), (640, 1560)] {
            let root = tempfile::tempdir().unwrap();
            let live = LiveSettingsHandle::new(&ApplicationSettings::default());
            let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
            pane.runner.update(|state| {
                state.provider = ApplicationSettingsProvider::from_settings(
                    root.path(),
                    ApplicationSettings::default(),
                )
                .with_theme_library(root.path().join("themes.json"));
            });
            pane.sync(width as f32, height as f32);
            let _ = pane.scene(width, height);
            let edit = taproot::Selector::class("setting-apply")
                .with_attr("data-action", "edit-themes")
                .on_surface("settings");
            let retained = pane.selector_point(&edit).unwrap().unwrap();
            let generic = {
                let dom = pane.dom_ref();
                let sheet = crate::ui::cambium_sheet(&ShellChromeConfig::default().appearance);
                taproot::resolve(
                    &[taproot::ProbeSurface {
                        name: "settings",
                        dom: &dom,
                        rect: [0.0, 0.0, width as f32, height as f32],
                        sheet: &sheet,
                    }],
                    &edit,
                )
                .map(|hit| hit.point)
            };
            let generic_hits_edit = generic.is_some_and(|(x, y)| {
                let sheet = crate::ui::cambium_sheet(&ShellChromeConfig::default().appearance);
                let mut dom = pane.dom.borrow_mut();
                let edit_node = taproot::matching(&dom, &edit)[0];
                pane.layout
                    .hit_test_scrolled(&mut dom, &sheet, width, height, x, y, &pane.scroll)
                    == Some(edit_node)
            });
            eprintln!(
                "settings {width}x{height}: generic={generic:?} retained={retained:?} generic_hits_edit={generic_hits_edit}"
            );
            let before = live.snapshot();
            pane.click(retained.0, retained.1, width, height);
            assert!(matches!(
                live.take_actions().as_slice(),
                [crate::action::Action::OpenThemeWorkshop]
            ));
            assert_eq!(live.snapshot(), before, "Edit themes cannot apply settings");
            assert!(!pandect::application_settings_exist(root.path()));

            // Dark occurs in both theme.id and theme.mode. Reject it rather
            // than assigning the first occurrence to the wrong row.
            assert!(
                pane.selector_point(
                    &taproot::Selector::class("radio")
                        .containing("Dark")
                        .on_surface("settings")
                )
                .is_err()
            );
            assert!(
                pane.selector_point(
                    &taproot::Selector::class("setting-apply").on_surface("settings")
                )
                .is_err()
            );
            assert_eq!(
                pane.selector_point(&edit.clone().on_surface("inspector")),
                Ok(None)
            );

            fn physical_click(
                pane: &mut SettingsPane,
                selector: taproot::Selector,
                width: u32,
                height: u32,
            ) {
                let _ = pane.scene(width, height);
                let point = pane
                    .selector_point(&selector.on_surface("settings"))
                    .unwrap()
                    .expect("painted visible control");
                pane.click(point.0, point.1, width, height);
            }
            physical_click(
                &mut pane,
                taproot::Selector::class("radio").containing("High Contrast"),
                width,
                height,
            );
            assert!(
                live.snapshot().theme_id().is_none(),
                "radio is still a draft"
            );
            physical_click(
                &mut pane,
                taproot::Selector::class("setting-apply").with_attr("data-setting", "theme.id"),
                width,
                height,
            );
            assert_eq!(
                live.snapshot().theme_id(),
                Some(tabard::theme::registry::THEME_ID_HIGH_CONTRAST)
            );
            physical_click(
                &mut pane,
                taproot::Selector::class("radio").containing("High contrast light"),
                width,
                height,
            );
            assert!(
                live.snapshot().theme_mode().is_none(),
                "mode is still a draft"
            );
            physical_click(
                &mut pane,
                taproot::Selector::class("setting-apply").with_attr("data-setting", "theme.mode"),
                width,
                height,
            );
            assert_eq!(live.snapshot().theme_mode(), Some("hc_light"));
            let saved = pandect::load_application_settings(root.path())
                .unwrap()
                .unwrap();
            assert_eq!(
                saved.theme.unwrap(),
                tabard::theme::choice::ThemeChoice::new(
                    tabard::theme::registry::THEME_ID_HIGH_CONTRAST,
                    Some(tabard::theme::registry::Mode::HcLight)
                )
            );
            assert!(
                live.take_actions().is_empty(),
                "Apply cannot open the workshop"
            );
        }
    }

    #[test]
    fn mode_radio_keyboard_stays_in_its_group_and_enter_applies_only_after_tab() {
        use cambium::{Key, KeyEvent, NamedKey};
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("themes.json");
        let live = LiveSettingsHandle::new(&ApplicationSettings::default());
        let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        pane.runner.update(|state| {
            state.provider = ApplicationSettingsProvider::from_settings(
                root.path(),
                ApplicationSettings::default(),
            )
            .with_theme_library(library.clone());
        });
        // The first click selects HcLight; the second focuses the now-active
        // roving-tabindex radio, exactly as the native fixture does.
        let selector = taproot::Selector::class("radio").containing("High contrast light");
        for _ in 0..2 {
            let node = taproot::matching(&*pane.dom_ref(), &selector)[0];
            let _: Vec<()> = pane
                .runner
                .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
        }
        pane.key(KeyEvent::new(Key::Named(NamedKey::ArrowUp)));
        // Arrow navigation is a draft, not a durable change.
        assert!(pane.settings().theme.is_none());
        assert!(live.snapshot().theme_mode().is_none());
        pane.focus_traverse(true);
        {
            let dom = pane.dom_ref();
            let apply = taproot::matching(
                &*dom,
                &taproot::Selector::class("setting-apply").with_attr("data-setting", "theme.mode"),
            )[0];
            assert_eq!(pane.runner.focus(), Some(apply));
        }
        pane.key(KeyEvent::new(Key::Named(NamedKey::Enter)));
        assert_eq!(live.snapshot().theme_mode(), Some("dark"));
        assert_eq!(
            live.snapshot().theme_id(),
            None,
            "mode keyboard cannot select the built-in Dark theme row"
        );
        let persisted = ApplicationSettingsProvider::load(root.path()).unwrap();
        assert_eq!(
            persisted
                .settings()
                .theme
                .as_ref()
                .unwrap()
                .theme_mode
                .as_ref()
                .unwrap()
                .as_key(),
            "dark"
        );
        // Repeat the visible group entry, then two ArrowUp presses choose Light.
        for _ in 0..2 {
            let node = taproot::matching(&*pane.dom_ref(), &selector)[0];
            let _: Vec<()> = pane
                .runner
                .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
        }
        pane.key(KeyEvent::new(Key::Named(NamedKey::ArrowUp)));
        pane.key(KeyEvent::new(Key::Named(NamedKey::ArrowUp)));
        pane.focus_traverse(true);
        pane.key(KeyEvent::new(Key::Named(NamedKey::Enter)));
        assert_eq!(live.snapshot().theme_mode(), Some("light"));
        assert_eq!(live.snapshot().theme_id(), None);
        pane.focus_traverse(false);
        assert!(pane.runner.focus().is_some());
        pane.blur();
        assert!(pane.runner.focus().is_none());
    }
}
