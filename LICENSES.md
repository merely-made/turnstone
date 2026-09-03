# Licenses in this repository

**This repository: MPL-2.0.** Every file Mark wrote carries Exhibit A and the
SPDX tag `MPL-2.0`, per the
[license posture brief](../mere/design_docs/2026-08-22_license_posture_brief.md)
of 2026-08-22 (mere `design_docs/2026-08-22_license_posture_brief.md`). The
full text is in [`LICENSE`](LICENSE).

This file is the provenance ledger. It is the authority for what the relicense
tool (mere `scripts/relicense_headers.py`) skips: the backtick-quoted paths in
the **Retained licenses** table are never touched. Provenance comes before
license: a file gets Exhibit A only if Mark wrote it.

## Retained licenses

Third-party code keeps its own license and its own notices. Nothing here is
relicensed, and nothing here receives a Merely copyright line.

| Path | License | Upstream | Notice files |
|---|---|---|---|

**Empty, as of 2026-09-03.** Turnstone vendors nothing. The sweep plan's
invariant 1 discovery — `Copyright` unqualified, `Licensed under`,
`Permission is hereby granted`, `Apache License`, and any SPDX line naming
something other than MPL-2.0 — was run over every tracked file and returned
hits only inside the repository's own `LICENSE-MIT` and `LICENSE-APACHE`,
which were Mark's own dual-license texts and are removed by this sweep. The
scenario fixtures under `scenarios/fixtures/` (PowerShell servers, Lua
handlers, HTML and Gemini pages, the `app_core_guest.wasm` guest) are Mark's
own test material, not imports. Turnstone's third-party code all arrives as
Cargo dependencies, which carry their own terms in their own registries and
are not part of this tree.

## Derivatives carrying MPL-2.0 with an upstream notice retained

**None.** No file in this repository is a rework of someone else's code, so
there is no upstream notice to retain. If one ever arrives, it is recorded
here and applied with the tool's `--retain-notice`, following mere's `luggage`
and genet's `meristem`: Exhibit A and Mark's copyright line are added, and
every upstream copyright line above them is kept verbatim.

**This section is deliberately not the skip list.** The tool reads only the
`## Retained licenses` table above. Adding a path here documents a
disposition; it does not exempt the path from receiving a header.

## Exceptions under the fork/vendor criterion

**None.** The brief's §4 test — a crate stays MIT OR Apache-2.0 only when a
third party would need to *modify or vendor* it rather than merely link it —
admits nothing in this repository. Turnstone is an application, not a library
offered for embedding.

If one is ever granted, its manifest says `MIT OR Apache-2.0` explicitly with a
comment naming the brief, and it is listed here.

## How to add a file from elsewhere

1. Do not delete or rewrite the upstream copyright or license notice, ever.
2. Add its path to **Retained licenses** above with its license, upstream URL,
   and where its notice text lives. The tool then skips it automatically.
3. If it is a substantial derivative rather than a verbatim import, the brief's
   rule is MPL-2.0 on the derivative *with the upstream notice retained*;
   record it in that section so the distinction is not lost.
4. Never add `license-file` to an owned manifest.
5. Re-run `python ../mere/scripts/relicense_headers.py --repo . --audit` and
   confirm the owned source count moved by exactly what you expected.
