# Official ICU4X CAPI source isolation

**Research result, 2026-10-07:** a single top-level `icu_capi` patch can select
the official source of the unchanged published CAPI 2.1.2 package. Actual
consumer resolution, compilation, and runtime acceptance remain Root-owned.
This research lane made evidence-only writes and ran no Cargo command.

The published package's [VCS record](registry-capi-cargo_vcs_info.json) points
to official ICU4X commit
[`30120d6a07fc5bb03a6c7647077bd2658b2559de`](https://github.com/unicode-org/icu4x/commit/30120d6a07fc5bb03a6c7647077bd2658b2559de).
It is the February 9 CAPI 2.1.2 release-source commit. The
[official crate metadata](official-crates-capi-version.json) checksum matches
the cached published archive; all 1,050 indexed source, generated-binding,
test, and original-manifest files in that archive match official Git blobs.
See the [archive verification](official-crate-archive-verification.json) and
[complete identity index](capi-source-identity-index.json). No CAPI 2.1.2 tag
was exposed by the [official tag inventory](official-tags.json); the older
`icu@2.1.0` tag is not a substitute for this exact released-source revision.

Proposed addition to the existing Root patch table:

```toml
[patch.crates-io]
icu_capi = { git = "https://github.com/unicode-org/icu4x", rev = "30120d6a07fc5bb03a6c7647077bd2658b2559de" }
```

The [official workspace manifest](official-Cargo.toml) declares ICU component
dependencies with workspace paths and compatible 2.1 requirements. The
[CAPI manifest](official-ffi-capi-Cargo.toml) is version 2.1.2 and inherits
those paths. Git path children therefore form a separate source family from
Genet's registry ICU 2.2 dependencies. Patch only CAPI; a global patch of the
ICU children would erase the intended isolation. The
[workspace audit](workspace-path-family-audit.json) records versions, paths,
dependencies, manifest hashes, and exact primary links. Normalizer 2.1.1,
properties 2.1.2, and segmenter 2.1.2 also match their published source/original
manifests at 5, 13, and 16 files respectively, with no mismatches in the
[component comparison](component-source-identity.json). That comparison does
not cover generated child data or every utility package.

The source substitution retains published CAPI/binding bytes and mozjs/Genet
version minima. It does not assert that bundled mozjs headers equal all CAPI
headers: the existing registry pairing already has four differences among 329
C headers, as the separate boundary review found. Exact CAPI Git identity
preserves that upstream pairing rather than introducing new headers. The
source-family argument still requires real metadata and typed/native checks;
it is not an ABI or runtime qualification receipt.
