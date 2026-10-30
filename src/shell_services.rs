// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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

/// The at-rest shellbar's projection.  In the current chrome it projects the
/// location caption; navigation controls remain a later provider consumer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellbarConfig {
    pub placement: ChromePlacement,
    pub visible: bool,
}

/// The base color family selected for host-owned chrome and Cambium panes.
/// `System` follows the OS light/dark preference the shell observes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
    HcLight,
    HcDark,
    Custom(String),
}

/// The running host presentation selected by Turnstone's application
/// settings. This remains a value: renderers consume it, while the settings
/// provider owns persistence and the shell owns polling/redraw.
#[derive(Clone, Debug, PartialEq)]
pub struct AppearanceConfig {
    /// The requested persona-synced identity; missing definitions keep it
    /// intact while shared resolution supplies a visible fallback.
    pub theme_id: Option<String>,
    /// The base light/dark family for host-owned surfaces.
    pub theme_mode: ThemeMode,
    /// Scale for retained UI surfaces, constrained by the application store's
    /// 0.5..=3.0 setting contract.
    pub ui_zoom: f32,
    /// Shared derived roles or exact authored CSS for host-owned surfaces.
    pub theme_presentation: Option<tabard::ThemePresentation>,
    /// Optional authored role declarations for the shared outer pane furniture.
    pub theme_typography: tabard::Typography,
    /// Host-resolved Interface family requests; availability is never persisted.
    pub interface_font_properties: String,
    /// The OS light/dark preference `System` follows. A host observation,
    /// never persisted; dark until the first window reports one.
    pub system_dark: bool,
}

/// Bounded local retention for intentional shell history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptRetention {
    pub max_entries: usize,
}

/// The chrome choices that can vary without becoming pane topology.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellChromeConfig {
    pub layout: crate::chrome_layout::ChromeLayout,
    pub omnibar: OmnibarConfig,
    pub shellbar: ShellbarConfig,
    /// Theme and UI scale consumed by the running host presentation.
    pub appearance: AppearanceConfig,
    /// Live native caption reservations in each window's physical pixels.
    /// Host observations only; never written into durable settings.
    pub titlebar_insets: BTreeMap<usize, cambium_rootstock::TitlebarInsets>,
    /// Each window's layout scale (its device scale factor): its surfaces are
    /// laid out at `physical / scale` and rasterized at `scale`.
    pub layout_scales: BTreeMap<usize, f32>,
    pub transcript_placement: ChromePlacement,
    pub transcript_retention: TranscriptRetention,
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

/// The shell service boundary consumed by app actions and rendered by chrome.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellServices {
    chrome: ShellChromeConfig,
    providers: ShellProviderRegistry,
    transcript: ShellTranscript,
    omnibar_context: Option<ContextSnapshot>,
    requested_context: Option<ContextSnapshot>,
}

#[cfg(test)]
#[path = "shell_services/tests.rs"]
mod tests;

#[path = "shell_services/appearance_config_tabard_mode.rs"]
mod appearance_config_tabard_mode;
#[path = "shell_services/impl_default_for_appearance_config.rs"]
mod impl_default_for_appearance_config;
#[path = "shell_services/impl_default_for_omnibar_config.rs"]
mod impl_default_for_omnibar_config;
#[path = "shell_services/impl_default_for_shell_chrome_config.rs"]
mod impl_default_for_shell_chrome_config;
#[path = "shell_services/impl_default_for_shell_services.rs"]
mod impl_default_for_shell_services;
#[path = "shell_services/impl_default_for_shell_transcript.rs"]
mod impl_default_for_shell_transcript;
#[path = "shell_services/impl_default_for_shellbar_config.rs"]
mod impl_default_for_shellbar_config;
#[path = "shell_services/impl_default_for_transcript_retention.rs"]
mod impl_default_for_transcript_retention;
#[path = "shell_services/now_millis.rs"]
mod now_millis;
#[path = "shell_services/shell_chrome_config_layout_scale.rs"]
mod shell_chrome_config_layout_scale;
#[path = "shell_services/shell_provider_id_new.rs"]
mod shell_provider_id_new;
#[path = "shell_services/shell_provider_registry_register.rs"]
mod shell_provider_registry_register;
#[path = "shell_services/shell_services_chrome.rs"]
mod shell_services_chrome;
#[path = "shell_services/shell_transcript_entries.rs"]
mod shell_transcript_entries;
#[path = "shell_services/theme_mode_from_setting.rs"]
mod theme_mode_from_setting;
use now_millis::now_millis;
