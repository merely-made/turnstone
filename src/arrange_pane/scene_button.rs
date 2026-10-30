// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn scene_button(verb: CanvasCommand) -> ArrangeView {
    apply_button(verb.label(), move |_| Some(ArrangeIntent::Verb(verb)))
}
