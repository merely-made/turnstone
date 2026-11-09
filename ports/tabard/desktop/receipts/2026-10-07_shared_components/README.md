# Shared-component native receipts

Captured on macOS x86_64 with the actual `tabard-desktop` host, its shared
render core/device and Mesquite scenarios. Wide frames are 1180 × 800 logical
pixels at 2× scale; narrow frames are 640 × 780. The scenarios operate normal
controls rather than mutating product state.

## Complete scenario acceptance before the shared font correction

All specimens already use Illume/Cambium highlighting, the extracted
Fleece/Inker/document-canvas reader, and Cambium/Sprigging graph paint.

| Scenario | Scenario frames | Captures | Receipt |
| --- | ---: | ---: | --- |
| Shared components | 79 | 9 | `before_font_shared.done` |
| Authoring/save/reopen | 70 | 8 | `before_font_authoring.done` |
| Fresh-process reopen | 9 | 1 | `before_font_reopen.done` |
| Narrow composition | 23 | 3 | `before_font_narrow.done` |

All four runs exit successfully: 181 scenario frames, 21 captures, none blank.
The complete receipts retain each capture's digest. Representative images
cover both high-contrast modes, graph selection, narrow reader/graph and
fresh-process reopening. Full local output is under
`/Users/markik/Code/tabard-workshop-receipts/2026-10-07_shared-components`.

## Typography correction and recapture limit

These captures exposed a shared font interning defect: the reader's regular
and bold faces share a collection allocation, and deduplication by blob ID
alone made the body use the heading face. Document-canvas now interns by
`(blob_id, collection_index)`; its deterministic regression passes.

`font_fixed_light.png` and `font_fixed_dark.png` are valid fresh native
captures from the rebuilt host. They show a bold heading followed by regular
body text. The enclosing shared-component run captured these two images but
failed later; it is not a passing full scenario receipt.

| Font-corrected attempt | Scenario frames | Captures | Result |
| --- | ---: | ---: | --- |
| Shared components | 199 | 2 | Capture timeout at high-contrast light |
| Shared components retry | 199 | 0 | Capture timeout at initial frame |
| Authoring/save/reopen | 190 | 0 | Capture timeout at initial frame |
| Fresh-process reopen | 129 | 0 | Capture timeout at initial frame |
| Narrow composition | 143 | 0 | Capture timeout at initial frame |

The failed `font_fixed_*.done` receipts are retained. Rootstock repeatedly
reported surface acquisition `Occluded`; the presented-frame callback did
not run, leaving captures pending past Mesquite's 120-frame grace. No blank
image or reader raster error was produced. The authoring controls and save
assertions continued, but that does not establish native frame acceptance.
Full failed-run logs remain under
`/Users/markik/Code/tabard-workshop-receipts/2026-10-07_shared-components-font-fixed`.

## Retained and shared checks

The final Tabard suites pass 22 tests: six reader/graph unit tests, twelve
mounted surface/component tests and four desktop tests. Cambium's
highlight-enabled suite passes 263 tests plus a compile-fail doctest, with one
existing editor doctest ignored. The new checks cover real lexer lexemes,
all sixteen local palette variables across four modes, UTF-8 preservation,
reader source identity, native graph selection and matching hit/paint geometry.
Strict `--no-deps` Clippy passes for both Tabard packages; port boundaries pass.

The wider document-canvas suite passes 85 tests and fails the existing
`normal_width_table_wraps_unbroken_link_inside_its_cell` geometry assertion.
The original HEAD font interner and the corrected interner fail identically
in a controlled comparison. Both font-table identity tests pass, and the
document-canvas doc-test target completes with no tests. Local comparison
logs are `/tmp/tabard-font-baseline-table.log`,
`/tmp/tabard-font-fixed-table.log` and `/tmp/tabard-font-fixed-identity.log`.

The reader is a bounded read-only appearance with extracted prose exposed as
an image name. This evidence does not claim link activation, rich session
accessibility, a live screen-reader session or a full graph workspace.
