# Headed scenario receipts

`run-headed-receipt.py` uses the adjacent `receipt_sink.py` by default. The runner keeps its existing browser options, page geometry, capture handling, timeout, and cleanup behavior. Supply its existing required `--web`, `--out`, `--scenario`, `--query`, and `--node` arguments as before.

An optional `--origin http://127.0.0.1:PORT` selects a separate local page gateway. When the selected sink resolves to the tracked `receipt_sink.py`, the runner passes that exact page origin through `--allowed-origin`. The sink admits JSON POSTs and their preflight only on `/scenario-progress` and `/scenario-receipt`, only from that origin. It does not use wildcard origins or credentials. Without a separate gateway, the allowed origin is the sink's own loopback port.

`--sink PATH` retains support for an explicit external sink. If it resolves to another file, the runner keeps the previous sink command arguments and does not pass `--allowed-origin`. A custom sink serving a separate gateway must already provide compatible receipt handling; this runner does not assume it supports the tracked helper's origin option.

For direct helper use:

```bash
python3 receipt_sink.py --web WEB_DIRECTORY --out RECEIPT_DIRECTORY \
  --port 19940 --allowed-origin http://127.0.0.1:19930
```

The receipt parser, progress file, scenario result, decoded captures, and `scenario.done` format are unchanged. This helper only delivers receipts; successful receipt transport does not itself qualify a scenario, GPU, accessibility, or presentation result.
