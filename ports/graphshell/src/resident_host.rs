// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Per-graph resident settings and the union of what every open pane needs
//! resident. The complete writer stays authoritative (resident host plan,
//! Task 4): nothing here is truth, only what to present.
use pandect::ViewKey;
use pandect::addressable_graph::{GraphAddress, ResidencyPolicy, ResidencyRoots, ResidentGraph};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// How a host learns that a newer publication exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ResidentRefreshMode {
    /// Only an explicit refresh checks.
    Manual,
    /// The host checks at most once per interval.
    HostDriven { minimum_check_ms: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResidentSettingsError {
    /// A zero interval would busy-poll.
    ZeroCheckInterval,
}
impl std::fmt::Display for ResidentSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCheckInterval => {
                f.write_str("host-driven refresh needs a positive minimum check interval")
            },
        }
    }
}
impl std::error::Error for ResidentSettingsError {}

/// A graph's resident display settings. Full, manual refresh and no publication
/// batching are the compatibility defaults.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResidentSettings {
    pub policy: ResidencyPolicy,
    /// The authored projection whose roster roots a Neighborhood walk.
    pub projection: Option<String>,
    pub refresh: ResidentRefreshMode,
    /// Minimum time between resident publications; 0 publishes at every store
    /// boundary.
    pub publication_batch_ms: u64,
}
impl Default for ResidentSettings {
    fn default() -> Self {
        Self {
            policy: ResidencyPolicy::Full,
            projection: None,
            refresh: ResidentRefreshMode::Manual,
            publication_batch_ms: 0,
        }
    }
}
impl ResidentSettings {
    /// Shape rules that need no host. Whether `projection` names an authored
    /// projection of this graph is the host's check before it enables
    /// Neighborhood.
    pub fn validate(&self) -> Result<(), ResidentSettingsError> {
        match self.refresh {
            ResidentRefreshMode::HostDriven { minimum_check_ms: 0 } => {
                Err(ResidentSettingsError::ZeroCheckInterval)
            },
            _ => Ok(()),
        }
    }

    /// The slot holding these settings, beside the P5 policy key under the same
    /// root graph.
    pub fn slot_key(root_graph: Uuid) -> String {
        format!("graphs/{root_graph}/resident/v1/host-settings")
    }
}

/// What one pane needs resident beyond the host's pins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaneRoots {
    /// The pane's authored projection roster. It is kept whole even for members
    /// that are not loaded or not in the catalog.
    pub roster: BTreeSet<Uuid>,
    /// The pane's transient focus. It never becomes a durable pin.
    pub focus: Option<GraphAddress>,
}

/// The union of every open pane's roots and the graph's explicit pins.
#[derive(Default)]
pub struct HostRoots {
    panes: BTreeMap<ViewKey, PaneRoots>,
    pins: BTreeSet<GraphAddress>,
    revision: u64,
}
impl HostRoots {
    pub fn set_pane(&mut self, pane: ViewKey, roots: PaneRoots) {
        if self.panes.get(&pane) != Some(&roots) {
            self.panes.insert(pane, roots);
            self.bump();
        }
    }

    /// Releases only this pane's roots; what another pane or a pin still needs
    /// stays in the union.
    pub fn close_pane(&mut self, pane: &ViewKey) {
        if self.panes.remove(pane).is_some() {
            self.bump();
        }
    }

    pub fn set_pins(&mut self, pins: BTreeSet<GraphAddress>) {
        if self.pins != pins {
            self.pins = pins;
            self.bump();
        }
    }

    pub fn roster(&self, pane: &ViewKey) -> Option<&BTreeSet<Uuid>> {
        self.panes.get(pane).map(|roots| &roots.roster)
    }

    /// Roots for a Neighborhood walk. Every pane's roster is a walk root; pins
    /// and each pane's focus are individually kept (zero hops), so neither
    /// seeds a walk.
    pub fn union(&self) -> ResidencyRoots {
        let mut roots = ResidencyRoots {
            pinned: self.pins.clone(),
            ..Default::default()
        };
        for pane in self.panes.values() {
            roots.projection_surfaces.extend(&pane.roster);
            roots.transient.extend(pane.focus);
        }
        roots
    }

    /// Moves whenever the union could have changed.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

/// What a host holds to present a partial graph: the settings, every open
/// pane's roots and the installed reader. The complete writer stays the host's
/// truth; nothing here is authority.
pub(crate) struct ResidentView<B> {
    pub(crate) settings: ResidentSettings,
    pub(crate) roots: HostRoots,
    pub(crate) reader: Option<ResidentGraph<B>>,
}
impl<B> Default for ResidentView<B> {
    fn default() -> Self {
        Self {
            settings: ResidentSettings::default(),
            roots: HostRoots::default(),
            reader: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> ViewKey {
        ViewKey::new("graphshell", name).unwrap()
    }

    #[test]
    fn closing_one_pane_releases_only_its_transient_roots() {
        let (first, second) = (key("first"), key("second"));
        let a = Uuid::from_u128(4000);
        let b = Uuid::from_u128(4001);
        let focus = GraphAddress::Resource(Uuid::from_u128(4002));
        let pin = GraphAddress::Surface(Uuid::from_u128(4003));
        let mut roots = HostRoots::default();
        roots.set_pins(BTreeSet::from([pin]));
        roots.set_pane(
            first.clone(),
            PaneRoots {
                roster: BTreeSet::from([a]),
                focus: Some(focus),
            },
        );
        roots.set_pane(
            second,
            PaneRoots {
                roster: BTreeSet::from([a, b]),
                focus: Some(focus),
            },
        );
        assert_eq!(roots.union().projection_surfaces, BTreeSet::from([a, b]));
        assert_eq!(roots.union().transient, BTreeSet::from([focus]));
        assert_eq!(roots.union().pinned, BTreeSet::from([pin]));
        roots.close_pane(&first);
        assert_eq!(roots.union().projection_surfaces, BTreeSet::from([a, b]));
        assert_eq!(roots.union().transient, BTreeSet::from([focus]));
        assert_eq!(roots.union().pinned, BTreeSet::from([pin]));
    }

    #[test]
    fn a_closed_pane_gives_up_its_own_roster_and_focus_only() {
        let (first, second) = (key("first"), key("second"));
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let (fa, fb) = (
            GraphAddress::Surface(Uuid::from_u128(10)),
            GraphAddress::Resource(Uuid::from_u128(11)),
        );
        let mut roots = HostRoots::default();
        roots.set_pane(
            first.clone(),
            PaneRoots {
                roster: [a].into(),
                focus: Some(fa),
            },
        );
        roots.set_pane(
            second.clone(),
            PaneRoots {
                roster: [b].into(),
                focus: Some(fb),
            },
        );
        roots.close_pane(&first);
        let union = roots.union();
        assert_eq!(union.projection_surfaces, BTreeSet::from([b]));
        assert_eq!(union.transient, BTreeSet::from([fb]));
        assert!(union.pinned.is_empty(), "focus never becomes a pin");
        assert_eq!(roots.roster(&first), None);
        roots.close_pane(&second);
        assert_eq!(roots.union(), ResidencyRoots::default());
    }

    #[test]
    fn an_empty_or_unloaded_roster_stays_authored() {
        let pane = key("scoped");
        let mut roots = HostRoots::default();
        roots.set_pane(pane.clone(), PaneRoots::default());
        assert_eq!(roots.roster(&pane), Some(&BTreeSet::new()));
        let missing = Uuid::from_u128(999);
        roots.set_pane(
            pane.clone(),
            PaneRoots {
                roster: [missing].into(),
                focus: None,
            },
        );
        assert_eq!(roots.roster(&pane), Some(&BTreeSet::from([missing])));
    }

    #[test]
    fn the_revision_moves_only_when_the_union_could_change() {
        let pane = key("p");
        let mut roots = HostRoots::default();
        let start = roots.revision();
        roots.set_pane(pane.clone(), PaneRoots::default());
        let after_set = roots.revision();
        assert!(after_set > start);
        roots.set_pane(pane.clone(), PaneRoots::default());
        roots.set_pins(BTreeSet::new());
        roots.close_pane(&key("absent"));
        assert_eq!(roots.revision(), after_set, "repeating a state is not a change");
        roots.close_pane(&pane);
        assert!(roots.revision() > after_set);
    }

    #[test]
    fn settings_default_to_the_compatibility_setting_and_round_trip() {
        let defaults = ResidentSettings::default();
        assert_eq!(defaults.policy, ResidencyPolicy::Full);
        assert_eq!(defaults.refresh, ResidentRefreshMode::Manual);
        assert_eq!(defaults.publication_batch_ms, 0);
        assert!(defaults.validate().is_ok());
        let tuned = ResidentSettings {
            policy: ResidencyPolicy::Neighborhood { hops: 2 },
            projection: Some("authored.review".into()),
            refresh: ResidentRefreshMode::HostDriven {
                minimum_check_ms: 500,
            },
            publication_batch_ms: 250,
        };
        let bytes = serde_json::to_vec(&tuned).unwrap();
        assert_eq!(serde_json::from_slice::<ResidentSettings>(&bytes).unwrap(), tuned);
        assert!(tuned.validate().is_ok());
        let zero = ResidentSettings {
            refresh: ResidentRefreshMode::HostDriven { minimum_check_ms: 0 },
            ..tuned
        };
        assert_eq!(zero.validate(), Err(ResidentSettingsError::ZeroCheckInterval));
        let id = Uuid::from_u128(5);
        assert_eq!(
            ResidentSettings::slot_key(id),
            format!("graphs/{id}/resident/v1/host-settings")
        );
    }
}
