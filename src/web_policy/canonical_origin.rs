// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(crate) fn canonical_origin(input: &str) -> String {
    match url::Url::parse(input) {
        Ok(url) => {
            let origin = url.origin().ascii_serialization();
            if origin == "null" {
                input.trim().to_ascii_lowercase()
            } else {
                origin
            }
        },
        Err(_) => input.trim().to_ascii_lowercase(),
    }
}
