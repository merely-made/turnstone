# Browser accessibility reconciliation, 2026-10-06

This is a bounded source and primary-API inventory, not a native accessibility
qualification. U2 in the [unusual-protocols plan](../../../../design_docs/2026-10-06_unusual_protocols_browser_plan.md)
puts accessibility on Windows, macOS and Linux first. The existing
[Scry/Weld receipt](../README.md) qualifies its stated pixel, cookie, input and
custody behavior; it does not qualify foreign-page accessibility. No build,
application run, source change or dependency migration was performed for this
inventory.

## Source identity and scope

| Owner | Primary revision observed | Consumer selection observed |
|---|---|---|
| Turnstone | `032463aedd49234ef07050a8cb2035d39eb83bf1`, shared dirty tree | [working manifest](../../../../Cargo.toml) and [lock](../../../../Cargo.lock); browser source remains in progress |
| Mere | `26857eafe293a66e30c785d85fafe44d1113c11e` | `edf175f9c0a8645318ac8925adf0af6956f61425` |
| Genet | `90c5ef507db943204d5493a1a7dd817695557cda` | `679d8314aab4ec9f57a903c79dde244c3c565c1e` |
| Scry | `2c3ebd24118851cf6125cfb193f55aab20e3ad67` | `39818a7eddb8bec33f7c61ed144da226bc0a1d04` |
| Weld | `2d9695b0670c3a80784ce97589d5f1cac31dfe85` | `4784d07c4064195c33136b9b91c8231913f08e06` |
| Graft | `95bab7e5127e71d2fd6f90cfe8266e6326d7e760` | `01f3c9f3d68df3247e6b7ec7f65ffbca57e9b2d4` |
| Upstream Servo | selected `1d44e5dd6a8b64c02f9dbf7fcbdf4ebdd0740019`, 0.5 branch | released 0.7 source examined separately at `aac43a3f31a259f04a574f5ec4e959c943ad7cc7` |

The shared working manifest and lock were observed with SHA-256
`38f4195d3b4b443846372f5e77cf39d2039c91b56f3030267532d1f30fa16641` and
`801e44839b5da4046766d0eda70fd3d7e7b98311cc816399c7e2dd35d0a94923`.
These are observation identities, not a new frozen source set. Later edits
and the coming executable need their own receipt. The observed working
`src/shell/mod.rs`, `src/shell/servo.rs` and `src/contributed_a11y.rs` hashes
were respectively `20cc67b8c50fd6b694b09f2008f2a34178186e6ea204d00abd7d97216b2de7ee`,
`3fbcefd1de32bddf31839a4eb0beb9fa5cca9176f32f240b18bb086a00baca37` and
`6e4dcb660590eb17bae88bf6668363134319e73afdbdddc7c6787273fa3fb3ae`.
Selected Mere sources were
also read with `git show`; primary Mere has newer producer-semantics handling,
but both versions have the tree/action gaps below.

## Host identity and action contract

The locked AccessKit 0.24.1 already has the required subtree mechanism.
In its [exact source](https://github.com/AccessKit/accesskit/blob/accesskit-v0.24.1/common/src/lib.rs),
`TreeId` is UUID-backed, the root ID is nil, and each guest tree has a distinct
ID. A host node's `tree_id` establishes a graft. Its parent update must arrive
before the guest's first update, which must include tree metadata. Removing
the graft retires the subtree. Focus chains through the parent graft to the
guest's local focus. Actions identify both tree and node and carry typed data.

The identity for routing is `(TreeId, NodeId)`, plus the host's current
admission/generation check. Keeping only `NodeId` is insufficient when two
trees allocate the same local ID. AccessKit `ActionRequest` retains `action`,
`target_tree`, `target_node` and `data`; the host must preserve all four.
Nested producer trees need the same treatment. These APIs do not require an
AccessKit upgrade.

The locked native adapters are accesskit_winit 0.32.2, Windows 0.32.1, macOS
0.26.3 and Unix 0.21.1. Their corresponding consumer implementations contain
graft-parent traversal. This source support is not a joined-tree screen-reader
receipt. Native adapters do not automatically ingest another process's native
provider or supply an IPC protocol. In particular, Windows adapter 0.32.1's
`GetEmbeddedFragmentRoots` returns null; a WebView2 provider cannot simply be
passed as an AccessKit node.

**Contract implications, not implemented here:** tree deltas need ordered
delivery or an explicit full-state resynchronization after overflow, rather
than a latest-frame mailbox. Activation must restore root and guest state in
parent-first order. Producer-local bounds, transforms, clipping, DPI, scroll
and visibility need a defined mapping to the pane/window. Detach, navigation,
crash and remount must reject late updates and stale actions observably.

## Current host seams

- [Turnstone's bridge](../../../../src/shell/a11y_bridge.rs), lines 44–75,
  stores one latest root update and queues full requests. Its activation and
  deactivation paths do not manage a registry of foreign trees.
- [Host projection and dispatch](../../../../src/shell/mod.rs),
  `projected_a11y_tree`, `push_a11y_tree` and `drain_a11y_actions`, join chrome,
  frozen pages and retained contributed sessions. Dispatch looks up only
  `target_node` and accepts Click/Focus; other typed actions are skipped.
  Foreign-tree routing is missing. Updates use a 30-redraw cadence, so an
  idle semantic change has no independent publication/wake contract.
- [Contributed projection](../../../../src/contributed_a11y.rs), lines
  120–204, namespaces and rehashes local node IDs into the root tree. Existing
  pane/generation/spec checks are useful custody controls. This is stitching,
  not independent TreeId grafting, and does not export captured browser pages.
- Mere [`crates/forme/uxtree/src/lib.rs:76–80`](https://github.com/merely-made/mere/blob/26857eafe293a66e30c785d85fafe44d1113c11e/crates/forme/uxtree/src/lib.rs#L76-L80) always emits a root-tree update.
  [`crates/cambium/cambium-winit-a11y/src/lib.rs:154–200`](https://github.com/merely-made/mere/blob/26857eafe293a66e30c785d85fafe44d1113c11e/crates/cambium/cambium-winit-a11y/src/lib.rs#L154-L200) at primary revision
  maps only Click, Focus and finite numeric SetValue and drops `target_tree`;
  the selected revision has the same identity limitation. ProducerSemantics
  describes drawing slots, not a full browser/editor tree.
- Mere [`crates/inker/inker/src/surface_engine.rs:970–1047`](https://github.com/merely-made/mere/blob/26857eafe293a66e30c785d85fafe44d1113c11e/crates/inker/inker/src/surface_engine.rs#L970-L1047) has pixels, input
  and web control, but no tree-update/action-delivery seam. Its accessibility
  capability field is disclosure, not semantic transport.

These are confirmed missing source seams. This inventory did not reproduce
a new native failure or invalidate previously qualified root/contributed
projection behavior.

## Supplier-specific findings

**Servo:** the selected 0.5 source has
[`WebView::set_accessibility_active` and a tree ID](https://github.com/servo/servo/blob/1d44e5dd6a8b64c02f9dbf7fcbdf4ebdd0740019/components/servo/webview.rs#L901-L999),
plus the [delegate's tree-update callback](https://github.com/servo/servo/blob/1d44e5dd6a8b64c02f9dbf7fcbdf4ebdd0740019/components/servo/webview_delegate.rs#L1085).
Accessibility is disabled by default
in its preferences. The exporter creates a webview tree and a nested document
tree; navigation epochs matter. Turnstone's local `src/shell/servo.rs` candidate
(preserved in the [source archive](../source-inputs-all3-pixel-control.zip))
does not activate or publish these updates. No public typed content-action
entry point was found in that selected Servo API; its shell has a content
action TODO.

**Material release difference:** Servo 0.7 at `aac43a3` publicly exposes
[`Servo::forward_accessibility_action(ActionRequest)`](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/components/servo/servo.rs#L1120-L1131).
The older missing entry point is not a claim about this release. Its presence
makes a coherent Servo/Graft migration a high-priority U2 candidate. The subsequent exact-source trace routes through constellation, script and
layout to [the DOM handler](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/components/script/dom/window/window.rs#L3832-L3843),
which implements only Click using an untrusted synthetic click. Other actions
fall through silently; the public method returns no typed result. The same
AccessKit 0.24 caret resolves to the host's 0.24.1, and graft-first activation
remains required. This is a concrete engine-routed Click candidate, with
Focus, text/value, selection and scroll still missing. No native assistive
behavior is qualified; a remaining shell TODO does not negate the public
method. The [currency snapshot](../dependency-currency/README.md)
records the Surfman/mozangle/ipc-channel migration and independent build gates.

**Weld/CEF:** both selected Welding and the [current primary owner](https://github.com/merely-made/wgpu-weld/blob/2d9695b0670c3a80784ce97589d5f1cac31dfe85/welding/src/windows_cef/cef_backed.rs) lack accessibility
activation/callback export. Mere's adapter explicitly reports Unsupported
([`crates/inker/engines/weld-engine/src/welding_0_15.rs:1022–1023`](https://github.com/merely-made/mere/blob/26857eafe293a66e30c785d85fafe44d1113c11e/crates/inker/engines/weld-engine/src/welding_0_15.rs#L1022-L1023)). The inspected local
CEF `151.8.0+151.3.24` Rust bindings expose an AccessibilityHandler, tree/location callbacks and
accessibility-state control. The [CEF callback contract](https://github.com/chromiumembedded/cef/blob/master/include/cef_accessibility_handler.h)
and [browser-state contract](https://github.com/chromiumembedded/cef/blob/master/include/cef_browser.h)
describe windowless tree export without native platform objects. A Welding
owner must retain callback data on its UI thread, associate browser/frame and
generation, translate updates, and qualify actions separately. No typed
AX-action dispatcher was verified in the inspected public bindings. Callback
availability alone does not prove actionable trees on any of the three OSes.

**Scry:** [Turnstone's production capability](../../../../src/shell/scry.rs)
is Unsupported; a later Supported value in that file is a test fixture.
Scry [`scrying/src/lib.rs`](https://github.com/merely-made/wgpu-scry/blob/2c3ebd24118851cf6125cfb193f55aab20e3ad67/scrying/src/lib.rs) and its GTK3/GTK4/WPE provider capability declarations
do not export a portable accessibility tree. On Windows the private WebView2
composition controller exists, but no automation-provider export is wired.
Microsoft's [CompositionController2 API](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2compositioncontroller2?view=webview2-1.0.3967.48)
can return a UIA provider. That native provider is not an AccessKit tree;
provider composition or semantic translation needs a concrete owner bridge.
Existing CDP input is not semantic/action coverage. WKWebView on macOS and
GTK WebKit/WPE on Linux need their own export and action paths; engine-native
accessibility does not prove integration under Turnstone's host tree. Current
live-browser consumer factories are Windows-only.

## Next bounded slice and acceptance

Mere owns a reusable subtree identity/lifecycle/order/action helper, with
native-host integration in its accessibility host and an additive producer
semantic/control seam reviewed by the Inker owner. Turnstone owns pane policy,
focus handoff and browser admission. Supplier owners export their actual trees
and dispatch their supported actions. A later read of Mere `fde06dc0` finds
the [published composition brief](https://github.com/merely-made/mere/blob/fde06dc0b769a418f280f3766069f871b2479e26/design_docs/cambium_docs/research/2026-10-06_app_composition_brief.md#L694-L718)
recording AC1–AC5 and U15's assignment of the shared helper and E1 to the
Turnstone unusual-protocols lane. Its
[smolweb plan](https://github.com/merely-made/mere/blob/fde06dc0b769a418f280f3766069f871b2479e26/design_docs/nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md#L36-L53)
records that lane's U16 ownership of Smolweb/Micron accessibility projections.
Published Turnstone `032463a` has neither receiving record yet. This browser
lane should consume the helper and supply browser-specific exports/actions;
it should not start a duplicate helper or E1. AC6's future cross-process channel
remains research and
must carry ordered semantic updates and typed actions regardless of whether
paint lists or shared textures carry pixels.

Before a native browser join, a portable consumer check should mount two
independent trees with identical local node IDs, exercise nested focus and
actions, retire/remount one, and deliberately reject missing-parent,
wrong-tree and stale-generation controls. Activation/resynchronization and
idle publication need checks. Coherent Servo 0.7 migration can then exercise
its public bridge; CEF and Scry still require their owner-specific seams.

Final acceptance remains open on all three platforms:

| Platform | Required native observation |
|---|---|
| Windows | Narrator or NVDA traverses chrome and foreign content as one host tree |
| macOS | VoiceOver performs the same joined-tree walk on the iMac |
| Linux | Orca performs the walk on Fedora/Wayland and Mint |

Each walk needs current names, roles, states, text, values and bounds; focus
into and out of the page without a Tab trap; each advertised typed action;
idle updates; resize/DPI/scroll clipping; navigation and close/reopen with
stale actions rejected. Text editing, selection, scrolling and SetValue are
qualified only where actually implemented. Keyboard/IME/caret gates remain
explicit. Automated tree checks, source inspection and successful pixels do
not substitute for these human AT observations. Capability disclosure must
remain Unsupported or Partial until the relevant owner/platform qualifies.

U9 holds final landing for the coordinated current tested set after WS4 is
pushed: Knot, then Redshank, then Turnstone, with the browser source owner.
The existing fontsan failed control, qualified supplier repair and pending
consumer/native/release gates remain intact.

Later independent owner repair: Genet `965b64e206a47d1c8808472de9aa461233638768`
is published directly atop the observed `90c5ef50`, with three focused codec
tests and exact six-file preservation guards. This does not change the source
observation table or automatically update Mere's immutable Genet pin.

The supplier's later complete Servo 0.7 producer-chain trace also finds no
action advertisement: private layout nodes start with empty AccessKit action
masks and reach the delegate unchanged. AccessKit's consumer requires Click
support on the node or the direct parent's child actions for invocability.
The public Click forwarding path therefore leaves native action exposure as
a separate source-qualified gap. The immutable source hashes and exact line
chain are recorded in Graft's `servo_present_hook_20261006` receipt; native
assistive behavior remains untested.
