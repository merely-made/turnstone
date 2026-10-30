// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Product error vocabulary over the standard once-only initializer.
//! Synchronization, single evaluation and failure retention belong to LazyLock.
pub(crate) type ServiceInit<T> =
    std::sync::LazyLock<Result<T, String>, Box<dyn FnOnce() -> Result<T, String> + Send>>;
