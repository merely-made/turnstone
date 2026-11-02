// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn surface_cursor_label(shape: inker::CursorShape) -> &'static str {
    match shape {
        inker::CursorShape::Default => "default",
        inker::CursorShape::Text => "text",
        inker::CursorShape::Pointer => "pointer",
        inker::CursorShape::Grab => "grab",
        inker::CursorShape::Grabbing => "grabbing",
        inker::CursorShape::Crosshair => "crosshair",
        inker::CursorShape::Move => "move",
        inker::CursorShape::ResizeNs => "resize-ns",
        inker::CursorShape::ResizeEw => "resize-ew",
        inker::CursorShape::ResizeNesw => "resize-nesw",
        inker::CursorShape::ResizeNwse => "resize-nwse",
        inker::CursorShape::NotAllowed => "not-allowed",
        inker::CursorShape::Hidden => "hidden",
    }
}
