# Mozilla ICU collator compatibility candidate

Unqualified copy of the published `mozjs_icu_collator` 153.6.0 crate,
archive SHA-256 `00b76d0de5caa407560685c101a9823a58902008b3d9e6c062c4315c66917945`.
Published provenance is servo/mozjs commit
`f02d3753d1027aee5714ffef182d34caea3be4eb`.

Only normalized `Cargo.toml` production requirements for `icu_locale` and
`icu_properties` change from `~2.1.1` to `^2.1.1`. Published Rust, data, license,
README, original manifest, and VCS bytes remain unchanged. `LICENSE` retains
Unicode-3.0 terms. Cache-owned markers are omitted.

The widened range permits the current registry ICU family; it does not prove
compatibility of private provider/data schemas. Consumer metadata, compilation,
and Intl behavior must qualify the candidate. Root owns its path patch.

[Exact inventories, original manifest and diff](../../docs/receipts/browser_family_20261007/icu-manifest-compat/README.md).
