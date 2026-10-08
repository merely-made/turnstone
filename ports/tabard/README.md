# Tabard appearance workshop

`tabard-workshop` is the reusable Cambium surface and retained authoring state
for Tabard. The shared theme, draft and file-library APIs live in
[`crates/system/tabard`](../../crates/system/tabard); this port composes them
into a product surface. [`desktop`](desktop) mounts that surface in a native
window without a second editor model.

The workshop edits an isolated user draft. Pick a primary, secondary, tertiary
or neutral seed, adjust its hue/saturation/lightness, or enter a six-digit RGB
hex color and press **Apply color**. RGB edits preserve the seed's existing
alpha. Incomplete hex input stays visible and blocks Save, export and navigation
until corrected or discarded. Locked accent harmony explains dependent hues
and keeps their saturation/lightness editable.

Inspect the specimens in light, dark or either high contrast mode. Preview
mode is local to the workshop and does not activate the theme in another host.
**Use preview as default** authors the theme's standard light/dark and contrast
defaults; changing the preview mode alone does not change those defaults.

The **Application stylesheet** preview is a separate real HTML document with
its own Livery cascade and media device. **Edit this mode's stylesheet** opens
a CSS field; **Apply to preview** applies its exact text to the selected mode,
and **Clear to derived** removes the override. Save also applies staged CSS.
Ordinary selectors such as `body`, `.toolbar`, `.address-field`, `button`,
`h1`, `p`, `a` and `.token-keyword` target the checked-in
[application fixture](fixtures/application.html). Its
[fixture rules](fixtures/application.css) consume `--tabard-color-*` and
`--tabard-syntax-*` properties with neutral fallbacks. Without an override,
Tabard supplies the selected mode's derived properties. With an override,
the exact authored sheet replaces that derived sheet and follows the fixture
rules in the cascade. The editor frame keeps its own appearance. CSS recovery
diagnostics come from the shared Livery parser; valid rules can still render
when another rule is rejected.

Undo, Redo and Discard operate on the draft. Undo first discards unapplied hex
or CSS input when present, then earlier applied edits can be undone. Choosing a
built-in creates a user copy. **New copy** creates another identity. Switching
themes, importing, deleting or reopening the library requires saving or
discarding edited work, including a newly imported theme or explicit new copy.
Save validates a candidate registry and persists authored definitions before
accepting the new save point. A failed write leaves the draft, history and
registry intact. **Delete theme…** asks for confirmation and removes only a
saved user definition; built-ins remain protected.

**Import theme…** accepts one complete theme JSON file into an unpublished
draft. A built-in marker or occupied identity produces a fresh user copy,
preserving existing definitions. **Export…** captures the current draft,
including unsaved edits, for the host to write to a selected destination.
Theme JSON preserves seeds, alpha, harmony, default mode flags and exact mode
stylesheet text using the existing `Theme` schema; JSON formatting is generated
by the shared serializer. CSS colors and DTCG tokens export the selected
canonical mode through Tabard's shared mode resolver, including high contrast.
Those derived formats refuse a selected custom mode or a nonempty stylesheet
attached to the selected mode; export Theme JSON to preserve such appearances.
Export does not save the library or clear Undo history. Existing destinations
require **Replace file** confirmation. The authored library, workshop settings
and their lock files cannot be used as export destinations.

The versioned library stores definitions. A separate `themes.json.workshop.json`
file, when the library is named `themes.json`, remembers the editor's last saved
user-theme selection and preview mode; it is not exported as theme data and
does not activate a theme elsewhere. In-memory hosts use
`WorkshopState::in_memory()` and save for the session. Native close requests
offer Save and close, Close without saving, or Keep editing when user work is
pending. Invalid input or a failed save keeps the window and draft available.

An embedding host mounts `workshop_view`, `workshop_stylesheet()` and
`WorkshopState`. The stylesheet composes Cambium's shared syntax and graph
rules with the workshop frame. Register `graph_leaf()` under `GRAPH_LEAF_KEY`.
Both `reader_preview()` and `stylesheet_preview()` return retained preview
handles implementing `PreviewScene`: `frame`, `revision` and `accessible_name`.
Register their texture producers under `READER_LEAF_KEY` and
`STYLESHEET_LEAF_KEY`. The desktop's generic
[`ScenePreviewProducer<T>`](desktop/src/reader.rs) demonstrates this adapter on
the existing host render core/device, with distinct raster keys and rebinding
when reopening replaces the state. The adapter is desktop-local; the portable
workshop exposes scenes and does not depend on a desktop port or create a
renderer.

Ordinary control messages synchronize the authoring model. Hosts using native
text/IME provide focused slots through `text_field` and `text_field_mut` for
`name`, `seed-hex` and `mode-sheet`, then call `sync_controls()` after dispatch.
They supply the existing file-chooser seam for import, consume `take_export()`
and call `complete_export()` after choosing a destination. Embedding close
policy calls `request_close()` and honors the explicit `exit_requested()`
decision. The desktop host demonstrates these seams without a second editor
model.

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

The chrome, reader, syntax and graph specimens remain typed seed-derived
appearances. Arbitrary authored CSS applies to the isolated application
document; it is not projected into reader or graph paint roles. Imported custom
modes with attached stylesheets can be previewed and edited there. Clearing a
custom sheet returns the preview to the theme's standard default mode.
Custom-mode calculator authoring and evaluation, Turnstone integration,
interactive reader navigation,
structured accessibility for the two document images and a full graph
workspace remain open. The bounded document previews expose names derived
from their actual source content, rather than presenting a rich reader or
interactive application session.

Run the desktop workshop from the repository root:

```sh
cargo run -p tabard-desktop
```

Use `-- --library /path/to/themes.json` for a separate authored library. See
the [desktop README](desktop/README.md) for native capture scenarios.

The retained acceptance tests cover actual controls and keyboard routing,
AccessKit names/roles, isolated computed CSS, preserved alpha, save/reload,
native chooser/export transactions, history, close guards and failed writes:

```sh
cargo test -p cambium --features highlight -p tabard-workshop -p tabard-desktop -p tabard -p tinct
```
