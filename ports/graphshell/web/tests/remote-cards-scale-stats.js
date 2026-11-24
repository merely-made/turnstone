// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
// Percentile summaries for the remote cards scale receipt.

export function quantiles(samples) {
  return Object.fromEntries(Object.keys(samples[0]).map(key => {
    const values = samples.map(row => Number(row[key])).sort((a, b) => a - b);
    const at = q => values[Math.ceil(values.length * q) - 1];
    return [key, { p50: at(0.5), p95: at(0.95), max: values.at(-1) }];
  }));
}
