// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn private_source_choices(
    owner: &Owner,
    entries: &[Entry],
    current_admissions: &[Admission],
    custody_open: bool,
) -> Vec<SourceChoice> {
    entries
        .iter()
        .filter(|entry| {
            entry.admission.key.session.as_uuid().to_string() == owner.session
                && entry.admission.key.graph.as_uuid().to_string() == owner.workspace
        })
        .map(|entry| {
            let key = saved_source_choice_key(entry).unwrap_or_else(|| {
                format!(
                    "private-unavailable:{}:{}",
                    entry.admission.key.node, entry.admission.occurrence
                )
            });
            let content = entry.content.as_ref();
            let current = current_admissions
                .iter()
                .any(|admission| admission == &entry.admission && admission.error.is_none());
            let sealed = entry.admission.key.is_sealed();
            let valid_hash = content.is_some_and(|content| {
                eidetic::Hash::parse(&content.text_hash)
                    .is_ok_and(|hash| hash == eidetic::Hash::of(content.text.as_bytes()))
            });
            let bounded = content.is_some_and(|content| {
                !content.text.is_empty() && content.text.len() <= MAX_PRIVATE_SOURCE_BYTES
            });
            let refusal = if !custody_open {
                Some("Unlock the profile to select a sealed saved source.".into())
            } else if !sealed {
                Some("Legacy unsealed saved sources cannot enter a private evidence brief.".into())
            } else if !current || entry.error.is_some() || entry.admission.error.is_some() {
                Some("This saved source is no longer admitted in the current graph.".into())
            } else if !valid_hash {
                Some("The retained source version failed its content hash check.".into())
            } else if !bounded {
                Some("Choose a shorter retained passage (up to 2048 UTF-8 bytes).".into())
            } else {
                None
            };
            let disclose_metadata = custody_open
                && sealed
                && current
                && entry.error.is_none()
                && entry.admission.error.is_none()
                && valid_hash
                && bounded;
            SourceChoice {
                key,
                title: if disclose_metadata {
                    entry
                        .content
                        .as_ref()
                        .map(|content| content.title.clone())
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| entry.admission.title.clone())
                } else {
                    "Private saved source".into()
                },
                version: if disclose_metadata {
                    content
                        .map(|content| {
                            let short_hash: String = content.text_hash.chars().take(12).collect();
                            format!(
                                "Full retained passage · {} bytes · {}",
                                content.text.len(),
                                short_hash
                            )
                        })
                        .unwrap_or_else(|| "unavailable".into())
                } else {
                    "locked".into()
                },
                selected: false,
                available: refusal.is_none(),
                refusal,
            }
        })
        .collect()
}
