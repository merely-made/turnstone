// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Compiler-only timing harness for Graphshell's disclosed grid projection.
//!
//! Run with `cargo run --release -p graphshell --example projection_compile_perf`.
//! Set `PROJECTION_COMPILE_PERF_SAMPLES` to request one through one hundred
//! samples per count. The output is one JSON object per occurrence count.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use graphshell::{
    projection_compile::{
        ProjectionDataset, ProjectionFieldType, ProjectionOccurrence, ProjectionValue, compile,
        refresh,
    },
    projection_editor::{Channel, SourceBinding},
};
use sceno::SourceRef;

fn dataset(count: usize) -> ProjectionDataset {
    let fields = BTreeMap::from([
        ("occurrence_id".into(), ProjectionFieldType::Text),
        ("label".into(), ProjectionFieldType::Text),
        ("x".into(), ProjectionFieldType::Number),
        ("y".into(), ProjectionFieldType::Number),
    ]);
    let occurrences = (0..count)
        .rev()
        .map(|index| {
            let id = format!("occurrence:{index:05}");
            ProjectionOccurrence {
                occurrence_id: id.clone(),
                source: SourceRef::new("benchmark", format!("source:{}", index % 17)),
                values: BTreeMap::from([
                    ("occurrence_id".into(), ProjectionValue::Text(id)),
                    (
                        "label".into(),
                        ProjectionValue::Text(format!("Item {index}")),
                    ),
                    (
                        "x".into(),
                        ProjectionValue::Number((index % 97) as f64 - 48.0),
                    ),
                    (
                        "y".into(),
                        ProjectionValue::Number((index % 89) as f64 - 44.0),
                    ),
                ]),
            }
        })
        .collect();
    ProjectionDataset {
        source: SourceBinding {
            authority: "benchmark.local".into(),
            domain: "benchmark".into(),
            resource: "compiler-perf".into(),
        },
        revision: "compiler-perf-v1".into(),
        fields,
        occurrences,
    }
}

fn samples(default: usize) -> usize {
    std::env::var("PROJECTION_COMPILE_PERF_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(1, 100))
        .unwrap_or(default)
}

fn median(mut values: Vec<Duration>) -> Duration {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    for count in [100, 1_000, 10_000] {
        let mut dataset = dataset(count);
        let mut definition = graphshell::projection_compile::default_definition(&dataset);
        definition.encoding.x = Channel::Field("x".into());
        definition.encoding.y = Channel::Field("y".into());
        let samples = samples(if count == 10_000 { 5 } else { 15 });
        let previous = compile(&definition, &dataset).expect("initial placement");
        dataset.occurrences[0]
            .values
            .insert("label".into(), ProjectionValue::Text("Edited label".into()));
        let timings = (0..samples)
            .map(|_| {
                let started = Instant::now();
                let compiled = compile(&definition, &dataset).expect("benchmark fixture compiles");
                std::hint::black_box(compiled.score.items.len());
                started.elapsed()
            })
            .collect();
        println!(
            "{{\"scope\":\"projection compiler plus scenomise solve; excludes GPU and headed FPS\",\"occurrences\":{count},\"samples\":{samples},\"median_us\":{}}}",
            median(timings).as_micros()
        );
        let refresh_timings = (0..samples)
            .map(|_| {
                let started = Instant::now();
                let compiled =
                    refresh(&previous, &definition, &dataset).expect("validated refresh");
                assert!(compiled.placement_reused);
                std::hint::black_box(compiled.labels.len());
                started.elapsed()
            })
            .collect();
        println!(
            "{{\"scope\":\"validated label refresh with retained placement; excludes GPU\",\"occurrences\":{count},\"samples\":{samples},\"median_us\":{}}}",
            median(refresh_timings).as_micros()
        );
    }
}
