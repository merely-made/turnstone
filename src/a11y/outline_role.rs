// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Map the outline's coarse role strings (genet's `role_of`) onto AccessKit
/// roles. Unknown strings stay groups.
pub(super) fn outline_role(entry: &OutlineFact) -> Role {
    match entry.role {
        "link" => Role::Link,
        "button" => Role::Button,
        "textbox" => Role::TextInput,
        "paragraph" => Role::Paragraph,
        "heading" => Role::Heading,
        "list" => Role::List,
        "listitem" => Role::ListItem,
        "image" => Role::Image,
        "label" => Role::Label,
        "navigation" => Role::Navigation,
        "banner" => Role::Banner,
        "contentinfo" => Role::ContentInfo,
        "main" => Role::Main,
        "region" => Role::Section,
        _ => Role::Group,
    }
}
