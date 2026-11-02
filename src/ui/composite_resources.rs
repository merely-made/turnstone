// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn composite_resources<P: PaintList>(
    viewport: DeviceIntSize,
    plist: &P,
) -> Result<paint_list_render::TranslatedDisplayList, paint_list_render::TranslationError> {
    let layers = [CompositeLayer {
        commands: plist.commands(),
        fonts: plist.fonts(),
        images: plist.images(),
    }];
    composite_paint_layers(viewport, &layers)
}
