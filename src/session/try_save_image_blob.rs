// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn try_save_image_blob(
    data_root: &Path,
    hex: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let dir = images_dir(data_root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(hex);
    if path.try_exists()? {
        if std::fs::read(&path)? == bytes {
            return Ok(());
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "existing content-addressed image blob has different bytes",
        ));
    }
    std::fs::write(path, bytes)
}
