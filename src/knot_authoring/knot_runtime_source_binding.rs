// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn knot_runtime_source_binding(
    source: SourceBinding,
    base_token: Vec<u8>,
) -> RuntimeSourceBinding<KnotRuntimeWitness> {
    RuntimeSourceBinding::new(source, KnotRuntimeWitness { base_token })
}
