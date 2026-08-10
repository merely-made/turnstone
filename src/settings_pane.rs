//! The application-settings projection.
//!
//! Turnstone owns the application settings store and its application namespace.
//! The pane obtains its rows from that provider. Cambium selects controls by
//! [`SettingControl`], never by a Turnstone setting id.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, RadioGroup, Slider, TextInput,
    clickable, el, lens, radio_group, slider, text, text_field_typed, toggle,
};
use genet_host_api::settings::{
    SettingControl, SettingSpec, SettingValue, SettingsProjection, SettingsProvider,
};
use genet_host_api::tile::SettingsRef;
use genet_layout::{IncrementalLayout, ScrollOffsets};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::LayoutDom;
use session_runtime::{ApplicationSettings, ShellbarEdge};

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
}

impl From<&ApplicationSettings> for ChromeSettings {
    fn from(settings: &ApplicationSettings) -> Self {
        Self {
            theme_id: settings.theme_id.clone(),
            theme_mode: settings.theme_mode.clone(),
            ui_zoom: settings.ui_zoom,
            shellbar_edge: settings.shellbar_edge,
            shellbar_hidden: settings.shellbar_hidden,
        }
    }
}

impl ChromeSettings {
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

struct SettingsState {
    provider: ApplicationSettingsProvider,
    live_settings: LiveSettingsHandle,
    text_inputs: HashMap<String, TextInput>,
    number_inputs: HashMap<String, Slider>,
    toggles: HashMap<String, bool>,
    choices: HashMap<String, RadioGroup>,
    status: String,
    viewport_w: f32,
    viewport_h: f32,
}

type SettingsView = Box<dyn AnyView<SettingsState, (), GenetCtx, GenetElement>>;
type SettingsRunner =
    GenetAppRunner<SettingsState, fn(&SettingsState) -> SettingsView, SettingsView, ()>;

fn number_range(min: Option<f64>, max: Option<f64>) -> (f64, f64) {
    let min = min.unwrap_or(0.0);
    let max = max
        .filter(|max| max.is_finite() && *max > min)
        .unwrap_or(min + 1.0);
    (min, max)
}

fn number_fraction(value: f64, min: Option<f64>, max: Option<f64>) -> f32 {
    let (min, max) = number_range(min, max);
    ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
}

fn number_value(fraction: f32, min: Option<f64>, max: Option<f64>) -> f64 {
    let (min, max) = number_range(min, max);
    min + f64::from(fraction.clamp(0.0, 1.0)) * (max - min)
}

fn slider_for(spec: &SettingSpec, min: Option<f64>, max: Option<f64>, step: Option<f64>) -> Slider {
    let value = match &spec.value {
        SettingValue::Number(value) => *value,
        _ => number_range(min, max).0,
    };
    let (low, high) = number_range(min, max);
    let range = high - low;
    let step = step.map(|step| (step / range) as f32).unwrap_or(0.01);
    Slider::new(number_fraction(value, min, max))
        .with_label(spec.label.clone())
        .with_steps(step, (step * 5.0).max(0.1))
}

fn text_for(spec: &SettingSpec) -> TextInput {
    let value = match &spec.value {
        SettingValue::Text(value) => value.clone(),
        _ => String::new(),
    };
    TextInput::new(value)
}

fn toggle_for(spec: &SettingSpec) -> bool {
    matches!(&spec.value, SettingValue::Boolean(true))
}

fn choice_for(
    spec: &SettingSpec,
    options: &[genet_host_api::settings::SettingOption],
) -> RadioGroup {
    let selected = match &spec.value {
        SettingValue::Text(value) => options
            .iter()
            .position(|option| option.value == *value)
            .unwrap_or(0),
        _ => 0,
    };
    RadioGroup::new(selected).with_label(spec.label.clone())
}

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

fn apply_text(setting_id: String) -> impl Fn(&mut SettingsState, cambium::PointerClick) {
    move |state, _| {
        let value = state
            .text_inputs
            .get(&setting_id)
            .map(|input| input.text().to_owned())
            .unwrap_or_default();
        apply_value(state, &setting_id, SettingValue::Text(value));
    }
}

fn apply_number(
    setting_id: String,
    min: Option<f64>,
    max: Option<f64>,
) -> impl Fn(&mut SettingsState, cambium::PointerClick) {
    move |state, _| {
        let fraction = state
            .number_inputs
            .get(&setting_id)
            .map(|slider| slider.value)
            .unwrap_or_default();
        apply_value(
            state,
            &setting_id,
            SettingValue::Number(number_value(fraction, min, max)),
        );
    }
}

fn apply_toggle(setting_id: String) -> impl Fn(&mut SettingsState, cambium::PointerClick) {
    move |state, _| {
        let value = state.toggles.get(&setting_id).copied().unwrap_or_default();
        apply_value(state, &setting_id, SettingValue::Boolean(value));
    }
}

fn apply_choice(
    setting_id: String,
    options: Vec<genet_host_api::settings::SettingOption>,
) -> impl Fn(&mut SettingsState, cambium::PointerClick) {
    move |state, _| {
        let selected = state
            .choices
            .get(&setting_id)
            .map(|choice| choice.selected)
            .unwrap_or_default();
        let value = options
            .get(selected)
            .or_else(|| options.first())
            .map(|option| option.value.clone())
            .unwrap_or_default();
        apply_value(state, &setting_id, SettingValue::Text(value));
    }
}

fn setting_label(spec: &SettingSpec) -> SettingsView {
    Box::new(
        el::<_, SettingsState, ()>(
            "div",
            format!(
                "{} · {:?} · {:?} · {:?}",
                spec.label, spec.scope, spec.movement, spec.mutability
            ),
        )
        .attr("class", "setting-label"),
    )
}

fn apply_button(
    setting_id: String,
    action: impl Fn(&mut SettingsState, cambium::PointerClick) + 'static,
) -> SettingsView {
    Box::new(clickable(
        el::<_, SettingsState, ()>("button", text("Apply"))
            .attr("class", "setting-apply")
            .attr("data-setting", setting_id),
        action,
    ))
}

fn setting_row(spec: &SettingSpec) -> SettingsView {
    let label = setting_label(spec);
    let setting_id = spec.id.clone();
    let control: SettingsView = match &spec.control {
        SettingControl::Text if matches!(&spec.value, SettingValue::Text(_)) => {
            let field_id = setting_id.clone();
            let field_spec = spec.clone();
            let field = Box::new(lens(
                |input: &mut TextInput| text_field_typed(input),
                move |state: &mut SettingsState| {
                    state
                        .text_inputs
                        .entry(field_id.clone())
                        .or_insert_with(|| text_for(&field_spec))
                },
            )) as SettingsView;
            let apply = apply_button(setting_id.clone(), apply_text(setting_id));
            Box::new(el::<_, SettingsState, ()>("div", (field, apply)))
        }
        SettingControl::Number { min, max, step }
            if matches!(&spec.value, SettingValue::Number(_)) =>
        {
            let slider_id = setting_id.clone();
            let slider_spec = spec.clone();
            let (min, max, step) = (*min, *max, *step);
            let control = Box::new(lens(
                |control: &mut Slider| slider(control),
                move |state: &mut SettingsState| {
                    state
                        .number_inputs
                        .entry(slider_id.clone())
                        .or_insert_with(|| slider_for(&slider_spec, min, max, step))
                },
            )) as SettingsView;
            let apply = apply_button(setting_id.clone(), apply_number(setting_id, min, max));
            Box::new(el::<_, SettingsState, ()>("div", (control, apply)))
        }
        SettingControl::Toggle if matches!(&spec.value, SettingValue::Boolean(_)) => {
            let toggle_id = setting_id.clone();
            let toggle_spec = spec.clone();
            let control = Box::new(lens(
                |checked: &mut bool| toggle(*checked),
                move |state: &mut SettingsState| {
                    state
                        .toggles
                        .entry(toggle_id.clone())
                        .or_insert_with(|| toggle_for(&toggle_spec))
                },
            )) as SettingsView;
            let apply = apply_button(setting_id.clone(), apply_toggle(setting_id));
            Box::new(el::<_, SettingsState, ()>("div", (control, apply)))
        }
        SettingControl::Choice { options } if matches!(&spec.value, SettingValue::Text(_)) => {
            let choice_id = setting_id.clone();
            let choice_spec = spec.clone();
            let options = options.clone();
            let display_options = options.clone();
            let state_options = options.clone();
            let control = Box::new(lens(
                move |choice: &mut RadioGroup| {
                    let labels: Vec<_> = display_options
                        .iter()
                        .map(|option| option.label.as_str())
                        .collect();
                    radio_group(choice, &labels)
                },
                move |state: &mut SettingsState| {
                    state
                        .choices
                        .entry(choice_id.clone())
                        .or_insert_with(|| choice_for(&choice_spec, &state_options))
                },
            )) as SettingsView;
            let apply = apply_button(setting_id.clone(), apply_choice(setting_id, options));
            Box::new(el::<_, SettingsState, ()>("div", (control, apply)))
        }
        _ => Box::new(
            el::<_, SettingsState, ()>("div", "Unsupported control/value pair")
                .attr("class", "setting-unsupported"),
        ),
    };

    Box::new(
        el::<_, SettingsState, ()>("div", (label, control))
            .attr("class", "setting-row")
            .attr("data-setting", spec.id.clone()),
    )
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
            let rows = projection.specs.iter().map(setting_row).collect::<Vec<_>>();
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
            text_inputs: HashMap::new(),
            number_inputs: HashMap::new(),
            toggles: HashMap::new(),
            choices: HashMap::new(),
            status,
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = SettingsRunner::new(
            dom.clone(),
            settings_view as fn(&SettingsState) -> SettingsView,
            state,
        );
        Self { dom, runner }
    }

    pub fn sync(&mut self, pane_w: f32, pane_h: f32) {
        self.runner.update(|state| {
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    pub fn scene(&self, w: u32, h: u32) -> netrender::Scene {
        let snapshot = self.runner.state().live_settings.snapshot();
        let mut chrome = ShellChromeConfig::default();
        snapshot.apply_to(&mut chrome);
        let sheet = crate::ui::cambium_sheet(&chrome.appearance);
        crate::ui::scene_from_dom(&self.dom.borrow(), &sheet, w, h)
    }

    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) {
        let snapshot = self.runner.state().live_settings.snapshot();
        let mut chrome = ShellChromeConfig::default();
        snapshot.apply_to(&mut chrome);
        let sheet = crate::ui::cambium_sheet(&chrome.appearance);
        let hit = {
            let dom = self.dom.borrow();
            let layout = IncrementalLayout::new(&*dom, &[&sheet], w as f32, h as f32);
            let scroll = ScrollOffsets::<NodeId>::default();
            layout.hit_test(&*dom, x, y, &scroll)
        };
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
        assert_eq!(dom.all_with_class(dom.document(), "setting-row").len(), 5);
        assert_eq!(dom.all_with_class(dom.document(), "setting-label").len(), 5);
        assert_eq!(dom.all_with_class(dom.document(), "setting-apply").len(), 5);
        assert_eq!(dom.all_with_class(dom.document(), "slider-track").len(), 1);
        assert_eq!(dom.all_with_class(dom.document(), "toggle").len(), 1);
        assert_eq!(dom.all_with_class(dom.document(), "radio").len(), 7);
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
