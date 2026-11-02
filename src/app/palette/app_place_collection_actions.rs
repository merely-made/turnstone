// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Captured-page scope is a local reading choice. The worker supplies the
    /// currently named exact versions; the palette carries those opaque values
    /// back unchanged, rather than trying to reconstruct a collection from its
    /// label. A stale selection names its current authorized replacement,
    /// rather than suggesting that the unavailable historical version can be
    /// searched.
    pub(super) fn place_collection_actions(&self) -> Vec<(String, Action)> {
        let crate::place::PlaceState::Offline { snapshot, .. } = &self.place else {
            return Vec::new();
        };

        let all_selected = matches!(
            &snapshot.captured_selection,
            crate::place::CapturedCollectionSelection::AllEffective
        );
        let mut rows = vec![
            (
                "Search captured pages".to_string(),
                Action::SearchCapturedPages,
            ),
            (
                format!(
                    "Use captured collection: all shared captures{}",
                    all_selected.then_some(" (selected)").unwrap_or_default()
                ),
                Action::SetPlaceCollection(None),
            ),
        ];
        let (selected, stale_current) = match &snapshot.captured_selection {
            crate::place::CapturedCollectionSelection::AllEffective => (None, None),
            crate::place::CapturedCollectionSelection::Collection { requested, status } => (
                Some(requested),
                match status {
                    crate::place::CapturedCollectionSelectionStatus::Stale { current } => {
                        Some(current)
                    },
                    _ => None,
                },
            ),
        };

        for choice in &snapshot.collection_choices {
            let same_name = snapshot
                .collection_choices
                .iter()
                .filter(|other| other.name == choice.name)
                .count()
                > 1;
            let suffix = if same_name {
                format!(
                    " ({})",
                    collection_id_suffix(
                        &choice.version,
                        &snapshot.collection_choices,
                        &choice.name
                    )
                )
            } else {
                String::new()
            };
            let state = if selected.is_some_and(|requested| requested == &choice.version) {
                " (selected)"
            } else if stale_current.is_some_and(|current| current == &choice.version) {
                " (use latest version)"
            } else {
                ""
            };
            rows.push((
                format!("Use captured collection: {}{suffix}{state}", choice.name),
                Action::SetPlaceCollection(Some(choice.version.clone())),
            ));
        }

        if let Some(current) = stale_current
            && !snapshot
                .collection_choices
                .iter()
                .any(|choice| &choice.version == current)
        {
            let name = snapshot
                .collection_choices
                .iter()
                .find(|choice| choice.version.collection == current.collection)
                .map(|choice| choice.name.as_str())
                .unwrap_or("selected collection");
            let has_named_choice = snapshot
                .collection_choices
                .iter()
                .filter(|choice| choice.name == name)
                .count()
                > 0;
            let suffix = has_named_choice
                .then(|| {
                    format!(
                        " ({})",
                        collection_id_suffix(current, &snapshot.collection_choices, name)
                    )
                })
                .unwrap_or_default();
            rows.push((
                format!("Use captured collection: {name}{suffix} (use latest version)"),
                Action::SetPlaceCollection(Some(current.clone())),
            ));
        }
        rows
    }

    /// The composed-section rows for the ACTIVE pane, when its content composes
    /// (a Gloss, an Overmap): one add/remove per registered provider, plus the
    /// reorder rows. Pane-scoped palette entries are how the gloss-composite
    /// design chose to expose composition (the right-click palette already
    /// selects the pane under the pointer), so no new chrome. Empty when the
    /// active pane is not a composable one.
    ///
    /// Written against `PaneContent::composition`, not a pane kind, so a pane
    /// that gains a composition gains this whole UI without touching it. The
    /// row's prefix is the pane's own tag, so it names itself too.
    pub(super) fn pane_section_actions(&self) -> Vec<(String, Action)> {
        let Some(pane) = self.active_pane else {
            return Vec::new();
        };
        let Some(content) = self.pane_content(pane) else {
            return Vec::new();
        };
        let Some(cfg) = content.composition() else {
            return Vec::new();
        };
        let who = pane_label(content);
        let mut rows: Vec<(String, Action)> = crate::sections::ALL
            .iter()
            .map(|p| {
                let on = cfg.sections.iter().any(|id| id == p.id);
                let verb = if on { "remove" } else { "add" };
                (
                    format!("{who}: {verb} section — {}", p.title),
                    Action::TogglePaneSection {
                        pane,
                        section: p.id.to_string(),
                    },
                )
            })
            .collect();
        // Reorder rows only where a move would DO something: nothing to
        // reorder with one section, and no "up" on the first (the palette
        // should not offer a no-op).
        if cfg.sections.len() > 1 {
            for (i, id) in cfg.sections.iter().enumerate() {
                let Some(p) = crate::sections::by_id(id) else {
                    continue;
                };
                if i > 0 {
                    rows.push((
                        format!("{who}: move section up — {}", p.title),
                        Action::MovePaneSection {
                            pane,
                            section: id.clone(),
                            delta: -1,
                        },
                    ));
                }
                if i + 1 < cfg.sections.len() {
                    rows.push((
                        format!("{who}: move section down — {}", p.title),
                        Action::MovePaneSection {
                            pane,
                            section: id.clone(),
                            delta: 1,
                        },
                    ));
                }
            }
        }
        rows
    }
}
