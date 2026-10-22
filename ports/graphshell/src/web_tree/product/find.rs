// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Find scopes the canvas using the saved graph owner's relation vocabulary.
use super::*;
use cambium::{button, el, lens, select, text_field_typed};
use graphshell::product::RelationFamilyFilter;

const FAMILIES: [(RelationFamilyFilter, &str); 7] = [
    (RelationFamilyFilter::All, "All relations"),
    (RelationFamilyFilter::Semantic, "Semantic"),
    (RelationFamilyFilter::Traversal, "Traversal"),
    (RelationFamilyFilter::Containment, "Containment"),
    (RelationFamilyFilter::Arrangement, "Arrangement"),
    (RelationFamilyFilter::Imported, "Imported"),
    (RelationFamilyFilter::Provenance, "Provenance"),
];

pub(super) struct FindControls {
    pub(super) open: bool,
    pub(super) query: TextInput,
    family: SelectState,
    count: Option<usize>,
}

impl Default for FindControls {
    fn default() -> Self {
        Self {
            open: false,
            query: TextInput::new(""),
            family: SelectState::new(0).with_label("Relation family"),
            count: None,
        }
    }
}

impl SavedProduct {
    pub(in crate::web_tree) fn filter_count(&self) -> usize {
        self.find.count.unwrap_or_else(|| {
            self.app
                .as_ref()
                .map_or(0, |app| app.host.graph().node_count())
        })
    }

    fn filter(&mut self, canvas: &mut Canvas, clear: bool) {
        let Some(app) = &self.app else { return };
        if clear {
            self.find.query = TextInput::new("");
            self.find.family.selected = 0;
        }
        let family = FAMILIES[self.find.family.selected.min(FAMILIES.len() - 1)].0;
        let query = self.find.query.text();
        let matches = app.host.matching_members(query, family);
        self.find.count = Some(matches.len());
        if query.trim().is_empty() && family == RelationFamilyFilter::All {
            canvas.clear_scope();
        } else {
            canvas.scope_to_members(matches.iter().copied());
        }
        self.status = format!("{} graph object(s) match", matches.len());
    }
}

pub(in crate::web_tree) fn section(page: &TreePage) -> Child {
    let Some(product) = &page.product else {
        return Box::new(el("div", ()));
    };
    let mut children: Vec<Child> = vec![Box::new(
        disclosure_trigger("Find and filter", product.find.open, |page: &mut TreePage| {
            if let Some(product) = &mut page.product {
                product.find.open = !product.find.open;
            }
        }),
    )];
    if product.find.open {
        children.push(Box::new(lens(
            |input: &mut TextInput| text_field_typed(input).attr("aria-label", "Search"),
            |page: &mut TreePage| &mut page.product.as_mut().expect("local find").find.query,
        )));
        children.push(Box::new(lens(
            |state: &mut SelectState| select(state, &FAMILIES.map(|(_, label)| label)),
            |page: &mut TreePage| &mut page.product.as_mut().expect("local find").find.family,
        )));
        children.push(Box::new(button(
            "Apply filter",
            |page: &mut TreePage, _| {
                if let Some(product) = &mut page.product {
                    product.filter(&mut page.shared.canvas.borrow_mut(), false);
                }
                page.shared.dirty.set(true);
            },
        )));
        children.push(Box::new(button(
            "Clear filter",
            |page: &mut TreePage, _| {
                if let Some(product) = &mut page.product {
                    product.filter(&mut page.shared.canvas.borrow_mut(), true);
                }
                page.shared.dirty.set(true);
            },
        )));
    }
    Box::new(el("section", children).attr("aria-label", "Find and filter"))
}
