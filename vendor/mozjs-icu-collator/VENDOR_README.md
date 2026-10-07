# Mozilla ICU collator compatibility candidate

Unqualified copy of the published `mozjs_icu_collator` 153.3.0 crate,
archive SHA-256 `e483fdb5592a4f52c2d0e3c419935acfb98a5b578154b93999b92dedbf170e0d`.
Published provenance is servo/mozjs commit
`f295442b3eb803b06674790d338b8151d9ca9980`.

Only normalized `Cargo.toml` production requirements for `icu_locale` and
`icu_properties` change from `~2.1.1` to `^2.1.1`. Published Rust, data, license,
README, original manifest, and VCS bytes remain unchanged. `LICENSE` retains
Unicode-3.0 terms. Cache-owned markers are omitted.

The widened range permits the current registry ICU family; it does not prove
compatibility of private provider/data schemas. Consumer metadata, compilation,
and Intl behavior must qualify the candidate. Root owns its path patch.

[Exact inventories, original manifest and diff](../../docs/receipts/browser_family_20261007/icu-manifest-compat/README.md).
