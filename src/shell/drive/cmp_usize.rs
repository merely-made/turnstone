// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::scenario::CmpOp;

pub(super) fn cmp_usize(op: &CmpOp, a: usize, b: usize) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ge => a >= b,
        CmpOp::Le => a <= b,
    }
}
