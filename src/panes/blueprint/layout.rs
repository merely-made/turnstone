// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashSet;

use super::{LayoutBranch, LayoutNode, LayoutPathStep, NormalizationPolicy, PaneId, SplitAxis};

impl LayoutNode {
    pub(super) fn normalized(
        self,
        known: &HashSet<PaneId>,
        policy: NormalizationPolicy,
    ) -> Option<Self> {
        match self {
            Self::Pane(pane) if !policy.prune_unknown_panes || known.contains(&pane) => {
                Some(Self::Pane(pane))
            }
            Self::Pane(_) => None,
            Self::Split { axis, children } => {
                let mut kept = Vec::new();
                for branch in children {
                    let Some(tree) = branch.tree.normalized(known, policy) else {
                        continue;
                    };
                    match tree {
                        Self::Split {
                            axis: nested_axis,
                            children: nested,
                        } if policy.join_same_axis_splits && nested_axis == axis => {
                            let outer = sane_fraction(branch.fraction);
                            kept.extend(nested.into_iter().map(|nested| LayoutBranch {
                                fraction: outer * sane_fraction(nested.fraction),
                                tree: nested.tree,
                            }));
                        }
                        tree => kept.push(LayoutBranch {
                            fraction: sane_fraction(branch.fraction),
                            tree,
                        }),
                    }
                }
                if kept.is_empty() {
                    None
                } else if kept.len() == 1 && policy.collapse_single_child {
                    Some(kept.remove(0).tree)
                } else {
                    normalize_fractions(kept.iter_mut().map(|branch| &mut branch.fraction));
                    Some(Self::Split {
                        axis,
                        children: kept,
                    })
                }
            }
            Self::Tabs { children, active } => {
                let mut active_after = None;
                let mut kept = Vec::new();
                for (index, tree) in children.into_iter().enumerate() {
                    let Some(tree) = tree.normalized(known, policy) else {
                        continue;
                    };
                    if index == active {
                        active_after = Some(kept.len());
                    }
                    kept.push(tree);
                }
                if kept.is_empty() {
                    None
                } else if kept.len() == 1 && policy.collapse_single_child {
                    kept.pop()
                } else {
                    let active = active_after.unwrap_or_else(|| active.min(kept.len() - 1));
                    Some(Self::Tabs {
                        children: kept,
                        active,
                    })
                }
            }
            Self::Grid {
                children,
                columns,
                mut shares,
            } => {
                let mut kept: Vec<_> = children
                    .into_iter()
                    .filter_map(|tree| tree.normalized(known, policy))
                    .collect();
                if kept.is_empty() {
                    return None;
                }
                if kept.len() == 1 && policy.collapse_single_child {
                    return kept.pop();
                }
                let columns = columns.clamp(1, kept.len());
                let rows = kept.len().div_ceil(columns);
                resize_and_normalize(&mut shares.columns, columns);
                resize_and_normalize(&mut shares.rows, rows);
                Some(Self::Grid {
                    children: kept,
                    columns,
                    shares,
                })
            }
        }
    }

    pub(super) fn without_pane(self, pane: PaneId) -> Option<Self> {
        match self {
            Self::Pane(id) if id == pane => None,
            Self::Pane(id) => Some(Self::Pane(id)),
            Self::Split { axis, children } => Some(Self::Split {
                axis,
                children: children
                    .into_iter()
                    .filter_map(|branch| {
                        branch.tree.without_pane(pane).map(|tree| LayoutBranch {
                            fraction: branch.fraction,
                            tree,
                        })
                    })
                    .collect(),
            }),
            Self::Tabs { children, active } => Some(Self::Tabs {
                children: children
                    .into_iter()
                    .filter_map(|tree| tree.without_pane(pane))
                    .collect(),
                active,
            }),
            Self::Grid {
                children,
                columns,
                shares,
            } => Some(Self::Grid {
                children: children
                    .into_iter()
                    .filter_map(|tree| tree.without_pane(pane))
                    .collect(),
                columns,
                shares,
            }),
        }
    }

    pub(super) fn collect_panes(&self, out: &mut Vec<PaneId>) {
        match self {
            Self::Pane(pane) => out.push(*pane),
            Self::Split { children, .. } => {
                children
                    .iter()
                    .for_each(|branch| branch.tree.collect_panes(out));
            }
            Self::Tabs { children, .. } | Self::Grid { children, .. } => {
                children.iter().for_each(|tree| tree.collect_panes(out));
            }
        }
    }

    pub(super) fn collect_active_panes(&self, out: &mut Vec<PaneId>) {
        match self {
            Self::Pane(pane) => out.push(*pane),
            Self::Split { children, .. } => children
                .iter()
                .for_each(|branch| branch.tree.collect_active_panes(out)),
            Self::Grid { children, .. } => children
                .iter()
                .for_each(|tree| tree.collect_active_panes(out)),
            Self::Tabs { children, active } => children
                .get((*active).min(children.len().saturating_sub(1)))
                .map(|tree| tree.collect_active_panes(out))
                .unwrap_or(()),
        }
    }

    pub(super) fn insert_beside(
        &mut self,
        target: PaneId,
        pane: PaneId,
        axis: SplitAxis,
        after: bool,
    ) -> bool {
        match self {
            Self::Pane(id) if *id == target => {
                let existing = std::mem::replace(self, Self::Pane(pane));
                let children = if after {
                    vec![
                        LayoutBranch {
                            fraction: 0.5,
                            tree: existing,
                        },
                        LayoutBranch {
                            fraction: 0.5,
                            tree: Self::Pane(pane),
                        },
                    ]
                } else {
                    vec![
                        LayoutBranch {
                            fraction: 0.5,
                            tree: Self::Pane(pane),
                        },
                        LayoutBranch {
                            fraction: 0.5,
                            tree: existing,
                        },
                    ]
                };
                *self = Self::Split { axis, children };
                true
            }
            Self::Pane(_) => false,
            Self::Split { children, .. } => children
                .iter_mut()
                .any(|branch| branch.tree.insert_beside(target, pane, axis, after)),
            Self::Tabs { children, .. } | Self::Grid { children, .. } => children
                .iter_mut()
                .any(|tree| tree.insert_beside(target, pane, axis, after)),
        }
    }

    pub(super) fn insert_tab(&mut self, target: PaneId, pane: PaneId) -> bool {
        match self {
            Self::Pane(id) if *id == target => {
                let existing = std::mem::replace(self, Self::Pane(pane));
                *self = Self::Tabs {
                    children: vec![existing, Self::Pane(pane)],
                    active: 1,
                };
                true
            }
            Self::Pane(_) => false,
            Self::Split { children, .. } => children
                .iter_mut()
                .any(|branch| branch.tree.insert_tab(target, pane)),
            Self::Tabs { children, .. } | Self::Grid { children, .. } => children
                .iter_mut()
                .any(|tree| tree.insert_tab(target, pane)),
        }
    }

    pub(super) fn activate_tab_containing(&mut self, pane: PaneId) -> bool {
        match self {
            Self::Pane(_) => false,
            Self::Split { children, .. } => children
                .iter_mut()
                .any(|branch| branch.tree.activate_tab_containing(pane)),
            Self::Grid { children, .. } => children
                .iter_mut()
                .any(|tree| tree.activate_tab_containing(pane)),
            Self::Tabs { children, active } => {
                if let Some(index) = children.iter().position(|tree| tree.contains_pane(pane)) {
                    *active = index;
                    true
                } else {
                    children
                        .iter_mut()
                        .any(|tree| tree.activate_tab_containing(pane))
                }
            }
        }
    }

    pub(super) fn set_split_fractions(
        &mut self,
        path: &[LayoutPathStep],
        fractions: &[f32],
    ) -> bool {
        let Some(target) = self.at_path_mut(path) else {
            return false;
        };
        let Self::Split { children, .. } = target else {
            return false;
        };
        if children.len() != fractions.len() || fractions.is_empty() {
            return false;
        }
        for (branch, fraction) in children.iter_mut().zip(fractions) {
            branch.fraction = sane_fraction(*fraction);
        }
        normalize_fractions(children.iter_mut().map(|branch| &mut branch.fraction));
        true
    }

    fn at_path_mut(&mut self, path: &[LayoutPathStep]) -> Option<&mut Self> {
        let mut node = self;
        for step in path {
            node = match (node, step) {
                (Self::Split { children, .. }, LayoutPathStep::Split(index)) => {
                    &mut children.get_mut(*index)?.tree
                }
                (Self::Tabs { children, .. }, LayoutPathStep::Tab(index)) => {
                    children.get_mut(*index)?
                }
                (Self::Grid { children, .. }, LayoutPathStep::Grid(index)) => {
                    children.get_mut(*index)?
                }
                _ => return None,
            };
        }
        Some(node)
    }

    fn contains_pane(&self, pane: PaneId) -> bool {
        match self {
            Self::Pane(id) => *id == pane,
            Self::Split { children, .. } => children
                .iter()
                .any(|branch| branch.tree.contains_pane(pane)),
            Self::Tabs { children, .. } | Self::Grid { children, .. } => {
                children.iter().any(|tree| tree.contains_pane(pane))
            }
        }
    }
}

fn sane_fraction(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

fn normalize_fractions<'a>(fractions: impl Iterator<Item = &'a mut f32>) {
    let fractions: Vec<_> = fractions.collect();
    let sum: f32 = fractions.iter().map(|value| sane_fraction(**value)).sum();
    for fraction in fractions {
        *fraction = sane_fraction(*fraction) / sum;
    }
}

fn resize_and_normalize(values: &mut Vec<f32>, len: usize) {
    values.resize(len, 1.0);
    normalize_fractions(values.iter_mut());
}
