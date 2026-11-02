// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Split an invite prompt line into its pre-key path and optional grant
/// lifetime, `<path> for <n><s|m|h|d>`. Read from the right, so a path with
/// spaces survives; a tail that is not a lifetime is part of the path.
pub fn split_invite_lifetime(line: &str) -> Result<(&str, Option<u64>), String> {
    let Some((path, tail)) = line.rsplit_once(" for ") else {
        return Ok((line, None));
    };
    let Some(unit) = tail.chars().last() else {
        return Ok((line, None));
    };
    let digits = &tail[..tail.len() - unit.len_utf8()];
    let scale: u64 = match unit {
        's' => 1_000,
        'm' => 60_000,
        'h' => 3_600_000,
        'd' => 86_400_000,
        _ => return Ok((line, None)),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok((line, None));
    }
    let lifetime = digits
        .parse::<u64>()
        .ok()
        .and_then(|count| count.checked_mul(scale))
        .ok_or_else(|| format!("grant lifetime {tail} is too long"))?;
    if lifetime == 0 {
        return Err("a grant lifetime must be longer than zero".into());
    }
    Ok((path.trim_end(), Some(lifetime)))
}
