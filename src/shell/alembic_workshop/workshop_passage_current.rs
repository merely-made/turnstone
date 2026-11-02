// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn workshop_passage_current(
    app: &crate::app::App,
    owner: &Owner,
    protection: Option<&dyn athanor::workshop_seal::Protection>,
    passage: &alembic::workshop::Passage,
) -> bool {
    if passage.public {
        crate::alembic_workshop::passage_current(app, owner, passage)
    } else {
        protection.is_some_and(|protection| {
            crate::alembic_workshop::private_passage_current(app, owner, protection, passage)
        })
    }
}
