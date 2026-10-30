// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Ask one surface to show `scale` as its page zoom, reporting a refusal
/// without letting it stand in for the requested value. Split from the effect
/// arm so the spawn path can seed a producer before it enters the map.
pub(super) fn apply_page_scale(
    app: &mut crate::app::App,
    node: uuid::Uuid,
    scale: f32,
    producer: &mut dyn inker::SurfaceProducer,
) {
    if let Err(error) = producer.apply_settings(&inker::SurfaceSettings {
        zoom_factor: f64::from(scale),
        ..Default::default()
    }) {
        let error = error.to_string();
        tracing::warn!(%node, scale, %error, "page zoom apply failed");
        app.note(crate::observe::AppEvent::AffordanceUnavailable {
            what: "page-zoom",
            target: error,
        });
    }
}
