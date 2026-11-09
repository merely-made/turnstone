# Standalone authoring receipts — 2026-10-08

Native macOS x86_64 captures from `scenarios/usability.scn` and
`scenarios/usability_reopen.scn`, using a separate fresh library under
`/tmp/tabard-usable-native-final-20261008`. Both processes exit successfully:
95 frames / 5 captures for authoring, then 18 frames / 1 capture for a fresh
process. All six captures are nonblank. Receipt paths retain their original
capture locations; the PNGs here are unchanged copies.

The controls enter `#2F7FFF`, set high-contrast dark as the authored default,
apply exact CSS custom properties, save, reopen, clear to derived, undo and
resize to 640 × 780. The second process restores the theme, selected mode and
stylesheet. Wide captures are 2360 × 1600 pixels (1180 × 800 logical at 2×);
the narrow capture is 1280 × 1560. The native startup briefly reported an
occluded surface, then recovered and completed both scenarios.

`themes.json` and its workshop sidecar preserve the resulting authored library
and editor selection. `sample-theme.json` is the saved definition extracted
from that library and can be imported through the workshop's theme control.
These are fixture data, not the user's default library.

The shared Tabard suite passes 70 tests, the workshop passes 40 and the desktop
passes 9. The retained checks use actual controls/native hooks, temporary-file
writes, isolated computed CSS and close decisions. They cover incomplete RGB,
alpha/history, custom-sheet removal through history, import collisions,
explicit replacement, library/sidecar alias protection, deletion, remembered
selection and failed writes. Native OS chooser panels and a live screen-reader
session were not automated; bounded document previews expose image names.

The host file-routing suite also passes 4 tests. Strict Clippy with `--no-deps`
passes for both ports; shared Tabard Clippy completes with its existing three
source warnings and seven library-test clone warnings. Port boundaries, scoped
formatting and staged diff checks pass.

The broader documentation judgment audit still reports its inherited snapshot
digest, duplicate browser-receipt README and missing D2 record errors
(257/258 active documents). This slice updates the existing owning plan and
index; it adds no active design document.
