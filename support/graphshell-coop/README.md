# Native co-op qualification

Use `run.py` for the active Mac-to-ThinkPad workflow and `verify.py --receipt
RECEIPT --commit COMMIT` for the public packet. `--mode direct`, `--mode relay`,
and `--mode recovery` share one scope, attribution, revocation and cold-reopen
controller. Relay modes require `--relay-binary`, its `--relay-sha256`, and an
owned SSH control socket; SSH carries control and invitation handoff only.
`run.py --help` lists the two source trees, frozen binary roots and private
output paths. Existing output directories are refused.

Recovery stops the owned relay before a guest save, records pending and failed
native states, checks the owner's durable graph before and after the failure,
then restarts the relay on its original address. The same guest window uses
Reconnect and explicitly retries its edit. Only an owner acknowledgement is a
save. A later live withdrawal and full cold reopen prove that recovery did not
widen scope or restore a revoked grant.

`machine.py` composes the frozen desktop/custodian provider and optional Linux
network-namespace provider from the original receipts. `freeze.py` records all
executed provider hashes along with current binaries, workspace inputs and
resolved suppliers. Historical receipts retain their exact scripts and hashes;
new modes and shared checks belong here rather than in another dated script
copy. The verifier also accepts the original relay packet for regression checks.
New receipts contain evidence, provenance and run-specific observations.

The relay is a short-lived private-LAN fixture, using the original locked Iroh
relay executable. Relay modes check that the guest has no UDP egress, while
TCP reaches the relay. Every admitted guest capture must report that exact
relay. This does not qualify WAN deployment or TLS relay operation.
