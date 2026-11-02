// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The protocol conversation captured by the omnibar's smolweb input mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmolwebInputPrompt {
    pub node: Uuid,
    /// The member address that initiated the actor request.
    pub requested_url: String,
    /// The final redirect target whose query the answer belongs to.
    pub input_url: String,
    pub prompt: String,
    pub sensitive: bool,
}
