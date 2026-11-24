// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use cambium::{Key, KeyEvent, Keyed, NamedKey, WheelEvent, on_key, on_wheel};
use graphshell_client::remote::RemoteSession;
use graphshell_client::{AcceptedLabelUpdate, AccessibleItem, NoLocalActions};
use mere_view::NodeEntry;

use super::{Child, TreePage, TreeRemote, button, el};

const VISIBLE_ROWS: usize = 12;
const ROW_HEIGHT: f32 = 18.0;

#[derive(Clone, Eq, Hash, PartialEq)]
enum CardKey {
    Live(Rc<str>, u64, u32),
    Resident(String),
}

struct RevisionKey {
    session: Rc<str>,
    snapshot: u64,
    epoch: u64,
    revision: u64,
}

#[derive(Clone, Copy)]
struct Position<'a> {
    session: &'a str,
    snapshot: u64,
    epoch: u64,
    revision: u64,
}

impl RevisionKey {
    fn current(remote: &RemoteSession) -> Option<Position<'_>> {
        let scene = &remote.mounted()?.scene;
        Some(Position {
            session: &remote.session()?.0,
            snapshot: remote.snapshot_generation(),
            epoch: scene.epoch.0,
            revision: scene.revision.0,
        })
    }

    fn new(at: Position<'_>) -> Self {
        Self {
            session: Rc::from(at.session),
            snapshot: at.snapshot,
            epoch: at.epoch,
            revision: at.revision,
        }
    }

    fn matches(&self, at: Position<'_>) -> bool {
        self.session.as_ref() == at.session
            && self.snapshot == at.snapshot
            && self.epoch == at.epoch
            && self.revision == at.revision
    }

    fn update<'a>(
        &self,
        remote: &'a RemoteSession,
        current: Position<'_>,
    ) -> Option<&'a AcceptedLabelUpdate> {
        let update = remote.accepted_label_update()?;
        (update.session.0 == current.session
            && update.epoch.0 == current.epoch
            && update.revision.0 == current.revision
            && self.matches(Position {
                revision: update.base.0,
                ..current
            }))
        .then_some(update)
    }
}

pub(super) struct LiveRows {
    at: RevisionKey,
    entries: Vec<(u32, String)>,
    index: HashMap<u32, usize>,
}

#[derive(Default)]
pub(super) struct Cache {
    rows: RefCell<Option<LiveRows>>,
    actions: RefCell<Option<(RevisionKey, Vec<chirograph::AdvertisedAction>)>>,
}

pub(super) enum Rows<'a> {
    Resident(&'a [NodeEntry]),
    Live(Ref<'a, LiveRows>),
}

impl Rows<'_> {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Resident(rows) => rows.len(),
            Self::Live(rows) => rows.entries.len(),
        }
    }

    fn get(&self, index: usize) -> Option<(CardKey, &str)> {
        match self {
            Self::Resident(rows) => rows
                .get(index)
                .map(|node| (CardKey::Resident(node.key.clone()), node.label.as_str())),
            Self::Live(rows) => rows.entries.get(index).map(|(instance, label)| {
                (
                    CardKey::Live(rows.at.session.clone(), rows.at.epoch, *instance),
                    label.as_str(),
                )
            }),
        }
    }

    fn position(&self, key: &CardKey) -> Option<usize> {
        match (self, key) {
            (Self::Resident(rows), CardKey::Resident(key)) => {
                rows.iter().position(|node| &node.key == key)
            },
            (Self::Live(rows), CardKey::Live(session, epoch, instance))
                if rows.at.session == *session && rows.at.epoch == *epoch =>
            {
                rows.index.get(instance).copied()
            },
            _ => None,
        }
    }
}

impl TreeRemote {
    pub(super) fn card_rows(&self) -> Rows<'_> {
        if let Some(session) = &self.reservoir {
            return Rows::Resident(&session.model().graph.nodes);
        }
        let Some(remote) = self.projection_session() else {
            return Rows::Resident(&[]);
        };
        let Some(current) = RevisionKey::current(remote) else {
            self.cards.rows.borrow_mut().take();
            return Rows::Resident(&[]);
        };
        let mut cache = self.cards.rows.borrow_mut();
        if self.use_label_updates()
            && let Some(rows) = cache.as_mut()
            && let Some(update) = rows.at.update(remote, current)
        {
            #[cfg(feature = "remote-cards-receipt")]
            let start = super::now_ms();
            for change in &update.changes {
                if let Some(&index) = rows.index.get(&change.instance.0) {
                    let label = change.card_label(remote.profile());
                    if rows.entries[index].1 != label {
                        rows.entries[index].1 = label.to_owned();
                        #[cfg(feature = "remote-cards-receipt")]
                        super::cards_receipt::record(|sample| sample.labels_patched += 1);
                    }
                }
            }
            rows.at.revision = update.revision.0;
            #[cfg(feature = "remote-cards-receipt")]
            super::cards_receipt::record(|sample| sample.labels_ms += super::now_ms() - start);
        }
        if cache.as_ref().is_none_or(|rows| !rows.at.matches(current)) {
            #[cfg(feature = "remote-cards-receipt")]
            let start = super::now_ms();
            let entries: Vec<_> = remote
                .accessibility_tree_with(&NoLocalActions)
                .into_iter()
                .flat_map(|tree| tree.children)
                .map(
                    |AccessibleItem {
                         instance, label, ..
                     }| (instance.0, label),
                )
                .collect();
            let index = entries
                .iter()
                .enumerate()
                .map(|(index, &(id, _))| (id, index))
                .collect();
            #[cfg(feature = "remote-cards-receipt")]
            super::cards_receipt::record(|sample| {
                sample.labels_ms += super::now_ms() - start;
                sample.labels_projected += entries.len();
            });
            *cache = Some(LiveRows {
                at: RevisionKey::new(current),
                entries,
                index,
            });
        }
        drop(cache);
        Rows::Live(Ref::map(self.cards.rows.borrow(), |rows| {
            rows.as_ref().expect("mounted card rows")
        }))
    }

    pub(super) fn projected_actions(
        &self,
        remote: &RemoteSession,
    ) -> Vec<chirograph::AdvertisedAction> {
        let mut cache = self.cards.actions.borrow_mut();
        let Some(current) = RevisionKey::current(remote) else {
            cache.take();
            return Vec::new();
        };
        let optimized = self.use_label_updates();
        if optimized && let Some((at, actions)) = cache.as_mut() {
            if let Some(update) = at.update(remote, current) {
                at.revision = update.revision.0;
            }
            if at.matches(current) {
                return actions.clone();
            }
        }
        #[cfg(feature = "remote-cards-receipt")]
        let start = super::now_ms();
        let actions: Vec<_> = remote
            .actions()
            .into_iter()
            .map(|(_, action)| action)
            .collect();
        #[cfg(feature = "remote-cards-receipt")]
        super::cards_receipt::record(|sample| {
            sample.actions_ms += super::now_ms() - start;
            sample.action_projections += 1;
        });
        if optimized {
            *cache = Some((RevisionKey::new(current), actions.clone()));
        }
        actions
    }
}

#[derive(Default)]
pub(super) struct Window {
    first: usize,
    anchor: Option<CardKey>,
    wheel: f32,
    rows: RefCell<HashMap<CardKey, RowView>>,
}

struct RowView {
    index: usize,
    total: usize,
    label: String,
    view: Rc<Child>,
}

impl Window {
    fn range(&self, rows: &Rows<'_>) -> Range<usize> {
        let first = self.anchor.as_ref().map_or(self.first, |key| {
            if rows.get(self.first).is_some_and(|(at, _)| &at == key) {
                self.first
            } else {
                rows.position(key).unwrap_or(self.first)
            }
        });
        let first = first.min(rows.len().saturating_sub(VISIBLE_ROWS));
        first..(first + VISIBLE_ROWS).min(rows.len())
    }

    fn set_first(&mut self, rows: &Rows<'_>, first: usize) {
        self.first = first.min(rows.len().saturating_sub(VISIBLE_ROWS));
        self.anchor = rows.get(self.first).map(|(key, _)| key);
    }
}

pub(super) fn handle_key(page: &mut TreePage, event: KeyEvent) {
    let remote = page.shared.remote.borrow();
    let rows = remote.card_rows();
    let window = &mut page.sections.cards;
    let first = window.range(&rows).start;
    let target = match event.key {
        Key::Named(NamedKey::ArrowUp) => first.saturating_sub(1),
        Key::Named(NamedKey::ArrowDown) => first + 1,
        Key::Named(NamedKey::PageUp) => first.saturating_sub(VISIBLE_ROWS),
        Key::Named(NamedKey::PageDown) => first + VISIBLE_ROWS,
        Key::Named(NamedKey::Home) => 0,
        Key::Named(NamedKey::End) => rows.len(),
        _ => return,
    };
    window.set_first(&rows, target);
    event.prevent_default();
}

fn handle_wheel(page: &mut TreePage, event: WheelEvent) {
    let remote = page.shared.remote.borrow();
    let rows = remote.card_rows();
    let window = &mut page.sections.cards;
    let first = window.range(&rows).start;
    let last = rows.len().saturating_sub(VISIBLE_ROWS);
    let delta = -event.delta.1;
    if !delta.is_finite()
        || delta == 0.0
        || (delta < 0.0 && first == 0)
        || (delta > 0.0 && first == last)
    {
        window.wheel = 0.0;
        return;
    }
    if delta.signum() != window.wheel.signum() {
        window.wheel = 0.0;
    }
    window.wheel += delta;
    let steps = (window.wheel / ROW_HEIGHT).trunc();
    window.wheel %= ROW_HEIGHT;
    window.set_first(&rows, first.saturating_add_signed(steps as isize));
    event.prevent_default();
}

fn page_button(label: &'static str, enabled: bool, offset: isize) -> Child {
    Box::new(
        button(label, move |page: &mut TreePage, _| {
            if enabled {
                let remote = page.shared.remote.borrow();
                let rows = remote.card_rows();
                let window = &mut page.sections.cards;
                let first = window.range(&rows).start;
                window.set_first(&rows, first.saturating_add_signed(offset));
            }
        })
        .attr("aria-disabled", (!enabled).to_string()),
    )
}

pub(super) fn view(page: &TreePage, remote: &TreeRemote) -> Vec<Child> {
    let rows = remote.card_rows();
    let total = rows.len();
    let window = &page.sections.cards;
    if total == 0 {
        window.rows.borrow_mut().clear();
        return Vec::new();
    }
    let range = window.range(&rows);
    let mut previous = window.rows.take();
    let mut current = window.rows.borrow_mut();
    let items: Keyed<_, _> = range
        .clone()
        .filter_map(|index| {
            let (key, label) = rows.get(index)?;
            let row = previous
                .remove(&key)
                .filter(|row| row.index == index && row.total == total && row.label == label)
                .unwrap_or_else(|| {
                    #[cfg(feature = "remote-cards-receipt")]
                    super::cards_receipt::record(|sample| sample.rows_built += 1);
                    let view: Child = Box::new(
                        el::<_, TreePage, ()>("li", label.to_owned())
                            .attr("role", "listitem")
                            .attr("aria-posinset", (index + 1).to_string())
                            .attr("aria-setsize", total.to_string())
                            .attr("title", label),
                    );
                    RowView {
                        index,
                        total,
                        label: label.to_owned(),
                        view: Rc::new(view),
                    }
                });
            let view = row.view.clone();
            current.insert(key.clone(), row);
            Some((key, view))
        })
        .collect();
    let list = el("ul", items)
        .attr("class", "tools-cards")
        .attr("role", "list")
        .attr("aria-label", "Cards")
        .attr("tabindex", if total > VISIBLE_ROWS { "0" } else { "-1" });
    let list = on_wheel(on_key(list, handle_key), handle_wheel);
    if total <= VISIBLE_ROWS {
        return vec![Box::new(list)];
    }
    let navigation = el(
        "div",
        (
            page_button("Previous cards", range.start > 0, -(VISIBLE_ROWS as isize)),
            el(
                "span",
                format!("{}–{} of {total}", range.start + 1, range.end),
            ),
            page_button("More cards", range.end < total, VISIBLE_ROWS as isize),
        ),
    )
    .attr("class", "tools-card-navigation");
    vec![Box::new(list), Box::new(navigation)]
}
