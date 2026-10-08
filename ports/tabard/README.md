# Tabard appearance workshop

`tabard-workshop` is the reusable Cambium surface and retained authoring state
for Tabard. The shared theme, draft and file-library APIs live in
[`crates/system/tabard`](../../crates/system/tabard); this port composes them
into a product surface. [`desktop`](desktop) mounts that surface in a native
window without a second editor model.

The workshop edits an isolated user draft. Pick a primary, secondary, tertiary
or neutral seed, adjust its hue/saturation/lightness, choose an accent harmony,
and inspect chrome, reader, syntax and graph specimens in light, dark or either
high contrast mode. Locked harmony explains dependent hues and keeps their
saturation/lightness editable. Preview mode is local to the workshop and does
not activate the theme in another host.

Undo, Redo and Discard operate on the draft. Choosing a built-in creates a user
copy. New copy creates another identity; switching themes or reopening the
library requires saving or discarding edited work first. Save validates a
candidate registry and persists the authored definitions before accepting the
new save point. A failed write leaves the draft, history and registry intact.
In-memory hosts use `WorkshopState::in_memory()` and save for the session.

An embedding host mounts `workshop_view`, `workshop_stylesheet()` and
`WorkshopState`. The stylesheet composes Cambium's shared syntax and graph
rules with the workshop frame. Register `graph_leaf()` under `GRAPH_LEAF_KEY`
and render `reader_preview()` under `READER_LEAF_KEY`; the desktop host shows
both integrations on its existing render core and device.
Ordinary control messages synchronize the authoring model. A host that edits
`text_field_mut("name")` through a native IME seam calls `sync_controls()` after
dispatch; the desktop host demonstrates that integration. Saved user themes
retain their existing `Theme` schema, including mode sheets and harmony. The
versioned library stores definitions separately from host appearance preferences.

The syntax specimen uses Cambium's read-only `highlighted_code` view: Illume
lexes the Rust source, Cambium maps its token kinds to Tinct roles, and the
selected mode supplies the local palette. These APIs also serve other Cambium
consumers; unknown languages retain plain source text.

The reader specimen extracts the checked-in HTML fixture through Fleece,
lowers it into Inker's shared document, and uses document-lanes and
document-canvas for shaping, reflow and paint. Palette changes preserve its
source document. The bounded preview exposes its extracted prose as a
read-only image; link activation and rich reader-session accessibility are
later capabilities. The graph specimen uses Cambium's `GraphCanvasSwatch`
and Sprigging paint, with real pointer, hover, focus and keyboard selection.

The current specimens show derived seed palettes. An authored mode stylesheet
is retained and explicitly identified by the surface, but is not rendered by
these specimens. Custom calculator previews, stylesheet editing, portable
import/export controls and Turnstone integration remain later work. Existing
CSS/DTCG exports retain their documented normal-contrast behavior; the syntax
specimen uses Tinct's explicit mode-profile API.

Run the desktop workshop from the repository root:

```sh
cargo run -p tabard-desktop
```

Use `-- --library /path/to/themes.json` for a separate authored library. See
the [desktop README](desktop/README.md) for native capture scenarios.

The retained acceptance tests exercise actual controls, keyboard routing,
AccessKit names/roles, laid-out specimen styles, save/reload and failed writes:

```sh
cargo test -p cambium --features highlight -p tabard-workshop -p tabard-desktop -p tabard -p tinct
```
