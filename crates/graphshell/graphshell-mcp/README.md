# graphshell-mcp

A stdio [Model Context Protocol](https://modelcontextprotocol.io) server over one
admitted Graphshell route. An agent harness (Claude Code, Codex) launches it as a
subprocess and reads and writes a mere through it, as a granted participant.

It is the optional MCP adapter the
[Graphshell reference host plan](../../../design_docs/archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md)
(H9) and the
[djinn family resident services plan](../../../design_docs/archive_docs/2026-10-28/2026-08-22_djinn_family_resident_services_plan.md)
(F2) schedule, over the same projection and intent grammar every Graphshell
client uses. It is not a domain model and adds no authority.

## Identity and grants come from the stack

The server holds no key and mints no grant (doctrine, Stack 2). `mere-mcp`
names itself with `--app` and dials djinn's owner-only application door. Djinn
admits that app id only for the routes its owner allows, serves it the
projections that app may see, and attributes every accepted edit to the persona
*via* that app (`Author::person(persona).via(app)`, `ports/djinn/src/resident_mere.rs`).
A refusal on the route is a refusal here, and reaches the model as a tool error
it can read. To give two harnesses different reach, run them as different app
ids; to withdraw one, revoke its route at djinn.

## Run

```text
cargo build -p graphshell-mcp --features door --release

claude mcp add mere -- target/release/mere-mcp --app mcp.claude-code --route mere/<domain>
codex  mcp add mere -- target/release/mere-mcp --app mcp.codex       --route mere/<domain>
```

`--route` is the route djinn registers for a mere in its reservoir,
`mere/<domain>`. `--endpoint` overrides `GRAPHSHELL_APP_ENDPOINT`. `--read-only`
leaves out the write tool. Standard output carries only MCP messages; diagnostics
go to standard error. The process exits when the client closes its input or the
resident drops the session, and the client restarts it (the protocol is stateless).

To try a client with no resident, `cargo run -q -p graphshell-mcp --example
notes_route` serves the in-memory route the tests use.

## What it exposes

| MCP | From the route |
|---|---|
| resource `mere://<route>/<projection>` | one projection's snapshot, as a JSON document: each instance with its source identity, the presentation offers bound to it with the endpoint's label, role and advertised actions, and the cards and text the endpoint disclosed (up to 256 KiB each); then the relations. Geometry is left out. What the grant withholds is absent. |
| tool `mere_actions` | every advertised action, with its target instance, intent, payload schema and effect class, and the session, epoch and revision it was read at |
| tool `mere_invoke` | `{projection, instance, intent, payload, observed_epoch, observed_revision}`; the one write path |

The write tool is one envelope on purpose, as `crates/script/app-host`'s action
envelope is: the surface does not change when the route's actions do, and the
route stays the only judge of an intent. The observed epoch and revision are
required, so the route's stale check stays the caller's to satisfy; `stale`
comes back with the current pair. `payload` is JSON, sent as its text, or a string
sent verbatim. A mere's graph is edited through the session item's
`mere.session.apply` action, whose schema `mere_actions` shows.

An agent that only needs to remember things does not compose graph edits. Djinn's
mere route also serves a **Notes** projection, and its card carries two actions.
`mere.notes.append` appends a typed note: kind, subject, djot body and evidence
refs. It is stored as a Knot note in the attached session and attributed like any
edit. `mere.notes.search` sets the text the projection is filtered by, after which
the agent reads the Notes resource. Each action's explanation spells out its
payload.

## Protocol

MCP 2026-07-28 (`server/discover`, per-request `_meta`, `resultType`) and the
handshake revisions 2025-11-25 to 2024-11-05, chosen per request: Claude Code
2.1.308 probes `server/discover` and then opens with `initialize`, so both halves
are in use. Implemented: `resources/list`, `resources/read`, `resources/templates/list`
(empty), `tools/list`, `tools/call`, `ping`. Not implemented: prompts, completion,
logging, subscriptions, tasks, sampling and elicitation, and any HTTP transport.
Nothing is cached (`ttlMs` 0), since another participant may change a mere at any
time.

## Tests

`cargo test -p graphshell-mcp` drives the real JSON-RPC against a route that
behaves like djinn's (opens a session, refuses an ungranted write, checks the
observed revision). `cargo test -p mere-resident --lib resident_mere::tests::mcp` serves
the adapter over djinn's real mere route (the reservoir, the route grants and the
session item, through the endpoint catalog the application door uses). There an
unadmitted app reaches no mere; an admitted agent lists the route's projections,
applies an edit through `mere_invoke`, reads it back, undoes it, and the stored
change names the app the door admitted; a malformed edit is the route's refusal and
leaves the mere alone; and the read-only ambient route advertises no edit. Not
covered: the socket itself (`AppRouteCarrier` against a running `djinn`) and
a model driving the tools.
