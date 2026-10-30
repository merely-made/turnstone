// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// What an invitation admits its recipient as.
///
/// Not carried on the wire: an invitation's envelope is unchanged, and the
/// difference is realized in the Moot the recipient then folds. A reader is
/// admitted to membership at `Read` and holds no delegation, so it reads the
/// place and its own worker refuses everything it tries to author.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaceInviteAccess {
    #[default]
    Writer,
    Reader,
}
