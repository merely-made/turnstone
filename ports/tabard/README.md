# tabard

Tabard is the theme-authoring port of the Genet engine.

A tabard is the garment that displays a household's livery. This port is where
a household's livery gets authored: seeds in, liveries out. It composes what
already interlocks: [tinct](https://crates.io/crates/tinct) derives the full
palette from a few seed colours, [illume](https://crates.io/crates/illume)
emits syntax spans that the derived syntax palette colours, and palette-aware
icons can recolour at render time.

Tabard authors portable theme artifacts. Livery consumes stylesheets Tabard
emits. Pelt has an optional named preview receipt for those artifacts, but does
not own their model.
Tinct remains the small, serde-only derivation crate. Illume continues to name
syntax spans rather than decide their appearance.

## Current artifact

The first implementation is deliberately portable and library-only:

- Theme owns a name and tinct::Seeds, then derives Tinct's normal-contrast
  base palette.
- The theme module holds the host theme model, moved from Mere's registry on
  2026-09-24: ThemeTokenSet and its ThemeRegistry, seed derivation, custom
  modes, chrome colours, edge-style tokens, and the theme a lens carries.
- Theme::design_tokens emits a typed DTCG 2025.10 color document. Every token
  has an explicit color type and a structured sRGB value, while the name,
  seeds, and derivation choice live under org.merely.tabard in $extensions.
- Theme::css_custom_properties emits a deterministic :root stylesheet with the
  same palette as --tabard-color-* custom properties. Livery can consume it
  as an ordinary author sheet.
- Theme::lagrange_palette_txt emits a deterministic `palette.txt` for
  Lagrange's application UI. It writes the documented `# Dark` and `# Light`
  sections in Lagrange's neutral, accent, and status-label order using
  `#RRGGBB` values. The return value carries diagnostics for Tabard roles
  which collapse into one Lagrange label or have no representation. This
  exporter does not change Lagrange page themes, which are selected separately
  from a site palette seed.

  The emitted label list follows the [v1.21.1 help vocabulary](https://raw.githubusercontent.com/skyjake/lagrange/v1.21.1/res/about/help.gmi).
  That release's stock [`loadPalette_Color` table](https://raw.githubusercontent.com/skyjake/lagrange/v1.21.1/src/color.c)
  omits the documented `yellow` and `magenta` labels, so the artifact retains
  those lines for documented-shape compatibility and reports a
  `StockVersionIgnored` diagnostic for each mode. A headed load receipt for a
  specific Lagrange build is required before claiming that all emitted labels
  are applied.

Its documented projection is explicit: dark neutrals are `bg`, `surface`,
`surface-2`, `surface-hover`, `text`; light neutrals are `text`, `text-dim`,
`surface-2`, `surface-hover`, `surface`. The remaining labels map as
`brown`/`orange` dim/bright variants of `primary`, and `teal`/`cyan` as
dim/bright variants of `secondary`. `red=danger` and `green=success` are used
only when their authored hues pass the exporter’s red/green semantic checks;
otherwise Lagrange’s documented defaults are emitted. Lagrange’s `yellow`,
`magenta`, and `blue` reserved colors likewise use its documented defaults.
The repeated accent mappings, defaults, omitted Tabard roles, and discarded
alpha are returned as diagnostics rather than hidden in the artifact.
- Pelt's optional `tabard-preview` receipt maps those generic properties onto
  Pelt-owned Chrome roles. It proves the shell recolors while the focused
  document, session history, tabs, and content aperture remain held. It is not
  a persistent appearance setting and does not recolor document content.
- Pelt's optional `tabard-reader-preview` receipt maps the same portable
  palette onto Reader's existing host palette. It proves a Fleece article
  keeps its held source and route-restoration behavior while Pelt supplies the
  Reader colors. Fleece and `genet-documents` do not depend on Tabard.

Recorded 2026-08-28: both named headed Windows consumer receipts passed at
960x640. `tabard-preview` completed after three redraws with compositor digest
`d0affd3746b03554`; `tabard-reader-preview` completed after nine redraws with
digest `ea505825544747b9`. The latter held a `genet.reader` Fleece article
beside a `genet.livery` neighbor and retained the Reader inspector's lineage.
These receipts validate consumer seams, not persistence or a platform theme
policy.

Syntax-color policy, icon policy, persistence, imports, a DTCG resolver and a
Geopard exporter are not here yet; the host theme model arrived with the theme
module.

## License

MPL-2.0 (see the repository `LICENSE`)
