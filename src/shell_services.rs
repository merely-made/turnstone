//! Shell-owned command, chrome, and transcript state.
//!
//! This module is deliberately independent from `Canvas`, pane renderers, and
//! winit.  A caller hands it a [`ContextSnapshot`] when an interaction starts;
//! a transcript entry keeps that immutable snapshot even if focus changes
//! before the interaction completes.  A2 replaces Turnstone's temporary
//! session-only snapshot with the focused pane's resolved graph context.

use std::collections::{BTreeMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::action::Action;
use crate::panes::{ChromeBlueprint, ChromeEdge, ChromePlacement, PaneContext, PaneId};

/// Stable identity of a registered shell provider.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShellProviderId(String);

impl ShellProviderId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The interaction classes a provider can contribute to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellProviderKind {
    Command,
    Navigation,
    Completion,
}

/// Metadata for one registered provider.  Providers remain application-owned:
/// this registry records identity, order, and capability without giving the
/// chrome a back-door into graph state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellProviderRegistration {
    pub id: ShellProviderId,
    pub label: String,
    pub kinds: Vec<ShellProviderKind>,
}

/// The registered provider set, ordered by stable provider id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellProviderRegistry {
    providers: BTreeMap<ShellProviderId, ShellProviderRegistration>,
}

impl ShellProviderRegistry {
    pub fn register(
        &mut self,
        provider: ShellProviderRegistration,
    ) -> Option<ShellProviderRegistration> {
        self.providers.insert(provider.id.clone(), provider)
    }

    pub fn unregister(&mut self, id: &ShellProviderId) -> Option<ShellProviderRegistration> {
        self.providers.remove(id)
    }

    pub fn get(&self, id: &ShellProviderId) -> Option<&ShellProviderRegistration> {
        self.providers.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ShellProviderRegistration> {
        self.providers.values()
    }
}

/// The default input lane selected when something summons the omnibar without
/// naming a lane explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OmnibarScope {
    #[default]
    Address,
    Command,
}

/// A configurable keyboard chord for an omnibar lane.  The platform adapter
/// performs the key decoding; the configuration stays platform-neutral.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OmnibarShortcut {
    pub key: char,
    pub ctrl: bool,
    pub scope: OmnibarScope,
}

/// The omnibar's projection and input defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmnibarConfig {
    pub placement: ChromePlacement,
    pub row_limit: usize,
    pub default_scope: OmnibarScope,
    pub shortcuts: Vec<OmnibarShortcut>,
}

impl Default for OmnibarConfig {
    fn default() -> Self {
        Self {
            placement: ChromePlacement::Overlay,
            // Preserve the previous default: six node matches and, when the
            // input is address-shaped, one literal-open row.
            row_limit: 7,
            default_scope: OmnibarScope::Address,
            shortcuts: vec![
                OmnibarShortcut {
                    key: 'l',
                    ctrl: true,
                    scope: OmnibarScope::Address,
                },
                OmnibarShortcut {
                    key: 'k',
                    ctrl: true,
                    scope: OmnibarScope::Command,
                },
            ],
        }
    }
}

/// The at-rest shellbar's projection.  In the current chrome it projects the
/// location caption; navigation controls remain a later provider consumer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellbarConfig {
    pub placement: ChromePlacement,
    pub visible: bool,
}

impl Default for ShellbarConfig {
    fn default() -> Self {
        Self {
            placement: ChromePlacement::Docked(ChromeEdge::Right),
            visible: true,
        }
    }
}

/// The base color family selected for host-owned chrome and Cambium panes.
/// `System` deliberately resolves to Turnstone's established dark presentation
/// until the platform adapter exposes a live system-color preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    /// Decode the provider's durable vocabulary without exposing provider
    /// types to the shell service seam.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            _ => Self::System,
        }
    }
}

/// The running host presentation selected by Turnstone's application
/// settings. This remains a value: renderers consume it, while the settings
/// provider owns persistence and the shell owns polling/redraw.
#[derive(Clone, Debug, PartialEq)]
pub struct AppearanceConfig {
    /// A persona-synced theme selector. The current host derives a stable
    /// accent from it until a theme-pack provider is registered.
    pub theme_id: Option<String>,
    /// The base light/dark family for host-owned surfaces.
    pub theme_mode: ThemeMode,
    /// Scale for retained UI surfaces, constrained by the application store's
    /// 0.5..=3.0 setting contract.
    pub ui_zoom: f32,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme_id: None,
            theme_mode: ThemeMode::System,
            ui_zoom: 1.1,
        }
    }
}

impl AppearanceConfig {
    /// A finite, user-setting-safe scale for layout and paint consumers.
    pub fn zoom(&self) -> f32 {
        self.ui_zoom.clamp(0.5, 3.0)
    }
}

/// Bounded local retention for intentional shell history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptRetention {
    pub max_entries: usize,
}

impl Default for TranscriptRetention {
    fn default() -> Self {
        Self { max_entries: 200 }
    }
}

/// The chrome choices that can vary without becoming pane topology.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellChromeConfig {
    pub omnibar: OmnibarConfig,
    pub shellbar: ShellbarConfig,
    /// Theme and UI scale consumed by the running host presentation.
    pub appearance: AppearanceConfig,
    pub transcript_placement: ChromePlacement,
    pub transcript_retention: TranscriptRetention,
}

impl Default for ShellChromeConfig {
    fn default() -> Self {
        Self {
            omnibar: OmnibarConfig::default(),
            shellbar: ShellbarConfig::default(),
            appearance: AppearanceConfig::default(),
            transcript_placement: ChromePlacement::Hidden,
            transcript_retention: TranscriptRetention::default(),
        }
    }
}

impl ShellChromeConfig {
    /// Apply durable composition from a space blueprint while retaining
    /// application-owned row, shortcut, and retention policy.
    pub fn apply_blueprint(&mut self, blueprint: &ChromeBlueprint) {
        self.omnibar.placement = blueprint.omnibar.clone();
        self.shellbar.placement = blueprint.shellbar.clone();
        self.shellbar.visible = !matches!(blueprint.shellbar, ChromePlacement::Hidden);
        self.transcript_placement = blueprint.transcript.clone();
    }

    /// Replace the shellbar's visible edge without making settings code reach
    /// into a renderer.
    pub fn set_shellbar_edge(&mut self, edge: ChromeEdge) {
        self.shellbar.placement = ChromePlacement::Docked(edge);
    }

    pub fn projects_omnibar(&self) -> bool {
        !matches!(
            self.omnibar.placement,
            ChromePlacement::Hidden | ChromePlacement::Pane(_)
        )
    }

    pub fn projects_shellbar(&self) -> bool {
        self.shellbar.visible
            && !matches!(
                self.shellbar.placement,
                ChromePlacement::Hidden | ChromePlacement::Pane(_)
            )
    }
}

/// The pane and context frozen for one intentional interaction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContextSnapshot {
    pub pane: Option<PaneId>,
    pub context: PaneContext,
}

/// A stable correlation id for one transcript entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShellEntryId(pub u64);

/// The raw user input retained for a shell interaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellInput {
    Omnibar(String),
    /// The input was intentionally withheld by its provider.  Keep a useful
    /// label without copying a secret into the local transcript.
    Redacted {
        label: String,
    },
}

/// The resolved action behind an input.  Stored separately from `ShellInput`
/// so a transcript can repeat an action without reparsing text against a
/// changed provider set.
#[derive(Clone, Debug, PartialEq)]
pub enum ShellIntent {
    SelectNode { url: String },
    Navigate { url: String },
    Command { label: String, action: Action },
}

/// The result of an intentional interaction, correlated by [`ShellEntryId`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellOutcome {
    Pending,
    Completed { summary: String },
    Rejected { message: String },
}

/// How much of an entry can be kept locally.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EntryPrivacy {
    #[default]
    Ordinary,
    Redacted,
}

/// One bounded, local record of a user-intended command or navigation.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellEntry {
    pub id: ShellEntryId,
    pub input: ShellInput,
    pub resolved_intent: ShellIntent,
    pub target: ContextSnapshot,
    pub outcome: ShellOutcome,
    pub timestamp_ms: u64,
    pub privacy: EntryPrivacy,
}

/// The frozen payload used by repeat actions.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellReplay {
    pub input: ShellInput,
    pub intent: ShellIntent,
    pub target: ContextSnapshot,
    pub privacy: EntryPrivacy,
}

/// A bounded transcript store.  It is a shell ledger, not a projection of the
/// frame-drained [`crate::observe::AppEvent`] stream.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellTranscript {
    next_id: u64,
    retention: TranscriptRetention,
    entries: VecDeque<ShellEntry>,
}

impl Default for ShellTranscript {
    fn default() -> Self {
        Self {
            next_id: 1,
            retention: TranscriptRetention::default(),
            entries: VecDeque::new(),
        }
    }
}

impl ShellTranscript {
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &ShellEntry> {
        self.entries.iter()
    }

    pub fn entry(&self, id: ShellEntryId) -> Option<&ShellEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn set_retention(&mut self, retention: TranscriptRetention) {
        self.retention = TranscriptRetention {
            max_entries: retention.max_entries.max(1),
        };
        while self.entries.len() > self.retention.max_entries {
            self.entries.pop_front();
        }
    }

    pub fn record(
        &mut self,
        input: ShellInput,
        intent: ShellIntent,
        target: ContextSnapshot,
        privacy: EntryPrivacy,
    ) -> ShellEntryId {
        let id = ShellEntryId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let input = match privacy {
            EntryPrivacy::Ordinary => input,
            EntryPrivacy::Redacted => ShellInput::Redacted {
                label: match input {
                    ShellInput::Omnibar(_) => "omnibar input".to_string(),
                    ShellInput::Redacted { label } => label,
                },
            },
        };
        self.entries.push_back(ShellEntry {
            id,
            input,
            resolved_intent: intent,
            target,
            outcome: ShellOutcome::Pending,
            timestamp_ms: now_millis(),
            privacy,
        });
        while self.entries.len() > self.retention.max_entries {
            self.entries.pop_front();
        }
        id
    }

    pub fn complete(&mut self, id: ShellEntryId, outcome: ShellOutcome) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.outcome = outcome;
        true
    }

    pub fn replay(&self, id: ShellEntryId) -> Option<ShellReplay> {
        let entry = self.entry(id)?;
        Some(ShellReplay {
            input: entry.input.clone(),
            intent: entry.resolved_intent.clone(),
            target: entry.target,
            privacy: entry.privacy,
        })
    }
}

/// The shell service boundary consumed by app actions and rendered by chrome.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellServices {
    chrome: ShellChromeConfig,
    providers: ShellProviderRegistry,
    transcript: ShellTranscript,
    omnibar_context: Option<ContextSnapshot>,
    requested_context: Option<ContextSnapshot>,
}

impl Default for ShellServices {
    fn default() -> Self {
        let mut providers = ShellProviderRegistry::default();
        providers.register(ShellProviderRegistration {
            id: ShellProviderId::new("turnstone.actions"),
            label: "Turnstone actions".to_string(),
            kinds: vec![ShellProviderKind::Command],
        });
        providers.register(ShellProviderRegistration {
            id: ShellProviderId::new("turnstone.navigation"),
            label: "Turnstone navigation".to_string(),
            kinds: vec![ShellProviderKind::Navigation, ShellProviderKind::Completion],
        });
        Self {
            chrome: ShellChromeConfig::default(),
            providers,
            transcript: ShellTranscript::default(),
            omnibar_context: None,
            requested_context: None,
        }
    }
}

impl ShellServices {
    pub fn chrome(&self) -> &ShellChromeConfig {
        &self.chrome
    }

    pub fn set_chrome(&mut self, chrome: ShellChromeConfig) {
        self.transcript.set_retention(chrome.transcript_retention);
        self.chrome = chrome;
    }

    pub fn providers(&self) -> &ShellProviderRegistry {
        &self.providers
    }

    pub fn providers_mut(&mut self) -> &mut ShellProviderRegistry {
        &mut self.providers
    }

    pub fn transcript(&self) -> &ShellTranscript {
        &self.transcript
    }

    /// Begin an omnibar interaction and freeze the caller-resolved target.
    pub fn begin_omnibar(&mut self, target: ContextSnapshot) {
        self.omnibar_context = Some(target);
    }

    pub fn close_omnibar(&mut self) {
        self.omnibar_context = None;
    }

    pub fn record_omnibar(
        &mut self,
        input: ShellInput,
        intent: ShellIntent,
        fallback_target: ContextSnapshot,
        privacy: EntryPrivacy,
    ) -> ShellEntryId {
        let target = self.omnibar_context.unwrap_or(fallback_target);
        self.omnibar_context = None;
        self.transcript.record(input, intent, target, privacy)
    }

    pub fn complete(&mut self, id: ShellEntryId, outcome: ShellOutcome) -> bool {
        self.transcript.complete(id, outcome)
    }

    /// Resolve a repeat request to a frozen intent and target, then record the
    /// new attempt as a separately correlated entry.
    pub fn repeat(&mut self, id: ShellEntryId) -> Option<(ShellEntryId, ShellReplay)> {
        let replay = self.transcript.replay(id)?;
        let repeated = self.transcript.record(
            replay.input.clone(),
            replay.intent.clone(),
            replay.target,
            replay.privacy,
        );
        Some((repeated, replay))
    }

    /// Ask the eventual focused-context router to open an entry's original
    /// target.  A2 consumes this value when graph runtimes become pane-scoped.
    pub fn request_target(&mut self, id: ShellEntryId) -> bool {
        let Some(entry) = self.transcript.entry(id) else {
            return false;
        };
        self.requested_context = Some(entry.target);
        true
    }

    pub fn take_requested_context(&mut self) -> Option<ContextSnapshot> {
        self.requested_context.take()
    }

    pub fn shortcut_scope(&self, key: &str, ctrl: bool) -> Option<OmnibarScope> {
        let key = key.chars().next()?;
        self.chrome
            .omnibar
            .shortcuts
            .iter()
            .find(|shortcut| shortcut.key.eq_ignore_ascii_case(&key) && shortcut.ctrl == ctrl)
            .map(|shortcut| shortcut.scope)
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(pane: u64) -> ContextSnapshot {
        ContextSnapshot {
            pane: Some(PaneId(pane)),
            ..ContextSnapshot::default()
        }
    }

    #[test]
    fn transcript_binds_the_target_at_open_and_replay_keeps_it() {
        let mut services = ShellServices::default();
        services.begin_omnibar(context(7));
        let original = services.record_omnibar(
            ShellInput::Omnibar("mere://field-notes".into()),
            ShellIntent::Navigate {
                url: "mere://field-notes".into(),
            },
            context(99),
            EntryPrivacy::Ordinary,
        );
        services.complete(
            original,
            ShellOutcome::Completed {
                summary: "opened mere://field-notes".into(),
            },
        );

        let (repeat, replay) = services.repeat(original).expect("entry replays");
        assert_eq!(replay.target, context(7));
        assert_eq!(
            services.transcript().entry(repeat).unwrap().target,
            context(7)
        );
        assert!(matches!(
            services.transcript().entry(repeat).unwrap().outcome,
            ShellOutcome::Pending
        ));
    }

    #[test]
    fn retention_and_redaction_bound_the_local_ledger() {
        let mut transcript = ShellTranscript::default();
        transcript.set_retention(TranscriptRetention { max_entries: 2 });
        for input in ["one", "two", "three"] {
            transcript.record(
                ShellInput::Omnibar(input.into()),
                ShellIntent::Navigate { url: input.into() },
                ContextSnapshot::default(),
                EntryPrivacy::Redacted,
            );
        }
        let entries: Vec<_> = transcript.entries().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, ShellEntryId(2));
        assert!(matches!(entries[0].input, ShellInput::Redacted { .. }));
        assert_eq!(entries[0].privacy, EntryPrivacy::Redacted);
    }

    #[test]
    fn blueprint_composition_changes_only_projection_choices() {
        let mut chrome = ShellChromeConfig::default();
        let blueprint = ChromeBlueprint {
            omnibar: ChromePlacement::Docked(ChromeEdge::Bottom),
            shellbar: ChromePlacement::Hidden,
            transcript: ChromePlacement::Floating,
            status: ChromePlacement::Docked(ChromeEdge::Bottom),
        };
        chrome.apply_blueprint(&blueprint);
        assert_eq!(chrome.omnibar.placement, blueprint.omnibar);
        assert!(!chrome.shellbar.visible);
        assert_eq!(chrome.transcript_placement, ChromePlacement::Floating);
        assert_eq!(chrome.omnibar.row_limit, 7, "layout does not own policy");
    }

    #[test]
    fn default_providers_and_shortcuts_are_registered_without_a_canvas() {
        let mut services = ShellServices::default();
        let commands = ShellProviderId::new("turnstone.actions");
        assert!(services.providers().get(&commands).is_some());
        assert_eq!(
            services.shortcut_scope("K", true),
            Some(OmnibarScope::Command)
        );
        services.providers_mut().unregister(&commands);
        assert!(services.providers().get(&commands).is_none());
    }
}
