// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Parse a behavior's actuation deadband.
///
/// Both numbers are positive integers. A malformed declaration refuses the
/// install instead of silently admitting the behavior without the bound it
/// claimed to carry.
pub fn parse_deadband(source: &str) -> Result<Option<servitor::Deadband>, String> {
    let Some(raw) = source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("--")?
            .trim()
            .strip_prefix("@deadband")
            .map(str::trim)
    }) else {
        return Ok(None);
    };
    let mut parts = raw.split_whitespace();
    let minimum_change: u64 = parts
        .next()
        .ok_or_else(|| "@deadband requires <minimum-change> <minimum-interval-ms>".to_string())?
        .parse()
        .map_err(|_| "@deadband minimum change must be a positive integer".to_string())?;
    let minimum_interval_ms: u64 = parts
        .next()
        .ok_or_else(|| "@deadband requires <minimum-change> <minimum-interval-ms>".to_string())?
        .parse()
        .map_err(|_| "@deadband minimum interval must be a positive integer".to_string())?;
    if parts.next().is_some() {
        return Err("@deadband takes exactly two integers".to_string());
    }
    servitor::Deadband::new(minimum_change, minimum_interval_ms)
        .map(Some)
        .map_err(|err| format!("invalid @deadband: {err}"))
}
