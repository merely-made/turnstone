// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Castellan's secret-free identity read model, at its pre-founding path.
//!
//! The types moved home to [`castellan::view`] when the keeper surface was
//! founded (2026-08-14): by the port law, identity is a capability the stack
//! owns, castellan is its port, and graphshell composes it. This shim keeps
//! every existing graphshell call site — the endpoint, the native hosts, the
//! receipt bins — compiling unchanged.
//!
//! Since the dramatis repo plan's DR-A (2026-10-09) the types themselves live
//! in `dramatis::view`; castellan re-exports them, and this shim reaches them
//! through castellan.

pub use castellan::view::*;
