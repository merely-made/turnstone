// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn fill_title(page: &mut PageRef, titles: &HashMap<&str, &str>) {
    if page.title.is_none()
        && let Some(title) = titles.get(page.url.as_str())
    {
        page.title = Some((*title).to_string());
    }
}
