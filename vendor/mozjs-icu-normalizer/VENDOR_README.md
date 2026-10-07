# Mozilla ICU normalizer compatibility candidate

Unqualified copy of the published `mozjs_icu_normalizer` 153.3.0 crate,
archive SHA-256 `362f589e9c5f836baade5fe90d587ffcab715c4e6bc01d6e2f266f939529876d`.
Published provenance is servo/mozjs commit
`f295442b3eb803b06674790d338b8151d9ca9980`.

Only normalized `Cargo.toml`'s optional production `icu_properties` requirement
changes from `~2.1.1` to `^2.1.1`. Published Rust, data, license, README, original
manifest, and VCS bytes remain unchanged. `LICENSE` retains Unicode-3.0 terms.
Cache-owned markers are omitted.

The widened range permits the current registry ICU family; it does not prove
compatibility of private provider/data schemas. Consumer metadata, compilation,
and Intl behavior must qualify the candidate. Root owns its path patch.

[Exact inventories, original manifest and diff](../../docs/receipts/browser_family_20261007/icu-manifest-compat/README.md).
