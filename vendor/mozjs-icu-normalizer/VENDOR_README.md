# Mozilla ICU normalizer compatibility candidate

Unqualified copy of the published `mozjs_icu_normalizer` 153.6.0 crate,
archive SHA-256 `f4c5c74ab2d167ca63b41b7e8b6fce2ecc047e8342a90208c422c73d6bc9a099`.
Published provenance is servo/mozjs commit
`f02d3753d1027aee5714ffef182d34caea3be4eb`.

Only normalized `Cargo.toml`'s optional production `icu_properties` requirement
changes from `~2.1.1` to `^2.1.1`. Published Rust, data, license, README, original
manifest, and VCS bytes remain unchanged. `LICENSE` retains Unicode-3.0 terms.
Cache-owned markers are omitted.

The widened range permits the current registry ICU family; it does not prove
compatibility of private provider/data schemas. Consumer metadata, compilation,
and Intl behavior must qualify the candidate. Root owns its path patch.

[Exact inventories, original manifest and diff](../../docs/receipts/browser_family_20261007/icu-manifest-compat/README.md).
