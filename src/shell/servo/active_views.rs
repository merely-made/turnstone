// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn active_views() -> usize {
    ROOT.with(|slot| slot.borrow().as_ref().map_or(0, |root| root.views.get()))
}
