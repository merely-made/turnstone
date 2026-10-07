# Mozilla ICU manifest compatibility candidate

**Status, 2026-10-07: UNQUALIFIED.** Root requested two exact published Mozilla
153.3.0 bundles with three manifest ranges widened. This lane has prepared the
candidate and its provenance; it has made no Root production Cargo/Rust/fixture
edits and run no Cargo, compilation, or native gate.

The preceding [official CAPI isolation proof](../icu-source-isolation/README.md)
remains valid, but Root's actual `resolve-icu-isolated` metadata attempt failed.
The private Mozilla collator additionally requires registry `icu_locale` and
`icu_properties` at `~2.1.1`, conflicting with the current Genet ICU family.
The private normalizer has another resolver-relevant optional properties edge.

The candidate uses official crates.io bundles:

| Package | Published files | Published bytes | Archive SHA-256 |
| --- | --- | --- | --- |
| mozjs_icu_collator 153.3.0 | 28 | 5,876,256 | `e483fdb5592a4f52c2d0e3c419935acfb98a5b578154b93999b92dedbf170e0d` |
| mozjs_icu_normalizer 153.3.0 | 40 | 435,570 | `362f589e9c5f836baade5fe90d587ffcab715c4e6bc01d6e2f266f939529876d` |

Both archive checksums match their preserved official version responses.
Their published VCS records identify servo/mozjs
[`f295442b3eb803b06674790d338b8151d9ca9980`](https://github.com/servo/mozjs/commit/f295442b3eb803b06674790d338b8151d9ca9980).
All Rust, data, license, README, original-manifest and VCS bytes are retained.
Only the two normalized production manifests differ. Unicode-3.0 `LICENSE`
files are preserved; cache-owned `.cargo-ok`/`.cargo-checksum.json` markers are
omitted. Each vendor directory adds one separately identified provenance note.

The exact changed ranges are:

| Package / production dependency | Before | After |
| --- | --- | --- |
| collator / icu_locale | `~2.1.1` | `^2.1.1` |
| collator / icu_properties | `~2.1.1` | `^2.1.1` |
| normalizer / optional icu_properties | `~2.1.1` | `^2.1.1` |

The [summary](candidate-summary.json), [collator inventory](mozjs-icu-collator-inventory.json),
[normalizer inventory](mozjs-icu-normalizer-inventory.json), and two manifest
diffs bind the exact package inputs and alterations. Original normalized
manifests and official registry metadata remain beside those files.

A global Git ICU 2.1 patch is not an established type-safe alternative. The
collator's baked-data macros pass its provider fallback configuration and
locale-core request types to `icu_locale`; its glue also passes locale-core
types directly. Mixing a Git locale/provider family with registry caret
dependencies can split those Rust identities. These manifest changes instead
allow the private forks and glue to share the current registry family while
leaving the unchanged CAPI behind its separate official Git source boundary.

The private crates explicitly describe their provider/data Rust representation
as unstable across minor versions. Widening a manifest range therefore requires
real selected-source/feature inspection, typed private-fork/glue checks, and
the Root-owned Intl scenarios before acceptance. Metadata alone is insufficient.
No ABI, native behavior, release, or publication claim is made here.
