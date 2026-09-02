# Trail recall evaluation

**Status (2026-09-02): in progress.** The local-only evaluation harness is
implemented. The current captured trail is below the admission threshold, so
BM25 remains the only evidence-backed default and the live settings remain an
experiment.

## Boundary

Turnstone owns the browsing corpus, private judgments, settings policy, and
promotion decision. Mere owns the disposable lexical projection and score
fusion. Eidetic remains the authority for browsing records, not the derived
index or this receipt.

The harness is test-only code. It accepts one explicit Turnstone session,
copies that session into a temporary directory, and opens Fjall only on the
copy. It prints aggregate counts, BLAKE3 digests, metrics, and resource costs.
It does not print queries, URLs, titles, or records. The judgment manifest stays
outside the repository.

## E1. Admit an evaluation corpus

A run is admitted only when the corpus has at least twenty distinct documents.
The private manifest must contain disjoint training and held-out targets, with
at least five distinct relevant URLs in each of these four cells:

| Split | Phrase cases | Control cases |
|---|---:|---:|
| Training | 5 | 5 |
| Held out | 5 | 5 |

Queries are unique after the recall provider's lexical splitting and
lowercasing. Phrase cases
must target a projected title containing at least two tokens. A target cannot
appear in both splits. Write each query from remembered intent before consulting
the stored title. This prevents the fixture from rewarding title transcription
instead of recall.

Done when the ignored receipt test accepts a private manifest without weakening
these checks. A smaller corpus produces an `insufficient_corpus` receipt and
does not select a nonzero weight.

## E2. Select on training only

The manifest explicitly supplies candidate maximum token orders, fusion
weights, `ranking_k`, and budgets. Maximum orders are unique values from 1
through 3: `1` means `{1}`, `2` means `{1, 2}`, and `3` means `{1, 2, 3}`.
Weights are unique finite values greater than 0 and at most 4. BM25 is always
the zero-weight baseline.

The selector compares tie-aware unique top-one wins first, then deterministic
top-one wins, Recall@K, and MRR@K. A candidate must preserve training control
top-one performance and fit the declared vector-payload and p95 query budgets.
Ties retain BM25, then prefer the smaller weight and token order. Held-out
results cannot influence this choice.

Done when a unit receipt demonstrates that training chooses an ordered phrase
candidate while the held-out cases are evaluated only afterward.

## E3. Judge the held-out result

A selected nonzero setting is only a promotion candidate when held-out phrase
unique-top-one performance improves, overall unique-top-one and Recall@K are
preserved, control unique-top-one performance is preserved, and both resource
budgets pass. Otherwise the verdict is `keep_bm25`.

The receipt reports exact dense float-vector bytes, serialized vector JSON
bytes, index build time, and p95 query time. Dense bytes are the enforced
in-memory payload budget. Hash-map, string, and allocator overhead are not
claimed as part of that exact count, so the serialized footprint is reported
beside it as a second scale indicator.

Done when a real held-out receipt returns either `promotion_candidate` or
`keep_bm25` with every metric and cost above. Promotion still requires an
explicit settings decision; the evaluator does not rewrite user settings.

## Private manifest shape

This example is abbreviated. A real file needs at least five distinct relevant
URLs in every split/kind cell.

```json
{
  "schema": "turnstone.recall-eval/v1",
  "ranking_k": 5,
  "candidate_orders": [1, 2, 3],
  "candidate_weights": [0.5, 1.0, 1.5, 2.0, 3.0, 4.0],
  "budgets": {
    "max_dense_vector_bytes": 67108864,
    "max_p95_query_us": 10000
  },
  "cases": [
    {
      "split": "train",
      "kind": "phrase",
      "query": "remembered phrase intent",
      "relevant_urls": ["https://private.example/target"]
    },
    {
      "split": "held_out",
      "kind": "control",
      "query": "remembered name",
      "relevant_urls": ["https://private.example/control"]
    }
  ]
}
```

Close Turnstone first so the copy is a coherent snapshot. Then run from a shell
that computes the active session path locally:

```powershell
$root = Join-Path $env:APPDATA 'turnstone'
$sessionId = (Get-Content (Join-Path $root 'current_session')).Trim()
$env:TURNSTONE_RECALL_EVAL_SESSION = Join-Path (Join-Path $root 'sessions') $sessionId
$env:TURNSTONE_RECALL_EVAL_MANIFEST = 'C:\private\turnstone-recall-eval.json'
cargo test trail_memory::evaluation::captured_trail_recall_receipt -- --ignored --nocapture --test-threads=1
```

## Findings

- **2026-09-02:** The copied active-profile corpus at BLAKE3
  `db4d9b34d4acbc57ee6a1accf84a7e03e3b3760e87b2c1076c1d56af11db6da6`
  contains four traces, eleven traversals, seven distinct pages, and one page
  with a current projected title. This is insufficient for disjoint five-case
  phrase and control cells. It is evidence for keeping BM25, not evidence
  against phrase features.
- **2026-09-02:** Live recall overlays graph and recycle-bin titles onto the
  Eidetic trace before minting its derived indexes. The evaluator must use that
  same projection or its ranking corpus differs from the product corpus.
- **2026-09-02:** A deterministic first hit can hide a score tie resolved by
  URL order. The receipt therefore reports deterministic top one and unique top
  one separately.

## Progress

- **2026-09-02:** Added strict private-manifest validation, internal session
  copying, training-only selection, held-out judgment, tie-aware ranking
  metrics, and vector/latency cost reporting.
- **2026-09-02:** Ran an aggregate preflight on a private copy of the active
  profile. Admission stopped at seven documents, before any judgments or weight
  selection.
- **2026-09-02:** The clean remote-pin Turnstone test graph compiled. The
  training-only synthetic partition test passed, all five trail-memory actor
  tests passed, and the ignored active-profile receipt returned
  `insufficient_corpus`.
