// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
