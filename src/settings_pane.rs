//! The application-settings projection.
//!
//! Turnstone owns the application settings store and its application namespace.
//! The pane obtains its rows from that provider. Cambium selects controls by
//! [`SettingControl`], never by a Turnstone setting id.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use cambium::{AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, el, setting_row};
use genet_host_api::settings::{SettingSpec, SettingValue, SettingsProjection, SettingsProvider};
use genet_scripted_dom::ScriptedDom;
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
    ui_zoom: f32,
    shellbar_edge: ShellbarEdge,
    shellbar_hidden: bool,
    /// Rounds one behavior cascade may run. Not chrome, but it rides the same
    /// live snapshot because it is the same kind of thing: an application
    /// setting the app must see change without a restart.
    cascade_budget: u32,
    /// Highest cumulative token n-gram order for the derived recall vectors.
    recall_ngram_max_order: u8,
    /// Vector weight relative to BM25. Zero keeps lexical-only recall.
    recall_vector_weight: f32,
}

impl From<&ApplicationSettings> for ChromeSettings {
    fn from(settings: &ApplicationSettings) -> Self {
        Self {
            theme_id: settings.theme_id.clone(),
            theme_mode: settings.theme_mode.clone(),
            ui_zoom: settings.ui_zoom,
            shellbar_edge: settings.shellbar_edge,
            shellbar_hidden: settings.shellbar_hidden,
            cascade_budget: settings.cascade_budget,
            recall_ngram_max_order: settings.recall_ngram_max_order,
            recall_vector_weight: settings.recall_vector_weight,
        }
    }
}

impl ChromeSettings {
    pub fn cascade_budget(&self) -> u32 {
        self.cascade_budget
    }

    pub fn recall_ngram_max_order(&self) -> u8 {
        self.recall_ngram_max_order
    }

    pub fn recall_vector_weight(&self) -> f32 {
        self.recall_vector_weight
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
    }
}

/// A cloneable live projection owned by the shell. The provider publishes only
/// after it has persisted a successful write, so consumers cannot observe a
/// value that failed to reach the application owner.
#[derive(Clone)]
pub struct LiveSettingsHandle(Rc<RefCell<ChromeSettings>>);

impl LiveSettingsHandle {
    pub fn new(settings: &ApplicationSettings) -> Self {
        Self(Rc::new(RefCell::new(ChromeSettings::from(settings))))
    }

    pub fn snapshot(&self) -> ChromeSettings {
        self.0.borrow().clone()
    }

    fn publish(&self, settings: &ApplicationSettings) {
        *self.0.borrow_mut() = ChromeSettings::from(settings);
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
            state.live_settings.publish(state.provider.settings());
            format!("Saved {setting_id}")
        }
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
        }
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
                body,
                el::<_, SettingsState, ()>("div", state.status.clone())
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
                ApplicationSettingsProvider::from_settings(
                    data_root,
                    ApplicationSettings::default(),
                ),
                format!("Could not load application settings: {error}"),
            ),
        };
        let live_settings =
            live_settings.unwrap_or_else(|| LiveSettingsHandle::new(provider.settings()));
        live_settings.publish(provider.settings());
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
        let pane = SettingsPane::new(root("render"));
        let dom = pane.dom_ref();
        assert_eq!(dom.all_with_class(dom.document(), "setting-row").len(), 8);
        assert_eq!(dom.all_with_class(dom.document(), "setting-label").len(), 8);
        assert_eq!(dom.all_with_class(dom.document(), "setting-apply").len(), 8);
        // UI zoom, cascade budget, and phrase influence are number controls.
        assert_eq!(dom.all_with_class(dom.document(), "slider-track").len(), 3);
        assert_eq!(dom.all_with_class(dom.document(), "toggle").len(), 1);
        // Theme mode (3), shellbar edge (4), and phrase order (3).
        assert_eq!(dom.all_with_class(dom.document(), "radio").len(), 10);
    }

    #[test]
    fn provider_owner_is_available_to_host_after_projection_construction() {
        let pane = SettingsPane::new(root("owner"));
        assert_eq!(pane.settings().ui_zoom, 1.1);
        assert_eq!(pane.settings().theme_id, None);
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
        assert_eq!(live.snapshot().recall_ngram_max_order(), 2);
        assert_eq!(live.snapshot().recall_vector_weight(), 0.0);
        let mut app = crate::app::App::test_stub();
        assert!(app.apply_chrome_settings_snapshot(&live.snapshot()));
        assert!(
            !app.shell_chrome_config().shellbar.visible,
            "the shell can observe the same persisted snapshot without a pane callback"
        );
    }

    #[test]
    fn snapshot_projects_every_live_application_axis_to_chrome() {
        let settings = ApplicationSettings {
            theme_id: Some("theme:night".into()),
            theme_mode: Some("light".into()),
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
}
