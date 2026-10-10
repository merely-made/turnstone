// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Typed field access for the shared host caret and IME path.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum TextField {
    Title,
    Tags,
    IntakeAddress,
    IntakeTitle,
    FacetId,
    FacetJson,
    RelationTarget,
    Search,
    CommandSearch,
    Codicil,
    Projection(usize),
    ProjectionOption(usize),
    History(usize),
}

impl TextField {
    pub(super) fn get(self, product: &SavedProduct) -> &TextInput {
        match self {
            Self::Title => &product.title,
            Self::Tags => &product.tags,
            Self::IntakeAddress => &product.intake_address,
            Self::IntakeTitle => &product.intake_title,
            Self::FacetId => &product.detail.facet_id,
            Self::FacetJson => &product.detail.facet_json,
            Self::RelationTarget => &product.detail.target,
            Self::Search => &product.find.query,
            Self::CommandSearch => &product.commands.query,
            Self::Codicil => &product.scenes.codicil,
            Self::Projection(index) => &product.projection.inputs[index],
            Self::History(index) => &product.history.inputs[index],
            Self::ProjectionOption(index) => &product.projection.options[index].input,
        }
    }
    pub(super) fn get_mut(self, product: &mut SavedProduct) -> &mut TextInput {
        match self {
            Self::Title => &mut product.title,
            Self::Tags => &mut product.tags,
            Self::IntakeAddress => &mut product.intake_address,
            Self::IntakeTitle => &mut product.intake_title,
            Self::FacetId => &mut product.detail.facet_id,
            Self::FacetJson => &mut product.detail.facet_json,
            Self::RelationTarget => &mut product.detail.target,
            Self::Search => &mut product.find.query,
            Self::CommandSearch => &mut product.commands.query,
            Self::Codicil => &mut product.scenes.codicil,
            Self::Projection(index) => &mut product.projection.inputs[index],
            Self::History(index) => &mut product.history.inputs[index],
            Self::ProjectionOption(index) => &mut product.projection.options[index].input,
        }
    }
}

/// Map the retained fields to the host's caret, IME and selection path.
pub(in crate::web_tree) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    let product = runner.state().product.as_ref()?;
    if product.saving || product.pending_intake.is_some() || product.pending_detail.is_some() {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let field = [
        ("Title", TextField::Title, product.detail_open),
        ("Search", TextField::Search, product.find.open),
        (
            "Search commands",
            TextField::CommandSearch,
            product.menu_open(),
        ),
        ("Codicil", TextField::Codicil, product.scenes.open),
        ("Tags", TextField::Tags, product.detail_open),
        ("Facet id", TextField::FacetId, product.detail_open),
        ("Facet JSON", TextField::FacetJson, product.detail_open),
        (
            "Relate to address",
            TextField::RelationTarget,
            product.detail_open,
        ),
        ("Address", TextField::IntakeAddress, product.intake_open),
        (
            "New object title",
            TextField::IntakeTitle,
            product.intake_open,
        ),
    ]
    .into_iter()
    .chain(
        projection::FIELDS
            .iter()
            .enumerate()
            .map(|(index, (_, label, _))| {
                (
                    *label,
                    TextField::Projection(index),
                    product.projection.open,
                )
            }),
    )
    .chain(
        history::LABELS
            .iter()
            .enumerate()
            .map(|(index, label)| (*label, TextField::History(index), product.history.open)),
    )
    .chain(
        product
            .projection
            .options
            .iter()
            .enumerate()
            .map(|(index, row)| {
                (
                    row.spec.label.as_str(),
                    TextField::ProjectionOption(index),
                    product.projection.open,
                )
            }),
    )
    .find_map(|(label, field, visible)| {
        (visible
            && taproot::matching(&dom, &Selector::role("textbox").containing(label))
                .contains(&node))
        .then_some(field)
    })?;
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(move |page: &TreePage| {
            let product = page.product.as_ref().expect("focused local detail");
            field.get(product)
        }),
        get_mut: Box::new(move |page: &mut TreePage| {
            let product = page.product.as_mut().expect("focused local detail");
            field.get_mut(product)
        }),
    })
}
