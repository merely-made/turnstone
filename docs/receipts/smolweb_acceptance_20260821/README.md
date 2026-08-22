# Smolweb acceptance receipt, 2026-08-21

`acceptance.done` is the summary. Every headed case also keeps its own
`scenario.done`; Titan and Spartan keep the server-side wire receipt beside
the captures.

## Passed gates

- **Gemini browse:** a live request to `gemini://geminiprotocol.net/` rendered
  as a content surface, and the first retained relative link opened as a new
  graph address. See [capsule](gemini-browse/01_gemini_capsule.png) and
  [followed link](gemini-browse/02_followed_gemini_link.png).
- **Gemini status 10:** a live request to
  `gemini://kennedy.gemi.dev/search` opened the in-app prompt, submitted
  `gemini protocol`, refetched, and rendered the result. See
  [prompt](gemini-input/01_gemini_input_prompt.png) and
  [results](gemini-input/02_gemini_search_results.png).
- **Gemini inline image:** a local TLS capsule served gemtext and a PNG as two
  requests. Turnstone fetched the image through its actor and durable TOFU
  path, decoded it within the document policy, and painted it inside the
  retained content surface. See the [capture](gemini-inline-image/01_inline_image.png),
  [wire receipt](gemini-inline-image/server.done), and
  [clean-archive receipt](gemini-inline-image/clean-archive.done).
- **Gemini download custody:** a local TLS capsule served a 13-byte binary
  response. Turnstone retained the exact bytes, deposited them in the session
  representation store, persisted the node's content hash and completed
  custody facet, wrote one collision-safe `archive.bin`, and projected the
  record through Steward. See the [capture](gemini-download/01_download_steward.png),
  [wire receipt](gemini-download/server.done), [custody receipt](gemini-download/custody.done),
  and [focused-test receipt](download-tests.done).
- **Gemini streaming render:** a local TLS capsule sent a 65-byte gemtext
  prefix, withheld its 57-byte tail until Turnstone captured the live content
  surface, then completed the response. The final app-authored frame preserves
  the prefix and adds the tail. See the [prefix](gemini-streaming/01_streaming_prefix.png),
  [complete document](gemini-streaming/02_streaming_complete.png), and
  [wire ordering receipt](gemini-streaming/server.done).
- **Gemini typography:** a local TLS capsule rendered a site-derived palette,
  serif body and heading hierarchy, a centered readable measure, Latin, Greek,
  and Japanese text, distinct same-capsule and web-link arrows, and a monospace
  preformatted block. See the [capture](gemini-typography/01_typography.png),
  [wire receipt](gemini-typography/server.done), and
  [scenario receipt](gemini-typography/scenario.done).
- **Titan mutation:** the composer accepted a dropped 19-byte LF body,
  changed MIME to `text/plain`, masked the optional token, required literal
  `send`, issued one actor command, and received status 20. The server receipt
  validates the target, body size, MIME, token, and response. See
  [composer](titan-mutation/01_titan_body_composer.png),
  [dropped body](titan-mutation/02_titan_dropped_body.png),
  [masked token](titan-mutation/03_titan_hidden_token.png),
  [confirmation](titan-mutation/04_titan_literal_confirmation.png), and
  [success](titan-mutation/05_titan_success.png).
- **Spartan mutation:** a server-authored `=:` row opened the same composer,
  submitted the 18-byte body to `/submit`, issued one actor command, and
  received success. The server receipt validates both the zero-byte fetch and
  the body submission. See [prompt](spartan-mutation/01_spartan_prompt.png),
  [confirmation](spartan-mutation/02_spartan_literal_confirmation.png), and
  [success](spartan-mutation/03_spartan_success.png).
- **Livery to Knot source evidence:** the ignored environment-gated test ran
  against a real `knot_endpoint` and passed. See
  [knot-source-evidence.done](knot-source-evidence.done).
- **Focused units:** Gemini status-10 routing and masking, Spartan target
  resolution, Titan composer behavior, the live Nematic route, image
  promotion/decoding/paint/click, fetch correlation, and actor completion
  passed. See [focused-tests.done](focused-tests.done) and
  [inline-image-tests.done](inline-image-tests.done).

## Execution boundary

The headed mutation executable was linked from the exact library artifact
produced for `b79baae` (`Wire explicit Titan and Spartan submission UI`). Its
SHA-256 is
`BFBD3BB4DBD986CED93F637E5717F8D532239330A79DD78E9472D28126380C2A`.
Current Turnstone main contains that slice; the later `84d6d5a` change selects
Livery for ordinary HTML and does not replace the smolweb composer or actor
path. The source-evidence test used the later current-main test artifact, whose
hash is recorded with the endpoint hash in its receipt.

The successful local mutation pair was run together through
`scenarios/run_smolweb_acceptance.ps1`; its aggregate result was:

```text
RESULT ok
RESULT ok titan-mutation
RESULT ok spartan-mutation
```

The live Gemini hosts are intentionally network-dependent, so their two
successful receipts were retained from the first clean run rather than made a
precondition for rerunning a local mutation case.

The inline-image case is local and deterministic. Its binary was built from a
lockless archive of Turnstone `c3994f7`, resolving Genet `e911b348` and Mere
`425d4252`, after `cargo check --tests` passed. The binary SHA-256 is
`52581CB03354896CB3635517C0B73A35499CB1EB177204ABFFCBB67FDA560775`.
The server receipt proves separate page and PNG requests; the app-authored
capture proves the fetched pixels reached the content surface.

The download case is also local and deterministic. Its clean detached binary
was built from Turnstone `372f156`, resolving Genet `e911b348` and Mere
`c6cb48b4`; its SHA-256 is
`51173E8B23B57746100C5514B591891DD25845557D386F6D91CA9A06099E0A07`.
The server receipt proves the binary response, the custody receipt independently
checks its bytes and persisted graph/facet/store evidence, and the app-authored
capture proves Steward rendered that durable record.

The streaming case is local and deterministic. Its clean detached binary was
built with `--locked` from Turnstone `f0be94f`, resolving Genet `ee3166c29e1`,
Mere `0953dd9522e`, and Netrender `4269ca583cc`; its SHA-256 is
`8FB4DDBCF0D556AD85E92EF0B05AF2A7DBAE8FE79D91D0B55F5F9E01E02AA69E`.
The server receipt proves that the prefix capture released the response tail.
The two app-authored captures prove visible rendering before EOF and a complete
final document. Netrender's retained tiles now compose in stable painter order,
and focused CPU and GPU replacement regressions cover the prefix-preservation
contract.

The typography case is local and deterministic. Its clean detached binary was
built with `--locked` from Turnstone `0588d51e987`, resolving Genet
`2647cf29bcf`, Mere `0953dd9522e`, and Netrender `4269ca583cc`; its SHA-256 is
`250199A333ADFF21EB8C1D41A471D959DBB7FDD4FA058BB6969A2498CAE5413D`.
The headed run completed without the prior ICU4X complex-script diagnostic.
Genet's `document-canvas` suite passed all 50 tests with Parley's
`complex-scripts` feature selecting dictionary-backed line and word breaking.

## Dependency defect closed

The 2026-08-21 Livery/Buckram cutover removed the retired `stylo_taffy` and
`genet-layout` package edges. A lockless clean archive now resolves against the
published Mere and Genet `main` branches and passes `cargo check --tests`.
The migration receipt records the exact revisions and focused gates in
[`design_docs/2026-08-21_livery_buckram_dependency_cutover.md`](../../../design_docs/2026-08-21_livery_buckram_dependency_cutover.md).
