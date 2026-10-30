// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Turnstone opts into linked gemtext images, while keeping the presentation
/// and resource budgets user-configurable at process launch.
pub(super) fn smolweb_inline_media_policy() -> SmolwebInlineMediaPolicy {
    let mut policy = SmolwebInlineMediaPolicy::images();
    policy.enabled = std::env::var("TURNSTONE_SMOLWEB_INLINE_IMAGES")
        .ok()
        .map(|value| !matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "off"))
        .unwrap_or(true);
    if let Some(limit) = std::env::var("TURNSTONE_SMOLWEB_INLINE_IMAGE_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        policy.max_images = limit;
    }
    if let Some(megabytes) = std::env::var("TURNSTONE_SMOLWEB_INLINE_IMAGE_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    {
        policy.max_encoded_bytes_per_image = megabytes.saturating_mul(1024 * 1024);
    }
    policy
}
