// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Whether an address names playable podcast audio, by declared media type
/// first and by extension when a feed declared none.
pub fn is_podcast_audio(url: &str, media_type: Option<&str>) -> bool {
    if let Some(media_type) = media_type {
        let media_type = media_type.split(';').next().unwrap_or("").trim();
        if media_type.eq_ignore_ascii_case("audio/mpeg")
            || media_type.eq_ignore_ascii_case("audio/mp4")
            || media_type.eq_ignore_ascii_case("audio/aac")
            || media_type.eq_ignore_ascii_case("audio/x-m4a")
        {
            return true;
        }
    }
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let lowered = path.to_ascii_lowercase();
    lowered.ends_with(".mp3") || lowered.ends_with(".m4a") || lowered.ends_with(".aac")
}
