// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A caret movement within the omnibar's single line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CaretMove {
    Left,
    Right,
    Home,
    End,
}
