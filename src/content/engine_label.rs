// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Names known engines without using a closed list to decide registration.
pub fn engine_label(id: &str) -> String {
    match id {
        "genet.livery" => "Genet",
        "genet.reader" => "Reader",
        crate::retained_clip::ENGINE_ID => "Retained clip",
        crate::saved_sources::ENGINE_ID => "Saved source",
        "scrying.web" => "System webview",
        "graft.servo" => "Servo",
        "weld.chromium" => "Chromium",
        "nematic.gemtext" => "Gemini / Spartan",
        "nematic.gopher" => "Gopher",
        "nematic.markdown" => "Markdown",
        "nematic.djot" => "Djot",
        "nematic.text" => "Plain text",
        "nematic.knot" => "Knot",
        "nematic.knot-djot" => "Djot",
        "nematic.file" => "Local file",
        "nematic.finger" => "Finger",
        "nematic.scroll" => "Scroll",
        "nematic.nex" => "Nex",
        "nematic.guppy" => "Guppy",
        "nematic.titan" => "Titan",
        "nematic.feed" => "Feed",
        nematic::ENGINE_MICRON => "NomadNet",
        "knot.authoring" => "Knot",
        _ => id,
    }
    .into()
}
