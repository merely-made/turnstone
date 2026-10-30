// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The document's path inside the vault, with `/` separators.
///
/// Both the plain root and its canonical form are tried: the disclosed
/// address comes from a directory walk and the configured root comes from the
/// environment, and on Windows those differ by a verbatim prefix often enough
/// that one comparison is not enough.
pub(super) fn vault_relative(
    path: &std::path::Path,
    vault_root: &std::path::Path,
) -> Option<String> {
    let canonical_path = std::fs::canonicalize(path);
    let canonical_root = std::fs::canonicalize(vault_root);
    let candidates = [
        (Some(path), Some(vault_root)),
        (
            canonical_path.as_deref().ok(),
            canonical_root.as_deref().ok(),
        ),
        (canonical_path.as_deref().ok(), Some(vault_root)),
        (Some(path), canonical_root.as_deref().ok()),
    ];
    candidates.into_iter().find_map(|(path, root)| {
        let relative = path?.strip_prefix(root?).ok()?;
        let relative = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        (!relative.is_empty()).then_some(relative)
    })
}
