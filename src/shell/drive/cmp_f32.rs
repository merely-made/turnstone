// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::scenario::CmpOp;

pub(super) fn cmp_f32(op: &CmpOp, a: f32, b: f32) -> bool {
    match op {
        CmpOp::Eq => (a - b).abs() < 1e-3,
        CmpOp::Ge => a >= b,
        CmpOp::Le => a <= b,
    }
}
