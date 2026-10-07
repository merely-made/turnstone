# Direct dependency currency snapshot

Observed October 6, 2026 EDT, completed October 7 at 02:08 UTC. This is a
read-only dependency survey, separate from compile, native and package receipts.
[snapshot.json](snapshot.json) records 229 direct declarations, 80 official
registry responses, 33 manifest/lock input hashes, repository heads and the
selected Turnstone lock identities. API queries completed without errors.

The library rows describe both current source declarations and the actual
published dependency metadata for `grafting 0.6.0`, `grafting-frame 0.1.0`,
`scrying 0.7.1` and `welding 0.14.1`. The Servo adapter and demos have separate
scope fields. Workspace lock identities include other consumers and optional
rows; they are not a resolved enabled-feature graph. Concurrent source work is
identified by input hashes rather than assumed clean. No Cargo command, resolver
operation, source/manifest change, runtime installation or publication occurred.

## Runtime and adapter migrations

| Boundary | Observed consumer/source | Current upstream | Consequence |
|---|---|---|---|
| Servo adapter and demos | Git `1d44e5dd`, August 15; crate 0.5.0 | [0.7.0](https://github.com/servo/servo/releases/tag/v0.7.0), October 5; [0.6 LTS](https://github.com/servo/servo/releases/tag/v0.6.0), September 29 | A coordinated adapter, ANGLE and native migration. Grafting's published core has no Servo dependency. |
| wgpu / wgpu-hal | Turnstone 30.0.1 | [30.0.1](https://github.com/gfx-rs/wgpu/releases/tag/v30.0.1), August 22 | Current host GPU line. |
| CEF | Turnstone 151.8.1+151.3.24; Weld workspace 151.8.0+151.3.24 | [154.4.0+154.0.33](https://github.com/tauri-apps/cef-rs/releases/tag/cef-v154.4.0%2B154.0.33), October 3 | Three engine majors newer. First the existing 151 patch drift, then an explicit 154 API/SDK/native battery; shared ANGLE loaded-module identity remains relevant. |
| WebView2 Rust bindings | 0.39.1 | [0.39.1](https://crates.io/api/v1/crates/webview2-com), March 11 | Current binding crate; this does not establish installed runtime currency. |
| WebView2 runtime / SDK | Local registry `pv` 156.0.4314.8; version directories 155.0.4283.33 and 156.0.4314.8 | Newest documented [stable runtime entry 154.0.4258.31](https://learn.microsoft.com/en-us/microsoft-edge/webview2/release-notes/runtime/) / [SDK 1.0.4258.31](https://learn.microsoft.com/en-us/microsoft-edge/webview2/release-notes/sdk/), September 28 | Release entries can omit later servicing patches. Installed version, SDK, binding and actual native runtime selection are separate facts. |
| Linux WebKit runtimes | Scry WPE requires pkg-config >=2.52; installed versions unmeasured | [WebKitGTK 2.54.1](https://webkitgtk.org/2026/10/02/webkitgtk2.54.1-released.html), October 2; [WPE WebKit 2.54.1](https://wpewebkit.org/release/wpewebkit-2.54.1.html), October 6 | Qualify actual installed runtime, WPEPlatform callback/input and DMABUF import; a manifest floor does not date the engine. WKWebView follows its host OS. |

Servo v0.7 resolves to `aac43a3f31a259f04a574f5ec4e959c943ad7cc7`.
The [comparison](https://github.com/servo/servo/compare/1d44e5dd6a8b64c02f9dbf7fcbdf4ebdd0740019...v0.7.0)
is **diverged**, with 921 release-side commits and two pin-only commits, sharing
merge base `d6ade2f68725062eb04db7670766aad01192320c`. This is an older release
branch, not merely 921 commits behind main. The new source uses surfman 0.14
instead of 0.13, mozangle 0.7 instead of 0.6 and ipc-channel 0.23 instead of
0.22. Upstream toolchain moves 1.95 to 1.97.1; declared MSRV remains 1.88 and
the consumer already uses 1.98.1. `RenderingContext::connection` carries a
surfman type, so a plain Servo repin would split the adapter's nominal types.

The read-only WebView2 inventory queried EdgeUpdate client
`F3017226-FE2A-4295-8BDF-00C3A9A7E4C5` and existing Application directories.
Both executables report file/product versions matching their directory names.
This proves local installation inventory only; it does not identify the channel
or the runtime used by any native run. The completed run's loaded module paths
and versions must establish actual selection separately.

The initial all-three-engine build's **fontsan conflict is a preserved failure
control** while owner repair proceeds. Both Servo revisions choose fontsan 0.7
with `libz-sys,wuff`; Genet `69a2383b` enabled defaults `libz-sys,woff2`.
Additive features enabled both mutually exclusive decoders. A Servo 0.7 repin
does not fix it. Genet Livery uses the common `fontsan::process` API, so align
the codec edge at its owner and qualify valid WOFF2 decoding, malformed-input
rejection and unchanged SFNT identity. See the [features](https://github.com/servo/fontsan/blob/main/Cargo.toml)
and [exactly-one guard](https://github.com/servo/fontsan/blob/main/build.rs).

## Published libraries and intentional compatibility

The wgpu 28/29/30 features are intentional host compatibility rows, not three
stale engines. Scry/Weld locks carry 29.0.4; Graft carries 29.0.3, one patch
behind. Their `29.0.1` declarations allow 29.0.4. Graft's demos can be qualified
on that patch without changing Turnstone's current wgpu 30 device. Its isolated
Iced workspace deliberately retains wgpu 28 because of the framework's web-sys
constraint. Lockfile-only wgpu 26 is a demo/transitive identity, not Turnstone's
selected device.

Library migrations beyond the browser engines are bounded: Graft declares
[gleam 0.15.1](https://crates.io/api/v1/crates/gleam) versus 0.16.0 (October 2)
and [glow 0.17](https://crates.io/api/v1/crates/glow) versus 0.18 (July 9), but
Servo 0.7/surfman 0.14 still use glow 0.17, so preserve their common GL types.
Weld's [base64 0.22](https://crates.io/api/v1/crates/base64) has 0.23.1 available
(August 4); Scry's [windows-numerics 0.3](https://crates.io/api/v1/crates/windows-numerics)
has 0.100.0 available (September 3). These need targeted API checks, not a
browser migration bundled with them. [Pollster 0.4 to 1.0.1](https://crates.io/api/v1/crates/pollster)
is dev/demo dependency work, not library runtime drift.

The GTK3 fallback is a coupled binding family: current published
[webkit2gtk 2.0.2](https://crates.io/api/v1/crates/webkit2gtk/2.0.2/dependencies)
requires gtk/glib/gio 0.18, JavaScriptCore 1.1 and soup3 0.5. Their newer major
lines cannot be individually substituted. Scry's separate webkit6 0.6.1 path
is current; its GTK4/GSK 0.11.4 and GLib/GIO 0.22.9 locks have patch updates
0.11.5 and 0.22.10. Other patch candidates include objc2 0.6.4 to 0.6.5,
libc, thiserror and adapter rustls. Exact versions/dates/requirements are in the
snapshot; absence of an update claim is not a security audit.

## Demo framework currency

Graft demos have framework drift independently of published library consumers:
eframe 0.35 to 0.36.2 (September 8), Slint 1.17 to 1.18.1 (September 21), and
Bevy 0.19.0 to 0.19.1 (August 13). The Blitz lane's anyrender 0.10,
anyrender_vello 0.10 and Vello 0.9 have current lines 0.14, 0.15 and 0.11
(October 2-3). Latest anyrender_vello/Vello explicitly support wgpu 30, making
that a useful coordinated demo migration candidate. It does not qualify a
Turnstone renderer change. Xilem/Masonry 0.4, winit 0.30.13 and the gpui registry
version 0.2.2 are current; GPUI is actually the patched Glass fork in this
workspace, so its package version alone cannot establish fork currency.

The JSON records direct requirements, primary API links and publication dates
for every surveyed crate. Source refreshes, package contents/MSRV, four-host
native gates and registry-only consumer release gates remain separate.

## Later accessibility follow-up

The snapshot above stays a read-only observation of its stated inputs. A later
source trace of Servo 0.7 at `aac43a3` found a material accessibility change:
[`Servo::forward_accessibility_action`](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/components/servo/servo.rs#L1120-L1131)
accepts an original AccessKit `ActionRequest`, absent in the selected 0.5 API.
Constellation routes by tree/pipeline, script checks the live document and
layout resolves the local node during reflow. The final
[DOM handler](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/components/script/dom/window/window.rs#L3832-L3843)
implements only Click, with an untrusted synthetic click. Other actions fall
through; the public call returns no typed completion/refusal. This can supply
an engine-routed assistive click after host integration, but does not establish
Focus, text/value, selection or scrolling support. AccessKit's caret remains
compatible with the host's 0.24.1 family. U2 raises this coherent Servo migration
priority without making a version bump sufficient for accessibility.
The complete producer-chain trace also finds empty action masks and no Click
or child-action advertisement; native invocability therefore needs work even
where the public bridge can execute a programmatic Click.
The [accessibility inventory](../accessibility-reconciliation/README.md)
records the additional host and supplier gates. No Servo 0.7 consumer repin occurred.
