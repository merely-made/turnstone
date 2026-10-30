// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn decode<const N: usize>(s: &str) -> Result<[u8; N], String> {
    if !s.is_ascii() || s.len() != N * 2 {
        return Err("invalid stored run identity".into());
    }
    let mut out = [0; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| "invalid stored run identity")?
    }
    Ok(out)
}
