// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn html(color: &str, name: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset='utf-8'><title>Cue {name}</title><style>html,body{{margin:0;background:{color};font:20px sans-serif}}input{{position:absolute;left:16px;top:16px;width:160px;height:40px;font:20px sans-serif}}a{{position:absolute;left:16px;top:90px;color:white}}</style></head><body><input value='hello' oninput=\"document.title='Cue value='+this.value\"><a href='page-b.html'>next</a></body></html>"
    )
}
