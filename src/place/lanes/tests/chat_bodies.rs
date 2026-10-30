// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn chat_bodies(open: &crate::place::worker::OpenPlace) -> Vec<String> {
    pollster::block_on(open.chat.projection())
        .unwrap()
        .messages
        .into_iter()
        .map(|message| message.message.body)
        .collect()
}
