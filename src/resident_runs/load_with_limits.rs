// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn load_with_limits(dir: &Path, limits: StorageLimits) -> Result<ResidentRuns, String> {
    let target = path(dir);
    let cap = limits
        .max_bytes
        .checked_add(1)
        .ok_or_else(|| "resident run byte bound is invalid".to_string())?;
    let file = std::fs::File::open(&target).map_err(|error| {
        format!(
            "resident run state unreadable at {}: {error}",
            target.display()
        )
    })?;
    let mut raw = Vec::new();
    file.take(cap).read_to_end(&mut raw).map_err(|error| {
        format!(
            "resident run state unreadable at {}: {error}",
            target.display()
        )
    })?;
    if raw.len() as u64 > limits.max_bytes {
        return Err("resident run state exceeds its configured storage bound".into());
    }
    let disk: DiskState = serde_json::from_slice(&raw).map_err(|error| {
        format!(
            "resident run state malformed at {}: {error}",
            target.display()
        )
    })?;
    if disk.version != VERSION {
        return Err(format!(
            "resident run state has unsupported version {}",
            disk.version
        ));
    }
    let state = ResidentRuns {
        limits,
        next_id: disk.next_id,
        runs: disk.runs,
    };
    state.validate()?;
    Ok(state)
}
