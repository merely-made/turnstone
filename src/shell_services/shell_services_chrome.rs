// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
