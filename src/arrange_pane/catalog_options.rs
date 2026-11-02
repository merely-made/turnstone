// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn catalog_options(catalog: &[(&str, &str)]) -> Vec<(String, String)> {
    catalog
        .iter()
        .map(|(id, label)| ((*id).to_string(), (*label).to_string()))
        .collect()
}
