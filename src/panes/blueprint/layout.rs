use std::collections::HashSet;

use super::{LayoutBranch, LayoutNode, NormalizationPolicy, PaneId};

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
                let mut kept: Vec<_> = children
                    .into_iter()
                    .filter_map(|tree| tree.normalized(known, policy))
                    .collect();
                if kept.is_empty() {
                    None
                } else if kept.len() == 1 && policy.collapse_single_child {
                    kept.pop()
                } else {
                    let active = active.min(kept.len() - 1);
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
