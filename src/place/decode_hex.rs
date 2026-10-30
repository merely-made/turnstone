// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn decode_hex(kind: &'static str, value: &str) -> Result<[u8; 32], PlaceIdError> {
    if value.len() != 64 {
        return Err(PlaceIdError {
            kind,
            reason: "expected 64 lowercase hexadecimal characters",
        });
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(PlaceIdError {
            kind,
            reason: "identifier contains a non-lowercase-hexadecimal character",
        });
    }
    let mut bytes = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| PlaceIdError {
            kind,
            reason: "identifier is not UTF-8 hexadecimal",
        })?;
        bytes[index] =
            u8::from_str_radix(text, 16).expect("validated hexadecimal pair always parses");
    }
    Ok(bytes)
}
