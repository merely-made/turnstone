// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The native consumer of Djinn's graph co-op routes.
pub(crate) mod controls;
pub(crate) mod guest;
pub(crate) mod worker;

pub(crate) type Child<S> = mere_view::MereViewElement<S, ()>;
