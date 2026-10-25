// SPDX-License-Identifier: MPL-2.0
use wgpu_cue::webcue::*;
/// Effective shared operations of this installed adapter, including exact
/// refusals that the older navigation vocabulary groups together.
pub(super) fn report() -> Result<CapabilityReport> {
    CapabilityReport::new(
        BackendIdentity { engine: "graft.servo".into(), version: Some("Servo 0.7.0 / aac43a3f".into()), platform: std::env::consts::OS.into(), build_features: vec!["wgpu-30".into(), if cfg!(target_os = "linux") { "vulkan-dmabuf".into() } else { "metal".into() }, "pre-present-graft".into()] },
        PresentationMode::NativeFrame, Evidence::Constructed, SemanticProtocol::Unavailable,
        Feature::ALL.map(|feature| (feature, match feature {
            Feature::Navigation | Feature::Reload | Feature::History => Support::Partial("asynchronous native navigation; completion acknowledges dispatch, with ordered navigation observations".into()),
            Feature::Zoom => Support::Partial("native page zoom with effective-value readback; content settles asynchronously".into()),
            Feature::Mouse => Support::Partial("move and primary/secondary/auxiliary buttons without modifiers".into()),
            Feature::Scroll => Support::Partial("unmodified pixel wheel input".into()),
            Feature::Focus => Support::Supported,
            Feature::Keyboard => Support::Partial("canonical logical keys; physical codes and repeat are not projected".into()),
            Feature::Text => Support::Partial("logical character-key input; explicit IME composition is not exposed".into()),
            Feature::Stop => Support::Unsupported("pinned Servo has no stop-loading API".into()),
            Feature::InlineHtml => Support::Unsupported("this host loads addresses; inline HTML/base-address construction is not implemented".into()),
            Feature::NativeKeyboard => Support::Unsupported("physical key codes and independent native character fields are not projected".into()),
            _ => Support::Unsupported("not exposed through this concrete shared binding; platform extensions retain their original owner".into()),
        })),
    )
}
