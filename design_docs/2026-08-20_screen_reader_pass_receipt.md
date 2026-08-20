# Screen-reader pass receipt: the frozen projection, heard

**Date:** 2026-08-20
**Operator:** Mark, at the machine, with Windows Narrator. Claude drove the
builds, the UIA dumps, and the fixes between attempts.
**Scope:** the manual leg of the projection grammar plan's B1 target (mere,
`design_docs/mere_docs/implementation_strategy/2026-08-15_projection_grammar_adoption_plan.md`),
run against turnstone's Frozen Projection pane over the sample graph.
**Precedent:** the 2026-06-09 AccessKit verification checklist, written for
meerkat and unrunnable since its retirement; this receipt replaces it as the
worked example.

## Result

**Pass.** Narrator walked the full stitched tree: window, chrome, canvas
group ("graph canvas, 12 nodes"), the frozen projection pane, the
"Disclosed projection" document **with its summary spoken**, the "12 items"
list with entries by name, and the "27 relationships" list to the last
entry — operator's words: "made it to 27 of 27."

## What the pass found that no automated receipt had

The finding is the reason B1 kept a manual leg.

1. **The OS bridge did not exist.** `accesskit` was in-process only: the
   scenario lane's `assert a11y` read a complete tree while a screen reader
   saw a bare window. The precedent checklist's own preflight
   ("a11y_bridge: installed") had never been satisfiable. Fixed:
   `shell/a11y_bridge.rs` installs `accesskit_winit` before the window first
   shows (the adapter panics on a visible window, so the window is now
   created hidden and shown after), serves the latest tree from a shared
   slot on whatever thread the platform activates from, and refreshes on a
   thirty-frame cadence.
2. **Attempt one dead-ended at the omnibar.** Operator report: "three
   buttons, got past close, got to the chrome, then nothing on down arrow."
   A live UIA dump showed every projected node at
   `rect = -Infinity × -Infinity`: the tree was served but boundless, and
   Narrator treats boundless elements as off-screen and stops. The
   in-process integrity test written that minute (no dangling children, no
   duplicate ids) passed — the tree was structurally sound and still
   unwalkable, which is exactly the gap between iterating a node list and
   being a UIA client with geometry.
3. **Fixed with window-extent bounds.** Every node without bounds claims the
   window rect: coarse geometry, correct names and structure, which is what
   a walk verifies. The second attempt walked everything.
4. **The summary is spoken.** The document's description (the WAI long-form
   count line) reached Narrator. A PowerShell probe had read it as empty;
   the probe was reading `HelpText` while the description crosses on a
   different UIA property. The operator's ears outrank the probe.

## Recorded follow-ups, none speculative

- **Route `ActionRequest`s.** Screen-reader activations are deliberately
  dropped (`DropActions`); a Narrator click does nothing. Routing into the
  action spine is the next leg when a consumer asks for operable, not just
  readable.
- **Per-node rects.** The surface plan knows real pane rects; the projection
  should carry them so scan order and touch exploration match the screen.
- **Label the frisket root.** One `Group ''` sits between content and the
  panes — announced as blank.
- **`app/tests.rs:2317` carries a doubled `#[test]`** — a sibling's
  in-flight edit, noted here rather than fixed under them.

## How to re-run

Build turnstone, launch with a fresh `TURNSTONE_ROOT`, right-click the
canvas → "Open Frozen Projection pane". `Ctrl+Win+Enter` for Narrator,
`CapsLock+Space` for scan mode, walk with the arrows. The tree served to the
OS is the same one `assert a11y` reads, so the driven scenario
(`scenarios/frozen_projection.scn`) remains the automated proxy between
manual passes.
