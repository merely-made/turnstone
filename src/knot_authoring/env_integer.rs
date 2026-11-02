// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn env_integer<T>(name: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse()
                .map_err(|error| format!("{name} must be an integer: {error}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}
