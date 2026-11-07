# Viewer build audit: what the Graphshell web viewer is for, what it pulls, and how to get far under the cap

**Status:** 2026-11-07: brief. Research and measurement only; no product code changed by the audit itself.
Mark's rulings on its five decisions are recorded in "Rulings" below and in the Graphshell host plan, T7. Written for
Mark's question of the same day.

**Question (Mark, 2026-11-07).** "Ideally, we'd be able to get far under that [the 5 MiB cap]. Or possibly
reorganize what's going on here ... Probably most of the build's framing is stale, I'd wager. Might be worth a
pass against the current stack ... Pulling, updating, etc."

**Method and limits.** Mere `b12fdcfa7` (main), fresh HTTPS clones at several paths, `cargo +1.99.1 build
--locked --profile viewer --target wasm32-unknown-unknown --no-default-features --features presentation-webgpu`
(the route `ports/graphshell/web/scripts/build-viewer.py` builds), `wasm-bindgen` 0.2.129 with
`--remove-name-section`, sizes are the Wasm file alone at gzip level 6 (`mtime=0`), the number the cap gates.
`twiggy` and `wasm-opt` are not installed and nothing was installed; attribution is a small script that reads the
Wasm name section of a `strip=none` build and sums function-body bytes by defining crate, and marginal gzip is
"gzip of all function bodies minus the group's bodies". Two caveats on that: generic instantiations of
`core`, `alloc` and `hashbrown` are charged to those crates, not to the crate whose types caused them (together
about 510 KB gzip, not attributable by name), and the data section cannot be attributed by crate at all, only by
content. The glue, mount script and history fixture caps were not re-measured. Brotli is not measurable here
(no tool, no module): every figure is gzip. The relock was built with 1.99.1, not 1.97.1 (the manifest's
`rust-version`); `cargo +1.97.1 update` was the resolver. The site's own repository is not under `Code/repos`, so
nothing below says what the live site loads today.

## Answer in brief

1. The viewer is a **site-only** artifact (loaded on interaction by a site page the repo does not hold); every
   other Graphshell page and the browser extension load the **full** default bundle, not this build. R157's
   routes are live in code; its "reproducible profile" and "automated gate" claims are false today.
2. The webgpu artifact is **3,708,455 B gzip6 (3.54 MiB)** in the large build mode and **3,625,018 B (3.46 MiB)**
   in the small one, 71% and 69% of the cap. The webgl2 artifact is **3,494,724 B** in both modes.
3. The bimodality is **a duplicated monomorphization of the `genet-livery` layout and paint pipeline over
   `ScriptedDom`** (about 419 KB raw, 83 KB gzip), re-rolled by any change to a crate hash. `codegen-units = 1` is
   already in force (the profile inherits release) and `--remap-path-prefix` does not remove the modes.
4. A relock to current mains and registry builds clean and costs about +4 KB. No Genet, Netrender, Armillary,
   Graphshell-protocol or Vello pin was behind main; the eidetic, gemot, dramatis, murm and esp git pins were.
5. The honest floor of the current architecture is about 3.0 MB. Under about 1 MB needs a different viewer.

## 1. What the viewer is for today

**Who loads it.** Only `ports/graphshell/web/viewer.html` (and the qualification fixture
`tests/presentation-webgl-fixture.html`, which the runner serves with `pkg/{webgpu,webgl2}/`) import `viewer.js`.
The README says a site host loads it on first interaction and keeps its own frozen first reading
(`ports/graphshell/README.md`, "Read-only viewer delivery"). Eleven other pages (`index.html`, `tree.html`,
`workspace.html`, `practice*.html`, `co_op.html`, `dataset.html`, `grouped-dataset.html`, `responsive-links.html`,
`embed.html`) and the extension's `graph.html` load `loader.js`, which imports the full `./pkg/graphshell_web.js`
(`extension/prepare-extension.sh` copies that `pkg/`). So the cap binds exactly one consumer: the public site,
which is outside the repository.

**What it renders and how it interacts.** One `<graphshell-tree>` element over a host dataset
(`scenomise.host-dataset/v2` history or v1 dataset): the pictograph canvas (node faces, links, territory),
the Inspector and ordinary controls as Cambium elements, a "Graph tools" panel with arrangement recipes and
physics, camera pan and zoom, selection, drag, and the Fold/history view. The qualification runs counted 24 command
cases, 11 presenter cases, 17 physics receipts and 4 pointer/keyboard assertions
(`archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:1255-1258,1284`). It is read-only in the sense of no
saved state, product panes or remote board, not in the sense of static: it runs live physics. The viewer carries
Latin and Greek text only (Arabic, CJK and Thai glyphs are missing; T7 says so).

**Framing claims, marked.**

| Claim | Where | Verdict |
|---|---|---|
| The viewer replaces the site's sandbox, loaded on interaction, the frozen first reading kept, with a 64 KiB first-load cap | one-tree plan `:1203-1221,1300-1302`; T7; presentation plan | **Current as intent; unverified as state.** Site-side deployment is T7's open gate; the repo cannot show it. |
| Two routes, `webgpu/` and `webgl2/`, "Classic Vello dropped" | one-tree plan; README; `build-viewer.py` `ARTIFACTS` | **Current, half-true.** Classic Vello is dropped only from the webgl2 artifact. The webgpu artifact still links `vello`, `vello_encoding`, `vello_shaders` and 23 compute shaders (section 2). |
| Non-WebGPU fallback: Auto loads webgpu if a device exists, else remounts on webgl2 | `mount.js` `chooseArtifact`, `WEBGPU_ONLY_REFUSAL`; tests README | **Current in code and in the six-route runner.** Whether it is still a live need depends on WebGPU availability in the browsers the site must serve, which this audit did not verify. |
| Wasm cap 3.25, then 3.30, 3.50 MiB | one-tree plan `:1213-1219`; presentation plan `:25` | **Superseded** by the 5.00 MiB ruling (T7, 2026-11-07). The presentation plan's Constraints still say 3.30 MiB; it needs the same amendment. |
| "Select a reproducible shipping profile"; "every `cargo build --release --locked` of a commit makes the same bundle" | one-tree plan `:1229-1230`; `web/Cargo.toml` profile comment | **Stale.** Same path gives identical bytes; different paths give 3.62 or 3.71 MB (section 3). |
| "An automated regression gate"; "the portable workflow's `viewer-delivery` job" | one-tree plan `:1298-1306`; README | **Stale.** The tree holds no such workflow (the only workflow file is `crates/cambium/.github/workflows/ci.yml`), Actions are disabled, so the gate is `build-viewer.py` plus `test_viewer_budget.py` run by hand. |
| "Remove capabilities outside the read-only cone" | one-tree plan `:1229` | **Partly stale.** The cone still holds the graph kernel, physics, a PNG encoder/decoder (`mandrake/src/pixels.rs`, unconditional `png`), the full table/CSS layout engine and `serde_json` (section 2). |
| T7: "`cambium` default scripting, about 45 KB" | host plan T7 | **Stale or mistaken.** `cambium` has one feature, `highlight`; there is no scripting default. `genet-scripted-dom` is the mutable DOM arena (`LayoutDom` over `NodeId`), a hard dependency with `regress` and `html5ever` inside it. Its code is about 55 KB gzip but it is the tree, not scripting. |
| T7: "physics through `seiche`, about 200 KB gzip" | host plan T7 | **Understated and not a flag.** Marginal gzip of the physics crates is 290 to 364 KB; `pictograph`'s `canvas` feature hard-wires `dep:seiche` (278 `seiche` references in 30 files), so there is nothing to switch off. |
| T7: "real growth since 2026-10-20 about 130 KB, none accidental" | host plan T7 | **Not re-derived.** The 130 KB is smaller than the build-path swing (83 KB) plus ordinary drift; the series in the earlier bisect is mode-mixed. |
| Host/guest and cycle_breaks D7 to D17 | cycle_breaks plan; host/guest brief | **Applies lightly.** `chirograph`, `sceno`, `sceno-backdrops` and `scenotime` are already git dependencies from `merely-made/graphshell` in the viewer lock. `vector-sheet`, the WIT and the guest-block worlds are not in the viewer cone ("the viewer cone had none of them already", guest_blocks `:444`). The viewer stays a Mere-side host above the protocol layer. |
| Engine split plan | cambium_publish_plan, key_lane_plan cite "engine-split P3" | **Not on main.** `2026-11-07_engine_split_plan.md` is absent at `b12fdcfa7`. The cited intent (the Cambium family, `mandrake` included, moving to the Graphshell repository) matters here: see section 3. |

## 2. What it pulls, measured

**Section totals, webgpu artifact, large mode, delivery build (names removed):** raw 10,405,627 B = code
8,363,908 + data 1,971,352 + about 70 KB of type, import and table sections; gzip6 3,708,455; gzip9 3,692,158.
Code compresses to about 3,051 KB, data to about 630 KB. Small mode: raw 9,985,975, gzip6 3,625,018, gzip9
3,608,492 (code 7,945,774, data 1,970,672).

**Crate table** (large mode, `strip=none`; raw is function-body bytes by defining crate, "marginal gz" is the
gzip saving if those bodies were absent, so rows overlap through shared generics and do not sum):

| Crate | Raw B | Marginal gz | Path from `graphshell-web` | Viewer job needs it? |
|---|---:|---:|---|---|
| `genet_livery` | 892,787 | 332,100 | `cambium-rootstock` → `genet-livery` (also `genet-render`, `taproot`, `pictograph`) | Yes today: the HTML/CSS layout and paint engine under every Cambium element. Not for a scene player. Carries table layout (`collapsed_table_borders`, `table_sizing`, `buckram::table`) the viewer's panels likely never use. |
| `livery`, `cadency`, `cssparser` | 345,868 + 44,547 + 21,548 | 125,783 + 18,797 + n/a | `genet-livery` → `livery` → `cadency` | Same engine: cascade and selector matching. |
| `buckram`, `taffy` | 193,062 + 159,748 | 78,880 + 56,052 | `genet-livery` → `buckram`; `mere-forme` → `taffy` | Block/flex/inline layout, yes; table algorithms, unlikely. |
| `rapier2d`, `parry2d`, `seiche`, `nalgebra`, `numen` | 441,443 + 179,505 + 119,297 + 14,945 + 29,785 | 210,124 + 79,585 + 55,042 (rapier+parry 290,326; whole physics group 364,255) | `pictograph` (`canvas` → `dep:seiche`) ← `graphshell-host` (`board`) and `mere` (`canvas`) | For live settle and drag, yes. Not for a viewer showing precomputed positions. |
| `pictograph` | 420,084 | 199,049 | `graphshell-host`, `mere` | Yes: node faces, canvas, territory, picking. |
| `mere-kernel`, `mere-cartography` | 323,885 + 9,445 | 130,694 | `mere` → `mere-kernel`; `pictograph` `signals` | The graph model and its JSON deserializers; a read-only dataset needs a small slice. |
| text: `harfrust`, `read_fonts`, `skrifa`, `parley`, `fontique`, `icu_*` | 212,759 + 272,045 + 264,759 + 64,068 + 33,096 + about 20,000 | group 307,310 | `genet-livery` → `genet-parley` → `harfrust`; `netrender` → `skrifa` → `read-fonts` | Shaping and glyph outlines for Latin and Greek. Any text through the Rust stack needs this; canvas or DOM text would not. |
| `graphshell_web`, `graphshell`, `cambium*`, `mandrake`, `scenomise`, `tabard`, `scenotime` | 1,156,603 (group) | 488,265 | direct and via `cambium-rootstock` | The host and its tree: yes. `scenomise` (110 KB raw) composes arrangements; a player of arranged scenes would skip it. |
| `serde_json` | 203,517 | 77,832 | 27 parents, direct from the viewer | The dataset is JSON text parsed in Rust. |
| `regress`, `genet_scripted_dom` | 79,468 + 49,478 | 54,692 (both) | `genet-scripted-dom` ← `cambium`, `cambium-rootstock`, `pictograph`, viewer | The DOM arena. Not removable without replacing the DOM. |
| `netrender`, `vello`, `vello_encoding`, `vello_shaders`, `wgpu`, `paint_list_render` | 82,344 + 57,052 + 37,062 + 35,155 + 30,310 + 62,185 | group 154,883 | `netrender`, `genet-presenter` | The renderer. Classic Vello (the three `vello*` crates) is webgpu-route only. |
| `png`, `fdeflate` (+ `miniz_oxide`) | 41,223 + 10,011 | 22,239 | `mandrake` (unconditional `png = "0.17"`, used by `pixels.rs` capture IO); `netrender-vello` has 0.18 | No: the viewer census is zero images (presentation plan V1). A capture helper, not a viewer feature. |
| `core`, `alloc`, `hashbrown`, `std` | 1,147,871 + 471,162 + 302,303 + 59,576 | 324,789 + 140,920 + 43,564 + 18,051 | everything | Monomorphized generics; shrinks only with the crates above. |

**Data section** (1.97 MB raw, about 630 KB gzip, content-identified): IBM Plex Sans (the only font the viewer
embeds, `src/web_tree.rs:1526`; 200,500 B, 88 KB gzip, `glyf` 113 KB and `GPOS` 61 KB); WGSL shader text (about
410 KB printable, about 83 KB gzip); a CSS property and value name table (about 45 KB); the remaining about 1.2 MB
is binary static tables I could not attribute (Unicode and segmentation data, CSS and property tables). The
webgl2 build has 1,553,936 B of data against webgpu's 1,971,352, so Classic Vello's shaders and tables are
417,416 B raw of the data section.

**Routes.** webgl2 (`presentation-webgl2,without-classic-vello`) is 3,494,724 B at one checkout path and
3,495,669 B at another. webgpu is 129 KB (small mode) to 214 KB (large mode) bigger. In small mode webgpu's code
is within 20 KB of webgl2's, so Classic Vello's cost there is almost all shader data.

**Cone versus binary.** The metadata cone is 373 normal-edge crates; LTO removes much of it, which is why several
duplicates cost only compile time: the second Vello pin enters through `sprigging` (`cambium` →
`sprigging` → `netrender-vello` at the old `dfb9b53` revision) and leaves a single 708-byte `sprigging` function
in the binary; `wgpu-core`, `naga`, `winit` (via `cambium-winit`, 317 B) and the Dramatis identity crates are in
the cone and effectively absent from the output.

## 3. Reproducibility

Eight checkouts of the same commit, same flags, same toolchain (gzip6 of the webgpu artifact):

| Path | Baseline | With `--remap-path-prefix` |
|---|---:|---:|
| `.../viewer-audit-20261107/mere` (A) | 3,708,455 (large) | 3,708,330 |
| `.../pathB-longer-dir/m` (B) | 3,711,582 (large) | 3,710,950 |
| `.../zz/m` (Z) | 3,709,802 (large) | 3,709,538 |
| `.../c4/m` | 3,714,731 (large) | not run |
| `.../c1/m` | 3,625,018 (small) | 3,624,764 (small) |
| `.../c2/m` | 3,626,536 (small) | 3,626,182 (small) |
| `.../c3/m` | 3,627,688 (small) | not run |
| `.../upd/m`, relocked | 3,631,331 (small) | not run |

- **Same path, new `--target-dir`: byte-identical** (SHA-1 equal). The build is deterministic; it is not
  deterministic across paths.
- **`codegen-units = 1`: already in effect.** `[profile.viewer]` inherits `release`, which sets it; the modes occur
  with it. Not a lever.
- **`--remap-path-prefix` (both source root and `~/.cargo` mapped to fixed names, via `--config
  target.wasm32-unknown-unknown.rustflags`, keeping the committed getrandom and stack flags): no effect on the
  mode.** Sizes move by a few hundred bytes (the flag changes the hash) and large stays large, small stays small.
  The two bands do not go away: large 3.708 to 3.715 MB, small 3.625 to 3.631 MB.
- **What differs between checkouts.** The path-dependency rlibs (`cambium`, `pictograph`) get different filename
  hashes per checkout; git and registry crates (`genet_livery`, `serde_json`, `rapier2d`) get identical ones. The
  viewer is its own workspace and its path dependencies live outside its workspace root (`../../../crates/...`),
  which fits cargo hashing their absolute path; I observed the effect, not the cargo source. The name `strip=none`
  and the profile change re-roll the same way (`c1` baseline small, `c1` with `strip=none` large; `c3` small both
  ways; `A` large both).
- **What the modes are.** Diffing a named large build (A) against a named small build (`c3`): code 8,362,048 vs
  7,963,198 B, 21,588 vs 21,069 functions, and 1,063 function names present only in the large build totalling
  508 KB. Net by crate: `genet_livery` +272 KB, `buckram` +40 KB, `core` +35 KB, `hashbrown` +20 KB, `taffy`
  +8 KB, `livery` +7 KB, `cadency` +6 KB. The large build holds **two copies of the same instantiation**
  (`<LiveryDocument<ScriptedDom>>::frame` twice for 52.6 KB, `collapsed_table_borders::<NodeId>` twice,
  `TextSystem::place`, `build_box`, `apply_buckram_table_layout`, `resolve_subtree_with_containers`, ...): 245
  duplicated names and 643 KB in the livery family in the large build against 45 names and 57 KB in the small one.
  Raw difference 419 KB, gzip difference 83 KB. Why a crate-hash change decides whether those instances
  deduplicate (several crates instantiate the same generic over `ScriptedDom`: `pictograph`, `cambium-rootstock`,
  the viewer itself) was not isolated.
- **So:** with the tools on the table the bimodality does not go away; every honest fix is to remove the
  duplication, and the expected result is the small band (about 3.62 MB, a prediction from the diff, not a build).
  Options, cheapest first: (a) one crate owns a concrete (non-generic) `genet-livery` document over
  `ScriptedDom` and the others call it, so no second crate can instantiate the pipeline (a Genet-side change);
  (b) `-Zshare-generics` experiments on nightly (not tried; share-generics is already the default at `s` and `z`);
  (c) pin a canonical build path in `build-viewer.py` (gives identical bytes, not the small band, and any
  dependency, feature or lock change re-rolls it); (d) engine-split P3, if the Cambium family becomes git
  dependencies, removes most path-dependent hashes but not the viewer's own, `pictograph`'s or `mere`'s.

## 4. Pull and update

Scratch clone, branch `scratch/relock-never-push`, `cargo +1.97.1 update` in `ports/graphshell/web`, not pushed.

- 1,140 to 1,151 lock entries. **134 registry bumps** (for instance `serde_json` 1.0.151 to 1.0.154, `rapier2d`
  0.36.0 to 0.36.1, `uuid` 1.27 to 1.29, `syn` 3.0.6 to 3.0.9, `smallvec`, `zerocopy`, `tokio`, `object` 0.37 to
  0.40, `sha1` and `md-5` to 0.11), `num_cpus` removed, `icu_locale_fallback`, `icu_locale_fallback_data` and
  `utf_types` added.
- **19 git moves**, all outside the rendering stack: the Eidetic family (`6b4ec493` to `29b5db5d`: `eidetic`,
  `muniment`, `chartulary`, `stickleback`, `servitor`, `luggage`, `mere-proofs`, `eidetic-search`), Gemot
  (`d2222357` to `df6da4a1`), Dramatis (`84962207` to `696472da`), Murm (`45b1cf47` to `525d948b`) and `esp`
  (`ca9334d1` to `4e53e7dc`). Genet (`24d7ddb3`), Netrender, Armillary, the Graphshell protocol repository
  (`40e8c8df`) and the Vello fork were already at main. That is the family repin loop's "check Eidetic main for
  other lanes' dep moves first" in action: the lane moves are Eidetic's, not the viewer's.
- It **builds clean** with `--locked` after the update and measures 3,631,331 B (small mode), against 3,625,018 to
  3,627,688 for the unrelocked small band: about +4 to +6 KB, inside the noise of a re-rolled mode. No code changed.
- **Outdated or duplicated in the viewer cone** (metadata of the webgpu feature set, normal edges): two Vello
  pins (`7784613` through Netrender, `dfb9b53` through `sprigging` and the root and web `[patch]` rows), with
  cargo warning on every build that the `netrender-vello-shaders` patch "was not used"; `hashbrown` ×3 (0.15
  `petgraph`, 0.16 `regress` and `spade`, 0.17 the rest), `glam` ×4 (all through `nalgebra`, physics only), `png`
  ×2 (0.17 `mandrake`, 0.18 `netrender-vello`), `miniz_oxide` ×2, `thiserror` 1 and 2 (1 through `personae` and
  `notochord`), `syn` 2 and 3, `foldhash` ×2, `getrandom` 0.3 and 0.4, `bitflags` 1 and 2, `embedded-io` ×2
  (`postcard`): 13 names. Only the `hashbrown` and `png` duplicates leave measurable code. Deprecated crates were
  not checked (`cargo deny` and `cargo audit` were not run).
- The `web/Cargo.toml` comments are partly stale: the profile comment promises the reproducibility it does not
  have, and "Keep every Genet source in step" names rows the standalone lock no longer needs.

## 5. Reorganization options

Estimates are gzip6 on the 3.71 MB large-mode baseline unless stated; "measured" means a build or a diff of two
builds, "estimated" means marginal gzip of function bodies or content arithmetic. The relevant interactions are
not additive; the order matters.

| # | Option | Saving | Basis | What the user loses |
|---|---|---:|---|---|
| 1 | Remove the duplicated `ScriptedDom` instantiation | 83 KB (large to small mode); makes the size deterministic | measured diff | Nothing. Needs a Genet-side change. |
| 2 | Drop the unconditional `png` in `mandrake` (capture IO behind a feature) | about 22 KB | estimated | Nothing in the viewer; capture helpers keep it under a feature. |
| 3 | Subset Plex Sans (Latin, Greek, Cyrillic; keep or drop `GPOS` kerning) | about 30 to 55 KB | estimated; no font tool installed | Glyphs outside the kept ranges; kerning if `GPOS` goes. Matches today's disclosed repertoire. |
| 4 | One artifact: ship the no-Classic-Vello (webgl2-class) build to every browser | 129 KB (small) to 214 KB (large) vs webgpu; the artifact is 3,494,724 B and unimodal | measured | The compute-shader Vello renderer on WebGPU hardware. Needs frame-time evidence (the receipts recorded 16.6 to 16.8 ms median at 21 nodes) and Auto's remount logic goes away. |
| 5 | Table and unused-CSS gating in `genet-livery` (features for table layout, `buckram::table`) | 40 to 100 KB | guessed from `buckram` (+40 KB in the large mode alone) and the table symbols listed above; not built | Tables in Inspector content, if any. Genet-side change. |
| 6 | Physics out of the first load: `seiche` and `rapier` as a second Wasm module in a Worker, loaded on first drag or arrange | about 290 to 364 KB off first load | estimated marginal gz; `canvas/seiche_bridge.rs`, `physics_board.rs` and the existing period worker are the seams | Nothing after load; a delay on first physics use and a message boundary for positions. Real engineering in `pictograph`. |
| 6b | Physics gated out, positions precomputed | same 290 to 364 KB | estimated | Live drag and settle, the "Graph tools" physics section, 17 receipts' subject. Not a flag today (278 references). |
| 7 | "Drop scripting from cambium defaults" | none | no such feature | Not an option; see the claims table. |
| 8 | `serde_json` replaced for dataset parsing (`JSON.parse` through `js_sys`) | up to about 70 KB | estimated | The Rust-side typed deserializers in `mere-kernel`, `sceno` and `scenomise` keep `serde`; the gain is smaller than it looks and the risk is high. Not recommended. |
| 9 | Lean scene player (below) | about 2.5 to 3.4 MB | estimated, unbuilt | Most of the viewer. |
| 10 | Brotli instead of gzip | typically 15 to 20% on Wasm | not measurable here | Needs the site's server and the cap restated in the delivery unit. |
| 11 | `wasm-opt` | negative | the 2026-10-13 measurements (one-tree plan `:1244-1248`) found `-Os`, `-Oz` and converged `-Oz` all worsen gzip | Not worth retrying. |

**Lean scene player.** The payloads that exist: `sceno::Scene` is a *placed projection* (instances with
representation slots, regions, folds), not paint; it still needs node faces (pictograph's IconVG derivation), links
and labels. `vector-sheet` is paintable (compound fills, strokes, theme-role paint, text runs) and a small painter
over Canvas 2D could draw it in tens of KB, but it is the guest-block return type, not what the viewer's graph is
made of. A player that took arranged scenes (so no `scenomise`, kernel or physics), reused `sceno`, `scenotime`
and pictograph faces in Wasm, painted with Canvas 2D through `web-sys` and built the inspector from real DOM
elements would be roughly: `serde_json` 78 KB + `sceno`, `scenotime`, faces and glue 100 to 250 KB + a font if it
does not use the page's, so **about 150 to 400 KB gzip** (a JS-only painter under 100 KB). Costs: it is a second
viewer, so the 24 command, 11 presenter and 17 physics receipts, the AccessKit-derived accessibility and Tabard
theming do not carry over (a DOM inspector would arguably improve accessibility, but nothing has been qualified);
the one-tree plan's stop rules say "no Graphshell-only UI pieces" and "the canvas's rendering, physics and picking
stay pictograph's" (`archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:1315-1321`), which this
contradicts; and it needs a payload the host produces, which the current host dataset is not. It is a product
decision, not a trim.

**Splitting into a core plus lazy chunks.** `wasm-bindgen` has no code splitting, so "lazy chunks" means separate
modules. The only clean seam is physics (row 6); text shaping and the layout engine are the tree itself.

**webgl2 route.** Keeping it costs nothing in the webgpu artifact (separate file); it matters only as a
second 3.49 MB artifact and as the answer to row 4. If row 4 is taken the route question is moot (one artifact);
if not, keep both.

**Recommended target and path.** Target **about 3.0 MB gzip6 (2.9 MiB) for the interactive viewer**, and a cap
ratchet that follows it, with the lean scene player held as a separate product question rather than the plan.

1. Rows 1, 2 and 3: about 3.50 to 3.52 MB, deterministic. No behavior change.
2. Row 4 if the hardware receipts hold: about 3.37 to 3.39 MB, one artifact, no Auto remount.
3. Row 6: about 3.0 MB first load, physics loading on first use.
4. Row 5 if Genet takes it: about 2.9 to 2.95 MB.

Reference points: today 3.62 to 3.71 MB; the original 3.25 MiB target (3.41 MB) sits between steps 2 and 3.

## Decisions for Mark

1. **Cap and target policy.** The 5.00 MiB cap was raised to cover a build-path artifact, not because the viewer
   needs it. Options: (a) keep 5.00 MiB; (b) ratchet with the work: 4.0 MiB after step 1, 3.5 MiB after step 2,
   3.25 MiB after step 3, each restated once the size is deterministic; (c) state the target in the unit the
   site actually serves (brotli) once measured. *Recommend (b)*, and fix the README and T7 claims the audit
   found stale (the nonexistent `viewer-delivery` job, "reproducible", the 45 KB scripting trim, the presentation
   plan's 3.30 MiB line).
2. **Physics in the first load.** Options: (a) keep it in the main module; (b) a lazy Worker module (about 290 to
   364 KB off first load; first drag pays a load); (c) precompute positions and drop live physics from the viewer.
   *Recommend (b)*, after step 1; (c) removes the feature the receipts qualify.
3. **One artifact or two.** Options: (a) keep webgpu with Classic Vello and webgl2 as fallback; (b) ship only the
   no-Classic-Vello build everywhere, dropping the compute-shader renderer and Auto's remount; (c) decide after
   the headed receipts run the webgl2 artifact on WebGPU hardware. *Recommend (c), leaning (b)*: it is the largest
   change that needs no new code, and it also removes the second size band's other half. It needs the site's
   browser-support picture, which I could not verify.
4. **Genet-side work.** The duplicate instantiation (row 1) and table gating (row 5) are changes in the Genet
   repository. Options: (a) ask for both there; (b) work around in Mere with a canonical build path only;
   (c) wait for engine-split P3 and revisit. *Recommend (a) for row 1*, because (b) and (c) leave the size to a
   hash roll; row 5 only if the table code proves unused in the viewer's panels.
5. **Scene player.** Options: (a) not now; (b) a one-page spike that paints a `vector-sheet` and an arranged
   `sceno::Scene` on Canvas 2D and reports bytes and what is missing, no commitment; (c) commit to a second viewer.
   *Recommend (b)*: the stop rules of the one-tree plan point the other way, and the numbers above are
   estimates until something is built.

## Rulings (Mark, 2026-11-07)

Recorded in the Graphshell host plan, T7, with the ratchet schedule and the physics seam design.

1. **Cap and target policy: "Ratchet down".** Option (b). The Wasm cap moves from 5.00 MiB to 4.0 MiB (4,194,304 B)
   now; 3.5 MiB when physics leaves the first load and Genet's duplicate fix lands; 3.25 MiB when the remaining
   stages land. A step is applied only after it measures green in both build modes. Applied: `build-viewer.py`,
   `test_viewer_budget.py`, both READMEs and the plans carry 4.0 MiB.
2. **Physics in the first load: "Lazy Worker module".** Option (b). The seam is designed in T7; the implementation
   is not done (see T7: it needs a `seiche` split and a command protocol, larger than one change).
3. **One artifact or two: "Decide after receipts".** Option (c). Open until the headed receipts run the webgl2
   artifact on WebGPU hardware; leaning to one artifact (option (b)).
4. **Genet-side work: "Both in Genet now".** The duplicate `ScriptedDom` pipeline (row 1) and the table gating
   (row 5) are being done by another agent in the Genet repository. Not touched from Mere.
5. **Scene player: "One-page spike".** Option (b). Another agent is running the spike; not built here.

**Stale claims fixed (2026-11-07).** The presentation plan's 3.30 MiB cap line, and the host plan's and the scenograph
and physics-catalog plans' older cap figures, now read 4.0 MiB. The README no longer names a `viewer-delivery` CI job
(none exists; Actions are disabled; the gate is `build-viewer.py` plus `test_viewer_budget.py` run by hand) and no
longer implies a reproducible package. `web/Cargo.toml`'s profile comment drops "every build makes the same bundle".
T7's "cambium default scripting, about 45 KB" is removed (no such cambium feature) and its "physics about 200 KB" now
reads 290 to 364 KB gzip, as measured here.

## Physics seam: options and costs (2026-11-07)

The design is in the host plan's T7. It was not built, because making the main module engine-free is not one
change. Estimates below are from reading the code, not from a build.

- **A. Full lazy Worker module (the ruling).** (1) Split `seiche` into a rapier-free API crate and the engine; the
  engine re-exports the API. (2) Make the command set data: `SetForces(Vec<Box<dyn Force>>)`,
  `SetCouplingEntryHandler` (a closure) and `SetLaggedRepulsion` (a boxed GPU solver, off in the viewer) cannot cross
  a message boundary, so the catalog's force construction (`composition.rs`, `physics_catalog.rs`, about 2,000
  lines) moves to the engine side behind descriptors, and the force declarations that `DynamicsSpec::derive` reads
  stay on the main side as data. (3) A `Physics::Remote` backend with a serializer, a new cdylib crate, a
  `physics-engine` feature through four crates, a second package row in `build-viewer.py`, `viewer.js` glue, and an
  asynchronous companion for the 17 physics receipts and the `seiche-repeat` determinism probe. Roughly 5 to 8
  focused sessions, the riskiest being step (2). Saves 290 to 364 KB gzip from each route's first load; the Worker
  artifact adds about 0.5 to 0.75 MiB gzip loaded later.
- **B. Gate physics out of the viewer, precomputed positions (audit 6b).** Same saving, much cheaper (the `seed_cluster`
  spiral or saved facets feed first paint; `Canvas` sees a no-op backend), but it removes live settle, drag and the
  "Graph tools" physics section the receipts qualify. This contradicts the ruling's premise.
- **C. A smaller first cut of A.** Do step (1) alone (the `seiche` split) as its own change: no behaviour change, the
  engine re-exports the API, every consumer compiles unchanged, and it can be measured by itself. It makes the rest a
  bounded sequence and is the recommended next change if A is to proceed.

