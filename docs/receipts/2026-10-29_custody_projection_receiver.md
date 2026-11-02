# Custody and projection receiver

Status: Migrated; locked native library check passed with inference and authoring.

Turnstone explicitly selects custody-client and projection-door. Device receipts,
inference and Knot authoring use `door::projection::{AppBrokerClient, AppRouteCarrier}`.
The removed mixed-client imports are not retained as aliases.

The lock receives Mere `c6290142866eefead385e728c5c5da24ac6f624a`, Knot
`704084d064da886dd14cbe8ac3dd8758af105e1d`, Dramatis
`051e95a68011072f172b9ec4c0f04d4e5259df5b` and Murm
`1f601488f851c4e54672cd439b7c45728255ea65`. The latter two pins preserve
Knot's upstream secure-publishing requirements. Genet and unrelated registry
selections stay unchanged. Resolved lock review found 77 supplier identities
advancing and 20 unchanged-identity owners changing supplier targets, with no
changed dependency target names or versions.

`cargo check --locked -p turnstone --lib --features inference,authoring` passed
on Linux with Rust 1.99.1, under the shared 3 GiB, zero-swap, 150 percent CPU,
one-job gate. This compiles all three moved import sites. It is not a live
resident, browser, Windows or GUI receipt. The existing ignored inference probe
requires its isolated resident fixture and was not executed.

The successful command and exit status are retained on T-PC in
`/home/markik/Code/testing/custody-contract-20261029/root-split-receivers-check.log`
and `testing/custody-projection-split-20261029/receiver-check-results.json`.
The enclosing multi-receiver gate records other consumers separately.
