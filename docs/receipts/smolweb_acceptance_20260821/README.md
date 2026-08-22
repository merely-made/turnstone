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
  resolution, Titan composer behavior, and the live Nematic route passed. See
  [focused-tests.done](focused-tests.done).

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

## Dependency defect closed

The 2026-08-21 Livery/Buckram cutover removed the retired `stylo_taffy` and
`genet-layout` package edges. A lockless clean archive now resolves against the
published Mere and Genet `main` branches and passes `cargo check --tests`.
The migration receipt records the exact revisions and focused gates in
[`design_docs/2026-08-21_livery_buckram_dependency_cutover.md`](../../../design_docs/2026-08-21_livery_buckram_dependency_cutover.md).
