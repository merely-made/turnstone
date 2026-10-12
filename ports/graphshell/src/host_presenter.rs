// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read-only occurrence selection and inspection over a validated host disclosure.
//! The selection outlives a visible Fold projection, but never changes source facts.
use std::collections::{BTreeMap, BTreeSet};

use crate::presentation_editor::presentation::{
    Condition, Context, Effect, PRESENTATION_RULE_VERSION, PrimitiveKind, Property, Resolution,
    Rule, RuleSet, Scalar, Target, ViewState,
};
use crate::projection_compile::{HostDatasetV1, ProjectionOccurrence, ProjectionValue};
use uuid::Uuid;

pub struct HostPresenter {
    pub envelope: HostDatasetV1,
    pub selected: BTreeSet<String>,
    pub inspected: Option<String>,
    pub visible: BTreeMap<String, Uuid>,
    pub notice: String,
}

impl HostPresenter {
    pub fn new(envelope: HostDatasetV1) -> Self {
        Self {
            envelope,
            selected: BTreeSet::new(),
            inspected: None,
            visible: BTreeMap::new(),
            notice: String::new(),
        }
    }

    pub fn occurrence(&self, id: &str) -> Option<&ProjectionOccurrence> {
        self.envelope
            .dataset
            .occurrences
            .iter()
            .find(|row| row.occurrence_id == id)
    }

    pub fn text(&self, id: &str, field: &str) -> Option<&str> {
        self.occurrence(id)?
            .values
            .get(field)?
            .text()
            .filter(|text| !text.trim().is_empty())
    }

    pub fn label<'a>(&'a self, id: &'a str) -> &'a str {
        self.text(id, "label").unwrap_or(id)
    }

    pub fn choice_label(&self, id: &str) -> String {
        let label = self.label(id);
        if self
            .envelope
            .dataset
            .occurrences
            .iter()
            .filter(|row| self.label(&row.occurrence_id) == label)
            .count()
            > 1
        {
            format!("{label} [{id}]")
        } else {
            label.to_owned()
        }
    }

    /// Install a checkpoint of the same authority atomically. Departed occurrences
    /// leave this view selection with an announcement, without changing retention.
    pub fn rebind(&mut self, envelope: HostDatasetV1) -> Result<(), String> {
        if envelope.dataset.source != self.envelope.dataset.source {
            return Err("Presenter refused a different source authority".into());
        }
        let known: BTreeSet<_> = envelope
            .dataset
            .occurrences
            .iter()
            .map(|row| row.occurrence_id.as_str())
            .collect();
        let previous = self.selected.len();
        self.selected.retain(|id| known.contains(id.as_str()));
        if self
            .inspected
            .as_ref()
            .is_some_and(|id| !known.contains(id.as_str()))
        {
            self.inspected = None;
        }
        self.notice = if previous != self.selected.len() {
            format!(
                "{} selected item(s) unavailable at this checkpoint",
                previous - self.selected.len()
            )
        } else {
            String::new()
        };
        self.envelope = envelope;
        Ok(())
    }

    pub fn set_visible(&mut self, visible: BTreeMap<String, Uuid>) {
        self.visible = visible
            .into_iter()
            .filter(|(id, _)| self.occurrence(id).is_some())
            .collect();
    }

    /// Reconcile actual graph input for visible occurrences only. Hidden selected
    /// subjects survive until an explicit removal/clear or a source departure.
    pub fn capture(&mut self, members: &[Uuid], focused: Option<&str>) -> bool {
        let before = (self.selected.clone(), self.inspected.clone());
        let visible = &self.visible;
        self.selected.retain(|id| !visible.contains_key(id));
        self.selected.extend(
            visible
                .iter()
                .filter(|(_, member)| members.contains(member))
                .map(|(id, _)| id.clone()),
        );
        if before.0 == self.selected {
            return false;
        }
        if let Some(id) = focused.filter(|id| self.selected.contains(*id)) {
            if !self
                .inspected
                .as_ref()
                .is_some_and(|old| self.selected.contains(old))
            {
                self.inspected = Some(id.to_owned());
            }
        }
        if self
            .inspected
            .as_ref()
            .is_some_and(|id| !self.selected.contains(id))
        {
            self.inspected = self.selected.iter().next().cloned();
        }
        before != (self.selected.clone(), self.inspected.clone())
    }

    pub fn toggle(&mut self, id: &str) -> Result<(), String> {
        if self.occurrence(id).is_none() {
            return Err("Item is no longer disclosed".into());
        }
        if !self.selected.remove(id) {
            self.selected.insert(id.into());
            self.inspected = Some(id.into());
        } else if self.inspected.as_deref() == Some(id) {
            self.inspected = self.selected.iter().next().cloned();
        }
        self.notice.clear();
        Ok(())
    }

    pub fn inspect(&mut self, id: &str) -> Result<(), String> {
        if self.occurrence(id).is_none() {
            return Err("Item is no longer disclosed".into());
        }
        self.inspected = Some(id.into());
        Ok(())
    }

    pub fn clear(&mut self) {
        self.selected.clear();
        self.inspected = None;
        self.notice.clear();
    }

    pub fn visible_members(&self) -> Vec<Uuid> {
        self.selected
            .iter()
            .filter_map(|id| self.visible.get(id).copied())
            .collect()
    }

    /// The first stable-face treatment uses R1 only for adjacent details. It
    /// supplies known view state and actual scalar facts, with no fabricated
    /// screen measurement, foreground pin or footprint/LOD decision.
    pub fn detail_resolution(&self, id: &str) -> Result<Resolution, String> {
        let occurrence = self.occurrence(id).ok_or("Item is no longer disclosed")?;
        let facts = occurrence
            .values
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    match value {
                        ProjectionValue::Text(value) => Scalar::Text(value.clone()),
                        ProjectionValue::Number(value) => Scalar::Number(*value),
                        ProjectionValue::Boolean(value) => Scalar::Bool(*value),
                    },
                )
            })
            .collect();
        let mut states = BTreeSet::new();
        if self.selected.contains(id) {
            states.insert(ViewState::Selected);
        }
        if self.inspected.as_deref() == Some(id) {
            states.insert(ViewState::Inspected);
        }
        RuleSet {
            version: PRESENTATION_RULE_VERSION,
            rules: vec![Rule {
                id: "adjacent-details".into(),
                label: "Inspect beside the graph".into(),
                enabled: true,
                priority: 0,
                target: Target {
                    kind: PrimitiveKind::Node,
                    ids: BTreeSet::new(),
                },
                condition: Condition::Any {
                    conditions: vec![
                        Condition::State {
                            state: ViewState::Selected,
                        },
                        Condition::State {
                            state: ViewState::Inspected,
                        },
                    ],
                },
                effects: vec![Effect::Details { visible: true }],
            }],
        }
        .resolve(&Context {
            id: id.into(),
            kind: PrimitiveKind::Node,
            facts,
            states,
            screen_size: None,
            previous_matches: BTreeSet::new(),
        })
        .map_err(|issues| format!("Presentation refused: {issues:?}"))
    }

    pub fn details_visible(&self, id: &str) -> bool {
        self.detail_resolution(id).ok().is_some_and(|result| {
            matches!(
                result
                    .winners
                    .get(&Property::Details)
                    .map(|row| &row.effect),
                Some(Effect::Details { visible: true })
            )
        })
    }
}

/// Only ordinary web navigation is disclosed by the first viewer. The source
/// text remains inspectable if a link is unsupported; credentials are refused.
pub fn navigation_url(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    (matches!(url.scheme(), "https" | "http")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn presenter() -> HostPresenter {
        HostPresenter::new(
            scenomise::host_dataset::parse_host_dataset(include_str!(
                "../web/fixtures/grouped-manifest-components.json"
            ))
            .unwrap(),
        )
    }
    #[test]
    fn hidden_selection_survives_visible_input_and_returns_to_the_graph() {
        let mut p = presenter();
        let ids: Vec<_> = p
            .envelope
            .dataset
            .occurrences
            .iter()
            .take(3)
            .map(|o| o.occurrence_id.clone())
            .collect();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        p.set_visible(BTreeMap::from([(ids[0].clone(), a), (ids[1].clone(), b)]));
        p.capture(&[a, b], Some(&ids[0]));
        p.set_visible(BTreeMap::from([(ids[1].clone(), b)]));
        p.capture(&[b], Some(&ids[1]));
        assert_eq!(p.selected.len(), 2);
        p.set_visible(BTreeMap::from([(ids[0].clone(), a), (ids[1].clone(), b)]));
        assert_eq!(
            p.visible_members().into_iter().collect::<BTreeSet<_>>(),
            BTreeSet::from([a, b])
        );
        p.clear();
        assert!(p.visible_members().is_empty());
    }
    #[test]
    fn checkpoint_departure_and_authority_refusal_are_explicit() {
        let mut p = presenter();
        let id = p.envelope.dataset.occurrences[0].occurrence_id.clone();
        p.toggle(&id).unwrap();
        let mut changed = p.envelope.clone();
        changed
            .dataset
            .occurrences
            .retain(|o| o.occurrence_id != id);
        p.rebind(changed).unwrap();
        assert!(p.selected.is_empty());
        assert!(p.inspected.is_none());
        assert!(p.notice.contains("unavailable"));
        let before = p.envelope.clone();
        let mut wrong = before.clone();
        wrong.dataset.source.authority = "other".into();
        assert!(p.rebind(wrong).is_err());
        assert_eq!(p.envelope, before);
    }
    #[test]
    fn inspection_is_occurrence_local_and_never_changes_source_or_selection() {
        let mut p = presenter();
        let original = p.envelope.clone();
        let a = original.dataset.occurrences[0].occurrence_id.clone();
        let b = original.dataset.occurrences[1].occurrence_id.clone();
        p.toggle(&a).unwrap();
        p.inspect(&b).unwrap();
        let member = Uuid::from_u128(1);
        p.set_visible(BTreeMap::from([(a.clone(), member)]));
        assert!(!p.capture(&[member], Some(&a)));
        assert_eq!(p.inspected.as_deref(), Some(b.as_str()));
        assert_eq!(p.selected, BTreeSet::from([a.clone()]));
        assert!(p.details_visible(&b));
        let result = p.detail_resolution(&b).unwrap();
        assert!(!result.winners.contains_key(&Property::Representation));
        assert_eq!(p.envelope, original);
        assert!(p.inspect("not-disclosed").is_err());
    }
    #[test]
    fn equal_labels_and_shared_sources_keep_independent_occurrence_selection() {
        let mut p = presenter();
        let a = p.envelope.dataset.occurrences[0].occurrence_id.clone();
        let b = p.envelope.dataset.occurrences[1].occurrence_id.clone();
        p.envelope.dataset.occurrences[1].source = p.envelope.dataset.occurrences[0].source.clone();
        let shared_label = p.label(&a).to_owned();
        p.envelope.dataset.occurrences[1]
            .values
            .insert("label".into(), ProjectionValue::Text(shared_label));
        let original = p.envelope.clone();
        p.toggle(&a).unwrap();
        p.toggle(&b).unwrap();
        assert_eq!(p.selected.len(), 2);
        assert_ne!(p.choice_label(&a), p.choice_label(&b));
        p.inspect(&a).unwrap();
        assert_eq!(p.inspected.as_deref(), Some(a.as_str()));
        assert_eq!(p.envelope, original);
    }

    #[test]
    fn navigation_refuses_active_content_credentials_and_relative_addresses() {
        for url in [
            "javascript:alert(1)",
            "data:text/html,hi",
            "//example.test",
            "https://user:secret@example.test",
            "mere://x",
        ] {
            assert!(navigation_url(url).is_none(), "{url}");
        }
        assert_eq!(
            navigation_url("https://example.test/project"),
            Some("https://example.test/project".into())
        );
    }
}
