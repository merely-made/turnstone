// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Instance-local readings of already verified capsule revisions.
//!
//! A reading is a presentation, not another publication or durable copy. Several
//! readings share one verified body while keeping distinct retained view keys.
//! Only the host's confirmed storage acknowledgment changes the kept observation.

use std::collections::BTreeMap;

use serde::Serialize;

use super::OpenedCapsule;

pub const MAX_READINGS: usize = 8;

struct Resource {
    opened: OpenedCapsule,
    kept: bool,
}

struct Reading {
    id: u32,
    revision: String,
}

#[derive(Serialize)]
pub struct ReadingView<'a> {
    pub id: u32,
    pub opened: &'a OpenedCapsule,
    pub kept: bool,
    pub selected: bool,
}

#[derive(Default)]
pub struct Readings {
    next_id: u32,
    readings: Vec<Reading>,
    resources: BTreeMap<String, Resource>,
    selected: Option<u32>,
}

impl Readings {
    /// The caller must first verify this body through the current CapsuleMount.
    pub fn open(&mut self, opened: OpenedCapsule, kept: bool) -> Result<u32, String> {
        if self.readings.len() >= MAX_READINGS {
            return Err("Eight readings are open; close one before opening another".into());
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("Reading identity exhausted")?;
        let revision = opened.entry.revision.clone();
        if let Some(resource) = self.resources.get_mut(&revision) {
            if resource.opened.entry != opened.entry
                || resource.opened.content_hash != opened.content_hash
                || resource.opened.body != opened.body
            {
                return Err("Different content for an already open revision".into());
            }
            resource.kept |= kept;
        } else {
            self.resources
                .insert(revision.clone(), Resource { opened, kept });
        }
        self.next_id = id;
        self.readings.push(Reading { id, revision });
        self.selected = Some(id);
        Ok(id)
    }

    pub fn view(&self, id: u32) -> Option<ReadingView<'_>> {
        let reading = self.readings.iter().find(|reading| reading.id == id)?;
        let resource = self.resources.get(&reading.revision)?;
        Some(ReadingView {
            id,
            opened: &resource.opened,
            kept: resource.kept,
            selected: self.selected == Some(id),
        })
    }

    pub fn selected(&self) -> Option<ReadingView<'_>> {
        self.selected.and_then(|id| self.view(id))
    }

    pub fn views(&self) -> Vec<ReadingView<'_>> {
        self.readings
            .iter()
            .filter_map(|reading| self.view(reading.id))
            .collect()
    }

    pub fn select(&mut self, id: u32) -> Result<(), String> {
        if self.view(id).is_none() {
            return Err("Reading is closed or unknown".into());
        }
        self.selected = Some(id);
        Ok(())
    }

    pub fn deselect(&mut self) {
        self.selected = None;
    }

    /// Closing a presentation never deletes a retained capsule from storage.
    pub fn close(&mut self, id: u32) -> Result<(), String> {
        let position = self
            .readings
            .iter()
            .position(|reading| reading.id == id)
            .ok_or("Reading is closed or unknown")?;
        let reading = self.readings.remove(position);
        if !self
            .readings
            .iter()
            .any(|other| other.revision == reading.revision)
        {
            self.resources.remove(&reading.revision);
        }
        if self.selected == Some(id) {
            self.selected = self
                .readings
                .get(position.saturating_sub(1))
                .or_else(|| self.readings.last())
                .map(|reading| reading.id);
        }
        Ok(())
    }

    /// A delayed acknowledgment may update only the view that requested it.
    /// Once confirmed, retention is a shared observation of that exact revision.
    pub fn kept(&mut self, id: u32, revision: &str) -> bool {
        if !self
            .readings
            .iter()
            .any(|reading| reading.id == id && reading.revision == revision)
        {
            return false;
        }
        let Some(resource) = self.resources.get_mut(revision) else {
            return false;
        };
        resource.kept = true;
        true
    }

    pub fn clear(&mut self) {
        self.readings.clear();
        self.resources.clear();
        self.selected = None;
        // Never reuse a view key within this host, including after replacement.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capsule_applet::CapsuleEntry;

    fn capsule(revision: &str) -> OpenedCapsule {
        OpenedCapsule {
            entry: CapsuleEntry {
                title: "Garden".into(),
                author: "Alice".into(),
                revision: revision.into(),
                url: format!("mere://capsule/{revision}"),
                bytes: 9,
            },
            body: "# Garden\n".into(),
            content_hash: "body-hash".into(),
        }
    }

    #[test]
    fn independent_accesses_share_content_and_closing_one_preserves_the_other() {
        let mut readings = Readings::default();
        let first = readings.open(capsule("revision"), false).unwrap();
        let second = readings.open(capsule("revision"), false).unwrap();
        assert_ne!(first, second);
        assert!(std::ptr::eq(
            readings.view(first).unwrap().opened,
            readings.view(second).unwrap().opened
        ));
        readings.select(first).unwrap();
        assert!(!readings.selected().unwrap().kept);
        readings.deselect();
        assert!(readings.selected().is_none());
        assert_eq!(readings.views().len(), 2);
        readings.select(first).unwrap();
        readings.close(first).unwrap();
        assert_eq!(readings.selected().unwrap().id, second);
        assert_eq!(readings.view(second).unwrap().opened.body, "# Garden\n");
        assert_eq!(readings.resources.len(), 1);
        readings.close(second).unwrap();
        assert!(readings.resources.is_empty());
    }

    #[test]
    fn delayed_retention_cannot_acknowledge_a_closed_or_different_reading() {
        let mut readings = Readings::default();
        let first = readings.open(capsule("revision"), false).unwrap();
        readings.close(first).unwrap();
        let second = readings.open(capsule("revision"), false).unwrap();
        assert!(!readings.kept(first, "revision"));
        assert!(!readings.kept(second, "other-revision"));
        assert!(!readings.view(second).unwrap().kept);
        let third = readings.open(capsule("revision"), false).unwrap();
        assert!(readings.kept(second, "revision"));
        assert!(readings.view(third).unwrap().kept);
        readings.close(second).unwrap();
        assert!(readings.view(third).unwrap().kept);
    }

    #[test]
    fn selection_and_clear_do_not_reuse_closed_view_keys() {
        let mut readings = Readings::default();
        let first = readings.open(capsule("first"), false).unwrap();
        let second = readings.open(capsule("second"), false).unwrap();
        assert!(readings.select(u32::MAX).is_err());
        assert_eq!(readings.selected().unwrap().id, second);
        readings.close(first).unwrap();
        assert_eq!(readings.selected().unwrap().id, second);
        readings.clear();
        let third = readings.open(capsule("first"), false).unwrap();
        assert!(third > second);
        assert!(!readings.kept(first, "first"));
    }

    #[test]
    fn view_bound_and_conflicting_resource_refuse_without_displacing_readings() {
        let mut readings = Readings::default();
        let first = readings.open(capsule("revision"), false).unwrap();
        let mut conflicting = capsule("revision");
        conflicting.body = "substituted".into();
        assert!(readings.open(conflicting, false).is_err());
        assert_eq!(readings.selected().unwrap().id, first);
        for _ in 1..MAX_READINGS {
            readings.open(capsule("revision"), false).unwrap();
        }
        assert!(readings.open(capsule("other"), false).is_err());
        assert_eq!(readings.views().len(), MAX_READINGS);
        assert_eq!(readings.resources.len(), 1);
        readings.close(first).unwrap();
        assert!(readings.open(capsule("other"), false).is_ok());
    }
}
