// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn surface_cursor_icon(shape: inker::CursorShape) -> Option<CursorIcon> {
    match shape {
        inker::CursorShape::Default => Some(CursorIcon::Default),
        inker::CursorShape::Text => Some(CursorIcon::Text),
        inker::CursorShape::Pointer => Some(CursorIcon::Pointer),
        inker::CursorShape::Crosshair => Some(CursorIcon::Crosshair),
        inker::CursorShape::Move => Some(CursorIcon::Move),
        inker::CursorShape::ResizeNs => Some(CursorIcon::NsResize),
        inker::CursorShape::ResizeEw => Some(CursorIcon::EwResize),
        inker::CursorShape::ResizeNesw => Some(CursorIcon::NeswResize),
        inker::CursorShape::ResizeNwse => Some(CursorIcon::NwseResize),
        inker::CursorShape::Grab => Some(CursorIcon::Grab),
        inker::CursorShape::Grabbing => Some(CursorIcon::Grabbing),
        inker::CursorShape::NotAllowed => Some(CursorIcon::NotAllowed),
        inker::CursorShape::Hidden => None,
    }
}
