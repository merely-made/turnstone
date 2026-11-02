// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyReceiptFact {
    pub fn body(self) -> ApparentBody {
        self.body
    }

    pub fn kind(self) -> SkyFactKind {
        self.kind
    }

    pub fn span(self) -> SkyFactSpan {
        self.span
    }

    /// Event-family model where Turquet exposes one separately from the
    /// ephemeris and observer-transform models retained in the receipt header.
    pub fn calculation_model(self) -> Option<Model> {
        self.calculation_model
    }
}
